# THREAT_MODEL.md

Binding document required by `SPEC_CS.md` §29. It defines what this system trusts,
what it does not trust, what the attacker can do, what security properties are
claimed, and — with equal weight — what is **not** claimed.

**Scope.** Play-money prototype, No-Limit Texas Hold'em, 2 players first and 3–6
players thereafter (`SPEC_CS.md` §32). Real money is explicitly out of scope, and
several classifications below would have to be re-examined before it were not.

**Status of the evidence.** Every factual claim about a library, a protocol or a
network measurement in this document is drawn from the Phase 0 research notes in
`docs/research/`, each of which carries its own per-claim verification line
(compiled code, or crate source read on disk). Where the research says something
is unverified, this document says so too. `SPEC_CS.md` §36 forbids papering over a
gap, §18 forbids claiming the system makes all cheating impossible, and the final
paragraph of the spec requires that we prove which attack classes are prevented,
which are only detected, and which are beyond the protocol's reach. This document
is the place where that distinction is made and it is deliberately conservative:
**an unjustified "prevented" is the worst error this document can contain.**

**Binding owner decisions.** `docs/DECISIONS.md` D-001 to D-008 outrank both the
research notes and any judgement in this document. Where a decision creates a
threat or an open question, it is recorded here as such rather than argued with.
**D-007 corrects D-006** and wins over it: at two seats an action deadline is
advisory, a fold-effect timeout certificate is forbidden, and no claim in this
document may present the certificate as protecting a two-seat table.
**D-008 generalises D-007** and wins over both: every rule that weakens, disables
or gates the timeout certificate is scoped on `|V|`, the size of the required
voter set — **never on `n`, the seat count**. A certificate whose required voter
set has fewer than two members has no effect: it is not an error and not
evidence, the deadline simply stays advisory. A seat leaves `V` only once a
**completed, valid** certificate names it; being voted against is not exclusion.
Any rule in this document still scoped on `n` would be a defect, and the attack
that scoping on `n` failed to stop is catalogued in its own right as **X30**.

---

## Table of contents

1. [System model and the parties](#1-system-model-and-the-parties)
2. [Trust assumptions](#2-trust-assumptions)
3. [Attacker capabilities](#3-attacker-capabilities)
4. [Security goals](#4-security-goals)
5. [Attack catalogue](#5-attack-catalogue)
6. [What cryptography does not solve](#6-what-cryptography-does-not-solve)
7. [The disconnect / abort problem](#7-the-disconnect--abort-problem)
8. [Privacy exposure from a fixed public infohash](#8-privacy-exposure-from-a-fixed-public-infohash)
9. [Known limitations and open questions](#9-known-limitations-and-open-questions)
* [Objections to the fix plan](#objections-to-the-fix-plan)

---

## 1. System model and the parties

**Source-tree ownership (`SPEC_CS.md` §23, interim).** This document governs **no
module**. It governs `tests/adversarial/` and `tests/fuzz/`, which are where its
classifications are made falsifiable (§5.5). The full §23 tree is deferred to
`docs/ARCHITECTURE.md` at the start of Phase 2; the other four specification
documents carry the same interim line for the module each of them governs.

### 1.1 Layering

The layering is fixed by `SPEC_CS.md` §1 and is itself a security boundary: the
network layer never decides poker rules and never creates cards.

```
GUI
 ├── Lobby / matchmaking          <- discovery only, never an authority
 ├── Poker state machine          <- deterministic, no clocks, no network
 ├── Mental poker / deck crypto   <- n-of-n threshold ElGamal + verifiable shuffle
 ├── Protocol / signed event log  <- canonical CBOR, Ed25519, hash chain
 └── P2P transport                <- libp2p; Mainline DHT for discovery only
```

Two independent signature domains exist and must never be confused
(`SPEC_CS.md` §20, and `research/LIBP2P.md` §6):

* the **libp2p identity keypair**, which produces the `PeerId` and authenticates a
  *connection*. It answers "which socket am I talking to", not "who is playing".
  GossipSub's `ValidationMode::Strict` authenticates only this.
* the **poker application signing keypair** (Ed25519, separate persistent key),
  which signs the canonical bytes of every protocol event. This is the only thing
  that authorises a poker action.

A valid libp2p connection carrying an event with a bad application signature is
rejected. A valid application signature arriving over an unauthenticated or
relayed path is accepted. Transport authenticity is defence in depth; it is never
the basis of a game-integrity claim.

### 1.2 The parties

**Player.** A human plus a client instance holding two long-lived secrets: the
libp2p identity key and the poker application signing key, stored in one profile
file next to the executable (`SPEC_CS.md` §21/§22, resolved in
`research/CRYPTO_LIBS.md` §6 as one file with a portable Argon2id passphrase slot
and an optional Windows DPAPI convenience slot). Per hand a player also holds
ephemeral secrets: one share of the joint deck key, a secret permutation and its
re-randomisation factors, and pre-reveal RNG commitments. A player is **not**
trusted by anyone, including their own table-mates.

**Table session.** The set of `n` seats that agreed on the signed table
parameters, plus a `session_id` / `table_id`. `session_id` is the object
`SPEC_CS.md` §14 and §20 call the *session nonce*; the name `session_nonce` is
retired and `PROTOCOL.md` §4.3 defines the single field. The table session is the only
place game state exists. Within a hand it is an **n-of-n** cryptographic group:
the aggregate deck key is `apk = Σ pk_i` over the seated, dealt-in players, and
opening any card requires a decryption share from *every* one of them
(Barnett–Smart, as implemented by `ziffle`; see `research/MENTAL_POKER.md` §1,
§7). There is no dealer, no host, no quorum and no threshold degradation. The
table session has no authority beyond what every participant can independently
verify.

**Lobby.** A GossipSub topic carrying signed `TABLE_ADVERTISEMENT` messages with
`timestamp` / `expires_at`, plus a request-response snapshot fetch for a
newly-joined client (`SPEC_CS.md` §3, §4). The lobby is **explicitly not an
authority over any game** (`SPEC_CS.md` §4). A malicious lobby can waste a
player's time, spam them, or hide tables from them. It cannot affect a hand that
has started. Any advert is only a claim; the table parameters that bind are the
ones all participants agreed on and signed before the first hand.

**Mainline DHT.** A public, adversarial, unauthenticated bulletin board. It
stores 6-byte compact `IP:port` records under one fixed, world-readable
`LOBBY_INFOHASH`. `SPEC_CS.md` §1 states its status precisely: the peer list is a
**hint about where to try connecting, never a claim about who is there**. Anyone
can write anything into it — measured directly: an infohash generated from 20
random bytes on the development machine and published nowhere else had a stranger
announcing under it within 24 minutes (`research/MAINLINE_DHT.md` §7). Identity is
settled only by the libp2p handshake and then by the application signature. The
DHT never carries game state, nicknames, `PeerId`s or table metadata.

**Relay (Circuit Relay v2).** Permitted by **D-001**. A relay carries an
already end-to-end encrypted, mutually authenticated libp2p stream whose payload
is additionally application-signed. It holds no key share, is not a party to any
table session, and is never asked to arbitrate. Two distinct roles must not be
blurred (**D-001 addendum**):

* *Relay as rendezvous.* Public relays are abundant — kubo enables the relay
  service by default on every publicly reachable node — but they cap a relayed
  connection at **2 minutes**, and at **128 KiB per circuit counted over both
  directions together** (verified in `libp2p-relay 0.21.1`; §3.5 gives the source
  lines and the kubo wording caveat). That is sized to coordinate one DCUtR hole
  punch, not to carry a session; the duration is what binds first (§3.5).
* *Relay as session transport.* Requires a relay with raised limits. Under
  **D-002** a publicly reachable poker client may volunteer for this, **off by
  default**, behind an explicit visible setting, with admission control.

A relay is a network adversary that the model already covers, plus a metadata
observer and a denial-of-service lever. Those two costs are real and appear in
§6 and §8.

### 1.3 Trust boundaries in one picture

```
     TRUSTED (locally)                   NOT TRUSTED (anything below the line)
 ------------------------------------------------------------------------------
 our own client binary                |  every other player, incl. all n-1 of them
 our own OS CSPRNG and key store      |  every table-mate's claims about state
 our own copy of the engine rules     |  the lobby, and every advert in it
 the assumptions A1-A7 of §2          |  the Mainline DHT and every record in it
                                      |  every relay
                                      |  every wall clock, ours included
                                      |  message ordering, delivery and timing
                                      |  the PeerId as a statement about a player
```

Everything below the line is input to be validated, never a source of authority.

---

## 2. Trust assumptions

Stated as "we assume X", each with the consequence if X is false. Assumptions
A1–A7 are cryptographic; A8–A15 are systemic. **A3 and A4 are the weakest links
in this document and are not currently verified.**

**A1 — We assume DDH is hard in the secp256k1 group used by the deck
cryptography.**
Card ciphertexts are ElGamal under `apk` in secp256k1 (inherited from `ziffle`,
which hardcodes `ark_secp256k1`; `research/MENTAL_POKER.md` §3.4). secp256k1 has
prime order and cofactor 1, so there are no small-subgroup or cofactor pitfalls.
*If false:* every hole card and every unopened board card is readable by any
observer of the transcript. Total loss of G1 and G2. Nothing else in the design
compensates.

**A2 — We assume the discrete logarithm problem is hard, so the Pedersen
commitments in the shuffle argument are binding, and we assume `ziffle`'s
generators have no known discrete-log relation to `G`.**
The second half was checked in source rather than assumed: `ziffle` derives `h`
and `gs[]` from hardcoded seeds via `CurveProj::rand`, and arkworks' `rand`
samples a random *base-field x-coordinate* and solves for `y` — it does not sample
a scalar and multiply the generator, which would have made the commitment
non-binding to anyone who knows the seed. This is a legitimate nothing-up-my-sleeve
derivation (`research/MENTAL_POKER.md` §4.1, verified by reading
`ark-ec-0.5.0/src/models/short_weierstrass/group.rs`).
*If false:* shuffle proofs stop binding the prover to a permutation, and card
substitution becomes possible with an accepted proof.

**A3 — We assume the Bayer–Groth 2012 shuffle argument, as implemented in
`ziffle` 0.1.0, is sound. THIS IS NOT VERIFIED.**
What *was* verified is much weaker: `ziffle`'s own 16 unit tests and 15 doctests
pass, and 13 hand-written adversarial probes were all rejected correctly,
including the `SPEC_CS.md` §8 attack verbatim (serialise a valid deck, overwrite
card 5's bytes with card 3's, present it with the honest proof — rejected).
Rejecting thirteen specific attacks is emphatically not soundness. A subtly wrong
exponent or a missing check can produce a system that accepts every honest proof,
rejects every naive attack, and still admits a clever forgery. `ziffle` is a
single-author crate with 3 GitHub stars, 8 commits, one release, 293 downloads and
no audit, whose own README says not to use it for non-trivial amounts of money
(`research/MENTAL_POKER.md` §4.1, §9 risk 1).
*If false:* G3 and G4 fall. A malicious shuffler can add, remove or substitute
cards with a proof that every honest client accepts. Every "cryptographically
prevented" row in §5 that concerns deck integrity degrades to "not prevented and
not detected". **This is the single largest risk in the system**, and it is why
`docs/CRYPTOGRAPHY.md` must record a line-by-line review of `ziffle`'s
`MultiExpArg` and `SingleValueProductArg` against the paper as a **prerequisite**,
and why the library must sit behind our own `DeckCrypto` trait so it can be
swapped for `barnett-smart-card-protocol` without touching anything else.

**A4 — We assume the Fiat–Shamir instantiation is sound in the random-oracle
model, including `ziffle`'s deliberately forked transcript. NOT VERIFIED.**
`ShuffleProof::new` clones the transcript and derives the multi-exponentiation and
the product arguments from the same state, separated only by different domain tags
(`ziffle/BG12MultiExpArgX/v1` vs `ziffle/BG12ProductArgX/v1`); the author flags
this in a source comment. Distinct domain separation is the standard defence and
this is probably fine, but transcript forking is exactly where weak-Fiat–Shamir
bugs of the "Frozen Heart" class live, and the transcript is a bespoke SHA-256
chain rather than a reviewed construction such as merlin
(`research/MENTAL_POKER.md` §9 risk 2).
*If false:* the same consequence as A3.

**A5 — We assume Ed25519 is EUF-CMA secure and that `verify_strict` is used
everywhere.**
`ed25519-dalek` 3.0.0. Plain `verify` accepts signatures under small-order or
non-canonical public keys and therefore admits malleability; a malleable signature
is an equivocation hole under `SPEC_CS.md` §14, because two distinct byte strings
would validate for one logical event. RUSTSEC-2022-0093 (double public key signing
oracle) is patched at `>= 2`; we are on 3.0.0, and the `hazmat` feature is never
enabled (`research/CRYPTO_LIBS.md` §2.1).
*If false:* G5, G6 and G7 fall; impersonation and history rewriting become
possible.

**A6 — We assume BLAKE3 is collision- and second-preimage-resistant, and that
every protocol hash is domain-separated and length-prefixed.**
BLAKE3 is the protocol hash for the transcript chain, `state_hash`, RNG
commitments and deck commitments. It is not length-extendable and offers keyed and
`derive_key` modes natively, so we do not hand-roll domain separation. Honest
caveat recorded in `research/CRYPTO_LIBS.md` §3.2: **no public third-party security
audit of BLAKE3 was found**; its assurance rests on the specification and its
BLAKE2/ChaCha lineage. Length prefixing is mandatory and not cosmetic: without it,
`"AB" ‖ "C"` and `"A" ‖ "BC"` hash identically and two different logical events
collide to one transcript hash — a direct §13/§14 break.
*If false:* the hash chain stops binding history; G6 falls.

**A7 — We assume the OS CSPRNG is sound and is the only entropy source.**
`getrandom::SysRng`. Note the spec deviation to record: `SPEC_CS.md` §7 names
`OsRng`, which **no longer exists** in the current release line — neither `rand`
0.10.2 nor `rand_core` 0.10.1 contains the symbol, and `rand_core` has no `os_rng`
feature. The requirement is implemented under its current name.

**Correction, and it weakens the assumption.** An earlier revision of this
document claimed the `rand` crate is deliberately absent from the dependency tree
and that §7's prohibition on `SmallRng`/`StdRng` is therefore enforced
structurally rather than by reviewer discipline. That is false. `rand 0.8.8` **is**
in the runtime tree via `ark-std 0.5.0` (which depends on `rand 0.8` with feature
`std_rng`), together with `rand_chacha 0.3.1` and `rand_core 0.6.4`. `SmallRng` is
absent only because rand 0.8's `small_rng` feature is not enabled; `StdRng` is
compiled in and is used by `ziffle` for deterministic nothing-up-my-sleeve public
constants. `SPEC_CS.md` §7 is therefore enforced by **the CI lint of
`CRYPTOGRAPHY.md` §12 item 6**, not by the dependency graph
(see `CRYPTOGRAPHY.md` §7.2 and its OQ-8).
*If false*, or if the lint is removed: §7's prohibition is unenforced and a future
contributor can reach `StdRng` from our own crates. And if the OS CSPRNG itself is
unsound, keys, permutations and RNG contributions become predictable; every
secrecy goal falls for the affected client.

**A8 — We assume each player's own endpoint (OS, RAM, display) is not
compromised.**
*If false:* that player's own hole cards leak, and their signing key may be stolen,
which is indistinguishable from that player acting maliciously. The rest of the
table is cryptographically unaffected — a compromised endpoint reveals only what
that endpoint is entitled to see. This is the boundary `SPEC_CS.md` §18 draws
between protocol cheating and endpoint cheating.

**A9 — We assume every player who is dealt in participates in the shuffle chain
with a secret permutation drawn from A7.**
The final deck order is the composition of all `n` permutations, so it is uniformly
random as long as **at least one** shuffler is honest. Since every dealt-in player
shuffles, the victim of any coalition is always among the shufflers. The last
shuffler gains nothing: it sees only ciphertexts under `apk`, holds one of `n`
shares, and therefore has no information about which slot holds which card, so
there is nothing to grind toward (`research/MENTAL_POKER.md` §6).
*If false* (a player is dealt in without shuffling): the remaining shufflers, if
they collude, know the full permutation composition and the deck order becomes
predictable to them.

**A10 — We assume the deck-index-to-recipient map is fixed before the shuffle
chain starts and is a pure function of `(table_id, hand_id, button)`.**
This is our design obligation, not the library's. If the map were chosen after the
final deck existed, a malicious last shuffler could argue about which index is
"the button's first hole card" and thereby choose outcomes
(`research/MENTAL_POKER.md` §6).
*If false:* the last shuffler acquires a real, exploitable edge, and A9's
"nothing to grind toward" argument collapses.

**A11 — We assume the proof context `ctx` is exactly `PROTOCOL.md` §4.5's
construction over
`(protocol_version, table_id, session_id, hand_id, sequence, shuffle_round,
sender_public_key)`.**
This is a fixed field list, not a minimum: `PROTOCOL.md` §4.5 owns the
construction, `CRYPTOGRAPHY.md` §6.4 reproduces it, and the raw-concatenation form
that document previously carried is deleted. `shuffle_round` is `0xFF` for every
`ctx` that is not a shuffle-chain step.
`ziffle` binds every proof to a caller-supplied `ctx` and probes confirmed proofs
do not transfer across different `ctx` values — but that is only as strong as what
*we* put in it (`research/MENTAL_POKER.md` §9 risk 3). This is our bug to make.
*If false:* a valid shuffle proof or reveal-token proof from one hand replays into
another hand or another table.

**A12 — We assume every honest client independently re-derives all public state
from the signed event log and never accepts a stack, a pot or a board from the
wire as authoritative.**
`SPEC_CS.md` §11: "the poker engine must not trust that the counterparty sends
legal actions; re-validate every incoming action locally."
*If false:* the "fake stack" and "fake pot" rows in §5 stop being harmless.

**A13 — We assume no trusted clock and no trusted ordering.**
Deadlines never enter the engine as a wall-clock read. Time enters only as a
**timeout certificate** signed by *every* other still-active player, naming the
seat, the sequence and the parent event hash (**D-006**). The engine itself
contains no clocks. The certificate stage is itself **collective** (`PROTOCOL.md`
§4.8, `STATE_MACHINE.md` §8.4): every required voter emits its own certificate and
`stage_hash` is taken over the whole set, so the `CERT_SETTLE_MS` settle timer is
deleted and there is no local timer anywhere in the chain-building rule.

**The assumption does not hold when `|V| < 2`.** `V` is the required voter set —
every other dealt-in seat, minus any seat a **completed, valid** certificate has
already attributed (`PROTOCOL.md` §8.3, D-008). When `|V| = 1`, "unanimous"
reduces to one signer and the certificate carries no more weight than that one
peer's word. **D-007** makes the heads-up action deadline advisory, produces no
signed state transition from it, and forbids a fold-effect certificate at two
seats; **D-008** generalises the protection to the quantity that actually governs
it, so a certificate with `|V| < 2` has no effect **at any seat count**, and `V`
cannot be shrunk by assertion. At `n = 2`, `|V| = 1` always, which is why the
heads-up case is the permanent instance of this. See X10, X30, §9.1 and OQ-A.
*If false:* i.e. if any single peer's clock assertion were honoured, that peer
could steal the action from a player who was about to act — which is precisely
the `|V| = 1` case D-007 and D-008 refuse to build on.

**A14 — We assume all participants at a table run the same
`protocol_version` and the same deterministic engine, and that a mismatch is
detected rather than silently tolerated.**
`protocol_version` is inside the signed body of every event, and the table
parameters are agreed and signed before the first hand (`SPEC_CS.md` §4, §12).
*If false:* two honest clients compute different legal-action sets or different
pots and accuse each other via `STATE_HASH` — a false dispute, not a break, but it
halts play.

**A15 — We assume our own implementation of the deterministic engine is correct.**
The property tests of `SPEC_CS.md` §26 (chip conservation, no duplicate card, board
≤ 5 cards, no bet above stack, pot equals contributions, no folded winner, side-pot
eligibility, no street skipping, no state change from an invalid signature) exist
to make this assumption testable rather than assumed.
*If false:* honest clients diverge and the §15 dispute mechanism fires with no
attacker present. This produces the one entry in §5 that is **detected but not
attributable**.

**A16 — We assume at least one usable path exists between any two players who
want to play (direct, hole-punched, or relayed).**
This is a liveness assumption, not a security one. It is the assumption most
likely to fail in practice: hole punching succeeds about **70% ± 7.1%** in the
wild across 4.4M measured attempts, and near 0% when either side is behind a
symmetric NAT (`research/NAT_AND_DISCOVERY.md` §4).
*If false:* those two players cannot play. Under **D-004** they still see the
lobby, because reading the lobby only requires an outbound connection.

### 2.1 What we explicitly do not trust

* Any other player, individually or as a coalition of up to `n-1`.
* The Mainline DHT, and every record retrieved from it.
* The GossipSub lobby, and every advertisement in it.
* Any relay, including one we ourselves are relaying through.
* Any `PeerId`, as a statement about who is playing.
* Any clock, including our own, as an input to the game state.
* Message ordering, delivery, timing, or the absence of a message.
* Any card, pot, stack or board value that arrives as data rather than being
  derived from the verified transcript.

---

## 3. Attacker capabilities

### 3.1 The baseline adversary (`SPEC_CS.md` §17)

The adversary runs a **fully modified client**. This is the starting point and it
is not negotiable. Concretely, assume the adversary can:

* read, modify and delete any local code, memory or storage on their own machine,
  including disabling every check the official client performs;
* emit **arbitrary bytes** on any protocol at any time — malformed, oversized,
  out-of-order, replayed, duplicated, or drawn from another hand or another table;
* refuse to send anything at all, at any moment of their choosing, including the
  most damaging one;
* observe every byte they legitimately receive, and retain it forever;
* run any amount of offline computation against what they have observed, subject
  to the hardness assumptions A1–A7;
* choose their own permutations, blinding factors and RNG contributions
  adversarially rather than randomly.

**No GUI-level or client-side assumption is ever a defence.** "The interface will
not let them click that" is not an argument this document accepts anywhere
(`SPEC_CS.md` §17, first line).

### 3.2 A malicious majority at the table

Assume `n-1` of `n` seats are one adversary, colluding perfectly, sharing all
state out of band, against one honest player. This is the *hardest* in-scope case
and it is where the n-of-n construction earns its cost.

What the coalition **can** do:
* know every card they themselves hold, and all of their own permutations, so the
  only unknown in the deck order is the honest player's permutation;
* coordinate their betting perfectly against the honest player (this is collusion,
  §6 — out of scope for the protocol);
* refuse to publish decryption shares and thereby force a hand to abort;
* produce a **timeout certificate** for the honest player, because the required
  voter set is every *other* dealt-in seat and the coalition is that whole set
  (see X10 in §5 — this is a genuine attack and is classified honestly). Note that
  at `n = 3` the "coalition" is two seats, and at `n = 2` it is one, which is why
  D-007 removes the mechanism entirely at two seats and D-008 removes it wherever
  `|V| < 2`, rather than pretending one signature is a consensus. **A coalition is
  not needed to reach `|V| = 1` if the voter set itself can be shrunk** — that was
  X30, and it is why the floor is written on `|V|` and why a seat leaves `V` only
  through a completed certificate;
* deny the honest player any table by simply not seating them.

What the coalition **cannot** do, under A1–A7:
* open the honest player's hole cards. Opening any card requires all `n` decryption
  shares including the victim's; `n-1` shares yield nothing. Verified empirically
  in the two-player case (`research/MENTAL_POKER.md` T7d/T7e: a single valid share
  alone returns `None`), and the argument is uniform in `n`;
* learn the board ahead of its street, for the same reason;
* add, remove or substitute a card without an invalid shuffle proof (contingent on
  A3);
* forge, alter or replay the honest player's signed events;
* rewrite the hand's history and have the honest player accept it;
* make the honest player's client accept an illegal action or an incorrect pot
  award, since the honest client re-derives all of it (A12).

This is the property that makes the design worth its complexity: **hole-card
secrecy and deck integrity do not depend on being in the majority.**

### 3.3 An attacker on the network but not at the table

An adversary who never joins the table. They can:

* enumerate the player roster continuously by polling
  `get_peers(LOBBY_INFOHASH)`, and can run Sybil DHT nodes near the infohash in
  the keyspace to see the write stream directly (§8);
* dial any player and complete a libp2p handshake, learning their `PeerId` and
  binding it to an IP address;
* flood the lobby with signed junk adverts, subscribe us to junk topics, or try to
  eclipse our GossipSub mesh with inbound-only Sybils;
* pollute `LOBBY_INFOHASH` with junk `IP:port` records so that discovery burns
  dial attempts on hosts that never ran this software — observed happening in the
  wild on a private random infohash (§8);
* attempt resource exhaustion: connection floods, oversized frames, deep-nested
  CBOR, hostile length prefixes;
* deny service to a specific player at the network level, at exactly the moment
  that hurts most.

They **cannot** read or alter game content: the table's traffic is end-to-end
encrypted between endpoints and separately application-signed, and they hold no
key share.

### 3.4 A passive DHT observer

The Mainline DHT is heavily crawled by anti-piracy monitors, academic scanners and
Sybil nodes. A passive observer needs no capability beyond running a DHT node and
polling. Measured on the development machine (`research/MAINLINE_DHT.md` §3.4,
§3.6, §7):

* one `announce_peer` lands on **19–32 independent storing nodes**, and a
  different process retrieved the record within **~113 ms**;
* one `get_peers` contacts **105–176 distinct DHT nodes**, each of which learns
  the tuple (our IP, the infohash we asked for), plus another ~130–140 that learn
  our IP alone;
* at the recommended 10-minute re-announce cadence that is on the order of **150
  disclosure events per day** to a rotating set of strangers;
* a record left unrefreshed was retrievable for ~45 minutes and gone by ~50, so
  presence is a ~10-minute-resolution online/offline signal per IP.

This is the cheapest attack in the whole document and it requires no interaction
with us at all. Consequences are enumerated in §8.

### 3.5 An attacker who controls the relay

Permitted and analysed by **D-001**. A malicious relay:

* **cannot read poker events.** The transport is Noise or TLS 1.3 terminated at
  the two peers, not at the relay, and the payload is separately application-signed.
* **cannot forge or alter an event.** Every receiver re-validates the application
  signature and the hash chain independently (`SPEC_CS.md` §12, §13, §14).
* **cannot become an authority**, hold a key share, decrypt a card, or arbitrate a
  dispute. It never participates in the protocol.
* **can see metadata**: which peers talk to each other, when, how much, for how
  long. This is a real privacy loss, not a hypothetical one (§6, traffic analysis).
* **can drop, delay or reorder a specific peer's traffic** — a targeted
  denial-of-service lever, and under D-005 an opponent's forfeited commitment is
  the payoff for using it (see X9 in §5).
* **can silently reset a session** by enforcing the default circuit limits. The
  binding one is **`max_circuit_duration = 120 s`**, not the byte cap: a session
  lasts far longer than two minutes, while the 131 072 B `max_circuit_bytes`
  default is **per circuit and bidirectional — a single budget for both
  directions combined, not one budget each way**. Verified in source rather than
  inferred: `libp2p-relay 0.21.1` gives the defaults in `impl Default for Config`
  (`src/behaviour.rs:162-163`, `2 * 60` s and `1 << 17` B) and relays each circuit
  with one `CopyFuture` carrying a single `bytes_sent: u64`
  (`src/copy_future.rs:41-48`); **both** `forward_data` calls — src→dst at
  `:88-95` and dst→src at `:97-104` — increment that same counter, which is then
  compared against the cap at `:78`. A table is a full mesh, so one circuit
  carries exactly one pair, and each of the two peers puts its own 8 979 B shuffle
  step and proof through it once per hand, independent of `n`: **2 × 8 979 =
  17 958 B per hand per circuit**, which is roughly **7 hands** of shuffle
  traffic, or about **5 hands** including the signed event stream, before the byte
  budget is reached. (These figures were previously stated per direction and were
  therefore twice as generous; the correction halves the headroom and changes no
  decision, because the duration binds first either way.) Note a discrepancy worth
  carrying: kubo's `docs/config.md` describes its `ConnectionDataLimit` as being
  "in each direction". Either the Go and Rust implementations differ or that
  wording is loose; our own relay is Rust, so the Rust behaviour binds us, and for
  a third-party relay the stricter (bidirectional) reading is the safe assumption.
  `research/NAT_AND_DISCOVERY.md` §3.5 names the duration limit precisely: a
  circuit that dies at 120 s is *an engineered abort attack against ourselves*.
  The client must read the `Limit` the relay returns
  with the reservation and refuse to seat a player whose only path cannot carry a
  hand, rather than starting a hand that will die mid-street.

A relay is therefore modelled as **a network adversary with a privileged
observation point and a reliable DoS capability, and nothing more.**

### 3.6 Explicitly outside the adversary model

* Breaking A1–A7 (DDH, DL, EUF-CMA, collision resistance, the OS CSPRNG).
* Physical access to an honest player's machine, or malware on it (A8).
* Compelling a player by force or law.
* Global passive network observation combined with unlimited traffic analysis.
* An attacker with the resources to sustain a network-level DoS against a target.

---

## 4. Security goals

Each goal is stated as a property that could in principle be **falsified by a
concrete experiment**, and each names the test that falsifies it. Together G1–G4
and G8 are the decomposition of the main invariant of `SPEC_CS.md` §35:

> No individual participant may learn another's unrevealed hole cards or the
> future board, and no individual participant may change the composition or
> order of the deck without a cryptographically detectable protocol violation.
> Simultaneously, every client must be able to verify independently all public
> poker state transitions and the cryptographic proofs needed to confirm the
> hand's correctness.

**G1 — Hole-card secrecy against any proper subset.**
For every hand, every honest player `A`, and every coalition `C ⊆ P \ {A}` (up to
`|C| = n-1`), `C` cannot compute `A`'s hole cards from all messages it legitimately
receives, until the rules require the reveal.
*Falsified by:* the `CheaterReadOpponentCard` row of §5.5, and §5.5's first
mandatory standalone test — an adversarial test in which a modified client holding
every message it received, and every secret of every other seat, outputs the honest
player's hole cards.
*Basis:* n-of-n threshold ElGamal (A1), empirically probed at n=2. The basis is
the **counting argument**, not the delivery path: `DEAL_PRIVATE` is a **broadcast**
stage (`PROTOCOL.md` §3.4/§4.6, `STATE_MACHINE.md` §7.8), and secrecy does not
depend on any token being withheld from delivery. Opening deck index `i` requires
a reveal token from every one of the `n` dealt-in seats; for a hole index owned by
seat `P`, every seat except `P` broadcasts its token, so at most `n-1` tokens for
that index ever exist publicly, and the missing one is `P`'s.

**G2 — Board unpredictability.**
Before street `s` opens, no coalition `C ⊊ P` can compute any card of street `s`
or any later street with probability better than guessing from the unopened
remainder.
*Falsified by:* the `CheaterFutureBoard` row of §5.5 and §5.5's second mandatory
standalone test — a coalition that predicts the flop before `FLOP_REVEAL`, or a
single malicious player who selects the future board by manipulating the last
RNG/shuffle step.
*Basis:* A1 plus street gating in our state machine plus A10.

**G3 — Deck integrity.**
For every completed hand, the multiset of all cards opened (hole cards revealed at
showdown plus the board) is a sub-multiset of the standard 52-card deck, no card
value appears twice, and every opened card corresponds to a distinct index of the
deck committed before the shuffle chain.
*Falsified by:* the `CheaterDuplicateAce` and `CheaterReplaceCard` rows of §5.5 —
a hand in which a card appears twice, or in which a card outside the original 52
appears, with all proofs accepted.
*Basis:* A1–A4. **Contingent on A3, which is unverified.**

**G4 — Shuffle soundness.**
If a shuffle proof verifies, then the output deck is a permutation and
re-randomisation of the input deck under the same aggregate key.
*Falsified by:* the `CheaterInvalidShuffle` row of §5.5 — any accepted proof for a
transition that is not such a permutation. Also falsified by the review named in A3
finding a soundness gap.
*Basis:* A2, A3, A4. **This is the property with the weakest evidence in the
document.**

**G5 — Action authenticity and non-repudiation.**
Every state-changing event is attributable to exactly one application key, and no
party can produce an accepted event attributed to a key it does not hold.
*Falsified by:* the `CheaterFakeStack` and `CheaterIllegalRaise` rows of §5.5, and
the impersonation tests they name — an accepted event whose `sender_public_key` is
another player's.
*Basis:* A5, plus canonical encoding (§5, X4).

**G6 — History immutability.**
Once an event is in the transcript, no party can produce a differing history that
an honest client accepts, and any accepted history is a single chain.
*Falsified by:* the `CheaterReplayAction` row of §5.5 and the rewritten-history
test it names — an honest client accepting two different orderings, or an event
re-parented to a different position.
*Basis:* A5, A6, A11.

**G7 — Equivocation produces evidence.**
If a player signs two conflicting **chained** events for the same
`(table_id, hand_id, sequence, event_class)`, the pair constitutes a
self-authenticating, transferable proof of that player's misbehaviour.

The predicate is scoped to chained events and is stated normatively in
`PROTOCOL.md` §5.2; it is quoted here because this goal is what it exists to
deliver:

> Peer `K` equivocates when two `SignedEvent`s `E1 != E2` exist such that both
> pass the canonicality gate, both verify under `K`'s public key with
> `verify_strict`, both carry `chain_scope == 1`, their bodies agree on all six of
>
> ```
> (protocol_version, table_id, hand_id, sequence, event_class, sender_public_key == K)
> ```
>
> and `event_hash(E1) != event_hash(E2)`.
>
> Events with `chain_scope == 0` are outside this predicate entirely and can
> never produce an `EquivocationProof`. Their anti-replay is per-type:
> `timestamp_unix_ms` strict monotonicity per `table_id` for lobby adverts,
> `list_serial` for `PLAYER_LIST`, `join_nonce` for the join RPC, and
> `connection_nonce` plus a per-connection counter for the handshake.

*Falsified by:* the `CheaterEquivocation` row of §5.5 — a divergence that no honest
peer can attribute, or two conflicting events that do not together prove
misbehaviour.
*Basis:* A5, plus the canonicality gate (X4) which removes the "two byte encodings
of one logical event" escape.
*Limit, stated here rather than in the justification column:* the proof exists as
soon as both events reach one honest party. It does **not** guarantee they do.
*Second limit — the false-positive class.* The predicate must never be applied to
unchained traffic. Before the `chain_scope` discriminator existed, two successive
honest lobby adverts, and a joiner's `JOIN_REQUEST` alongside its own
`RNG_REVEAL`, both satisfied the old predicate — an honest peer would have been
provably framed by a rule in the specification. A specification-level
false-positive source is worse than a user-level one, and it is recorded here so
that any future message type is checked against it. `NETWORK_STACK.md` §7.4's
founder-contradiction rule for two adverts with the same `timestamp_unix_ms` is a
local lobby-hygiene rule and is **not** an `EquivocationProof` in this sense.

**G8 — Independent verifiability.**
Every honest client can decide accept/reject on every public state transition and
every proof using only the transcript and public parameters, with no appeal to any
other party's word, and with no "the host is right" tie-break (`SPEC_CS.md` §15).
*Falsified by:* any protocol step whose correctness a client must take on trust.

**G9 — Chip conservation, as a ledger identity.**
Chips are conserved against a ledger of entries and exits, not against a constant.
The canonical statement is `STATE_MACHINE.md` §10 invariant I1:

```
I1  (ledger identity)
    Σ_s stack[s] + Σ_s committed_hand[s] == ledger_in − ledger_out

    ledger_in   = Σ over every accepted seat-entry of its buy-in
    ledger_out  = Σ over every accepted seat-exit of the stack it removed

    Corollary, tournament mode: no entry or exit occurs after the first hand,
    so the right-hand side is constant and equals players_at_start × start_stack
    — which is the old I1, now derived rather than assumed.

    Cash mode: the right-hand side changes only at a hand boundary (T47).
```

`sum(pots) + sum(returned_uncalled) = sum(committed_this_hand)` continues to hold
within a hand (`research/POKER_RULES.md` §A0 invariants 1 and 2). This must hold on
**every** path, including a D-005 abort with forfeiture, and on the restoration
paths of `cause = 4` and the no-attribution aborts of §9.1's `|V| < 2` and
multi-subject cases.
*Falsified by:* a property test that finds any transition changing the identity.

**G10 — No silent divergence.**
If two honest clients hold different public state, they detect it at the next
`STATE_HASH` checkpoint and neither continues playing
(`SPEC_CS.md` §15).
*Falsified by:* a scenario in which two clients play on with different pots or
different boards and never notice.

**Non-goals, stated so they are not mistaken for goals.** Fairness of outcomes
against out-of-band collusion; unlinkability of a player across sessions;
availability under a network-level attack; protection of a player from their own
compromised machine; recovery of a hand whose participant has vanished. Each is
addressed in §6 or §7.

---

## 5. Attack catalogue

### 5.1 Classification scheme

| Bucket | Meaning |
|---|---|
| **CP** — cryptographically prevented | Under assumptions A1–A7 (and the specific ones named in the row), the adversary cannot construct a message that both achieves the goal and is accepted by an honest client. The adversary's only option is to be rejected. This includes attacks that are *structurally excluded*, i.e. for which no message in the protocol's grammar could express the attack. |
| **D&A** — detected and attributed | The adversary *can* emit the message. Every honest client rejects it, the state does not advance, and the misbehaviour is bound to a specific application key by a signature, so the evidence is transferable to third parties. |
| **DNA** — detected but not attributable | The divergence or conflict is detected, and play stops, but the transcript does not establish *who* was at fault. |
| **OOS** — out of scope | The protocol does not and cannot address it. Mitigations may exist and are named, but no security claim is made. |

One row (X7) carries **two classes**, because its class depends on how many seats
are silent at once and on `n`. A split cell is honest where a single label would
overstate one of the cases; it is written as `D&A / DNA` and the cases are
enumerated in the justification. Splitting is not a way to avoid a verdict, and no
row may split between CP and anything else — CP is a claim about what an adversary
can construct, and it either holds or it does not.

**Every CP row inherits A1–A7. In particular, every CP row that concerns deck
integrity (rows 1–4) also inherits A3 and A4, which are NOT verified.** If the
review named in A3 finds a soundness gap, rows 1–4 fall out of CP entirely.

### 5.2 The `SPEC_CS.md` §17 catalogue

| # | Attack (§17) | Class | Justification |
|---|---|---|---|
| 1 | Fake card (a card not in the original 52) | **CP** | The deck is committed once as the masked encoding of the 52 known plaintext group elements. Every subsequent deck is proven to be a permutation-and-re-randomisation of its predecessor, and the Fiat–Shamir challenge absorbs `apk`, the whole previous deck, the whole next deck and the permutation commitment, so a proof is bound to that specific transition. Introducing a 53rd plaintext requires forging the shuffle argument. *Inherits A3, A4.* |
| 2 | Duplicate card | **CP** | Same argument. Probed directly: a valid deck was serialised, card 5's bytes overwritten with card 3's, re-deserialised, and presented with the honest proof — rejected (`research/MENTAL_POKER.md` T4). A two-shuffle chain was opened in full and all 52 cards were present exactly once (T8). *Inherits A3, A4.* |
| 3 | Removing a card from the deck | **CP** | Deck length is a compile-time constant in the proof type (`MaskedDeck<52>`, `ShuffleProof<52>`), and the proof binds the entire previous deck, so a shortened deck is neither type-representable nor provable. *Inherits A3, A4.* |
| 4 | Shuffle manipulation | **CP**, with a stated non-claim | Two distinct things are conflated by the word. (a) *An invalid shuffle* — one that is not a permutation and re-randomisation — is prevented by the argument's soundness. (b) *Choosing which permutation to apply* is entirely legal and cannot be prevented; the claim is that it is **worthless**: the shuffler sees only ElGamal ciphertexts under `apk`, holds one of `n` decryption shares, and therefore has zero information about which slot holds which card. Grinding permutations gains nothing to grind toward. This second half depends on **A10** — if the index-to-recipient map could be chosen after the final deck existed, (b) would become a real attack. *Inherits A3, A4, A10.* |
| 5 | RNG manipulation | **CP** for the deck | The deck order is the composition of `n` secret permutations with fresh re-randomisation from the OS CSPRNG; it is uniform if **at least one** shuffler is honest (A9), and the victim of any coalition is always among the shufflers. No player and no coalition of `n-1` controls it. The last shuffler has no advantage, per row 4(b). The separate non-deck beacon (seating, initial button) is a commit/reveal whose failure mode is different and is classified as X6 below. |
| 6 | Reading another player's hole cards | **CP** | Opening any card requires a decryption share from **every** dealt-in player. The basis is the counting argument of G1 and **not** the delivery path: `DEAL_PRIVATE` is broadcast (`PROTOCOL.md` §3.4/§4.6), so for a hole index owned by seat `P` at most `n-1` tokens exist publicly and the missing one is `P`'s; a coalition of `n-1` holds only tokens already among those `n-1`, and producing `P`'s is equivalent to computing `sk_P` from `pk_P`. Verified at n=2: one valid share alone returns `None`; both together return the card (`research/MENTAL_POKER.md` T7c/T7d/T7e). A reveal token verified against the wrong public key is rejected, and a token replayed onto a different card is rejected (T7a/T7b). A token from `P` for `P`'s own hole index before showdown is a protocol violation, rejected and attributed. This is the property that survives a malicious majority. *Inherits A1.* |
| 7 | Reading the board early | **CP** (opening); attempting it is D&A | Board indices are ElGamal ciphertexts under `apk` until every player publishes a token for that index, so a coalition cannot open them (row 6's argument). Street gating is our state machine's job, not the library's: `ziffle` has no notion of "too early". A player who publishes a token for a future street's index commits a violation that every honest client rejects and attributes by signature. Note the attempt is also futile: one early token opens nothing. *Inherits A1, A10.* |
| 8 | Illegal poker action | **D&A** | A modified client can emit `ACTION_RAISE` for an amount exceeding its stack, out of turn, or on a folded hand — the message is constructible. Every honest client re-validates it against the deterministic engine (A12) and rejects it; the state does not advance; the signature names the sender. `SPEC_CS.md` §11 states this obligation directly. |
| 9 | Fake stack size | **D&A** | Stacks are **derived**, never transmitted as authority: every peer computes them from the signed transcript. A claim to the contrary either does not appear in the message grammar or is ignored, and the resulting divergence surfaces at the next `STATE_HASH` checkpoint with the liar's signature on the event that caused it. |
| 10 | Fake pot | **D&A** | Identical to row 9. The pot and every side pot are a pure function of `committed_this_hand[]` and the fold/all-in states (`research/POKER_RULES.md` §A7). |
| 11 | Action out of order | **D&A** | `player_to_act` is a pure function of the state; the event carries a monotonic `sequence` and a `previous_event_hash`. An event whose parent is not the client's current head, or whose sender is not the player to act, is rejected and attributed. |
| 12 | Changing an already-signed action | **CP** | Ed25519 EUF-CMA over the exact received bytes. Two further rules make this hold in practice: the signature is verified over the **bytes as received**, never over a re-encoding (canonicalise-then-verify would let a signature migrate onto bytes it never signed), and `verify_strict` is mandatory. *Inherits A5.* |
| 13 | Replay of old actions | **CP** | The signed body binds `protocol_version`, `table_id`, `hand_id`, `sequence` and `previous_event_hash`, so an event is valid at exactly one position of one chain. Shuffle and reveal proofs additionally bind `ctx` (A11); probes confirmed proofs do not transfer across different `ctx` values. *This row is contingent on our own `ctx` construction being right, which is why an adversarial test that replays a valid shuffle proof from hand `h` into hand `h+1` and asserts rejection is mandatory, not optional.* *Inherits A5, A11.* |
| 14 | Rewriting a hand's history | **CP** | The transcript is a hash chain from `GENESIS`; changing any past event changes every subsequent `previous_event_hash`, which requires a BLAKE3 collision, and every event is independently signed. *Inherits A5, A6.* |
| 15 | Impersonating another participant | **CP** | Authorisation comes from the application Ed25519 signature alone. The `PeerId` is never authentication (`SPEC_CS.md` §20), the DHT record is never an identity claim, and a GossipSub `Signed`/`Strict` message proves only which socket spoke. Announcing someone else's `IP:port` under `LOBBY_INFOHASH` is possible and meaningless — it produces a dead dial, not an identity. *Inherits A5.* |
| 16 | Different histories to different players (equivocation) | **D&A** | Not preventable: a modified client can sign two conflicting events. What the design guarantees is that the pair is **self-authenticating evidence** — two valid signatures by one key over the same `(protocol_version, table_id, hand_id, sequence, event_class)`, both with `chain_scope == 1`, with different `event_hash` values (G7, `PROTOCOL.md` §5.2). Detection is fast in practice because all `n` peers at a table are mutually connected and exchange `STATE_HASH` after critical transitions, and because an equivocator cannot carry two divergent hands to showdown: opening any card needs every player's share, so both branches stall. The honest limit: the evidence only exists once both halves reach one honest party, and a partition can delay that. |
| 17 | Malformed packets | **D&A**, with a residual risk | The event decoder is bounded by construction: `minicbor` validates a claimed length against the remaining input *before* allocating — measured at **0 bytes allocated** for a byte string claiming 4 GiB, for one claiming `u64::MAX`, and for an array claiming 4 GiB of elements, and 20 000 levels of nesting produced an error rather than a stack overflow (`research/CRYPTO_LIBS.md` §4.8). No `eval`, no `pickle`, explicit schema validation. **Residual risk, stated rather than hidden:** the *cryptographic* deserialisers (arkworks / `ziffle`) have **not** been fuzzed, and `ziffle`'s `Transcript::update_with_serialized` contains an `assert!` panic path if a serialised element exceeds a 256-byte buffer. Unreachable for 33-byte points, but it is a panic on network-derived data. Until `SPEC_CS.md` §27 fuzzing lands over `ShuffleProof`, `MaskedDeck`, `RevealToken` and `OwnershipProof`, a malformed crypto object is a plausible remote panic, i.e. a DoS. |
| 18 | Oversized packets | **D&A** | Hard caps at every boundary: GossipSub `max_transmit_size` is a two-sided protocol constant frozen next to `LOBBY_INFOHASH` (a peer with a different value simply rejects our frames, so it cannot be tuned per build); `request-response` codecs have explicit request/response size maxima; our own length-prefixed framing carries a maximum. Over-cap frames are dropped and the forwarding peer is scored down via `MessageAcceptance::Reject`. |
| 19 | Resource exhaustion "in reasonable measure" | **OOS** | Mitigated, not solved, and `SPEC_CS.md` §18 lists DoS as beyond the protocol's reach. Mitigations in place: `connection_limits` (max pending/established, per-peer cap), `memory_connection_limits` at a percentage of RAM, GossipSub peer scoring and `validate_messages()`, `flood_publish(false)` to remove an amplification lever, a bounded dial budget for unverified DHT hints, subscription filters, and the relay's own reservation/circuit rate limiters. An adversary with meaningful bandwidth defeats all of it. |

**§17 catalogue counts: CP 11 · D&A 7 · DNA 0 · OOS 1 · total 19.**

### 5.3 Protocol-level attacks not named in §17

These fall out of the research notes and of D-001 to D-008. They are catalogued
with the same scheme because omitting them would make the §17 table look more
complete than the system is.

| # | Attack | Class | Justification |
|---|---|---|---|
| X1 | Rogue aggregate key: the last player to publish `pk_i` chooses it as `X − Σ others` so that `apk` has a discrete log they know | **CP** | Every public key is accompanied by an `OwnershipProof` — a textbook Schnorr proof of knowledge of `sk` — and the aggregate constructor accepts only `Verified<PublicKey>` values. `Verified<T>` has a private field and deliberately **no** `CanonicalDeserialize` impl, so it cannot be forged from outside the crate or smuggled in off the wire; a downstream attempt to construct one fails to compile with `E0423` (`research/MENTAL_POKER.md` §4.1, negative compile test). Our obligation: verify every ownership proof before aggregating, and never bypass the `Verified` type-state. |
| X2 | Remapping deck indices to recipients after the final deck is known | **CP** (structurally excluded) | The map is a pure function of `(table_id, hand_id, button)` fixed at `HAND_INIT`, which is itself chained and signed. There is no message in which a different map can be asserted; a peer that computes a different one simply diverges and is caught by `STATE_HASH`. *This row is exactly assumption A10 and is only "prevented" as long as A10 is honoured in the implementation.* |
| X3 | Publishing a reveal token for a future street's index early | **D&A** | The library has no concept of "too early"; our state machine gates it. The message is constructible, achieves nothing alone (row 6), is rejected by every honest client, and is signed. |
| X4 | Encoding equivocation: two byte encodings of one logical event, signed separately, sent to different peers | **CP** | Every signed structure is a definite-length CBOR **array** with a fixed field order — no maps, so there is nothing to sort and no ordering to get wrong — plus a decode-re-encode-compare gate applied **before** any signature check. Measured to catch all three hostile encodings: non-preferred integers (`8218011a00000018`), indefinite-length arrays (`9f…ff`), and trailing bytes; truncated inputs are all rejected without panic (`research/CRYPTO_LIBS.md` §4.7). No floats anywhere, and `#[n(..)]` field indices are append-only forever, since reusing one silently changes the meaning of historical signed bytes. *Inherits A5.* |
| X5 | Cross-hand or cross-table proof replay | **CP** | Same mechanism as row 13 applied to the cryptographic objects: the `ctx` binding (A11). Requires the named regression test to be real. |
| X6 | RNG beacon abort bias: the last revealer sees everyone else's value, computes the outcome, and refuses to reveal if it dislikes it | **D&A** | Inherent to commit/reveal and not preventable without a delay function or a threshold construction, neither of which is warranted here. Non-revelation is a protocol failure attributable to a specific key, and the beacon governs only seating and the initial button — never the deck, whose randomness comes structurally from the shuffle chain (row 5). The cost of the attack is one visible, attributed refusal for one re-draw of the seating. |
| X7 | Withholding decryption shares to force a hand to abort (griefing) | **D&A / DNA** | Permanent and inherent to the n-of-n construction: any player who goes silent makes it impossible for anyone to open any further card. The missing `REVEAL_TOKEN` for a given `(hand_id, card_index)` is publicly visible and every other player's signed events prove they did their part. **Attribution depends on the case and is not uniform:**<br><br>*one silent seat, with `\|V(subject)\| >= 2`* — **D&A**;<br>*two or more seats silent simultaneously* — **DNA**;<br>*any silent seat where `\|V(subject)\| < 2`, which at `n = 2` is always* — **DNA**.<br><br>Attribution is sound for a single silent seat whenever the required voter set still has two or more members; it is unavailable when two or more seats stop at once, because each required voter set contains the other subject and D-008 forbids removing a seat from `V` on anything short of a completed certificate (`PROTOCOL.md` Q-02, `STATE_MACHINE.md` Q3, OQ-E), and unavailable whenever `\|V\| < 2` for the reason in D-007 as generalised by D-008. **It cannot be prevented, and the fix that would prevent it is forbidden** — see §7. The attacker gains no cards and no chips by doing it; under D-005 they lose their commitment when they are attributed, and where `\|V\| < 2` they do not, because the abort carries `attributed = []` and restores stacks (§9.1, OQ-A). Repeated aborts attributable to one identity are visible to everyone; in play money that is the whole penalty, and `SPEC_CS.md` §18 forbids claiming more. |
| X8 | Rage-quit: a player about to lose a large pot disconnects so the hand is voided and their money is returned | **D&A** | This is the reason **D-005** chose forfeiture over restoration. Restoring start-of-hand stacks would hand every player a free, in-protocol escape from a losing pot, available to anyone at will with no special capability. Instead the vanished player's committed chips are distributed to the remaining players in proportion to their own contributions: quitting costs exactly what folding would have cost. Chip-conserving, so G9 holds. Note also that the hand completes normally if everyone else folds — a quitter does not escape when the others simply fold. The escape is closed on the abort path (`cause = 1`, forfeiture) **only where a certificate can attribute somebody, that is where `\|V(subject)\| >= 2`**. It is **not** closed on the divergence path (`cause = 4`, restoration — X29), and it is not closed at all wherever `\|V\| < 2` — always at `n = 2`, and at any seat count if the voter set could be shrunk, which is what D-008 stops (X30). There the only available disposition is restoration with no attribution (D-007, D-008, OQ-A). |
| X9 | Knock an opponent off the network right after a large bet, to collect their forfeited commitment | **OOS** | The acknowledged cost of D-005's choice, and it is recorded there rather than hidden. It requires a real out-of-protocol capability against the victim's connection (network DoS, or control of the relay their connection depends on — §3.5). It is not free and not available to everyone, unlike X8. The trade is deliberate: close the free in-protocol exploit at the price of an expensive out-of-protocol one that §6 already lists as beyond the protocol's reach. |
| X10 | A forged timeout certificate by all other dealt-in seats, stealing the subject's action | **DNA** (where `\|V\| >= 2`) | Under **D-006** a timeout takes effect only through a certificate signed by every member of `V(subject)`, the set of **all other dealt-in seats**, naming the seat, the sequence and the parent event hash. Nominally `\|V\| = n - 1`: **one seat at `n = 2`, two at `n = 3`, nine at `n = 10`** — but that table is a ceiling, not the operative rule. `V` is the other dealt-in seats **minus any seat a completed, valid certificate has already attributed in this hand** (`PROTOCOL.md` §8.3), so the operative quantity is `\|V\|` and every rule in this row is written on `\|V\|` rather than on `n`, per **D-008**. That is not a formality: an earlier draft let a seat be removed from `V` by an unproven accusation, which collapsed `\|V\|` to one at any table size — X30. Requiring all of `V` defeats a single false accuser. It does **not** defeat a coalition consisting of all of them, who are unanimous with themselves — available with two colluders at `n = 3`. The victim can broadcast their own signed action carrying the same parent hash, so any third party sees a conflict — but with no trusted clock (A13) nobody can establish which came first. **Detected, not resolvable.**<br><br>**Whenever `\|V\| < 2` the mechanism gives no protection at all and is therefore inert.** With `\|V\| = 1`, "unanimous" is one signature and unanimity is vacuous; **D-007** makes the heads-up action deadline advisory, a `kind = 1` certificate is invalid at two seats and must be rejected by every receiver, and no signed state transition follows from a heads-up deadline. **D-008** carries the same floor to every seat count: a certificate whose required voter set has fewer than two members has no effect at all — not an error, not evidence, the deadline simply stays advisory. At `n = 2`, `\|V\| = 1` always, so heads-up is the permanent instance rather than the only one.<br><br>**The race between a late action and a certificate is settled only by the presence of an honest required voter; there is no cryptographic artefact that settles it.** An earlier draft carried this as a separate row claiming the race was structurally impossible, on the argument that a peer emitting both a `TIMEOUT_VOTE` and a real event for one stage had equivocated provably. That claim was false — the vote is signed by the voter and the action by the subject, two events by two different keys, which prove nothing against anyone — and after the `event_class` discriminator (`PROTOCOL.md` §5.2, §4.8) the two do not even occupy the same slot. The honest statement: if at least one required voter is honest and saw the action, no certificate forms and the action stands; if every required voter is dishonest, or none saw the action, a certificate forms. Nothing settles the race when a voter lies about what it saw. OQ-C records the `ACTION_SEEN` construction that would make a lying voter provable, and it is **not** adopted.<br><br>This is a genuine, unfixed limitation of the design and it is carried forward as OQ8 and, for the `\|V\| < 2` case (which heads-up always is), OQ-A. Distinguish it from X30: X10 is a coalition that *legitimately is* the whole voter set; X30 was a single client *manufacturing* that position. |
| X12 | Colluding muck: a colluder with the winning hand mucks so the pot goes to their partner | **OOS** | Collusion, §6. It also interacts with an unresolved design question: if mucking is allowed at all, the transcript can only prove "the award was correct given the players who did not forfeit", not "no unrevealed hand was better" (`research/POKER_RULES.md` §A8). That weakens G8 in a precisely stateable way. Mandatory universal reveal removes the attack but changes the game and leaks strictly more than real poker. Unresolved — OQ6. |
| X13 | Table-advert bait and switch: advertised parameters differ from those actually played | **D&A** | The advert is signed and carries the full parameter set (blinds, stacks, schedule, timers, seats). The parameters that bind are the ones every participant agreed and signed before the first hand (`SPEC_CS.md` §4), and every event carries them transitively through the chain. A mismatch is rejected before any hand starts. |
| X14 | Protocol-version downgrade | **D&A** | `protocol_version` is inside every signed body and inside `ctx`. A peer offering an older version is refused rather than accommodated (A14). |
| X15 | Lobby spam: advert spam, pinning a table in every lobby forever, or flooding `LOBBY_CHAT` | **D&A** per peer (residual DoS is out of scope) | **Scope includes lobby chat** (`PROTOCOL.md` `0x0106 LOBBY_CHAT` on `LOBBY_CHAT_TOPIC`), which is rate-limited to 1 message per 2 s with burst 5 per remote `PeerId` (`PROTOCOL.md` §7.6, `NETWORK_STACK.md` §6.6) and capped at `LOBBY_CHAT_MAX = 2 048 B` with a 512 B text field. Chat carries no security claim of any kind — see §6. For adverts: GossipSub runs with `validate_messages()`, so nothing is forwarded until the application accepts it; a failed application signature or a failed freshness check produces `MessageAcceptance::Reject`, which applies the peer-score penalty to the forwarder. The default `message_id_fn` (`source ‖ seqno`) is **overridden with a hash of `(data, topic)`**, or one peer could republish identical content under new sequence numbers forever and it would never deduplicate. Adverts with `expires_at` more than ~5 minutes ahead, or a `timestamp` in the future beyond a small skew allowance, are rejected outright; local eviction uses age-since-receipt rather than the advertised clock (`research/NAT_AND_DISCOVERY.md` §7.3). |
| X16 | Polluting `LOBBY_INFOHASH` with junk `IP:port` records | **OOS** | Structural: a DHT announce carries no proof of possession beyond the storing node's IP check, so anyone can write anything. Observed in the wild on a private random infohash within 24 minutes (§8). Cost is bounded — a bounded dial budget, deduplication, dropping private/reserved ranges, and the handshake filtering non-libp2p listeners — so the impact is dial timeouts, not a correctness failure. `SPEC_CS.md` §1 already declares the list a hint. |
| X17 | Sybil eclipse of the GossipSub lobby mesh | **OOS** | Sybil resistance is out of scope without an identity/reputation layer (§6). Mitigations: `mesh_outbound_min = 3` raises the cost of an eclipse by inbound-only Sybils, the snapshot fetch queries several independent peers, and the DHT provides a peer source independent of the mesh. None of these is a proof. |
| X18 | The relay reads game content | **CP** | End-to-end Noise/TLS terminated at the peers; the relay is a byte pipe holding no key share (**D-001**). |
| X19 | The relay forges or alters events | **CP** | Every event is application-signed and every receiver re-validates the signature and the hash chain independently (**D-001**). *Inherits A5, A6.* |
| X20 | The relay drops, delays or resets a target peer's connection | **OOS** | A liveness dependency and a DoS lever, acknowledged in **D-001**. Includes the *default* case, not only the malicious one: a public relay's **`max_circuit_duration = 120 s`** will reset a poker session mid-hand. The **duration**, not the byte cap, is the binding limit — `max_circuit_bytes = 1 << 17` is per circuit and **bidirectional**, one budget for both directions combined (`libp2p-relay 0.21.1`: a single `bytes_sent` counter on `CopyFuture`, incremented by both `forward_data` calls — `src/copy_future.rs:41-48, 78, 88-104`; see §3.5, including the kubo wording caveat). A table is a full mesh so one circuit carries one pair, and **both** peers push their own 8 979 B shuffle step and proof through it once per hand regardless of `n`, so the circuit carries `2 × 8 979 = 17 958 B` per hand: roughly 7 hands of shuffle traffic, or about 5 hands including the signed event stream. An earlier revision gave these as ~14 and ~10 by reading the cap as per-direction; the conclusion is unchanged, the margin is half what it said. Required behaviour: read the `Limit` returned with the reservation, prefer direct then DCUtR then relay, surface "direct or relayed" honestly in the network status panel, and refuse to seat a player whose only path cannot carry a hand rather than starting one that will die. A relayed connection loss is treated exactly like any other disconnect (§7). |
| X21 | Abusing a D-002 volunteer relay's bandwidth | **OOS** (resource abuse), with mandatory mitigation | Circuit Relay v2 is **not protocol-selective**: the hop and stop protocol names are compile-time constants and the `Behaviour` decides accept-or-deny purely on resource limits — it has no application ACL hook. Left alone, enabling the relay server makes the user an **open relay for the entire libp2p network**, IPFS traffic included, on their own line. The usable hook is `Config::reservation_rate_limiters` and `Config::circuit_src_rate_limiters`; admission control is installed there through a public trait re-exported at the crate root (`libp2p::relay::RateLimiter`, `libp2p-relay-0.21.1/src/lib.rs:42`), implemented by a named type that holds the admitted-peer set, and **both** vectors must be gated — one governs who may reserve, the other who may open a circuit (**D-002**, verified by compiling exactly that). That same type is also where the per-tier live-circuit ceiling of `NETWORK_STACK.md` §9.6 is enforced, because `max_circuits_per_peer` is a single global `Config` field and cannot express it. Even correctly gated, an attacker running our own client consumes capacity, so this is mitigation, not prevention. Relaying is off by default and must be disclosed plainly before it is enabled. |
| X22 | State divergence caused by an honest implementation bug | **DNA** | `STATE_HASH` detects it and play stops (G10), but the transcript shows only that two clients disagree, not who is wrong — there is no signed event to attribute, because both peers believe they followed the rules. Resolving it requires human diagnosis. It is the reason A15 and the §26 property tests exist. Note that the *deliberate* version of the same divergence — a peer publishing a `state_hash` it did not derive — is X29, and no live rule distinguishes the two, which is why neither is attributable. |
| X23 | Timing side channel on the secret permutation or on `sk_i` | **OOS** for the play-money prototype, pending measurement | Only throughput was measured, never constant-time behaviour, and arkworks is not written with curve25519-dalek's constant-time discipline (`research/MENTAL_POKER.md` §9 risk 4). Declaring it out of scope is defensible for play money and **is not defensible for real money**. Settled by `dudect`-style analysis of `shuffle_deck` and `reveal_token`, or by an explicit decision. OQ4. |
| X24 | Endpoint compromise (malware reading the player's own cards or stealing their signing key) | **OOS** | §6, and assumption A8. |
| X25 | Out-of-band collusion (a voice channel, a shared screen, one person at two machines in a room) | **OOS** | §6. This is the single most damaging real-world attack on any poker system and no cryptography addresses it. |
| X26 | Multi-accounting / Sybil identities at one table | **OOS** | §6. Identity is free: a new profile is a new Ed25519 key. |
| X27 | Coercion of a player | **OOS** | §6. |
| X28 | Traffic analysis of relayed and DHT traffic | **OOS** | §6 and §8. The relay sees who talks to whom, when and how much; the DHT publishes presence to strangers on a schedule. |
| X29 | Fault any table on demand by publishing a false `state_hash` at a checkpoint | **DNA** | A `state_hash` is a one-field value in a message every peer must emit. A single peer that publishes a value it did not derive forces the `PROTOCOL.md` §6.3 case (c) path: the hand aborts with `cause = 4`, stacks are restored, and the table closes, with no attribution live. It needs no invalid signature and no divergent transcript, and "at least two peers disagreeing" is satisfied by one liar plus the honest victim. Bounded by three things — it costs the attacker the table and, in a tournament, their own equity in it; checkpoint 7 sits **before `SHOWDOWN_REVEAL`**, and no checkpoint is placed after a hole card has been opened **to anyone but its owner** (every player opens its own two cards at `DEAL_PRIVATE`, which precedes checkpoints 3–7, so the rule is about *public* opening and is stated that way rather than in the falsifiable shorter form an earlier revision used), so the attacker must commit while it still knows only its own hand; and the evidence, the unanimous transcript plus every peer's signed `STATE_HASH`, is written to the profile directory and **is preserved in a form sufficient for a human, or for a future adjudicator, to diagnose the divergence — no adjudication procedure is specified.** An earlier revision of this row claimed the evidence was "deterministically adjudicable offline by any third party running the reference engine over it, forever". **That claim is withdrawn.** No document in this corpus defines, versions against `protocol_version`, or authorises such a reference engine, and moving a derivation offline does not manufacture the observer-independent reference that this row's own argument says does not exist. Whether a canonical reference engine is named is an open decision, recorded in `docs/DECISIONS.md`'s open list. Until it is, this third bound is **evidence preservation, not recourse**: neither live nor offline adjudication is available, and neither is claimed. An earlier draft resolved case (c) by removing the odd peer out under a "unanimity minus one" rule; that rule was **deleted**, because it let `n-1` colluders take an honest player's committed chips at `n >= 3` (two colluders suffice at `n = 3`), and because there is no observer-independent derivation at run time by which anyone could be named — every peer derives with its own engine, so "attribute whoever disagrees" is a vote over the facts, which `SPEC_CS.md` §15 forbids. Not solved. Related: X8, X22, OQ-D. |
| X30 | Shrinking the required voter set to one seat — itself — and then taking the subject's committed chips with a certificate that seat signs alone, at **any** table size | **D&A**, and only because of a rule our own client enforces | **The attack.** `V(subject)` was the other dealt-in seats minus any seat "named as the subject of an outstanding, older unmet deadline", and a seat is *named* by any peer emitting a `TIMEOUT_VOTE` against it — an assertion the protocol concedes is unprovable when the voter lies ("there is no artefact that settles the race when a voter lies about what it saw", `PROTOCOL.md` §8.3). Nothing bounded how many seats one client could name. Six-seat table, one modified client at seat 1: Mallory votes against seats 2, 3, 4 and 5 at one stage; at the next she declares seat 6 the subject, and `V(6) = {1,2,3,4,5} \ {2,3,4,5} = {Mallory}`. She emits the one required vote, assembles a complete certificate from her own signature, the stage completes, a `kind = 2` certificate aborts the hand with seat 6 attributed, and under **D-005** seat 6's committed chips are forfeited to the remaining seats — which is mostly to her.<br><br>**The earlier scoping on `n` did not stop it, and that is the point.** Every protection D-007 wrote was conditioned on `n == 2`; here `n` is six and stays six, so nothing rejected the certificate. The nominal `\|V\| = n - 1` table printed in three documents was never the operative rule — `\|V\|` was `n - 1 - \|excluded\|`, and the attacker controlled `\|excluded\|`. This is the A-1 attack reappearing above heads-up, and worse than A-1, which cost a folded hand rather than chips.<br><br>**The mitigation.** **D-008**: every rule that weakens, disables or gates the certificate is scoped on `\|V\|` and never on `n`; a certificate whose required voter set has fewer than two members **has no effect**; and **a seat leaves `V` only once a completed, valid certificate names it — being voted against is not exclusion.** The second half is what makes the first hold inductively: each exclusion now costs a completed certificate, and each such certificate needed `\|V\| >= 2` at the moment it formed, so `V` cannot be collapsed by assertion at any seat count. Normative in `PROTOCOL.md` §8.3/§8.4 and `STATE_MACHINE.md` §8.4; checkable by any verifier replaying the transcript, with no new cryptography.<br><br>**Why D&A and not CP, stated rather than rounded up.** The message stays constructible: a modified client can always sign and broadcast a one-signer certificate. What changed is that an honest client gives it no effect, so the state does not advance and no chips move; the artefact names its signers, so it is bound to a key. The attribution half is deliberately weak and must not be overstated — under D-008 an inert certificate is *not evidence of misbehaviour*, so no seat is sanctioned for emitting one and nothing is forfeited; what is transferable is the artefact, not a verdict. And the rejection rests on our own clients enforcing the `\|V\|` floor, an implementation obligation in the class of A12 and A14, not on A1–A7 — which is exactly why this row is not CP.<br><br>**Residual.** D-008 closes the *manufactured* `\|V\| = 1`, not the earned one. Where the voter set is honestly small or honestly hostile, X10 stands unchanged (**DNA**), and two or more seats going silent together is still X7's DNA case. |

**Extended catalogue counts: CP 6 · D&A 7 · DNA 3 · OOS 12 · split D&A/DNA 1 (X7)
· total 29.**

**X11 is retired.** Its claim — that unanimity structurally excluded the race
between a late action and a timeout certificate — was false, and its substance is
folded into X10. The number is not reused, so that references to X11 elsewhere
resolve to a deletion rather than to a different attack.

### 5.4 Totals

| Bucket | §17 catalogue | Extended | Combined |
|---|---:|---:|---:|
| Cryptographically prevented (CP) | 11 | 6 | **17** |
| Detected and attributed (D&A) | 7 | 7 | **14** |
| Detected but not attributable (DNA) | 0 | 3 | **3** |
| Split D&A / DNA by case (X7) | 0 | 1 | **1** |
| Out of scope (OOS) | 1 | 12 | **13** |
| **Total** | **19** | **29** | **48** |

Counted after the Phase 0 review corrections: X11 removed from CP, X29 added to
DNA, X7 reclassified from D&A to the split row. Then after the Phase 1
verification: **X30 added to D&A** (the manufactured single-voter certificate,
closed by D-008), taking the extended catalogue from 28 to 29 and the combined
total from 47 to 48. One row counts in exactly one line of this table; X7 has its
own line because its class depends on the number of simultaneously silent seats
and on `|V|`, and forcing it into either bucket would overstate one case.

Read the CP column with A3 and A4 in mind. Rows 1–4 of the §17 table — four of the
seventeen CP entries, and the four that matter most to the integrity of the deck —
rest on the soundness of an unaudited 1779-line implementation of Bayer–Groth. If
that review fails, those four move to "not prevented and not detected", which is
the worst bucket in the scheme and one that no row currently occupies.

### 5.5 The `SPEC_CS.md` §25 malicious peers, mapped to rows and tests

`SPEC_CS.md` §25 names eleven malicious peer implementations and requires, for
each, that the attack be *"cryptographically impossible **or** unambiguously
detected and the hand stopped, **per the threat model**"*. The map lives here
because this document holds the CP / D&A / DNA classification that assertion is
measured against; separating the map from the classification is how a cheater ends
up with no test.

The test modules are **planned locations, not existing code** — `tests/adversarial/`
is this document's own source-tree responsibility (§1) and none of it is written
yet. Every asserted outcome below is the outcome the test must assert; a test that
asserts something weaker than the row it points at is a specification failure, not
a test failure.

| §25 malicious peer | Catalogue row | Test module (planned) | Asserted outcome |
|---|---|---|---|
| `CheaterDuplicateAce` | §5.2 row 2; G3 | `tests/adversarial/deck_integrity.rs` | **CP** — the tampered deck is rejected at proof verification; no honest client advances. *Inherits A3, A4.* |
| `CheaterReplaceCard` | §5.2 rows 1, 3; G3 | `tests/adversarial/deck_integrity.rs` | **CP** — a 53rd plaintext, or a shortened deck, is not provable and is rejected. *Inherits A3, A4.* |
| `CheaterInvalidShuffle` | §5.2 row 4(a); G4 | `tests/adversarial/shuffle.rs` | **CP** — any transition that is not a permutation-and-re-randomisation fails verification. *Inherits A3, A4.* |
| `CheaterPredictableRNG` | §5.2 row 5; A9; X6 | `tests/adversarial/rng.rs` | **CP for the deck**, asserted **statistically**. The shuffle chain is uniform if **one** shuffler is honest, so the test seeds every cheater's RNG from a fixed value, leaves exactly one honest shuffler, and asserts the final deck order is not predictable from the cheaters' seeds. Recorded explicitly: **this is a statistical test, not a proof**, and it is the only row in this table whose assertion is not a hard accept/reject. The commit/reveal beacon's own abort bias is X6 and is out of this row's scope. |
| `CheaterReplayAction` | §5.2 row 13; X5; G6 | `tests/adversarial/replay.rs` | **CP** — an event or proof from hand `h` presented in hand `h+1`, or on another table, is rejected. Includes the mandatory `ctx` regression test of OQ3. *Inherits A5, A11.* |
| `CheaterIllegalRaise` | §5.2 row 8; G5 | `tests/adversarial/engine_rules.rs` | **D&A** — the message is constructible; every honest client rejects it against the deterministic engine (A12), the state does not advance, the signature names the sender. |
| `CheaterFakeStack` | §5.2 row 9; G5 | `tests/adversarial/engine_rules.rs` | **D&A** — stacks are derived, never accepted from the wire; the divergence surfaces at the next `STATE_HASH` checkpoint with the liar's signature on the causing event. |
| `CheaterEquivocation` | §5.2 row 16; G7 | `tests/adversarial/equivocation.rs` | **D&A** — two chained events by one key agreeing on `(protocol_version, table_id, hand_id, sequence, event_class)` with different `event_hash` values produce a transferable `EquivocationProof`. The test must also assert the **false-positive** half of G7: no unchained pair — two honest lobby adverts, or a `JOIN_REQUEST` alongside the sender's own `RNG_REVEAL` — ever produces one. |
| `CheaterReadOpponentCard` | §5.2 row 6; G1 | `tests/adversarial/hole_card_secrecy.rs` | **CP** — a coalition of `n-1` holding every message it legitimately received cannot output the victim's hole cards. *Inherits A1.* |
| `CheaterFutureBoard` | §5.2 row 7; X3; G2 | `tests/adversarial/street_gating.rs` | **CP** for opening a future street's index; **D&A** for the attempt — an early reveal token opens nothing and is rejected and attributed. *Inherits A1, A10.* |
| `CheaterDisconnect` | X7, X8, X30; §7 | `tests/adversarial/disconnect.rs` | **D&A with one silent seat where `\|V(subject)\| >= 2`; DNA with two or more silent seats; DNA wherever `\|V\| < 2`, which at `n = 2` is always** — matching X7's split class. The test must cover all three cases, and must assert that a `\|V\| < 2` abort carries `attributed = []` and moves no chips (OQ-A). It must also cover **X30**, because D-008's floor is only as real as the check that enforces it: a client emitting `TIMEOUT_VOTE`s against several seats must **not** thereby shrink `V` — exclusion requires a completed, valid certificate — and a certificate assembled with `\|V\| < 2` must have no effect at **any** seat count, not merely at two. |

**The two mandatory standalone tests of `SPEC_CS.md` §25**, which are not tied to a
named cheater implementation:

| Mandatory test | Catalogue row | Test module (planned) | Asserted outcome |
|---|---|---|---|
| A modified client cannot obtain the plaintext of an opponent's hole cards from data it legitimately received over the network | §5.2 row 6; G1 | `tests/adversarial/hole_card_secrecy.rs` | **CP** — asserted against the counting argument of G1, with `DEAL_PRIVATE` broadcast, so the test must give the adversary every broadcast token and still fail to open the victim's index. *Inherits A1.* |
| One malicious player cannot choose the future board by manipulating the last RNG/shuffle step | §5.2 rows 4(b), 5; G2; A10 | `tests/adversarial/last_shuffler.rs` | **CP** — the last shuffler sees only ciphertexts under `apk` and holds one of `n` shares, so grinding permutations has nothing to grind toward. The test must fix the deck-index map **before** the shuffle chain, because the property is exactly A10 and collapses without it. |

`CRYPTOGRAPHY.md` §12 item 8 and `STATE_MACHINE.md` §10's adversarial notes are
subordinate to this table: they enumerate library- and engine-level checks, while
this is the single map from a §25 name to the claim it must falsify.

---

## 6. What cryptography does not solve

`SPEC_CS.md` §18 requires this section to be explicit and concrete. It draws the
line between **protocol cheating** — attacks executed through the poker protocol
itself, which is what the design targets — and **real-world / endpoint cheating**,
which it does not and cannot address. Nothing in this project may be described as
making all cheating impossible.

**Collusion between players who share hole cards out of band.** Two players on a
voice call, telling each other their cards, are running a *legal* protocol
perfectly. Every signature verifies, every proof checks, every state transition is
correct. The protocol sees two honest participants. Collusion converts a
multi-player game into an information asymmetry that no cryptography can remove,
because the information was legitimately theirs to disclose. Real poker sites fight
this statistically — hand-history analysis, VPIP/PFR correlation across pairs,
relationship graphs — which requires exactly the central hand database that
`SPEC_CS.md` §34 forbids. **We have no equivalent and should not pretend to.** In
play money the incentive is low; that is a property of the deployment, not of the
protocol. This is the attack the design is *least* able to touch and the one most
likely to matter if the project ever handled value.

**Malware on the player's machine.** Software running as the player reads that
player's hole cards from process memory, because the player's client must decrypt
them to display them. It can also steal the application signing key and the libp2p
identity key, at which point it *is* that player as far as every other participant
can tell. Mitigations reduce the window and do not close it: `Zeroizing` and
`ZeroizeOnDrop` on the Ed25519 seed, the derived KEK, pre-reveal RNG secrets,
decryption shares and every mental-poker private scalar; a redacting `Debug`/`Display`
newtype so secrets cannot reach a log by accident; DPAPI with `CRYPTPROTECT_UI_FORBIDDEN`
and application entropy so another process running as the same user cannot unwrap
the blob. Honest limitation from `research/CRYPTO_LIBS.md` §5: **`zeroize` cannot
reach copies the allocator, the compiler, or the OS swap file already made.**

**Screen sharing and real-time assistance.** A second person watching the screen,
or a solver consulted between actions, is invisible to the protocol: the player
submits legal actions with valid signatures. Detecting it requires client-side
attestation or behavioural surveillance, neither of which is compatible with an
open-source client that anyone can modify — and §17 already tells us to assume the
client *is* modified.

**Sybil identities and multi-accounting.** A poker identity is an Ed25519 key
generated locally at zero cost. One person can seat several identities at one
table and play them as one hand. There is no identity layer, no proof of personhood
and no reputation with teeth. Note the direct consequence for D-005 and X7: the
"reputation penalty" for repeatedly aborting hands is only as strong as the cost of
a new identity, which is zero. In play money the visible, attributable record of
aborts is the entire sanction, and `SPEC_CS.md` §18 explicitly forbids claiming
more. Two limited mitigations are worth noting without overstating them: the DHT
roster ties identities to IP addresses (§8), which is itself a privacy cost rather
than a feature, and a table's participants can choose whom they seat.

**Lobby chat.** `LOBBY_CHAT` (`PROTOCOL.md` §4/§7) carries **no security claim of
any kind**. It is signed by the sender's application key, which says only which key
emitted the bytes; it is not authenticated as coming from any particular *player*
beyond that key, and impersonation by `display_name` is trivial and expected —
identity is free (§6, Sybil), and a display name is a display string, never an
identifier. Chat text is never parsed, never an identifier, and never an input to a
state transition. It is the one message class that is pure attacker-controlled
UTF-8 reaching a UI, so `PROTOCOL.md` restates the §9.4 string rules at the message
rather than letting it inherit them: control characters `U+0000`–`U+001F` and
`U+007F` and the bidi overrides `U+202A`–`U+202E`, `U+2066`–`U+2069` are rejected.
Spam and flooding are X15.

**Coercion.** A player forced to reveal their cards or to play a certain way is
outside every technical boundary this document draws.

**Traffic analysis.** Three separate exposures compound here. (i) A relay sees which
peers talk to each other, when, how much and for how long — this is the metadata
cost D-001 requires be documented rather than glossed. (ii) The Mainline DHT
publishes presence, on a schedule, to strangers (§8). (iii) Even on a direct
connection, packet timing and volume around each action leak: a long think followed
by a large burst is visible. No padding or cover traffic is planned. A relay
operator is in the strongest position of the three, and under D-002 a relay
operator may be another player.

**Denial of service.** Explicitly out of the protocol's reach (§17 row 19). Two
poker-specific aggravations deserve naming rather than being folded into a generic
"DoS is out of scope": an attacker who knocks a player offline at the moment they
face a large bet collects that player's forfeited commitment under D-005 (X9), and
the DHT roster is exactly the target list an attacker needs to find the victim's IP
in the first place (§8).

**Deliberate disconnection.** Covered in full in §7. A player can always force a
hand to abort by going silent at the worst possible moment, and this cannot be
prevented without breaking hole-card secrecy.

---

## 7. The disconnect / abort problem

This is the sharpest security-versus-convenience trade in the system, and
`SPEC_CS.md` §19 decides it in advance: *"Never create a mechanism that, for the
sake of disconnect robustness, allows several players to decrypt others' hole cards
prematurely. Security takes precedence over conveniently finishing a hand."*

### 7.1 The threat

Barnett–Smart is n-of-n. Any player who was dealt in holds one share of the joint
deck key, and **until they publish, no further card can be opened by anyone** —
not the flop, not the other players' showdown cards. A player who stops responding
at a chosen moment therefore halts the hand for everyone. `ziffle` does nothing
about this and neither does the underlying construction
(`research/MENTAL_POKER.md` §8).

The capability is unremovable, cheap and available to every participant. It is
catalogued as X7 (griefing) and X8 (escaping a losing pot).

### 7.2 The fix that must be refused

The standard robustness upgrade is `t`-of-`n` threshold ElGamal via Feldman or
Pedersen VSS, so a quorum can finish the hand without the missing player. **This
must not be done.** With `t < n`, any `t` colluding players can decrypt **every**
hole card at the table, at any time, with no protocol violation whatsoever. That
directly destroys G1 and the main invariant of §35, and it converts the disconnect
problem into the far worse attack of "collude, disconnect a player, read their
cards" — which is exactly the attack §19 names.

The same reasoning forecloses every softer variant: escrowed shares, a "recovery"
share held by a subset, a designated backup opener, or any scheme in which someone
other than the card's owner can cause it to open. If a mechanism can finish the
hand without the absent player, that mechanism can also open the absent player's
cards, and an attacker will arrange the absence.

`n`-of-`n` stays. Abort and attribute.

### 7.3 What is done instead

Three cases, sharpened by **D-005** and **D-006**, which correct a conflation that
matters — and read throughout with **D-007** as corrected and generalised by
**D-008**, which scope every certificate rule on `|V|` rather than on `n`:

**(a) The human is away but the client is running.** This is the overwhelmingly
common case and it costs nothing. Publishing a decryption share is an automatic
client step that never waits for the human, so an away-from-keyboard player still
cooperates cryptographically: the board opens on schedule, showdowns work, the hand
plays out normally. The only thing missing is a betting decision, and the engine
supplies it — **check if nothing is owed, fold if facing a bet, never fold a hand
that could check for free.** The auto-action is a real signed protocol event that
every peer derives identically from the same state, not a local UI convenience.
After a configured number of consecutive auto-actions the seat is marked sitting
out and enters case (b). **No timeout of any kind ends the tournament or the cash
game** (D-006).

The deadline itself is agreed without a trusted clock through the **timeout
certificate** of D-006, whose forgery by all the other dealt-in seats is X10 and
whose voter set cannot be shrunk by assertion (D-008, X30).

**This whole paragraph applies only where `|V(subject)| >= 2`.** Where the
required voter set has fewer than two members there is no action-timeout
certificate at all (D-007, generalised by D-008): the deadline is a UI countdown,
it produces no signed transition, `consecutive_auto_actions` therefore never
increments from a timeout, and such a seat is never marked sitting out by the
deadline path — it can still sit out voluntarily. At two seats `|V| = 1` always,
so this is the permanent heads-up case; above two seats it arises only after
completed certificates have legitimately attributed enough seats to bring `|V|`
below two, and never through unproven accusations (X30). `SPEC_CS.md` §32 makes
heads-up the first shipped mode, so the first mode this project ships is the one
in which this machinery does not apply. See X10, X30, §9.1.2 limitation 3, and
`STATE_MACHINE.md` §8.5 and §9.5.

**(b) The player is absent between hands.** From the next hand onward they are
simply **not a party to the cryptography**: not in the joint key, not dealt cards,
and nothing waits for them. Their seat is a chip pile that pays its blinds and
antes and folds when the action reaches it, draining one orbit at a time until it
busts. On reconnect they rejoin the key-holder set at the next hand boundary. No
new cryptography is needed, because a player who is never dealt in never has to
open anything.

**The forced deviation from live rules, stated rather than smoothed over.** Under
TDA-style rules an absent player is still dealt in, and if their stack is below the
blind they are all-in for it and their hand goes to showdown — they can win. We
cannot do that. Opening their cards at showdown needs their decryption share and
they are not there to publish it, and any mechanism that let the others produce it
would be precisely the mechanism §7.2 forbids. **So an absent seat posts its blind
as dead money, takes no cards, and cannot win the hand it pays for** (D-005). The
practical difference is small — absent players lose their blinds either way and
only rarely win an all-in — but it is a deviation and it is documented as one.

**(c) The player vanishes mid-hand, having been dealt in.** Two sub-cases:

1. *The hand can still be decided without opening anything* — everyone else folds
   to one player. It completes normally; no shares are needed. Note the useful
   consequence: a player who quits to escape a loss does not escape if the others
   simply fold.
2. *Anything else* — the deadline expires and the hand **aborts**. Timeout, abort
   and attribution are all protocol events in the transcript, so every participant
   can verify what happened and when (`SPEC_CS.md` §19's "evidence of which peer
   failed"). **Attribution, however, is available in only one of three cases**, and
   the qualification belongs here rather than in the catalogue alone (X7):

   * *one silent seat, with `|V(subject)| >= 2`* — the abort carries a signed
     record of which peer failed to publish, and D-005 forfeiture applies.
     **D&A.**
   * *two or more seats silent simultaneously* — no certificate can be built,
     because each required voter set contains the other subject and D-008 allows
     a seat to leave `V` only once a completed certificate has named it. The
     interim
     behaviour is to abort with `attributed = []`, `cert_hash = None` and stacks
     restored: nobody is named, because a seat that did not vote may be silent,
     partitioned or simply slow, and forfeiting a partitioned honest seat's chips
     under D-005 would be a positive-gain attack on an honest player. This is a
     safe default, not a solution — it lets two colluding seats void a hand for
     free. `PROTOCOL.md` Q-02 and `STATE_MACHINE.md` Q3 stay open; OQ-E below is
     blocking. **DNA.**
   * *any silent seat where `|V(subject)| < 2`, which at `n = 2` is always* — no
     deadline is enforceable at all (D-007, generalised to `|V|` by D-008). A
     `kind = 2` certificate may still end the hand, because otherwise a vanished
     opponent deadlocks the table forever, but it carries `attributed = []` and
     produces no forfeiture; stacks return to their start-of-hand values. This
     reopens the rage-quit escape D-005 closed, wherever `|V| < 2` — in practice
     heads-up, since above two seats `|V|` falls that low only after completed
     certificates have attributed the other seats, which an attacker cannot
     manufacture (X30). **DNA**, and recorded as an unfixed limitation in §9.1
     rather than hidden. OQ-A.

### 7.4 The chips on abort, and the exploit each option opens

The disposition of committed chips is a security decision, not an accounting one,
because the two obvious options open different attacks.

| Option | Exploit it opens |
|---|---|
| Restore every stack to its start-of-hand value | A free, in-protocol escape from a losing pot, available to **anyone, at will, with no special capability**. A player about to lose a big pot disconnects and gets their money back. |
| The absent player forfeits what they committed; it is distributed to the remaining players in proportion to their own contributions | A DoS incentive: an opponent who can knock a player offline right after a large bet collects it. |
| Fault the table with restoration, when nobody can be attributed (`cause = 4`) | The same free escape as row 1, now reachable by a single peer publishing a wrong `state_hash` at a checkpoint (X29), and unavoidable, because there is no observer-independent derivation by which the liar could be named. |

**D-005 takes the second**, and the reasoning is that the two costs are not
comparable. The rage-quit escape (X8) is an in-protocol exploit anyone can use for
free and must therefore be closed. Knocking a peer off the network (X9) is an
out-of-protocol attack that this document already lists as out of scope and that
requires real capability against the victim's connection. Closing the free exploit
at the price of a documented, expensive one is the right trade. It is also the
closest chip-conserving analogue to the live rule, under which a player who cannot
act has their hand folded and loses what they bet — here the hand cannot continue,
so their commitment goes to the players who were still in it. Chip conservation
(G9) holds on this path and must be property-tested on it specifically.

**The third row is not a choice D-005 made; it is what is left when nobody can be
named.** On `HAND_ABORT cause = 4` — an unresolvable state divergence — restoration
is not safe, it is merely the only disposition every peer can agree on. Forfeiture
there would punish a party nobody can identify: attribution by "differs from the
derivation" needs an observer-independent reference derivation, and at run time
there is none, so the rule is only implementable as "attribute everyone who
disagrees with me", which is a vote over the facts and `SPEC_CS.md` §15 forbids it.
The same reasoning applies to the two no-attribution cases of §7.3(c). Which
disposition is correct is escalated as **OQ-D**, and where `|V| < 2` — heads-up
always — as **OQ-A**; the
interim default is restoration, on the principle that a liveness and fairness loss
is preferable to a theft from an honest player. Chip conservation holds on the
restoration paths too, and must be property-tested on them as well (G9).

### 7.5 Residual, unfixed

A malicious player can **always** force a hand to abort by going silent. They gain
no cards, and where `|V(subject)| >= 2` with no other seat silent they gain no
chips either, because D-005 forfeits their commitment; but they can degrade the
game for everyone indefinitely. The only mitigations are social — visible, attributable, repeated
aborts — and §6 explains why those have little force when identity is free. This is
a permanent property of the chosen construction, and it is the price of G1.

**Two cases are worse than that, and are not fixed.** When two or more seats go
silent at once, and whenever `|V(subject)| < 2` — which at two seats is always —
no attribution is possible and the abort carries `attributed = []` with stacks
restored (§7.3(c)). In those cases the silent player does *not* lose their
commitment: they escape the pot for free. That is the rage-quit escape D-005
closed reopening in the corner of the design where nobody can be named, and it is
carried as OQ-A and OQ-E rather than argued away. What is **not** left open is an
attacker reaching that corner deliberately at a larger table by voting the voter
set away: that was X30, and D-008 closes it by making exclusion from `V` cost a
completed certificate.

---

## 8. Privacy exposure from a fixed public infohash

`SPEC_CS.md` §3 mandates one fixed `LOBBY_INFOHASH` shipped in every client, and
requires the resulting limitation to be documented: DHT records are unverified,
short-lived, must be re-announced, and a public infohash is continuously watched,
so announcing publishes the player's IP address as someone interested in this one
specific value. None of it justifies a central server. The player should know it.

This section states it with numbers, all measured
(`research/MAINLINE_DHT.md` §3.4, §3.6, §7).

### 8.1 What is published, by construction

`announce_peer(LOBBY_INFOHASH, Some(external_quic_port))` writes the tuple
`(public IPv4, port)` into a public network with **no authentication and no access
control**. `LOBBY_INFOHASH` ships in every binary, so *everyone* knows it. There is
no unlisted mode.

Measured: one announce landed on **19–32 independent storing nodes**, and an
unrelated process retrieved it **~113 ms** after asking. Repeated announces widen
the storing set rather than merely refreshing it — five announces put the record on
**54** nodes.

### 8.2 How many strangers learn it

A single `get_peers` contacts **105–176 distinct DHT nodes**, each of which learns
(our IP, this infohash); the accompanying self-lookup tells another ~130–140 our IP
alone. At the 10-minute re-announce cadence that is roughly **150 disclosure events
per day**, to a rotating, self-selected set of strangers. Mainline is heavily
crawled by anti-piracy monitors, academic scanners and Sybil nodes, all of whom log
exactly this.

**This is not hypothetical, and it was observed first-hand.** During the TTL
experiment, an infohash of 20 bytes generated on the development machine — never
published anywhere, referenced by exactly one `announce_peer` — returned a
**stranger's address** 24 minutes later:

```
--- t+24 min
get_peers(2f59ec56c7dd6013e5b070a42762a20a496d1f8c) ... unique_peers=2
    peer 203.0.113.28:31786      <- not us
    peer 198.51.100.17:45888       <- us
```

Somebody is harvesting announce traffic near the keyspace and re-announcing under
whatever infohashes they see. A *publicly known, hard-coded* infohash is a far
easier target than a random one, so treat this as a floor on the surveillance, not
a ceiling. It is simultaneously a live demonstration of X16: the DHT will hand our
client addresses that never ran this software.

### 8.3 What an observer can derive

1. **The player roster.** Poll `get_peers(LOBBY_INFOHASH)` on a loop and you get a
   near-complete list of IPs running this client. Each node returns a random ≤10
   subset, so one query undercounts, but repeated polling converges — and Sybil
   nodes placed near the infohash in the keyspace receive the write stream directly.
2. **Session times.** A record dies without re-announce (measured: retrievable ~45
   minutes, gone by ~50, thinning from t+30), and we re-announce every 10 minutes.
   Presence is therefore a ~10-minute-resolution online/offline signal per IP; over
   weeks it is a behavioural profile — when this person plays, how long, how often,
   which time zone.
3. **Geolocation and ISP** by trivial GeoIP, often with a reverse-DNS hostname.
4. **Linkage across time, and a stronger identifier than either half.** A
   residential IP is stable for days to months. Combined with the deliberately
   persistent `PeerId` (§3 requires persistence), an observer who dials us obtains
   a **stable cryptographic identity bound to a physical location** — a stronger
   link than either identifier alone.
5. **A target list.** The roster doubles as a list of hosts to DoS, port scan, or
   attempt to de-anonymise. Poker supplies the motive: knowing which IP is at which
   table is the first step in targeted collusion, or in DoSing the opponent who is
   about to act (X9).
6. **A trivially greppable signature.** For anyone who can observe the user's
   network — ISP, employer, household, a legal request — a UDP packet carrying a
   well-known 20-byte constant identifies the application immediately. Announcing
   on `LOBBY_INFOHASH` is in no way steganographic.

### 8.4 What it does not leak

Nothing about cards, stacks, table membership or game state. All of that travels
over libp2p between table participants only, and never through the DHT
(`SPEC_CS.md` §1). And a DHT record is not an identity claim — the token check
binds only to an IP, so anyone can announce any `IP:port` under our infohash. The
list is a hint, exactly as the spec says.

Add to this the relay's metadata view under D-001 (who talks to whom, when, how
much, how long) and, under D-002, note that the relay operator may be **another
player**. That is a privacy consideration the relay-volunteer setting must disclose
in both directions.

### 8.5 Honest mitigations

None eliminates the disclosure. The spec forbids a central fallback, and rightly.

* **Tell the user before the first announce.** A one-time, plain-language consent
  screen: joining the lobby publishes your IP address to a public network, and
  anyone can see that this computer is running this poker client. §3 requires this
  be documented; making it a UI element is the honest version.
* **Ship a no-announce direct-invite mode from day one.** Join a table by an
  out-of-band ticket (multiaddr plus table key). Discovery cost zero, privacy cost
  zero.
* **Announce only while actually looking for a game**, and stop once seated.
* **Put nothing but `IP:port` in the DHT** — no nicknames, no `PeerId`, no table
  metadata. This is already the design; keep it.
* **VPN works, Tor does not.** Mainline is UDP and Tor is not, so Tor cannot carry
  the DHT. A VPN moves the exposure to the VPN operator; say so plainly rather than
  recommending it as a fix.
* **An epoch-rotating infohash** (`H("p2p-poker/lobby/v1" ‖ floor(day))`) would stop
  a historical crawl of one constant from indexing the whole user base forever. It
  does not stop a live observer, costs cross-version compatibility, and the spec
  currently mandates one fixed constant — so it is an improvement, not a fix, and
  it would need a numbered decision.

### 8.6 One further exposure that is not the DHT's fault

`mainline` 8.0.0 starts in *adaptive mode* and, on a routing-table refresh tick,
**silently promotes itself to a public DHT server** if it is not firewalled, at
which point it begins answering strangers' queries and **storing other people's
data** — up to 2000 infohashes × 500 peers, plus BEP 44 values. There is no
opt-out flag. For a poker client that is unexpected inbound traffic, unexpected
bandwidth and unexpected third-party content on the user's machine. The supported
block is a deny-all `RequestFilter` with minimal `ServerSettings`, verified to
compile and run (`research/MAINLINE_DHT.md` §4.1). **Ship that filter.** If we ever
want to be a good DHT citizen, make it a deliberate opt-in toggle, not an accident.

---

## 9. Known limitations and open questions

### 9.1 Limitations that are permanent given the design

#### 9.1.1 Deviation register

`SPEC_CS.md` §36 and the Phase 0 brief require a deviation from a binding spec
section to be **recorded**, not absorbed. This is the single register for the whole
corpus; a document that deviates adds a row here and points at it rather than
carrying its own list.

| # | Spec section | Deviation | Recorded in |
|---|---|---|---|
| 1 | §7 | `OsRng` no longer exists; the OS CSPRNG is `getrandom::SysRng`. A rename, not a weakening. | `CRYPTOGRAPHY.md` §7.2 |
| 2 | §16 | `RNG_COMMIT` / `RNG_REVEAL` run **once per table** (setup chain), not per hand. The shuffle chain is the per-hand randomness; the beacon covers seating and the initial button only. | `PROTOCOL.md` §4.4, `CRYPTOGRAPHY.md` §7.3 |
| 3 | live procedure | **No burn cards.** Not a spec deviation — `SPEC_CS.md` does not mention burns — but a departure from live procedure, recorded because it changes `index_map_hash`. A burn exists to defeat physical marked-card and edge-sorting attacks and has no analogue here; a burn that is never opened is indistinguishable from an unused index. Two conforming clients with different index maps would produce a guaranteed `DECK_COMMIT` mismatch every hand. | `PROTOCOL.md` §4.5 |
| 4 | §19 / TDA | An absent seat posts its blind as dead money, takes no cards, and cannot win the hand it pays for (D-005). | `STATE_MACHINE.md` §12, this document §7.3 |
| 5 | §4 / D-006 | Wherever the required voter set has fewer than two members — at two seats always — an action deadline is advisory and produces no signed transition (D-007, scoped on `\|V\|` rather than on `n` by D-008), so the hand-to-hand progress guarantee of §4 depends on the participating clients cooperating. | `PROTOCOL.md` §8.3 |

#### 9.1.2 The limitations themselves

1. **A malicious participant can always force a hand to abort** by going silent
   (X7). Inherent to n-of-n; the fix is forbidden by §19 (§7.2).
2. **An absent seat cannot win the blind it posts** (D-005). A documented,
   deliberate deviation from TDA rules, forced by the same n-of-n property.
3. **Action deadlines are not enforceable against a determined opponent, and the
   failure mode differs by the size of the required voter set** (X10). Where
   `|V(subject)| >= 2`, a coalition of all the other dealt-in seats can steal one
   honest player's action through a certificate that is valid by construction; it
   is detectable and not adjudicable without a trusted clock. Where `|V| < 2` no
   action deadline is enforceable at all: **a certificate has no effect below
   `|V| = 2`** (D-008) — it is not an error and not evidence, the deadline stays
   advisory — so a stalling opponent cannot be punished and the only remedy is to
   leave the table. **At two seats `|V| = 1` always, so heads-up deadlines are
   advisory permanently and by construction** (D-007): the countdown is a UI
   element that produces no signed transition. The scoping matters and is not
   cosmetic: the same rules written on `n` left a modified client able to
   manufacture `|V| = 1` at a six-seat table and take an honest player's chips —
   X30. **Neither case is solved**; what D-008 fixes is that only the honest cases
   remain.
4. **Where `|V| < 2` — heads-up always — a hand that stalls on a missing
   cryptographic contribution aborts without attribution and with stacks
   restored**, so such a player can escape a losing pot by going silent. This
   reopens the rage-quit escape D-005 closed, in that corner only (X7, X8). The
   alternative — letting one peer's unilateral assertion forfeit the other's
   committed chips — is a positive-gain attack on an honest player, so the interim
   default prefers a liveness and fairness loss to a theft. Closing this requires a
   mechanism that does not exist: **OQ-A**.
5. **Any single peer can fault any table on demand** by publishing a `state_hash`
   it did not derive at a checkpoint (X29). The hand aborts with `cause = 4`,
   stacks are restored and the table closes, with nobody named, because no
   observer-independent derivation exists at run time by which the liar could be
   attributed. It costs the attacker the table and their own equity in it, the
   checkpoint sits before `SHOWDOWN_REVEAL` so they must commit before knowing
   whether they won, and the evidence is **preserved** — the unanimous transcript
   plus every peer's signed `STATE_HASH` — in a form sufficient for a human or a
   future adjudicator to diagnose the divergence. **No adjudication procedure is
   specified, live or offline.** An earlier revision said the evidence was
   "deterministically adjudicable offline"; that is withdrawn, because it needs a
   canonical reference engine that is named, versioned against `protocol_version`
   and agreed authoritative, and no document in this corpus owns one. Whether to
   define one is an open decision in `docs/DECISIONS.md`. **OQ-D.**
6. **Collusion, endpoint compromise, screen sharing, Sybil identities, coercion,
   traffic analysis and DoS are outside the protocol's reach** (§6). This is not a
   temporary gap.
7. **Reputation has no teeth.** Identity is a locally generated keypair; the
   penalty for repeated aborts is visibility, nothing more.
8. **Both players behind a symmetric NAT, with no relay of raised limits
   available, cannot play each other.** Address prediction fails, so mutual dialing
   and DCUtR both fail; they still see the lobby (D-004 layers 0 and 3) but cannot
   start a hand. Real, and not hidden.
9. **Discovery is IPv4-only.** `mainline`'s KRPC socket calls `unimplemented!()` on
   IPv6, matching the fact that Mainline is an IPv4 network. The libp2p layer can
   be dual-stack; discovery cannot.
10. **A liveness dependency on relays for CGNAT-only clients.** If every reachable
    relay is down, such a client cannot connect at all. The failure must be reported
    honestly in the UI — "no direct route and no relay available" — never disguised
    (D-001).
11. **BLAKE3 has no public third-party audit** that could be found
    (`research/CRYPTO_LIBS.md` §3.2). Its assurance rests on its specification and
    lineage.

### 9.2 Open questions

| # | Question | Blocks | Owner / where resolved |
|---|---|---|---|
| **OQ1** | Is `ziffle`'s Bayer–Groth implementation *sound*? 1779 lines, unaudited, one author. A line-by-line review of `MultiExpArg` and `SingleValueProductArg` against the paper is a **prerequisite**, not a nice-to-have. If it fails, fall back to `barnett-smart-card-protocol` with its three repos vendored. | Assumptions A3/A4; §5 rows 1–4; goals G3/G4 | `docs/CRYPTOGRAPHY.md`; Phase 5 |
| **OQ2** | Is the forked Fiat–Shamir transcript safe? Same review, plus a deliberate attempt to produce a proof valid under one sub-argument's challenges and invalid under the other's. | A4 | same as OQ1 |
| **OQ3** | Is the `ctx` binding *sufficient*? The construction itself is settled and normative in `PROTOCOL.md` §4.5 — `(protocol_version, table_id, session_id, hand_id, sequence, shuffle_round, sender_public_key)` under the length-prefixed `derive_key` hasher of `PROTOCOL.md` §2.8 — and A11 states it. What remains open is whether that field set closes every replay avenue, and the mandatory regression test replaying a valid shuffle proof from hand `h` into hand `h+1` and asserting rejection. Our bug to make, not the library's. | A11; §5 row 13, X5 | `docs/PROTOCOL.md` §4.5; Phase 5 |
| **OQ4** | Are remote timing side channels in scope? Defensible to exclude for play money; **not** defensible for real money. Settled by `dudect`-style analysis of `shuffle_deck` and `reveal_token`, or by an explicit decision. | X23 | this document, revised |
| **OQ5** | Fuzzing the cryptographic deserialisers (§27), including `ziffle`'s `Transcript` `assert!` panic path. Until this lands, §17 row 17 carries a live remote-panic risk. | §5 row 17 | Phase 6 |
| **OQ6** | Mucking policy at showdown. Three options — mandatory universal reveal, TDA-faithful mucking with binding forfeiture, or delayed reveal at end of tournament — and each changes the cryptographic protocol, not just the engine. Option 3 reopens §19 because escrowed shares must survive a disconnect. Affects G8 and X12. | G8; X12 | `docs/DECISIONS.md`, then `docs/PROTOCOL.md` |
| **OQ7** | Relay admission policy: by `identify` protocol name (lets any stranger running our client in, which is the point) or by lobby presence (stricter, keeps a brand-new client out). Both `reservation_rate_limiters` and `circuit_src_rate_limiters` must be gated either way. | X21 | `docs/NETWORK_STACK.md` (D-002 open question) |
| **OQ8** | Is there any defence against a forged timeout certificate signed by all the other dealt-in seats (X10) that does not introduce a trusted clock? None is currently known. Where `\|V\| < 2` — heads-up always — the question is subsumed by OQ-A, and the *manufactured* `\|V\| = 1` is closed by D-008 (X30) rather than left to this question. | X10 | `docs/PROTOCOL.md` |
| **OQ9** | Is a visible abort record a meaningful sanction when identity is free? If not, say so plainly in the UI rather than implying a reputation system exists. | §6, §9.1.2 limitation 7 | `docs/PROTOCOL.md` |
| **OQ10** | Is it acceptable for a client to spend its own bandwidth relaying strangers' games (D-002)? Must be a visible, consenting setting, never silently on. | X21 | project owner (D-001 addendum) |
| **OQ11** | Should the epoch-rotating `LOBBY_INFOHASH` of §8.5 be adopted, at the cost of cross-version compatibility? The spec currently mandates one fixed constant, so this needs a numbered decision. | §8 | `docs/DECISIONS.md` |
| **OQ12** | What is the real per-hand byte count over **one relayed circuit, counting both directions together**, measured against the `Limit` a real relay returns? **The measurement must be bidirectional**, because `max_circuit_bytes` is one budget for the whole circuit (§3.5, verified in `libp2p-relay 0.21.1`); measuring one direction reports twice the headroom there is. Estimated at `2 × 8 979 = 17 958 B` per circuit per hand of shuffle traffic — each of the pair sends its own step and proof once, independent of `n` — and on the order of 20–26 KB per circuit per hand including the signed event stream, against a 131 072 B public-relay budget: roughly 5–7 hands, which is still not what binds. **The binding limit is the 120 s `max_circuit_duration`, not the byte cap.** A relayed peer's *total* per-hand outbound at an `n`-seat table is `(n-1) × 8 979 B` (44 895 B at six seats), but that is a bandwidth figure spread over `n-1` separate circuit budgets and is never compared to a single cap. | X20 | Phase 5 measurement, then Phase 8 |
| **OQ13** | Open-source licence. `zshuffle` was rejected partly on GPL-3.0-only; the recommended set is permissive. Needed before publication. | — | project owner (already open in `DECISIONS.md`) |

**Open questions opened or restated by the Phase 0 review.** These are lettered
rather than numbered because they cut across documents and each needs a numbered
owner decision, not a technical answer from an editor. Their wording is canonical
and is carried identically in the documents named.

| # | Question | Blocks | Owner / where resolved |
|---|---|---|---|
| **OQ-A** | At two seats a hand that cannot complete must end, and there is no way for the two peers to agree which of them failed. Ending it with restoration lets a player escape a losing pot by going silent; ending it with forfeiture lets a player take an honest opponent's committed chips by asserting a deadline that did not pass. This plan takes restoration, because a liveness/fairness loss is preferable to a theft, but the choice reverses D-005 at `n = 2` and needs a numbered owner decision. | X7, X8, X10; §7.3(c), §9.1.2 limitations 3–4 | Still open and **not yet in `docs/DECISIONS.md`'s open list**. The number `D-008` was taken by a different decision — the `\|V\|` scoping — which does **not** answer this question. Also `PROTOCOL.md` §12 |
| **OQ-B** | A dispute path that does not require the accused peer's signature. Already recorded in `DECISIONS.md` (D-007 point 4); it is repeated here because `PROTOCOL.md` §6.3 case (b) currently depends on exactly that circularity and the replacement is not designed. | X7; §7.3(c) | `docs/DECISIONS.md` open list; `PROTOCOL.md` §12 |
| **OQ-D** | On `HAND_ABORT cause = 4` (unresolvable state divergence) the chips are restored to their start-of-hand values because no peer can be attributed. This gives any single peer a free escape from a losing pot at the price of the table. The alternatives — forfeiting an unnamed party's commitment, or settling from the last `STATE_ACK`-agreed checkpoint — each need a numbered decision and neither is adopted here. | X8, X22, X29; §7.4; §9.1.2 limitation 5 | `PROTOCOL.md` §12, `STATE_MACHINE.md` §11, `docs/DECISIONS.md` |
| **OQ-E** | **Blocking.** How is a timeout certificate constructed when two or more seats are simultaneously unresponsive, and whom does it attribute? `signers == participants \ {subject}` is unachievable, and attributing every non-voting seat punishes honest seats behind a partition. The interim behaviour is to abort with no attribution and no chip movement, which lets two colluding seats void a hand for free. Blocking for Phase 4. | X7 (DNA case); §7.3(c) | `PROTOCOL.md` Q-02, `STATE_MACHINE.md` Q3 |

**D-008 generalises the scope of OQ-A and OQ-E, and their wording has deliberately
not been changed here.** Both are written on "two seats" and on "simultaneously
unresponsive" seats because both predate D-008. Read them on `|V|`: OQ-A's
question arises wherever the required voter set has fewer than two members — at
two seats always, and above two seats only after completed certificates have
legitimately attributed the other seats — and OQ-E's arises wherever no
certificate can complete because each required voter set contains another subject.
The wording is left byte-identical with `PROTOCOL.md` §12 and
`STATE_MACHINE.md` §11 as the lettered questions require; whoever restates it must
restate it in every document at once, and this paragraph records why it has not
been done unilaterally here.

**OQ-C** is recorded in `PROTOCOL.md` §12 only, because it is a message-design
question rather than a risk this document carries: should a required voter publish
a signed `ACTION_SEEN { sequence, event_hash }` before it may vote, so that
vote-and-seen are two events by one key in one slot and a lying voter becomes
provable? It is **not adopted**, and X10's honest statement of the race stands
without it.

### 9.3 The standing caution

This document classifies **17 of 48** catalogued attacks as cryptographically
prevented. That number is meaningful only alongside four qualifications, and it
must never be quoted without them:

1. Four of those seventeen — the deck-integrity rows — rest on **A3 and A4, which
   are not verified**. OQ1 is the gate.
2. Thirteen attacks are **out of scope entirely**, and they include the ones most
   likely to be used against a real game: collusion, endpoint compromise, and
   denial of service.
3. Three are **detected but not attributable**, and one more (X7) is attributable
   in only one of its three cases. Detection without attribution stops a hand; it
   does not name anybody, and under this design it does not move chips either.
4. "Prevented" always means *under the stated assumptions*, never *impossible*.

The count fell from 18 to 17 in the Phase 0 review: X11 claimed the race between a
late action and a timeout certificate was structurally excluded, and that claim was
false. A CP row removed because its argument did not hold is the review working as
intended, and the number is recorded as having moved rather than quietly restated.

`SPEC_CS.md`'s closing instruction is binding on every future revision of this
file: never claim the system makes all cheating impossible; prove precisely which
classes it prevents, which it merely detects, and which lie beyond the protocol's
reach.

---

## Objections to the fix plan

Recorded under the Phase 0 editing rule: a ruling of
`docs/research/PHASE0_FIXPLAN.md` was applied as written, and the disagreement was
written down here rather than resolved differently in this document. One objection
was raised, and it has since been **upheld and closed**.

**Objection 1 — A-6's "publicly adjudicable offline" claim has no referent —
UPHELD, and resolved in the text. Withdrawn as an objection.**

*The objection, as it stood.* The A-6 ruling required X29 (and `PROTOCOL.md` §6.3
case (c)) to state that the evidence of a state divergence — the unanimous
transcript plus every peer's signed `STATE_HASH` — is *"deterministically
adjudicable offline by any third party running the reference engine over it"*.
That sentence was written into X29 as instructed, under the objection that it rests
on the very thing A-3 and A-6 spent their reasoning demolishing. Both rulings refuse
live attribution on the ground that **there is no observer-independent derivation**:
every peer derives with its own engine, so "attribute whoever disagrees with the
derivation" is a vote over the facts and `SPEC_CS.md` §15 forbids it. Moving the
same derivation offline does not by itself produce the missing observer-independent
reference; it produces one only if a **canonical reference engine** exists, is
identified, is versioned against `protocol_version`, and is agreed authoritative.
No document in this corpus owns such an engine. `STATE_MACHINE.md` specifies *the*
deterministic engine, but as a specification of what every client implements, not
as a normative artefact a third party can run to settle a dispute; and D-4 defers
the source tree to `docs/ARCHITECTURE.md` in Phase 2, so there is not even a named
binary.

*The disposition.* `PHASE1_VERIFY.md` records the objection as **well founded**
(finding **N2**, five occurrences across three documents, two of them softening DNA
classifications) and points at disposition 2. It is applied here, in both places
this document carried the claim:

* **X29** now says the evidence is *preserved in a form sufficient for a human, or
  for a future adjudicator, to diagnose the divergence — no adjudication procedure
  is specified*, records that the earlier sentence is withdrawn, and states that the
  third bound is evidence preservation rather than recourse.
* **§9.1.2 limitation 5** carries the same weakening, and the same explicit
  withdrawal.

Nothing was strengthened, and the DNA classification of X29 no longer rests on a
mechanism that does not exist. Whether a canonical reference engine is defined —
named, versioned against `protocol_version`, and agreed authoritative — is now an
open decision in `docs/DECISIONS.md`'s open list; if it is defined, X29 and
limitation 5 may point at the definition and the stronger sentence becomes true
again. Until then, an offline-adjudication claim must not be quoted from this
document as a mitigation, here or anywhere else in the corpus.
