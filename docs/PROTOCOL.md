# PROTOCOL.md — the versioned wire protocol

Phase 1 output. Binding spec sections: `SPEC_CS.md` §4, §12, §13, §14, §15, §16,
§17, §20, §27. Binding owner decisions: `DECISIONS.md` D-001 … **D-015**.

**D-013 is the decision that shapes this revision, and it lands in two places.**
Liveness is inherited from **demonstrated participation in the agreed chain**, not
from a seat's status: §3.2 defines `P(k)` and forbids any required emitter set
defined on a status word, and §4.4 instantiates it for `HAND_INIT` (J2). And
`advert_hash` — a per-receiver quantity out of the lobby view — is **removed from
`GENESIS(0)`, from `session_id` and hence from `ctx`**, replaced by §3.1's
`table_params_hash` (J1). §2.9 is this document's D-012 sweep, which is what should
have found the second of those four passes ago.

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
> 4. **Superseded by D-015, and the supersession is recorded rather than the
>    clause rewritten.** This point read: *"Proofs and certificates are still
>    **produced** — they are how a human, or a later version, adjudicates — but
>    no consumer of one moves a chip or removes a player. Whether they should be
>    produced at all in the MVP is an open question the owner has not answered."*
>    That question was `OQ-F`, and **D-015 answers it: they are not produced**
>    (the box below, §12). Points 1–3 are untouched and remain in force — they
>    are about what a *consumer* may do, and a later version reinstating the
>    producer does not reinstate a consumer.
>
> **The accepted cost, stated plainly rather than hidden (`SPEC_CS.md` §18).**
> The rage-quit escape returns, and not only below D-008's floor: at every table
> size and on every abort path, a player losing a big pot can stall, disconnect
> or fault the table and get their chips back. D-005 closed that with forfeiture;
> D-010 reopens it knowingly, because forfeiture was measured over four passes to
> take chips from honest players, and an exploit that harms an honest player is
> worse than one that merely lets a dishonest player escape a loss. `THREAT_MODEL.md` §9.1.2 limitation 4 carries
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

**D-015 is the decision that closes the deadline machinery, and it is stated here
because every later section is read against it.** `OQ-F` asked whether the
machinery should be *produced* at all now that D-010 gives it no effect. The
answer is no.

> **Normative, canonical for this document. `TIMEOUT_VOTE` (`0x0601`),
> `TIMEOUT_CERT` (`0x0602`) and the `EquivocationProof` object of §5.2.4 are
> defined but not produced in version 1.**
>
> 1. **Nothing conforming emits one.** No client of this version signs a
>    `TIMEOUT_VOTE`, assembles a `TIMEOUT_CERT`, or constructs an
>    `EquivocationProof`. There is no legal emission path for any of the three.
> 2. **A receiver rejects one.** A `SignedEvent` whose `event_type` is `0x0601`
>    or `0x0602` is **rejected at §4.0 step 6** — the known-and-legal-on-this-
>    channel gate — before its signature is verified, before its body is
>    decoded, and before any store is touched. A `DISPUTE` whose `n(0) kind` is
>    `2 EQUIVOCATION` is rejected at **step 11**, the first step at which the
>    payload's `kind` exists, and its `n(3) payload` is never decoded as an
>    `EquivocationProof`. In both cases the message is dropped: not buffered,
>    not chained, not retained, not counted into
>    `MAX_DISPUTES_PER_SENDER_PER_HAND`, and not forwarded under §1.5. It is
>    **not** a protocol violation — the sender may be a later version — so no
>    `Fault` is recorded, nothing is attributed, and no `DISPUTE` is raised
>    about it. The disposition is the drop §1.1 gives any message this version
>    does not implement, and it is a *drop*, never a *violation*, precisely
>    because a receiver cannot tell a future version from a modified client.
> 3. **The definitions stay, and so do the code points.** §4.8's two message
>    shapes, §5.2.4's proof object, the `event_class` values `1` and `2` in
>    §2.3's envelope, the `subject` arm of §5.2.1's slot key, and
>    `DISPUTE kind = 2` are all retained exactly as written. Adding the
>    machinery in a later version is then a **capability**, not a wire break
>    (§10.1): the codes are already reserved and already mean what they will
>    mean. **Narrowing any of those definitions is a defect**, and §5.2.1's key
>    in particular keeps both subject axes — five passes went wrong narrowing
>    it, and a key that is right costs nothing while nothing populates it.
> 4. **No state is allocated for them.** §5.3's two subject-class structures are
>    not created; §5.3 records the bound they would have needed and why it is
>    the evidence D-015 rests on.
>
> **What replaces them, so that nothing is left waiting.** A player whose human
> has walked away is still running a cooperating client: it publishes its
> decryption shares automatically and it emits **its own** auto check/fold when
> its own timer expires — a single-writer `ACTION_CHECK` or `ACTION_FOLD` by the
> seat itself, needing no vote, no quorum and no shared clock. A client that
> emits nothing at all stalls one hand to `hand_deadline_ms` (§8.4), which needs
> no certificate from anybody, and is then outside `P(k+1)` under D-013 (§3.2).
> The certificate sat between those two cases and covered neither.
>
> **What it costs is stated in `THREAT_MODEL.md` §9.1.0 and not softened here.**
> The load-bearing one for this document: an equivocation is still **detected**
> — two bodies in one slot, by §5.2.2's predicate, which is unchanged — and is
> still no longer **provable to a third party who was not present**, because the
> object that carried the two bodies to them is not produced.

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
No gap to report on that account.** `docs/research/GUI_STACK.md` research exists but is not
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

For `PROTOCOL_MAJOR = 1` there are **eight** strings, exactly:

```
identify protocol             /p2p-poker/1
GossipSub lobby topic         /p2p-poker/lobby/1        (IdentTopic, not Sha256Topic)
GossipSub lobby slice topic   /p2p-poker/lobby/1/<slice> (S1-EX; see below)
GossipSub lobby chat topic    /p2p-poker/lobby-chat/1   (IdentTopic; see §7.7)
GossipSub search-queue topic  /p2p-poker/search-queue/1 (IdentTopic, and /<slice>
                                                         beside each lobby slice; §7.13)
lobby snapshot RPC            /p2p-poker/lobby-snapshot/1
join RPC                      /p2p-poker/join/1
table event stream            /p2p-poker/table/1
```

**The lobby's slices (`S1-EX`).** A lobby topic may carry a `<slice>`: the
leading bits of the advertised table's own key, written in hexadecimal, four
bits a character. The depths are **0, 4, 8, 12 and 16 bits** and no others, so
the slice is zero to four characters long and depth zero is the lobby topic
itself, unchanged and unversioned.

* A founder **publishes its advert at every depth** -- five publications per
  re-broadcast. Publishing into a topic nobody subscribes to costs nothing, and
  it is what removes any need for two clients to agree on a depth: a client that
  knows nothing of slices subscribes to the lobby topic and hears every table
  exactly as before.
* A receiver takes an advert from whichever slices it subscribes to and computes
  the slice of a table from the table's own key, so a listener and a founder
  never have to be told the same number.
* A client **chooses its depth by what arrives**: every table advertises once
  per `AD_REBROADCAST_MS`, so the rate on a known share of the lobby is the size
  of the whole of it. It goes a depth deeper when that rate is over what it will
  carry, and shallower only when the shallower depth would still be inside it.
  Slices are arbitrary, never derived from the client's own key, and one is
  swapped for another every few minutes so that a client holding four slices of
  a large network still walks across it.
* The peers of a slice find each other under the slice's own DHT provider key
  (`p2p-poker/main-lobby/v1/<slice>`). This is not optional: a mesh is built
  only out of connected peers that share the topic, so without it a sliced lobby
  cannot hold a mesh at all.

**The lobby's hours (`D-070`).** Beside the lobby's DHT provider key
(`p2p-poker/main-lobby/v1`) a client provides the key of the current hour,
`p2p-poker/main-lobby/v1/hour/<n>`, where `n` is the Unix time in seconds divided
by 3600, rounded down and written in decimal, and looks it up while it has no
poker client to talk to. A provider record outlives
its client by two days; an hour's key names nobody who was not here within the
hour, so it is where a newcomer finds a client that is running now
(`NETWORK_STACK.md` §3.2). Nothing on the wire changes and nothing depends on
it: a client that knows nothing of the hours meets everybody under the lobby's
own key, exactly as before.

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
| **Search queue broadcast** | `/p2p-poker/search-queue/1` and its slices (§7.13) | one `SignedEvent` per broadcast message |
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
| `TABLE_CHAT` | table mesh: the table's group, or its topic where there is no group (§7.8) |
| `TABLE_LEAVE` | table mesh: the table's group; before the table is set, its topic too (§7.10) |
| `TABLE_HEARING` | table mesh: the table's group and its topic, before the table is set (§7.11) |
| `TABLE_CONTINUES` | table mesh: the group and the topic of the table that goes on, before it is set (§7.12) |
| `SEARCH_PRESENCE` | search queue broadcast (§7.13) |
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
`LOBBY_SNAPSHOT_RESPONSE`, `LOBBY_CHAT`, `TABLE_CHAT`, `TABLE_LEAVE`, `TABLE_HEARING`, `TABLE_CONTINUES`,
`SEARCH_PRESENCE`, **`DISPUTE`**. Everything else is chained.

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
what is unaudited about it are constructions, and `CRYPTOGRAPHY.md` owns
them** (D-011 rule 1) — §1's summary table, §9's library table and `OQ-6`, which
is where the content actually is; the §3.2 this used to name was never written. The four-reason list and the audit limitation that stood
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
| `p2p-poker v1 table-params` | `table_params_hash` (§3.1's normative box) |
| `p2p-poker v1 connection` | `connection_nonce` (§1.2) |
| `p2p-poker v1 table-id` | reserved; see §4.1 — `table_id` is currently the table public key itself |
| `p2p-poker v1 advert` | reserved; unused in version 1. `advert_hash` is everywhere the `event_hash` of the `LOBBY_TABLE_AD`, never a separate digest. Since **D-013** it is a **lobby-layer pointer only**: it names which advertisement a joiner is answering, and it enters no chained hash — not `GENESIS(0)`, not `session_id`, not `ctx`, not `roster_hash`, not `state_hash`. `table_params_hash` (§3.1) carries what it used to be there for. |
| `p2p-poker v1 timeout-cert` | the certificate subject digest (§8.3) |
| `p2p-poker v1 member-binding` | what a seat's client signs to say which member of the table's carrier group it is: `h(domain, [chat_id, member_key])`, signed with the seat's application key and carried as the member's name in the group (D-051) |

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

### 2.9 The D-012 sweep of this document

D-012 forbids canonical state derived from a per-receiver quantity. This document
was **patched** at the two sites that decision named and never **swept** for the
class it describes, and J1 was the survivor that cost. The sweep is recorded here
so the next editor checks a new construction against a list instead of
re-deriving one, and so the same gap cannot reopen silently.

**Method.** Three enumerations, each exhaustive by construction rather than by
judgement: every hash this document defines (§2.8's register is the index, and a
construction absent from it is a bug), every collective stage's required emitter
set (§4.11's *Stage kind* column is the index), and every field of
`PublicTableState` (§6.1 is the index, and it is the widest hashed struct in the
corpus). Each input is traced back to one of three answers — a protocol constant,
a value fixed by chained content every participant accepted, or a local view. The
third answer is a survivor.

**1. The hash constructions.** Eighteen, and three were survivors, all of them one
defect.

| Construction | Verdict |
|---|---|
| `GENESIS(0)` §3.1 | **was a survivor — J1, fixed in this pass.** `advert_hash` is replaced by `table_params_hash` |
| `GENESIS(k)`, `k >= 1` §3.1 | **was a survivor transitively — J1, fixed**, through `session_id` |
| `session_id` §4.3 | **was a survivor — J1, fixed** |
| `table_params_hash` §3.1 | **new here.** Every part is a field of one signed `LOBBY_TABLE_AD`, and the two fields that varied between re-broadcasts are excluded by name |
| `roster_hash(k)` §3.1 | clean. `seat_flags` was deleted by D-012 and §3.1's two-case argument is what deleted it |
| `ABORT_TERMINAL(k)` §3.1 | clean in itself — it is why an abort's terminal cannot fork — and it inherited J1 through `GENESIS(k)`, which it no longer does |
| `stage_hash`, single-writer §3.2 | clean |
| `stage_hash`, collective §3.2 | clean **conditionally on `R`**, and `R` is enumeration 2, which is where J2 lived |
| the witness-independent terminal stage §3.2 | **no `stage_hash` at all**, so "which copy did I accept" reaches nothing |
| `event_hash` §3.2 | clean; excludes the signature, against Ed25519 nonce malleability |
| `commitment_i` §4.4 | clean in its own inputs; inherited J1 through `session_id` |
| `seed` §4.4 | clean — the combine runs over the **chained committer set**, never over the reveals a peer happened to receive |
| `ctx` §4.5 | clean in its own inputs; inherited J1 through `session_id` and no longer does (§4.5) |
| `index_map_hash` §4.5 | clean — `m` and `button_position` are both `HAND_INIT` fields |
| `input_deck_hash` / `output_deck_hash` / `final_deck_hash` §4.5 | clean — deck bytes |
| `subject_digest` §4.8 | clean — `parent_event_hash` is a `stage_hash` and `deadline_ms` is the parent's chained `next_deadline_ms`, not a clock read |
| `state_hash` §6.1 | clean after the `absent` deletion — enumeration 3 |
| `password_proof` §4.3 | clean, and it enters no chain |

**2. The collective emitter sets.** Eleven rows and twelve sets: nine rows and ten
sets after checkpoint 8 split `STATE_HASH`'s row across `P(k-1)`, `P(0)` and
`P(k)` (§4.9), plus the two this pass added — the reconciliation round's
`R(c) ∪ W` and `HAND_INIT`'s `P(m) ∪ A`. This is where J2 lived, and `N1` lived
one row below it, in the set nobody had asked whether it could have one member. The finding
is not that one set was wrong; it is that **the class had never been enumerated**.
§3.2 defined the stage kind and its `R`, §4 instantiated each one, and no document
in seven passes asked what removes a seat from an `R`.

| Stage | `R` before this pass | `R` now |
|---|---|---|
| `TABLE_READY` | the `PLAYER_LIST` roster | unchanged; its signers **are** `P(0)` |
| `RNG_COMMIT`, `RNG_REVEAL` | "every seated participant" | **changed** — `P(0)`, the `TABLE_READY` signers |
| `HAND_INIT` | dealt-in seats **plus every occupied seat that is absent or sitting out** | **changed — this was J2** — `P(k-1)` |
| `HAND_COMPLETE` | "the same set that emitted `HAND_INIT`" | unchanged in form, and participation-derived now that `HAND_INIT`'s is |
| `DECK_INIT`, `DECK_COMMIT`, `DEAL_PRIVATE`, `BOARD_REVEAL` | `dealt_in` | unchanged in form; **`dealt_in` is a subset of `P(k-1)`, now normative** (§4.4). Without that containment the stall moves from `HAND_INIT` to `DECK_INIT` and nothing else changes |
| `SHOWDOWN_REVEAL` / `SHOWDOWN_MUCK` | the required-to-show set | unchanged — a function of the betting sequence and the rules, every input chained |
| `TIMEOUT_CERT` | `V(subject)`, inductive over completed certificates | unchanged — clean, and clean because D-008 already fixed the version that was not |
| `STATE_HASH`, `STATE_ACK` | "all present seats" | **changed** — `P(k-1)`, `P(0)` at checkpoint 1, and **`P(k)` at checkpoint 8** (§4.9). Checkpoint 8's is the one `R` in the document that is not `P(k-1)` or a subset of it, and it is the one stage whose *accepted* set is wider than its *required* set: an out-of-set `STATE_HASH` there is compared rather than rejected, which is `L3`'s disposition and is what stops two peers whose `P` has forked from never colliding |
| the **reconciliation round** of §6.3 step 3 | the re-derived checkpoint's own set | **changed in this pass — `N1`** — `R(c) ∪ W`, where `W` is this receiver's contradiction set. **The one required emitter set in this document with a per-receiver component**, and the paragraph below is why it is admissible; it is also the one with a **floor**, `\|R\| >= 2`, which is the point of it |
| `HAND_INIT`, when §4.9's readmission set is non-empty | — | **unchanged, and that is `P2`'s disposition** — `R` stays `P(m)`. The set `A` — the senders of a stale `PLAYER_SIT_IN`, or of a stale checkpoint-8 `STATE_HASH` that agreed — widens this stage's **accepted** emitter set to `P(m) ∪ A` and never its required one. `N5` put the union in `R`, where a replayed agreeing copy re-enlarged it once per hand for 4 096 hands and stalled stage 0 each time; a required set is now enlarged only by a seat's own accepted copy of a **chain-`m+1`** event, which no replay can forge (§4.9, §4.4) |
| `HAND_ABORT` | none — witness-independent terminal | unchanged |

**Four sets changed on the status-word finding, and every one of them was defined
on a status word.** The words were *absent*, *sitting out*, *seated* and
*present*. Not one of them is a quantity a silent seat can ever change, which is
the whole of J2 and the reason §3.2 now carries the rule rather than each message
type carrying its own habit.

**Two more sets changed in this pass, on a different finding, and one of them is
per-receiver — recorded here rather than left for the sweep to miss.** D-012
forbids **canonical state** derived from a per-receiver quantity. `W` derives no
state: it enlarges a required emitter set and nothing else, so it can only make a
stage complete **later** at this peer, never sooner and never differently, and it
cannot reach another peer's set at all. The value the stage carries is an ordinary
chained collective body read identically everywhere. The same test passes `A`,
which also only enlarges, and whose enlargement lands at the one place —
`HAND_INIT`'s collective byte-identical body — where a disagreement about it
**stalls stage 0** loudly instead of forking anything (§4.10, §4.9). **The rule
this pass adds to the sweep's method, because it is what the two new rows have in
common:** a per-receiver quantity that can only *enlarge* a required emitter set
is admissible, and one that can *shrink* or *replace* one is not — the second is
J2's shape and the first cannot reach canonical state without passing through a
stage every peer recomputes.

**3. `PublicTableState`.** Every field of **§6.1's table**, which is this
enumeration's index and is the **one normative statement of the struct's field
order** in this document; the list that stood here is deleted rather than
corrected (`P7-w`). It listed the same fields in a different order —
`signed_this_hand` before the `Vec<bool>` flag vectors instead of after them — and
`PublicTableState` is `#[cbor(array)]`, so **field order is the encoding**: two
transcriptions from the two lists are two `state_hash` values for one state, and
every checkpoint in the corpus then reports a divergence between two peers who
agree about the game. A sweep is a list of verdicts and has no business being a
second wire definition, so it carries none: **this paragraph enumerates verdicts
and never order**, `STATE_MACHINE.md` §3.3 adopted §6.1's table for the engine and
adds no third listing, and a fourth listing anywhere is a defect on sight
(D-011 rule 2).

All clean, and four are worth naming. `level`, `small_blind` and `big_blind` are
**derived from `hand_id` in closed form** rather than incremented, which is the
single most common place a poker protocol admits a per-receiver quantity and which
this document closed before the rule existed. `transcript_head` is the `stage_hash`
the event carrying the hash chains from (§6.1's row says which that is at the
boundary), and every checkpoint is itself a collective stage, so every emitter has
completed the same prefix. And the flag vector **was five and is
now four**: `absent` is deleted (§6.1, J5). `signed_this_hand` is the fourth worth
naming and is **new here (L2)**: it is a `Vec<bool>` by seat like the four flags
and is emphatically not a fifth one — it is `P(k)` (§3.2), whose per-receiver
residual is `Q-10`'s and is named rather than removed, exactly as the correction
below records. It is in the struct **because** that residual exists: hashing it is
what makes two peers who hold different `P` collide at checkpoint 8 (§4.9) instead
of forking in silence.

**One thing this sweep asserted and got wrong, corrected here rather than left
standing (K1).** It read: *"No 'who is connected', 'who was heard', or
delivery-order quantity enters one."* **`P(k)` does**, on exactly one path, and
this sweep missed it because it audited the *sets* and not the *narrowing*.
`P(k)` is `stage_hash` membership wherever the stage completed, and wherever the
stage stalled it is strictly *who was heard* — `Q-10`, named in §3.2 and adopted
as a default there. §3.2 proves that `P` narrows at a stalled stage 0 and nowhere
else, so the per-receiver input is present on **every** path where `P` does
anything at all. The honest form of the sentence is: *no per-receiver quantity
enters any construction in this document, and the one that enters a required
emitter set is `Q-10`'s, is named, and is contained by the solitary-stage rule of
§3.2 rather than removed.* A sweep that enumerates sets and not the transitions
between them will miss this class again, which is the reusable finding.

**What the sweep looked for and did not find, recorded so it is not re-filed.** No
wall-clock read enters any construction — §2.6 is the rule and §8.2 is the one place
a timer is legitimately read, locally, feeding nobody's canonical state. One
candidate was chased and cleared:
`TIMEOUT_CERT`'s `n(1) votes` embeds complete `SignedEvent`s **with signatures**,
and §3.2 concedes that a malicious signer can produce a second valid signature over
one body, so two honest certificate emitters could embed different bytes for one
vote. That does not fork the stage, because the certificate stage is collective and
`stage_hash` is taken over the **whole** set of certificates rather than over one
chosen copy — a second property the collective-stage rule buys that §3.2 does not
claim for it.

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
                 ZERO32, table_params_hash, ZERO32 ])

GENESIS(k) = h("p2p-poker v1 genesis",                              for k >= 1
               [ u16_be(protocol_version), table_id, u64_be(k),
                 session_id, roster_hash(k), TERMINAL(k-1),
                 R(k) as u8 seat indices, ascending, without repeats ])

roster_hash(k) = h("p2p-poker v1 roster",
                   [ for each seat s in ascending seat index:
                       u8(s) || app_public_key[s] || u64_be(stack_at_hand_start[s]) ])
```

**`R(k)` is in the genesis, and this closes a divergence that was measured
rather than argued.** `roster_hash(k)` is the seating and the stacks; `R(k)` is
who the hand requires. Two peers can derive the same seating from different
participation — one had certified a seat absent and dropped it, the other had
not — and before this term they then opened hand after hand at **identical**
genesis hashes with different players in them. Nothing refused anything, because
every check downstream compares against the genesis and the genesis could not
tell the two hands apart. Three clients, one killed, five hands of `[0, 1, 2]`
against `[1, 2]`. With `R(k)` inside, two peers that disagree about who is
playing cannot open the same hand at all, which turns a silent split into a
refusal at the first `HAND_INIT`. `R(1)` is the signers of `TABLE_READY` (§3.2).

**`stack_at_hand_start[s]` at `k = 0` is the ratified roster's `SeatEntry.buyin`,
and this sentence closes `U1`.** The field was defined only as `TERMINAL(k-1)`'s
`n(5) final_stacks`, and for the first hand there is no `TERMINAL(-1)`;
`TERMINAL(0)` is a `stage_hash` and carries no fields at all. So the construction
that every hand's `GENESIS(k)` depends on had, for `k = 0`, **no defined value** —
and `roster_hash(0)` feeds `session_id`, which feeds every subsequent
`GENESIS(k)`. Two implementers guessing differently would diverge on every event
of every hand, silently and with nothing to attribute it to. That is the
`role_code` class of defect with a larger blast radius: a term used normatively
and defined nowhere, in a construction that *reads* complete.

`SeatEntry.buyin` is the only candidate that exists, and it is the right one for
three reasons rather than by elimination. It is **signed** — it travels in
`PLAYER_LIST n(0)`, under the table key, and again in every `TABLE_READY` that
ratifies that roster, so every seat hashes a value it has verified rather than
one it inferred. It is **agreed** — `TABLE_READY` is a collective stage over the
same roster, so a seat that read a different buy-in cannot ratify. And in a
tournament it is **forced**: §7.2 pins `min_buyin == max_buyin == start_stack`, so
there is exactly one admissible value and no room for a founder to vary it per
seat.

`SeatEntry.buyin` must therefore lie within the advert's `[min_buyin, max_buyin]`,
and must equal `start_stack` in a tournament mode — the check belongs with the
other `JOIN_ACCEPT` and `PLAYER_LIST` validations, and closes `U8` in the same
breath, because an unconstrained buy-in in the roster is an unconstrained
starting stack in `roster_hash(0)`.

**The two forms have the same six parts in the same slots, and slot 4 of
`GENESIS(0)` is `ZERO32` (K5).** It carried `table_public_key`, and §4.1 is
normative that `table_id` **is** `table_public_key` — the same 32 bytes — so
slot 2 and slot 4 were one value hashed twice. The duplicate is removed. It is
replaced by the sentinel rather than deleted outright so that both forms keep six
parts under one domain string: `h` (§2.8) carries no arity, so two part-lists of
different lengths under one domain are two preimages a future editor would have
to reason about, and there is no reason to create that obligation for a value
that binds nothing. `ZERO32` in slot 4 reads exactly as `ZERO32` in slot 6
already does — *there is no session yet*, beside *there is no previous terminal*
— and `session_id` cannot appear there, because §4.3 derives it from the
`TABLE_READY` events of the chain `GENESIS(0)` opens.

The reason this section gives below for excluding `protocol_version` and
`table_id` from `table_params_hash` — *"`GENESIS(0)` already carries both as
separate parts"* — is unaffected and is now the only claim resting on that part
list: slots 1 and 2 carry them, once each.

**The roster is the ordered list of seated identities and their start-of-hand
stacks, and nothing else. This is D-012 and it is normative.** A
`u8(seat_flags[s])` component stood in this hash and is **deleted**. It appeared
exactly once in the whole corpus — here — and was never defined, so two
implementers had to guess it; but it is deleted rather than defined, and the
reason is general and binds every future addition.

`roster_hash(k)` feeds `GENESIS(k)`, and every event of hand `k` chains
transitively to `GENESIS(k)`. A component two honest receivers can compute
differently therefore does not cost them one field: it costs them every event of
the hand, because neither will verify a single one of the other's. That failure
is total, and it is silent until the first event arrives.

Anything mutable about a seat is one of exactly two things, and neither belongs
here:

* It is **established by a chained event every participant accepted** — sitting
  out by `PLAYER_SIT_OUT`, a stack by the previous hand's terminal, a departure
  by `PLAYER_LEAVE`. Then it is already in the chain, signed and accepted by
  every participant, and hashing it into the genesis a second time adds no
  binding the chain does not already carry.
* It is a **local view** — what this peer last heard from that seat, whether this
  peer believes it is away, how long ago it answered. Then it can differ between
  honest receivers, and hashing it forks the genesis.

There is no third case. **No per-seat status, flag, presence bit or liveness
estimate may enter `roster_hash`**, and a document or an implementation that adds
one has a bug. Seat status is `STATE_MACHINE.md`'s (D-011 rule 1) and it changes
only through a chained event; this hash is not a second route to it.

The three surviving components are each a function of accepted chained content.

**The seat vector is frozen for the whole session, and this is the disposition of
K2's genesis half.** The clause that stood here read *"ratified unanimously at
`TABLE_READY` and change only at a hand boundary through an accepted
`PLAYER_LEAVE` or seat entry"*, and its second half was both **unreachable** and
**unsafe**.

> **Normative: the seat index and `app_public_key[s]` components of
> `roster_hash(k)` are fixed at `TABLE_READY` and never change again. No seat is
> added to the vector, removed from it, or reordered within it for the life of the
> table. `roster_hash(k)` therefore varies only in `stack_at_hand_start[s]`, which
> is `TERMINAL(k-1)`'s `n(5) final_stacks` and is settled by that terminal — so
> `GENESIS(k)` is a function of `TERMINAL(k-1)` and of session constants, and of
> nothing that happens after `TERMINAL(k-1)`.**

*Unreachable*, because §4.3 already says so in terms: *"The set of `n`
`TABLE_READY` events is unanimous ratification of the roster by every
participant, so from here on nobody can be added, removed or reordered without a
new table."* There is no seat-entry message after formation in version 1 — §4.3
is formation only, and §4.1 ends the table key's authority at `TABLE_READY` — so
half of the deleted clause named a route that does not exist, which is the defect
class J4 was filed for.

*Unsafe*, because the other half named a route that does: `PLAYER_LEAVE` is
accepted in the hand boundary window (§4.10), which sits **after** `TERMINAL(k)`
and has no required emitter set, so no stage completion proves a receiver has
seen all of it. Letting it reach `roster_hash(k+1)` would derive `GENESIS(k+1)`
from *which boundary events this receiver happened to hear* — a per-receiver
quantity in the position D-012 forbids above all others. The consequence is J1's,
total and silent: two peers that differ by one `PLAYER_LEAVE` derive different
`GENESIS(k+1)`, so **no event of hand `k+1` verifies at either**, and — because
`ABORT_TERMINAL(k+1)` is a function of `GENESIS(k+1)` — their `GENESIS(k+2)`
differs too, and every hand after it. That fork is permanent for the *terminal*
half and no rule in this document can end it; the *roster* half of a fork made
by a certificate one receiver never had in time is the one case §4.9's late
roster repair closes.

**What happens to a departing seat instead.** Its seat stays in the vector with
its `app_public_key` and takes no further part: it is outside `P` — §3.2 excludes
`PLAYER_LEAVE` from participation, for the reason given there — hence outside
every `R` and outside `dealt_in`, and it occupies a row in a hash and nothing
else. Its **chips** leave through the field that already carries them,
`HAND_INIT`'s `n(11) ledger_delta` (§4.4), which is inside a collective
byte-identical body that every receiver recomputes. That is the property this
disposition buys: two peers that heard different boundary windows now **reject
each other's `HAND_INIT` copy and stall stage 0** — loud, at a stage, and
disposed of by §8 — instead of deriving two genesis values and never verifying
another event of each other's for the life of the table. **That path is exactly
the one §3.2's solitary-stage rule covers, and it is a precondition of this
disposition rather than a separate concern**; the dependency is stated in §4.10's
boundary-window box.

**What is not settled here, and is not invented here.** *When* the departing
stack is subtracted — which terminal's `final_stacks` first shows the seat at
zero, and therefore which `HAND_INIT`'s `ledger_delta` carries it — is a
question about `n(5) final_stacks` and the ledger identity, not about the
genesis, and it is untouched by this box. It is recorded in `DECISIONS.md`'s open
list rather than answered, because answering it wrongly is how a redundant hash
becomes a fork: `roster_hash(k)`'s stack vector is `TERMINAL(k-1)`'s
`final_stacks` and nothing else, whatever the answer turns out to be.

`stack_at_hand_start[s]` is `TERMINAL(k-1)`'s `n(5) final_stacks`, which is
chained and agreed. By the bullets above the stack
is redundant — it *could* be dropped on the same argument — and it is retained
deliberately, as a check rather than as a binding: §4.10's receiver validation of
`n(5) final_stacks` is written against it, and a redundant hash of a quantity
every peer already agrees on cannot fork anything.

**`table_params_hash` — normative, and this is the disposition of J1 (D-013).**
`advert_hash` stood in `GENESIS(0)`, in `session_id`, and transitively in `ctx`. It
is **removed from all three** and this box replaces it. Other documents reference
this box by number and reproduce no part of it (D-011 rule 2).

> ```
> table_params_hash = h("p2p-poker v1 table-params", [
>     u16_be(game),                    //  LOBBY_TABLE_AD n(0)
>     u16_be(mode),                    //                 n(1)
>     preset_id,                       //                 n(2)   payload bytes, verbatim
>     u64_be(small_blind),             //                 n(4)
>     u64_be(big_blind),               //                 n(5)
>     u64_be(ante),                    //                 n(6)
>     u64_be(min_buyin),               //                 n(7)
>     u64_be(max_buyin),               //                 n(8)
>     u64_be(start_stack),             //                 n(9)
>     u8(max_players),                 //                 n(11)
>     u8(min_players_to_start),        //                 n(12)
>     u16_be(blind_schedule.mode),             //         n(13) BlindSchedule n(0)
>     u16_be(blind_schedule.every_n_hands),    //                            n(1)
>     u64_be(blind_schedule.first_small_blind),//                            n(2)
>     u64_be(blind_schedule.small_blind_cap),  //                            n(3)
>     u32_be(action_timeout_ms),       //                 n(14)
>     u32_be(action_grace_ms),         //                 n(15)
>     u32_be(crypto_step_timeout_ms),  //                 n(16)
>     u32_be(hand_deadline_ms),        //                 n(17)
>     u32_be(join_deadline_ms),        //                 n(18)
>     u32_be(hand_delay_ms),           //                 n(19)
>     u16_be(button_rule),             //                 n(20)
>     u16_be(odd_chip_rule),           //                 n(21)
>     u16_be(showdown_policy),         //                 n(22)
>     deck_suite                       //                 n(24)  payload bytes, verbatim
> ])
> ```
>
> **Twenty-five parts, in exactly that order**, under §2.8's constructor — each part
> length-prefixed, `preset_id` and `deck_suite` as their raw payload bytes and every
> other part in the fixed-width big-endian form shown. The order is part of the
> protocol: it is §7.2's ascending `LOBBY_TABLE_AD` field order, with `BlindSchedule`
> expanded in place at its own field's position.
>
> **Nothing else may enter it**, and each exclusion is named because a future editor
> will be tempted by one of them. `n(3) table_name` and `n(10) players` are display
> and advisory. `n(23) password_required` is an admission gate, spent at
> `JOIN_REQUEST` and irrelevant once a seat is held. `n(25) founder_app_key` and
> `n(26) founder_peer_id` are identity and routing, and the founder's special
> position ends at `TABLE_READY`. `n(27) timestamp_unix_ms` and
> `n(28) expires_at_unix_ms` are **the two fields that made `advert_hash`
> per-receiver in the first place**, since §7.2 rule 6 obliges every re-broadcast to
> carry a strictly greater timestamp. `protocol_version` and `table_id` are excluded
> because `GENESIS(0)` already carries both as separate parts and a second copy
> binds nothing.

Every joiner derives the **identical** `table_params_hash` from whichever
re-broadcast reached it, because the parameters are what it agreed to and the
timestamp is not. That restores, this time truly, the property this section used to
claim for `advert_hash`: **every participant provably sat down to the same game**,
which is what §4 of the spec means by "so that everyone agrees on them before the
first hand is dealt". The old claim was false as written — the advertisement each
participant joined under is exactly what differed between them — and the failure it
hid was total and silent: two peers thirty seconds apart derived different
`GENESIS(0)`, so neither verified a single one of the other's `TABLE_READY` copies,
the collective stage never completed, and formation was abandoned at
`join_deadline_ms` with no participant able to see why.

**J1(b) is closed by the same value, at three places rather than one.** Nothing
forbade the founder re-signing with a *different* `small_blind` or `max_players`
and handing two joiners two rule sets — which forks `HAND_INIT`'s `n(4) level`,
`n(5) small_blind` and `n(6) big_blind`, a collective stage whose bodies must be
byte-identical. Now: §7.2's receiver rule 7 discards a re-broadcast whose parameters
changed; §4.3's `PLAYER_LIST` carries `table_params_hash` and every receiver checks
it against its own; and §4.3's `TABLE_READY` carries it and every receiver checks
that every other seat's copy equals its own. The fork is refused before the first
chained event depends on it, rather than surfacing as a stage that never
completes.

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

**Normative — what may define `R`. This is the disposition of J2 (D-013), it is
canonical for the corpus, and every `R` in §4 is an instance of it.** A collective
stage's required emitter set is the table's liveness gate: the stage does not
advance until every member has been heard, so a member that cannot be heard stops
the table.

> **`R` is derived from demonstrated participation in the agreed chain, never from
> a seat's status.**
>
> **A seat is required to emit in hand `k+1` only if it signed at least one chained
> event during hand `k`. For the first hand, the required set is the signers of
> `TABLE_READY`.**
>
> Write `P(k)` for that set: the seats that signed at least one `chain_scope = 1`
> event, accepted by this receiver, **in chain `k`** — every stage of chain `k`,
> plus chain `k`'s hand boundary window, whose chain, `hand_id`, `sequence`,
> parent and total order §4.10 defines (this is the closure of `Q-09`), plus
> chain `k`'s **boundary checkpoint**, whose same four quantities §4.9 defines.
> `P(0)` is
> the set of seats that signed `TABLE_READY`. Every `R` in §4 is `P(k-1)`, or a
> subset of it fixed by chained content, **with exactly three exceptions, named
> here so the reader does not have to find them, and each is an enlargement rather
> than a substitution:**
>
> 1. **the boundary checkpoint of §4.9, whose `R` is `P(k)`** because it is the
>    stage that publishes what `P(k)` is — a stage that compares the participation
>    set cannot be required of the set it replaces. Safe for the reason §4.9's box
>    gives: every member of `P(k)` demonstrated itself by signing during hand `k`,
>    and the exit if one stops afterwards is a timer and never a peer;
> 2. **a reconciliation round of §6.3 step 3, whose `R` is `R(c) ∪ W`** — the
>    re-derived checkpoint's set together with this receiver's contradiction set.
>    It is the one required emitter set in this document with a **floor**, and the
>    floor is why it exists: a stage whose purpose is to detect a fork may not have
>    a set the forked peer can satisfy alone (§4.9, `N1`);
> **There is no third, and there was one until this pass.** `N5` made
> `HAND_INIT(m+1)`'s `R` be `P(m) ∪ A`, with `A` §4.9's readmission set. `P2`
> deletes that enlargement: `A` widens that stage's **accepted** emitter set and
> never its required one, so the list of exceptions to *"`R` is `P(k-1)` or a
> subset of it"* is two long and not three. The reason is in §4.9 and reduces to
> one sentence — **`A` is written from a stale event, §4.0 step 10a is skipped for
> stale events, so a replay writes it again, and a required emitter set that grows
> from a replayable input is a stall an attacker can renew once per hand.**
>
> **Both remaining exceptions only ever add seats**, which is why neither weakens
> the liveness gate this box is about: a larger `R` waits for more, never for
> less, and a disagreement about who is in it stalls stage 0 loudly rather than
> completing a stage quietly. **Both are also written from an event bound to the
> chain whose set they enlarge** — a chain-`k` signature for the boundary
> checkpoint, this receiver's own accepted contradictions for a reconciliation
> round — which is the property `A` did not have and the one a future editor must
> re-check before adding a fourth.
>
> **A peer's own emission counts into its own `P`, and this is stated because it
> was not (K1).** "Accepted by this receiver" includes the events this receiver
> itself signed and emitted: a peer accepts its own emissions into its own chain —
> it must, since it chains from them and computes `stage_hash` over them — and the
> alternative reading is a reductio. Under it a peer would never be in its own
> `P(k)`, hence never in any `R` of hand `k+1`, hence not a permitted emitter of
> the `HAND_INIT` it is required to emit; and the base case would fail outright,
> since `P(0)` is *the signers of `TABLE_READY`* and every peer signed one. It
> counts. `STATE_MACHINE.md` maintains the same set as `signed_this_hand` and
> takes this reading with it.
>
> **Two events are excluded, and neither exclusion is optional.**
>
> **(1) A terminal `HAND_ABORT` never counts as participation.** It enters no
> `stage_hash`, its stage is witness-independent, and two honest peers routinely
> accept copies signed by **different seats** (the terminal shape defined below,
> §4.10) — so counting its signer would derive `P(k)` from *which copy this
> receiver happened to accept first*, which is the exact per-receiver quantity P3
> built the terminal stage to refuse. `STATE_MACHINE.md` I31(c) asserts this from
> the engine side.
>
> **(2) A `PLAYER_LEAVE` never counts as participation.** It is the one chained
> event whose content is a statement that its signer will emit nothing further,
> and `P` is a *required emitter* set. Counting it would make every voluntary
> departure a required emitter of the next hand, which nobody can satisfy: the
> stage stalls, hand `k+1` runs to `hand_deadline_ms` and aborts, and the seat
> leaves `P` only on the hand after the one it announced its departure in. That is
> a stall bought for nothing, on the one path where the seat told us in advance.
> The exclusion costs nothing, because a departing seat needs no vote and is
> removed from no hash (§3.1).
>
> `PLAYER_SIT_OUT` is **not** excluded and must not be: a seat that sits itself out
> is alive, keeps posting dead money (D-005), and can and must sign the next
> hand's `HAND_INIT`. It is outside `dealt_in`, which is `STATE_MACHINE.md`'s
> derivation and a strict subset of `P` (§4.4) — a different question from this
> one.

**Where `P(k)` comes from, in one line per case, because the two cases have
different strength.** For every stage of chain `k` that **completed**, membership
is exactly `stage_hash` membership (the collective form enumerates `R`; the
single-writer form names `writer_seat`), so two peers holding the same prefix agree
by construction. One useful consequence follows immediately and is worth stating,
because it removes most of the surface: **if `HAND_INIT(k)` completed, then `P(k)`
contains the whole of `P(k-1)` at every peer and can narrow nowhere.** Every member
of `R = P(k-1)` signed stage 0; every later `R` is a subset of `P(k-1)`, so no seat
outside it can sign a stage event at all; and no peer holds a later stage without
holding stage 0's `stage_hash`. The only way `P(k)` then differs from `P(k-1)` is
by *growing* — a seat re-entering with a hand-boundary `PLAYER_SIT_IN` (§4.10),
which is a chained single-writer event every peer accepts or rejects on the same
grounds, so it is not a per-receiver difference either. **The set can therefore
narrow at one stage only, stage 0**, which is exactly the case it exists for.

**The stalled stage is the residual, it is named rather than smoothed over, and it
is `Q-10`.** The stage that stalled has no `stage_hash`, so "seat `s` contributed
there" is strictly *who was heard*. It cannot simply be excluded: a hand that stalls
at `HAND_INIT` completes no stage at all, so excluding it would empty the next
hand's set and stop the table instead of skipping one seat. **Named default,
adopted: a contribution to the stalled stage counts.**

**The residual is real and it is not bounded to one hand. The claim that it was
is deleted (K1).** What stood here read that two peers who disagree *"derive
different `n(8) dealt_in`, each rejects the other's `HAND_INIT` copy, the stage
does not complete, and the hand aborts at the deadline; that is a hand lost and
recoverable"*. Every clause of that is true and the conclusion does not follow,
because the analysis stops after one iteration. Run it twice: each peer's `P`
narrows again at the hand that just aborted, and it narrows to `{self}` — after
which every collective stage has one member, completes alone, and the two peers
never need each other again. One lost hand is the first step of a permanent
silent fork, not the whole cost.

What is true, and is all that may be claimed, is that the residual **cannot fork
the chain**: `GENESIS(k+1)` is a function of `roster_hash(k+1)` and
`ABORT_TERMINAL(k)` (§3.1) and reads no participation at all, so both peers hold
the same genesis and every event each emits still verifies at the other. That is
what the solitary-stage rule below turns into a detection: the events keep
arriving, and a peer that has narrowed to `{self}` must stop rather than ignore
them. `Q-10` stays open — nothing here ratifies the stalled stage — and it is
the same question as `STATE_MACHINE.md` Q8, which correctly filed it here.

**One name, two halves.** `STATE_MACHINE.md` maintains this set as
`signed_this_hand` and gates `dealt_in` on it (its §5.3 step 4, I31). That is the
**engine** half; this is the **wire** half — the required emitter set of a
collective stage — and the two are the same set under two names, one per owner
(D-011 rule 1). Neither document restates the other's derivation.

**`P(k)` is agreed wherever the stage that determined it completed, and is
`Q-10`'s residual wherever it did not — which is every hand in which `P` narrows.
This paragraph replaces a false one, and the falsehood is the whole of K1.** What
stood here read:

> *"`P(k)` is not a per-receiver quantity … the terminal stage of a chain is
> witness-independent, so two honest peers that reach `TERMINAL(k)` have accepted
> the same chain-`k` prefix and derive the same `P(k)`."*

**The clause after the semicolon asserts the converse of the property it cites,
and the converse is false.** `ABORT_TERMINAL(k)` is a function of `GENESIS(k)`
and of nothing else **precisely so that peers holding different prefixes reach
it** — §3.1 says so in terms, and that is what unfroze eleven phases. Reaching
the same terminal on the abort path is therefore evidence of nothing whatever
about the prefix. This section proves above that `P` narrows at stage 0 and
nowhere else, and a hand whose stage 0 stalled is a hand that aborts.
So the honest statement is the one that heads this paragraph: **`P(k)` is agreed
exactly where it is inert, and per-receiver exactly where it acts.**

**There is no construction that removes the residual, and pretending otherwise is
what produced K1.** Agreeing who contributed to the stalled stage requires a
collective step at exactly the point where collectivity failed; ratifying it in
the abort body makes `P(k)` a function of which abort copy this receiver accepted,
which is the same per-receiver quantity by another name and is inadmissible under
D-012; and refusing to narrow `P` at all restores the fixed point D-013 exists to
escape. The residual is real, it is `Q-10`, and what this document owes is not a
proof that it cannot happen but a rule that it cannot be **silent**. That rule is
the solitary-stage rule below.

**The solitary-stage rule — normative, and this is the disposition of K1.**

> A peer **was in the solitary regime for hand `k`** when the required emitter set
> of a collective stage of hand `k` was `{self}` and contained no other seat.
> Hand `k` has exactly two such sets, so the test is the disjunction and is stated
> as one so that it cannot be read as either half alone:
> **`P(k-1) == {self} ∨ P(k) == {self}`** — the first is hand `k`'s stage 0 and
> every collective stage of it but one, the second is the boundary checkpoint
> (§4.9), and **each disjunct is what one of the two freeze routes needs.**
> **The test is a property of the hand the event names, not of the phase the
> receiver is in when the event arrives**, and it is answered from that hand's
> retained record (§5.3) for as long as the record is kept. Dealing such a hand is
> permitted: a table whose other seats are genuinely gone must drain them, or a
> tournament never ends and D-013 buys nothing.
>
> **A chained event of hand `k` from a seat outside `P(k-1)`, arriving at a peer
> that was in the solitary regime for hand `k`, is not a rejection. It is a state
> divergence, and the receiver enters §6.3 at step 1: it freezes, completes no
> stage, awards no pot, and evaluates no end condition.** It reaches that
> disposition through §4.0 step 12a, by way of step 12 while hand `k` is live and
> by way of **step 10b** once hand `k` is finished. **Two cases are exempt** and
> are handled ordinarily: `0x0804 PLAYER_SIT_IN` (§4.10), the message a seat
> outside `P` exists to be able to send, and a **checkpoint-8 `STATE_HASH`**
> (§4.9), which is compared rather than frozen on — and which is **this same
> divergence, reaching §4.0 step 12a with this same disposition**, whenever the
> comparison disagrees on a hand this peer derived alone. The exemption is from
> freezing on arrival and never from freezing on disagreement (§4.9, §6.3 step 1).

**Why the test is a disjunction and neither disjunct may be dropped — written out
because dropping one of them was tried in this pass and it deleted the freeze on
K1's own walk.** The two freeze routes ask different questions of different sets:

* the **membership** route — a non-exempt chained event of hand `k` from a seat
  outside `P(k-1)` — is about the hand this peer **derived**, so its disjunct is
  `P(k-1) == {self}`;
* the **comparison** route — a checkpoint-8 `STATE_HASH` of hand `k` that differs
  from this peer's own — is about the boundary this peer is **about to deal from**,
  and checkpoint 8 is required of `P(k)` (§4.9), so its disjunct is
  `P(k) == {self}`.

They are not the same hand's worth of information, and K1's trace lands on the
second one first: the hand `P` narrows in is a hand whose stage 0 was required of
**everybody**, and it is the hand that stalls and aborts. Its `P(k-1)` has three
seats and its `P(k)` has one — so a rule holding only the first disjunct calls
that hand *not solitary*, throws away the checkpoint-8 mismatch that arrives one
network delay later, and the freeze fires only on the next hand, if a
non-exempt event happens to arrive at all. **The `P(k-1)` half alone is the
version that misses the earliest and best-evidenced detection this document
has.** Both disjuncts, one bool in §5.3's record, no per-hand set beyond the one
step 10b's membership test already needs.

**And the membership test's set is `P(k-1)`, which is `N7`.** §4.0 step 10b tested
*"outside the recorded `P`"* against a record documented as `P(hand_id)`; §3.2's
rule here and §4.0 step 12 both say `P(k-1)`, and `P(k-1)` is right — it is what
hand `k`'s stages were **required of**, so a seat outside it is a seat this
receiver did not expect to hear from in hand `k`, which is the whole content of
the contradiction. `P(k)` is a different set, grown on purpose by the two exempt
events, and testing against it would silence the rule for exactly the seats those
exemptions admitted. The two coincide in the pure solitary case, which is why the
wrong one survived a pass; they diverge the moment an exempt event lands. §5.3's
record now says `P(hand_id - 1)` at the field and the regime bool is the
disjunction above, which are two quantities because the two routes need two.

**Why this is the disposition, and what it costs.** In the solitary regime this
peer's belief is *no other seat is participating in hand `k`*. A verifying,
canonical, correctly-signed chained event of hand `k` from another seat is that
seat's signature on the opposite belief. **They cannot both be true, and the
event is the only wire evidence that separates the two situations a solitary peer
cannot otherwise tell apart** — the opponent is gone, or the opponent is there and
we disagree. A peer that is genuinely alone receives nothing and drains normally,
so the drain path is untouched and costs nothing. A peer that is not genuinely
alone receives the contradiction **on its first solitary hand**, because the peer
that did not narrow is still emitting.

**That last sentence is true about the emitter and says nothing about the
receiver, and the receiver is the half the rule is written on. This paragraph was
half an argument and the missing half was `L4`.** The contradiction is emitted on
the first solitary hand; it *arrives* one network delay later, at a peer that has
by then computed several more hands, because a solitary hand has no stage that
waits for anybody and `hand_delay_ms` is not an engine input — it is a
presentation delay and *"the protocol never waits for it"* (`STATE_MACHINE.md`
§2). §8.3's hand-boundary row budgets `hand_delay_ms + crypto_step_timeout_ms`
for the boundary, which is an upper bound on how long a peer may wait and not an
instruction to wait at all; nothing in this document paces a hand. Under the
rule as first written — present tense, scoped to the receiver's current hand — the
event was dropped for being late and K1's trace ran to completion unchanged: both
peers drain, both reach §9.3's end condition, both privately win the tournament.
The rule is now anchored on §5.3's retained record and answered in the past tense,
so it fires **on the first contradicting event to arrive at all, whenever it
arrives**, and §4.0 step 10b is the step that carries it. **The rule made §4.0's
treatment of a chained event naming a past hand consensus-critical, which it had
never been**, and that is stated here because the previous revision of this
paragraph named its dependants and named only the ones inside this document.

Without it, K1's trace runs: one dropped `HAND_INIT` copy, both peers derive
different `P`, each rejects the other's copy, each narrows to `{self}`, each
self-completes every collective stage of every later hand, each drains the other
one blind at a time, and each reaches §9.3's tournament end condition **naming
itself the winner**. Nothing in that trace is invalid, no deadline fires, no
invariant breaks, chip conservation holds at both, and no checkpoint is placed
(K3) — so the two peers are two tables that were one, and neither can tell. That
is an integrity failure, and `SPEC_CS.md` §19 is explicit about the ranking:
*"Security má přednost před pohodlným dokončením handy."* A table that stops is
the price, and it is the right price.

**What it does not do, stated rather than hidden.** It does not make `P` agreed,
and it does not close `Q-10`; it converts one outcome into another. The residual
is a **bidirectional** partition — each peer hears nothing from the other for the
whole drain — under which both drain, both finish, and no contradiction reaches
either. That outcome is not K1's: it is what D-013's drain does under a partition
whatever `P` is defined to be, it is indistinguishable at both peers from the case
the drain exists for, and there is no wire evidence that separates them, because
by construction no wire carries anything. K1's own trace — one dropped frame, both
peers transmitting throughout — is closed, and it is closed unilaterally: the
freeze needs no cooperation, no quorum and no reply, so it holds even if the
contradicting peer never speaks again.

**Rejected alternatives, and why.** *Floor `|P(k)|` at 2* — a table with fewer
than two participants pauses rather than dealing — was the other candidate. It
prevents the fork instead of exposing it, and it makes the own-emission question
harmless, since both readings then land on `Paused`. It is rejected because it
deletes the drain, which is the whole of D-013's liveness answer and of §12.1.2's
ending: a heads-up table whose opponent leaves would pause forever, nobody would
bust, and no end condition could fire — D-013's fixed point again, reached from the
other side. It also strands the paused peer, whose `P` cannot grow without hearing
events it is no longer in a stage to accept. The solitary-stage rule buys the same
integrity property on the only path where the two differ — a peer that is not
alone — and buys it without paying for it on the path where they do not.

**What now depends on this rule, and must be checked with it.** §3.1's frozen
roster deliberately routes a boundary-window disagreement into a **stalled stage
0** rather than into a forked genesis, and a stalled stage 0 is the one place `P`
narrows; §4.10's boundary window closes per-receiver for the same reason; and
§4.9's checkpoint-8 box routes an out-of-set `STATE_HASH` that races
`HAND_INIT(k+1)` onto the same stall. All three are safe **because** a narrowed
`P` can no longer complete a hand in silence. An editor who deletes or weakens the
solitary-stage rule reopens §3.1, §4.9 and §4.10 in the same edit, and the failure
it reopens is J1's, not K1's.

**And what this rule now depends on, which is the half the previous revision did
not write down.** Three rules, and the first two are in other sections of this
document rather than in this one, which is where the last four passes have found
the worst defect each time:

1. **§4.0 step 10b**, the evaluation of a chained event naming a finished hand.
   Without it the trigger cannot fire, because the trigger's own event is late by
   construction. That is `L4`, and it is checked: step 10b runs before step 11, in
   every phase, on every chained event, and routes to step 12a.
2. **§5.3's retained record.** The regime test is answered from it, so a hand
   whose record has been evicted answers *"not solitary"* and the event is dropped
   as it was before. That is a bound and it is stated with its cost: the record is
   LRU-capped at `MAX_RETAINED_HAND_RECORDS` hands, and K1's own trace lands the
   contradiction within one network delay of the first solitary hand, so eviction
   reaches only a contradiction that has been in flight for thousands of hands —
   which is the bidirectional partition this rule already concedes it cannot see.
3. ~~**§6.1's `signed_this_hand`.**~~ **Deleted, and the item is kept struck
   through because the reasoning above still refers to it.** It was what made the
   checkpoint-8 exemption a refinement rather than a hole. The premise was that
   two honest peers could be made to agree about it; they cannot, because the
   rule that set it — *this peer accepted an event signed by that seat* — is a
   fact about the accepted chain of **one** peer, and on a lossy link the two
   accept different sets. Measured, it manufactured the divergence it was meant
   to detect: 124 refused settlements in one nine-minute run, 100 % of them
   differing in this hash and none in any figure of money. §6.1 records the
   removal and the measurement; §4.9's box records what now carries the
   exemption, and that it carries less.
4. **§4.9's cardinality floor on the reconciliation stage**, which is what makes
   the freeze a freeze. The freeze routes into §6.3, and §6.3 step 3 ends in a
   stage whose completion releases it; while that stage was required of *the
   checkpoint's own set*, a solitary peer completed it in the step it emitted into
   it, agreed with itself, and resumed — so the rule installed a freeze the same
   document immediately handed back the key to. That was `N1`. The floor is now
   normative in §4.9 (`R(c) ∪ W`, never fewer than two seats) and the release
   condition is exhaustive in §6.3 step 3. **An editor who deletes the floor has
   deleted this rule**, whatever this section still says.

A status field could not do the job, and the circularity is the whole reason: a
seat's status changes only
through a chained event (D-012), a silent seat emits none, so nothing can ever
change its status and a set defined on it contains that seat **forever**. D-013
records what that cost — stage stalls, the hand deadline expires, every stack
restored by §4.10, next hand byte-identical, nothing ever busts, no end condition
can fire. One `hand_deadline_ms` per iteration — since `G4-P3` a per-table figure
bounded below by a floor that scales with the seat count (§8.2), which §13 sets at
3 300 000 ms for the rated preset's ten seats: **fifty-five minutes** per iteration,
not the ten this sentence claimed for four passes (`G7-S7`) — unbounded. **A required emitter set defined
on a seat's status is a defect in this document**, §2.9 is the sweep that says
there are none left, and §4.11's *Emitter* column is where a new one would be
visible.

**Within a hand, silence still stalls, and that is deliberate.** `P(k)` has hand
granularity. A seat that signs `HAND_INIT` and then goes quiet blocks the stage it
owed a contribution to, and §8 disposes of that hand at `hand_deadline_ms`. What
D-013 removes is the **repetition**: the seat is outside `P(k)`, hence outside
every `R` of hand `k+1`, so exactly one hand pays — the one the seat went silent
in — and every later hand proceeds among the seats that are actually there.

**A seat outside `P(k)` keeps everything except its vote.** It keeps its seat, its
stack and its `roster_hash(k+1)` entry; it is not dealt in (§4.4); it posts blinds
and antes as dead money exactly as D-005 requires, so it drains and **genuinely
busts**, which is what lets a tournament reach an end condition at all. It rejoins
by **signing a chained event** — a `PLAYER_SIT_IN` at a hand boundary, whose
legality condition §4.10 widens for exactly this, or a checkpoint-8 `STATE_HASH`
that agrees with this receiver's — which places it in `P` and therefore back in
every `R` of the following hand. **If the event arrives after this receiver has
closed the window it names, it is not lost: §4.9's readmission set makes its
sender an accepted emitter of the next hand init this receiver runs** (`N5`,
`P2`), so its own copy of that `HAND_INIT` places it in `P` and in every `R` from
the hand after — which is what keeps the promise honest at a peer that is
draining, where every window is zero-width. The readmission costs one hand of
latency and buys the property `P2` needed: **the set a returning seat re-enters is
grown by that seat's own current-chain signature and never by a replay of an old
one.**
Signing requires it to be alive, which is the entire test. **No certificate, no vote, no quorum, no proof
and no attribution appear anywhere on this path, and none may be added**: this is
deliberately not the machinery of D-006 to D-008, which D-010 made inert and OQ-F
still questions.

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

Every event of stage `s` carries `previous_event_hash = stage_hash(s-1)`, with
`stage_hash(-1) = GENESIS(hand_id)` and
`stage_hash(BOUNDARY_SEQUENCE_BASE - 1) = TERMINAL(hand_id)` — the second for the
hand boundary window of §4.10, whose events all chain from the terminal because
no emitter can know which other seats will occupy the window.

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
* every timeout vote and certificate — **none in version 1 (D-015)**: nothing
  emits either, so this line describes a class the transcript of this version is
  always empty of. It is kept because a transcript reader must not treat the
  absence of the class as a corrupted transcript, and because a later version
  puts them back in exactly this position;
* the final `HAND_COMPLETE` or `HAND_ABORT` with the per-seat chip deltas.

**Kept beside the transcript, but not part of it:** every `DISPUTE` received for
the hand. A dispute is unchained (§2.3, §4.9), so it occupies no stage and enters
no `stage_hash`; it is stored alongside the chain because §6.4's divergence report
needs it and because a dispute's *contents* — a signed `STATE_HASH`, a
`kind = 3` `SignedEvent` — are verifiable on their own. **The
`EquivocationProof` that stood third in that list is deleted under D-015**: no
dispute of this version carries one, and a `kind = 2` dispute is dropped rather
than stored, so it is never kept beside anything. Nothing about the chain's integrity
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
| 10a | **NOT IMPLEMENTED IN VERSION 1 — see §5.2.5 for the checks that run in its place, and §5.2.1's header for why this one does not.** As specified: for a chained event, look up **`slot(E)` exactly as §5.2.1 defines it** — this step reproduces no part of that tuple and reads it whole; for an unchained event, the per-type rule in §5.2.1's box. **For a chained event whose `hand_id` names a hand this receiver has completed, this step is *skipped* and step 10b is the whole of its anti-replay rule — normative, and this is the disposition of `N2`.** §5.3's `event_class == 0` store is one map per table per hand and is dropped when the hand ends, so there is no slot to look such an event up in and **none is created**: no `hand_id` a sender chooses causes an allocation of any kind, here or anywhere in the pipeline. **One exception, and it is the only structure that outlives its hand:** the checkpoint-8 `STATE_ACK` band of §4.9, whose slots §5.3 retains until `TERMINAL(k+1)`; an event landing there is looked up here exactly as a live one is. Skipping the step gives up nothing a stale event could use, because step 10b applies none of them, enters none in a `stage_hash` and counts none into a `P` of an initialised hand — so there is no effect for a duplicate to repeat. §5.3 states the bound and the one capability the skip does give up | violation or duplicate-drop |
| 10b | **Stale hand — normative, and this is the disposition of `L4`.** If the envelope's `hand_id` names a hand this receiver has **completed**, a chained event is *not* dropped for arriving late. Step 10a has been skipped for it (above) and this step is its whole anti-replay rule. It is evaluated against that hand's **retained record** (§5.3): if the record says this receiver **was** in the solitary regime for that hand (§3.2, past tense) and `sender_seat` is outside the recorded **`P(hand_id - 1)`** — the required emitter set *of the hand the event names*, never `P(hand_id)`; §3.2's rule and step 12 both say `P(k-1)` and this step now says the same thing, which is `N7` — and the type is neither of the two exempt cases, it routes to step 12a; if it is a checkpoint-8 `STATE_HASH` (§4.9) it is compared against the retained `checkpoint8_state_hash`, and a **mismatch** enters §6.3 at step 1 and, where the record says the hand was solitary, routes to step 12a with it (`N1`), while a **match** from a seat outside the recorded set adds its sender to §4.9's readmission set, as a `0x0804 PLAYER_SIT_IN` of that hand's boundary window does (`N5`); otherwise it is dropped as out of stage, exactly as before. It is **never applied**, never enters a `stage_hash`, **counts into no `P` of a hand this receiver has already initialised**, and **enlarges no required emitter set of any hand — `P2`**: §4.9's readmission set is read once, at the next hand init, where it widens that stage's *accepted* emitter set and not its required one, which is the entire exception and is the reason a replay of this event is inert. **One more disposition, and it is the shrink-side twin of readmission (§4.9's late-roster-repair box):** a `TIMEOUT_CERT` of a hand this receiver ended by an **abort**, verified against that hand's own roster, banks its roster half — position-free, D-024 point 1 — for as long as `HAND_INIT(hand_id + 1)` has not completed at this receiver; a bank that changes `R(hand_id + 1)` re-derives that hand's opening and re-opens it at the corrected genesis. Nothing is applied, nothing is chained, and a settled hand banks no timeout certificate. **Its grow-side twin (D-028, §8.3.1):** a `RETURN_CERT` of a hand this receiver has **settled**, verified against that hand's own roster and against the evidence it carries, banks its subject into `IN(hand_id)` under the same retention, and a bank that changes `R(hand_id + 1)` re-derives that hand's opening the same way. It never reaches step 13 | freeze, compare, readmit, repair, or drop |
| 11 | Decode the payload struct, canonicality gate, per-field range checks | violation |
| 12 | Stage legality: is this `event_type` from this seat expected at this `sequence`? **And the stage-contribution rule: a seat that has already contributed to a stage may not contribute to it again under a different `event_type`** — the one exception is the terminal `HAND_ABORT` of §4.10, which by construction lands at a stage its emitter has usually already contributed to, and which is also one of the two events exempt from step 10a's chain-position rule (§4.10; the other is a boundary event, whose parent is `TERMINAL(k)`). **And the solitary-stage rule (§3.2): if this receiver was in the solitary regime for the hand this event names and the sender is outside `P(k-1)`, the event is a state divergence and not a rejection — see step 12a.** And **an out-of-set checkpoint-8 `STATE_HASH` is not an out-of-stage event**: §4.9's box widens the accepted set at that one stage and this step must not reject it | violation |
| 12a | **Solitary-regime divergence (§3.2, K1).** Reached when step 12 says so, **and when step 10b says so for a hand already finished** — the second route is what makes the rule able to fire at all (`L4`). The event is *not* rejected, *not* applied, and *not* counted into any `P`: the receiver enters §6.3 step 1 and freezes, and the freeze is **latched** — released by §6.3 step 3's reconciliation stage alone, which §4.9 requires of at least two seats, and by nothing else. **Two cases are exempt from reaching this step on arrival** and fall through to ordinary handling: `0x0804 PLAYER_SIT_IN` (§4.10), the one message a seat outside `P` exists to be able to send, and a **checkpoint-8 `STATE_HASH`** (§4.9), which is compared instead. **The second exemption is from arrival and not from disagreement (`N1`):** a checkpoint-8 `STATE_HASH` whose value differs from this receiver's own, on a hand the retained record says was solitary, reaches this step with the same finding and the same latch (§4.9, §6.3 step 1) | **freeze; §6.3** |
| 13 | Semantic legality: the poker engine re-validates the action from scratch | violation |
| 14 | Cryptographic proof verification (shuffle proof ≈ 42 ms, DLEQ ≈ 0.1 ms) | violation, `INVALID_SHUFFLE_PROOF` |
| 15 | Apply to state | — |

**Step 10b is new in this pass and it is the step K1's fix could not fire
without.** Before that fix, §4.0 was a validation pipeline in which every failure
mode was *drop*, and dropping a late event cost nothing, because a late event of a
finished hand carries no information a finished hand needs. The solitary-stage
rule inverts that for exactly one class: **a late chained event from a seat
outside `P` is the only wire evidence that distinguishes "I am alone" from "I am
wrong"**, and every other step of this pipeline would have discarded it before
step 12 for the ordinary reason that it is late. Nothing paces a solitary hand —
every collective stage of it has `R = {self}` and completes in the step that emits
it, and `hand_delay_ms` is a presentation delay that the protocol never waits for
(`STATE_MACHINE.md` §2; §8.3's boundary row budgets for it as a ceiling, not as a
wait) — so a solitary peer traverses a hand in microseconds and the contradicting
copy reliably arrives naming a hand that peer has finished. The trigger is
therefore anchored on a **record** rather than on a live phase.

**It runs on every incoming chained event, in every phase, at every table, and a
document that makes a phase absorbing against it has reopened K1.** That includes
a table the engine has closed: the fortieth drain hand of a silent fork is
followed by a tournament-end condition, and a rule that stops evaluating
contradictions at that point loses the race it exists to win. Verification here is
cheap and reaches nothing — the event is never applied — so there is no phase in
which refusing to run this step buys anything. The engine half of that statement,
and which phase holds the freeze, is `STATE_MACHINE.md`'s (D-011 rule 1).

**Its cost is one memory bound and §5.3 is the section that owns it.** The record
is three quantities and a hash per finished hand, LRU-capped; nothing else about
the pipeline changes. The step sits **before** step 11, so the
cheap-before-expensive ordering is untouched: it reads only envelope fields that
steps 1–10 have already validated, and it costs a map lookup.

**And step 10a is skipped for the events this step evaluates — `N2`, and it is
the step this fix newly made load-bearing rather than a defect of its own.** Step
10a runs **before** step 10b and indexes §5.3's per-hand `event_class == 0` store;
§5.3 drops that store when the hand ends. So a chained event naming a finished
hand reached a lookup with no store to be looked up in, and the two available
readings were both wrong: **allocate a map for the named hand** is an
attacker-driven allocation keyed on a `u64` the sender chooses, in the section
whose first sentence forbids exactly that, and **treat the absence as a miss and
fall through** is correct and was unstated, which is a coin-flip for an
implementer. The rule is the first sentence of row 10a: for such an event step 10a
is skipped, the event occupies no slot, no store is created, and step 10b is the
whole anti-replay rule.

**What the skip gives up, stated rather than glossed.** Step 10a's two jobs are
duplicate suppression and giving §5.2's equivocation predicate a slot to detect a
collision in. **Duplicate suppression is not needed**: step 10b applies nothing,
chains nothing and grows no initialised hand's `P`, and each of its four
dispositions is idempotent — a freeze already held is not re-entered, a latch
already set is not re-set, a seat already in the readmission set is already in it,
and a drop is a drop — so a repeat costs a map lookup and changes nothing.

**That third clause was false for one pass and its repair is `P2`, recorded here
because this paragraph is where the claim is made.** *"A seat already in the
readmission set is already in it"* is a statement about the set, and the set is
**cleared at every hand init** (§4.9). Idempotence of the write does not survive a
consumer that empties the container between writes: while `A` enlarged
`R(HAND_INIT(m+1))`, one replayed, perfectly **agreeing** checkpoint-8
`STATE_HASH` re-enlarged that required set once per hand, for every hand of the
4 096 §5.3 retains a record for, and stage 0 stalled to the full hand deadline
each time with `cause = 1` and `attributed = []`. **No key and no forgery were
needed**, only a signed event the accused really emitted and §1.5's permission to
forward it. §4.9 now has `A` widen the stage's *accepted* set instead, which is a
disposition that genuinely repeats to no effect, and the clause is true as
written. **The shape is the one the last gate named**: this step deleted a guard
and §4.9 added a disposition behind it, in the same pass, and each half was right
about itself. Re-read this list after any edit to §4.9's box, not before it.
**Equivocation detection at a finished hand is given up, and it was already
gone**: §5.3 keeps only `TERMINAL(k)`, the terminal body and the retained record
after a hand ends, so the *other* copy the predicate would pair this one with is
no longer in memory whatever step 10a does. An equivocation whose second copy
arrives after its hand has closed is **not detectable by this receiver and never
was**; the full transcript is on disk (§5.3) and a later reader can still find it.
That is a weakening of a claim this document never actually made, recorded here so
that no future pass reads the skip as having removed something.

**The bound, which is what §27 asks for.** Per stale-hand event: one lookup in an
LRU of `MAX_RETAINED_HAND_RECORDS = 4 096` records at 56 B each — under 256 KB,
allocated once and never grown by an incoming event — plus, for a checkpoint-8
`STATE_ACK` still inside §4.9's retention, one slot in a structure §5.3 bounds at
`8 × MAX_SEATS` entries for at most one finished hand at a time. **Zero bytes are
allocated on the naming of any other `hand_id`**, whether the receiver ever saw
that hand or not: an unknown hand misses the LRU and is dropped. Volume is §9.5's
question and §9.5's ladder is keyed on volume and never on fault, which is the
same answer this document gives for every other cheap-to-emit message.

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
attributed in a `DISPUTE`. **Nothing further follows automatically (D-010), with
exactly one exception — a D-014 tier-1 finding — which the box below states in
full rather than leaving to §11.**

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
> failed verification — with exactly one exception, D-014 tier 1, stated below.**
> That includes the transport layer: an `EquivocationProof`, an invalid signature
> and a failed shuffle proof are each evidence, and none of them is an input to
> any peer-**blocking** decision anywhere. A block list may be present and
> compiled in, but its only trigger is the **user**. §9.5's volume-keyed ladder is
> untouched by this and is the whole of what remains automatic.
>
> **The sentence deleted from this box named `NETWORK_STACK.md` §6.6 and §11.5 as
> asserting the opposite. They no longer do, and had stopped before this pass**:
> §6.6 now says *"a proven protocol violation produces no transport action at
> all"* and §11.5.1 forbids the `block_peer` path in a box of its own. A normative
> box that accuses a compliant document of non-compliance sends the next reader to
> re-fix something already fixed, and is the same class of defect as `N8` — a
> citation that outran its source, pointing the other way. **What `NETWORK_STACK.md`
> owed was the *exception*, not the rule, and it is now paid**: it carried **zero**
> occurrences of `D-014` while its §0.1 and §1.2 prohibition 7 stated the
> no-unseating rule in the corpus-wide, every-layer form D-014 tier 1 narrows —
> a contradiction rather than an omission. Both sites now say *at this layer* and
> name the exception without restating it, §11.5.1's box is unchanged and gains
> the paragraph that stops an implementer arriving from `STATE_MACHINE.md` T64
> reading it as licence to call `block_peer`, and that layer's own D-014 pass is
> `NETWORK_STACK.md` §0.6. **The transport prohibition is absolute and unamended**,
> exactly as this box has said (`DECISIONS.md` `G4-P5`).
>
> **The exception, exactly, and it is narrow.** D-014 narrowed D-010 point 3 and
> narrowed nothing else. A **tier-1** finding — an event **signed by the accused**
> whose illegality any peer decides alone from that event's own bytes: a failed
> `verify_strict` at step 9, a non-canonical encoding at step 2 or step 4, an
> out-of-range field at step 11, a failed proof at step 14, a deck that is not a
> permutation, a signer that is not a party to the table — **removes its sender
> from the table**, by the derivation §4.9's
> `kind = 3` box specifies and reaching canonical state only through
> `HAND_INIT(k+1)`'s collective body. A **tier-2** finding — illegal only against
> game state — removes nobody until this receiver holds the completed `STATE_ACK`
> stage §4.9 requires; before one exists it voids the hand and removes nobody.
> **Nothing else moves:** the exception ends in a seat, never in a chip, and
> D-010 points 1 and 2 stand unamended — `n(1) attributed` is still read by
> nothing, and no removal is derived from any field naming a culprit.
>
> **One clause that stood in this list is deleted and may not return: *the event
> chains to a parent that does not exist*.** Whether a parent exists is decidable
> only against **the receiver's own store**, so a single dropped or reordered
> frame would remove an honest player — the exact failure the two tiers exist to
> prevent, sitting in the tier meant to be safe. It is gone from §4.9's
> acceptance gate, from `DECISIONS.md` D-014, from `THREAT_MODEL.md` §5.1's D&A
> cell and §5.2's row 13, and from `src/security/validation.rs`, whose
> `no_tier_one_violation_depends_on_the_receivers_store` test is the tripwire
> against it coming back. A missing parent is still a reason to **buffer or
> reject** the event (§5.1), which is what it always was; it is not evidence
> against anybody. `THREAT_MODEL.md` **§5.1.1** states the general test a proposed
> tier-1 addition must pass — three questions, all of which must be answered *no*:
> does deciding it read anything the receiver stores; can the answer change with
> what the network did; does it need a second message, a count, a vote or a
> certificate — and it is the normative owner of that reasoning (D-011 rule 1).
>
> **Everything else in this document keeps the full prohibition.** Every liveness
> judgement, every attribution, every fault record, every timeout certificate and
> the whole of §8 remove nobody. The line D-014 crosses is **who signed the thing
> that is wrong** — the accused's own key, or somebody's opinion about the accused
> — and it is the only line it crosses, because that is the one input on which two
> honest peers cannot reach different verdicts.

**Why this box says it, when §11 already did.** §11 is a routing table, read by
someone tracing an attack to the mechanism that acts on it, and it carries the
exception in full and says it is *"stated here rather than hidden"*. This box is
where a **receiver implementer** reads the rule, and until this pass it said the
opposite of both §11 and §4.9's own `kind = 3` box — and the opposite of the code,
where `src/security/validation.rs` already implements the two tiers with the
tier-2 checkpoint precondition enforced by the type system. A normative box that
binds every document while contradicting the decision it is downstream of is worse
than no box, and this is `N3`.

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
the joiner sent; **that `table_params_hash` (§3.1) recomputed from `advert_event`
equals the joiner's own** — which is trivially true while the founder echoes the
joiner's own copy back, and is required anyway so that an implementation which ever
relaxes the echo rule still carries the parameter check; that the seat is not already
taken in `roster_so_far`; that the roster has no duplicate `app_public_key` and no
duplicate `seat`. **The joiner then connects directly to every peer in the roster
and runs the §1.2 handshake with each.** It does not take the founder's word for
who is at the table — see `TABLE_READY`.

> **`roster_so_far` is a snapshot and this message gives the receiver no way to
> tell how old it is.** `PLAYER_LIST` and `TABLE_READY` both carry
> `list_serial`; `JOIN_ACCEPT` does not, and the validation list above has no
> rule about recency. The founder emits the reply and the broadcast in the same
> turn, but they do not arrive together: measured, one joiner asked at t=10.6 s
> and was answered at t=20.6 s, and in those ten seconds two more seats were
> filled and it had adopted every list up to the last. A reply that slow
> describes an *older* table than the one its receiver already holds, and
> carries nothing that says so.
>
> Adopting it blind rewinds the roster to exactly `seat + 1` entries. Measured
> at ten seats over 900 s, that prediction held for every affected peer with no
> exceptions: the seat that fell from ten members to eight never sealed a table
> and played no hand in the remaining 880 s, and the one that fell to nine had
> **already sealed**, so it went on computing `roster_hash(0)` — and therefore
> `GENESIS(1)` — over nine entries where everybody else used ten. It opened
> every hand about 11.8 s late and finished none of them.
>
> An implementation must therefore not let a `JOIN_ACCEPT` replace a roster it
> cannot prove is newer. This one takes the seat unconditionally and the roster
> only when what it holds does not already seat it where the founder says, which
> is a ranking and not a comparison. **The proper fix is a field**: give
> `JOIN_ACCEPT` the `list_serial` its two siblings carry, and this becomes the
> same `NotNewer` check as everywhere else. That is a wire change and is not
> made here.

---

**`0x0203 JOIN_REJECT`**

*Direction:* founder → joiner. Signed by the table key.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) request_hash` | `bytes[32]` | |
| `n(1) reason` | `u16` | enumerated: `1` table full, `2` seat taken, `3` bad password, `4` buy-in out of range, `5` advert expired, `6` banned, `7` capability mismatch, `8` already seated, `9` out for good (`D-047`), `10` too soon (`S1-GR`) |
| `n(2) retry_after_ms` | `u32` | ≤ 3 600 000; advisory -- for `10`, when the founder seats the key again |

**`10` too soon** (`S1-GR`, the owner's word): a key whose seat ended at this table
-- its player's leave, or the seat given back -- twice or more in the last 10
minutes, and that holds no seat now, is seated again only 30 s after its latest end,
the wait doubling with each further end up to 5 minutes. Every other key is seated at
once. Before the table is set only.

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
| `n(1) table_params_hash` | `bytes[32]` | §3.1's box, over the parameters this table is being formed under. **This field was `advert_hash` and is replaced (J1, D-013)** |
| `n(2) list_serial` | `u64` | strictly increasing per table |

**Roster uniqueness is on three keys and not two, and the third closes `U17`:
`seat`, `app_public_key` **and `peer_id`**.** The field was carried in every
`SeatEntry` from the beginning and no rule ever read it — which is the
carried-and-never-checked shape this corpus names as a defect class in its own
right, because a field nothing validates is a field every reader assumes
somebody else validates.

What the rule buys, stated exactly. It stops **one node holding two seats at one
table**: `n(2) peer_id` must equal the connection's authenticated remote PeerId,
so a second seat from the same node is caught at the founder and again at every
receiver of that table's `PLAYER_LIST`. It stops the accidental case outright —
two copies of the client on one machine, one person joining a table twice — and
it costs a determined attacker one more process.

**The rule is per roster, and multi-tabling is therefore unaffected and
intended.** Each table checks its own roster and no other, so one client may hold
a seat at as many different tables as it likes; what it may not do is hold two
seats at the same one. That is the distinction the rule is drawn on, and reading
it as *one node, one table* would forbid the ordinary way people play.

One consequence worth stating rather than leaving to be discovered: a client that
multi-tables under one `app_public_key` links those tables to any observer of the
lobby. Using a distinct application key per table unlinks them and costs nothing
in this protocol, since the key is per-table already by §4.3's *"not already
seated"* rule.

What it does not buy, and this is not a gap that closing it would fix. **It is
not one person per seat.** A person with two machines, or two containers, or one
machine and a virtual one, presents two PeerIds and two application keys and is
indistinguishable from two people. That is Sybil without an identity layer, which
`SPEC_CS.md` §18 places outside the threat model, and no roster rule reaches it.
The distinction is worth keeping sharp: *one key per seat* and *one node per seat*
are enforceable and now enforced; *one person per seat* is not enforceable and is
not claimed.

*Receiver must validate:* signature by the table key; sorted, unique on `seat`,
on `app_public_key` **and on `peer_id`**;
`list_serial` strictly greater than the last accepted one (this is the anti-replay
for a message that is not yet in a hash chain); **`n(1) table_params_hash` equals
this client's own recomputation under §3.1 from the advertisement it joined under**
— a mismatch means the founder is forming a table under parameters other than the
ones this client agreed to, and the client leaves rather than sitting down; every
entry's `app_public_key` is one this client has completed a §1.2 handshake with, or
is one it must now connect to.

A `PLAYER_LIST` is a **proposal**, not a fact. It becomes fact only when every
listed seat signs `TABLE_READY` over it.

---

**`0x0205 TABLE_READY`**

*Direction:* **collective stage 0 of the setup chain (`hand_id = 0`)**. Every
seated participant emits exactly one, to every other.
*Legal:* once `PLAYER_LIST` names a roster of at least `min_players_to_start` -- or, after
one that size was named, a roster the founder says again of at least two seats (D-044: a
seat given back before the first hand does not un-set the table; the floor is heads-up) --
and this client **hears every other listed seat** on the table mesh (D-060): each is in this
client's copy of the table's group and has been heard there within `QUIET_LIMIT_S` (20 s). A
receiver cannot check that and does not try; it is the sender's rule, and the one that makes a
table start only full of seats that can play with each other. Until then the seat says whom it
cannot hear (§7.11).
*Envelope:* `hand_id = 0`, `sequence = 0`, `previous_event_hash = GENESIS(0)`.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) roster_hash` | `bytes[32]` | `roster_hash(0)` computed from the `PLAYER_LIST` |
| `n(1) list_serial` | `u64` | the serial being ratified |
| `n(2) table_params_hash` | `bytes[32]` | §3.1's box, recomputed by this sender from the parameters it joined under. **This field was `advert_hash` and is replaced (J1, D-013)** |
| `n(3) my_seat` | `u8` | must equal the sender's seat in that roster |
| `n(4) capability_set` | `Vec<bytes>` | ≤ 32; repeated inside the signed table scope so the roster binds capabilities |

*Receiver must validate:* the sender is in the roster at `my_seat`; `roster_hash`
recomputes; **`n(2) table_params_hash` equals this receiver's own recomputation
under §3.1 and equals the `n(1) table_params_hash` of the `PLAYER_LIST` being
ratified**; every seat's `capability_set` contains `deck/bs-bg12-secp256k1/1` and
a seat-count capability covering `max_players`. If any seat's capabilities are
insufficient, the table does not start and the founder must re-form it.

**That parameter check is new and it is the second half of J1.** The field it
replaces was `n(2) advert_hash`, and the gate's finding was precise: the field was
"carried into a signed body and read by nothing" — a per-receiver quantity inside a
signed payload with no gate on it anywhere in this document. The replacement is a
value every honest joiner computes identically, and it is now **checked**, so a
founder who advertised two different games is refused here, loudly, before any
chained event depends on it.

**This is the moment the table becomes real.** The set of `n` `TABLE_READY`
events is unanimous ratification of the roster by every participant, so from here
on nobody can be added, removed or reordered without a new table. The founder's
special position ends here.

```
session_id = h("p2p-poker v1 session",
               [ table_id, table_params_hash, roster_hash(0),
                 for each seat s ascending: event_hash(TABLE_READY from s) ])
```

Every subsequent hand's `GENESIS(k)` contains `session_id`, so every hand event is
bound to this exact roster ratification. Two tables with the same participants and
the same parameters still get different `session_id`s — **because `table_id` is a
fresh key for every table, and for no other reason.**

**Corrected.** This paragraph said the uniqueness came from *"the `HELLO` nonces
[which] feed the connections and the `join_nonce`s [which] feed the join requests
whose hashes are in the roster chain"*. **No nonce enters this construction.**
Enumerating the four inputs settles it: `table_id` and `table_params_hash` carry
none; `roster_hash(0)` is `u8(seat) ‖ app_public_key ‖ u64_be(stack)` per §3.1 and
carries none; and a `TABLE_READY` body is `roster_hash`, `list_serial`,
`table_params_hash`, `my_seat` and `capability_set`, which carries none. A
`join_nonce` reaches `JOIN_REQUEST` alone, and that message is unchained and enters
nothing downstream — which §4.3 states in the same breath as the reason it is
unchained.

The correction matters in one direction in particular: an editor who believed a
nonce carried the uniqueness could relax the freshness requirement on `table_id`
on the grounds that something else covered it. Nothing else does.

**`advert_hash` is gone from this construction (J1, D-013)** and §3.1's
`table_params_hash` stands in its place. `session_id` is a component of every
`GENESIS(k)` and of `ctx` (§4.5), so a per-receiver value here reached **every hash
in this protocol except `event_hash` itself** — which is the size of what the
removal closes. `ctx` inherits the fix with no edit of its own, because `ctx` never
named `advert_hash` directly: it names `session_id`.

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

*Direction:* collective stage 1 of the setup chain; the required emitter set is
`P(0)`, the seats that signed `TABLE_READY` (§3.2). The phrase "every seated
participant" stood here and is deleted: it is a status word, and §3.2 forbids an
`R` defined on one.
*Legal:* after `TABLE_READY` is complete.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) commitment` | `bytes[32]` | the canonical five-part commitment below |

```
commitment_i = h("p2p-poker v1 rng-commit",
                 [ table_id, session_id, committer_app_public_key, r_i, salt_i ])
```

**This section is the normative owner of `commitment_i` and of the `seed` combine
below, for the whole corpus, and this is the disposition of K6 (D-011 rule 1).**
The earlier two-part `h(..., [r_i, salt_i])` is deleted: it bound neither the
table, nor the session, nor the committer, so a commitment could be lifted from
one table into another (C-2).

**`K-6` is closed and this paragraph is rewritten in the past tense, which is the
disposition of `L6`.** `CRYPTOGRAPHY.md` §7.3 **used to** reproduce both
constructions in full with an ownership sentence pointing the other way — *"this
five-part binding is the canonical one; the two-part form that `PROTOCOL.md` §4.4
previously carried … **was corrected to match this section**"*. The two copies were
byte-identical, which is exactly the condition D-011 rule 1 was adopted for after
five passes of copies drifting, and it was settled in the direction the rest of the
corpus runs: these are **wire** values — they are the payload of `0x0301` and
`0x0302` and a receiver check is written against them in this section — and
`CRYPTOGRAPHY.md` §6.4's `ctx` blocks were deleted under the same rule in the H7
sweep. **The owner has since made that edit.** §7.3's two code blocks and its
"corrected to match this section" sentence are deleted, and it now reads
*"`PROTOCOL.md` §4.4 owns `commitment_i` and `seed`, and this section owns the
argument for why they are shaped that way"* — the direction this section asked
for. What §7.3 keeps is what only it can say: the binding, hiding, ordering and
anti-regrind arguments, its recorded `SPEC_CS.md` §16 deviation, and its D-012
check. It keeps no construction.

**The tense is the point, and it is why this is a rewrite rather than a deletion.**
A paragraph that reports another document's defect must not be left standing after
that document has fixed it: the next editor reads it as a live finding and either
re-files it or re-makes the fix. That is `J-4` and `J-5` in the other direction and
this is its third iteration — the first in which the stale report was **this**
document's about `CRYPTOGRAPHY.md` rather than the reverse. `CRYPTOGRAPHY.md` §0
states the general rule in the words this pass adopts: *"a claim that was false
when made and is true now must not be left reading as evidence that the check was
once run."* The history is kept, in the past tense, because it is the argument for
why the ownership is where it is.

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

*Direction:* collective stage 2 of the setup chain; the required emitter set is
`P(0)` (§3.2), the same set as stage 1's.
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
emitter set is `P(k-1)`, §3.2's participation set. Each such seat computes the
byte-identical body from the state after `TERMINAL(k-1)`, signs its own copy under
its own application key, and emits it. There is no writer. (The button position may
still be a dead, empty seat under the TDA dead-button rule. [RULES A1.3])

**The required emitter set, normatively. This is §3.2's rule instantiated, it is
the disposition of J2, and the clause it replaces is deleted (D-013):**

> **`R(HAND_INIT, k) = P(k-1)` — the seats that signed at least one chained
> event this receiver accepted between `GENESIS(k-1)` and `GENESIS(k)`. For
> `k = 1`, `P(0)` is the set of seats that signed `TABLE_READY`. §4.9's
> readmission set `A` does not appear in it and this is `P2`'s disposition.**
>
> **`A` widens the *accepted* emitter set of this one stage and nothing else.**
> It holds the sender of a `0x0804 PLAYER_SIT_IN`, or of a checkpoint-8
> `STATE_HASH` that **agreed** with this receiver's own value, that arrived naming
> a window this receiver had already closed (§4.9, §4.0 step 10b, `N5`). It is
> read at this one place, cleared here, and read nowhere else. A `HAND_INIT(k)`
> copy from a seat in `A` is accepted rather than rejected at §4.0 step 12, enters
> `P(k)` under §3.2, and **neither completes nor blocks the stage** — completion
> is `heard ⊇ P(k-1)`, which `A` does not move. On a table where every event lands
> inside its window — which is every healthy table — `A` is empty and nothing here
> is reachable at all.
>
> **`R` is never enlarged by an event of a hand this receiver has finished, and
> that is the rule `P2` installs.** §4.0 step 10a is skipped for such an event, so
> a replay of one is not suppressed; a rule that let a replayed, *agreeing*
> checkpoint-8 `STATE_HASH` grow this set grew it once per hand, for the 4 096
> hands §5.3 retains a record for, and stalled stage 0 to the hand deadline each
> time. The only thing that grows a required emitter set is now a seat's own
> accepted copy of a **chain-`k`** event, which no replay of an older chain can
> forge (§2.4). §4.9's box carries the argument in full.
>
> **It may shrink, by one road, and only after an abort.** A verified `TIMEOUT_CERT`
> of a hand `k` this receiver ended by an abort, arriving before `HAND_INIT(k+1)`
> has completed here, removes its subject from `R(k+1)` exactly as the same
> certificate arriving in time would have (D-024 point 1: the roster half is
> position-free), keyed on the subject digest so a replay removes nothing twice.
> The terminal does not move — `ABORT_TERMINAL(k)` is a function of `GENESIS(k)`
> alone — so the only quantity that moves is `R(k+1)`, and with it
> `GENESIS(k+1)`. After a *settled* terminal nothing moves: that terminal is the
> `HAND_COMPLETE` stage hash, which a receiver that did not chain the certificate
> stage cannot reach, and a late certificate there is D-024 point 4's fork.
> §4.9's late-roster-repair box carries the rest.
>
> `dealt_in ⊆ R(HAND_INIT, k)` is unchanged in form and now genuinely follows the
> required set, so a seat readmitted through `A` is dealt in at hand `k+1` rather
> than at hand `k` — one hand of latency, which §4.9 states as the cost.

> **Adopting a hand — normative, D-029 (`S1-CR`).** A receiver that holds the
> table's roster and session and **no hand of it** — a client that restarted,
> rejoined the group and holds nothing, or one that sat down at a table already
> playing — may open hand `k` from the members' own `HAND_INIT(k)` copies rather
> than derive it: copies from a **strict majority of the occupied seats**, each a
> chain-`k` stage-0 event of this table signed by a roster key, grouped by
> `(previous_event_hash, body)` and byte-identical within the group; the parent
> is taken as `GENESIS(k)`, the body's `n(9) stacks`, `n(1) button_position`,
> `n(4) level` and blinds as the opening's, and **the body's `n(8) dealt_in` joined
> with the seats that signed as `R(k)`** (D-039; it was the signers alone). A receiver
> that `dealt_in` names is a member of the adopted hand: it opens it at that genesis
> and emits its own `HAND_INIT(k)` with the same body, which is the copy the table's
> stage 0 is waiting for. A receiver it does not name is a bystander, as below.
> The body's stacks must hash to its `n(10) roster_hash` and its blinds must be
> §7.2's for hand `k`, so a majority cannot name a table that does not exist.
> **Nothing here enters a genesis**: the receiver adopts what the table derived
> and derives nothing itself until it is dealt in — at each boundary it adopts
> the next hand from the copies again, because the set that signed is `R(k)`
> only when every required seat's copy arrived, and a derivation from a smaller
> set is a genesis nobody shares. It is a bystander in the adopted hand: it
> signs nothing, follows every stage, and at a settled boundary asks to sit in
> (§4.10). The boundary checkpoint decides whether it followed, and an agreeing
> checkpoint is the evidence a `RETURN_CERT` needs (§8.3.1) — that certificate
> is what makes it a deriving member again, and it also says the adopted set
> was exact. A majority at one genesis is followed whatever a minority signed;
> a table with no majority at one genesis is one this receiver waits at.
>
> **A seat on a hand nobody else has — normative, D-038 (`S1-DH`).** A receiver that holds a hand of this table
> while a strict majority of the seats in its own required and returned sets, other than itself, have signed hands
> two or more ids past it drops that hand and every local fact of the branch, keeps the table, the group and its
> keys, and takes the road above from the table's copies: it adopts the running hand as a bystander and asks to sit
> in at its boundary (§4.10, §8.3.1). Nothing of the table's rests on the dropped branch, since no other seat signed a
> frame of it; a seat outside those sets is not evidence of where the table is, and two seats are not the table.
> The return counts under D-032.
>
The deleted clause read *"every seat that will be `dealt_in`, plus every occupied
seat that is absent or sitting out and therefore posts dead money"*. It defined the
table's liveness gate on a seat's **status**, and **no status ever removed a seat
from it**: `dealt_in = false` did not, `SittingOut` did not, and `Absent` — the
status this corpus spent two passes arguing about — was named in the *inclusion*
clause. Since a status changes only through a chained event (D-012) and a silent
seat emits none, a silent seat was a required emitter forever, and the fixed point
D-013 derives is the whole table: stage stalls, the hand deadline expires, every
stack restored by §4.10, next hand identical, nothing ever busts, so no end
condition can fire either. **The table makes no progress, ever.** What replaces it
costs one hand of stall rather than an unbounded sequence of them.

**The price is that a dead-money seat no longer signs the hand it pays into**, and
it is the right price by this section's own argument for the collective form — *a
body with no choices in it must not give one seat a veto* — which never intended to
extend the veto to seats that take no cards. Nothing leaves the record with it:
`n(9) stacks` and `n(11) ledger_delta` range over **every occupied seat**, so a
non-participating seat's postings are inside the signed body of every emitter
whether that seat signs or not, and its stack stays inside `roster_hash(k)` and
hence inside `GENESIS(k)`.

*Legal:* immediately after `TERMINAL(k-1)`, with no human input. `SPEC_CS.md` §4
requires the next hand to start automatically and deterministically, not to be
"driven by whoever clicks first". [RULES A9]

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) hand_id` | `u64` | must equal the envelope's `hand_id` |
| `n(1) button_position` | `u8` | `< max_players` |
| `n(2) sb_position` | `u8` | `< max_players` |
| `n(3) bb_seat` | `u8` | `< max_players`, must be an occupied seat; it need **not** be in `dealt_in`, because a seat that posts dead money still posts the big blind (D-005). **The qualifier `non-absent` is deleted (J5)**: nothing sets that status, so the qualifier constrained nothing; which occupied seat the button rule selects is `STATE_MACHINE.md`'s (D-011 rule 1) |
| `n(4) level` | `u16` | |
| `n(5) small_blind` | `u64` | |
| `n(6) big_blind` | `u64` | `== 2 * small_blind` |
| `n(7) ante` | `u64` | `0` in version 1 |
| `n(8) dealt_in` | `Vec<u8>` | ≤ `MAX_SEATS`, ascending, unique; the cryptographic parties to this hand. **`dealt_in` is a subset of `P(k-1)`, always** — see below |
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

**`dealt_in` is a subset of `P(k-1)`, and that containment is normative.** Without
it the narrowing of `HAND_INIT`'s emitter set buys nothing: every collective stage
after it whose `R` is `dealt_in` — `DECK_INIT`, `DECK_COMMIT`, `DEAL_PRIVATE`,
`BOARD_REVEAL` — could still require a contribution from a seat `HAND_INIT` did
not, and the table would stall at `DECK_INIT` instead of at `HAND_INIT` with
nothing else changed. **The containment is the whole of what this document says
about it**: the full derivation of `dealt_in` — which seats a status, a stack or an
accepted seat event removes on top of the containment — is `STATE_MACHINE.md` §5.3
step 4's and its I31(b)'s, and is not restated here (D-011 rule 1). Every term of
that derivation is a function of accepted chained content, which is what makes the
receiver check below implementable: every peer recomputes the vector and rejects a
copy that disagrees.

**The clause this replaces read "`dealt_in` excludes absent and sitting-out seats
per D-005"** and rested half on a status nothing sets (J5). The sitting-out half
survives, in the form that names the chained event rather than the status it
produces; the absent half is gone with the status.

**D-051 (2026-09-13).** A seat that a certificate names with `cause = 1` -- every voter's own
client cut it off for flooding the table's carrier group, and the subject digest commits to
it -- is out of the table for good at that hand's boundary in the way D-047's paragraph below
states: its chips leave the table, and every client removes it from the carrier group for good.
The flooding seats' own votes are never needed, because they are the seats named; one voter's
cause alone completes nothing, because a vote with a cause and a vote without one are about two
subjects.

**D-047 (2026-09-12).** A seat certified absent that has already come back `MAX_RETURNS`
times is out of the table for good at that hand's boundary: its chips leave the table
(`stack_at_hand_start` is zero from hand `k+1`), so every rule in this section treats it
as busted -- not dealt in, no blinds, no return -- and every client removes it from the
table's group for good (D-045). Every seat derives the same from the same certificates;
nothing new is said on the wire. The paragraph below is the dead seat short of that limit.

A seat outside `dealt_in` keeps its stack, pays its blinds and antes as dead money,
takes no cards, and is not a party to the cryptography. That is D-005 and it is
unchanged. It cannot win the blind it posts — a documented, forced deviation from
TDA rules, because any workaround is precisely the collude-and-disconnect attack
`SPEC_CS.md` §19 forbids. Because the blinds keep eating it, it drains and
**genuinely busts**, which is what lets a tournament reach an end condition — the
thing the deleted status-based emitter set made impossible, since §4.10 restores
every stack on every abort and a table that only ever aborts never busts
anybody.

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
| `n(1) input_deck_hash` | `bytes[32]` | `h("p2p-poker v1 deck-commit", [input deck bytes])`; for round 0 see below |
| `n(2) output_deck_hash` | `bytes[32]` | over the preceding `SHUFFLE_STEP`'s deck |
| `n(3) proof` | `bytes` | ≤ 8192 B; 5547 B for a 52-card Bayer–Groth proof |

Round 0 is the exception: its input is the **open deck**, which is a library
constant and never travels, so `input_deck_hash` is the fixed value
`h("p2p-poker v1 deck-commit", ["the open deck"])`. It binds nothing, and it is
not supposed to: the open deck is identical in every hand at every table, so a
round-0 proof is bound to its hand entirely by the context, which carries the
table, the session, the hand, the sequence and the shuffler's own key. The field
is present at round 0 only so that one decoder reads every link.

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
six-handed, at an assumed 100 ms RTT.** **Two-network runs have since been made and this
sentence used to deny it** — `NEXT.md`'s *“The two-network run's real finding”*
carries them, and what they found was not a latency figure but that a relayed
hand dies at 128 KB, so the estimate above still stands unmeasured while the
claim that nothing had been tried does not. The figure is one of `SPEC_CS.md`
§33's seven profiling targets, and `CRYPTOGRAPHY.md` §6.5 holds the target table
and names Phase 8 as where it is measured. The estimate must also be revised upward for
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

**`ctx` and J1.** D-013 removes `advert_hash` from `GENESIS(0)`, `session_id` **and
`ctx`**. This block needs no edit for the third of those, and that is the point:
`ctx` never named `advert_hash` directly, it names `session_id`, and §4.3 has
removed it there. With that removal every one of `ctx`'s seven parts is a protocol
constant or a value fixed by accepted chained content — which is the condition
`CRYPTOGRAPHY.md`'s discipline item 1 states for the Fiat–Shamir binding, and which
this construction previously failed transitively without either document knowing
the path existed.

**This is the normative construction for the whole corpus** and it supersedes
`CRYPTOGRAPHY.md` §6.4's raw-concatenation form, which is deleted (C-2). Hashing
rather than concatenating is what makes the input fixed-length and reuses the one
audited length-prefixed, domain-separated hasher; `ziffle` hashes whatever it is
given (`SHA-256(ctx)`), so a 32-byte input loses nothing. `CRYPTOGRAPHY.md` §6.4
gives the reasoning about **what** `ctx` must bind and points here for the
construction; it reproduces no part of this block. The sentence that stood here
said it "reproduces this block for the reader", which was true until the D-011
sweep deleted both `ctx` blocks from that document (J6). The class is worth naming
because D-011 does not: **when a copy is deleted, the owner's sentence describing
the copy is the second edit.**

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

**`role_code` and `owner_seat_or_0xFF`, normatively.** This block used `role_code`
for seven passes without ever saying what the numbers are, which is the same
defect the burn-card paragraph below describes and with the same consequence:
two conforming clients that each pick a reasonable encoding produce a different
`index_map_hash` for an identical table, hence a guaranteed `DECK_COMMIT`
mismatch every hand, hence a manufactured section 15 dispute that after section
6.3 faults the table. The gap survived because the construction *reads* complete
- the term looks like it is defined elsewhere, and no document defines it.

| Role | `role_code` |
|---|---|
| reserved, never emitted | 0 |
| first hole card | 1 |
| second hole card | 2 |
| flop | 3 |
| turn | 4 |
| river | 5 |

Codes start at 1 so that a zeroed buffer is not a valid map. `role_code` has no
value for an unused index, and needs none: the product runs over `0..2m+5` and an
unused index is never hashed.

`owner_seat_or_0xFF(i)` is the **seat index** - the seat's position at the table,
not its position `d_j` in deal order - for the two hole-card ranges, and `0xFF`
for the five board indices.

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
| `n(0) street` | `u16` | `3` flop, `4` turn, `5` river — §4.7's street code, which owns the encoding |
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

**When a muck is sent is the sender's own affair, inside the stage's budget
(D-050).** A client whose hand may muck may wait for its player, who may show
the hand instead, for at most 3 000 ms, cut so that `SHOW_MARGIN_MS` = 8 000 ms
of the showdown stage's budget is left to the seats behind it; below 2 000 ms it
does not wait. Receivers see only the one message
that ends the wait, and nothing about it is legal that was not legal before.

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
| `n(0) street` | `u16` | the street code below; must equal the engine's current street |
| `n(1) seat` | `u8` | must equal `player_to_act` and the sender's seat |
| `n(2) action_index` | `u32` | count of actions so far in this hand; must equal the engine's |

**`street`, normatively.** The code is **the number of board cards the street
has**, and this section owns it:

| Street | `street` |
|---|---|
| pre-flop | `0` |
| flop | `3` |
| turn | `4` |
| river | `5` |

Every message in this document with a `street` field uses this table and no
other: `ACTION_*` above, `BOARD_REVEAL` (§4.6), and `street` inside
`PublicTableState` (§6.1).

This block exists because the encoding was **not stated anywhere** for seven
passes, and that is the same defect this document records against `role_code` in
§4.5, with the same consequence: two conforming clients each pick a reasonable
enumeration — `0,1,2,3` from the engine's own `Street` discriminants is the
obvious other one — and every action of every hand mismatches at `n(0)`. It was
inferrable, from `BOARD_REVEAL`'s `3` flop / `4` turn / `5` river and from
`PublicTableState`'s `board` being *"length 0/3/4/5"*, and inferrable is not
stated. **`0,1,2,3` is the withdrawn reading and appears nowhere in this corpus
except in this paragraph, which records its withdrawal.**

The count was chosen over the discriminants because it is the one encoding a
reader can check against the message it travels with: a `BOARD_REVEAL` carrying
`street = 3` carries three entries, and a client that got the mapping wrong
fails a length check in the same message rather than silently agreeing to
different streets.

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
`docs/research/POKER_RULES.md` A3, A4 and A5, in particular

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

> **Normative — this whole group is defined but not produced in version 1
> (D-015).** Both message types below keep their fields, their code points,
> their `event_class` values and their place in §5.2.1's slot key, and **no
> conforming client of this version emits either one**. A receiver drops both at
> §4.0 step 6, as the header box states, and records no fault for it. Everything
> from here to the end of §4.8 is the definition a later version implements, and
> the conditions in it are the conditions that version must satisfy — read them
> as the specification of a message, never as a description of traffic this
> version carries. §8.3 and §8.4 carry the same mark; §8.1's two-case table is
> what governs a stalled stage in this version, and its two answers are the
> seat's own auto check/fold and `hand_deadline_ms`.

---

**`0x0601 TIMEOUT_VOTE`** — *defined, not produced in version 1 (D-015)*

*Direction:* any seat in the required voter set → all.
*Legal:* only when the local monotonic timer for the subject stage has expired
(§8.2) -- at once for a seat gone by its own word, below -- **and** this client has
not accepted a valid event for that stage **from `subject_seat`**.

**`S1-GK`: at once, for a seat gone by its own word.** Once a client has accepted a
seat's own signed `TABLE_LEAVE` (§7.10) at a set table, and that seat's client has
left the table's group or has not been heard there for eight seconds since, the
client's timer for that seat has expired at every cryptographic step and every
turn, and the first hand's allowance for seats joining the group does not hold it
(`S1-GN`): nobody waits out the clock of a player who said it is gone. A seat heard
in the group again has its clock again, and a word whose seat is still heard there
thirty seconds on is forgotten. A receiver still checks
nothing about when a vote was cast, and a certificate still needs a vote from every
seat of `V(S)`: a voter that did not hear the word votes at the deadline, and the
seat is out when the last voter says so.

**D-059: a shorter timer, at a cryptographic step, for a seat that has made the
table wait.** Each client counts, on its own clock and once each stall is over,
every stage that stood on a seat for ten seconds and every turn of that seat
that ran past its deadline -- none during which it heard none of the other seats
for fourteen seconds, which is its own line gone -- and times its vote about the
seat at a cryptographic step to the step's budget less ten seconds for every
such wait counted before: at most three, and never less than ten seconds. A
turn's timer is never shortened. The
vote still names the parent's `next_deadline_ms` in `deadline_ms`; a receiver
checks that value and nothing about when the vote was cast, and a certificate
needs a vote from every seat of `V(S)`, so a seat is voted out when the most
patient of them says so. Nothing on the wire changes.

**`subject_event_type` for a betting stage, normatively.** A cryptographic
stage has one type and every voter names it. A **betting** stage has five —
`ACTION_CHECK` through `ACTION_FOLD` are all legal at one `sequence`, and which
one the seat would have chosen is precisely what nobody knows, because it never
spoke. So the value is the **group base `0x0500`**, which names the group and
commits to no member of it.

It has to be pinned, and this is the same defect this document records against
`role_code` in §4.5 and the street code in §4.7, with a worse consequence: the
field is inside `subject_digest`, so two conforming clients that each picked a
reasonable member — `ACTION_FOLD` because that is the effect, `ACTION_CHECK`
because it is the first — produce different digests, their votes land in
different slots, **no certificate ever assembles, and the whole path is
unreachable with nothing to attribute the failure to**. A gate that cannot be
reached is not a gate.

`0x0500` is chosen over any member because it is the one value that is a
function of the stage rather than of a guess about the seat, and over a fresh
sentinel because the group base already exists and means exactly this.

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
| `n(2) subject_event_type` | `u16` | what was expected from that seat; the stage's own type, or `0x0500` for a betting stage — see below |
| `n(3) parent_event_hash` | `bytes[32]` | `stage_hash(subject_sequence - 1)` |
| `n(4) deadline_ms` | `u32` | the `next_deadline_ms` carried by the parent stage's events |
| `n(5) kind` | `u16` | `1` = action deadline, `2` = cryptographic-step deadline |
| `n(6) cause` | `Option<u16>` | **D-051.** Absent: the deadline passed, and nothing more is said. `1`: and the voter's own client cut the seat off for flooding the table's carrier group. `2` (**D-065**): the seat is not one the stage waits on but a voter of the round at that stage that has said nothing about it within the round's air -- no vote about every seat the voter voted about there, or no copy of the certificate the voter sealed. `3` (**D-066**): the seat, one the stage waits on, has been out of the table's group or silent there for `LONG_GONE_S` or more by the voter's own reading. Any other value, `0` included, is refused. Absent it is not encoded, so a vote without a cause is byte for byte a vote without the field; present, it is part of the subject (`subject_digest` below) |

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
`n(6) cause` (D-051) is fixed by the voter with its first vote about that seat at
that stage: a client votes once about one seat at one stage, and a seat it cut off
after voting is voted about with the cause at its next stage.

**`cause = 2` and when it is said (D-065).** A voter votes about another voter of
the round at a stage with `cause = 2` one stage deadline after its own last vote
there about a seat the stage waits on, and after the copy of the certificate it
sealed there if it sealed one -- and at once where its own reading has that voter
out of the table's group or silent there for `QUIET_LIMIT_S`. It does so only
while the certificate naming the seats it voted about and that voter together
would clear §8.3's floor, and never about a seat the stage waits on: that seat is
voted about with the ordinary vote. The fields `n(0)`..`n(5)` are the stage's own,
exactly as for the seats the stage waits on, so the one certificate carries both.

**`cause = 3` and when it is said (D-066).** A voter votes about a seat the stage
waits on with `cause = 3` at once -- without waiting out the stage's deadline -- when
its own reading has that seat out of the table's group, or silent there, for
`LONG_GONE_S` = 300 s or more, counted only while the voter's own line is sound --
it hears some other seat of the table, and its library is on the network, since two
seats that lost their line together still hear each other over their own LAN. A
voter that voted about the seat at that stage without the cause votes again with it:
another subject in another slot (§5.2), and the one exception to *once about one seat
at one stage* -- a turn whose stage stands until `hand_deadline_ms` has no next stage
to carry the cause.

**A vote is also a question (D-065).** A `TIMEOUT_VOTE` about seat `s` at stage
`x` says its voter has accepted nothing from `s` there, which a receiver holding
that event can answer by sending it again under its author's signature -- an
honest peer's events need no other authority, and a voter whose vote split from
the table's because it lacked an event is answered with the event rather than
certified. One receiver answers: the lowest dealt-in seat that is neither `s`, nor
out of the table's group, nor heard voting the same; the next takes the question
up when that one holds nothing. A vote about the receiver itself is answered with
the receiver's own events of that stage, and one with `cause = 2` about it with
its own votes and copies there and every event of that stage it holds -- what it
did not vote about may be what it has. Once per hand, stage, seat and cause at
each receiver. Answering is never a precondition of anything: a receiver that
answers nothing breaks no rule, and the vote is judged as before.

**The question may come early.** A voter may cast its vote about seat `s` at stage
`x` before `x`'s deadline -- `QUESTION_AFTER_MS`, five seconds, into the stage --
when it holds an event of a later stage of this hand signed by a seat other than
`s` and itself. That seat moved past `x`, which no seat does without `s`'s event of
`x` (a collective stage needs every event, a single-writer one builds on the last),
so it will never vote about `s` at `x` and the early vote can complete no
certificate: it is a question and nothing else, and the answer comes seconds after
the loss rather than a deadline after it.
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

**`0x0602 TIMEOUT_CERT`** — *defined, not produced in version 1 (D-015)*

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
| `n(0) subject_digest` | `bytes[32]` | `h("p2p-poker v1 timeout-cert", [u64_be(subject_sequence), bytes(subject_seats), u16_be(subject_event_type), parent_event_hash, u32_be(deadline_ms), u16_be(kind)])` — `subject_seats` the seats named, ascending, one byte each; for one seat this is `u8(subject_seat)` as before (D-036) — and, only when a seat is named with a cause (D-051), one more part: every named seat's `cause` as `u16_be`, zero for none, in the seats' order. A subject with no cause hashes exactly as before |
| `n(1) votes` | `Vec<bytes>` | for every seat named, ascending, one complete `SignedEvent` of a `TIMEOUT_VOTE` about it from every seat of `V(S)`, ascending by voter; ≤ `MAX_SEATS² / 4` entries (D-036: `\|V(S)\| > \|S\|` and `\|S\| + \|V(S)\| <= MAX_SEATS`; D-063: `\|S\| · \|V(S)\|` with `\|S\| + \|V(S)\| <= MAX_SEATS`, the same bound) |
| `n(2) resignations` | `Vec<bytes>` | `D-063`: for every seat of `S` whose player left the table by its own signed word, that `TABLE_LEAVE` (§7.10) as the complete `SignedEvent`, ascending by seat; empty when none did; ≤ `MAX_SEATS` entries |

*Receiver must validate:* every embedded vote independently passes §4.0 steps
2–11; all votes are about one stage under one deadline, and the seats they name
are `S`, the set the certificate is about (D-036); every seat of `S` has a vote
from one and the same voter set, no seat of `S` is in it, and that set is exactly
`V(S)` as §8.3 defines it, with no duplicates — **every seat absent from it is
absent because it is named by this certificate or by a completed, valid
certificate earlier in this hand** — a `V` shrunk by bare votes is not a `V`;
every resignation is a `TABLE_LEAVE` for this table that verifies under the key
of a seat of `S`, one per seat (`D-063`); `|V(S)| >= 1`, and with `Q` the seats of
`S` carrying no resignation, either `Q` is empty or `|V(S)| >= 2` and
`|V(S)| > |Q|` — a seat that said it left counts for nothing against the floor,
its own word being its consent — or, `D-066`, `|V(S)| >= 2` and either
`|V(S)| = |Q|` with the lowest seat of `V(S) ∪ Q` in `V(S)`, or every seat of `Q`
not named `cause = 2` named `cause = 3`; a receiver that is itself a seat of `Q`
named without `cause = 2` does not take a certificate that clears the floor only by
`D-066` while its own line was sound for the last `LONG_GONE_S` — it was here, which
such a certificate may not overrule — and takes it when its line was down within
that time, since then it may really have been gone; every certificate names at least one seat
without `cause = 2` -- a seat its stage waits on -- and a `kind = 1` certificate
names exactly one, the seat to act, with any voters named `cause = 2` beside it
(D-065); every vote about one seat names one `cause`, and a defined one (D-051);
`subject_digest` recomputes; the emitter is itself a member of `V(S)`.

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

**`0x0603 RETURN_VOTE`** — *the grow side of the roster (D-028, `S1-BM`)*

*Direction:* single-writer, one per voter per subject. The voters are
`R(k) \ OUT(k)` — the roster of hand `k` less the subjects of complete
`TIMEOUT_CERT`s of hand `k` — taken **before** any return is added
(`READMISSION.md` §5, correction 1), and written without `dealt_in` or `grace`,
which are per-receiver.
*Legal:* only at the boundary of a hand that **settled**: `TERMINAL(k)` is the
`HAND_COMPLETE` stage hash (correction 2 — a receiver that reached the
settlement by §4.10's late road holds no checkpoint of its own and is a party to
this all the same); only about a seat **outside** `R(k)` that occupies a seat of
the table with chips; only when the voter holds the subject's signed
`PLAYER_SIT_IN` of this boundary and the subject's signed checkpoint-8
`STATE_HASH` whose value is the voter's own. A vote does nothing alone.
*Envelope:* `chain_scope = 1`, `event_class = 1`, `hand_id = k`,
**`sequence = RETURN_SEQUENCE_BASE + subject_seat`** and
**`previous_event_hash = TERMINAL(k)`** — the subject's slot on the terminal,
for every voter, so a vote sealed anywhere else is not a vote about this
boundary at any receiver. The band `8 224 … 8 233` is disjoint from the stages,
from §4.10's window and from §4.9's checkpoint band (§13).

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) subject_seat` | `u8` | `< MAX_SEATS`, outside `R(k)`, not the voter |
| `n(1) terminal` | `bytes[32]` | `TERMINAL(k)`, equal to the envelope's parent |
| `n(2) request_hash` | `bytes[32]` | `event_hash` of the subject's `PLAYER_SIT_IN` at this boundary |
| `n(3) state_hash` | `bytes[32]` | the subject's checkpoint-8 value, which the voter attests is its own |

*Receiver must validate:* the envelope binding above; `subject_seat` is not the
sender's seat; the sender is in `R(k) \ OUT(k)`; the subject is outside `R(k)`;
`terminal` is this receiver's **settled** terminal — held while this receiver
has no terminal yet or a different settlement, refused when its terminal is an
abort's. Payload cap `RETURN_VOTE_CAP = 160`.

**`0x0604 RETURN_CERT`** — *collective, the shape of `0x0602` (D-028)*

*Direction:* **collective stage**; the required emitter set is the voter set
above, each voter emitting its own copy from the votes it holds — never from a
peer's certificate.
*Legal:* only when a `RETURN_VOTE` about one subject has been collected from
**every** voter, and `|voters| >= 2`. The floor is met heads-up: the subject is
outside `R(k)`, so removing it removes nothing (correction 3).
*Envelope:* `chain_scope = 1`, **`event_class = 2`**, the same `sequence` and
parent as the votes it carries.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) subject_digest` | `bytes[32]` | `h("p2p-poker v1 return-cert", [u8(subject_seat), terminal, request_hash, state_hash])` |
| `n(1) votes` | `Vec<bytes>` | 2 … `MAX_SEATS - 1` entries, each a complete `SignedEvent` of a `RETURN_VOTE`, ascending by voter seat, all about one subject |
| `n(2) request` | `bytes` | the subject's complete signed `PLAYER_SIT_IN` of this boundary |
| `n(3) checkpoint` | `bytes` | the subject's complete signed checkpoint-8 `STATE_HASH` of hand `k` |

*Receiver must validate:* every carried vote independently, as above; one
subject; no voter twice; the certificate sealed at the slot its votes name;
`subject_digest` recomputes; **the carried evidence opens as the subject's
own** — the request at the subject's window slot on `terminal` with the
canonical empty payload, the checkpoint at round 0's hash slot on `terminal`
naming checkpoint 8 with `transcript_head = terminal`, both signed by the key
the roster holds for the seat — and it is what the votes name; the emitter is a
voter; the voter set is exactly `R(k) \ OUT(k)` as this receiver derives it, a
strict subset refused and a superset held; and where this receiver holds a
checkpoint-8 value of its own it equals `state_hash`, while a receiver with
none accepts the voters' unanimous word (correction 2). Payload cap
`RETURN_CERT_CAP = 8 192`. **Effect:** the subject enters `IN(k)`, and
`R(k+1) = ((R(k) \ OUT(k)) ∪ IN(k)) ∩ ALIVE(k+1)` (§8.3.1). Banked once per
subject digest; a redelivery is inert.

### 4.9 Group 7 — synchronisation and disputes

Codes `0x0700`–`0x07FF`. Semantics in §6.

**`0x0701 STATE_HASH`** — collective stage. `n(0) checkpoint: u16`,
`n(1) state_hash: bytes[32]`, `n(2) transcript_head: bytes[32]`. `transcript_head`
here is the `stage_hash` of the hand's last completed stage at the checkpoint -- at
checkpoint 8, `TERMINAL(k)`, the `HAND_COMPLETE` stage hash on the settled path --
and every reconciliation round of the checkpoint (§6.3) carries the same value. It
is **one stage later** than §6.1's field of the same name inside `state_hash`, which
`HAND_COMPLETE`'s own body carries at checkpoint 8 and which therefore cannot hold
that stage's hash (`S1-CJ`).

**Its required emitter set is `P(k-1)`, the same set as `HAND_INIT`'s** (§3.2,
§4.4), at checkpoints `2` to `7` of hand `k`; at checkpoint 1, which sits in the
setup chain, it is `P(0)`, the `TABLE_READY` signers; and at **checkpoint 8**, the
boundary checkpoint, it is `P(k)` — the box below states that exception, states
why the set has to be the later one, and states the wider set the checkpoint-8
stage is *compared* over. `STATE_ACK`'s set is the set of the
`STATE_HASH` stage it confirms, and a reconciliation round's is the set of the
checkpoint it re-derives **enlarged by this receiver's contradiction set** — which
the box below states in full, together with the floor that enlargement exists to
put under it. **The phrase "all
present seats", which stood in §4.11's cells for both types, is deleted: "present"
is a status word, and §3.2 forbids a required emitter set defined on one** (J2,
D-013).

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
> **Its required emitter set is `R(c) ∪ W`**, where `R(c)` is the required
> emitter set of the checkpoint being re-derived and `W` is this receiver's
> **contradiction set** for that checkpoint: every seat other than this receiver
> whose accepted event put this receiver into §6.3 step 1 — the signer of a
> `state_hash` at checkpoint `c` that differs from this receiver's, whether it
> arrived as a chained event or as the evidence of a `DISPUTE { kind = 1 }`
> (§6.3 step 2), and the sender of any event that reached §4.0 step 12a.
> Successive rounds take successive `sequence` values, so a peer that reconciles
> twice occupies two slots and never two bodies in one; `W` is the union over
> every round of the dispute and never shrinks. `STATE_ACK` follows a
> reconciliation round exactly as it follows a checkpoint, over the same set.
>
> **`W` is never empty, so `|R(c) ∪ W| >= 2`, and no peer completes a
> reconciliation stage alone.** Every entry condition of §6.3 step 1 names a seat
> other than the receiver: two distinct values at one checkpoint are two seats'
> signatures and at most one of them is this peer's, and the solitary-stage rule
> fires on an event from a seat outside `P(k-1)`, which always contains this peer
> (§3.2's own-emission rule). **This is normative and it is a general rule rather
> than a special case: a stage whose purpose is to detect a fork may not have a
> required emitter set that the forked peer can satisfy alone.**

**Why the set is not simply the checkpoint's, which is what this box said until
this pass and which was `N1`.** Read as *the emitter set of that checkpoint*, the
reconciliation stage of the **boundary** checkpoint is required of `P(k)` — and a
peer in the solitary regime has `P(k) = {self}`. It therefore completed the stage
in the same step it emitted into it, carrying one value that agreed with itself,
and `STATE_MACHINE.md` T53 — *"complete over its required signer set ∧ every value
in it agrees"* — released the freeze §3.2 had installed microseconds earlier,
without any other seat having spoken, and cleared the latch that §9.3's end
condition reads. **A freeze a peer can release by talking to itself is not a
freeze**, and every disposition in this document that routes a disagreement onto
§6.3 was routing it into that hole. Under `R(c) ∪ W` the solitary peer's stage is
required of `{self}` together with the seat that contradicted it, and completes
only when that seat publishes its own re-derived value — which is the one event
that can settle the question and is exactly what the stage exists to collect.

**`W` is a per-receiver quantity, it is the only required emitter set in this
document that is one, and it is admissible for a reason D-012 does not reach.**
D-012 forbids deriving **canonical state** from a per-receiver quantity; `W`
derives no state. It only ever **enlarges** a required set, so it can delay a
completion and never manufacture one: a peer with a larger `W` waits longer, and
no peer's `W` shrinks another peer's set or completes another peer's stage. The
procedure it belongs to is unilateral and per-receiver already and says so —
§6.3 step 1's freeze *"is unilateral and complete on its own"* — and an agreed `W`
is unobtainable here for the reason §3.2 gives about `P` itself: the peers are in
this procedure precisely because they do not agree about who is participating.
What the stage produces — one reconciled `state_hash`, or two — is still a chained
collective stage and is still read identically at every peer.

**The healthy table pays nothing for this, and that is checked rather than
assumed.** Wherever `P` has not forked, the seat whose value differed is already a
required emitter of the checkpoint, so `W ⊆ R(c)` and `R(c) ∪ W = R(c)` — the set
this box carried before this pass, unchanged, on every ordinary divergence at
checkpoints 1–7 and on every boundary checkpoint of a table that is talking. The
union is larger only when the contradicting seat is one this receiver had excluded
from the hand, which is the forked case and the only case the floor exists for.

**What this newly makes load-bearing: §6.3 step 2's dispute is now a liveness
precondition and not only a disclosure.** A peer whose own copies all agreed never
observed the divergence and would never emit into the reconciliation stage; what
brings it in is the `DISPUTE { kind = 1 }` every frozen peer is **obliged** to
broadcast, whose evidence is that peer's own `STATE_HASH` and which therefore puts
a second value at that checkpoint in front of every recipient. That obligation is
already normative and already mandatory-rather-than-optional in §6.3 step 2; what
is new is that the stage cannot complete without it, so a peer that withholds it
stalls the reconciliation rather than merely staying quiet — and the stall is
disposed of by §8 like any other, with the freeze holding throughout.

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

---

#### Checkpoint 8, the boundary checkpoint — normative, and this is the wire half of `K-3` (L1, L3)

`STATE_MACHINE.md` §5.2 places **checkpoint `8`** at every hand boundary, on every
hand and on both terminal paths, emitting it the moment `TERMINAL(k)` is fixed at
this peer — as a side effect of T45 on the settled path and of T46 on the aborted
one — and gates T47 on it on the settled path only. §6.2 row 8 carries it. It
arrived here with **no chain position at all**, which is `Q-09` a second time on a
second message, and on the message K-3's own fix newly made load-bearing:
`hand_id` and `sequence` are inside `TO_BE_SIGNED` (§2.4) and are two of §5.2.1's
eight slot-key components that §4.0 step 10a reads whole, so two implementations
that place this stage differently produce **different signed bytes for one intent**
and cannot look each other's copies up. This box gives it the same quantities
§4.10's box gave the hand boundary window, in the same shape and for the same
reason.

> **Chain.** A checkpoint-8 `STATE_HASH`, its `STATE_ACK`, and every
> reconciliation round of it belong to **chain `k`**, the hand that has just
> ended, and carry `hand_id = k`. Not chain `k+1`: what they publish is hand `k`'s
> boundary state including `P(k)`, and `HAND_INIT(k+1)` — chain `k+1`'s stage 0 —
> derives its required emitter set and its `n(8) dealt_in` from `P(k)`. A
> checkpoint placed in chain `k+1` would be chained from a `GENESIS(k+1)` whose
> own first stage is a consumer of the value the checkpoint exists to compare, and
> it would sit **after** the event it is meant to be checked before.
>
> **Parent.** `previous_event_hash = TERMINAL(k)`, for every emitter and whatever
> the order of arrival — `stage_hash` of the `HAND_COMPLETE` stage on the settled
> path, `ABORT_TERMINAL(k)` on the aborted path (§3.1). Both are agreed by every
> peer by construction, and on the aborted path there is **no other value in
> existence**, because the terminal `HAND_ABORT` has no `stage_hash` at all
> (§3.2). Equivalently, §3.2's rule extended a second time:
> `stage_hash(BOUNDARY_CHECKPOINT_BASE - 1) := TERMINAL(k)`, beside
> `stage_hash(BOUNDARY_SEQUENCE_BASE - 1) := TERMINAL(k)` and
> `stage_hash(-1) = GENESIS(hand_id)`.
>
> **`sequence`.** The `STATE_HASH` stage is at `sequence =
> BOUNDARY_CHECKPOINT_BASE` and its `STATE_ACK` stage at
> `BOUNDARY_CHECKPOINT_BASE + 1`, where `BOUNDARY_CHECKPOINT_BASE = 8 192` (§13).
> Reconciliation round `r` of this checkpoint (the reconciliation box above) takes
> `BOUNDARY_CHECKPOINT_BASE + 2r` for its `STATE_HASH` and `+ 2r + 1` for its
> `STATE_ACK`, the pair kept together so a round's acknowledgement can never land
> on the next round's hash. **`1 <= r <= 7`**: the band is the sixteen values
> `8 192 … 8 207`, and a checkpoint-8 event at a `sequence` outside the band is a
> stage violation at §4.0 step 12.
>
> **Why the base is above the boundary window and not below it.** Every value in
> the band is disjoint from every ordinary stage index, which
> `MAX_STAGES_PER_HAND = 2 048` bounds at 2 047, and from the hand boundary window
> at `4 096 … 4 105` (§4.10, §13). A reconciliation round extends **upwards**, by
> `+2r`, so a base below the window would let a disputed boundary checkpoint walk
> into the window's reserved slots and collide with a seat's `PLAYER_SIT_IN`. That
> is the constraint §5.2.1's key imposes on this fix and it is why the number is
> 8 192.
>
> **Total order.** One stage, one `sequence`, one copy per seat; §3.2's collective
> `stage_hash` enumerates `R` in ascending seat index, so no tie-break is needed
> and none is defined. The `STATE_ACK` stage follows at the next `sequence` and
> never precedes it.
>
> **Required emitter set.** `P(k)` **as it stands at the moment `TERMINAL(k)` is
> fixed at this peer** — the seats that signed at least one accepted chained event
> of chain `k` (§3.2) up to that point. The snapshot is what removes the
> circularity: a checkpoint-8 `STATE_HASH` is itself a chain-`k` event and adds its
> sender to `P(k)`, so a set read after the stage opened would grow with its own
> contributions and never be complete. A seat that joins `P(k)` by an out-of-set
> copy is in `P(k)` for hand `k+1` and is **not** retroactively a required emitter
> here. **This is the one required emitter set in
> this document that is `P(k)` rather than `P(k-1)` or a subset of it, and §3.2's
> sentence is amended to name it as the exception.** It is the stage whose whole
> job is to publish what `P(k)` is, so requiring it of `P(k-1)` would compare the
> boundary against the set the boundary replaces. Liveness is unaffected: every
> member of `P(k)` demonstrated itself by signing during hand `k`, and the exit
> when one of them stops between `HAND_COMPLETE` and here is a **timer**, T61, on
> hand `k+1`'s `hand_deadline_ms`, which §8.2 already starts at `TERMINAL(k)` — no
> new timer and no new message. `STATE_ACK`'s set is that same set, as at every
> checkpoint.
>
> **The comparison is not scoped on that set — this is the disposition of `L3`.**
> A checkpoint-8 `STATE_HASH` naming `hand_id = k` is **accepted, compared and
> retained from any occupied roster seat**, in `P(k)` or not. An out-of-set copy
> is **not** a stage violation and §4.0 step 12 must not reject it. It cannot
> complete the stage and cannot block it: completion is `heard ⊇ P(k)` and nothing
> else, so an extra copy neither advances the gate T47 reads nor holds it open. It
> occupies its own slot — §5.2.1's key contains `sender_public_key`, so capacity
> is one per seat at this `sequence` — and the admission therefore adds at most
> `MAX_SEATS` slots per hand, which §5.3 counts.
>
> **`STATE_ACK` is not widened, and neither is any other checkpoint.** A
> `STATE_ACK` asserts that this peer saw the complete required set and that every
> member of it agreed; a seat outside the set has not seen the set and has nothing
> to assert. A checkpoint-8 `STATE_ACK` from a seat outside `P(k)`, and a
> `STATE_HASH` from a seat outside the required set at checkpoints `1`–`7`, are
> rejected under §4.0 step 12 like any other out-of-stage chained event.
>
> **Close, and it closes over the `STATE_HASH` copies only (`N6`, `N5`).** The
> checkpoint-8 window for hand `k` closes at this receiver's acceptance of a
> complete `HAND_INIT(k+1)` stage — the same condition, at the same event, that
> closes the hand boundary window (§4.10). What the close bounds is the admission
> of a **`STATE_HASH`** copy, because that is the only checkpoint-8 event that can
> grow `P(k)`. A checkpoint-8 `STATE_HASH` for hand `k` arriving after the close
> is a **stale-hand** event, and **§4.0 row 10b is the sole authority on what
> becomes of it** — its outcome column, *"freeze, compare, readmit, or drop"*, is
> the enumeration, and this box reproduces no part of it (D-011 rule 1). What
> matters here is only the consequence for the close: **whichever of those four
> dispositions applies, the copy is never applied and never enters a
> `stage_hash`**, so a `STATE_HASH` arriving after the close cannot grow `P(k)`,
> which is the whole of what the close bounds. The words *"disposes of it in
> three ways and no others"* stood here and enumerated three of the four,
> omitting the ordinary **drop** of an agreeing copy from a seat **inside** the
> recorded set — the §1.5 forwarding reorder this box's own `N6` argument is
> built on, and not an adversarial case at all. That was a restatement claiming
> exhaustiveness against its own owner (`G6-R5`), and it is replaced by the
> pointer rather than by a fourth item.
>
> **The checkpoint-8 `STATE_ACK` stage is not closed by that window — normative,
> and this is `N6`.** Every peer emits its checkpoint-8 `STATE_ACK` and its
> `HAND_INIT(k+1)` copy on the same trigger, and forwarding (§1.5) can deliver
> another peer's `HAND_INIT(k+1)` copy ahead of that peer's `STATE_ACK` whatever
> order the two were written in, so a window that closed on `HAND_INIT(k+1)`
> would drop the acknowledgement in the **ordinary, non-adversarial** case —
> leaving §6.2's *"definite, chained 'we all agreed here' point that a later
> dispute can name"* unplaced at every hand boundary, and D-014's tier-2
> precondition, *a completed `STATE_ACK` stage*, unsatisfiable there. **A
> checkpoint-8 `STATE_ACK` of chain `k` from a seat in `P(k)` is therefore
> accepted until `TERMINAL(k+1)` is fixed at this receiver**, and its slots are
> the one part of chain `k`'s anti-replay store that outlives the hand (§5.3).
> Admitting it late costs nothing that has to be argued: an out-of-set
> `STATE_ACK` is rejected — the paragraph above is unchanged — so an accepted one
> is always from a seat already in `P(k)`, it **grows no set**, it changes no
> body, and the only thing that reads the stage is a completion test.
>
> **Deferred readmission — normative, `N5`, and it widens the *accepted* set and
> never the *required* one (`P2`).** A stale chained event of
> a finished hand `k` that is either a `0x0804 PLAYER_SIT_IN` in chain `k`'s
> boundary window (§4.10) or a checkpoint-8 `STATE_HASH` of chain `k` **whose
> value equals this receiver's retained `checkpoint8_state_hash(k)`** adds its
> sender to a **readmission set `A`**, and does nothing else: it is not applied,
> it enters no `stage_hash`, it completes no stage, and it counts into no `P` of a
> hand this receiver has already initialised. `A` is read at exactly one place —
> the next hand init this receiver runs — and is cleared there. What it does
> there is **one thing and it is not what this box said until this pass**:
>
> > **`R(HAND_INIT, m+1)` is `P(m)`, unchanged and unenlarged. `A` widens the
> > *accepted emitter* set of that one stage to `P(m) ∪ A`.** A
> > `HAND_INIT(m+1)` copy from a seat in `A` is **accepted, retained and counted
> > into `P(m+1)` under §3.2** — §4.0 step 12 must not reject it — and it
> > **cannot complete the stage and cannot block it**, because completion is
> > `heard ⊇ P(m)` and nothing else. It is accepted until `TERMINAL(m+1)` is
> > fixed at this receiver, exactly as `N6` retains the checkpoint-8 `STATE_ACK`
> > band and for the same reason: the copy and the completion race each other
> > through the forwarding of §1.5 and neither order is a fault.
>
> `|A| <= MAX_SEATS`; it is one seat set, it is read once, and it is the whole
> mechanism.
>
> **Late roster repair — normative, the shrink-side twin of deferred readmission,
> read at the same place and closed by the same event (`S1-BS`, D-027).** A
> receiver that ended hand `k` by an **abort** keeps hand `k`'s record whole
> until `HAND_INIT(k+1)` completes here. A `TIMEOUT_CERT` of hand `k` arriving in
> that window is verified against hand `k`'s own roster and **banks its roster
> half** — its subject leaves `R(k+1)`, keyed on the subject digest — and does
> nothing else: it is not applied, it enters no `stage_hash`, it completes no
> stage. Every such bank **re-derives `Opening(k+1)`** from hand `k`, through the
> one derivation every seat runs; if the derived `GENESIS(k+1)` differs from the
> one this receiver opened hand `k+1` at, and hand `k+1` is still at sequence 0
> here, hand `k+1` is **re-opened** at the corrected opening: the same `hand_id`,
> the same readmission set `A`, the events it was holding carried across — the
> other seats' `HAND_INIT(k+1)` copies, held as *a different parent*, now count.
> The number of re-opens per hand is bounded by the number of distinct
> certificates about hand `k`, at most `|dealt_in(k)| - 2`.
>
> > **No seat signs one hand twice.** A `HAND_INIT` is sealed at
> > `(hand_id, sequence 0)`, and §5.2.1 keeps the parent out of the slot key on
> > purpose, so a second signed `HAND_INIT(k+1)` at a corrected parent would be
> > the equivocation §5.2.3 forbids of an honest peer. Therefore: a receiver
> > that already signed hand `k+1` re-opens it **muted** — its own copy is heard
> > in its own stage and never sent — follows the corrected hand silently, and
> > rejoins at hand `k+2`, whose genesis is a function of the corrected
> > `GENESIS(k+1)` and `R(k+1)` alone; and a receiver that, before it opens hand
> > `k+1`, already holds `HAND_INIT(k+1)` copies from at least two roster seats
> > at one other genesis and none at its own opens it **quietly** — its copy
> > heard, not sent — and sends it the moment as many seats are counted at its
> > genesis as at any other, or a certificate re-derives the hand, in which case
> > the re-open is its first and only signature. Hand `k+1` is lost on the muted
> > road; it was lost anyway, because no stage 0 completes without that seat.
>
> > **Never deal the next hand alone.** A receiver whose hand `k+1` ended at
> > sequence 0 while more roster seats had signed `HAND_INIT(k+1)` at one other
> > genesis than were counted at its own does not derive hand `k+2`; it waits,
> > a late certificate about hand `k` still repairs it, and the client's *a hand
> > ahead* latch ends the wait when the table is two hands on. Ties deal on.
> > This is a decision about waiting, read off signed envelope fields; it
> > derives nothing (D-012).
>
> **Why the required set is the wrong place to put it, and this is `P2`.** `A` is
> written by §4.0 step 10b, which runs on a **stale** event — one naming a hand
> this receiver has finished — and step 10a's anti-replay is *skipped* for exactly
> those events (`N2`). So one signed, perfectly **agreeing** checkpoint-8
> `STATE_HASH`, replayed by anybody, entered `A` again on every replay; `A` is
> cleared at every hand init; and a set that is cleared once per hand and refilled
> once per hand is not idempotent however idempotent each individual write is.
> Under the deleted rule that re-enlarged `R(HAND_INIT(m+1))` by a seat that is not
> there, **once per hand, at every receiver, for as long as §5.3 retains the
> record of hand `k`** — `MAX_RETAINED_HAND_RECORDS = 4 096` hands — and stage 0
> then stalled to the full hand deadline every time, with `cause = 1` and
> `attributed = []`. **No key is needed**: the event is one the accused seat
> genuinely signed, it agrees with this receiver's own value, and forwarding it is
> a thing §1.5 permits any peer to do.
>
> **What closes it is the signature, not a counter.** Under the rule above the
> only thing that can enlarge a required emitter set is a seat's **own accepted
> copy of `HAND_INIT(m+1)`**, whose `hand_id`, `sequence` and
> `previous_event_hash` are inside `TO_BE_SIGNED` (§2.4) and are bound to chain
> `m+1`. **No replay of any chain-`k` event can produce one**, so the amplifier
> has no input: replaying the stale copy now re-opens an acceptance that costs a
> map lookup and completes nothing, which is precisely the idempotence §4.0's
> skip argument claimed and, until this pass, did not have. It also restores that
> argument's own sentence to truth — *"a seat already in the readmission set is
> already in it"* — because entering `A` no longer has an effect that repeats.
>
> **What it costs, stated rather than glossed.** A seat readmitted by this route
> is **not `dealt_in` for hand `m+1`** — `dealt_in ⊆ R(HAND_INIT, m+1) = P(m)`
> (§4.4) — and becomes a required emitter, and dealable, at hand `m+2`, one hand
> later than the deleted rule promised. D-013's readmission promise is kept and
> the latency is one hand: what that promise owes is *a moment at which a
> returning seat can be heard*, and being accepted into `P(m+1)` is that moment.
> §4.10's *"a seat rejoins by signing a chained event … that is the entire test"*
> is satisfied literally rather than by a set this receiver enlarges on the
> seat's behalf.
>
> **And the race it does not enlarge.** Two peers can still disagree about
> `P(m+1)`, one having accepted the returning seat's copy and the other not, and
> hand `m+2`'s stage 0 then stalls — the same disagreement, on the same path, that
> the paragraph below already routes and bounds. What has changed is the entry
> condition: it now requires the returning seat to have actually signed into
> chain `m+1`, so the stall costs one hand for a seat that is really there,
> instead of one hand per hand for a seat that is not.
>
> **Why it is needed and why it is safe.** Without it, D-013's readmission promise
> is void in the regime D-013 itself created. Every window in this document closes
> at `HAND_INIT(k+1)`; a solitary peer's `HAND_INIT(k+1)` is required of `{self}`
> and self-completes; so **every window at a solitary peer is zero-width**, and a
> seat that comes back can never be heard by the peer that is draining it —
> §4.10's *"a seat rejoins by signing a chained event … that is the entire test"*
> would be a test with no moment at which it can be taken. It is safe because the
> set can only **grow an *accepted* emitter set**, at a stage-0 boundary, and an
> extra accepted copy neither completes a stage nor blocks one. **The argument
> that stood here was that it can only grow a *required* set, and that argument
> was the defect (`P2`)** — growth is monotone within one hand but the set is
> cleared at every hand init, so a replayable write into it is a stall renewable
> once per hand. What survives is the routing: a disagreement between two peers
> about who is in `P` is still resolved onto a **stalled stage 0** by §4.10's box
> and by this one — loud, disposed of by §8, and held back from becoming a silent
> fork by §3.2's solitary-stage rule — one hand later and only for a seat that
> really signed. That sentence used to continue *"and because agreement at
> checkpoint 8 is agreement about the participation set itself (§6.1's
> `signed_this_hand`), a seat readmitted by the second route has signed this
> receiver's own account of who was playing"* — and field 28 is deleted, so it
> no longer does. A seat readmitted by the second route has signed this
> receiver's account of the **settled state**: the transcript head, the deck
> commitment, the board and every final stack. Not of who was heard. The box
> above says what that trade buys and what it costs. **`A` does not thaw a freeze:**
> a latched solitary divergence is released by the reconciliation stage of the box
> above and by nothing else (§6.3 step 3).

**Why the accepted set is wider than the required set, and why this is the only
place in the document where the two differ.** Every other collective stage rejects
an out-of-set emitter because an out-of-set body could only be noise — the seat is
not entitled to the choice the stage carries, or is not a member of the set the
body is derived over. Checkpoint 8 is the exception because **its body is a claim
about who the participants are**, and the seat whose participation is in question
is exactly the seat whose copy the strict rule throws away. Two peers that have
each concluded the other is not a required emitter — K1's fixed point, where both
hold `P(k) = {self}` — would under the strict rule reject the one message that
proves the other is there. `STATE_MACHINE.md` §5.2 states the engine half in terms
(*"T49 and T50 are **not** scoped on that set … that is deliberate and it is the
whole value of the checkpoint"*); this box is the wire half, and without it that
sentence describes an event no conforming receiver accepts and both of K-3's
payoffs are void.

**What the admission does not buy, stated because that is the part that could be
over-claimed.** Accepting an out-of-set copy adds its sender to **this receiver's**
`P(k)` under §3.2, and `P(k)` is hand `k+1`'s required emitter set — so a copy
that arrives at one peer before its `HAND_INIT(k+1)` and at another peer after it
leaves two peers holding two `P(k)` values. That is a per-receiver quantity and it
is not removed here; it is **routed**, onto exactly the path §4.10 routes a
boundary-window disagreement onto. The two peers derive different `n(8) dealt_in`,
reject each other's `HAND_INIT(k+1)` copy, and **stage 0 stalls** — loud, disposed
of by §8, with both peers still holding the same `GENESIS(k+1)`. The close
condition above is what bounds the window in which that race can happen, and it is
the same condition as the boundary window's, deliberately, so an implementer has
one rule to write and not two. **A copy that arrives after the close does not
disappear and does not enlarge that race:** if it agrees it goes to the
readmission set and takes effect one hand later, as an **acceptance** at the next
hand init and never as an enlargement of its required set (`P2`), and the same
stall disposes of the same disagreement one hand further on; if it differs it is a
divergence.
Neither reaches a hand this receiver has already initialised, which is the one
thing the close was protecting.

**A checkpoint-8 `STATE_HASH` from a seat outside `P` is the second case exempt
from §3.2's solitary-stage rule.** A solitary peer that froze on this message
would fault the table on the one message that can tell it whether it is alone or
merely wrong — and it would fault it in **both** cases, including the case where
the two peers agree.

**The exemption used to rest on §6.1 hashing `signed_this_hand`, and that field
is deleted. What carries it now is weaker, and this paragraph says how much.**
The old argument was exact: agreement at checkpoint 8 *was* agreement about the
participation set, so two peers whose `P` had forked could not agree here. It
was also unusable — the field was a per-receiver quantity, so two *honest* peers
could not agree here either, and §6.1 records what that cost.

The twenty-eight remaining fields still make agreement hard to reach from a
forked chain rather than impossible to reach only from a matching one.
`transcript_head` is the `stage_hash` the settlement chains from; `roster`
carries every seat's final stack; `deck_commitment` and the board carry the
hand's cryptography; and `GENESIS(k)` commits to `participants` = `R(k)`
(§3.1), so a fork in the *required* set forks the chain outright and cannot
reach a common checkpoint at all. What is no longer separated is a fork in the
**accepted** set alone, where two peers agree about every quantity above and
differ only in whom they heard. That case now agrees at checkpoint 8 and admits
the out-of-set sender.

**Which is the intended behaviour, and the reason the trade is worth taking.**
That case is exactly the honest one: two peers who computed the same money and
heard different neighbours. Admitting it is the readmission this document wants
and the old rule refused — measured, readmissions went from zero in a
nine-minute run to twenty. What is lost is the ability to distinguish it from a
*dishonest* peer claiming an accepted set it did not have, and the cost of that
is bounded by what such a peer must still reproduce to be admitted: the
transcript head, the deck commitment, the board and every final stack, none
computable without having followed the chain it claims to have followed.

`Q-10` is not closed by this and is not made worse by it. What changed is that
the residual is now a quantity nobody compares, rather than one everybody
compares and honest peers fail.

**The exemption is from freezing on *arrival*, never from freezing on
*disagreement*, and reading it the other way was half of `N1`.** A checkpoint-8
`STATE_HASH` of hand `k` whose value **differs** from this receiver's own, where
the retained record (§5.3) says this receiver derived hand `k` with a required
emitter set of `{self}`, is a **solitary-regime divergence** and reaches §4.0
step 12a, not §6.3 step 1 alone. The argument is the one two paragraphs above,
used in the other direction: agreement here is agreement about the participation
set, so **disagreement here is the forked `P`**, reached by comparison instead of
by membership — the same finding, from the same evidence, and it must not carry a
weaker disposition merely because it arrived through the comparison. Specified
apart, the two routes had two memories: the membership route latched and the
comparison route did not, so on the interleaving where only the checkpoint-8 copy
arrives the peer froze, the hand timed out, the freeze was left behind by the
abort, and the table thawed and dealt again — every `hand_deadline_ms`, forever,
with every invariant passing. One rule, one disposition, one latch, whichever
route fired.

---

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
| `n(0) kind` | `u16` | the register below |
| `n(1) accused` | `Option<bytes[32]>` | app public key, when the kind names one |
| `n(2) at_sequence` | `u64` | the stage the dispute is *about*; it does not place the dispute anywhere |
| `n(3) evidence` | `Vec<bytes>` | ≤ 4 entries, each a complete `SignedEvent`, each ≤ `MAX_EMBEDDED_EVENT` = 32 768 B |
| `n(4) note` | `bytes` | ≤ 256 B UTF-8; human text, never parsed |
| `n(5) table_id` | `bytes[32]` | the table this dispute concerns; must equal the `table_public_key` of the session it is received on |
| `n(6) hand_id` | `u64` | the hand this dispute concerns |

`n(5)` and `n(6)` are appended rather than placed first because §2.2 rule 5 makes
field indices append-only; `JOIN_REQUEST`'s `n(8) table_id` was added the same way
for the same reason.

**The `kind` register, and it is closed.** Three values, and a `kind` outside them
is dropped at §4.0 step 11 like any other out-of-range field:

| `kind` | Name | Carries | Defined in |
|---|---|---|---|
| `1` | `STATE_DIVERGENCE` | this peer's own `STATE_HASH` for the disputed checkpoint | §6.3 step 2 |
| `2` | `EQUIVOCATION` | an `EquivocationProof` (§5.2) — **not produced in version 1 (D-015); a `DISPUTE` carrying this `kind` is dropped at §4.0 step 11 and its payload is never decoded** | §5.2 |
| `3` | `CHEAT_EVIDENCE` | **exactly one** `SignedEvent`, signed by the seat named in `n(1) accused`, which the emitter holds to be provably illegal under **D-014** | the box below |

`kind = 2` is written as `EQUIVOCATION` in §5.2 and carried no number until this
pass; assigning one is not a change of meaning and §5.2's object is untouched.
`kind = 3` is new with D-014. The transcript exchange of §6.3 step 3 has no `kind`
because it is not a dispute, and no request message for it is defined here.

#### `kind = 3 CHEAT_EVIDENCE` — the wire form of a D-014 finding

**`DECISIONS.md` D-014 is the decision and is not restated here; what this section
owes is the messages** (D-011 rule 1). D-014 removes from the table a player who
sends a **provably illegal** message, voids the hand that message attacked, and
lets play continue; its safety rests on evidence being **self-authenticating** —
*a message signed by the accused, whose illegality any peer can decide alone, from
that message plus state the peers provably share* — and on two tiers, the second
of which is state-dependent and waits for a checkpoint. Those are D-014's terms.
The wire consequences are these:

> **The evidence is one `SignedEvent` and it travels in a `DISPUTE`.** A D-014
> finding is published as `DISPUTE { kind = 3, accused = the offender's app public
> key, at_sequence = the offending event's `sequence`, evidence = [ that one
> complete `SignedEvent` ], table_id, hand_id }`. **Exactly one entry**, not up to
> four: a finding that needs a second event to be decidable is not
> self-authenticating and is not a D-014 finding. No new message type is added and
> none is needed — `DISPUTE` is already unchained, already legal at any time,
> already de-duplicated by `event_hash`, and already capped at
> `MAX_DISPUTES_PER_SENDER_PER_HAND = 8` per sender per hand.
>
> **Why the carrier has to be unchained.** Half of D-014's tier 1 is events this
> protocol never accepts into a chain — a signature that does not verify (§4.0
> step 9), a non-canonical encoding (§2.5), a malformed body, a field out of range
> (§4.0 step 11). Those events reach only the peers they were sent to,
> so a chained carrier would need the offender's cooperation to place, and a
> receiver that never saw the offending message would never learn of it. The
> `DISPUTE` is what puts the bytes in front of every peer; **it is not what makes
> the removal true**, and §4.9's own rule stands unchanged and is what keeps this
> safe: *a dispute is not itself evidence of anything — it is a carrier.* Every
> receiver re-runs the illegality check against the embedded event itself.
>
> **There is no removal message, and none may be added.** A removal is **derived
> by each peer from the evidence**, which is the whole of what makes it automatable
> where an accusation is not: no vote, no quorum, no timing, no per-receiver
> judgement. It reaches canonical state at exactly one place — **`HAND_INIT(k+1)`'s
> collective, byte-identical body** (§4.4), whose `n(8) dealt_in` excludes a
> removed seat and whose `n(11) ledger_delta` is recomputed by every receiver. Two
> peers that disagree about a removal therefore reject each other's `HAND_INIT`
> copy and **stage 0 stalls** — loud, disposed of by §8, and held back from
> becoming a silent fork by §3.2's solitary-stage rule. That is the same
> disposition §4.10's boundary window and §4.9's checkpoint-8 admission take, for
> the same reason, and it is chosen over a dedicated event because a collective
> stage that could not complete without the offender's own copy would hand the
> offender a veto over its own removal.
>
> **A removed seat may not re-enter, and this document is where a receiver
> enforces it.** `0x0804 PLAYER_SIT_IN` from a seat removed under D-014 is
> rejected under §4.0 step 12; so is every other chained event from it. D-014's
> exit is one-way, unlike a seat that merely went silent, and §4.10's widened
> `PLAYER_SIT_IN` legality condition is amended to say so. The seat keeps its row
> in `roster_hash` and its stack for the life of the table (§3.1) — chip
> conservation is not negotiable and a removal must not change the chip total —
> and what happens to that stack afterwards is `STATE_MACHINE.md`'s.
>
> **Tier 2 is judged against a checkpoint both peers signed — normative.** A
> tier-2 finding is one that is illegal only against game state: an out-of-turn
> action, a raise below the minimum, a bet larger than the stack, a showdown claim
> that contradicts the board. Such a finding **removes** its subject only when this
> receiver holds a **completed `STATE_ACK` stage** for a checkpoint of the same
> chain whose `sequence` is at or before the offending event's, and whose emitter
> set contained **both the accused and this receiver**, and the offending event is
> illegal against the `PublicTableState` that checkpoint fixed. Before such a
> checkpoint exists, a tier-2 finding **voids the hand and removes nobody**
> (`cause = 6`, §4.10). Tier 1 needs no checkpoint and never did: its illegality
> is decidable from the offending message alone.
>
> **Why the `STATE_ACK` and not the `STATE_HASH`.** The `STATE_HASH` stage is one
> peer's claim about the state; the `STATE_ACK` stage is the chained *"we all
> agreed here"* point §6.2 exists to produce, and the accused's own signature is in
> it. Judging a tier-2 finding against anything weaker would let a peer whose state
> has drifted — K1's divergence is exactly such a drift — see an honest player's
> perfectly legal action as illegal and remove it. That is the failure mode D-010
> closed, arriving by a new road, and the checkpoint precondition is the whole of
> what keeps it shut.

**What this makes load-bearing, stated in advance for once.** D-014 turns **the
correctness of every validator in §4.0 steps 11 and 13** into a consensus rule: a
validator that is too strict now removes an honest player rather than merely
rejecting a message, and two implementations that differ by one range check
disagree about the roster. `DECISIONS.md` D-014's own shipping gate is the mirror
of that — *for every legal action an honest client can emit, under every legal
interleaving, no honest peer is ever evictable* — and it is a gate on the feature,
not on this specification. What this document can do about it, and does, is keep
the removal's inputs to two: **a signature that verifies under the accused's key**,
and, for tier 2 only, **a checkpoint the accused signed**. Nothing else is read.
`n(1) attributed` in particular is **not** read: the removal is derived from the
signature on the evidence, never from any field naming a culprit, so D-010 point 2
survives D-014 unamended.

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
is the same set that emitted `HAND_INIT`, which is `P(k-1)` (§3.2, §4.4) and is
therefore participation-derived rather than status-derived. Every field is derived,
so by §3.2's stage-kind principle there is no writer: each seat computes the
byte-identical body, signs its own copy, and emits it.

**It is not narrowed further within the hand, deliberately.** A seat that signed
`HAND_INIT` and then went quiet blocks this stage, and §8 disposes of that hand at
`hand_deadline_ms`. `P(k)` has hand granularity (§3.2): exactly one hand pays for a
seat going silent — the one it went silent in — and from the next hand that seat is
outside every `R`.
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
| `1`, certified-subject path (`cert_hash = Some`) — **reachable from D-023**, which restored the decision clock and with it the certificate this row was always written for. D-015's *unreachable, therefore reject* disposition is superseded and does not survive anywhere: a `kind = 2` certificate is produced whenever `\|V\| >= 2` and a cryptographic deadline passes, and the emitter of the abort **must** populate `cert_hash` with that `TIMEOUT_CERT`'s `event_hash` whenever it names a subject. `attributed` and `cert_hash` travel together or not at all — a named subject without the certificate that named it is one peer's accusation on its own word, and §4.10's shape rules refuse it in both directions | it holds, or `n(3) evidence` carries, the named `kind = 2` `TIMEOUT_CERT` with `\|V\| >= 2`. *Holds* means it verified that certificate itself — opened every carried vote, checked each against the voter set, and found unanimity — and never that it took the hash on trust | **buffer, do not reject**: the mesh does not order two messages, so an abort whose certificate has not arrived yet is ordinary weather and not a fault |
| `1`, uncertified path (`attributed = []`, `cert_hash = None`) — the hand-deadline path and §6.3 case (b) are **one row**, see below | its **own** `hand_deadline_ms` has expired (§8.2), **or** it has itself reached §6.3 case (b) | **buffer, do not reject** |
| `2`, `3` | `n(3) evidence` verifies — the failing `SHUFFLE_PROOF` or reveal proof carries its own disproof | accept at once |
| `4` | it is itself in the §6.3 case (c) terminus | **buffer, do not reject** |
| `6`, tier 1 (D-014) | `n(3) evidence` carries exactly one `SignedEvent` signed by the seat named in `n(1) attributed`, and **this receiver's own** run of §4.0 over that event returns a tier-1 illegality — a signature that does not verify, a non-canonical encoding, a malformed message, an out-of-range field, a failed shuffle / decryption-share / key-ownership proof, a deck that is not a permutation, a signer who is not a party to this table. **The list is closed and every member is decidable from the offending event's own bytes**; *a parent that does not exist* stood here and is deleted, because it is decidable only against this receiver's own store (§4.0's box, `THREAT_MODEL.md` §5.1) | accept at once |
| `6`, tier 2 (D-014) | as above, **and** this receiver holds a completed `STATE_ACK` stage for a checkpoint of the same chain **whose §6.2 checkpoint number is at or after the number of the checkpoint the illegality was fixed at** (`G7-S8`; the clause read *at or before the offending event's `sequence`*, and the box below says why the number replaces it), whose emitter set contained both the accused and this receiver, **and** the event is illegal against the `PublicTableState` that checkpoint fixed | reject |

**The tier-2 precondition is stated in the checkpoint number, not in a `sequence` — `G7-S8`.**
The clause that stood in that row named *"a checkpoint of the same chain at or before the offending
event's `sequence`"*, and no path evaluates it: `STATE_MACHINE.md`'s T64 guard compares
`c.number >= n`, and `src/security/validation.rs` builds `AgreedCheckpoint` from `emitters`,
`hand_id` and `number` and refuses a `Tier2Finding` with `CheckpointTooEarly` unless
`against.number() >= fixed_at_checkpoint`. One precondition written in two units is D-011 rule 1's
shape, and the two units are not equivalent: *at or before the offending event* is satisfied by
checkpoint 1 of the chain, which fixes almost no state and decides almost no illegality. What a
receiver needs is **agreement over the checkpoint the illegality was fixed at, or a later one of the
same chain**, and later is not a weakening — `transcript_head` is in `PublicTableState` (§6.1) and
chains through every earlier stage, so agreement at `n' >= n` is agreement over a prefix containing
checkpoint `n`'s stage. That the judging checkpoint precedes the offending event is not a separate
test on the store: it is the row's last conjunct, since an event cannot be illegal against a
`PublicTableState` that already contains it. The checkpoint's own `sequence` keeps the purpose it
has — §4.1's `round` derivation — and stops being asked to carry a precondition as well.

**Why the two uncertified `cause = 1` paths are one row, and not two (H4).** They
stood here as two rows, and a receiver could not tell which of them it was
looking at. Both bodies carry `cause = 1`, `attributed = []`, `cert_hash = None`,
`deltas` all zeroes and `final_stacks` equal to the start-of-hand stacks — and
those are every field of `HAND_ABORT`. **The two bodies are byte-identical.** A
gate that selects a row by a distinction the wire does not carry is not
implementable: the receiver has no input to select on, so a row it cannot reach
is either dead text or a licence to guess.

**The union is adopted.** One row, one trigger, and the trigger is the
disjunction the two rows already spanned: the receiver buffers an uncertified
`cause = 1` abort until **either** its own `hand_deadline_ms` has expired **or**
it has itself reached §6.3 case (b), then accepts. Adopting the union rather than
inventing a discriminator field is the safe direction, because the two triggers
had the same disposition already — buffer, then abort neutrally, table not
faulted — so the union admits no outcome either row did not, and it removes the
only choice the receiver could have got wrong. Note that §6.3 case (b)'s row was
already the union of its own trigger and the deadline; this makes the deadline
row match it rather than the other way round.

The `cause` table further below still records **three** ways an uncertified
`cause = 1` abort comes about. That table is for a reader and a future
adjudicator, and it stays: what an emitter believed ended the hand is worth
recording even though no receiver can verify which of the three it was, and no
rule in this document reads it. What is not permitted is a *receiver* rule keyed
to a distinction the bytes do not carry, and there is now none.

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
the deadlock again by a third route. This is the **first of exactly two**
exemptions from §4.0 step 10a's chain-position rule in this document — the other
is the hand boundary window below, whose events all chain from `TERMINAL(k)`
because no emitter can know which other seats will occupy it (K2) — and it is
safe for exactly
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
it completes only if **every** seat of `P(k-1)` emitted its copy, so a completing
`HAND_COMPLETE` proves that no seat was silent, which is the premise every abort
rests on. The race is narrow — it needs a peer's deadline to expire between its
own `HAND_COMPLETE` emission and the arrival of the last other copy — and it is
resolved without a vote, a timer or a tie-break.

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) cause` | `u16` | `1` failure to publish a required cryptographic contribution; `2` invalid shuffle proof; `3` invalid reveal proof; `4` unresolvable state divergence; `6` **anti-cheat void (D-014)** — a party emitted a provably illegal message and the hand it attacked is voided. **Value `5` (equivocation proven) is deleted and its code point is not reused**, which is why D-014's value is `6` and not `5` — see §5.2's ordering rule and the note below. Every surviving value is a function of chained content that the ordering buffer can place, which is what makes two honest peers derive one body |
| `n(1) attributed` | `Vec<bytes[32]>` | ≤ `MAX_SEATS` app public keys, ascending by encoded bytes; may be empty. It carries the certified subject on `cause = 1`'s certified-subject path -- the seats the certificate names without `cause = 2` (D-065) -- the shuffler for `cause = 2`, the revealer for `cause = 3`, and is **empty** on `cause = 1`'s uncertified path — the gate's single `attributed = []`, `cert_hash = None` row, which spans both the hand-deadline expiry and §6.3 case (b) — and for `cause = 4`. **Evidence only — nothing in this document reads it to move a chip or to remove a player (D-010)** |
| `n(2) cert_hash` | `Option<bytes[32]>` | `event_hash` of the `TIMEOUT_CERT`. Required for `cause = 1` **when a certificate with an effect exists**, which is the certified-subject path and only that; `None` on every `attributed = []` path, where unanimity was by construction never reached and no certificate can exist |
| `n(3) evidence` | `Vec<bytes>` | ≤ 2 `SignedEvent`s, each ≤ `MAX_EMBEDDED_EVENT` = 32 768 B; required for `cause` 2 and 3, which are the two causes a single chained event proves on its own |
| `n(4) deltas` | `Vec<i64>` | **all zeroes, always (D-010).** An abort moves no chips, so there is no per-seat delta to carry; the field is a vector of `0` of length `\|occupied seats\|`. It is kept rather than removed so that `HAND_ABORT` and `HAND_COMPLETE` stay directly comparable to a verifier, and so that "the deltas sum to zero" stays one receiver check across both terminal messages rather than two |
| `n(5) final_stacks` | `Vec<u64>` | **the start-of-hand stacks, always (D-010)** — that is, `stack_at_hand_start[s]` for every occupied seat ascending, exactly the values already bound into `roster_hash(k)` and hence into `GENESIS(k)`. Every peer holds them from the genesis of the hand, so this field is a function of agreed state and two honest peers cannot derive it differently |

*Receiver must validate:* the `cause` is one of the five defined values; **the
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
**user** decision (D-010 point 3). `THREAT_MODEL.md` §9.1.2 limitation 4 carries this
as an unfixed limitation.

**The `cause` values are evidence, not dispositions.** They no longer select
between chip outcomes, because there is exactly one chip outcome. What they still
do is record which of four things ended the hand, for a reader and for a future
adjudicator:

| `cause` | Ends the hand when | `attributed` |
|---|---|---|
| `1` failure to publish | a `kind = 2` `TIMEOUT_CERT` with `\|V\| >= 2` closed a stage (§8.3) — **not produced in version 1, so this first path is unreachable here (D-015)**; **or** `hand_deadline_ms` expired with no certificate that had an effect (§8.4); **or** §6.3 case (b) — a peer is missing events no holder will serve | the certified subject on the first path, **empty** on the other two — and therefore **empty on every `cause = 1` abort this version produces** |
| `2` invalid shuffle proof | a `SHUFFLE_PROOF` failed verification (§4.0 step 14, `INVALID_SHUFFLE_PROOF`) | the shuffler |
| `3` invalid reveal proof | a Chaum–Pedersen DLEQ failed at `DEAL_PRIVATE`, `BOARD_REVEAL` or `SHOWDOWN_REVEAL` — the hand aborts here rather than stalling into a deadline (`CRYPTOGRAPHY.md` §8 rule 1) | the revealer |
| `4` unresolvable divergence | §6.3 case (c): transcripts byte-identical after reconciliation and derived states still differ | **empty, always** |
| `6` anti-cheat void (D-014) | a `DISPUTE { kind = 3 }` carried a `SignedEvent` this receiver independently found illegal under D-014 (§4.9), or this receiver found one itself | the offender, and **still evidence only** — the removal is derived from the signature on the evidence, never from this field (D-010 point 2) |

Three of the five paths therefore carry `attributed = []`, which is no longer
worth a table of exceptions: an abort that names nobody and an abort that names
someone have identical effects **on the chips**, which is the only thing
`attributed` was ever read for and is now read for nowhere.

**Under D-015 it is four of the five, and `cause = 1` names nobody at all.** The
certified-subject path is the only `cause = 1` path that ever populated
`attributed`, and it needs a certificate this version does not produce. So the
non-empty cases this version can reach are exactly `cause = 2` (the shuffler),
`cause = 3` (the revealer) and `cause = 6` (the offender) — all three
self-authenticating from the offending event's own bytes, none of them a claim
about a deadline. **That is the first of D-015's stated costs made concrete in
this document: there is no signed record naming who timed out**, only the
transcript's visible gap at the stalled stage, which is what a human or a later
version would have read anyway (D-010 point 2).

**`cause = 6` is the one cause a seat's removal follows, and it does not follow
from this message.** The abort voids the hand — stacks restored to their
start-of-hand values, `n(4) deltas` all zeroes, `n(5) final_stacks` the
start-of-hand stacks, exactly like every other abort under D-010, which is why
D-014 point 1 says *"nothing new is needed"* for the chip disposition. Whether the
offender is also **removed** is decided by each receiver from the evidence and the
tier (§4.9's box), and it takes effect through `HAND_INIT(k+1)`'s collective body.
A `cause = 6` abort whose tier-2 checkpoint precondition is not met at this
receiver is a void with **no** removal, and that is D-014's own rule, not a
weakening of it. The table is **not** faulted: `cause = 4` remains the one cause
after which no further hand is dealt, and a removal exists precisely so that play
can continue without the removed player.

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
the only asymmetry left between the causes that is about the **table**. The one
asymmetry that is about a **player** is `cause = 6`'s, and it is D-014's rather
than this field's: the removal follows from the evidence, not from the `cause`
value, which is why a `cause = 6` abort whose tier-2 precondition is unmet at a
receiver is an ordinary neutral abort at that receiver.

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

**And under D-013 an abort no longer needs to reach a seat's state at all.** What
D-005's absent-seat rule was trying to express — *a silent seat is out of the next
hand because it was silent* — is now expressed directly, by `P(k)` (§3.2), without
any status in the path: the seat is outside `P(k)` because it signed nothing in
hand `k`, so it is outside every `R` of hand `k+1` and outside `dealt_in(k+1)`
(§4.4). That is a function of the accepted chain, not of any abort's body, which
is what makes it safe under D-012 and is why this rule could be adopted where a
status could not.

---

#### The hand boundary window — normative, and this is the closure of `Q-09` (K2)

`0x0803 PLAYER_SIT_OUT`, `0x0804 PLAYER_SIT_IN` and `0x0805 PLAYER_LEAVE` are
chained (`chain_scope = 1`) and legal only at a hand boundary. This box gives
them the four envelope quantities that were undefined, and the total order that
was undefined with them. **`Q-09` graded this a placement decision and that grade
was wrong**: `hand_id` and `sequence` are inside `TO_BE_SIGNED` (§2.4), so two
implementations that place the event differently produce **different signed bytes
for the same intent**; they are two of the eight components of the slot key
(§5.2.1) that §4.0 step 10a reads whole, so a receiver cannot look the event up;
`sequence` fixes `previous_event_hash` through §3.2; and §4.0 step 12 asks
whether this type from this seat is expected at this `sequence` and had no answer
to be given. Under D-013 a seat outside `P(k)` may legally emit **exactly one**
chained event — `PLAYER_SIT_IN` — so the whole of re-entry ran through the one
message whose chain position this document declined to specify.

> **Chain.** A boundary event belongs to **chain `k`**, the hand that has just
> ended, and carries `hand_id = k`. It does **not** belong to chain `k+1`.
>
> **`sequence`.** `sequence = BOUNDARY_SEQUENCE_BASE + sender_seat`, where
> `BOUNDARY_SEQUENCE_BASE = 4 096` (§13) and `sender_seat` is the emitter's own
> seat index. Every seat has one reserved slot in the window and no other. A
> receiver rejects a boundary event whose `sequence` is not exactly that value,
> and rejects any other chained type at a `sequence >= BOUNDARY_SEQUENCE_BASE`.
>
> **Parent.** `previous_event_hash = TERMINAL(k)` for **every** boundary event of
> the window, whichever seats emit and in whatever order they arrive. Equivalently,
> §3.2's rule extended: `stage_hash(BOUNDARY_SEQUENCE_BASE - 1) := TERMINAL(k)`,
> beside `stage_hash(-1) = GENESIS(hand_id)`.
>
> **The window has no `stage_hash`.** Nothing chains from it: `GENESIS(k+1)` is a
> function of `roster_hash(k+1)` and `TERMINAL(k)` (§3.1), and neither reads the
> window. It is the second stage in this protocol with no `stage_hash`, after the
> witness-independent terminal, and for the same reason — nothing needs one.
>
> **Total order.** Ascending seat index, which the `sequence` rule already fixes.
> Two seats emitting at one boundary occupy two `sequence` values and are applied
> low seat first, at every receiver, without a tie-break.
>
> **One per seat per boundary.** A seat may emit at most one boundary event for
> hand `k`; a second, of any type, is a stage violation under §4.0 step 12. **That
> rule, and not the stage count, is what bounds the window**: `sequence` values at
> or above `BOUNDARY_SEQUENCE_BASE` are exempt from §5.3's `MAX_STAGES_PER_HAND`
> abort, which counts stage indices below it (`L5`, §5.3, §13). Without the
> exemption a receiver that range-checks `sequence < MAX_STAGES_PER_HAND` rejects
> every event this box defines.
>
> **Close.** The window for hand `k` closes at this receiver's acceptance of a
> complete `HAND_INIT(k+1)` stage. A boundary event for hand `k` arriving after
> that is rejected as out of stage; the seat re-emits at the next boundary.

**Why chain `k` and not chain `k+1`, and this is what decides it.** A boundary
event placed at the head of chain `k+1` would chain from `GENESIS(k+1)` — and
`GENESIS(k+1)` reads `roster_hash(k+1)`, which a `PLAYER_LEAVE` would change.
The event would then be chained from a value it is an input to. Placing the
window at the tail of chain `k` has no such circularity: `TERMINAL(k)` is fixed
before the window opens, on both terminal paths, and §3.2's `P(k)` window —
*"every stage of chain `k`, plus chain `k`'s hand boundary window"* — is exactly
this placement and needed no adjustment to receive it.

**Why the parent is `TERMINAL(k)` and not the previous boundary event.** A seat
signs its boundary event without knowing which other seats will emit one, so a
running parent is not computable by the emitter. `TERMINAL(k)` is: it is
`stage_hash` of the `HAND_COMPLETE` stage on the decided path and
`ABORT_TERMINAL(k)` on the abort path, and **both are agreed by every peer by
construction** — the abort path's terminal is agreed even when the abort's own
`sequence` is not, which is the whole of P3's property and the one place it is
used for something. The window is therefore a fan and not a chain, and that is
why each seat needs a reserved `sequence` rather than a running one. This is the
**second** exemption from §4.0 step 10a's chain-position rule, after the terminal
`HAND_ABORT`'s, and it is stated here so the count stays exact.

**Why a per-receiver close is safe here, and what it depends on.** Two receivers
can hold different windows — one heard a `PLAYER_LEAVE` the other did not — and
nothing collective closes the window. That would be fatal if the window fed
`GENESIS(k+1)`; §3.1 is what stops it, by freezing the roster's seat vector for
the life of the table, so a window disagreement reaches canonical state only
through `HAND_INIT(k+1)`'s **collective, byte-identical** body — its
`n(8) dealt_in` and `n(11) ledger_delta` — where the two peers reject each
other's copy and **stage 0 stalls**. That is loud, it is disposed of by §8, and
both peers still hold the same `GENESIS(k+1)` and the same `ABORT_TERMINAL(k+1)`,
so nothing is permanent.

**And a stalled stage 0 is the one place `P` narrows, so this disposition depends
on §3.2's solitary-stage rule and is not safe without it.** With it, the peers
that narrowed cannot complete a hand in silence and the disagreement surfaces;
without it, the very next hand is K1's trace. The two rules are one fix and must
be read together: an editor who deletes either has restored a permanent silent
fork by way of the other.

**What is still `STATE_MACHINE.md`'s.** Which engine phase the table sits in
while the window is open, and for how long, is that document's (D-011 rule 1) and
is `K-3b` in `DECISIONS.md`'s open list: `HandComplete` is a zero-width derived
hop there, and a wire window with no phase to hold it is a window no
implementation can use. This box defines the wire; the phase is owed.

---

**`0x0803 PLAYER_SIT_OUT`** — single-writer stage at a hand boundary only.
`n(0) reason: u16` (`1` voluntary, `2` derived from consecutive auto-actions).
The derived case needs no message at all — after `MAX_CONSECUTIVE_AUTO_ACTIONS`
every peer marks the seat sitting out identically (D-006 point 4) — so this
message exists for the voluntary case and as an explicit record. A seat that is
sitting out keeps its stack, pays its blinds and antes, takes no cards, and drains
until it busts.

> **This client never sends it** (`S1-BZ`, closed 2026-09-18). A seat of this
> build says it sits out by §7.9's member status, which needs no boundary and
> changes no rule, and the window records a received `PLAYER_SIT_OUT` and reads it
> for nothing. A peer that sends one is conforming and loses nothing by it.

**`0x0804 PLAYER_SIT_IN`** — single-writer stage in the boundary window defined
above: `hand_id = k`, `sequence = BOUNDARY_SEQUENCE_BASE + seat`, parent
`TERMINAL(k)`. No fields beyond the envelope. Takes effect from the next
`HAND_INIT`, never mid-hand. **Two effects by two roads (D-028):** at a
receiver whose window is open it writes §4.9's readmission set `A`, which
widens the accepted set of `HAND_INIT(k+1)` and moves no roster; and it is
the first half of the evidence a `RETURN_CERT` carries (§4.8, §8.3.1),
which is the one road by which a seat re-enters `R(k+1)`.

**It is one of the two cases exempt from §3.2's solitary-stage rule**, and the
exemption is not incidental: a solitary peer that treated it as a contradiction
would fault the table on the message whose whole purpose is to end the solitude.
The other exempt case is a checkpoint-8 `STATE_HASH` from a seat outside `P(k)`,
which §4.9's box admits and has compared rather than frozen on — the sentence that
stood here, *"a seat outside `P` may emit this and nothing else"*, was true when
written and is no longer, and it is corrected rather than left standing. Every
other chained type from a seat outside `P(k-1)`, naming a hand this peer was
solitary for, is a divergence under §4.0 step 12a whether it arrives during that
hand or after it (step 10b).

**Its legality condition is widened by D-013, and this is the whole of re-entry
(J2).** It read *"legal only from a seat currently sitting out"*, which under the
old status-based emitter set was the only way a seat could be outside one. It now
reads:

> Legal from **any occupied seat with a non-zero stack that is not a required
> emitter of the next hand and has not been removed under D-014** — that is, any
> occupied seat outside **`R(k)`**, the roster of hand `k` (D-028; it read `P(k)`,
> §3.2, while the two coincided), whether it is outside because it sat itself out
> with `PLAYER_SIT_OUT` or because it was certified out. A copy from a seat inside
> `R(k)` decides nothing and is rejected as an out-of-stage chained event under
> §4.0. **`R(k)` and not `P(k)`, and the difference is the whole return road:** a
> bystander in §4.9's readmission set `A` signs stage 0 of hand `k`, so it is in
> `P(k)` without being in `R(k)`, and its `PLAYER_SIT_IN` at hand `k`'s boundary
> is exactly the request a `RETURN_CERT` (§8.3.1) is about — `split174002-9`,
> where eight voters refused it as deciding nothing.
>
> **The D-014 clause is the one-way half of that decision and this is where a
> receiver enforces it** (§4.9). A seat removed for a provably illegal message is
> outside `P` like a silent seat and unlike a silent seat it may not come back:
> every chained event from it is rejected, this one included. That is the only
> asymmetry between the two, and it is deliberate — a silent seat proved nothing
> about itself, a removed seat proved something with its own signature.

**A seat rejoins by signing a chained event, which requires it to be alive.** That
is the entire test and there is no other route: no certificate, no vote, no
quorum, no proof, no attribution, and no action by any other seat. Nothing else
puts a seat back and nothing else needs to.

**And the test now has a moment at which it can be taken, which is `N5` and is a
correction to this sentence rather than a new route.** A chained event is accepted
into the window it names, and this window closes at acceptance of a complete
`HAND_INIT(k+1)` — one round trip at a healthy table and **zero at a peer in the
solitary regime**, whose `HAND_INIT(k+1)` is required of `{self}` and
self-completes. Read strictly with §4.0 step 10b's *"counts into no `P`"*, a seat
could therefore never rejoin the one peer that most needs to hear from it: the
peer that is draining it. §4.9's **readmission set** is the disposition: a stale
`PLAYER_SIT_IN` of chain `k`'s boundary window, and a stale checkpoint-8
`STATE_HASH` of chain `k` that **agrees** with this receiver's own value, make
their sender an **accepted** emitter of the next hand init this receiver runs and
do nothing else at all — never a required one, which is `P2`. The test is still
*signing a chained event*, still requires being alive, and
still admits no certificate, vote, quorum, proof or third party; what changed is
that missing a window is no longer permanent.

**`0x0805 PLAYER_LEAVE`** — single-writer stage at a hand boundary, and **only**
there, in the window defined above. `n(0) reason: u16` (`1` voluntary, `2` client
shutting down). A leave is never *required*: a client that vanishes must be
handled identically, because a departing peer announcing anything can never be a
precondition. [NAT §7.3]

> **This client never sends it** (`S1-BZ`, closed 2026-09-18). A player of this
> build who leaves says so by `TABLE_LEAVE` (§7.10), unchained, at any moment of a
> hand or before the table is set, and its seat is voted out at once (`D-063`); a
> received `PLAYER_LEAVE` is recorded in the window and read for nothing, as below.

**It removes no seat from `roster_hash` and it counts into no `P` (K2, §3.1,
§3.2).** The seat keeps its row in the roster vector for the life of the table —
§4.3 already says the roster cannot be *"added to, removed from or reordered
without a new table"*, and §3.1 now says it normatively where the hash is
defined, because letting this message reach `roster_hash(k+1)` would derive
`GENESIS(k+1)` from a window no stage completion covers. Its chips leave through
`HAND_INIT`'s `n(11) ledger_delta`, inside a collective body every receiver
recomputes. And it is excluded from `P` for the reason §3.2 gives: it is the one
chained event that says its signer will emit nothing further, so counting it as
participation would make every announced departure a required emitter of the next
hand and stall that hand to the deadline.

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
| `0x0107` | `TABLE_CHAT` | table mesh (the table's group; its topic where there is no group) | 0 | — | a seated application key (§7.8) |
| `0x0108` | `TABLE_LEAVE` | table mesh (the table's group; before the set, its topic too) | 0 | — | a seated application key, of its own seat (§7.10) |
| `0x0109` | `TABLE_HEARING` | table mesh (the table's group and its topic, before the set) | 0 | — | a seated application key, of its own seat (§7.11) |
| `0x010A` | `TABLE_CONTINUES` | table mesh (the group and the topic of the table that goes on, before the set) | 0 | — | a seated application key, the founder of the advert it carries (§7.12) |
| `0x010B` | `SEARCH_PRESENCE` | search queue broadcast (§7.13) | 0 | — | any peer, about itself |
| `0x0201` | `JOIN_REQUEST` | join RPC | 0 | — | joiner |
| `0x0202` | `JOIN_ACCEPT` | join RPC | 0 | — | table key |
| `0x0203` | `JOIN_REJECT` | join RPC | 0 | — | table key |
| `0x0204` | `PLAYER_LIST` | table mesh | 0 | — | table key |
| `0x0205` | `TABLE_READY` | table mesh | 1 | collective | every seat of the `PLAYER_LIST` roster; its signers **are** `P(0)` |
| `0x0301` | `RNG_COMMIT` | table mesh | 1 | collective | `P(0)` — the `TABLE_READY` signers |
| `0x0302` | `RNG_REVEAL` | table mesh | 1 | collective | `P(0)` — the `TABLE_READY` signers |
| `0x0303` | `HAND_INIT` | table mesh | 1 | collective | `P(k-1)` — §3.2, §4.4. §4.9's readmission set `A` widens the **accepted** emitter set to `P(k-1) ∪ A` and leaves the required set alone (`P2`); it is the second stage in the document whose accepted set is wider than its required one, the first being checkpoint 8 |
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
| `0x0601` | `TIMEOUT_VOTE` | table mesh | 1 (`event_class = 1`, keyed also on `subject_seat`) | — | **none in version 1 (D-015)** — defined emitter is the required voters |
| `0x0602` | `TIMEOUT_CERT` | table mesh | 1 (`event_class = 2`, keyed also on `subject_digest`) | collective | **none in version 1 (D-015)** — defined emitter is the required voters |
| `0x0603` | `RETURN_VOTE` | table mesh | 1 (`event_class = 1`, keyed also on `subject_seat`) | — | `R(k) \ OUT(k)`, the voters of hand `k`'s boundary (§4.8, D-028) |
| `0x0604` | `RETURN_CERT` | table mesh | 1 (`event_class = 2`, keyed also on `subject_digest`) | collective | the same voters, each its own copy (§4.8, §8.3.1) |
| `0x0701` | `STATE_HASH` | table mesh | 1 | collective | required of `P(k-1)` at checkpoints 2–7; `P(0)` at checkpoint 1; **`P(k)` at checkpoint 8, and there accepted and compared from any occupied seat, in-set or not**; in a **reconciliation round**, `R(c) ∪ W` — never fewer than two seats — §4.9 |
| `0x0702` | `STATE_ACK` | table mesh | 1 | collective | the set of the `STATE_HASH` stage it confirms, at every checkpoint including 8 and in every reconciliation round; **at checkpoint 8 it is accepted until `TERMINAL(k+1)` and not only until `HAND_INIT(k+1)`** — §4.9 |
| `0x0703` | `DISPUTE` | table mesh | **0** | out-of-stage | any participant |
| `0x0801` | `HAND_COMPLETE` | table mesh | 1 | collective | the set that emitted `HAND_INIT`, `P(k-1)` — §4.10 |
| `0x0802` | `HAND_ABORT` | table mesh | 1 | **witness-independent terminal** | any seat of the `HAND_INIT` set; no required set (§3.2, §4.10) |
| `0x0803` | `PLAYER_SIT_OUT` | table mesh | 1 | single, boundary window of chain `k` — §4.10 | the seat, at `sequence = BOUNDARY_SEQUENCE_BASE + seat` |
| `0x0804` | `PLAYER_SIT_IN` | table mesh | 1 | single, boundary window of chain `k` — §4.10 | the seat, same `sequence` rule; the one type a seat outside `P(k)` may emit |
| `0x0805` | `PLAYER_LEAVE` | table mesh | 1 | single, boundary window of chain `k` — §4.10 | the seat, same `sequence` rule; counts into no `P` (§3.2) |

43 message types. Lobby chat is on `/p2p-poker/lobby-chat/1` and not on the lobby
topic, and the search queue's presence is on `/p2p-poker/search-queue/1` (§7.13),
so §1.4's "a message on the wrong channel is dropped" rule covers them like any
other.

**Two of the 41 rows have no emitter in version 1, and the count stays 41
(D-015).** `0x0601` and `0x0602` keep their rows, their codes, their
`chain_scope`, their `event_class` and their stage kinds, so the table remains
the complete register of the wire and a later version adds an emitter rather
than a row. **The remaining 39 are the whole of the traffic this version
produces**, and the two exceptions are marked in the `Emitter` column rather
than deleted from the table, because a deleted row is how a code point gets
reused. The interleaving table below keeps its rows 31 and 32 for the same
reason and for one more: a verdict that has been wrong twice is worth keeping
correct while nothing exercises it.

**The `Emitter` column carries no status word, and that is now a rule.** Every
collective row names either a set fixed by chained content (`dealt_in`, the
showdown set, `V(subject)`) or `P` (§3.2). Four rows used to read "all present
seats" or "all seats" and were changed in this pass — `HAND_INIT`, `STATE_HASH`,
`STATE_ACK`, `HAND_COMPLETE`, plus `RNG_COMMIT` and `RNG_REVEAL`'s "every seated
participant" in §4.4. **A new row whose emitter cell names a status is a defect**
(J2, D-013, §2.9's enumeration 2).

**Four table-mesh messages carry `chain_scope = 0` — `HELLO`, `CAPABILITIES`,
`PLAYER_LIST` and `DISPUTE` — and the table above is the normative statement of
it; §2.3's exhaustive thirteen-type list is the other index and the two must be
kept in step.** This sentence read *"`DISPUTE` is the one table-mesh message with
`chain_scope = 0`"* and contradicted the table three lines above it, which is
`M-1` in `DECISIONS.md`'s open list, found while transcribing the table into
`src/protocol/messages.rs`. **An implementer builds the sentinel check off §2.3's
list or off this table, never off prose.** What the deleted sentence was reaching
for is true of the three groups that carry hand state and is stated that way:
**`DISPUTE` is the only message of groups 3–8 with `chain_scope = 0`**, which is
why it is the only one that can be legal outside its stage and the only one that
can never appear in an `EquivocationProof` (§5.2). Its `Stage kind` is
"out-of-stage" precisely because it occupies no slot — the two cells are
consistent, not in tension. `PLAYER_LEAVE` is on the chained side of that line
with no exception of any kind (M4, §4.10).

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
| 14 | `TABLE_READY` | **one per seat per `list_serial`**, all of them at `hand_id = 0, sequence = 0` on the setup chain — an honest client re-ratifies whenever `adopt` sees a new serial | `list_serial`, `roster_hash`, `emitted_at_unix_ms` — **all three in the payload or outside the key, so none of them separates the slots** | **FAILS, and this is the sixth recurrence.** The row read *"one per seat, setup chain / — / Clean"* for five revisions; the emission count was simply wrong. Two ratifications by one honest seat at two serials are one slot and two `event_hash`es, which is §5.2.2's predicate exactly. Measured: 14 honest ratifications on a five-seat formation produce 5 slots and **9** convictions. This row is why §5.2.1 is not implemented (see its header) and it is the first thing a future wire-in must fix — by giving each ratification its own `sequence`, not by narrowing the key |
| 15 | `RNG_COMMIT` | one per seat, own stage | — | **Clean** |
| 16 | `RNG_REVEAL` | one per seat, the next stage | `sequence` | **Clean** |
| 17 | `HAND_INIT` | one derived copy per seat of `P(k-1)` | — | **Clean.** A `HAND_INIT` that stalls is disposed of by a `HAND_ABORT` at `HAND_INIT`'s own `sequence`, which differs in `event_type` — row 36 |
| 18 | `DECK_INIT` | one per dealt-in seat | — | **Clean** |
| 19 | `SHUFFLE_STEP` | one, single-writer | — | **Clean** |
| 20 | `SHUFFLE_PROOF` | one, the next stage | `sequence` | **Clean** |
| 21 | `DECK_COMMIT` | one per dealt-in seat | — | **Clean** |
| 22 | `DEAL_PRIVATE` | one per dealt-in seat, **exactly** `2(m−1)` entries in one body | — | **Clean.** The "exactly" is what forbids incremental publication, which would be two bodies at one stage |
| 23 | `BOARD_REVEAL` | one per dealt-in seat per street | `sequence` — each street its own stage | **Clean** |
| 24 | `SHOWDOWN_REVEAL` | one per showdown seat | — | **Clean** |
| 25 | `SHOWDOWN_MUCK` | one per showdown seat, mutually exclusive with row 24 by `showdown_policy` (§4.6) | `event_type`, if a seat emits both | **Clean.** Since `event_type` entered the key the pair is two slots, so a seat emitting both is a **stage violation** under §4.0 step 12, not an equivocation. §4.6's exclusivity stays normative and is what rejects the second (§5.2.1) |
| 26–30 | `ACTION_CHECK`, `ACTION_CALL`, `ACTION_BET`, `ACTION_RAISE`, `ACTION_FOLD` | one per turn, single-writer | `event_type`, if a seat claims two actions for one turn | **Clean.** Same change as row 25 and the same price: two *different* action types at one `sequence` are a stage violation, not a proof; two bodies of the **same** type — two `ACTION_RAISE` with different amounts — are still an equivocation (§5.2.1) |
| 31 | `TIMEOUT_VOTE` | **none in version 1 (D-015)**; the defined emission is one per subject per stage, and two simultaneous subjects are **normal** (§8.4) | `subject_seat` | **Vacuously clean in version 1** — an honest peer emits none, so it emits no pair. **Clean since M2 for the defined behaviour**, by the subject axis in the key; the verdict is retained because the axis is retained (D-015 point 3) |
| 32 | `TIMEOUT_CERT` | **none in version 1 (D-015)**; the defined emission is one per `subject_digest` per stage | `subject_digest` | **Vacuously clean in version 1**, same reason. **Clean since M2 for the defined behaviour**, by the subject axis; its one variable field `n(1) votes` is pinned by the receiver check §4.8 defines |
| 33 | `STATE_HASH` | one per checkpoint, **plus one per reconciliation round** — a required re-emission with *changed* content, the only one in the corpus — plus, at **checkpoint 8**, one from a seat outside the required set, which §4.9 admits | `sequence`, and only because §4.9 gives each reconciliation round its own — `s_ckpt + r` inside a hand, `BOUNDARY_CHECKPOINT_BASE + 2r` at checkpoint 8 | **Clean since P1, and clean by that rule alone.** Not by the stage rule: an editor who deletes §4.9's normative boxes reopens P1 the same day. The checkpoint-8 admission does not touch this verdict — the key contains `sender_public_key`, so an out-of-set emitter fills **its own** slot at that `sequence` and one honest peer still emits one body per slot |
| 34 | `STATE_ACK` | one per checkpoint and one per reconciliation round; **not** widened at checkpoint 8 | `sequence`, same rule | **Clean since P1**, same reason |
| 35 | `HAND_COMPLETE` | one derived copy per seat of `P(k-1)`, at a fresh `sequence` — the last stage completed, so nothing occupies it | — | **Clean** |
| 36 | `HAND_ABORT` | **one per emitter per hand**, at the stalled stage's own `sequence`, where the emitter has usually already contributed | `event_type` — against the contribution it shares a `(sequence, class)` with | **Clean since G1, and only because `event_type` is in the key (§5.2.1, D-011 rule 2).** This row is the reason the key was rewritten. Under the seven-tuple it read FAILS: the abort collided with its own emitter's contribution, was rejected at §4.0 step 10a, and was a verifying proof against an honest peer |
| 37 | `PLAYER_SIT_OUT` | **at most one per seat per boundary window**, at `sequence = BOUNDARY_SEQUENCE_BASE + seat` in chain `k`, parent `TERMINAL(k)` — §4.10 | — | **Clean since M4, and now clean for a stated reason (K2).** The seat's own reserved `sequence` gives it capacity one by construction; a second boundary event of any type from that seat is a stage violation at step 12, and where the type differs it is two slots, so it is a rejection and never a proof |
| 38 | `PLAYER_SIT_IN` | same window, same reserved `sequence` | — | **Clean, same reason.** This is the one type a seat outside `P(k)` may emit, and the window is what makes that emission placeable |
| 39 | `PLAYER_LEAVE` | same window, same reserved `sequence`, and **only** there — the courtesy-notice clause is deleted | — | **Clean, same reason.** It shares the seat's one slot with rows 37 and 38, which is what forbids a seat both sitting out and leaving at one boundary |

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
| duplicate message | **in version 1: the per-stage `heard` map of §5.2.5, not a slot store.** The design target was an `event_hash` set per **slot** under §5.2.1's key | a byte-identical repeat is idempotent; a differing repeat in one slot is equivocation. §4.11 carries the per-type check that says why each key component is there |
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

#### 5.2.1 The anti-replay slot key — DESIGN RECORD, not implemented in version 1 (D-011 rule 2, D-025)

**NOT IMPLEMENTED IN VERSION 1, AND NOT IMPLEMENTABLE AS WRITTEN AGAINST THIS
MESSAGE SET. This section is retained as a design record and as the reference a
future version must start from; it is not an acceptance condition, and no client
computes it.** The modules that implemented it — `src/protocol/slot.rs` and
`src/protocol/antireplay.rs` — were deleted rather than wired in. Read the
box below as *"the key a corrected message set would need"*, never as *"what the
receiver checks"*.

**Why it was not wired in, in one sentence:** applied to the message set this
document actually specifies, the key convicts honest peers. `TABLE_READY` is a
chained event pinned to `hand_id = 0, sequence = 0` for every seat (§4.2), an
honest client re-ratifies on every `list_serial` change, and the two fields that
distinguish those emissions — `list_serial` and `roster_hash` — are **payload**
fields, which this key excludes on purpose. So several distinct honest bodies by
one signer land in one slot, and the predicate of §5.2.2 labels each repeat after
the first an equivocation. Measured on an ordinary attack-free five-seat
formation: **14 honest ratifications, 5 slot keys, 9 honest bodies convicted.**
That is `§4.11` row 14's verdict — *"one per seat, setup chain / — / Clean"* —
being wrong, and it is the **sixth** recurrence of the defect this section says
five review passes were spent on. The executable form of the measurement is
`tests/anti_replay_authority.rs`.

**By this section's own closing rule that is a defect in the message and not in
the key**, so the key is left as it stands. Correcting `TABLE_READY`'s placement
is a wire change and belongs to a version that makes it, together with the rest
of the wire-in prerequisites in §5.2.5.

**What carries the property in the running client instead is §5.2.5, and every
document that used to reference this section for an enforcement claim now
references that one.** §4.0 step 10a, §5.3 and §4.11 are annotated accordingly.

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

The reason is mechanical rather than aesthetic. Network arrival order is removed
by an **ordering buffer** that runs in the `protocol` layer, before the engine's
`step` is ever called, keyed on

```
(table_id, hand_id, sequence, previous_event_hash)
```

— four envelope fields §2.3 defines, so **this document owns that key and states it
here** (D-011 rule 1). The sentence that stood here cited `STATE_MACHINE.md` §3.2
for it; that document deleted its copy under the same rule and asked this one to
state it, and the stale pointer is J7. **This is not §5.2.1's slot key**, and the
two must never be conflated: the slot key is the eight-tuple that defines
equivocation and it deliberately **excludes** `previous_event_hash`, while this key
deliberately **includes** it, because the buffer's whole job is to place an event
relative to its parent. §2.3 obliges every unchained event to carry a **sentinel in
all four** of these fields. The buffer
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

> **Normative — the proof object is defined but not produced in version 1
> (D-015), and the detection is not.** §5.2.1's key and §5.2.2's predicate are
> untouched and run on every chained event: a receiver still finds two bodies in
> one slot, still rejects the second at §4.0 step 10a or step 12, still diverges
> from a peer that accepted the other first, and the hand still ends through the
> chained path below. **What is not produced is the `EquivocationProof` object
> and the `DISPUTE kind = 2` that carried it.** No client of this version
> constructs one, broadcasts one, or accepts one — a `DISPUTE` arriving with
> `kind = 2` is dropped at §4.0 step 11 (header box, point 2). The object's
> definition, its `DISPUTE kind` code point and its verification procedure stay
> exactly as written below, so a later version reinstates it without a wire
> break.
>
> **The cost, and it is the one D-015 records as unsettled.** Equivocation is
> **detected and no longer provable to a third party who was not present.** The
> detector still knows; the two peers who diverge still stop; the transcript
> still shows it. What is gone is the self-contained artefact that carried the
> two signed bodies to somebody who was not at the table, and with it every
> claim in this section about evidence "usable by anyone, forever". Nothing else
> is lost, because under D-010 nothing consumed the proof anyway: it ended no
> hand, moved no chip and unseated nobody. `THREAT_MODEL.md` §9.1.0 carries the
> loss as a standing limitation and `DECISIONS.md` D-015-2 is the decision that
> must revisit it before real money.

The two conflicting events share a slot — or, when they differ in `event_type`,
the same stage cell — so §4.0 step 10a or step 12 rejects whichever
reaches each peer second, as a violation; peers that accepted different first
copies now hold different state; the next checkpoint (§6.2) shows two `state_hash`
values; §6.3 runs; and the hand ends through that **chained** path, neutrally,
like every other abort. **The outcome is identical either way**, which is what
makes the labelling difference §5.2.1 accepts affordable. **Under D-015 that
chained path is the whole of what happens** — the sentence that stood here,
*"The proof is still built, still broadcast in a `DISPUTE`, still retained
forever and still shown to the user"*, is withdrawn: none of the four happens in
version 1. What the detector does instead is what it did before the object was
ever consulted — reject the second body, diverge, and end the hand chained and
neutrally — and it may show the user what it found, from its own two retained
copies, without an object to hand anybody else. And if no peer ever diverges,
there was nothing to decide — an equivocation nobody's state disagreed about
cost nobody anything.

**The proof object, defined and not produced (D-015).**

```
EquivocationProof  #[cbor(array)]
  n(0) accused    : bytes[32]     the equivocating public key
  n(1) event_a    : bytes         complete SignedEvent, <= 32768 B
  n(2) event_b    : bytes         complete SignedEvent, <= 32768 B
```

Carried as the payload of a `DISPUTE` with `kind = EQUIVOCATION` — **in the
version that produces it. Version 1 produces neither**, and a `DISPUTE` arriving
with that `kind` is dropped (D-015). Whether, in that later version, it is also
broadcast on the lobby topic so that peers who were never at the table hold
durable evidence, is **OPEN QUESTION Q-05**, which D-015 makes moot for this
version without answering.

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

**Consequence, under D-010: none that is automatic. Under D-015 there is no
object to have a consequence.** In the version that produces it the proof is
retained, kept beside the transcript (§3.3), broadcast in a `DISPUTE`, and shown
to the user; the hand does **not** abort on it, no chip moves, the accused is not
unseated, and no key is added to any block list. In **this** version none of that
happens, because nothing builds one. The sentence this paragraph carried — *"The hand
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

> **Nothing produces it (D-015).** No client of this version constructs an
> `EquivocationProof`, and no receiver accepts one: a `DISPUTE` with
> `kind = 2 EQUIVOCATION` is dropped at §4.0 step 11, unopened, with no fault
> recorded. The retention rule that stood here — *"retained as evidence, in
> every phase, at every table, and never silently discarded"* — is the rule of
> the version that produces one, and it is kept in this box for that version
> rather than deleted, because the reason it was stated this widely still holds
> and would have to be restated otherwise.
>
> **Nothing consumes it.** There is no transition, in any document, that takes an
> `EquivocationProof` as its input event. It ends no hand, moves no chip, unseats
> nobody, block-lists nobody, refuses nobody a seat, and produces no
> `AbortRecord`. That sentence is now true twice over — once because D-010 and
> D-011 rule 3 removed every consumer, and once because D-015 removed the
> producer — and **it is the first half that is load-bearing**: a later version
> may reinstate the object, and reinstating it must not reinstate a consumer.
> Whether it is forwarded beyond the table is Q-05, moot in this version.
>
> **There is no wire representation for an equivocation-caused abort and none
> will be added.** §4.10's `cause` enumeration is `1`–`4` and `6`; value `5` is
> deleted and the code point is not reused. **D-014's `cause = 6` is not a route
> back in**, and the reason is the distinction D-014 rests on: an
> `EquivocationProof` shows that a key signed two conflicting things and does not
> show which one was real, so it is an accusation and not a self-authenticating
> illegality — `DECISIONS.md` D-014 excludes it by name (*"no removal on an
> equivocation proof, whose predicate has failed five times"*). An engine alphabet containing an
> `AbortKind::Equivocation`, or a transition producing one, names an outcome no
> legal `HAND_ABORT` can carry, and is a defect in that document.

Why retention is stated this widely **in the version that produces a proof**,
even though nothing follows from it — and it is also why D-015's cost is stated
as loudly as it is:
equivocation is *detected* precisely when things look fine to the detector — it is
two peers comparing what they each received, which is §1.5's forwarding rule
working as designed — so the common case is a proof arriving with no divergence in
sight. An engine that only held proofs raised inside its divergence phase would
discard the strongest evidence this protocol can produce, in the case where it
most often appears, for want of a place to put it. The evidence is the whole point
of producing the proof at all now that no transition consumes it, which is exactly
the question §12 records and does not answer.

#### 5.2.5 What bounds replay and names a double signer in version 1 (NORMATIVE)

**This section is the enforcement authority. §5.2.1 is not.** Every claim in this
corpus of the form *"a replay is stopped"* or *"an equivocation is named"* points
here, and each row below names the check that carries it. There is no slot store
and no eight-tuple lookup anywhere in the client; a document that says otherwise
is stale.

| What | The check that carries it | Where | Disposition |
|---|---|---|---|
| A chained event at the wrong chain, hand, stage or parent | `table_id`, `hand_id`, `sequence`, `previous_event_hash` compared field by field, then `verify_strict` over the **bytes as received** | `net/chained.rs` `open_inner` | rejected and named |
| **Replay of any ordinary chained event of a stage the hand has left** — the principal bound | the monotone stage cursor: `sequence < slot.sequence` | `table/hand.rs` `on_event` | dropped silently; the mesh redelivers as a matter of course |
| An event of a stage not yet reached | `sequence > slot.sequence` | same | **held** in a FIFO bounded at 64, re-judged on every later event |
| **A duplicate contribution at a collective stage** | the per-stage `heard` map: an identical `event_hash` from a seat already heard | `table/stage.rs` `Collective::hear` → `Heard::Again` | idempotent, and pre-checked by `heard(seat)` before anything is spent |
| **A second, different body from one seat at one collective stage** — the equivocation the corpus cares about | the same map: a **differing** hash from a seat already heard | `table/stage.rs` `Collective::hear` → `Heard::Equivocation`; nine call sites in `table/hand.rs` | the **first** stands, the second is `Failed::Equivocation{seat}`; the stage hash is taken over the first |
| Double application at a single-writer stage | the cursor again — `slot` advances synchronously with the effect — plus `action_index` on a betting action | `table/hand.rs` | second copy falls under the cursor rule |
| A redelivered `TIMEOUT_CERT` | a per-hand set keyed on `subject_digest` | `table/hand.rs` `bank_certificate`, and `certified` behind `commit_certificate` | applied exactly once per hand per subject |
| A redelivered `TIMEOUT_VOTE` | a map keyed by voter seat, plus the subject rebuilt from the receiver's own position | `table/hand.rs` `take_vote`, `on_timeout_vote` | overwritten, so it cannot double-count |
| A re-applied `HAND_ABORT` | `Phase::Aborted` short-circuits | `table/hand.rs` | idempotent |
| A stale `PLAYER_LIST` | `list_serial` strictly greater than the held one | `table/join.rs` `admit_list` | refused — **except while the client's own serial is still 0** |
| A re-broadcast lobby advert for a table already held | `timestamp_unix_ms` strictly greater than the held one | `net/lobby.rs` `LobbyStore::offer` rule 6 | refused — **only when an entry is already held** |
| A third party's replay of a `JOIN_REQUEST` | `req.peer_id` must equal the transport-authenticated connection peer id | `table/join.rs` `admit_join` | refused |
| A `JOIN_ACCEPT` replay | bound to the joiner's own outstanding `request_hash`, and `pending` is cleared on the first | `table/join.rs`, `net/formation.rs` | refused |
| Byte-identical frames within 120 s | the GossipSub duplicate cache, `duplicate_cache_time(120s)`, over a 64-bit non-cryptographic id | `net/swarm.rs` | dropped before the application; **also suppresses this node's own re-publish** |

**Three properties of this set are load-bearing and are stated rather than left
to be inferred.**

1. **The cursor is a cursor, not a record.** It bounds replay by refusing to look
   backwards, and it keeps no per-slot history. Nothing in the client can answer
   *"have I seen this exact event before?"* once the stage that held it is gone.
   That is sufficient for the safety property — a passed stage applies nothing —
   and it is **not** sufficient for evidence: a double signer at a *single-writer*
   stage is not named, because the second body arrives under the cursor and is
   dropped without comparison. §5.2.1's key would have named it. Naming it buys
   nothing today, because of point 3.
2. **The equivocation detector is per-receiver and arrival-ordered.** `hear`
   keeps whichever body reached this client first. Two honest peers that received
   different copies each name the other's, and their stage hashes differ. The
   detector reports the split; it does not prevent it, and no slot key would.
3. **Detection has no consumer.** There is no `EquivocationProof` object in the
   client, `Failed::Equivocation` becomes a warning and a GossipSub `Reject` aimed
   at the *relayer* rather than the signer, and **GossipSub peer scoring is not
   configured at all** — no `with_peer_score` call exists. So `Reject` means only
   *"this node will not forward that copy"*. Under D-010 that is the intended
   state: a proof ends no hand, moves no chip and evicts nobody. It is recorded
   here because a reader who assumes otherwise will over-value both this section
   and §5.2.1.

**Prerequisites a future wire-in of §5.2.1 must satisfy, all of them findings
against the tree rather than opinions:**

* **`TABLE_READY` must get a distinct `sequence` per `list_serial`** (§4.11 row
  14), or honest formation traffic collides in the key. This is the blocker.
* **`event_class` 1 and 2 are produced now.** D-023 restored `TIMEOUT_VOTE` and
  `TIMEOUT_CERT` and D-024 gave a certificate a persistent roster effect, so a
  store that refuses those classes — as the deleted one did — rejects exactly the
  messages the certificate path is built from. §5.3's *"not allocated in version
  1"* and §4.8/§8.3's *"defined, not produced"* are stale in the same way.
* **D-024 already installs a narrower dedup key** over those events — *"keyed on
  the subject digest so that … a redelivery of either counts once"* — and does not
  say whether that key is digest-alone or digest-plus-emitter. Two authorities
  over one message class is the condition this section exists to prevent, so that
  ambiguity is closed before, not after.
* **§4.0 step 10a's skip rule interacts with D-024 point 3.** A `kind = 2`
  certificate ends the hand from any position; ending the hand drops the class-0
  store and switches step 10a off for every later event of that `hand_id`. A key
  that stops applying at the moment the roster is edited is not an authority.

---

### 5.3 Bounded anti-replay state

Anti-replay must not become a memory-exhaustion vector (`SPEC_CS.md` §17, §27).

**NOT IMPLEMENTED. No structure described below exists in the client** — the
module that held it, `src/protocol/antireplay.rs`, was deleted with §5.2.1's key
and for the same reason. What bounds replay instead is §5.2.5, whose state is the
per-stage `heard` map, the stage cursor, and the two per-hand sets on
certificates; all of them are dropped with the stage or the hand that owns them,
so the memory-exhaustion question this section opens is answered by construction
rather than by a capacity constant. Retained as the design record a future
version starts from, and as the measurement D-015 rests on.

**The stored state is one structure per `event_class`, and each is indexed by
exactly the components of §5.2.1's key that vary within one `(table_id, hand_id,
sender)` — nothing more and nothing less.** The three fixed components
(`protocol_version`, `table_id`, `hand_id`) are the scope of the structure rather
than part of its index, and `sender_public_key` is the seat. That is the whole
derivation; this section defines no key of its own.

* **`event_class == 0`.** One map per table per hand, keyed by
  `(stage, seat, event_type)` — the **whole `u16`**, exactly as §5.2.1 carries
  it, not a positional slot standing in for it — holding the accepted
  `event_hash`. **The `event_type` axis is not optional**: without it the
  terminal `HAND_ABORT` of §4.10 and its emitter's own contribution to the
  stalled stage collide, which is defect G1.

  **Capacity 2 is a bound on occupancy, not a component of the index, and the
  distinction is the whole of H3.** An earlier revision indexed this structure by
  an `event_type_slot` of capacity 2, which is not §5.2.1's `event_type` and does
  not agree with it: two different `ACTION_*` claimed for one turn, and
  `SHOWDOWN_REVEAL` against `SHOWDOWN_MUCK`, are **two slots** under the key
  (§5.2.1, §4.11 rows 24–25 and 26–30) and would have been **one cell** in that
  store. A receiver would then reject the second at step 10a as a replay, and
  refuse an `EquivocationProof` a conforming client built from the same pair —
  the store contradicting the key it claims to index. Keying by the full
  `event_type` removes the contradiction without costing the bound, because the
  bound never came from the index in the first place: **§4.0 step 12 admits at
  most two `class = 0` events per `(stage, seat)`** — the seat's one
  contribution, plus the terminal `HAND_ABORT`, which is step 12's only
  exception (§4.11 row 36) — and this structure records only events that were
  accepted. So occupancy stays at `MAX_STAGES_PER_HAND × MAX_SEATS × 2` entries.
  `MAX_STAGES_PER_HAND = 2048`; a legitimate hand uses well under 200 stages and
  fills the second entry at most once, in the hand's last stage. A third
  `class = 0` type at one `(stage, seat)` is rejected by step 12 before it
  reaches this structure, so no additional cap is needed here and none is
  imposed. **Two reserved bands sit above `MAX_STAGES_PER_HAND` and both are
  counted here (`L5`):** the hand boundary window at `4 096 … 4 105` (§4.10) and
  the boundary checkpoint's band at `8 192 … 8 207` (§4.9). The window admits at
  most one event per seat per hand and the band at most one `STATE_HASH` and one
  `STATE_ACK` per seat per round over at most eight rounds, so occupancy is
  `MAX_STAGES_PER_HAND × MAX_SEATS × 2` **plus `MAX_SEATS` for the window plus
  `16 × MAX_SEATS` for the band** — 170 further entries per hand at
  `MAX_SEATS = 10`, against 40 960. The bound stated here before this pass was
  computed over indices below 2 048 only and was short by exactly those entries. Sparse rather than dense for the same reason as the classes below: a
  dense `u16` axis would be 65 536 cells per `(stage, seat)` for a pair.
* **`event_class` 1 and 2 — not allocated in version 1 (D-015), and this is the
  measurement the decision rests on.** No structure exists for either class,
  because nothing conforming emits either class and §4.0 step 6 drops both types
  before any store is reached. There is no per-stage map, no lazy allocation,
  and no code path an incoming frame can take to create one.

  **The bound they would have needed, recorded because a later version needs the
  number and because it is the evidence D-015 rests on.** The structures were a
  per-stage map keyed by `(seat, subject_seat)` for votes and
  `(seat, subject_digest)` for certificates, allocated only for a stage at which
  such an event had actually been accepted, at most
  `MAX_SEATS × (MAX_SEATS − 1) = 90` entries per stage per class. Measured over
  `MAX_STAGES_PER_HAND` at `MAX_SEATS = 10`, worst case per hand:

  | class | entries | resident |
  |---|---:|---:|
  | ordinary events (`event_class == 0`, the bullet above) | 41 130 | 2.2 MiB |
  | timeout votes | 184 320 | 9.8 MiB |
  | timeout certificates | 184 320 | 16.3 MiB |
  | **total, had both been allocated** | | **28.3 MiB** |

  At the protocol's limit of eight concurrent table sessions that is ~226 MiB,
  **of which 26 MiB per hand are the two classes above** — the client's largest
  attacker-influenced allocation, held for machinery D-010 had already made
  consequence-free. Not allocating them is the whole of the reduction: the
  section's resident worst case is **2.2 MiB per hand**, the `event_class == 0`
  store plus the constants named in the bullets around it.

  **What a later version must restore with them, stated so it is not
  re-derived.** The certificate map is the larger of the two because its key
  carries a 32-byte `subject_digest` where the vote map carries a `u8`. Both
  must be **sparse** — the dense form is
  `MAX_STAGES_PER_HAND × MAX_SEATS × 2 × MAX_SEATS` slots for a structure a
  legitimate hand populates a handful of times, since the deadline classes only
  ever touch a stage that stalled. And **the subject axis is not optional**:
  without it two votes by one honest voter about two seats at one stage collide,
  which is defect M2 and which §5.2.1's key still carries the axis to prevent.
  §5.2.1 is unchanged by D-015 for exactly that reason — the key stays right
  while nothing populates it, because narrowing it is how five passes went wrong.

**Why the structures are separate rather than one map (R-4), and why that still
matters when only one of them exists.** The sentence that stood here said the
`event_class` axis "is what lets a seat hold both its own contribution at stage
`s` and a `TIMEOUT_VOTE` about stage `s`", immediately after an index that did
not contain `event_class`. That was true in effect and wrong in wording: the
class axis is expressed by *these being separate structures*, not by a component
of any one index. The separation is the axis. **Under D-015 one of the three is
built and two are not**, which is why the `event_class == 0` store's index above
still omits `event_class`: it is the only structure, so the separation is
trivially satisfied, and a later version that adds the other two adds structures
rather than an index component. An implementer who "simplifies" by folding the
class into a single map has pre-committed the version-2 defect.
* Per table, per hand, per sender: a count of accepted distinct `DISPUTE`s and
  their `event_hash`es, bounded by `MAX_DISPUTES_PER_SENDER_PER_HAND = 8`
  (§4.9). `DISPUTE` is unchained, so it has no slot in the array above and needs
  its own bound; without one, the one message type that is legal at any time
  would be the one unbounded allocation in the section.
* A hand exceeding `MAX_STAGES_PER_HAND` aborts with `cause = 4`. **The two
  reserved bands are exempt from that abort and this is normative (`L5`):** the
  count is of stage indices **below** `BOUNDARY_SEQUENCE_BASE`, and a `sequence`
  in `4 096 … 4 105` or `8 192 … 8 207` is checked against §4.10's and §4.9's own
  rules instead — one boundary event per seat, one checkpoint-8 pair per seat per
  round, `r <= 7` — which bound those bands by construction and much more tightly
  than a stage count would. Without the exemption an implementer who range-checks
  `sequence < MAX_STAGES_PER_HAND` rejects **every** boundary event and every
  boundary checkpoint, which is `Q-09`'s closure and `K-3`'s closure both undone
  by a comparison.
* Finished hands keep only `TERMINAL(k)`, the terminal body — the `HAND_COMPLETE`
  body, or the accepted `HAND_ABORT` copy — the **retained hand record** below,
  and the **checkpoint-8 `STATE_ACK` slots** of the paragraph after it, in memory;
  the full transcript is written to the profile directory and dropped from RAM.
* **The `event_class == 0` store is dropped when its hand ends, and §4.0 step 10a
  is skipped for a chained event that names a hand already finished — normative,
  and this is the memory half of `N2`.** The two are one rule read from two ends:
  there is no store for a finished hand, so there is nothing to look such an event
  up in, and **no store is created for one**. A sender therefore cannot cause an
  allocation by choosing a `hand_id`, which is what the first sentence of this
  section forbids and what the rejected reading — *allocate the named hand's map
  and look there* — would have handed to any peer that can send one frame.
  §4.0 step 10b is the whole of such an event's anti-replay rule and its four
  dispositions are idempotent, so nothing accrues per repeat either; §4.0 states
  what the skip gives up, which is an equivocation detection that the dropped
  store had already taken away.
* **The checkpoint-8 `STATE_ACK` slots outlive their hand, and they are the only
  thing that does (`N6`, §4.9).** For the most recent finished hand `k`, this
  receiver keeps the `event_class == 0` entries of chain `k` at `sequence`
  `BOUNDARY_CHECKPOINT_BASE + 2r + 1`, `0 <= r <= 7`, until `TERMINAL(k+1)` is
  fixed. **Bound:** `8 × MAX_SEATS = 80` entries, for **one** hand at a time — the
  slice for hand `k` is dropped when hand `k+1`'s is created — so the addition to
  this section's occupancy is 80 entries and a constant, against 40 960. Nothing
  else of a finished hand's store is retained, and an event arriving at any other
  `sequence` of a finished chain takes the skip above.
* **The readmission set `A` (§4.9, `N5`, `P2`).** One `SeatSet`,
  `|A| <= MAX_SEATS`, written by §4.0 step 10b and read and cleared at the next
  hand init, where it widens that stage's **accepted** emitter set and not its
  required one. It is not indexed by anything, holds no event and no hash, and is
  the smallest structure in this section. **Its write is reachable by replay and
  that is why its read may not enlarge a required set**: step 10a is skipped for
  the stale events that write it (`N2`), so nothing suppresses a second copy, and
  this section's LRU is exactly what keeps such a copy effective — a retained
  record is what makes a stale event evaluable, so the replay window and the
  retention window are the same 4 096 hands. Under the deleted rule that was
  4 096 stalled hands from one forwarded, agreeing event.
* **The retained hand record — normative, and this is the memory half of `L4`.**
  For each finished hand, this receiver keeps four quantities and nothing else:

  ```
  RetainedHand  { hand_id: u64,
                  was_solitary: bool,        // §3.2's regime test in the past tense, and it is
                                             //   the DISJUNCTION:
                                             //   P(hand_id - 1) == {self} || P(hand_id) == {self}
                  p: SeatSet,                // P(hand_id - 1) — the required emitter set OF the
                                             //   hand named, which is the set §3.2's rule and
                                             //   §4.0 steps 10b and 12 all test against (N7)
                  checkpoint8_state_hash: bytes[32] }   // §4.9, this peer's own value
  ```

  **`p` is `P(hand_id - 1)` and not `P(hand_id)`, and this is the disposition of
  `N7`.** §3.2's solitary-stage rule and §4.0 step 12 both test *"a seat outside
  `P(k-1)`"*, because `P(k-1)` is what hand `k`'s stages were **required of** — a
  seat outside it is a seat this receiver did not expect to hear from in hand `k`,
  which is the whole content of the contradiction. `P(hand_id)` is a different set:
  it is who signed **into** chain `k`, it is grown by the two exempt events on
  purpose (§4.9's out-of-set checkpoint-8 copy, §4.10's `PLAYER_SIT_IN`), and
  testing against it would silence the rule for exactly the seats those exemptions
  admitted. The two coincide in the pure solitary case, which is why the wrong one
  survived a pass; they diverge the moment an exempt event lands. One set, one
  name, tested at one place, which is D-011 rule 2's discipline applied to this
  record. **`was_solitary` is *not* `p == {self}`** and is deliberately an
  independent bool: §3.2's regime test is the disjunction
  `P(hand_id - 1) == {self} ∨ P(hand_id) == {self}`, because the two freeze routes
  ask about two different sets — the membership route about the hand this peer
  derived, the checkpoint-8 comparison route about the boundary it is about to deal
  from. On K1's own walk the hand `P` narrows in satisfies only the **second**
  disjunct, so a record that derived the bool from `p` alone would drop the
  earliest detection this document has. Two fields, because two routes; §5.3 keeps
  the bool rather than the second set, since nothing else reads it and the record
  stays at four fields and 56 bytes.

  It is read by §4.0 step 10b and by nothing else, it is never an input to a hash
  or to a stage, and it is a pure function of the events this receiver accepted —
  no clock, no timer, no arrival fact. **It exists because K1's contradicting
  event is late by construction** (§3.2): a solitary hand self-completes every
  stage, so the copy that proves this peer is not alone reliably names a hand this
  peer has finished, and without a record to evaluate it against the only
  available reading of §4.0 step 12 is *"not expected at this `sequence`"*, which
  is a drop, which is precisely what §3.2 says that event must not be.

  **Bound:** an LRU of `MAX_RETAINED_HAND_RECORDS = 4 096` hands, at 56 bytes each
  — under 256 KB, against a `MAX_STAGES_PER_HAND` map that is two orders larger.
  A hand whose record has been evicted answers *"not solitary"* and its late
  events are dropped as they were before this pass; §3.2 states what that costs
  and why it is not K1's case.

  **What making a stale-hand event evaluable does not reopen, checked because the
  last four passes were caught on exactly this question.** §5.2's equivocation
  predicate is unaffected, because a step-10b event is never applied and never
  enters a `stage_hash`. The `event_class = 0` map above is unaffected, because
  the record is not that map and is not indexed by `slot(E)`. §4.0's
  cheap-before-expensive ordering is unaffected, because step 10b sits before step
  11 and reads only envelope fields. The one thing it opens is a memory bound, and
  it is bounded in the paragraph above, in the section that already owns that
  discipline.
* The lobby keeps at most `MAX_TRACKED_TABLES = 512` `(table_id, last_timestamp)`
  pairs in an LRU, and at most `MAX_TRACKED_PRESENCE = 8192` peers.

---

## 6. `STATE_HASH`, `STATE_ACK` and divergence (`SPEC_CS.md` §15)

### 6.1 What is hashed

```
state_hash = h("p2p-poker v1 state", [ canonical_cbor(PublicTableState) ])
```

`PublicTableState` is a `#[cbor(array)]` struct containing exactly:

> **Normative, and this is `P7-w`.** The table below is the **field order**, read
> top to bottom, left to right within a cell. `#[cbor(array)]` means the order
> **is** the encoding, so this table is not a summary of the struct — it is the
> struct. It is the only field-order statement this document makes: §2.9's sweep
> deleted its own second list rather than align it, and `STATE_MACHINE.md` §3.3
> adopts this table for the engine (D-011 rule 1). An implementer transcribing
> `PublicTableState` into `src/protocol/messages.rs` reads this table and nothing
> else.

| Field | Notes |
|---|---|
| `protocol_version`, `table_id`, `hand_id`, `checkpoint` | |
| `roster` | `Vec<(seat, app_public_key, stack)>`, ascending by seat |
| `button_position`, `sb_position`, `bb_seat` | positions, which may be empty seats [RULES A1.3] |
| `level`, `small_blind`, `big_blind`, `ante` | |
| `street` | `u16` |
| `board` | `Vec<u8>` card codes, length 0/3/4/5 [RULES A2] |
| `committed_this_round`, `committed_this_hand` | `Vec<u64>` by seat |
| `folded`, `all_in`, `acted_this_round`, `sitting_out` | `Vec<bool>` by seat |
| `current_bet`, `last_full_raise` | |
| `player_to_act` | `Option<u8>` |
| `pots` | derived `Vec<PotView { size, eligible }>` [RULES A7] |
| `deck_commitment` | `final_deck_hash` from `DECK_COMMIT`, or 32 zero bytes before it |
| `ledger_in`, `ledger_out` | `u64` each; the running totals of accepted buy-ins and of removed stacks (§4.4's `ledger_delta`). They are inside `state_hash` because otherwise two peers could disagree about the ledger and never detect it (C-9) |
| `transcript_head` | the `stage_hash` the event carrying this state hash chains from -- its `previous_event_hash`. At checkpoints 2 to 7 that is the last completed stage. At checkpoint 8 the state is hashed inside `HAND_COMPLETE`'s own body, which cannot contain its own stage's hash, so it is the last stage completed **before** the settlement: the stage the settlement chains from. The `STATE_HASH` frame's field of the same name is one stage later, `TERMINAL(k)` (§4.9). Corrected 2026-09-18 (`S1-CJ`): this row read *the last completed stage*, which no implementation could satisfy at checkpoint 8; the value every peer hashes is unchanged |

**`signed_this_hand` WAS field 28 of this struct, and it is deleted. The
argument for it was sound and its premise was false, and the difference was only
visible under measurement.**

The argument ran: every other quantity the boundary checkpoint covers — the
stacks, the button, the level, the ledger — is a function of the previous
boundary and already bound into `GENESIS(k)`, so two honest peers cannot hold it
differently without having already rejected each other's `HAND_INIT`. The
participation set is the one input to hand `k+1` that two honest peers can hold
differently and never notice (§3.2's `Q-10` residual), so putting it in
`state_hash` turns a quantity each peer reads alone into one they compare.

**The false premise is that the two peers can be made to agree about it.** The
rule that set the vector was *this peer accepted an event of hand `k` signed by
that seat*, and this document defended that as *"a fact about the accepted chain
and not an observation about a connection"*. It is a fact about the accepted
chain **of one peer**. On a link that loses packets two honest peers accept
different sets, so they must differ here, and a hash containing it can never be
made to agree. The comparison did not detect a divergence; it manufactured one.

**What that cost, measured on ten-seat two-machine runs.** 124 refused
settlements in one nine-minute run and 553 in another, and in **100 % of them the
differing field was this hash** — while the number differing in stacks, deltas,
pots, refunds or busted seats was **zero**. The money agreed everywhere, always.
A refused settlement leaves the seat still awaited, so its action clock runs out
and it is certified out for a silence it did not commit: **35 % of all timeout
accusations named a seat heard at the very stage it was accused of ignoring**. It
could then never return, because §4.9's readmission wants a `state_hash` that
agrees about who took part in a hand the seat did not take part in — readmissions
in a whole run: zero. And §6.3's freeze, the terminus this document provides for
exactly this disagreement, fired **zero** times against those 124, because the
refusal that detects the disagreement is also what stops the checkpoint being
reached. One hand ended by abort on every node with the pot never awarded, which
is `D-026` violated by leaving things alone.

**With the field removed, measured on the same rig the following day:** refused
settlements **124 → 0**, false timeout accusations **14 → 0**, readmissions
**10 → 20**, hands opened by all ten seats **16 → 19**.

**What still guards what it was guarding.** Chip conservation never rested on it:
`HAND_COMPLETE` carries the pots, the deltas, the final stacks, the refunds and
the busted seats, and every peer recomputes and compares that whole body
independently of this hash. A participation-set divergence still surfaces, one
stage earlier and in a form two peers *can* agree about: it changes `dealt_in`,
`dealt_in` is `HAND_INIT`'s `n(8)`, and `HAND_INIT` bodies are compared whole at
stage 0. The difference is that a `HAND_INIT` body contains no observation of the
listener.

**What is genuinely weaker, stated rather than glossed.** §4.9's out-of-set
checkpoint-8 admission was justified by this field and is now justified by the
twenty-eight that remain — see that section's box. That is a weaker claim and it
is written there as one.

**The lesson worth keeping is about the shape, not this field.** A value every
peer must agree on may contain only quantities every peer derives identically. A
predicate with *this peer* in it is not such a quantity, however factual it is
about that peer. The engine maintains the same set under the same name for its
own use (`STATE_MACHINE.md`
§2.8, §5.3 step 4) and neither document restates the other's derivation (D-011
rule 1).

**Adding it is a wire change on the same footing as the `absent` deletion below.**
`PROTOCOL_MAJOR = 1` has not shipped and no peer is emitting this struct, so it is
a revision of version 1's definition rather than a break; it must land before the
first release, and after that the same edit would need a major bump (§10.2). It
changes `state_hash` for every peer at every checkpoint, which is why
`src/protocol/messages.rs` must not write `PublicTableState` before this field is
in it.

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

**Every per-seat flag in this struct must be a deterministic function of accepted
chained events, and D-012 is what makes that a requirement rather than a
convention.** `folded`, `all_in`, `acted_this_round` and `sitting_out` are the
four, and each is a `Vec<bool>` by seat. The bullet above already excludes what a
peer merely *observes*; this states the same rule from the other side. A seat's
state must not be derived from how long that seat has been quiet at this receiver,
from a dropped connection, from a relay status, or from any field of an accepted
`HAND_ABORT` — §4.10 forbids the last of these outright, and the general form is
D-012. Two honest peers with the same accepted chain must compute the same four
vectors; if they cannot, the checkpoint reports a divergence that neither caused
and §6.3 ends a hand for nothing. Which chained events set them is
`STATE_MACHINE.md`'s (D-011 rule 1) and is not restated here; that they may have
no other source is this section's.

**The fifth vector, `absent`, is deleted from `PublicTableState` and therefore
from `state_hash` (J5, D-013).** It was the one this paragraph used to single out
as "the one that could be got wrong", and it was permanently `false` at every seat:
no transition in `STATE_MACHINE.md` enters `SeatStatus::Absent`, which is what
D-012's deletion of T46's status derivation left behind. A vector that is always
false forks nothing, so this is not a correctness fix — it is the removal of a
**trap**, and the trap is specific. An implementer who finds a status the engine
never sets, sees it hashed into every checkpoint of every hand, and wires it to the
obvious local signal — a dropped connection, a peer that has been quiet a while —
has forked the chain at every checkpoint, silently, and §6.3 then ends hands for
nothing. Keeping the field with a comment saying it is unreachable was the cheaper
option and it is refused, because **D-013 removes the reason the field existed at
all**: liveness is inherited from `P(k)` (§3.2), and no rule in this document now
needs to know whether a seat is "away". A field that nothing sets, that no rule
reads, and that is hashed into the consensus-critical path is dead weight on the
one path where dead weight is dangerous.

Deleting it is a wire change, and it is a revision of version 1's definition rather
than a break, on exactly the footing §4.10 states for `HAND_ABORT`'s `cause = 5`:
`PROTOCOL_MAJOR = 1` has not shipped and no peer is emitting this struct, so it
must land before the first release and after that the same edit would need a major
bump (§10.2). `SeatStatus::Absent` itself and its remaining readers are
`STATE_MACHINE.md`'s (D-011 rule 1) and are filed in `DECISIONS.md`'s open list as
J-3. What **this** document is normative about is that no vector named `absent` is
hashed here, and that none may be re-added.

### 6.2 Checkpoints

`STATE_HASH` is emitted as a collective stage, followed immediately by a
`STATE_ACK` collective stage, at these points and no others — plus, when a
divergence is being resolved, the **reconciliation stage** of §4.9 and §6.3 step
3, which re-derives one of these eight and carries its `checkpoint` number rather
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
| `8` | **the boundary checkpoint** — the moment `TERMINAL(k)` is fixed at this peer, on **every** hand and on both terminal paths. Its chain, parent, `sequence`, total order, required emitter set and the wider set it is *compared* over are §4.9's checkpoint-8 box; what it covers is `STATE_MACHINE.md` §5.2's |

**Row 8 is new and it is the wire half of `K-3` (L2).** The table read `1`–`7`
under the words *"at these points and no others"*, which made a conforming
receiver reject the one checkpoint `STATE_MACHINE.md` §5.2 requires of every hand,
and the cost of that rejection was not "a missing comparison": T47's settled-path
gate never discharges, so **T61 fires at `hand_deadline_ms` after every settled
hand** — one whole `hand_deadline_ms` every two hands on a table where nothing is
wrong, and since `P3` that figure scales with the seat count, which is
D-013's own fixed point re-created by the fix for the defect D-013's fixed point
left behind. Checkpoints `2`–`7` all need a `DECK_COMMIT` or a betting round, so
**two hand shapes place none of them**: a drain hand, which `STATE_MACHINE.md`
§5.3 step 9 sends straight to `Settling`, and a hand that stalled at `HAND_INIT`
and aborted. Those two shapes are D-013's steady state — about forty consecutive
drain hands against a silent opponent (`STATE_MACHINE.md` §12.1.2) — and the
tournament result itself sat at the end of them, never checkpointed. Row 8 places
a comparison on every hand of every shape, including the last one.

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
checkpoint it settles rather than chosen by an emitter. **Its required emitter set
is not the re-derived checkpoint's** — it is that set enlarged by the seats whose
signatures froze this receiver, so it can never be satisfied by one peer alone
(§4.9). That floor is what makes row 8 worth placing: a boundary checkpoint on
every hand gives the freeze a stage to chain a resolution from, and a resolution
stage a lone peer could complete would have handed the freeze straight back.

A checkpoint after the cards are known **to more than their owner** is an abort
trigger the loser can pull with full information: a peer that publishes a
`state_hash` it did not derive forces the §6.3 case (c) path, and until this fix
checkpoint 7 sat immediately before `HAND_COMPLETE`, which is after the showdown.
Moving it before `SHOWDOWN_REVEAL` does not close the exploit — see §6.4 and X29 —
but it forces the attacker to commit to burning the table while it still knows
only its own hand. That is the property the rule protects, and knowing one's own
two cards does not threaten it: every seat has that knowledge, symmetrically, from
`DEAL_PRIVATE` onwards.

**Checkpoint 8 sits after the showdown and is the one checkpoint that rule does
not reach, and the reason is that it has nothing left to pull.** Every other
checkpoint is inside a live hand, so faulting the table at it voids **that** hand
and returns the attacker's commitment — that is the whole shape of the exploit
§6.4 and X29 carry. Checkpoint 8 is emitted only once `TERMINAL(k)` is fixed, and
on the settled path `TERMINAL(k)` is the `stage_hash` of a **collective**
`HAND_COMPLETE` stage: every seat of `P(k-1)` recomputed every pot, every delta
and every final stack and signed a byte-identical copy, and a mismatching copy was
rejected at the stage (§4.10). Hand `k`'s settlement is therefore already agreed,
already chained and already final when the checkpoint opens, and a peer that
publishes a false `state_hash` for it cannot reopen it — it can only fault the
table for the hands that have not happened, at the cost of its own equity in them,
with no pot recovered. **Which hand the resulting abort disposes of, and how a
boundary divergence is settled without double-restoring hand `k`'s commitments, is
`STATE_MACHINE.md`'s** (D-011 rule 1, T50/T54/T61): what this section is normative
about is that hand `k` is not among the hands it can reach.

The one thing checkpoint 8 does place after the cards are public is a comparison
of the **boundary** state, and that state contains no card: §6.1's `board` is a
public vector and no hole card, reveal token or deck secret enters `state_hash` at
all (§3.4). So the rule's *stated* property — no card opens while peers disagree —
is untouched by row 8, because row 8 opens nothing and is placed when there is
nothing left to open.

`HAND_COMPLETE` needs no checkpoint of its own, because it is now a collective
derived stage (§3.2, §4.10): every receiver recomputes every field, and a
mismatching copy is rejected at the stage rather than caught later at a
checkpoint. **Row 8 is not that checkpoint and does not reopen the question.** It
was a comparison of the one boundary quantity `HAND_COMPLETE`'s collectivity
cannot police: `signed_this_hand`, which is not a field of `HAND_COMPLETE` and
which two peers that have each stopped expecting the other never exchange at
all. **Field 28 is deleted and row 8 no longer compares it.** What row 8
compares now is the settled state — the transcript head, the roster with its
stacks, the deck commitment and the board — which two peers that have stopped
expecting each other still cannot both produce from a forked chain. It no
longer separates a fork in the *accepted* set alone; §4.9's box says why that
case is now admitted rather than faulted. That is
also why row 8 is placed on the aborted path, where there is no `HAND_COMPLETE`
stage to recompute anything.

`STATE_HASH` publishes each peer's own computed hash. `STATE_ACK` then confirms
that this peer saw the complete set and that every member of it agreed — giving a
definite, chained "we all agreed here" point that a later dispute can name. The
two-stage form matters: a peer that publishes a matching `STATE_HASH` but then
refuses to `STATE_ACK` is stalling, and is handled by §8 rather than by ambiguity.

A `BOARD_REVEAL` stage runs only after the preceding street's `STATE_ACK` stage
completes, **where one was placed**. That is what guarantees no card opens while
peers disagree about the state.

**The three words are `L9` and they are not a softening.** Without them the gate
is unsatisfiable on the **all-in run-out** — `STATE_MACHINE.md` T38's path, where
every remaining seat is all in and the betting phase for a street is skipped. A
skipped betting round closes no round, so rows `4`–`6` place no checkpoint, so
there is no preceding `STATE_ACK` for the turn's or the river's `BOARD_REVEAL` to
follow and neither street can open. That is a deadlock on one of the two most
common hand shapes in no-limit hold'em, reached by two honest peers doing exactly
what this document says. The gate as amended reads: a `BOARD_REVEAL` may not run
while a checkpoint that **was** placed for the preceding street is unacknowledged;
where no checkpoint was placed there is nothing to wait for, and nothing is
weakened, because the property the rule protects is *no card opens while peers
disagree about the state* and a street that placed no checkpoint produced no
disagreement to be waiting on.

### 6.3 Divergence: the resolution procedure

"The host is always right" is explicitly forbidden (`SPEC_CS.md` §15). There is no
host. The procedure is:

**Step 1 — freeze.** The moment any peer observes two distinct `state_hash`
values at one checkpoint, it stops accepting and stops emitting hand events. No
card opens, no action is applied, no chips move. Silently continuing is what §15
forbids.

**There is a second entry condition and it is normative (K1).** A peer that
**was** in the **solitary regime** for hand `k` (§3.2) — every collective stage of
that hand required of `{self}` alone — freezes here on receipt of any chained
event of hand `k` from a seat outside `P(k-1)`, other than the two exempt cases
§3.2 names, that reaches §4.0 step 12 or step 12 by way of step 10b. The event
**is** the divergence: it is another seat's signature on the proposition that it
is participating in a hand this peer believes it played alone. **The freeze is
unilateral and complete on its own** — it needs no reply, no quorum and no
reconciliation, and it holds even if the contradicting peer never speaks again.
Steps 2 to 4 then run as written and may recover the hand; if they cannot, case
(b) or case (c) disposes of it. What the freeze guarantees regardless is that this
peer completes no stage alone, awards no pot, and reaches no §9.3 end condition
while another seat is signing against it.

**Both routes into this step carry the same finding when the hand was solitary,
and that is `N1`'s second half.** A checkpoint-8 `STATE_HASH` of hand `k` whose
value differs from this receiver's own, where §5.3's retained record says this
receiver derived hand `k` with a required emitter set of `{self}`, reaches §4.0
**step 12a** and not this step alone (§4.9's box states the wire rule and the
argument). The two routes were specified as if they were two dispositions — the
membership route latched, the comparison route did not — and the interleaving
where only the checkpoint-8 copy arrives therefore froze, timed out, thawed
through the abort, dealt another solitary hand and froze again, indefinitely,
with no invariant able to see it. The engine consequence is one latch for both
routes and it is `STATE_MACHINE.md`'s (D-011 rule 1); what this document owes is
that both routes deliver one finding, and they now do.

**Both halves of that sentence changed in this pass and both changes matter.**
The condition read *"a peer in the solitary regime … any chained event of this
hand"*, present tense on the receiver's own current hand, and a solitary hand is
traversed in microseconds because every stage of it self-completes, so the
contradicting copy reliably arrived at a receiver that had moved on and was
dropped for being late (`L4`). It is now a property of **the hand the event
names**, answered from §5.3's retained record by §4.0 step 10b, and it fires
whenever the event arrives. And the justification this paragraph carried — *"there
is no checkpoint on that path to compare hashes at … that is `K-3`"* — is
**withdrawn**: §6.2 row 8 now places the boundary checkpoint on every hand
including a drain hand and an aborted one, so the checkpoint route exists.

**The two triggers are complementary and neither replaces the other, which is why
both are kept.** Checkpoint 8 compares only what actually arrives: a genuinely
solitary peer's checkpoint-8 stage has `required = P(k) = {self}`, so it completes
alone and compares nothing, and the comparison happens only if the other peer's
copy reaches it — which §4.9's box makes admissible and which the freeze does not
need. The solitary-stage rule fires on the **first** contradicting chained event
of any type, and is the only one of the two that fires on an event the receiver
cannot compare at all. Checkpoint 8 is the route that heals a table where the two
peers agree; the freeze is the route that stops one where they do not.

**Which phase the engine is in while frozen, and which transitions carry
steps 2–4, is `STATE_MACHINE.md`'s** (D-011 rule 1); the sentence that named the
phase and mapped the steps onto its transitions is deleted. What this section
specifies is the **messages** the procedure emits and the **stages** they occupy,
and nothing else.

**Step 2 — declare.** Every peer broadcasts `DISPUTE { kind = 1 STATE_DIVERGENCE,
at_sequence = the checkpoint stage, table_id, hand_id, evidence = its own
STATE_HASH event }` and, on request, its full transcript for the hand over the
table mesh.

**The embedded `STATE_HASH` is an observation for step 1's purposes, and this is
now load-bearing rather than incidental.** A recipient whose own copies all agreed
did not observe the divergence and would otherwise take no part in the procedure;
the dispute is what puts a second, independently verifying value at that
checkpoint in front of it, and §4.9 already says what makes that admissible — *a
dispute is a carrier, and what carries weight is what it contains*. Such a
recipient enters this procedure at step 1 and adds the evidence's signer to its
contradiction set `W` (§4.9). The embedded event is **not** applied, occupies no
slot, enters no `stage_hash` and completes no stage: a `DISPUTE` is unchained
(§2.3) and nothing carried inside one ever becomes a chained event by being
carried. This costs the emitter no new obligation — step 2 was already mandatory
for every frozen peer — but it is why §4.9's reconciliation stage can complete at
a table where only two of the seats saw the mismatch directly.

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
reconciliation stage §4.9 defines.

**Which checkpoint is the disputed one, for each of the two entry conditions.**
On the comparison route it is the checkpoint the two `state_hash` values were
observed at, which the `STATE_HASH` payload's `n(0) checkpoint` names. On the
**solitary-stage** route the event that froze this peer is not a `STATE_HASH` at
all, and the disputed checkpoint is **checkpoint 8 of the hand that event names**
— the only checkpoint a solitary hand places (§6.2 row 8), the one whose
`stage_hash` the reconciliation round chains from, and the one whose body was
the participation set the contradiction is about until field 28 was deleted
(§6.1). Its body is now the settled state, which is what the reconciliation
round compares. Without that
sentence the solitary freeze had no checkpoint to name and therefore no stage to
be released by, which is the other half of what made it releasable by nothing but
a timer. That stage is the artefact the engine consumes:
the divergence is **resolved** when the stage completes carrying one value, and
**unresolved** when it completes carrying two, and the engine needs no other
signal.

**What releases the freeze, exhaustively, because the previous revision left it
to be inferred and the inference was wrong (`N1`).** The freeze of step 1 — and
the latch a solitary-regime divergence sets with it — is released by **exactly one
thing: a reconciliation stage that completes over `R(c) ∪ W` with every value in
it agreeing.** Since `|R(c) ∪ W| >= 2` (§4.9), completing it requires at least one
seat **other than this receiver** to have signed the same re-derived value, which
is the only evidence that separates *a gap this peer could fill* from *a fork it
cannot*, and it is precisely what the freeze exists to wait for.

**Nothing else releases it.** Not a timer; not the abort of the hand it froze in;
not `TERMINAL(k)`; not a later hand's boundary checkpoint; not a `PLAYER_SIT_IN`;
not §4.9's readmission set; not the table closing. A peer whose contradicting seat
never speaks again therefore **stays frozen**: the hand it was in is disposed of
at `hand_deadline_ms` by §8 like any other stalled hand and every stack is
restored, **and the latch is still set at the next boundary**, where
`STATE_MACHINE.md`'s end condition for it closes the table. That is the terminus
of the silent branch, and it is stated as an outcome rather than left implicit:
**the table closes, no further hand is dealt, and no end condition names a
winner.** It is §6.4's shape reached for a better-evidenced reason — this peer
holds another seat's signature against its own account of a hand it believed it
played alone — and it is what §3.2's *"a table that stops is the price"* costs
when the contradiction cannot be reconciled.

**The two completing branches are unchanged.** One value: the divergence was a
gap, the peer resumes at the phase it held, and the latch clears with it. Two
values: case (c) below, `cause = 4`, the table is faulted — at which point the
latch is moot, because no further hand is dealt either way.

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
| `n(2) preset_id` | `bytes` | **a closed two-value enum in version 1: `"RATED_SNG_POKERTH_V1"` or `"CUSTOM"`, and nothing else.** Any other byte string — including a name a client recognises from its own source — is rejected by rule 3 below. This is `G6-R6`; the ≤ 32 B ASCII bound remains as the parse limit and is no longer the admission rule |
| `n(3) table_name` | `bytes` | ≤ 64 B UTF-8, no control characters; display only |
| `n(4) small_blind` | `u64` | ≥ 1, **and `== n(13) first_small_blind`** — this is a separate part of `table_params_hash` from the schedule's, and it is the level-1 blind because an advertised table has dealt no hand: it is withdrawn when it starts (§7.3 `reason = 1`) and rule 7 discards a re-broadcast whose parameters changed (`G7-S3`) |
| `n(5) big_blind` | `u64` | `== 2 * small_blind` |
| `n(6) ante` | `u64` | `0` in version 1 |
| `n(7) min_buyin` | `u64` | ≥ `big_blind`, **and `== n(9) start_stack` in a tournament mode** (`G7-S3`) |
| `n(8) max_buyin` | `u64` | ≥ `min_buyin`, **and `== n(9) start_stack` in a tournament mode** (`G7-S3`) |
| `n(9) start_stack` | `u64` | tournament modes only; `0` for cash. In a tournament every entrant receives this stack, so the buy-in **is** the stack and `n(7) == n(8) == n(9)`; without that rule the two buy-in fields are free parts of `table_params_hash` that a named configuration has to pin one at a time, which is how `G7-S3` happened |
| `n(10) players` | `u8` | currently seated, ≤ `max_players`; advisory |
| `n(11) max_players` | `u8` | `2 ≤ max_players ≤ 10` [RULES B1] |
| `n(12) min_players_to_start` | `u8` | `2 ≤ … ≤ max_players` |
| `n(13) blind_schedule` | `BlindSchedule` | see below |
| `n(14) action_timeout_ms` | `u32` | `5_000 ≤ … ≤ 300_000` [RULES B3] |
| `n(15) action_grace_ms` | `u32` | `≤ 30_000` |
| `n(16) crypto_step_timeout_ms` | `u32` | `30_000 ≤ … ≤ 58_000` — **the carrier's repair window, not an arbitrary range.** The floor was `1_000`, which is a stage that must close in a second on a carrier whose blind repair ladder reaches T+33 s: every seat whose datagram the wire refused is voted out for a message that was on its way (`S1-BK`). The ceiling is `GC_CONFIRMED_PEER_TIMEOUT`, past which the peer is no longer in the group and a longer budget waits for nobody. Both are mirrored in `constants.rs` beside static assertions, and the ladder they are derived from is a checked patch marker. |
| `n(17) hand_deadline_ms` | `u32` | `HAND_DEADLINE_MIN(n(11)) ≤ … ≤ 3_600_000` — the lower bound is derived, not a literal (`P3`, and raised by one `REOPENING_COST` in this pass by `G5-Q6`); it is a function of `n(11)`, `n(14)`, `n(15)`, `n(16)` and `n(19)`, it is derived in §8.2, and an advert below it is rejected by rule 2a below. A single figure here made legal play at six seats and up abort itself, and the floor alone let the first re-raise do the same |
| `n(18) join_deadline_ms` | `u32` | `≤ 3_600_000` |
| `n(19) hand_delay_ms` | `u32` | `≤ 60_000` |
| `n(29) time_bank_ms` | `u32` | `≤ TIME_BANK_CAP(n(11))`, the per-hand thinking reserve every seat may spend on top of `n(14)`. **Inside `table_params_hash`.** It is a table parameter rather than a client's own setting because every peer adds it to a betting stage's `next_deadline_ms` (§8.2) before it will vote that a seat is late: if each client chose its own, a generous client would have its hands aborted by a stingy one, and — the outcome that matters — the table would certify a player who was still legitimately thinking. Every peer budgets the **whole** reserve for every other seat, never what that seat has left of it, because what a seat has left is known only to that seat. `0` disables it, which is what §13's preset carries |
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
`DOUBLE_EVERY_N_HANDS`, under which
`small_blind(h) = min(first_small_blind · 2^(⌊(h-1)/every_n_hands⌋), small_blind_cap)`
and `big_blind = 2 · small_blind`. **`RATED_SNG_POKERTH_V1`'s values for these and
for every other part are §13's and are not restated here** (D-011 rule 2, and
`G7-S3`: this paragraph carried a third copy of four of them, one of which called
`n(11)` *"seats"*). Every one of §13's rated numbers is enforced by PokerTH's own
rated-game settings check, which is the strongest available evidence of what "the
rated preset" means. [RULES B1, B2, B4]

*Receiver must validate, before the advert is shown to a user or stored:*

> **These numbered rules are §7.2's and are cited as *"§7.2 rule N"*.** Six sites
> in this document, and several outside it, cited the same list as *"§9.4 rule
> 2a"* / *"rule 3"*; §9.4 is *Collection bounds* and contains no numbered rule —
> this document's six are corrected, the rest are filed (`G6-R7`). The rules
> below are the joiner's admission check and are the only place `preset_id` and
> `hand_deadline_ms` are validated against anything.

1. `verify_strict` passes under `sender_public_key`, which is taken as the
   `table_public_key`; the envelope carries the unchained sentinels of §2.3;
2. every numeric range above, including `big_blind == 2 * small_blind`,
   `min_buyin ≤ max_buyin`, `2 ≤ max_players ≤ 10`,
   `min_players_to_start ≤ max_players`, **`small_blind == blind_schedule.first_small_blind`**,
   and, in a tournament mode, **`min_buyin == max_buyin == start_stack`** (`G7-S3`);
2a. **the whole-hand deadline admitted minimum of §8.2 — `P3` and `G5-Q6`, and it is
   the one range check in this list that is a *derived* bound rather than a
   literal.** `n(17) hand_deadline_ms >= HAND_DEADLINE_MIN(n(11))`, computed from
   this advert's own `n(11)`, `n(14)`, `n(15)`, `n(16)` and `n(19)`. Below
   `HAND_DEADLINE_FLOOR`, **every legal hand at this table aborts on its own
   deadline** with `cause = 1` and `attributed = []` — no progress, nobody named.
   Between the floor and the minimum the table buys the walk and no reopening, so
   **the first re-raise reaches that same abort** (`G5-Q6`), which is why the
   admitted bound is the floor plus one `REOPENING_COST(n(11))` and not the floor.
   Either way the advert is rejected here and the seat is never taken. The check
   belongs to the joiner because the value is the **founder's**, signed into
   `table_params_hash`; a founder who wants a table where nothing can ever be won
   needs no attack, only a small number. A client that joins anyway and applies
   its own bound locally has made two peers disagree about whether a hand aborted,
   which §8.2 classes as a consensus fault;
3. **`preset_id` is one of exactly two byte strings and its value implies the
   whole configuration.** `"RATED_SNG_POKERTH_V1"` implies §13's exact values, or
   the advert is rejected — a preset name that does not carry the preset's values
   is a lie about what game is being offered. `"CUSTOM"` implies nothing and
   carries every value in the fields above. **Any third value is rejected on
   sight, whether or not this client knows the name** (`G6-R6`). A receiver that
   accepts an unknown name has accepted a table whose identity it cannot check:
   the name asserts values by this very rule, the advert asserts values in its
   fields, and nothing in version 1 says the two agree. Two clients shipping
   different tables under one unrecognised name is `G6-R1`'s defect with a
   different label on it — `hand_deadline_ms` alone is signed into
   `table_params_hash`, so they cannot join each other and neither can say why.
   **A named configuration that lives only in an implementation is therefore
   advertised as `"CUSTOM"`**, which costs nothing: every value is already in the
   advert and already bound into `table_params_hash`, so a joiner reads the
   numbers rather than trusting the name. §13's heads-up reference configuration
   is exactly such a name and is not a `preset_id`. Adding a third value is a
   minor change under §10.1 only if it arrives with a full pinned value list in
   §13; without one it is not addable at all;
4. `deck_suite` is a suite this client supports;
5. `expires_at_unix_ms > timestamp_unix_ms`, and `expires_at` is **not more than
   `MAX_AD_LIFETIME_MS = 300_000` ahead of local time**, and `timestamp` is not
   more than `MAX_CLOCK_SKEW_MS = 120_000` in the future. Without the first bound a
   malicious peer pins a table into every lobby forever, which is free spam and
   exactly what §4 requires us to prevent. [NAT §7.3]
6. if a `LOBBY_TABLE_AD` for this `table_id` is already held, `timestamp_unix_ms`
   must be strictly greater than the held one, or the message is discarded. This
   blunts replay of stale adverts. [NAT §7.3]
7. **and, if one is already held, the re-broadcast's `table_params_hash` (§3.1)
   must equal the held advert's, or the advert is discarded and this client marks
   the table unjoinable.** This is J1(b). Rule 6 compares two adverts on their
   `timestamp` and on nothing else, so nothing stopped a founder re-signing with a
   different `small_blind` or `max_players` and handing two joiners two **rule
   sets** — which forks `HAND_INIT`'s `n(4) level`, `n(5) small_blind` and
   `n(6) big_blind`, a collective stage whose bodies must be byte-identical. A
   table whose parameters changed under a live advert is not one this client can
   join safely, and the discard is the whole remedy: a founder who wants to change
   the game forms a **new** table under a new key, which is what changing the game
   means.

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
several peers before relying on live GossipSub. This runs as a request-response
RPC on `/p2p-poker/lobby-snapshot/1` with the RPC codec's own framing -- a
length-prefixed signed event each way, as the join RPC (§4.3), never a second
encoding around a signature -- and **not** over GossipSub, because a lobby with
hundreds of tables would blow past any sane gossip frame, and because the mesh is
not to be relied on for it at all (below). Both messages are signed by the
sender's application key.
[LIBP2P §6]

**`0x0104 LOBBY_SNAPSHOT_REQUEST`** — `n(0) max_tables: u16` (≤ 128),
`n(1) since_unix_ms: u64` (0 for everything), `n(2) nonce: bytes[32]`.

**`0x0105 LOBBY_SNAPSHOT_RESPONSE`** — `n(0) request_nonce: bytes[32]`,
`n(1) adverts: Vec<bytes>` (≤ `SNAPSHOT_MAX_ADS` = 128 entries, each a complete
`SignedEvent` of a `LOBBY_TABLE_AD`, each ≤ `TABLE_AD_SIGNED_MAX` = 1 536 B),
`n(2) truncated: bool`, and since D-047 (2026-09-12) `n(3) out: Option<Vec<OutWord>>` -- the
table's word about seats out for good from the tables the answerer sits at, each
`OutWord` = `n(0) table_id: bytes[32]`, `n(1) app_key: bytes[32]`, `n(2) seat: u8`,
`n(3) hand_id: u64`, `n(4) cert: bytes` (a complete `TIMEOUT_CERT` of that hand, ≤ `FRAME_CAP`);
at most `SNAPSHOT_MAX_OUT` = 4 an answer, absent when there is none. The receiver verifies
each certificate from its bytes against the table's roster and reads only the word about
its own seat; the answerer's authority is not what it rests on.

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

> **Asked of everybody, all the time -- normative, D-040 (`S1-DK`).** GossipSub tells
> a peer what topics a client is on once, with their first connection, and a first
> connection that did not carry that exchange leaves the founder deaf to a neighbour
> that is listening for as long as any connection between them stays open: measured,
> a founder whose every `publish` said *nobody subscribed* while the joiner sat
> connected, subscribed and grafted to it. So the question is not for a newly
> joined client alone. **A client asks every poker peer** -- one whose `identify`
> carries this protocol's version -- the moment it is recognised and again every
> `AD_REBROADCAST_MS` while it is connected; a client whose search (§7.13) runs
> asks the peers subscribed to its queue topics every ten seconds besides, at most
> sixteen a round, since a table another searcher founds is what its search waits
> for. **A founder answers with the advert it
> offers right now** (its own open table; a table that has dealt is not offered,
> §7.3); any client may add adverts it holds, up to `SNAPSHOT_MAX_ADS`. The asker
> takes every advert through §7.2's checklist as if it had come over gossip. **An
> answer is also a withdrawal**: a table the asker lists whose `founder_peer_id` is
> the answering peer and which the answer does not name is removed at once -- the
> founder is the authority on what it offers -- rather than at `AD_TTL_MS`. A
> responder answers at most one question per peer in five seconds; a question it
> cannot read it does not answer.

### 7.6 Anti-spam

`SPEC_CS.md` §4 requires expiry of stale adverts and protection against spamming.
The layers, cheapest first:

1. **Expiry** — §7.2's TTL, which costs nothing and removes the persistent-spam
   category entirely.
2. **Per-key rate limits**, enforced locally by every client:
   `MAX_ADS_PER_TABLE_KEY_PER_MIN = 4`, `MAX_ADS_PER_PEER_PER_MIN = 90`,
   `MAX_PRESENCE_PER_PEER_PER_MIN = 4`, and for `LOBBY_CHAT` **1 message per 2 s
   with a burst of 5, per remote `PeerId`**, matching `NETWORK_STACK.md` §6.6.
   Exceeding a limit gets
   `MessageAcceptance::Reject`, which applies the GossipSub P₄ score penalty to the
   forwarder as well.
3. **Bounded caches** — `MAX_TRACKED_TABLES = 512`, `MAX_TRACKED_PRESENCE = 8192`,
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
here: **the fixed public rendezvous key publishes each player's persistent
`PeerId`, together with every address the swarm holds, and leaves it there for up
to 48 hours after the player closes the client.** There is no way to withdraw it —
`stop_providing` is a local operation and the remote copies run their own clock.
Phase 0 observed a stranger re-announcing under a freshly generated random
infohash within 24 minutes, so crawling was seen first-hand rather than
hypothesised, and the audience is now *worse* than that observation suggests: the
key is fixed and public, so the nodes closest to it are computable by anybody, and
a node ground close to it receives every announcement for ever.

The one thing that got better is worth stating beside it: a storing node refuses a
provider record whose identity is not the peer on the authenticated connection
that sent it, so **nobody can announce somebody else's `PeerId` in this lobby**.
That is the whole of the improvement, and it does not make the lobby private.

**This paragraph named `LOBBY_INFOHASH` and "roughly 100 arbitrary internet hosts
per announce cycle" until 2026-09-02.** Both were Mainline's, measured against a
mechanism `56b0b50` deleted, and no counterpart figure has been measured for
Kademlia — how many distinct nodes one `get_providers` walk contacts is
`NETWORK_STACK.md` §3.5's first `[UNMEASURED]`. It is left unquantified rather
than re-quoted at a number that is no longer about anything. That is a
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
D-054 adds the two bounds a character rule needs to be one: at most **four
combining marks** on one base character, and at most **120 base characters** --
a byte cap bounds neither the height a line is drawn at nor the number of
glyphs in it. A sender cleans to exactly this rule before signing; a receiver
refuses what is not already clean, on both string fields.

**One author, one budget (D-054).** The per-neighbour budget above bounds what
each *link* carries; on a gossip mesh one speaker reaches a receiver through
every neighbour it has, so a receiver charges the **author key** as well --
after the signature verifies, never before -- at 12 lines a minute for chat and
4 for presence. A budget spent by a key that has not been verified is a way to
silence the player that key names.

Chat carries **no security claim of any kind**. It is authenticated as coming from
an application key and nothing more; impersonation by display name is trivial and
expected, and a client must never present a chat line as evidence about who a
player is.

### 7.8 `0x0107 TABLE_CHAT`

`SPEC_CS.md` §22's table screen carries a chat among the seats of one table
(`S1-CS`, 2026-09-10), and the message type exists here for the reason §7.7's
does.

*Channel:* the table's own group (D-019), which is the closed set of its
seats, and **nothing else** (D-054). Not the table's GossipSub topic: that
topic's name is derived from a `table_id` every advertisement carries, so any
peer of the network may publish into it, and what it publishes is bytes on
every seat's link that the group's own meter (D-051) never sees. Never
the lobby's chat topic: a line at a table is for the seats at it. A receiver
that sees a `TABLE_CHAT` on any carrier but the group refuses it and does not
forward it.
*Signed by:* the sender's application key, which must hold a seat in the
receiver's roster for the table the line names; a line from any other key is
refused, and a line naming another table is dropped.
*Envelope:* unchained, per the rule at the head of §7 (`chain_scope = 0`).

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) display_name` | `bytes` | ≤ 32 B UTF-8, §9.4 string rules |
| `n(1) text` | `bytes` | ≤ 256 B UTF-8, §9.4 string rules |
| `n(2) table_id` | `bytes(32)` | the table the line is said at |

Payload cap `LOBBY_CHAT_MAX = 2 048 B`, shared with §7.7. Everything §7.7 says
of the two strings and of the absence of any security claim holds here
unchanged. Muting a seat is the receiver's own affair -- it stops showing that
seat's lines -- and is never on the wire.

**What a receiver allows (D-054).** Every rule below is the receiver's own and
rests on what reached it, because the sender's client is the attacker's to
write:

1. **Two budgets, and the order of them is the rule.** The *carrier's* budget
   is charged first, against the group member key the carrier itself reports
   for the bytes -- a fact, not a claim -- and nothing of the message is read
   until it is paid. The *seat's* budget is charged only **after** the
   signature verifies. Charging the key inside the envelope before that is a
   way to silence any player: the key is public and in every roster, so an
   unsigned line under it every two seconds spends its owner's whole allowance.
2. **What each budget is:** three lines held back and spent at once, then one
   line per two seconds, and at most 2 048 B of chat a minute. The byte budget
   is what keeps chat from crowding out a hand while every line of it is legal.
3. **A line is one line of printable text**, and so is a display name: no
   control character (a newline is a screenful of pane out of one line's
   budget), none of the invisible characters that change the direction or
   shape of what is around them, at most four combining marks on one base, and
   at most 120 bases. A sender of this protocol cleans what it sends to exactly
   this rule before it signs, so a refusal here is a client that did not.
4. **A refusal is noise, and noise is D-051's business.** A spent budget counts
   as one point against the member that carried it and anything else as four,
   so a peer that spams is cut off from the group by the same measure, and by
   the same joint certificate, as one that floods it -- and no single peer's
   word is involved in either.

### 7.9 A seat's word that it sits out -- no message

D-049. A seat whose own clock ran out sits out: its client checks or folds at
once on every turn until the player is back. It says so **without a message of
this protocol**: as its own member status in the table's group -- the carrier
D-019 chose keeps one per member -- *away* while it sits out and *none*
otherwise, set again every 20 s and on every fresh copy of the group. A receiver reads the status of the group member its evidence
binds to the seat (the most recently heard entry, S1-DU) and shows the seat
sitting out only while the group holds it. The status is display data: it is
not signed by the application key, never evidence, never hashed, and never
changes what any seat may do.

### 7.10 `0x0108 TABLE_LEAVE`

`S1-FQ`. A seat's own word that its player has left the table -- the only word
from which another seat may say that a player left. The carrier's own report
that a member left on purpose is no such word: a client that merely rejoins the
group says the same goodbye.

*Channel:* the table's group, and nothing else, as for §7.8. A receiver that
sees one on any other carrier refuses it and does not forward it.
*Signed by:* the leaving seat's application key, which must hold a seat in the
receiver's roster for the table the word names; a word from any other key is
refused, and one naming another table is dropped.
*Sent:* once, when the player leaves the table, before the client leaves the
group. Never for a restart, a lost line or a rejoin, and never required: a seat
whose client vanishes without it is an absent seat, as §8 treats one.
*Envelope:* unchained, per the rule at the head of §7 (`chain_scope = 0`).

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) table_id` | `bytes(32)` | the table left |
| `n(1) reason` | `u16` | `1`, the player left; any other value is refused |

Payload cap `LOBBY_CHAT_MAX = 2 048 B`, shared with §7.7. The emitter's clock
must be within `CLOCK_SLACK_MS` of the receiver's, and §7.8's carrier budget is
charged before anything is read. A receiver marks the seat gone from the table
for good; where it was the only other seat still in the game, the game is over
at that receiver. It changes no roster and moves no chip: to every rule of §8
the seat is an absent one.

**Before the table is set** (`S1-GA`) the word is said too, over the table's group
**and** its topic -- a seat still joining the group is heard by the founder on the
topic alone -- and received on either; and it is sent to the founder itself as a
request on the join RPC (§4.3's channel, `S1-GP`), which the founder answers with an
empty frame and judges as the topic's copy -- a player that leaves before its client
is in the group, at a founder the topic's mesh does not reach, was otherwise heard by
nobody and held its seat as a ghost. The founder gives the seat back at once
(`PLAYER_LIST` said again without it); a joiner only notes it and waits for that
list. A word said more than 10 s before the seat's present sitting, as the founder
learned it, is an old word carried again and changes nothing. **From the set on**
(`D-063`) the word is carried in the `TIMEOUT_CERT` that names the seat (§4.8),
which every seat checks as it checks the votes, and against which the seat counts
for nothing on §8.3's floor: two players leaving a table of three leave the third
a table of one, not a hand that ends on its budget for ever. The founder's own
word before the set is §7.12's: its seats go on without it -- and it goes to every
seat of its roster as a request on the join channel too (`D-062`): a seat not yet in
the table's group, at a founder the topic's mesh does not reach, learned of it from
the lobby's asking half a minute later otherwise. A seat takes it that way from the
founder's own peer only, and answers with an empty frame.

### 7.11 `0x0109 TABLE_HEARING`

`D-060`. Before the table is set, a seat's own word on which other seats of the
roster it cannot hear -- the founder's evidence for giving back a seat the table
cannot play with, since the founder alone cannot see whether two joiners hear each
other.

*Channel:* the table's group and its topic, before the table is set; after it,
neither sends nor reads one.
*Signed by:* the seat's application key, which must hold a seat in the receiver's
roster for the table the word names.
*Sent:* while the table forms, whenever the seats this seat cannot hear change or
the roster serial does, and every 6 s otherwise. A seat cannot hear a seat that is
not in its copy of the table's group, or has not been heard there within
`QUIET_LIMIT_S`.
*Envelope:* unchained (`chain_scope = 0`).

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) table_id` | `bytes(32)` | the table |
| `n(1) list_serial` | `u64` | the `PLAYER_LIST` serial the word is about |
| `n(2) unheard` | `bytes` | seat numbers, ascending, none twice, each below `MAX_SEATS`, never the sender's own; empty when it hears every seat |

The emitter's clock must be within `CLOCK_SLACK_MS` of the receiver's. It is not
charged to §7.8's chat budget -- every seat says one every few seconds while the
table forms -- and a receiver takes one a second from a carrier at most.

**What the founder does with it.** A word stands for 15 s, and only while its
serial is the founder's own. A seat still inside `GROUP_JOIN_GRACE` (40 s) of
sitting down is neither judged nor counted against another. A settled seat is in
trouble when it cannot hear a settled seat whose own word stands (its own word,
which costs a liar only its own seat), or when two settled seats at least cannot
hear it (so no single seat can have another given back) -- or, when no word of its
own stands, when any one settled seat cannot hear it (`S1-GQ`): not hearing a seat
that says nothing is that seat's trouble and never its hearer's. A trouble that lasts `MESH_GRACE` (20 s) gives the
seat back: of several, the one with the most trouble, and between equals the later
seated. Once every seat hears every seat, a seat that has not ratified for
`READY_GRACE` (15 s) holds the table up and is given back too. A seat whose word
names an older serial is sent the roster again, at most every 10 s. And a seat
whose word stands is alive and speaking: the founder gives it back for its goodbye
alone, never for the founder's own reading of its silence -- that reading is the
founder's line as often as the seat's, and the seat's hearing is judged above.
The founder judges no seat while its own line to the group's carrier is down, or
was within `QUIET_LIMIT_S`, or while two members or more timed out of its copy of
the group within a minute -- its own line, before the carrier says so (`S1-GX`).
**The founder ratifies last** (`S1-HE`): it says `TABLE_READY` for a roster only
once it holds every other seat's ratification of it, so its word completes the set
everywhere at once, and it gives no seat back after -- a roster changed under a set
half made set the table in two halves.

### 7.12 `0x010A TABLE_CONTINUES`

`D-061`. A forming table whose founder is gone goes on at a new table founded by one
of its seats. The founder's authority before `TABLE_READY` is the table key's, and
nobody else holds that key; so the table is not handed over, it is **founded again**
-- same name, same parameters -- by a seat of its roster, with a key of its own, and
this word takes the other seats there.

*Channel:* the group and the topic of the table that goes on, before it is set. The
seat that founds stays in the old group for the minute it says the word there, and
leaves it after: a seat with no poker peer on the lobby's line hears it in the group
and nowhere else (`S1-IA`).
*Signed by:* the application key of a seat in the receiver's roster for that table.
*Sent:* by the seat that founded the new table, every 5 s for a minute after.
*Envelope:* unchained (`chain_scope = 0`).

| Field | Type | Limit / rule |
|---|---|---|
| `n(0) table_id` | `bytes(32)` | the table that goes on |
| `n(1) list_serial` | `u64` | the roster of that table the speaker held |
| `n(2) advert` | `bytes` | the `LOBBY_TABLE_AD` of the new table, verbatim: its signature must hold, its table key must not be `table_id`, its `founder_app_key` must be the speaker's, and it must play the receiver's table's own game -- the same `table_params_hash` (§3.1), `table_name` and `password_required` as the advert that table was joined under |

**When a seat goes on, and at whose table.** Before the set, a seat goes on when the
founder said it left (§7.10), or when it has not heard the founder for
`QUIET_LIMIT_S` and `FOUNDER_GONE_GRACE` (20 s) more and hears other seats; either way
only while it knows the table still forms: no frame of a hand of that table has
reached it, and either its founder admitted it with `JOIN_ACCEPT` -- which a founder
gives a stranger only before its table deals -- or another seat, neither itself nor
the founder, has said `TABLE_HEARING` (§7.11) since the founder went or within
`HEARING_FRESH` (15 s) before. A set table's seats never say that word, and a seat
already on a roster is answered with the roster, not admitted; so a client that holds
a table's roster without its session -- a player back at a table that has dealt --
never goes on, while a seat its founder admitted goes on even alone, and the table
fills again at its continuation. A seat of a set table takes no `TABLE_CONTINUES` at
all. The seat it goes on at is the lowest seat number of the roster, the founder's
excepted, not passed over -- itself included. If that is itself, it founds the new
table and says this word. Otherwise it goes on at the table of the lowest seat whose word it has, once
that seat is its choice or lower; a choice that says nothing within `CONTINUES_WAIT`
(20 s) is passed over for the next -- within 8 s when the seat does not hear it, since it
may still be founding, heard by others, and two seats must not both found one.

**Through the lobby, first (`D-062`).** A word reaches the seats the topic's mesh
reaches in that minute and carries nothing a seat can weigh, and seats moved by words
alone sat at three tables. So every seat keeps the table it first sat at -- its
*origin*: the table's id, its founder's key, every application key its rosters seated
with its seat, and the advert it was joined under -- across every continuation, until
its player leaves or joins a table that is none of the origin's. A *table of its
game* is the origin, offered by its founder, or a table founded by a key of the
origin's rosters that plays the origin's game (the same `table_params_hash`,
`table_name` and `password_required`); its rank is 0 for the origin and the
founder's origin seat otherwise. The best table of the game is the one with most
players (the advert's `players`), then the origin, then the lowest rank -- among the
tables heard within 90 s, joinable, not full, and not one that refused this seat
within 5 minutes. Every 30 s: a seat that reads its founder as gone goes to the best
table offered before it follows any word or founds anything (a table with a single
player only from a higher seat, whose own table would rank below it); the founder of
a forming table goes on, as a seat, to a table with more players than its roster
holds, and never on a word alone; a seat asking for a table of its game asks for the
best table instead, and founds the game's continuation from its origin when none is
offered for 60 s. A seat that hears its founder moves at its founder's word alone.
The word above serves the first seconds, before the lobby's asking has carried an
advert.

**Not through a line of its own (`S1-GS`).** A seat reads its founder gone by the
group only while its own line to the group's carrier is up and has been for `QUIET_LIMIT_S`; and
not while the founder answers the lobby's question (§7.5) naming the table within
75 s, which is a founder alive with the group's line in trouble -- unless the founder
has been out of the group's hearing for 120 s, past which the table cannot form. The
founder's own signed word (§7.10) stands whatever the line says.

**What it cannot do.** A seat offers only a table it founded itself, and only the
same game -- a continuation with other rules, another name or another gate is not
heard; it moves only seats that read the founder as gone themselves, so a seat
cannot take players away from a founder they hear; and it holds, at the new table, a
founder's authority and nothing more -- which is none over the cards, the chips or
any hand. Through the lobby it can say more players sit at its table than do: seats
that go there find its real roster and read the lobby again, and a table that
refuses them is avoided for five minutes.

### 7.13 `0x010B SEARCH_PRESENCE`

`D-064`. A client that searches for a game by itself says so, so that every client
can count who is looking and a searching client can say how many others are, and
how long they have waited. It is the queue's mirror of `LOBBY_PLAYER_PRESENCE`
(§7.4): the same kind of message, on a topic of its own, judged the same way.

*Channel:* the search-queue topic `/p2p-poker/search-queue/1`, an `IdentTopic`
with the lobby topic's settings -- and, wherever the lobby is sliced (`S1-EX`), the
slice topic `/p2p-poker/search-queue/1/<slice>`, the slice named by the speaker's
own application key at every depth, exactly as a founder names an advert's slice by
its table key. A client listens to the queue's slices that match the lobby's slices
it holds, so a sliced lobby and a sliced queue are one decision.
*Signed by:* the speaker's application key.
*Sent:* every `SEARCH_PRESENCE_EVERY_MS` while the client searches, and once more,
with `state = 0`, when it stops -- so the count falls at the cancel and not three
heartbeats later.
*Envelope:* unchained (`chain_scope = 0`).
*Body:* `n(0) state: u8` -- `0` stopped searching, `1` heads-up, `2` six seats,
`3` ten seats (the full ring), `4` any tournament table; `n(1) tables: u8` -- how many games at
once, `1..=4`; `n(2) since_unix_ms: u64` -- when the search began, on the speaker's
clock, so a listener can say how long the queue has waited (a value ahead of the
listener's clock reads as *now*).

A receiver takes the message in the order §7.4's is taken: the forwarding
neighbour's budget first, then the shape, then the clock (`CLOCK_SLACK_MS`), then
the signature, then the author's own budget (`MAX_PRESENCE_PER_PEER_PER_MIN`, on a
map of its own, so a client's lobby heartbeat and its search heartbeat never spend
each other's allowance). A `state` or a `tables` outside its bounds is malformed
and `Reject`ed. A presence is held until `SEARCH_PRESENCE_TTL_MS` after it was
heard -- three heartbeats, the advert's own ratio -- or until the speaker says it
stopped; at most `MAX_TRACKED_SEARCHERS` speakers are held, the longest unheard
making way.

**What it decides, and what it never decides.** It moves a number in a window,
it says what an *any* search founds (the format most searchers want), and it
orders the founding: a client founds its own table only after a delay that grows
with the number of searchers of its format whose key is lower, so the lowest key
founds first and the others find its table inside a lobby round. That is the
whole of its authority. It is never an input to any game decision, a table's
roster, a seat, a card or a chip; a forged presence moves the count by one and
delays one founding by fifteen seconds. Every seat the search takes is a
`JOIN_REQUEST` (§4.3) judged by its founder as any other, every table it founds is
advertised as any other (§7.2, with `min_players_to_start = 2` and a name that
begins `Auto Sit & Go`, so a search reads the founder's rule for starting short of
full), and every seat it gives back is a `TABLE_LEAVE` (§7.10).

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

A seat whose own clock ran out once then **sits out** (D-049, §7.9): its client
makes the same check or fold at once on every later turn, until the player is
back.

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

The constant is the same at every peer. What `D-059` shortens is the moment a
client votes about a seat that has made the table wait, at a cryptographic step
(see `TIMEOUT_VOTE`), and a certificate absorbs a difference in that moment
because it needs every voter.

**The subject of a certificate is read from the votes it carries, never rebuilt
from the receiver's own stage.** The first carried vote's payload is the
preimage and `subject_digest` must recompute from it; every carried vote's
envelope `sequence` and `previous_event_hash` must equal the `subject_sequence`
and `parent_event_hash` that its own payload names, and the certificate's
envelope must too. **That is what replaces the receiver's cursor as the replay
barrier**, and it is strictly stronger: a vote sealed at one stage cannot be
counted towards a subject at another, for any receiver, because the voter signed
the position it voted at. `deadline_ms` must equal the value the receiver
derives from the hand's own parameters, or a position-free receiver would be
taking the deadline on the emitters' word.

The carried voter set must contain the receiver's own `V(subject)` and be
contained in `dealt_in \ {subject}`, with `|carried| >= 2` and `|dealt_in| >= 3`.
**A shortfall against the receiver's own `V` is buffered, not refused**: a
receiver missing an earlier certificate derives a larger `V`, and the peer that
most reliably misses one is the subject itself.

**The peer least able to stand at the subject stage is the subject of the
certificate**: it moved past that stage precisely because it emitted the event
the voters never received. A receiver that could only check a certificate
against its live cursor could therefore never apply one about itself, would
never shrink its own `R(k+1)`, and would play a table the others had already
left — under identical genesis hashes, with no refusal logged anywhere. That is
measured, five hands. See D-024.

**And the legality condition governs emission only.** *"The receiver has not
itself accepted an event for that stage from `subject_seat`"* is a rule about
whether a peer may **emit** a vote. It is never applied to the votes carried
inside a certificate: at the subject it is unsatisfiable by construction, and an
implementer hardening the vote path would silently re-close this exact door.

`next_deadline_ms` is **normative, not the emitter's choice.** Its value is a
deterministic function of the table parameters and the kind of the next stage:

| Next stage kind | `next_deadline_ms` |
|---|---|
| a betting action | `action_timeout_ms + action_grace_ms + time_bank_ms` |
| any cryptographic contribution (`DECK_INIT`, `SHUFFLE_*`, `DECK_COMMIT`, `DEAL_PRIVATE`, `BOARD_REVEAL`, `SHOWDOWN_*`) | `crypto_step_timeout_ms` |
| `STATE_HASH` / `STATE_ACK` | `crypto_step_timeout_ms` |
| a hand boundary (`HAND_INIT` after `hand_delay_ms`) | `hand_delay_ms + crypto_step_timeout_ms` |
| the seat-order beacon (`RNG_COMMIT`, `RNG_REVEAL`) | `crypto_step_timeout_ms` |
| **anything in table formation** — `CAPABILITIES`, `JOIN_REQUEST`, `JOIN_ACCEPT`, `JOIN_REJECT`, `PLAYER_LIST` | `0`, meaning *this event arms no deadline* |
| `TABLE_READY` | `crypto_step_timeout_ms`, because the next stage is the beacon |

A peer that writes a different value emits an invalid event. There is nothing to
negotiate and nothing to game.

**The last three rows close `U4`.** The table had four rows and none of them
covered a formation message, while this paragraph declares the field normative
and a wrong value invalid — so every join message was an invalid event under one
reading and an unconstrained one under another, and an implementer had to guess
in the one constructor the whole join path runs through.

`0` is the right value for the unchained formation messages because **they arm
nothing**. The join RPC carries its own 20-second timeout at the transport
(§1.4); the handshake is covered by `HELLO`'s `HANDSHAKE_DEADLINE_MS`, which is
armed once for the whole exchange rather than by each message in it; and a
`PLAYER_LIST` is a proposal that obliges no one to answer within any time. A
deadline field that named a duration nothing would measure against would be a
value two peers could disagree about for no purpose, which is exactly what this
paragraph exists to prevent.

`TABLE_READY` is the exception because it is the one formation message that *is*
a chained stage, and what follows it is `RNG_COMMIT` — a cryptographic
contribution, and now a row of its own rather than a type absent from the list.

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

The window covers `hand_delay_ms` plus the whole of hand `k`. `STATE_MACHINE.md`
§5.2 already reads it this way, so this sentence removes a corpus disagreement
rather than creating one. **What that window has to be large enough for is the
next box, and until this pass it was not** — the sentence that stood here said
the §13 constant *"is set on that basis and is not rescaled by this ruling"*,
which is exactly the claim `P3` falsified.

#### `hand_deadline_ms` scales with the seat count — normative, and this is `P3`

`hand_deadline_ms` was one figure, 600 000 ms, for every table from two seats to
`MAX_SEATS`. **A legal hand at six seats exceeds it before a single stage goes
wrong.** A no-re-raise hand in which every seat takes its allowed time is `4n`
betting actions at `action_timeout_ms + action_grace_ms` each; at the §13 preset
that is `24 × 25 000 = 600 000 ms` at six seats — the deadline exactly, with the
boundary, the deck and every checkpoint still unpaid — and `40 × 25 000 =
1 000 000 ms` at `MAX_SEATS = 10`, against 600 000. The hand then aborts under
§8.4 with `cause = 1`, `attributed = []` and `cert_hash = None`: **no progress and
nobody at fault**, repeatable for ever, on a table where every peer is honest and
every message is on time.

**The harm is not only the stall, and the sharper statement is the one that fixes
the shape of the rule.** §8's two abort paths are asymmetric on purpose: the
certificate path names a subject, the whole-hand path names nobody. A whole-hand
deadline that can expire *before* the per-stage deadlines it exists to back up
lets any seat reach the non-attributing path by making a hand long, which costs it
nothing and is not even misbehaviour. So the requirement is not *"large enough for
a typical hand"* but:

> **`hand_deadline_ms` must exceed the sum of the per-stage deadlines of every
> stage a legal hand walks.** It is a backstop for a stage that stalls where no
> certificate can be produced (§8.3's `|V| < 2` floor, and the `HAND_INIT` stall
> of reason 1 above). A backstop that fires first is not a backstop; it is a
> shorter deadline that attributes nobody.

**The floor.** `n` is `max_players` — `LOBBY_TABLE_AD`'s `n(11)`, §7.2 — and
**never the number of seats currently occupied**: `hand_deadline_ms` is
two-sided, every peer must hold the identical value (§8.2's opening rule), and a
floor derived from a live count moves as players sit down. A hand's walk is
**`6n + 20`** stages that need a round trip — the boundary's three, the deck's
`2n + 2`, the deal's three, `n + 2` pre-flop, `3(n + 2)` over the three streets
and the showdown's four (`research/PHASE3_GATE4.md` §4.1 enumerates them, and
`n = 4` reproduces the earlier gate's 44 exactly). **`4n − 3` of those are betting
actions** — `n` pre-flop and `n − 1` on each of three streets — **and the
remaining `2n + 23` take `crypto_step_timeout_ms`** from the table above. So,
normatively:

```
HAND_DEADLINE_FLOOR(n) =  hand_delay_ms
                        + (2n + 23) * crypto_step_timeout_ms
                        + 4n        * (action_timeout_ms + action_grace_ms)
                        + n         * time_bank_ms
```

The reserve enters **once per seat**, which is what a per-hand reserve is: every
seat may spend the whole of its own, and a hand in which all of them do must
still be payable. Omit the term and a table that offers a reserve abandons the
first hand its players actually use it in — the failure the reserve was added to
prevent, arriving by the same door.

`4n` rather than `4n − 3` because the three spare allowances are cheaper than an
off-by-one an implementer has to re-derive. At the §13 preset —
`hand_delay_ms = 7 000`, `crypto_step_timeout_ms = 30 000`,
`action_timeout_ms + action_grace_ms = 25 000`:

| `n` | crypto term | action term | **floor** | preset before `P3` |
|---:|---:|---:|---:|---:|
| 2 | 810 000 | 200 000 | **1 017 000** | 600 000 — **below** |
| 4 | 930 000 | 400 000 | **1 337 000** | 600 000 — **below** |
| 6 | 1 050 000 | 600 000 | **1 657 000** | 600 000 — **below** |
| 8 | 1 170 000 | 800 000 | **1 977 000** | 600 000 — **below** |
| 10 | 1 290 000 | 1 000 000 | **2 297 000** | 600 000 — **below** |

The preset was below its own floor **at every seat count**, including the two the
last three passes measured; two and four seats survived because the crypto term is
three orders of magnitude of margin that a healthy table never spends, and six is
where the *action* term alone crosses. That is why the widening found it and the
narrow passes did not.

**Re-raises are budgeted by the headroom above the floor, and the headroom is the
founder's to buy.** A raise that reopens the action entitles up to `n − 1` further
actions, so a table whose advertised deadline sits above its floor tolerates

```
REOPENINGS(config) = ⌊ (hand_deadline_ms − HAND_DEADLINE_FLOOR(n))
                       ÷ ((n − 1) * (action_timeout_ms + action_grace_ms)) ⌋
```

reopening raises in one hand. At the §13 preset's new value this is four at ten
seats. **A hand with more reopenings than that still aborts on a legal path**;
that residual is stated rather than closed, and the two ways of closing it — a cap
on raises per street, which is a rule of the game and not this document's, or a
deadline that extends deterministically on accepted chain content, which is a new
mechanism — are `G4-P3-r` in `DECISIONS.md`'s open list rather than chosen here.

#### The admitted minimum is the floor plus one reopening — normative, and this is `G5-Q6`

**`REOPENINGS` may not be zero, because a table where the first re-raise can abort
the hand is the `P3` defect one raise later.** The floor above budgets a
**no-re-raise** hand: every seat calls or folds, nobody ever raises into a live
bet. That is a legal hand and it is not poker. A table advertising exactly
`HAND_DEADLINE_FLOOR(n)` has `REOPENINGS = 0`, so the **first** raise that reopens
the action can carry the hand past its own deadline, and it ends under §8.4 with
`cause = 1`, `attributed = []` and `cert_hash = None` — no progress, nobody named,
on a table where every peer is honest, every message is on time, and the only
thing that happened is that somebody re-raised.

That is the identical harm the floor above already refuses, so it takes
the identical remedy rather than a new one. Naming the denominator of `REOPENINGS`
is the whole of the mechanism; no quantity is added:

```
REOPENING_COST(n)    = (n − 1) * (action_timeout_ms + action_grace_ms)

HAND_DEADLINE_MIN(n) = HAND_DEADLINE_FLOOR(n) + REOPENING_COST(n)
```

`HAND_DEADLINE_MIN(n)` is the **admitted minimum** and is what §7.2 rule 2a
compares against; `HAND_DEADLINE_FLOOR(n)` keeps its meaning unchanged as the cost
of the walk, and remains the term every other statement in this document reads.
`n(17) >= HAND_DEADLINE_MIN(n)` is exactly `REOPENINGS >= 1`, which is why it is
written as the floor plus one unit of the same cost rather than as a second
formula.

| `n` | floor | `REOPENING_COST` | **admitted minimum** |
|---:|---:|---:|---:|
| 2 | 1 017 000 | 25 000 | **1 042 000** |
| 4 | 1 337 000 | 75 000 | **1 412 000** |
| 6 | 1 657 000 | 125 000 | **1 782 000** |
| 8 | 1 977 000 | 175 000 | **2 152 000** |
| 10 | 2 297 000 | 225 000 | **2 522 000** |

The §13 preset's `3 300 000` clears the ten-seat minimum by 778 000 ms and its
`REOPENINGS` is unchanged at four. The cap is untouched and still binds from
above: `2 522 000 < 3 600 000`, so the playable configuration space is non-empty
at every seat count from two to ten, which is the check that has to pass before a
lower bound may be raised at all.

**What this does not close, said plainly so the next pass does not read it as
closed.** `G4-P3-r` stands exactly as it was. One reopening is now guaranteed
instead of zero; a hand with more reopenings than the founder's headroom bought
still aborts on a legal path, and the two closures for *that* are still unchosen.
This raises the bound by one unit and changes nothing about the shape of the
residual. It was taken because zero and one are not the same kind of number here:
at zero the abort is reachable by ordinary play at **every** table that sits on
its floor, and at one it needs a hand the founder did not pay for.

**What a client does with a table whose advertised deadline is below its own
derived minimum: it refuses the table.** This matters because `hand_deadline_ms` is
a **signed table parameter chosen by the founder** and bound into
`table_params_hash` (§3.1), so a founder who wants every hand at their table to
abort neutrally needs no attack at all — they advertise a small number. The check
therefore belongs to the **joiner**, and it belongs with the other range checks of
§7.2, **before the advert is shown to a user or stored**:

> **Normative.** A receiver computes `HAND_DEADLINE_MIN(n(11))` from the
> advertised `n(14)`, `n(15)`, `n(16)`, `n(19)` and `n(11)`, and **rejects the
> advertisement** if `n(17) hand_deadline_ms < HAND_DEADLINE_MIN(n(11))` — that
> is, if the table cannot afford the walk **and one reopening raise** (`G5-Q6`). A
> rejected advertisement is not displayed, not stored, not joined, and the seat is
> never taken. **A client must not join and substitute its own floor locally**:
> the deadline is two-sided, and two peers running different whole-hand deadlines
> disagree about whether a hand aborted, which §8.2's opening paragraph classes as
> a consensus fault rather than a UX detail.

**The refusal is the safe direction and the cap does the rest.** Declining a table
costs a player nothing — no chips exist yet — while joining one guarantees every
hand ends with `cause = 1` and nobody named. And a founder cannot escape upwards
either: `n(17)` is capped at `3 600 000` (§7.2), so a configuration whose
`HAND_DEADLINE_MIN` exceeds that cap has **no legal deadline at all** and every
conforming client refuses it. That is the correct outcome and it is why the cap is
not widened here — the pair (minimum, cap) bounds the playable configuration space
from both sides, and a config outside it is unplayable rather than quietly broken.

If it expires the hand aborts under §8.4 with `cause = 1`,
`attributed = []` and `cert_hash = None`: **it produces no certificate and names
nobody.** The earlier form of this sentence — *"produces a certificate with
`kind = 2` naming every seat that has an outstanding contribution"* — was the
attribute-every-non-voting-seat rule that §8.4 deletes, and it is withdrawn. The
hand deadline is scoped on nothing, because it attributes nobody; that is what
makes it safe at every `|V|` (§8.4, D-008 point 4).

**The engine contains no clock, and under D-015 it takes no signed artefact of
time at all.** The sentence that stood here — *"Time enters the state machine
only as a signed `TIMEOUT_CERT`"* — is corrected: no certificate is produced, so
time reaches the engine as **the seat's own ordinary action** (an
`ACTION_CHECK` / `ACTION_FOLD` its client emitted when its own timer expired, a
single-writer event indistinguishable in the chain from a human's) and as **the
terminal `HAND_ABORT` of §4.10** on the whole-hand limit. Both are ordinary
chained events; neither is a claim about a clock. `STATE_MACHINE.md` must still
carry the deadline as explicit state rather than as a wall-clock read inside the
engine (D-006), and the rule is now easier to hold rather than harder, because
the one construction that ever put an agreed time inside the chain is not built.
`CERT_SETTLE_MS` was the last wall-clock read in the chain-building rule and it
was already gone (§4.8).

**One residual, stated rather than hidden.** A stage can close two ways — by the
subject's own event, or by certificates — and which one happened is chain content.
Two peers agree on it only if at least one required voter is honest and refuses to
vote after accepting the action. Where `|V| >= 2` that is the honest-voter
assumption already in force. Where `|V| < 2` there is no such voter, and D-008's
floor applies, so the ambiguity cannot arise: no certificate of either kind takes
effect at all (§8.3).

### 8.3 The timeout certificate — defined, not produced in version 1 (D-015)

> **Normative. No certificate exists in version 1, so no rule in this section
> fires.** Nothing emits a `TIMEOUT_VOTE`, so no `V(subject)` is ever assembled,
> no certificate ever completes, `consecutive_auto_actions` is never incremented
> by a deadline, and no seat is ever marked sitting out by this path. The
> `|V| >= 2` floor, the inductive exclusion rule and the effect table below are
> **retained in full** as the specification a later version implements; they are
> the part of this machinery that took five passes to get right, and deleting
> them would mean deriving them a sixth time.
>
> **What governs a missed deadline in this version.** An **action** deadline is
> answered by the seat itself: the client whose human has walked away emits its
> own `ACTION_CHECK` — or `ACTION_FOLD` when facing a bet — as an ordinary
> single-writer event at `player_to_act`'s own stage (§4.7, §8.1). That is a
> real signed event by the seat that owed one, so it needs no vote, no voter
> set, no unanimity and no shared clock, and §4.11 row 26–30's verdict covers it
> unchanged. A **cryptographic-step** deadline has no such answer, because no
> peer may publish another's decryption share: the stage stalls, and the hand
> ends at `hand_deadline_ms` under §8.4 with `cause = 1`, `attributed = []`,
> `cert_hash = None` and stacks restored — which is exactly what §8.3 already
> prescribed below the `|V| >= 2` floor, now applied at every `|V|`.
>
> **So the certificate's whole remaining content in this version is its
> absence**, and the two consequences of that absence are stated once here:
> **(a)** `attributed` is empty on every abort this version can produce, since
> §4.10's only non-empty path is the certified-subject path; and **(b)** there
> is no signed record of *who* timed out, only the transcript's visible gap at
> the stage that stalled. `THREAT_MODEL.md` §9.1.0 carries both as costs.

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
> **D-036 — several seats quiet at one stage (2026-09-11).** A certificate names
> a set `S` of seats quiet at one stage under one deadline, and its voter set is
> `V(S)` = the dealt-in seats, minus `S`, minus every seat a completed, valid
> certificate earlier in this hand already names. Every member of `V(S)` votes
> about every member of `S`, and the certificate carries all of those votes.
> Legal only when `|V(S)| >= 2` **and `|V(S)| > |S|`** — the voters outnumber
> the seats named, so no group short of a majority of the live seats completes a
> certificate about the rest. `V(subject)` everywhere in this corpus is the case
> `|S| = 1`, digest and bytes alike. Being voted against is still not exclusion:
> `S` leaves `V(S)` only inside the certificate that unanimously names every
> member of it, and a vote removes nobody from anything.
>
> **`D-063` -- the seats that resigned (2026-09-17).** The seats of `S` whose
> player left by its own signed `TABLE_LEAVE` (§7.10), carried in the certificate,
> count for nothing against that floor: with `Q` the rest of `S`, the certificate
> is legal when `|V(S)| >= 1` and either `Q` is empty or `|V(S)| >= 2` and
> `|V(S)| > |Q|`. A seat that said it left is at no fork -- it is at no table --
> and the seats left need no majority to remove it; a certificate naming it puts
> it out of the table for good.
>
> **`D-065` -- a vote that does not come stops being a veto (2026-09-18, the
> project owner's ruling).** Unanimity is kept and made reachable. A voter of
> the round at a stage that has said nothing about it within the round's air
> is voted about in turn with `cause = 2` (§4.8), and one certificate names the
> seats the stage waits on and those voters together, unanimously among the
> rest, under the floor above with the silent voters counted among the named.
> **What the table takes from a seat named with `cause = 2` is its veto for the
> rest of the hand and nothing else:** it leaves `V` of every later certificate
> of the hand as a seat an earlier certificate names; it is not struck, stays in
> `R(k+1)`, is not attributed by an abort resting on the certificate, and no
> word is kept for its client. At a betting stage the certificate still acts for
> the one seat to act. A seat merely slow therefore loses nothing it needs, a
> seat really gone is named by the next hand's opening as any quiet seat is, and
> a client that plays every turn and never votes can no longer hold a
> certificate about anybody -- which, before D-065, cost a table every hand
> after one of its seats died, and a betting stage its whole `hand_deadline_ms`.
>
> **`D-066` -- half the table silent (2026-09-18, the project owner's choice).**
> The majority floor kept a table standing whenever the seats that stopped were
> half of it or more: every hand ended on its deadline with the same seats, and a
> turn stood until `hand_deadline_ms`. Two exceptions, each needing two voters at
> least: **exactly half** is enough for the half that holds the lowest seat of
> `V(S) ∪ Q` -- one half of a table can hold it, never both, so two halves never
> certify each other -- and **any number** is enough when every seat of `Q` is named
> `cause = 3`, out of the table's group for `LONG_GONE_S` = 300 s by every voter's
> own reading, counted only while that voter hears some other seat and its library
> is on the network. Neither exception is taken by a seat of `Q` whose own line was
> sound all the while: a seat that was here refutes the only claim such a
> certificate rests on, so a half or a minority that lied about the rest forks away
> alone. A seat of `Q` whose line was down within `LONG_GONE_S` takes it -- it may
> really have been gone -- and comes back by D-028's return, which is how a far end
> of the table that lost its line together rejoins. **The price, accepted by the
> owner:** a real partition of the network that lasts past `LONG_GONE_S` while both
> sides keep their lines splits the table in two, each half going on without the
> other.
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
stalled hand now waits `hand_deadline_ms` (3 300 000 ms in
`RATED_SNG_POKERTH_V1` since `P3`, and never below `HAND_DEADLINE_MIN(n)`)
instead of `crypto_step_timeout_ms` (30 000 ms). The
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
is strictly better for an attacker than waiting out `hand_deadline_ms` for the same outcome,
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
is marked sitting out at the next hand boundary, which under D-005 means it keeps
its stack, posts dead money, takes no cards and drains. **The phrase "enters the
D-005 absent-seat state" stood here and is deleted (J5):** there is no such state
— nothing sets `SeatStatus::Absent`, and since D-013 nothing needs to, because a
seat's participation rather than its status decides what it owes (§3.2). The
counter's effect is the sitting-out marking and nothing beyond it. This path exists only where `|V| >= 2`: below the floor there is no
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

### 8.3.1 The return certificate — the grow side of the roster (D-028, `S1-BM`)

> **Normative.** The roster of hand `k+1` is
>
> ```text
> R(k+1) = ((R(k) \ OUT(k)) ∪ IN(k)) ∩ ALIVE(k+1)
> ```
>
> where `OUT(k)` is the subject set of complete `TIMEOUT_CERT`s of hand `k` and
> `IN(k)` the subject set of complete `RETURN_CERT`s of hand `k` (§4.8). Both
> directions are certificate-gated: neither `P(k)` nor §4.9's per-receiver
> readmission set `A` ever reaches a required emitter set, which is what D-012
> demands of anything that enters `GENESIS(k+1)`. `A` keeps its one effect — it
> widens the **accepted** set of `HAND_INIT(k+1)` — and gains none. The
> `TIMEOUT_CERT` rules of §8.3 are unchanged.
>
> **The rules, in the order a receiver applies them.**
>
> 1. **Settled boundaries only.** A return is voted on and certified only where
>    `TERMINAL(k)` is the `HAND_COMPLETE` stage hash. The condition is that chain
>    fact and not *this receiver holds a checkpoint*: a receiver that reached the
>    settlement by §4.10's late road holds the settlement's terminal and no
>    checkpoint of its own, and it accepts a certificate it cannot compare on the
>    voters' unanimous word rather than refusing it — refusing would be the
>    permanent fork with no dissent and no attacker. There is no return at a
>    boundary the table aborted.
> 2. **The voter set is `R(k) \ OUT(k)`, before any return is added**, written
>    without `dealt_in` or `grace`, both per-receiver. Two seats returning at one
>    boundary share one voter set, and their two certificates applied in either
>    order derive one `R(k+1)`.
> 3. **Only on the subject's own signed request** — a `PLAYER_SIT_IN` sealed in
>    §4.10's window at the subject's slot on `TERMINAL(k)` — and only with the
>    subject's own signed checkpoint-8 `STATE_HASH`, whose value each voter
>    attests is its own. The certificate carries both, so a receiver that never
>    heard the subject verifies the whole claim itself: D-024's *the artefact
>    proves its own position*, applied to the subject.
> 4. **The subject is outside `R(k)`, occupies a seat, and has chips.** A seat
>    inside the roster decides nothing by asking; a busted seat is
>    `∩ ALIVE(k+1)`'s business and is refused at the vote and at the certificate
>    alike.
> 5. **Unanimous, floor two, byte-identical.** Every voter seals its own copy
>    from the votes it holds, never from a peer's certificate; the floor is met
>    heads-up.
> 6. **Banked once per subject digest, position-free.** A bank on a hand this
>    receiver has already derived `k+1` from re-derives `Opening(k+1)` through the
>    one derivation every seat runs and re-opens it from a sequence 0 this
>    receiver never left — D-027's road, now on the settled path. A returned
>    seat's allowance is refilled at the derivation, so it is required **and
>    dealt in** at `k+1` (correction 4: `grace` is not left as an unowned gate
>    between the roster growing and the player playing).
>
> **What it costs.** A return costs two hands: a seat certified out at the
> boundary of hand `j` plays no part in `j+1`, asks at `j+1`'s settled boundary,
> and is dealt into `j+2`. On a carrier that aborts most hands the wait is
> unbounded and this rule does not bound it (correction 5). **The automation
> belongs to the client**: it sends the request at every settled boundary where
> its seat is outside the roster with chips, so the player clicks nothing and
> sees *sitting in at the next hand*; `--stay-out` withholds it, which is how a
> seat watches a table it has chips at without being dealt in. **And the
> client publishes its checkpoint and its request at the terminal, then holds
> the next deal for up to `RETURN_GRACE_MS = 6 000` ms while a return is in
> flight** -- a request seen in the window, or its own, with no certificate
> banked yet -- because a certificate that lands after `HAND_INIT(k+1)` has
> left stage 0 can no longer move `R(k+1)`, and a heads-up hand leaves stage 0
> in a tenth of a second (`run164337-3`: nine requests, no return, before the
> hold; `run170442-3` with it: certified out in hand 4, dealt back into hand 6
> at one genesis on every client). A client liveness parameter, not a wire
> rule: past it the table deals on and the seat asks again at the next
> boundary.
>
> **Guard.** `src/table/hand.rs`'s `return_voters` is scanned by its own test
> for the two forbidden words; the whole road is
> `a_return_certificate_puts_the_seat_back_into_the_roster_at_one_genesis`, the
> late road `a_receiver_without_a_checkpoint_of_its_own_accepts_on_the_voters_word`,
> and each rule above was broken on purpose and seen red before this section
> was written — `S1-BM`'s row lists the fifteen mutations.

### 8.4 Simultaneous failures

> **D-015 dissolves this section's problem rather than answering it.** No
> certificate is produced, so there is no unanimity to fail to reach and no
> deadlock to resolve: **every** stall — one subject, two, or ten — takes the one
> path this section already identified as the safe one, the whole-hand limit. The
> analysis below is retained because it is the argument for that path and because
> a later version reinstating the certificate meets the same deadlock; **the
> normative content for this version is the `hand_deadline_ms` boxes below,
> which need no certificate from anybody and are unchanged by D-015.** `Q-02`
> stays open for that later version and blocks nothing here (§12).

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

**Two simultaneous subjects deadlocked, by design, until D-036 (2026-09-11).**
Neither certificate could form, because neither voter set could be reduced, and
that was the price of the fix above. D-036 pays it differently: the two are
named **together**, by one certificate whose voter set is everybody else —
`V(S)` = the dealt-in seats less `S` less the already certified (§8.3) — with a
vote from every one of them about every seat named. Nothing shrinks `V` by
assertion: the seats named leave `V(S)` only inside the certificate that
unanimously names all of them. And the voters must outnumber the named,
`|V(S)| > |S|`, so a group short of a majority of the live seats completes nothing
about the rest — the one-client attack above needs, at six seats, five votes it
does not have, and two clients at six seats can name the other four no more
than they can leave the table and play without them. A sealer names the greatest
set every outside seat has voted about; such sets are closed under union, so the
honest seats converge on one, and a certificate carries its votes so a seat that
missed one holds it from the copy (`DECISIONS.md` D-036).

Where the quiet seats are half the live seats or more no certificate forms, no
vote is cast, and the whole-hand limit resolves it as before. When
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
every `|V|`, including `|V| = 0`, and under D-015 it is the only piece that
exists,** because it names nobody and moves nothing
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
Q-02** is answered by D-036 (§8.3, §8.4), the above having been its written interim behaviour; the same
question is `STATE_MACHINE.md` Q3 and is carried as blocking in
`THREAT_MODEL.md` §9.2 (OQ-E).

### 8.5 How a timeout becomes evidence — nothing does, in version 1 (D-015)

> **Normative.** No certificate is produced, so **no timeout becomes evidence in
> this version.** The offline verifier specified below has nothing to verify,
> and the two costs D-015 accepts are exactly the two things this section used to
> deliver: there is **no signed record of who timed out**, and **a later version
> cannot adjudicate today's transcripts** for timeout questions, because the
> evidence was never produced. Both are accepted rather than mitigated — under
> D-010 the evidence had no consequence, so nothing was going to be adjudicated
> from it. What a transcript still shows is the stage that stalled and whose
> event is missing from it, unsigned and legible to anyone holding the
> transcript, which is what a human would have read in any case.
>
> The verifier below is retained as the specification a later version
> implements. It is also the reason §9.6's fuzz target 5 is retained: a receiver
> of **this** version must still refuse a `TIMEOUT_CERT` without being harmed by
> its contents, and it refuses at §4.0 step 6 — before the nested `SignedEvent`s
> are decoded — which is a shallower and safer path than the one that target was
> written for, not a deeper one.

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
subject no chips and does not remove it from the table.

**The last sentence of this section used to defer to an open question and now
answers it.** It read: *"The certificate is produced because it is the artefact a
human or a later version adjudicates from — whether producing it is worth its
cost while nothing consumes it is the open question §12 records."* **D-015
answers it: it is not worth its cost, and it is not produced** (§12, `OQ-F`
closed). The cost was measured — 26 MiB of a 28.3 MiB per-hand anti-replay
worst case, the client's largest attacker-influenced allocation (§5.3) — against
a benefit D-010 had already reduced to a record nothing reads.

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
| `SEARCH_PRESENCE` (`SEARCH_PRESENCE_MAX`) | 512 | ~150 |
| `LOBBY_SNAPSHOT_REQUEST` | 128 | ~45 |
| `LOBBY_SNAPSHOT_RESPONSE` | 262 144 | ≤ 128 × ~500; the hard ceiling is ≤ 128 × 1 536 = 196 608 |
| `LOBBY_CHAT` | 2 048 | ~120 |
| `JOIN_REQUEST` | 512 | ~180 |
| `JOIN_ACCEPT` | 8 192 | ~1 800 |
| `JOIN_REJECT` | 128 | ~45 |
| `PLAYER_LIST` | 2 048 | ~1 200 |
| `TABLE_READY` | 1 536 | ~200[^ready-cap] |
| `RNG_COMMIT` | 64 | 34 |
| `RNG_REVEAL` | 128 | 68 |
| `HAND_INIT` | 512 | ~140 |
| `DECK_INIT` | 256 | 102 |
| `SHUFFLE_STEP` | 8 192 | 3 435 |
| `SHUFFLE_PROOF` | 16 384 | 5 615 |
| `DECK_COMMIT` | 256 | 101 |
| `DEAL_PRIVATE` | 4 096 | ≤ 18 × 132 = 2 376 |
| `BOARD_REVEAL` | 1 024 | ≤ 3 × 132 = 396 |

[^ready-cap]: **1 024 until 2026-09-02, and 1 024 could not hold a `TABLE_READY`
    this specification calls legal.** §1.3 permits 32 capabilities of 32 bytes
    each. As the sorted array of byte strings §1.3 requires, that is
    32 × (2 + 32) = **1 088 B for `capability_set` alone**, before
    `roster_hash` (34), `table_params_hash` (34), `list_serial`, `my_seat` and
    two array headers — about 1 160 B of payload. A conforming peer that filled
    its capability set would have been refused for sending exactly what §1.3
    permits.

    1 536 is not a new invention: it is `TABLE_AD_SIGNED_MAX`, and it is derived
    the way §9.4 derives every other container — from the collection bound of
    what it holds. `S1-W`.
| `SHOWDOWN_REVEAL` | 512 | 264 |
| `SHOWDOWN_MUCK` | 64 | ~10 |
| `ACTION_*` | 64 | ~20 |
| `TIMEOUT_VOTE` | 256 | **n/a — not produced in version 1 (D-015)**; the defined body is ~60 |
| `TIMEOUT_CERT` | 8 192 | **n/a — not produced in version 1 (D-015)**; the defined body is ≤ 9 × ~250 |
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
| votes in `TIMEOUT_CERT` | `MAX_SEATS - 1` = 9 | ascending by voter seat, unique. **Never reached in version 1 (D-015)** — the message is dropped at §4.0 step 6, before its payload is decoded, so this bound is the one a later version restores rather than one this version enforces |
| `evidence` in `DISPUTE` | 4 | each ≤ `MAX_EMBEDDED_EVENT` = 32 768 B |
| `MAX_DISPUTES_PER_SENDER_PER_HAND` | 8 | accepted distinct `DISPUTE`s from one sender for one hand; `DISPUTE` is unchained and has no stage slot to bound it (§4.9, §5.3) |
| `capabilities` | 32 | each name ≤ 32 B, sorted, unique |
| adverts in a snapshot (`SNAPSHOT_MAX_ADS`) | 128 | each a complete `SignedEvent` ≤ `TABLE_AD_SIGNED_MAX` = 1 536 B; 128 × 1 536 = 196 608 B, inside `SNAPSHOT_RESP_MAX` |
| `ledger_delta` in `HAND_INIT` | `MAX_SEATS` | ascending by seat, unique |
| `MAX_STAGES_PER_HAND` | 2 048 | exceeding it aborts the hand |
| `MAX_TRACKED_TABLES` | 512 | the row shown longest makes way, D-055; never the table this client is at |
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
   deepest recursion the protocol has. **Retained under D-015 with its target
   changed rather than deleted**: this version never runs that verifier, so what
   the target must now establish is that a `0x0601` or `0x0602` frame, however
   malformed, is **dropped at §4.0 step 6 without its payload being decoded at
   all** — the nested-decode path must be unreachable, not merely safe. A fuzz
   target deleted because its subject was deferred is how a deferred subject
   comes back unfuzzed; a later version restores the deep target beside this
   shallow one.

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
* **An emitter for a type that is defined but not produced — and this is the
  route D-015 leaves open.** `0x0601`, `0x0602` and `DISPUTE kind = 2` already
  have code points, shapes, `event_class` values, slot-key arms and §4.11 rows,
  and this version drops all three unopened. A later minor version may begin
  emitting them **behind a negotiated capability**, and nothing about the wire
  has to change for it: the receiver rule this version states — drop, no fault —
  is exactly the rule that lets a v1 client sit at a table with a client that
  emits them and keep playing. **Two obligations come with that route.** The
  definitions may not be narrowed in the meantime (header box, point 3), and the
  emitting version may not treat a v1 client's silence as a vote: a `V(subject)`
  computed over peers who negotiated the capability is a `V` shrunk by
  something other than a completed certificate, which is the N3 attack under a
  new name (§8.3).
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
| **a founder advertising a deadline every legal hand exceeds** — `hand_deadline_ms` is a **signed table parameter the founder chooses**, so a founder who wants every hand at their table to abort neutrally needs no attack at all: they advertise a small number. Free, no message, no key, no coalition, and it reaches the one abort path that attributes nobody | §7.2 rule 2a **and nothing else**: the joiner derives `HAND_DEADLINE_MIN(n(11))` from the advert's own fields and **refuses the table** before a seat is taken (§8.2, `G4-P3`, `G5-Q6`). There is no second line of defence, because by the time a seat is taken the value is inside `table_params_hash` and every peer holds it | owed to `THREAT_MODEL.md` §5.2; filed as **`G4-P3-e`** in `DECISIONS.md`'s open list (third point) |
| **replaying a signed, *agreeing* checkpoint-8 `STATE_HASH` to re-enlarge a required emitter set once per hand** — also free, also no key, valid for the 4 096 hands §5.3 retains a record for | §4.9: the readmission set widens an **accepted** emitter set and never a required one, so only a seat's own **current-chain** signature can grow `R` (§4.4, §3.2, `P2`) | `THREAT_MODEL.md` X8, same family |
| faulting a table with a false `state_hash` | §6.3 case (c) and §6.4 | `THREAT_MODEL.md` X29 |
| a provably illegal message — bad signature, non-canonical encoding, out-of-range field, failed proof, illegal action against an agreed checkpoint | the `DISPUTE { kind = 3 }` carrier and the two-tier removal rule of §4.9, `HAND_ABORT cause = 6` (§4.10), and the re-entry bar on `PLAYER_SIT_IN` (§4.10) | `DECISIONS.md` D-014 |
| two peers privately playing on as if the other were gone | the boundary checkpoint of §4.9 and §6.2 row 8, compared over a set wider than the one it is required of (field 28, `signed_this_hand`, was inside `state_hash` and is deleted — §6.1 and §4.9's box); §3.2's solitary-stage rule, fired by §4.0 step 10b through **both** its routes, membership and comparison; and §4.9's floor on the reconciliation stage, `\|R(c) ∪ W\| >= 2`, which is what stops the frozen peer from releasing its own freeze (§6.3 step 3) | `DECISIONS.md` K-1 |

**Three statements of this document's own limits, kept here because they are
about the wire and not about the threat model:**

* **A mechanism above is a *check*, never a *sanction* — with exactly one
  exception, which D-014 created and which is stated here rather than hidden in
  it.** Every other row ends in a rejection, a buffered event or a signed record.
  **No row ends in a chip moving or a key being blocked**, and that is unchanged:
  D-010 point 3 and D-011 rule 3 stand in full. The one row that ends in a seat
  being taken is D-014's, and it is admissible for the reason D-014 gives and for
  no other — the evidence is a message **signed by the accused** whose illegality
  every peer decides alone, so there is no vote, no quorum, no timing and no
  per-receiver judgement in it, which are the four things that produced every
  defect D-010 closed. A reader who reads the rest of the left column as a list of
  things that are punished has still read the wrong document.
* **§11's old bullet list is where the rage-quit escape was disclosed**, and the
  disclosure is not weakened by moving it: `THREAT_MODEL.md` §9.1.2 limitation 4 and
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
| **Q-02** | **Answered by D-036 (2026-09-11): several seats quiet at one stage are named together by one certificate, whose voter set is every other live seat and whose votes cover every seat named; legal only when the voters are at least two and outnumber the named (§8.3, §8.4).** The question was: **how is a multi-subject deadline certificate constructed, and whom does it attribute?** `signers == participants \ {subject}` was unachievable when two or more seats were unresponsive at once, because each required voter set contained the other subject; the exclusion rule that removed already-subject seats from `V` had been deleted by D-008 because it let one client shrink `V` to itself, and simultaneous subjects then deadlocked until `hand_deadline_ms`. D-036 keeps D-008 whole — nothing leaves `V` on a bare vote — and attributes the whole set at once, on the word of everybody outside it. Same question as `STATE_MACHINE.md` Q3 and `THREAT_MODEL.md` OQ-E, answered with it. | `STATE_MACHINE.md`, Phase 4 | project owner |
| **Q-03** | Should `TABLE_READY` require every participant to have completed a §1.2 handshake with every other, or is founder-mediated introduction acceptable when a pair cannot connect directly? Requiring a full mesh is the safe answer and is what §1.5 specifies, but it means one unreachable pair prevents a table that would otherwise form. Relates to D-004's symmetric-NAT case. | `NETWORK_STACK.md`, §1.5 | project owner |
| **Q-04** | **CLOSED.** Should the certificate stage be collective instead of a `CERT_SETTLE_MS` timer with a lowest-seat tie-break? **Answer: collective** (§4.8). A certificate's body is a pure function of the votes, so by §3.2's stage-kind principle it carries no choice and must not have a single writer; the emitter set is `V(subject)`; the timer, the tie-break and the chain fork all disappear together, and `CERT_SETTLE_MS` is deleted from §13. | — | closed by the Phase 0 fix plan, C-10 |
| **Q-05** | **Moot for `kind = 2` in version 1 (D-015), which is not produced; live for `kind = 1` and `kind = 3`.** Does a `DISPUTE` need to be gossiped to the whole lobby, or only within the table mesh? Lobby-wide gossip gives non-participants durable evidence of equivocation, which is the only reputational pressure play money has; it also creates a defamation and spam vector, since a `DISPUTE` is cheap to emit and its `note` is attacker-controlled text. | `THREAT_MODEL.md`, §7.6 | project owner |
| **Q-06** | Should the per-hand transcript be persisted in full to the profile directory by default? It is the only artefact behind §6.3 case (c)'s diagnostic claim — the transcript is necessary for it, and, pending **OQ-A**, not sufficient — and it is small (~20 KB heads-up, ~60 KB six-handed). But it is also a permanent record of every hand every opponent played, which has its own privacy cost. | `storage/`, `THREAT_MODEL.md` | project owner |
| **Q-07** | On `HAND_ABORT cause = 4` (unresolvable divergence) the chips are restored, because no peer can be attributed, so any single peer has a free escape from a losing pot at the price of the table (§6.4). The alternatives — forfeiting an unnamed party's commitment, or settling from the last `STATE_ACK`-agreed checkpoint — each need a numbered decision and neither is adopted here. D-010 decides the first half **against** for the MVP; what stays open is whether settling from the last agreed checkpoint is worth building. Formerly this document's `OQ-D` (R-2). | §6.4, `STATE_MACHINE.md` | project owner |
| **Q-08** | **Moot in version 1 (D-015): there are no votes.** Should a required voter be obliged to publish a signed `ACTION_SEEN { sequence, event_hash }` before it may vote, so that vote-and-seen are two events by one key in one slot and a lying voter becomes provable (§8.3)? A design change with a cost in messages and latency; not adopted. Formerly this document's `OQ-C` (R-2). | §8.3, §8.4 | project owner |
| **Q-09** | **CLOSED (K2), and closed a second time in this pass for a second message (L1).** *Which chain does an event that sits between two hands belong to, and at what `sequence`?* It was asked of the hand-boundary single-writer events and answered for them; **checkpoint 8** arrived at the same position from `STATE_MACHINE.md` with the same four quantities undefined, and §4.9's checkpoint-8 box answers it in the same shape — chain `k`, `hand_id = k`, parent `TERMINAL(k)`, `sequence` in the reserved band `BOUNDARY_CHECKPOINT_BASE … +15`, total order by the collective stage rule. **The reusable finding is that the position between `TERMINAL(k)` and `HAND_INIT(k+1)` needs a rule per message that occupies it, not one rule**, and the next message placed there will need a third. **Answer for the boundary events, in §4.10's boundary-window box:** chain `k`, `hand_id = k`, `sequence = BOUNDARY_SEQUENCE_BASE + sender_seat`, `previous_event_hash = TERMINAL(k)` for every event of the window, total order ascending seat index, one event per seat per boundary, window closing at this receiver's acceptance of a complete `HAND_INIT(k+1)`. The grade this row carried — *"a placement decision, not a wire change"* — **was wrong**: `hand_id` and `sequence` are inside `TO_BE_SIGNED` (§2.4) and in the slot key (§5.2.1), so two placements are two signed byte strings for one intent. Chain `k+1` is refused because a boundary event would then chain from a `GENESIS(k+1)` that a `PLAYER_LEAVE` is an input to. | — | closed in this pass |
| **Q-10** | **What ratifies a contribution to the stage that stalled?** For every stage that completed, `P(k)` (§3.2) is exactly `stage_hash` membership and two peers holding the same prefix agree by construction. For the one stage that stalled — the reason the hand aborted — no `stage_hash` exists, so "seat `s` contributed there" is strictly *who was heard*, the quantity P3 refused. **Named default, adopted in §3.2: a contribution to the stalled stage counts; a terminal `HAND_ABORT` and a `PLAYER_LEAVE` never do; and a peer's own emission does (K1's sub-question, answered).** It cannot simply be excluded: a hand that stalls at `HAND_INIT` completes no stage, so excluding it empties the next hand's set. **The claim that the residual was "loud rather than silent" is withdrawn and was K1**: it costs a hand at the deadline on the *first* iteration and a permanent silent fork on the second, because both peers then narrow to `{self}` and every collective stage self-completes. What replaces it is not a ratification but a detection — §3.2's **solitary-stage rule**, which makes a peer whose `P` has narrowed to itself freeze on the first contradicting event instead of playing on. **That rule could not fire as first written and now can (L4):** its trigger is a property of the hand the event names, answered from §5.3's retained record at §4.0 step 10b, because a solitary hand completes in microseconds and the contradicting event is late by construction. A second detection route was added with it: §6.2's checkpoint 8, compared over a wider set than it is required of (§4.9), which until field 28 was deleted carried `signed_this_hand` inside `state_hash` so that agreement there was agreement about `P` itself — §6.1 records why that was unattainable between honest peers and what replaced it. `Q-10` itself stays open: nothing ratifies the stalled stage, and no construction can, because agreeing it needs a collective step at the point collectivity failed. **Which of the two routes a client runs, decided 2026-09-19 by the project owner (`DECISIONS.md` `S1-CH`): the checkpoint route alone.** The retained-record trigger of §4.0 step 10b is specified here and in `STATE_MACHINE.md` (T62, T63), and its predicate stands in the code with its tests, but no path of the client calls it -- and that is a decision now, where it was an omission: T63 is the one transition that opens a closed table again; the two wins a bidirectional partition leaves behind are play money and cost nobody anything; and what a client keeps about a finished game (`DECISIONS.md` D-067, D-068) has no road back from a win once it is written down. So the containment of `Q-10` in this version is checkpoint 8, and the solitary tournament win stays the residual this entry always named. The trigger is to be wired the day a win first means something to somebody else -- a published ranking -- together with what its reversal does to that record. Same question as `STATE_MACHINE.md` **Q8**, which filed it here; referenced, never redefined there. | §3.2, §4.4 | project owner |

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

**OQ-F** — whether the machinery is produced at all. **CLOSED by D-015: it is
not.** The question was:

> Whether `TIMEOUT_VOTE`, `TIMEOUT_CERT` and `EquivocationProof` should still be
> *produced* in the MVP now that D-010 gives them no effect, or be deferred
> wholesale until the machinery is sound. Producing them keeps the transcript
> adjudicable later; deferring them removes four passes' worth of surface.

**The answer, and what it did and did not touch in this document.** Nothing
emits or consumes any of the three in version 1 (header box). What was
**deferred**: every emission path, and §5.3's two subject-class structures,
which are not allocated. What was **kept**, deliberately, so that a later
version is a capability rather than a wire break: the two deadline classes and
both subject axes in §5.2.1's key, `V(subject)` and its inductive exclusion rule
in §8.3, §8.5's offline verifier, the `EquivocationProof` object and its
`DISPUTE kind` in §5.2.4, and both rows of §4.11. **The measurement that decided
it is §5.3's table** — 26 MiB of a 28.3 MiB per-hand worst case were the two
timeout classes, against a benefit D-010 had already reduced to a record nothing
reads.

**What the closure moved rather than settled**, recorded here so no reader takes
D-015 for more than it is:

| Was blocked on the machinery | Now |
|---|---|
| `Q-02` — the multi-subject certificate | **not blocking version 1.** Every stall takes the `hand_deadline_ms` path (§8.4), so there is no simultaneous-subject case to construct. Stays open for the version that reinstates the certificate |
| `Q-08` — an obligatory `ACTION_SEEN` before a vote | **moot in version 1**, since there are no votes. Returns with the certificate |
| `Q-05` — lobby-wide gossip of a `DISPUTE` | **moot for `kind = 2`**, which is not produced; still live for `kind = 1` and `kind = 3` |
| whether an equivocation is provable to a third party | **it is not, and that is a cost rather than a question** — carried as `D-015-2` in `DECISIONS.md`'s open list and as a standing limitation in `THREAT_MODEL.md` §9.1.0, to be decided before real money |


### Closed elsewhere, recorded here so the answer is not lost

`STATE_MACHINE.md` **Q7** — *what marks a seat `Absent` at all?* — is **closed by
D-013, by dissolution rather than by an answer** (J4, J5). Q7 was asked because a
seat's status was the table's liveness gate; §3.2 and §4.4 have moved that gate onto
`P(k)`, demonstrated participation in the agreed chain, and no rule in this document
now reads a seat's presence for any purpose. **Nothing marks a seat `Absent`,
nothing needs to, and this document hashes no such vector** (§6.1).

**The escape route Q7 named is withdrawn rather than left standing, and that is
J4.** It read that a silent seat "is dealt in again every hand and stalls each one
for `hand_deadline_ms` **until a human sits it out (T59) or leaves (T58)**". That
route did not exist. `PLAYER_SIT_OUT` and `PLAYER_LEAVE` are single-writer **by that
seat** (§4.10, §4.11 rows 37 and 39), so the only human who could take it was the
one who was not there; no participant and no quorum of participants had any action
that changed that seat's status. Under D-013 no escape is needed and none is
claimed: the silent seat stalls exactly one hand, is outside `P(k)` from the next,
drains on the blinds and busts. **A route that does not exist must not be left in a
document as a remedy**, because an implementer builds a GUI against it — a wait
that never ends, offered where the only real action was to leave.

`STATE_MACHINE.md` **Q2** — whose key signs a derived `HAND_INIT` /
`HAND_COMPLETE`, and is a counter-signature needed — is **closed** by the
collective-stage form of §3.2 and §4.4: every seat of `P(k-1)` signs its own
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

LOBBY_DERIVATION_STRING         = "p2p-poker/main-lobby/v1"
LOBBY_NAMESPACE_KEY             = 12207e342925602a7c6558d6ac574207bcc56f7989e17ad52b7694f2c964d772c4b6
RELAY_DERIVATION_STRING         = "/libp2p/relay"
RELAY_NAMESPACE_KEY             = 1220245eebd20d2cd4c81b5d4ac27c73746279f436d62f3ef52c452a369e6ef7b610

  Each key is 34 bytes: the multihash prefix 0x12 (sha2-256) 0x20 (32-byte
  digest) followed by sha256(derivation string). That is how go-libp2p's routing
  discovery keys a namespace, and matching it is what lets a p2p-poker client and
  a go-libp2p client agree on where the lobby is.

  **These four lines were wrong until 2026-09-02 and this is the one place in
  the corpus where being wrong is fatal to interoperability.** They still named
  `"p2p-poker/mainline-lobby/v1"` and a 20-byte BitTorrent infohash,
  `fd7c0d69…`, four days after `56b0b50` moved discovery to a libp2p Kademlia
  provider record and deleted the `mainline` crate. So `PROTOCOL.md` §13 and
  `NETWORK_STACK.md` §3.2 gave one name two values, which is exactly what
  `NETWORK_STACK.md` §14's *"no value has two names anywhere in the corpus"* is
  there to prevent, read from the other end — and a second implementation built
  from §13 would have announced in the BitTorrent DHT and never found ours.
  `S1-AB`.

  The relay derivation string is **not** ours and deliberately so: `/libp2p/relay`
  is where go-libp2p's own AutoRelay advertises and looks, so a p2p-poker
  volunteer is findable by any libp2p client and vice versa. A private string
  here would have made this project's relays invisible to the network whose
  relays it wants to use.

  Pinned by `the_lobby_rendezvous_key_is_the_published_one` in `src/net/run.rs`,
  which asserts the bytes rather than recomputing them: a test that recomputes
  the thing it checks passes whatever the code does.

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
BOUNDARY_SEQUENCE_BASE          = 4 096         (§4.10's hand boundary window;
  a boundary event of chain k carries sequence = BOUNDARY_SEQUENCE_BASE + seat,
  so the window occupies 4 096 .. 4 105 and is disjoint from every stage index,
  which MAX_STAGES_PER_HAND bounds at 2 047. Only §4.9's boundary checkpoint may
  carry a sequence >= BOUNDARY_SEQUENCE_BASE besides these; no other chained type
  may.)
BOUNDARY_CHECKPOINT_BASE        = 8 192         (§4.9's checkpoint-8 box;
  checkpoint 8 of chain k is a STATE_HASH stage at BOUNDARY_CHECKPOINT_BASE and a
  STATE_ACK stage at +1, with reconciliation round r at +2r and +2r+1, 1 <= r <= 7,
  so the band is the sixteen values 8 192 .. 8 207. It sits ABOVE the boundary
  window because a reconciliation round extends upwards and must never reach the
  window's reserved per-seat slots.)
RETURN_SEQUENCE_BASE            = 8 224         (§4.8's return band, D-028; a
  RETURN_VOTE and a RETURN_CERT about seat s of chain k are sealed at
  RETURN_SEQUENCE_BASE + s, parented on TERMINAL(k), so the band is the ten
  values 8 224 .. 8 233, above the last reconciliation ack at 8 207 with room
  to spare. The three boundary bands never meet; `returnwire::tests` holds
  that.)
RETURN_GRACE_MS                 = 6 000         (client liveness, §8.3.1: how
  long the next deal is held at a boundary while a return is in flight; not a
  wire rule, and two clients that disagree about it disagree about nothing on
  the wire.)
RESUME_GIVE_UP_MS               = 600 000       (client liveness, D-029: how long
  a client keeps trying to rejoin an unfinished session once the table's
  advertisement is gone and no peer of the session has answered; while the
  advert is up the attempt never ends on a timer, and a finished session is
  learned from the founder's refusal.)
RESUME_RECORD_MAX_AGE_MS        = 1 800 000     (client liveness, D-031: how old a
  session record may be and still be offered at start; an older one names a
  game that is long over and is dropped instead of asked about.)
HEADS_UP_STAGE_BUDGET_MS        = 100 000       (client liveness, D-031: heads-up,
  the stage budget of a hand both seats have signed. Nobody can vote at two
  seats, so a stage budget's only effect there is a unilateral give-up, and a
  give-up of a hand the other seat is in must wait out a brief outage the
  carrier repairs by itself within CARRIER_GIVES_UP_MS. A hand the other seat
  never signed keeps crypto_step_timeout_ms.)
MAX_RETURNS                     = 3             (client liveness, D-032: how many
  times a seat may come back to a table it dropped out of. At three seats or
  more a return is a certificate and every seat counts them; at the limit a
  client votes for no further return of that seat, and a certificate needs
  every voter. Heads-up the other seat's client counts the absences it asked
  about, and at the fourth ends the game.)
DECISION_MS                     = 30 000        (client liveness, D-034: how long a
  seat at one of this client's own tables has to decide once it is its turn;
  the window's clock runs over exactly this. The rated preset keeps its own
  numbers.)
DECISION_GRACE_MS               = 3 000         (client liveness, D-034: the
  network's share on top of DECISION_MS, not the player's; the table's
  certificate can fold a seat after the two together, and the client's own
  tables offer no reserve beyond.)
MAX_RETAINED_HAND_RECORDS       = 4 096         (§5.3's retained hand record, the
  per-hand (hand_id, was_solitary, p, checkpoint8_state_hash) tuple §4.0 step 10b
  evaluates a stale-hand event against, where p is P(hand_id - 1) and was_solitary
  is the disjunction P(hand_id - 1) == {self} || P(hand_id) == {self} — NOT
  p == {self}; §3.2 says why the second disjunct cannot be dropped. LRU; ~56 B
  each. It is the ONLY per-hand structure a stale-hand
  event touches: §4.0 step 10a is skipped for such an event and no store is
  allocated for a finished hand, which is N2's bound.)

Both reserved bands are EXEMPT from the MAX_STAGES_PER_HAND abort of §5.3: that
count is of stage indices below BOUNDARY_SEQUENCE_BASE, and each band is bounded
by its own rule instead — one boundary event per seat, one checkpoint-8 pair per
seat per round. An implementer who range-checks sequence < MAX_STAGES_PER_HAND
rejects every boundary event and every boundary checkpoint.
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
SEARCH_PRESENCE_EVERY_MS        = 30 000        (D-064, §7.13: the queue's heartbeat)
SEARCH_PRESENCE_TTL_MS          = 90 000        (three heartbeats, the advert's ratio)
SEARCH_PRESENCE_MAX             = 512           (the whole signed event)
  (REANNOUNCE_INTERVAL_MS, 600 000, stood here until 2026-09-15: Mainline's
  ten-minute re-announce, read by nothing since 56b0b50. A provider record is
  republished by the Kademlia library on its own interval; NETWORK_STACK.md
  section 10.1. No peer ever parsed it, so its removal changes no wire.)
IDLE_CONNECTION_TIMEOUT_MS      = 60 000
MDNS_QUERY_INTERVAL_MS          = 15 000
SNAPSHOT_PEER_COUNT             = 4
MAX_TRACKED_TABLES              = 512           (local)
MAX_TRACKED_PRESENCE            = 8 192         (local)
MAX_ADS_PER_TABLE_KEY_PER_MIN   = 4             (local)
MAX_ADS_PER_PEER_PER_MIN        = 90            (local)
MAX_PRESENCE_PER_PEER_PER_MIN   = 4             (local; and the same figure for
  SEARCH_PRESENCE on a map of its own, §7.13)
MAX_TRACKED_SEARCHERS           = 2 048         (local, D-064)

HANDSHAKE_DEADLINE_MS           = 15 000
MAX_CONSECUTIVE_AUTO_ACTIONS    = 3

RATED_SNG_POKERTH_V1 — all twenty-five parts of table_params_hash (§3.1), in
that box's order, so this block can be diffed against it field by field:
  n(0)    game                  = 1     NLHE                  sole legal value §7.2
  n(1)    mode                  = 2     TOURNAMENT_SNG_PLAY_MONEY      (G7-S3)
  n(2)    preset_id             = "RATED_SNG_POKERTH_V1"
  n(4)    small_blind           = 50                          == n(13.2), see below
  n(5)    big_blind             = 100                         == 2 * n(4), §7.2
  n(6)    ante                  = 0                           sole legal value §7.2
  n(7)    min_buyin             = 10 000                      == n(9)        (G7-S3)
  n(8)    max_buyin             = 10 000                      == n(9)        (G7-S3)
  n(9)    start_stack           = 10 000
  n(11)   max_players           = 10        (written "seats" before this pass)
  n(12)   min_players_to_start  = 10
  n(13.0) blind_schedule.mode   = 1     DOUBLE_EVERY_N_HANDS  sole legal value §7.2
  n(13.1) every_n_hands         = 11
  n(13.2) first_small_blind     = 50
  n(13.3) small_blind_cap       = 50 000                      = n(11)*n(9)/2
  n(14)   action_timeout_ms     = 20 000
  n(15)   action_grace_ms       = 5 000
  n(16)   crypto_step_timeout_ms = 30 000
  n(17)   hand_deadline_ms      = 3 300 000  (was 600 000 — G4-P3. n(11) = 10, so
    HAND_DEADLINE_FLOOR(10) = 7 000 + 43*30 000 + 40*25 000 = 2 297 000, and the old
    value was below its own floor: a legal no-re-raise hand is 40 actions at 25 000 ms
    = 1 000 000 ms of human time alone. 3 300 000 clears the floor by 1 003 000 ms,
    which buys REOPENINGS = 1 003 000 / (9 * 25 000) = 4 reopening raises per hand,
    and stays under n(17)'s 3 600 000 cap. §8.2 derives both formulae.)
  n(18)   join_deadline_ms      = 120 000
  n(19)   hand_delay_ms         = 7 000
  n(20)   button_rule           = 1     DEAD_BUTTON           sole legal value §7.2
  n(21)   odd_chip_rule         = 1     FIRST_SEAT_LEFT_OF_BUTTON  sole legal value §7.2
  n(22)   showdown_policy       = 1     MANDATORY_REVEAL      (pending Q-01)
  n(24)   deck_suite            = "bs-bg12-secp256k1/1"       sole legal value §7.2
  not a part of table_params_hash, and stated only so its absence is not read
  as an omission: n(23) password_required = false (no password)
```

**Why the block is now indexed to `n(…)`, and this is `G7-S3`.** `n(1) mode`,
`n(7) min_buyin` and `n(8) max_buyin` are parts of `table_params_hash` and this
block pinned **none** of them; §7.2 gives each only a range. Two clients both
correctly implementing `RATED_SNG_POKERTH_V1` therefore computed different
`table_params_hash` values and **neither was wrong**, so they could not join each
other's table and §7.2 rule 3 — *the name implies §13's exact values* — had no
values to imply. That is `G6-R1`'s defect a third time, and it was invisible
because a **missing** line reads as an omission rather than as a contradiction.
The three values are not choices: `mode` is forced to `2` by `n(9) start_stack`,
which §7.2 admits in tournament modes only; and a Sit-and-Go's buy-in **is** its
starting stack, every entrant getting an equal stack, so `n(7) = n(8) = n(9)` —
§7.2 rule 2 now enforces that in tournament modes rather than leaving it to be
restated per configuration. The remedy for the class is the indexing: a named
configuration is audited by diffing its block against §3.1's twenty-five parts,
and a part with no line is visible at a glance. **Every part carries a line**,
including the ones §7.2 admits a single legal value for in version 1: those are
pinned by the range itself and are marked *sole legal value* rather than omitted,
because a part left out to save a line is indistinguishable from a part nobody
thought about — which is the whole of this finding.

**`n(4) small_blind` is the level-1 blind and cannot be anything else.** It is a
distinct part from `n(13.2) first_small_blind`, so a configuration that pins only
the schedule pins only one of the two. Nothing has to be added for it: an advert
is withdrawn when the table starts (§7.3 `reason = 1`) and §7.2 rule 7 discards
any re-broadcast whose parameters changed, so an advertised table has dealt no
hand and is at level 1, where `small_blind == first_small_blind`. §7.2 states the
equality as a receiver check so that the two parts cannot be filled in
independently.

**`hand_deadline_ms` is not a constant of this section any more; it is a derived
lower bound and a per-table parameter above it (`G4-P3`).** `HAND_DEADLINE_FLOOR(n)`,
`REOPENING_COST(n)` and `HAND_DEADLINE_MIN(n)` are **defined in §8.2 and are not
reproduced here**, and neither are their values at each seat count: §8.2's two
tables are the only ones in the corpus. This section carried a second copy of both
the formulae and the table until this pass, which is a value no test validates
sitting in two places — the shape that produced `G6-R1` and `G7-S1`, and the reason
`G7-S6`'s missing `n = 8` row would otherwise have had to be added in three places
rather than the two §8.2 owns. What §13 owes an
implementer here is the pointer and nothing else, because there is no longer a
number to find: the deadline is the founder's, bounded below by §8.2's derivation
and above by `n(17)`'s cap.

A receiver rejects any `LOBBY_TABLE_AD` whose `n(17)` is below
`HAND_DEADLINE_MIN` (§7.2 rule 2a). **The admitted minimum is the floor plus one
reopening and not the floor itself — `G5-Q6`:** the floor budgets a hand nobody
re-raises, and a table sitting exactly on it aborts on the first raise that
reopens the action, with `cause = 1` and nobody named, which is `P3`'s harm one
raise later. The `n(17)` cap of `3 600 000` is unchanged and is the other side of
the bound: a configuration whose minimum exceeds the cap is unplayable and is
refused by every conforming client, which is the correct outcome and is why the
cap is not widened. `2 522 000 < 3 600 000`, so the space stays non-empty at every
seat count from two to ten.

The `RATED_SNG_POKERTH_V1` values, their PokerTH provenance, and the four
`[OUR CHOICE]` timing values are documented in [RULES B1–B5]. `crypto_step_timeout_ms`
is this document's addition; it has no PokerTH analogue and must comfortably
exceed one shuffle prove-and-propagate cycle, measured at ~100 ms of proving plus
network latency [MENTAL §5.1].

**`RATED_SNG_POKERTH_V1` is fully specified and is not playable by the MVP.** It
pins `n(11) max_players = 10` and `n(12) min_players_to_start = 10`, while `SPEC_CS.md` §32 requires
two-player heads-up as the first supported mode and §1.3 scopes the MVP at
`nlhe/2-6`. The MVP ships `CUSTOM` tables; `RATED_SNG_POKERTH_V1` becomes playable
when `nlhe/7-10` lands. This is stated rather than left to inference because a
reader who does not notice will build the wrong acceptance test —
`STATE_MACHINE.md` §9.5 carries the same statement next to its heads-up
reachability analysis, and the Phase 8 acceptance test for D-003/D-004 uses a
`CUSTOM` two-seat table.

#### The heads-up reference configuration — **not a preset, and this is `G6-R6`**

The `CUSTOM` two-seat table the sentence above names is the only table the MVP
can actually play, so its values are written down here. **They are a reference
configuration and not a `preset_id`.** The constant behind it lives in
`src/poker/tournament.rs`; version 1 has **no such preset** and §7.2 rule 3 admits
exactly two `preset_id` values, so a table built from these values advertises
`preset_id = "CUSTOM"` and carries every number below in its own advert fields.

```
the heads-up reference configuration — advertised as CUSTOM.
All twenty-five parts of table_params_hash (§3.1), in that box's order:
  n(0)    game                  = 1     NLHE                  sole legal value §7.2
  n(1)    mode                  = 2     TOURNAMENT_SNG_PLAY_MONEY
  n(2)    preset_id             = "CUSTOM"
  n(4)    small_blind           = 50                          == n(13.2)
  n(5)    big_blind             = 100                         == 2 * n(4), §7.2
  n(6)    ante                  = 0                           sole legal value §7.2
  n(7)    min_buyin             = 10 000                      == n(9)
  n(8)    max_buyin             = 10 000                      == n(9)
  n(9)    start_stack           = 10 000
  n(11)   max_players           = 2
  n(12)   min_players_to_start  = 2
  n(13.0) blind_schedule.mode   = 1     DOUBLE_EVERY_N_HANDS  sole legal value §7.2
  n(13.1) every_n_hands         = 11
  n(13.2) first_small_blind     = 50
  n(13.3) small_blind_cap       = 10 000                      = n(11)*n(9)/2
  n(14)   action_timeout_ms     = 20 000
  n(15)   action_grace_ms       = 5 000
  n(16)   crypto_step_timeout_ms = 30 000
  n(17)   hand_deadline_ms      = 1 200 000
  n(18)   join_deadline_ms      = 120 000
  n(19)   hand_delay_ms         = 7 000
  n(20)   button_rule           = 1     DEAD_BUTTON           sole legal value §7.2
  n(21)   odd_chip_rule         = 1     FIRST_SEAT_LEFT_OF_BUTTON  sole legal value §7.2
  n(22)   showdown_policy       = 1     MANDATORY_REVEAL      (pending Q-01)
  n(24)   deck_suite            = "bs-bg12-secp256k1/1"       sole legal value §7.2
  not a part of table_params_hash: n(23) password_required = false (no password)
```

**Why these numbers, since values written down have to be justified whether or not
a name carries them.** Every part except `n(11)`, `n(12)`, `n(13.3)` and `n(17)`
is the rated preset's,
unchanged, because nothing about two seats makes 50/100 blinds, a 10 000 stack, a
zero ante or an eleven-hand level the wrong choice — the rated values already carry
[RULES B1–B5] and copying them is cheaper than inventing a second provenance.
**`small_blind_cap = 10 000` is the rated preset's rule and not a new number:**
PokerTH computes the cap as `seats × start_stack ÷ 2`, half the chips the
tournament started with, which is 50 000 at ten seats and **10 000 at two**
[RULES B4]. The same formula at a different seat count is not a deviation. Two
values are genuinely this configuration's:

* **`min_players_to_start = 2`**, which is `max_players`, so the table starts when
  it is full. There is no smaller legal value, and D-007 is why a two-seat table
  is a distinct object rather than the rated preset with eight empty chairs.
* **`hand_deadline_ms = 1 200 000`**, against the preset's 3 300 000. It is a
  per-table parameter and not a constant (`P3`), and `HAND_DEADLINE_MIN(2)` is
  **1 042 000**, so this clears the admitted minimum by 158 000 ms and buys
  `REOPENINGS = ⌊(1 200 000 − 1 017 000) ÷ 25 000⌋ = 7` — more reopenings per hand
  than the rated preset's four at ten seats, which is the right way round, since
  this is the table the MVP's hands are actually played and tested on. It is under
  the `3 600 000` cap with room to spare.

**Why this is a reference configuration and not a third `preset_id`, which is the
decision `G6-R6` asked for.** A `preset_id` is a **claim about values carried by a
name**: §7.2 rule 3 makes the name imply the numbers, which is what gives
`RATED_SNG_POKERTH_V1` its worth and is exactly what made it dangerous when the
code and §13 disagreed by 600 000 ms (`G6-R1`). That claim is only worth its cost
when an outside authority fixes the values — PokerTH's own rated-game settings
check does, and there is **no analogous authority for a two-seat rated game**;
every number above is either copied or chosen here. Pinning them under a name
would manufacture the appearance of provenance for a configuration this corpus
invented, and would create a second set of values that two documents, a source
file and a test fixture must be kept in step on — the drift that produced
`G6-R1` in the first place, now with nothing outside the project to check against.

Advertising the same table as `CUSTOM` costs nothing and closes the hazard
outright: every value is in the advert's own fields and every one of them is bound
into `table_params_hash` (§3.1), so a joiner reads numbers instead of trusting a
name, and two clients whose "heads-up table" differs by one field see the
difference **before** a seat is taken (§7.2 rules 2, 2a and 7) rather than
discovering it as a failure to join. The name may go on staying in the source, in
a test fixture and in a UI menu; what it may not do is travel on the wire.

**What the code must carry for a two-seat table, now that this is a reference
configuration rather than a preset — `G7-S9`, and it is larger than `G6-R6`'s two
lines.** `G6-R6`'s two lines have landed: `src/poker/tournament.rs` renamed the
constant to `HEADS_UP_CUSTOM_2P`, its `id` is `"CUSTOM"`, and the doc comment that
claimed two clients agree on the table *by its name* is gone. What replaces the
name is a **complete advert**, and that is the part the code does not have yet.
A `Preset` carries **fourteen** of §3.1's twenty-five parts. The eleven it does not
carry are `n(0) game`, `n(1) mode`, `n(4) small_blind`, `n(5) big_blind`,
`n(7) min_buyin`, `n(8) max_buyin`, `n(13.0) blind_schedule.mode`,
`n(20) button_rule`, `n(21) odd_chip_rule`, `n(22) showdown_policy` and
`n(24) deck_suite` — and every one of them is hashed, so a client that defaults
any of them while building the advert computes a `table_params_hash` no other
client reproduces. That is `G7-S3` in the code rather than in a document, and it
is the same defect wearing the same disguise: a **missing** field reads as an
omission, never as a contradiction. Three things follow, and none is a value:

* **The advert type owns the twenty-five parts, not `Preset`.** `Preset` may stay
  as the convenience that fills the fourteen it knows; the eleven above have to be
  filled explicitly at the one place a `LOBBY_TABLE_AD` payload is built, and the
  fields §7.2 admits exactly one legal value for are *still* filled explicitly,
  because a part that nobody writes is a part nobody notices is wrong.
* **`n(2)` is typed, not spelt.** `Preset::id` is a `&'static str` today, so a
  third preset name is representable and §7.2 rule 3 rejects it only at a
  receiver. `src/protocol/constants.rs` already carries the closed enum
  (`PresetId::RatedSngPokerthV1`, `PresetId::Custom`); `id` should be that type, so
  the rule the wire enforces is the rule the type enforces and no client of ours
  can emit a name a conforming joiner will reject.
* **Units are part of the hash.** `Preset` stores `*_sec`; every timing part is
  hashed as `u32_be(…_ms)`. The ×1000 is not a display detail — a client that
  hashes seconds sits at a different table from one that hashes milliseconds, with
  no message and no error to say so.

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
Notes 21 to 24 do the same for the Phase 3 gate's findings **K1, K2 and K5**: a
deleted justification that inverted the property it cited, a rule that stops a
peer rather than preventing a fork, the boundary window that `Q-09` declined to
place, a frozen roster, and one duplicated genesis part. Notes 21 and 22 are one
fix stated twice and each names the other as its precondition.
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
chain to `GENESIS(0)`, which contains `table_params_hash` (§3.1, since D-013;
`advert_hash` stood here and is removed) and, through `session_id`,
`roster_hash(0)`. It is
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
`crypto_step_timeout_ms` — **fifty-five minutes** rather than thirty seconds under
`RATED_SNG_POKERTH_V1` (§13; this said *ten minutes* until `G7-S7`, against the
600 000 ms `G4-P3` retired). Two visible consequences: §4.10's `attributed = []` table
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
path, and under D-010 stopping the hand
sooner would have moved no chips anyway. **The clause *"the proof is still
produced and retained"* is withdrawn by D-015** — it is not, in this version —
and the bound survives without it, because the bound was always the chained
divergence path and never the proof. **`STATE_MACHINE.md` must follow**: the
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
`CRYPTOGRAPHY.md` §1, §9 and `OQ-6`'s; §1.4's libp2p behaviour column and its GossipSub and
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

**21. `P(k)`'s residual is contained rather than removed, and the containment
costs a table (K1).** §3.2 previously justified D-013 with a sentence that
inverted P3's property, and the inversion hid a permanent silent fork reachable
from one dropped frame. The sentence is deleted and the truth stated: `P(k)` is
agreed exactly where it is inert and per-receiver exactly where it acts. Three
repairs were available and two are refused in the text. **Ratifying the stalled
stage** — an abort body naming its emitter's contributor set, with `P(k)` the
union over accepted copies — is circular: it needs a collective step at the point
collectivity failed, and *which abort copy this receiver accepted* is the same
per-receiver quantity under a new name, inadmissible under D-012. **Flooring
`|P(k)|` at 2** prevents the fork outright and is the stronger guarantee; it is
refused because it deletes the drain, and the drain is D-013's entire liveness
answer — a heads-up table whose opponent leaves would pause for ever, nobody would
bust, and no end condition could fire, which is D-013's own fixed point reached
from the other side. What is adopted is the **solitary-stage rule**: a peer whose
required emitter set is `{self}` alone may deal, but a chained event of that hand
from a seat outside `P(k-1)` freezes it. **Amended in this pass (`L4`):** the rule
is a property of the hand the event *names*, answered in the past tense from
§5.3's retained record by §4.0 step 10b, because a solitary hand self-completes
every stage and the contradicting copy therefore arrives naming a hand the
receiver has already finished. As first written the rule was scoped to the
receiver's current hand and could not fire at all. Two cases are exempt —
`PLAYER_SIT_IN`, and a checkpoint-8 `STATE_HASH`, which is compared instead
(§4.9) — **and which carries the same finding and the same latch when the
comparison disagrees on a hand this peer derived alone; the exemption is from
freezing on arrival, never on disagreement (`N1`)**. **The cost is a stopped table
where there
would have been a completed one**, on every path where two live peers disagree
about `P`; `SPEC_CS.md` §19 ranks that above a silent fork and this document takes
the ranking. **The residual, which is not closed:** under a bidirectional
partition each peer drains the other and each declares itself the winner, because
no contradiction crosses a partition. That is what D-013's drain does under a
partition however `P` is defined, it is indistinguishable at both peers from the
case the drain exists for, and it is stated in §3.2 rather than argued away.

**22. `Q-09` is answered as a wire question and its old grade is recorded as
wrong (K2).** §12 graded it *"a placement decision, not a wire change"*, and
`hand_id` and `sequence` sit inside `TO_BE_SIGNED` and inside the slot key, so two
placements are two signed byte strings for one intent. §4.10's boundary-window box
fixes chain, `sequence`, parent and total order. Two consequences are recorded
rather than buried. **The window is a second exemption from §4.0 step 10a's
chain-position rule** — every boundary event chains from `TERMINAL(k)`, not from
its predecessor — because an emitter cannot know which other seats will emit; the
exemption count in this document is now two, and both are stated where they arise.
**And the window closes per receiver**, which is safe only because §3.1 freezes
the roster's seat vector so the window cannot reach `GENESIS(k+1)`. The
alternative was to let `PLAYER_LEAVE` change `roster_hash` and to define a
collective close for the window: a new required emitter set at a hand boundary,
which is a new liveness gate in the place D-013 spent a decision removing one. It
is not taken. **What that leaves is a stalled stage 0 on a window disagreement,
which is the one path where `P` narrows — so note 21's rule is a precondition of
this one**, and an editor who weakens either has restored J1's failure mode
through the other.

**23. The roster's seat vector is frozen by deletion, and one route disappears
with it (K2).** §3.1's clause *"change only at a hand boundary through an accepted
`PLAYER_LEAVE` or seat entry"* is deleted. Its seat-entry half named a message
that does not exist — §4.3 is formation only and §4.1 ends the table key's
authority at `TABLE_READY` — and §4.3 already said the roster cannot be added to,
removed from or reordered without a new table, so the deletion aligns two sections
that disagreed rather than deciding between them. The cost is that **a departing
seat keeps a row in every `roster_hash` for the life of the table**, and that
version 1 has no mid-session seat entry and no re-buy; both were already true and
neither was written down. What is *not* decided is when the departing stack is
subtracted, which is `K-8` in `DECISIONS.md`'s open list: `roster_hash(k)`'s stack
vector is `TERMINAL(k-1)`'s `final_stacks` whatever the answer, which is what
makes leaving it open safe.

**24. `GENESIS(0)`'s duplicate part becomes a sentinel rather than being removed
(K5).** Deleting `table_public_key` outright would leave the two genesis forms
with five and six parts under one domain string, and `h` (§2.8) carries no arity.
That is a preimage-shape obligation created for nothing, so the slot keeps its
place and carries `ZERO32`, reading as *no session yet* beside slot 6's *no
previous terminal*. The cost is one part hashed that binds nothing, which is
exactly what the old text had; what changes is that it is no longer a second copy
of `table_id` that an implementer can accidentally make differ.

**25. A reconciliation stage may not be satisfiable by the peer it is meant to
un-fork, and the fix is a per-receiver enlargement with a floor (`N1`).** §4.9's
reconciliation round was required of *the emitter set of the checkpoint it
re-derives*, which at a solitary peer's boundary checkpoint is `P(k) = {self}`, so
the peer completed the stage in the step it emitted into it, agreed with itself,
and released the freeze §3.2 had installed — including the latch §9.3's end
condition reads. Every fix in the previous pass that routed a disagreement into
§6.3 was routing it there. The set is now `R(c) ∪ W`, `W` being the seats whose
signatures froze this receiver, and it never has fewer than two members.

Three things are recorded rather than buried. **`W` is per-receiver**, the only
required emitter set in this document that is, and §2.9's sweep now carries it
with the test that admits it: a per-receiver quantity that can only **enlarge** a
required emitter set is admissible, because it can delay a completion and never
manufacture one, and D-012 forbids per-receiver **canonical state**, which `W`
does not touch. **A silent contradicting peer now leaves this peer frozen
permanently**, which is a liveness cost paid deliberately — §6.3 step 3 states the
terminus, the table closes at the next boundary with no winner named, and
`SPEC_CS.md` §19's ranking is what makes that the right price. **And §6.3 step 2's
`DISPUTE` became a liveness precondition**: a peer whose own copies agreed is
brought into the reconciliation stage only by the dispute every frozen peer is
obliged to broadcast. That obligation already existed; what is new is that
withholding it now stalls a stage instead of merely being quiet.

**26. A chained event of a finished hand allocates nothing, and the anti-replay
step is skipped for it (`N2`).** §4.0 step 10a runs before step 10b and indexes
§5.3's per-hand store, which §5.3 drops at hand end; the two readings available to
an implementer were an allocation keyed on a `u64` the sender picks — forbidden by
§27 and by §5.3's own first sentence — and an unstated fall-through. The rule is
the skip, and the bound is that **no `hand_id` a sender names causes an allocation
of any kind**. What is given up is stated rather than glossed: an equivocation
whose second copy arrives after its hand closed is not detectable by this
receiver — and was not before, because the copy it would be paired with went to
disk with the transcript. The one structure that outlives its hand is the
checkpoint-8 `STATE_ACK` band, at `8 × MAX_SEATS` entries for one hand at a time,
and it exists for note 27's reason.

**27. Two windows in this document closed on an event emitted concurrently with
the one they were bounding, and both are corrected (`N6`, `N5`).** A peer emits
its checkpoint-8 `STATE_ACK` and its `HAND_INIT(k+1)` copy on the same trigger,
and forwarding can reorder them between peers, so a window closing on
`HAND_INIT(k+1)` dropped the acknowledgement **in the healthy case** — leaving
§6.2's chained *"we all agreed here"* point unplaced at every boundary and
D-014's tier-2 precondition unsatisfiable. The `STATE_ACK` stage is therefore not
closed by that window; it is open until `TERMINAL(k+1)`, which is safe because an
accepted checkpoint-8 `STATE_ACK` is always from a seat already in `P(k)` and
grows no set. The same close was the whole of D-013's readmission promise, and at
a peer in the solitary regime **every** window is zero-width, so a returning seat
could never be heard by the peer draining it. §4.9's readmission set `A` is the
disposition: one seat set, written by §4.0 step 10b for a stale `PLAYER_SIT_IN` or
a stale checkpoint-8 `STATE_HASH` that **agrees**, read once at the next hand
init, cleared there, and widening that stage's **accepted** emitter set alone.
Both corrections only ever enlarge a set, both land at `HAND_INIT`'s collective
body where a disagreement stalls stage 0 loudly, and neither thaws a freeze.
**`P2` narrowed the second of them from the required set to the accepted set**,
because `A`'s write is reachable by replay — §4.0 step 10a is skipped for the
stale events that write it — while `A` itself is cleared every hand, so a
required set built from it could be re-enlarged once per hand for as long as §5.3
retains the record the replay is evaluated against.

**28. §4.0's anti-eviction box names D-014's exception instead of contradicting it
(`N3`).** The box bound every document and every layer against unseating a peer
*"on the strength of a protocol proof … or a failed verification"*, which is D-014
tier 1 verbatim, and the footnote above it said *"nothing further follows
automatically"* full stop. §11 carried the exception, §4.9's `kind = 3` box carried
it, and `src/security/validation.rs` had already implemented both tiers with the
tier-2 checkpoint precondition enforced by the type system — so the one place a
receiver implementer reads the rule was the one place it was wrong. The box keeps
its force, states the exception with its inputs, and states what is unchanged: the
line is **who signed the thing that is wrong**, and every liveness judgement,
attribution and fault record still removes nobody and moves no chip.

**29. The solitary-regime rule reads two sets, and this pass tried writing it on
one before noticing (`N7`, and it is the load-bearing check catching this pass's
own fix).** §4.0 step 10b tested membership against a record documented as
`P(hand_id)` while §3.2's rule and §4.0 step 12 both say `P(k-1)`; `P(k-1)` is
right, because it is what hand `k`'s stages were *required of*, and `P(k)` is grown
on purpose by the two exempt events, so testing against it silences the rule for
exactly the seats those exemptions admitted. §5.3's field now says `P(hand_id - 1)`.

The interesting half is the correction to the correction. The obvious follow-on —
narrow §3.2's regime test onto the same set, so one predicate serves both — was
written, and then checked against K1's own walk, and it **deletes the earliest
detection in the document**: the hand `P` narrows in is a hand whose stage 0 was
required of every seat, so `P(k-1) == {self}` is false for it, while its boundary
checkpoint is required of `P(k) = {self}` and is where the mismatch actually
arrives. The regime test is therefore written out as the disjunction
`P(k-1) == {self} ∨ P(k) == {self}`, with a sentence at each disjunct saying which
freeze route needs it, and §5.3 keeps `was_solitary` as an independent bool rather
than deriving it from `p`. **Two routes, two sets, and a single-set rule is wrong
for one of them whichever set it picks** — which is also why the engine's
`|signed_this_hand| == 1`, the first disjunct alone, cannot consume the second
route (`N-1e`, `N-4` in `DECISIONS.md`'s open list).

