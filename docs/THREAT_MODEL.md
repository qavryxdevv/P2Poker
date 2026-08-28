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

**Binding owner decisions.** `docs/DECISIONS.md` D-001 to D-013 outrank both the
research notes and any judgement in this document. Where a decision creates a
threat or an open question, it is recorded here as such rather than argued with.
**D-007 corrects D-006** and wins over it: at two seats an action deadline is
advisory, no signed state transition follows from it, and no claim in this
document may present the certificate as protecting a two-seat table. Two things
about D-007's own wording are superseded and must not be reproduced as live
rules: it is scoped on the seat count, which D-008 replaces below, and it says a
certificate is *forbidden* or *invalid*, where the settled disposition is that a
below-floor certificate is **silently ignored and is not an error**
(`PROTOCOL.md` §8.3, D-009 rule 2).
**D-008 generalises D-007** and wins over both: every rule that weakens, disables
or gates the timeout certificate is scoped on `|V|`, the size of the required
voter set — **never on `n`, the seat count**. A certificate whose required voter
set has fewer than two members has no effect: it is not an error and not
evidence, the deadline simply stays advisory. A seat leaves `V` only once a
**completed, valid** certificate names it; being voted against is not exclusion.
Any rule in this document still scoped on `n` would be a defect, and the attack
that scoping on `n` failed to stop is catalogued in its own right as **X30**.

**D-009 reinforces D-008** with three rules that bind every claim below.

1. **No sequence of actions the protocol *requires* of an honest peer may
   produce a valid `EquivocationProof` against that peer.** The rule's normative
   home is `PROTOCOL.md` §5.2 and this document does not restate the predicate
   or the slot key in its own words; it classifies what happens when the
   property fails. The attack it closes is catalogued as **X31**, and when it
   was found it was the most damaging one any review pass had produced, because
   under D-005 it ended with an honest player's chips forfeited and their key
   blocked. **D-010 removed the chip consequence and D-011 rule 3 removed the
   block-listing**, so X31's worst case is now a wasted hand. The rule is
   unaffected by that and is not softened: a specification under which an honest
   peer manufactures evidence against itself is defective whether or not
   anything currently acts on the evidence. It has now been violated **five**
   times — lobby and join traffic, `DISPUTE`, `TIMEOUT_VOTE`, `STATE_HASH`
   re-emission on the happy path of divergence recovery, and the terminal
   `HAND_ABORT` (`research/PHASE2_GATE.md` G1, catalogued here as **X32**). The
   first four are closed. The fifth is closed by **D-011 rule 2**, which stops
   restating the rule as prose and fixes the slot key as one literal tuple in
   one place.
2. **A certificate below the `|V| >= 2` floor is inert in every document and at
   every table size**: not chained, not evidence, no terminating effect, no
   `AbortRecord`, no chip movement. It is silently ignored. This holds for
   `kind = Crypto` and for the hand deadline as much as for anything else.
   Liveness is not owed — §19 of the spec ranks security above finishing a hand.
   **What this rule buys changed under D-010 and the claim is weakened
   accordingly.** It used to be the thing that kept D-005's closure of the
   rage-quit escape from being reopened by one signature. D-010 reopens that
   escape deliberately and by decision, so inertness no longer prevents the
   escape; it only makes the escape cost `hand_deadline_ms` of visible stalling
   instead of one message, and keeps a below-floor certificate from becoming a
   chained event that names its own victim (§7.3(c), X8).
3. **No security property may be stated as an absence from the dependency
   tree.** State the discipline our own code follows and enforce it
   mechanically. A7 below is written under this rule.

**D-010 outranks all of the above, and it is the reason many classifications in
this document changed.** For the MVP an abort is **neutral**:

1. **Stacks are restored to their start-of-hand values.** No chips move on an
   abort, in any direction, for any cause.
2. **Attribution is evidence with no automatic consequence.** The transcript
   still records which peer failed to publish, signed and verifiable; nothing
   acts on it automatically.
3. **No automated eviction.** No peer is block-listed, unseated or penalised by
   the protocol on the strength of a proof.
4. Equivocation proofs and timeout certificates may still be *produced* — they
   are how a human or a later version adjudicates — but **consuming one never
   moves a chip or removes a player in this version**.

**D-011 outranks D-010 and it is what this revision was rewritten against.** It
has three rules and all three bind this document:

1. **One normative owner per concept, and a document restates nothing another
   owns.** `PROTOCOL.md` owns the wire — message shapes, the event envelope, the
   chain, sequence numbers, the anti-replay slot, canonical bytes, and what a
   receiver validates. `STATE_MACHINE.md` owns state and transitions.
   `CRYPTOGRAPHY.md` owns the constructions. `NETWORK_STACK.md` owns transport,
   discovery and connectivity. **This document owns classifications and restates
   nothing.** Where a definition is needed here it is *referenced* by section
   number; where two documents disagreed, the owner won. Every restatement this
   document carried has been deleted and replaced by a pointer, and §9.1.3
   counts them and records what that costs the reader.
2. **The anti-replay slot key is one literal tuple, written once, in
   `PROTOCOL.md` §5.2, and it includes `event_type`.** Every other document
   points at it. This is D-009 rule 1 stopped being prose. No tuple appears
   anywhere in this file any more.
3. **D-010 point 3 binds every layer, transport included.** No `block_peer`, no
   unseating, no allow/block list driven by a protocol proof, in any document.
   `NETWORK_STACK.md` block-listed a peer on an `EquivocationProof` in two
   places and `STATE_MACHINE.md` unseated a seat on a self-contained proof in
   one; all three are deleted. Until they were, this document's claim that a
   proof "block-lists nobody" was true only of this document.

**D-012 outranks D-011, and it adds a row rather than moving one.** Its rule:
**no canonical state — nothing entering a state hash, a roster hash, a chained
event body or the next hand's genesis — may be derived from a quantity that can
differ between honest receivers.** Canonical state changes only through a chained
event every participant accepted; everything else is a local view. What it
catalogues here is **X33**: an abort's `attributed` field is per-receiver, a
transition elsewhere read it into the next hand's roster, and two honest peers
therefore forked the table permanently. Three things about it are worth carrying
into the reading of §5. It needs **no attacker** — silence is enough, and silence
is free under X7 — so it is classified **DNA** for a reason no other row has:
there is nobody to name. It was created by the fix that unfroze the stalled hand,
which is a **third defect shape** to set beside D-011's two, and the one that
tells a reviewer where to look next: *the path the previous fix newly made
load-bearing*. **The price D-012 believed it was paying was stated wrongly, and
the correction is D-013's** — see below and X34; the sentence that stood here,
*"nothing marks a seat absent automatically any more, so a silent seat is dealt
in every hand and stalls each one to the deadline until a human acts"*, was
false in all three of its parts and is not to be reproduced. D-012 also carries
a process rule this document is bound by: **every decision sweep covers every
document**, because twice a document scoped out by judgement became a blocking
defect. This document was the third such omission: it stood at **one** occurrence
of `D-013` while carrying that decision's superseded cost model in two places
and its superseded reconnect rule in a third.

**D-013 outranks D-012, and it corrects D-012 rather than extending it.** Its
rule: **liveness is inherited from the chain, not from a seat's status** — a
seat is required to emit in hand `k+1` only if it signed at least one chained
event during hand `k`, and for the first hand the required set is the signers of
`TABLE_READY`. `PROTOCOL.md` §3.2 owns the set and its notation; this document
does not restate either. Four consequences bind the classifications below.

1. **D-012's cost model was not merely optimistic, it was the wrong shape**, and
   the corrected version is catalogued in its own right as **X34**. Marking a
   seat absent never removed it from the required emitter set, so the liveness
   D-012 believed it was spending had been gone for every previous pass; and the
   stall was not a slow table but a **fixed point** — every stack is restored on
   abort, so no seat busts, so no end condition can fire. Under D-013 a silent
   seat stalls exactly one hand and is then skipped, it is blinded off, it
   genuinely busts, and the tournament can end.
2. **No human acts, and none can.** Wherever this document said a seat's stall
   ends "until a human sits it out or leaves", the grammatical subject of that
   escape was the *silent* seat, and `PLAYER_SIT_OUT` and `PLAYER_LEAVE` are
   single-writer by the seat itself. There is no operator, no majority and no
   third party with a lever here, and no revision of this file may imply one.
3. **Nothing rejoins automatically.** A seat returns only by signing a chained
   event. §7.3(b) carried the opposite and is corrected.
4. **D-013's own fix has made two paths load-bearing that were not**, which is
   D-012's third shape applied to D-013 itself. Both are catalogued: the genesis
   the required set is anchored to (**X35**) and the required set on the one
   path where it narrows (**X36**). X36 is the first row this document has ever
   had to place in the bucket §5.4 previously said no row occupied.

**D-014 outranks D-013, and it is the first decision in this series that gives a
payoff *back* rather than deleting one.** Its rule: **a player who sends a
provably illegal message is removed from the table, the attacked hand is voided,
play continues without them, and an information window names the anti-cheat.** It
narrows D-010 point 3, which banned automated peer removal wholesale after four
passes in which every severe defect ended with an honest peer's chips forfeited.
The narrowing turns on one distinction and this document is bound by it:

> **Evidence is self-authenticating when it is a message signed by the accused,
> whose illegality any peer can decide alone, from that message plus state the
> peers provably share.**

Four consequences bind the classifications below, and the fourth is a new risk
rather than a re-classification.

1. **Every row whose worst case was "the message is rejected" now has a second
   worst case, and it is the attacker's seat.** That is the whole of the
   improvement and it is real: the D&A bucket's name has meant *detected and
   named* since D-010, never *detected and answered*, and for the tier-1 subset
   it now means detected, named and **answered**. §5.1's D&A definition carries
   the qualification and §5.3's rows carry the split.
2. **The improvement stops exactly at D-010's boundary and the boundary is the
   evidence, not the severity.** No removal on a timeout, a missing publication,
   an `EquivocationProof`, an `attributed` field, a vote, a certificate, or any
   quorum. So **row 16 (equivocation), X7, X10, X30, X31 and X32 are untouched**
   — the six rows a reader would expect this decision to fix are the six it
   deliberately does not, because each of them turns on a judgement two honest
   peers can reach differently, which is what produced every defect D-010 closed.
3. **Tier 2 waits for a checkpoint.** Illegality decidable only against game
   state — an out-of-turn action, a raise below the minimum, a bet larger than
   the stack — is safe to act on **only** once the state it is judged against is
   fixed by a checkpoint both peers have signed. Before that point the violation
   is recorded and the hand is voided, and nobody is removed.
4. **The decision creates a risk of its own and it is catalogued as X37 rather
   than as a caveat inside another row.** *A validator that is too strict now
   ejects honest players rather than merely rejecting a message.* Every previous
   pass's worst defect landed on the path the previous fix newly made
   load-bearing; the path this fix makes load-bearing is **the correctness of
   every validator in the corpus**, which was never consensus-critical against a
   *person* before. And the corpus contains a live example of how an honest
   player's legal action comes to look illegal: **K-1**, two honest peers holding
   different `P` and therefore different `dealt_in`, in which each peer's
   perfectly legal `HAND_INIT` is wrong at the other. That is why tier 2 exists
   and why its checkpoint precondition is a precondition rather than advice.

Read the catalogue in §5 with all of that in front of you. **Two payoffs have
been deleted from the design, and every row that turned on either is
re-classified.** The first was chips: every row whose payoff was chips taken
from a peer — X9, X10's chip half, X30, X31 — now has the same worst case as a
flaky network connection, **a wasted hand** (D-010). The second was the
victim's network: every row whose payoff was a peer's key block-listed or a
seat unseated — X31, X32, and the transport half of row 16 — now has as its
worst case **a wasted hand and, at most, a connection the victim
re-establishes** (D-011 rule 3). §5.3 tabulates every row the two decisions
move, including the one they move the wrong way. That is the single largest
honest improvement this document has ever recorded, and it was obtained by
deleting machinery rather than by adding any. It is also not free, and the price
is carried as a catalogue row in its own right rather than as a footnote to a
decision: **X8**, the rage-quit escape, is back, and it is classified as
*visible but not prevented* (§5.1, §7.4). `SPEC_CS.md` §18 forbids describing it
as anything more than visible.

**The four properties D-007, D-008, D-009 and D-010 leave standing are stated in
one place, and only one: §9.1.0.** They were previously scattered across three
or four sites each and drifted apart there, which is the same failure D-011
rule 1 addresses between documents. Every other mention in this file elaborates
§9.1.0; where an elaboration appears to contradict it, §9.1.0 wins.

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
  service by default on every publicly reachable node — but their default
  circuit limits are sized to coordinate one DCUtR hole punch, **not to carry a
  session**, and the duration limit is what binds first. **[R24]** The values,
  the byte accounting and the kubo wording caveat are `NETWORK_STACK.md` §9.5
  and §16.1's; this document names the consequence and not the numbers, because
  it printed them twice and had them wrong in both places.
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

**Correction, twice made, and it weakens the assumption both times (D-009 rule
3).** An early revision of this document claimed the `rand` crate is deliberately
absent from the dependency tree, so that §7's prohibition on `SmallRng`/`StdRng`
was enforced structurally rather than by reviewer discipline. A later revision
narrowed that to "`SmallRng` is absent because rand 0.8's `small_rng` feature is
not enabled". **Both are false, and no restatement of an absence belongs here.**

What is true, verified in crate source rather than inferred:

* `rand` is in the runtime tree at **three majors — 0.8.8, 0.9.5 and 0.10.2**.
  `rand 0.8.8` arrives via `ark-std 0.5.0` (feature `std_rng`) with
  `rand_chacha 0.3.1` and `rand_core 0.6.4`; `rand 0.9.5` arrives via
  `hickory-proto`, `hickory-resolver`, `igd-next` and `yamux`, reached through
  libp2p's `dns`, `upnp` and `yamux` features (`DEPENDENCIES.md` §5.4, §7).
* `rand-0.9.5/Cargo.toml` lists
  `default = [ "std", "std_rng", "os_rng", "small_rng", "thread_rng" ]`, and
  `igd-next 0.16.2` and `yamux 0.13.10` both take `rand` **with defaults**.
  Features unify additively, so **`SmallRng` and `rand::rng()` are compiled into
  the integrated binary**, alongside `StdRng`, which `ziffle` uses for
  deterministic nothing-up-my-sleeve public constants.
* **This cannot be changed without dropping libp2p features we need.**
  `igd-next` arrives through `libp2p-upnp` and `yamux 0.13.10` is mandatory for
  the relay of D-001/D-002. Removing `SmallRng` from the build means removing
  those, which the connectivity design depends on.

`SPEC_CS.md` §7 is therefore **not** enforced by the dependency graph and no
document may claim it is. The enforced property is a **discipline over our own
code**: every draw goes through `security::rng::fill`, which wraps
`getrandom::SysRng`, and nothing in this crate names another generator. That is
mechanical rather than editorial —
`src/security/rng.rs`'s `tests::our_own_code_uses_no_generator_but_the_os_one`
scans this crate's own sources on every test run and **fails the build** on
`SmallRng`, `StdRng`, `thread_rng`, `from_seed`, `seed_from_u64` or
`rand::rngs`, with `src/security/rng.rs` itself the single exemption because it
must name the identifiers in order to forbid them. The gate was verified to bite
by injecting a violation and watching the test fail, then pass again once it was
removed (D-009 rule 3); a gate never seen to fail is not a gate. The CI lint of
`CRYPTOGRAPHY.md` §12 item 6 is the same rule at the repository level
(see `CRYPTOGRAPHY.md` §7.2 and its OQ-8).

**Re-checked this revision (M3 residue).** Every occurrence of `SmallRng`,
`StdRng`, `thread_rng` and `small_rng` in this document was re-read, and **no
absence claim about any of them survives here** in any form. The two sentences
quoted above are quoted in order to be withdrawn, which is the only form in
which such a claim may appear; every other occurrence asserts the opposite —
that the generators *are* linked in and cannot be removed — and §9.1.2
limitation 13 states it as a limitation rather than as a mitigation. The claim
that survived one document away, in `PROTOCOL.md` §4.4, has since been quoted
and withdrawn there too, and `research/CRYPTO_LIBS.md` carries the corrected
three-major table (`research/PHASE2_GATE.md` M3, RESOLVED). D-009 rule 3 is now
satisfied in every document that ever carried the claim, and no sentence in this
document rests on it either way.
*If false*, or if the source scan and the lint are removed: §7's prohibition is
unenforced and a future contributor can reach `StdRng` or `SmallRng` from our own
crates — both are already linked in and need only be named. And if the OS CSPRNG
itself is unsound, keys, permutations and RNG contributions become predictable;
every secrecy goal falls for the affected client.

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
chain starts, and is a pure function of state every peer already agrees on.**
**[R22]** Which state, exactly, is `PROTOCOL.md` §4.5's index-map construction,
and the field list this assumption used to name is deleted under D-011 rule 1 —
an assumption that prints its own version of a construction can be satisfied
against the printed version while the real one drifts.
This is our design obligation, not the library's. If the map were chosen after the
final deck existed, a malicious last shuffler could argue about which index is
"the button's first hole card" and thereby choose outcomes
(`research/MENTAL_POKER.md` §6).
*If false:* the last shuffler acquires a real, exploitable edge, and A9's
"nothing to grind toward" argument collapses.

**A11 — We assume the proof context `ctx` is exactly the construction
`PROTOCOL.md` §4.5 defines, with no field omitted and none added.**
**[R1]** The field list this assumption used to reproduce is deleted under D-011
rule 1; `PROTOCOL.md` §4.5 owns it, and reproducing it here was a second copy
that could drift from the one the receiver checks. Read the assumption as: the
`ctx` an honest client builds is byte-identical to §4.5's, and the list there is
a fixed set rather than a minimum.
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
signed **timeout certificate** (**D-006**), and the engine itself contains no
clocks. **[R2]** The certificate's fields, its emitter set and its stage shape
are `PROTOCOL.md` §4.8's and `STATE_MACHINE.md` §8.4's; the description this
assumption used to carry is deleted under D-011 rule 1, because a threat model
that restates a wire shape is a copy that drifts and then gets quoted back as
though it bound anybody. What this assumption asserts is only the negative: no
peer's unsupported word about the clock changes any state.

**The assumption does not hold when `|V| < 2`.** `V` is the required voter set —
every other dealt-in seat, minus any seat a **completed, valid** certificate has
already attributed (`PROTOCOL.md` §8.3, D-008). When `|V| = 1`, "unanimous"
reduces to one signer and the certificate carries no more weight than that one
peer's word. **D-007** makes the heads-up action deadline advisory, produces no
signed state transition from it, and forbids a fold-effect certificate at two
seats; **D-008** generalises the protection to the quantity that actually governs
it, so a certificate with `|V| < 2` has no effect **at any seat count**, and `V`
cannot be shrunk by assertion. At `n = 2`, `|V| = 1` always, which is why the
heads-up case is the permanent instance of this. See X10, X30 and §9.1. What an
unfinishable hand then does with the chips was carried here as a lettered open
question; it is **closed by D-010** — restoration on every path — and the letter
`OQ-A` now names a different question (§9.2).
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
  denial-of-service lever. Under D-005 the payoff for using it was the victim's
  forfeited commitment; **under D-010 there is no payoff at all**, because the
  hand aborts neutrally and the victim's chips return to their start-of-hand
  value. What is left is the ability to waste hands and to put the victim's name
  in the transcript as the peer that failed to publish — evidence with no
  automatic consequence (see X9 and X20 in §5). **D-011 rule 3 closes the one
  remaining escalation:** a relay that can reliably stall a victim used to be
  able to drive that victim's key onto a transport block list by way of the
  liveness terminus the stall provokes (X32). It cannot now, because no layer
  block-lists anybody.
* **can silently reset a session** by enforcing the default circuit limits, so a
  hand dies mid-street. **[R3]** The limits, their accounting, the per-hand byte
  arithmetic and the kubo-versus-`rust-libp2p` documentation discrepancy are
  `NETWORK_STACK.md` §9.5 and §16.1's, and the derivation this bullet used to
  reproduce is deleted under D-011 rule 1 — it was a second copy of a transport
  fact, and it had already been wrong once by a factor of two. What this document
  classifies is the consequence: the **duration** limit, not the byte cap, is
  what a public relay reaches first, and a circuit that dies mid-hand is *an
  engineered abort attack against ourselves*. The mitigation is also
  `NETWORK_STACK.md`'s — read the `Limit` returned with the reservation and
  refuse to seat a player whose only path cannot carry a hand, rather than
  starting one that will die.

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
If a player signs two conflicting **chained** events into one anti-replay slot,
the pair constitutes a self-authenticating, transferable proof of that player's
misbehaviour.

**[R4] The predicate and its slot key are `PROTOCOL.md` §5.2's, and this goal
does not reproduce either.** An earlier revision quoted the predicate in full,
listed the slot key as a six-field tuple, and enumerated the per-type
anti-replay rules for unchained traffic. All three are deleted under D-011
rule 1. The copy was not harmless: it was a **stale** copy — it omitted
`subject_seat`, which D-009 rule 1 had already added, and it omitted
`event_type`, which D-011 rule 2 adds — so a reader checking a message type
against the key as this document printed it would have certified X31 and X32 as
clean. That is the exact failure D-011 rule 1 exists to stop, observed in this
file. The key is **one literal tuple in `PROTOCOL.md` §5.2**, including
`event_type`; §5.3 owns the matching anti-replay index; this goal points at both
and says only what they buy.

**What the proof does, and no longer does (D-010, D-011 rule 3).** It is
produced, it is verifiable by anyone, and it is written to the transcript. It
does **not** move a chip, does **not** remove a player, and — since D-011 rule 3
carried D-010 point 3 down to transport — does **not** cause any layer to
disconnect, refuse or block-list the accused key. Consuming one ends the hand
neutrally at most. The goal is therefore about *evidence*, not about
*consequence*, and any reading of it as a deterrent must be qualified by §6's
observation that identity is free.

**What the goal now depends on, stated because deleting the copy changes it.**
While this document printed the predicate, a reader could check G7 against the
message set without leaving the file. They can no longer. G7 is true only if
`PROTOCOL.md` §5.2's key is what §5.2 says it is, and the check that it still
covers every message type is the standing mirror test of §5.5, not a paragraph
here. That is the cost of D-011 rule 1 and it is recorded rather than absorbed
(§9.1.3).

*Falsified by:* the `CheaterEquivocation` row of §5.5 — a divergence that no honest
peer can attribute, or two conflicting events that do not together prove
misbehaviour.
*Basis:* A5, plus the canonicality gate (X4) which removes the "two byte encodings
of one logical event" escape.
*Limit, stated here rather than in the justification column:* the proof exists as
soon as both events reach one honest party. It does **not** guarantee they do.
*Second limit — the false-positive class, and it is the one that keeps
recurring.* The predicate must never be applied to unchained traffic, and its
slot key must never be coarser than the behaviour the protocol requires. It has
been coarser **five** times:

| # | Message type | What honest conduct filled one slot twice | Closed by |
|---|---|---|---|
| 1 | lobby adverts, and `JOIN_REQUEST` beside the sender's own `RNG_REVEAL` | two successive honest adverts; a joiner doing both required things | the `chain_scope` discriminator — unchained traffic is outside the predicate |
| 2 | `DISPUTE` | an honest peer is *required* to emit it twice | `DISPUTE` made unchained |
| 3 | `TIMEOUT_VOTE` | two required votes about two simultaneous subjects (**X31**) | `subject_seat` in the key (D-009 rule 1) |
| 4 | `STATE_HASH` | the reconciling peer must re-sign different content for a checkpoint it already signed | a reconciliation round given its own stage and `sequence` (`PROTOCOL.md` §4.9) |
| 5 | the terminal `HAND_ABORT` | every peer that already contributed to the stalled stage must emit the abort *at that stage's own index* (**X32**) | **D-011 rule 2** — `event_type` in the key, and the key written once |

Each recurrence was more reachable than the last: #3 needed a Sybil pair, #4 one
dropped stream, #5 nothing at all but an opponent going quiet on the shipped
heads-up configuration. What none of them costs any more is chips or a
connection — D-010 deleted the forfeiture and D-011 rule 3 deleted the
block-listing that #3, #4 and #5 all terminated in.

**[R5] D-009 rule 1's normative text is `PROTOCOL.md` §5.2's and is no longer
quoted here.** The block quote this paragraph carried is deleted under D-011
rule 1, and the deletion is the point rather than tidying: **the rule was prose,
and prose was re-derived differently by each editor five times.** D-011 rule 2's
answer is not a sixth restatement — it is to stop restating. The key becomes one
literal tuple in one place, including `event_type`, and every document points at
it. **This is the sixth attempt at one property**, counting the five closures
above; the difference this time is that the fix is a definition with a single
site rather than a principle each editor re-applies. If a seventh instance
appears, D-011's own closing rule applies: cut the mechanism out of the MVP
rather than write a seventh rule.

A specification-level false-positive source is worse than a user-level one, and
every future message type is to be checked against the rule **before** it is
added, with the check standing as a test rather than a paragraph: `SPEC_CS.md`
§25 already requires a `CheaterEquivocation` peer, and the mirror of it — an
*honest* peer that never generates a proof against itself under any legal
interleaving — is the one that catches this class (§5.5).
`NETWORK_STACK.md` §7.4's founder-contradiction rule for two adverts with the
same `timestamp_unix_ms` is a local lobby-hygiene rule and is **not** an
`EquivocationProof` in this sense.

**G8 — Independent verifiability.**
Every honest client can decide accept/reject on every public state transition and
every proof using only the transcript and public parameters, with no appeal to any
other party's word, and with no "the host is right" tie-break (`SPEC_CS.md` §15).
*Falsified by:* any protocol step whose correctness a client must take on trust.

**G9 — Chip conservation, as a ledger identity.**
Chips are conserved against a ledger of entries and exits, not against a constant.
**[R6]** The identity itself is `STATE_MACHINE.md` §10 invariant I1, and the copy
this goal used to carry — the summation, the two ledger terms, the tournament
corollary and the cash-mode note — is deleted under D-011 rule 1. Invariants are
`STATE_MACHINE.md`'s to state; a second copy here could only ever agree with it
or silently diverge from it, and this goal needs neither. The within-hand
identity `sum(pots) + sum(returned_uncalled) = sum(committed_this_hand)` is
likewise `STATE_MACHINE.md` §10's, from `research/POKER_RULES.md` §A0 invariants
1 and 2. What this goal claims is only that **it must hold on every path,
including every abort path**, and that a path on which it does not is a defect
this document classifies rather than a rule it writes.

**D-010 makes the abort paths the easy ones rather than the delicate ones.**
Every abort is now a restoration: `committed_hand[s]` returns to `stack[s]` for
every seat `s`, no chips move between seats, and this is true for every `cause`,
for every value of `attributed`, and at every table size. The per-branch
forfeiture arithmetic that previously had to be property-tested separately —
`culprits`, `others`, the proportional share and the clockwise remainder loop —
is **deleted from the MVP**, and with it the class of bug where one branch of it
conserved chips and another did not. The property test that replaces it is
stronger and simpler: **after an abort, every stack is bit-identical to its
start-of-hand value.**
*Falsified by:* a property test that finds any transition changing the identity,
or any abort path on which a stack differs from its start-of-hand value.

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
| **D&A** — detected and attributed | The adversary *can* emit the message. Every honest client rejects it, the state does not advance, and the misbehaviour is bound to a specific application key by a signature, so the evidence is transferable to third parties. **Under D-010 the second half of the name is narrower than it sounds and must be read narrowly:** attribution puts a signed name in the transcript and nothing follows from it automatically — no forfeiture, no block list, no unseating. D&A means *detected and named*, never *detected and answered*. **D-011 rule 3 is what makes the "no block list" half true at every layer rather than only in this document**: until it landed, `NETWORK_STACK.md` still called `block_peer` on an `EquivocationProof` in two places and `STATE_MACHINE.md` still unseated a seat on a self-contained proof in one, so a row classified D&A here could still cost the *accused* — honest or not — its connections. That gap is closed, and the honest reading of every D&A row is now uniform: rejection, a name in the transcript, and nothing else. **D-014 splits that uniformity in two and the split is by evidence, not by severity.** Where the rejected message is **tier-1 self-authenticating** — a signature that does not verify, a non-canonical encoding, a malformed message or out-of-range field, a failed shuffle / decryption-share / key-ownership proof, a deck that gains or loses a card, a signer who is not a party to the table — the sender **loses its seat**: the hand is voided neutrally, the seat is dead and blinded off, and the exit is one-way (`STATE_MACHINE.md` T64, T65, I34). Where the rejected message is **tier-2 state-dependent**, the same is true **only** once the state it was judged against is fixed by a checkpoint both peers signed; before that, D-010's reading stands unchanged — rejection, a name, and nothing else. Where the "evidence" is a timeout, a certificate, a vote, an `attributed` field or an `EquivocationProof`, D-010's reading stands **permanently**, and a row that reads a removal into any of those has rebuilt the forfeiture D-010 deleted. A reader classifying a new row must therefore answer one question before reaching for D&A's stronger half: *can two honest receivers of this evidence disagree about the verdict?* If yes, there is no removal, whatever the attack costs. **The sharp form of that question, and the general test a proposed tier-1 addition must pass, is §5.1.1.** |
| **DNA** — detected but not attributable | The divergence or conflict is detected, and play stops, but the transcript does not establish *who* was at fault. **Two different situations share this bucket, and X33 was the first of the second kind:** either an adversary is present and the design cannot name them (X10, X22, X29), or **there is no adversary at all** and the design has produced a divergence between honest peers (X33, X34, X35). Both are "detected, nobody named"; only the first is somebody escaping a name. The second kind is now three of the six DNA rows, which is the shape of the last three review passes and not a coincidence: the defects this corpus produces are no longer attacks. |
| **NP&ND** — not prevented and not detected | The divergence happens, nothing rejects it, no invariant fires, no deadline expires, no proof forms, and **no peer ever learns that it happened**. Each peer's own view stays internally consistent and self-verifying, so there is no moment at which anything could be reported to a human. This is the **worst bucket in the scheme** — worse than V, which at least leaves a record somebody can read, and worse than OOS, which makes no claim rather than a false one. §5.4 said for four passes that no row occupied it. **X36 occupies it**, and the honest consequence is that this document may no longer offer "every divergence is at least detected" as a property of the design. A row leaves this bucket only by a fix that makes the divergence observable, not by a fix that makes it rarer. |
| **V** — visible, not prevented | The attack **succeeds**. Nothing rejects it, nothing in the protocol acts against the attacker, and no proof changes the outcome. What the design delivers is a signed, permanent record that it happened, and a per-identity count of it in the lobby. This is the weakest non-OOS bucket in the scheme, and `SPEC_CS.md` §18 forbids describing a row in it as anything more than visible. It is distinguished from OOS only in that the attack runs *through the poker protocol* and the protocol therefore sees and records it. |
| **OOS** — out of scope | The protocol does not and cannot address it. Mitigations may exist and are named, but no security claim is made. |

**The NP&ND bucket is new in this revision and it exists because of X36**, on
the same discipline that produced V: there was no honest label for a divergence
that nothing detects among the five buckets that existed, and stretching DNA to
cover it would have been the error this document is most concerned with, since
DNA's whole content is that *something* stopped. Adding the bucket costs one
line in a table; stretching DNA would have cost the meaning of four other rows.
Note what this does to the reading of §5.4: the CP column did not move, the
denominator rose, and the design acquired a failure mode nobody can see.

**The V bucket is new in the previous revision and it exists because of D-010.** Before
D-010 the rage-quit escape was closed by automated forfeiture and X8 was a D&A
row; D-010 removes the forfeiture, the escape returns, and there is no honest
label for it among the four buckets that existed — it is neither prevented, nor
rejected-and-attributed, nor undetected, nor beyond the protocol's reach. Adding
a bucket rather than stretching one is the same discipline §5.3's split cell
follows: a label that overstates is the error this document is most concerned
with.

One row (X7) carries **two classes**, because its class depends on how many seats
are silent at once and on `|V|` — never on the seat count `n`, which is the
scoping D-008 point 4 makes a defect wherever it survives. A split cell is honest
where a single label would
overstate one of the cases; it is written as `D&A / DNA` and the cases are
enumerated in the justification. Splitting is not a way to avoid a verdict, and no
row may split between CP and anything else — CP is a claim about what an adversary
can construct, and it either holds or it does not.

**Every CP row inherits A1–A7. In particular, every CP row that concerns deck
integrity (rows 1–4) also inherits A3 and A4, which are NOT verified.** If the
review named in A3 finds a soundness gap, rows 1–4 fall out of CP entirely.

#### 5.1.1 The admissibility test for tier 1, and the clause that failed it

D-014's tier 1 is the only evidence in this design that costs a person their
seat with no checkpoint, no quorum and no waiting. What makes that safe is a
property of the *evidence*, not of the attack, and the property has to be checked
clause by clause. **This is the general test, and it is stated here because this
document owns the classification scheme (D-011 rule 1); every other document
points at it rather than restating it.**

> **A clause belongs in tier 1 only if its verdict is a function of the offending
> message's own bytes and inputs the peers provably share — and of nothing else.**
> Concretely, three questions, all of which must be answered *no*:
>
> 1. **Does deciding it require reading anything the receiver stores?** Its own
>    chain, its own head, its own set of seen events, its own buffer, its own
>    clock, its own connection table.
> 2. **Can the answer change for one honest receiver because of what the network
>    did** — a frame dropped, delayed, reordered, or delivered to somebody else
>    first?
> 3. **Does it need a second message, a count, a vote, a certificate or another
>    peer's cooperation to become decidable?**
>
> Any *yes* puts the clause in tier 2 at best, where a checkpoint both peers
> signed fixes the state it is judged against — or out of the removal path
> altogether.

**The clause that failed it, and it stood in tier 1 in three documents: *the
event chains to a parent that does not exist*.** It fails question 1 outright and
question 2 with it. Whether a parent exists is decidable **only against the
receiver's own store**: an honest peer that has not yet received the parent — one
dropped frame, one reordered delivery, one slow relay — computes *no such parent*
where every other peer computes *present*, and it computes it about a message the
accused signed perfectly legally. **One dropped frame would remove an honest
player.** That is the exact failure the two tiers exist to prevent, and it was
sitting in the tier meant to be safe, where it would have fired without a
checkpoint and without any chance to wait for one.

It is deleted here (§5.1's D&A cell, §5.2's re-classification table),
in `PROTOCOL.md` (§4.0's normative box, §4.9's acceptance gate) and in
`DECISIONS.md` D-014's own list. It was deleted from the code first:
`src/security/validation.rs` no longer has `SelfContained::ParentUnknown`, and
its `no_tier_one_violation_depends_on_the_receivers_store` test asserts the
variant count so that adding one forces the author past the comment that says
why. **A missing parent is still a reason to buffer or reject the event, which is
all it ever was** — it is not evidence against anybody.

**Why this matters more than one deleted bullet.** Every tier-1 clause that
survives is answered by the *cryptography* or the *encoder* — a signature, a
canonical encoding, a range, a proof, a permutation, a roster membership — and
none of them reads a store. The parent check looked like that family because it
is cheap and local; it is not in that family, because *local* and
*receiver-independent* are different properties and only the second one is the
one tier 1 needs. That confusion is the general shape a future addition will
arrive in, which is why the test above is three questions rather than an appeal
to judgement.

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
| 16 | Different histories to different players (equivocation) | **D&A** | Not preventable: a modified client can sign two conflicting events. What the design delivers instead is that the pair is **self-authenticating evidence** — two chained events by one key in one anti-replay slot with different `event_hash` values. **[R7]** The slot key is not reproduced here; it is one literal tuple in `PROTOCOL.md` §5.2 and this row points at it (D-011 rules 1 and 2). The earlier revision of this cell printed a five-field version of it, which was already stale when it was written. That is evidence of misbehaviour **only** while no honest peer can be made to fill one slot twice by following the rules, which is a property of the message set rather than of the signature scheme, is not implied by A1–A7, and has failed **five** times already (X31, X32, G7's table, §9.1.2 limitation 12). Under **D-010** what a proof buys is smaller than it was, and under **D-011 rule 3** smaller again: the pair is evidence in the transcript, consuming it moves no chips, unseats nobody, and — the half that only became true with D-011 — causes no layer to block-list the accused key. So the worst case at the end of this row, for the equivocator and for a peer falsely accused alike, is **a wasted hand**; where the accused's transport was dropped for other reasons it is a wasted hand and a connection they re-establish. Detection is fast in practice because all `n` peers at a table are mutually connected and exchange `STATE_HASH` after critical transitions, and because an equivocator cannot carry two divergent hands to showdown: opening any card needs every player's share, so both branches stall. The honest limit: the evidence only exists once both halves reach one honest party, and a partition can delay that. |
| 17 | Malformed packets | **D&A**, with a residual risk | The event decoder is bounded by construction: `minicbor` validates a claimed length against the remaining input *before* allocating — measured at **0 bytes allocated** for a byte string claiming 4 GiB, for one claiming `u64::MAX`, and for an array claiming 4 GiB of elements, and 20 000 levels of nesting produced an error rather than a stack overflow (`research/CRYPTO_LIBS.md` §4.8). No `eval`, no `pickle`, explicit schema validation. **Residual risk, stated rather than hidden:** the *cryptographic* deserialisers (arkworks / `ziffle`) have **not** been fuzzed, and `ziffle`'s `Transcript::update_with_serialized` contains an `assert!` panic path if a serialised element exceeds a 256-byte buffer. Unreachable for 33-byte points, but it is a panic on network-derived data. Until `SPEC_CS.md` §27 fuzzing lands over `ShuffleProof`, `MaskedDeck`, `RevealToken` and `OwnershipProof`, a malformed crypto object is a plausible remote panic, i.e. a DoS. |
| 18 | Oversized packets | **D&A** | Hard caps at every boundary, and an over-cap frame is dropped rather than parsed. **[R20]** Which caps exist, their values, and the fact that the gossip cap is a two-sided constant a peer cannot tune per build are `NETWORK_STACK.md` §11.3 and §6.5's, and the enumeration this cell carried is deleted under D-011 rule 1. What is classified here: the caps are **structural**, applied before decoding, so an oversized frame costs a receiver nothing beyond the bytes it already read; and the response to one is volume-keyed, never keyed on fault or attribution, so it is not an eviction and is untouched by D-011 rule 3. |
| 19 | Resource exhaustion "in reasonable measure" | **OOS** | Mitigated, not solved, and `SPEC_CS.md` §18 lists DoS as beyond the protocol's reach. Mitigations in place: `connection_limits` (max pending/established, per-peer cap), `memory_connection_limits` at a percentage of RAM, GossipSub peer scoring and `validate_messages()`, `flood_publish(false)` to remove an amplification lever, a bounded dial budget for unverified DHT hints, subscription filters, and the relay's own reservation/circuit rate limiters. An adversary with meaningful bandwidth defeats all of it. |

**§17 catalogue counts: CP 11 · D&A 7 · DNA 0 · OOS 1 · total 19.**

**Re-classified by D-014, and the movement is inside the cells rather than between
buckets.** No row above changes bucket — CP is a claim about what an adversary can
*construct* and D-014 constructs nothing, and D&A is a claim about detection and
attribution, both of which already held. What changes is the **worst case at the
end of the row**, which is the sentence D-010 shrank to *"a wasted hand"* and which
for the tier-1 subset is now *"a wasted hand and the attacker's seat"*. It is
tabulated rather than written into eleven cells, because the reader who needs it is
checking a **boundary**, and a boundary is only checkable if both sides are visible:

| §17 row | Tier under D-014 | Worst case for the attacker, after D-014 |
|---|---|---|
| 1, 2, 3 — fake / duplicate / removed card | **1** (a deck that gains, loses or duplicates a card across a shuffle) | CP unchanged; the message was already unconstructible, and if one is nevertheless emitted the emitter **loses its seat** |
| 4(a) — an invalid shuffle | **1** (failed shuffle proof) | as above. `STATE_MACHINE.md` T21 already aborted the hand and named the shuffler; the removal is what is added |
| 12 — changing an already-signed action | **1** (signature does not verify) | CP unchanged; **loses its seat** |
| 13 — replay of old actions | **none**, and this row is the correction §5.1.1 exists for. It read *tier 1 (chain parent does not exist at the replayed position)*, which fails question 1 of the admissibility test: the parent is looked for in **this receiver's own store**, so a peer that has not yet received the parent would remove an honest sender over a dropped frame | **CP unchanged, and it is CP that answers this row** — the signed body binds `protocol_version`, `table_id`, `hand_id`, `sequence` and `previous_event_hash`, so a replayed event is valid at exactly one position of one chain and is simply rejected and named. No removal follows. A replay that is *also* illegal against checkpoint-fixed state reaches tier **2** by that other route and by its own precondition, never by this one |
| 15 — impersonation | **1** (signer is not a party to this table) | CP unchanged; **loses its seat** |
| 17 — malformed packets | **1** (malformed message, out-of-range field, non-canonical encoding) | D&A becomes **detected, named and answered**: the sender **loses its seat**. The residual in that row is untouched — an unfuzzed crypto deserialiser is a remote panic, and a peer that panics never reaches the validator that would remove anybody |
| 18 — oversized packets | **none** | unchanged, and deliberately: an over-cap frame is dropped **before decoding**, so there is no signed message to judge and no accused. The response is volume-keyed, which D-011 rule 3 requires it to stay |
| 7 — reading the board early | **2** (which street it is, is state) | removal **only** after the checkpoint that fixed the street. Before it: rejection and a name, as before |
| 8 — illegal poker action | **2** | as above, judged against the checkpoint that closed the previous betting round |
| 9, 10 — fake stack, fake pot | **2** | as above |
| 11 — action out of order | **2** (`player_to_act` is a function of state) | as above |
| 16 — equivocation | **neither** | **unchanged, and this is the row that shows where the boundary is.** An `EquivocationProof` is not a message the accused signed asserting something illegal; it is a *predicate over two* messages, and that predicate has failed five times, twice against honest peers following the rules (X31, X32). D-014 excludes it by name. The worst case stays a wasted hand |

**The pattern in that table is the useful part, and it is now exact rather than
nearly exact.** Every tier-1 row is one the **cryptography or the encoder**
already answered, and every tier-2 row is one the **engine** answers — which is
why tier 2 needs the checkpoint and tier 1 does not, and why the rows D-010 was
written about (16, and X7, X10, X30, X31, X32 below) are in neither column.
D-014 hardens the cases that were already hard and leaves the soft ones exactly
as soft as they were. **Row 13 is what made the pattern only nearly exact**: it
was tier 1 on a check the *store* answers, which is neither of the two, and
§5.1.1 is the test that catches that class. A proposed tier-1 addition that
cannot be handed to the cryptography or to the encoder is in the wrong tier, and
that is the cheap version of §5.1.1's three questions.

### 5.3 Protocol-level attacks not named in §17

These fall out of the research notes and of D-001 to **D-014**. They are catalogued
with the same scheme because omitting them would make the §17 table look more
complete than the system is.

**Reclassified by D-010, and listed here so the change is not buried in the
cells.** Six rows below turned on automated forfeiture: five because it was the
attacker's payoff — in four of those, an *honest* peer's chips or key — and one
(X8) because forfeiture was the only thing closing it. Delete forfeiture and all
six change, five for the better and one for the worse.

| Row | Payoff before D-010 | Payoff after D-010 | Class before | Class after |
|---|---|---|---|---|
| X8 | none — the escape was closed by forfeiture | **the attacker keeps its committed chips** | D&A | **V** |
| X9 | the victim's forfeited commitment, collected by the attacker | nothing; a wasted hand | OOS | OOS (unchanged label, **payoff deleted**) |
| X10 | a stolen action, and under `kind = 2` the subject's chips | a stolen action and a wasted hand | DNA | DNA (unchanged label, **chip half deleted**) |
| X29 | the table, and a free escape from a losing pot | the table, and an escape that X8 now gives more cheaply | DNA | DNA (unchanged label, **no longer the cheapest escape**) |
| X30 | the subject's committed chips at any table size | a rejected message and a wasted hand | D&A | D&A (unchanged label, **prize deleted**) |
| X31 | **an honest player's chips forfeited and their key blocked** | a wasted hand | D&A | D&A (unchanged label, **prize deleted**) |

Only one label moves, and it moves in the direction that costs us something.
That is the shape of an honest re-classification: D-010 does not make the design
stronger against any of these five, it removes what they were worth, and it pays
for that with the sixth. Where a label is unchanged the cell says explicitly what
was deleted from underneath it, because a class that survives for a different
reason than it was assigned is exactly the kind of stale verdict four review
passes have been finding.

**Reclassified again by D-011 rule 3, and for the same reason: a payoff was
deleted, not a defence added.** D-010 removed the chips. It left the *second*
prize untouched at the layers it did not reach — a proof still put the accused
key on a transport block list (`NETWORK_STACK.md`, two sites) and a
self-contained proof still unseated a seat (`STATE_MACHINE.md` T11). Every row
whose payoff was, or could escalate to, the victim losing its network or its
seat is re-classified here. **In each case the worst case is now a wasted hand,
plus at most a connection the victim re-establishes** — which is the same
outcome as the flaky Wi-Fi the transport layer has to survive anyway.

| Row | Payoff before D-011 | Payoff after D-011 rule 3 | Class before | Class after |
|---|---|---|---|---|
| §17 row 16 | the accused key on a transport block list, honest or not | nothing; a wasted hand | D&A | D&A (unchanged label, **transport half deleted**) |
| X31 | this document already claimed "not block-listed"; the transport layer still did it | nothing; a wasted hand | D&A | D&A (unchanged label, **claim now true at every layer**) |
| **X32** *(new)* | the honest peer that ends a stalled hand manufactures a proof against itself and is block-listed for it | a wasted hand, and the stalled hand ends | — | **D&A** |
| X6 | the beacon's *reveal-mismatch* sibling — a seat unseated on a self-contained commitment-mismatch proof — had no catalogue row of its own and so was never classified here | no seat is removed; a table that cannot agree its beacon simply fails to form, at `ledger_in == 0` | *(unclassified)* | folded into **X6** as D&A |

**Nothing in this table is a strengthening and the counts must not be read as
one.** Two of the four labels do not move at all; what moves is the size of the
worst case underneath them. The one genuinely new row, **X32**, is a defect the
corpus was carrying — an honest peer framed by the mechanism meant to rescue a
stalled hand — and adding it enlarges the denominator, exactly as X30 and X31
did. A review that works makes the proportion classified as prevented fall.

| # | Attack | Class | Justification |
|---|---|---|---|
| X1 | Rogue aggregate key: the last player to publish `pk_i` chooses it as `X − Σ others` so that `apk` has a discrete log they know | **CP** | Every public key is accompanied by an `OwnershipProof` — a textbook Schnorr proof of knowledge of `sk` — and the aggregate constructor accepts only `Verified<PublicKey>` values. `Verified<T>` has a private field and deliberately **no** `CanonicalDeserialize` impl, so it cannot be forged from outside the crate or smuggled in off the wire; a downstream attempt to construct one fails to compile with `E0423` (`research/MENTAL_POKER.md` §4.1, negative compile test). Our obligation: verify every ownership proof before aggregating, and never bypass the `Verified` type-state. |
| X2 | Remapping deck indices to recipients after the final deck is known | **CP** (structurally excluded) | **[R23]** The index map's construction is `PROTOCOL.md` §4.5's and the field list this cell reproduced is deleted under D-011 rule 1. What makes the row CP is structural and states no construction: the map is **derived**, from state fixed and signed before the shuffle chain begins, so **there is no message in the grammar in which a different map can be asserted**. A peer that computes a different one does not attack anybody; it simply diverges and is caught at the next checkpoint. *This row is exactly assumption A10 and is only "prevented" as long as A10 is honoured in the implementation.* |
| X3 | Publishing a reveal token for a future street's index early | **D&A** | The library has no concept of "too early"; our state machine gates it. The message is constructible, achieves nothing alone (row 6), is rejected by every honest client, and is signed. |
| X4 | Encoding equivocation: two byte encodings of one logical event, signed separately, sent to different peers | **CP** | **[R21]** The canonical encoding and the gate that enforces it are `PROTOCOL.md` §2.8's and `CRYPTOGRAPHY.md` §4.7's — the container shape, the field-order rule, the position of the gate relative to the signature check and the append-only field-index rule are all theirs, and the description this cell carried is deleted under D-011 rule 1. What is classified here is that the attack is **structurally excluded**: canonicalisation is decided by the encoding rules and checked before any signature is verified, so a second byte encoding of one logical event is rejected at the gate rather than reaching a slot. The measured evidence for the gate is this document's to keep, because it is evidence rather than definition — all three hostile encodings were caught: non-preferred integers (`8218011a00000018`), indefinite-length arrays (`9f…ff`), and trailing bytes; truncated inputs are all rejected without panic (`research/CRYPTO_LIBS.md` §4.7). The one thing this row does add, because it is a *threat* observation rather than an encoding rule: reusing a field index would silently change the meaning of historical signed bytes, so the append-only discipline `PROTOCOL.md` §2.8 states is load-bearing for G6 and not a style preference. *Inherits A5.* |
| X5 | Cross-hand or cross-table proof replay | **CP** | Same mechanism as row 13 applied to the cryptographic objects: the `ctx` binding (A11). Requires the named regression test to be real. |
| X6 | RNG beacon abort bias: the last revealer sees everyone else's value, computes the outcome, and refuses to reveal if it dislikes it | **D&A** | Inherent to commit/reveal and not preventable without a delay function or a threshold construction, neither of which is warranted here. Non-revelation is a protocol failure attributable to a specific key, and the beacon governs only seating and the initial button — never the deck, whose randomness comes structurally from the shuffle chain (row 5). The cost of the attack is one visible, attributed refusal for one re-draw of the seating.<br><br>**The reveal-*mismatch* sibling is folded in here rather than left unclassified, and its payoff is deleted by D-011 rule 3.** A seat that reveals a value not matching its commitment produces a self-contained proof — the commitment and the bad opening, needing no table state — and the transition that consumed it **unseated the subject**. That is an eviction applied by the protocol on the strength of a proof, which D-010 point 3 forbids and D-011 rule 3 removes at every layer, so the transition is deleted (`STATE_MACHINE.md` §5.2). The behavioural consequence is stated rather than glossed: a seat that commits and then reveals badly, or never reveals at all, **can no longer be removed at any table size**, so one seat can stall a forming table until its join deadline. That is a griefing cost and not an integrity cost, because no hand has started and `ledger_in == 0` — nobody's chips are at stake, and the remedy is that the table does not form. Classified **D&A** on the same terms as the non-revelation half: the mismatch is detected by every peer, named by signature, and answered by nothing. |
| X7 | Withholding decryption shares to force a hand to abort (griefing) | **D&A / DNA** | Permanent and inherent to the n-of-n construction: any player who goes silent makes it impossible for anyone to open any further card. The missing `REVEAL_TOKEN` for a given `(hand_id, card_index)` is publicly visible and every other player's signed events prove they did their part. **Attribution depends on the case and is not uniform:**<br><br>*one silent seat, with `\|V(subject)\| >= 2`* — **D&A**;<br>*two or more seats silent simultaneously* — **DNA**;<br>*any silent seat where `\|V(subject)\| < 2`, which at `n = 2` is always* — **DNA**.<br><br>Attribution is sound for a single silent seat whenever the required voter set still has two or more members; it is unavailable when two or more seats stop at once, because each required voter set contains the other subject and D-008 forbids removing a seat from `V` on anything short of a completed certificate (`PROTOCOL.md` Q-02, `STATE_MACHINE.md` Q3, OQ-E), and unavailable whenever `\|V\| < 2` for the reason in D-007 as generalised by D-008. **It cannot be prevented, and the fix that would prevent it is forbidden** — see §7. **Under D-010 the three cases have identical chip outcomes**: every abort restores every stack to its start-of-hand value, so the silent seat keeps its commitment in all three, and the split above now records only whether the transcript *names* anybody. That distinction no longer changes any outcome, because attribution carries no automatic consequence — it is kept in the split because naming is still what a human or a later version would adjudicate on, and because a document that collapsed the cases would be claiming an attribution property it does not have. The attacker gains no cards. What it gains is a wasted hand; where the silent seat is also the losing seat, that is X8's escape rather than pure griefing, and X8 is where the cost is classified. Repeated aborts attributable to one identity are visible to everyone and counted per identity in the lobby; in play money that is the whole penalty, and `SPEC_CS.md` §18 forbids claiming more. |
| X8 | **The rage-quit escape, reopened by D-010**: a player about to lose a large pot stalls or disconnects, the hand aborts, and their committed chips come back to them | **V** — visible, not prevented | **This row is D-010's accepted cost, carried as a catalogue entry in its own right rather than as a footnote to a decision.** D-005 closed this escape with forfeiture: the vanished player's committed chips were distributed to the remaining players in proportion to their own contributions, so quitting cost exactly what folding would have cost. **D-010 reverses that**, knowingly and with the reason recorded: forfeiture was the prize that made five successive rounds of attacks worth mounting, and every severe one of them ended in an *honest* peer's chips being taken and its key blocked for following the protocol (X30, X31, X32). An exploit that harms an honest player is worse than one that merely lets a dishonest player escape a loss, so the escape is accepted and the machinery is deleted.<br><br>**What the attack now is.** Any player, at any table size, at will, with no special capability, no coalition, no modified client beyond the ability to stop sending: stop publishing at a cryptographic step, wait, and the hand ends with every stack restored. There is no `\|V\|` condition on it any more, no `cause` on which it fails, and nothing that distinguishes it from the honest disconnect it imitates. The one case it does not reach is unchanged and is worth stating, because it is the only structural bound: **the hand completes normally if everybody else folds**, so a quitter does not escape when the others simply fold.<br><br>**The mitigations, in full, and there are exactly two.** (i) The transcript records the abort and the missing contribution, signed, permanently, and every participant can verify which peer stopped publishing — where a certificate can attribute, it is named outright, and where it cannot the gap in the stage is still legible. (ii) The client shows a **per-identity abort count in the lobby**, so players can decline to sit with someone who does it. Both are *social*: sitting a repeat aborter out is a user decision, never a protocol action (D-010 point 3). Neither has teeth, and §6 says why — a new identity is a locally generated keypair and costs nothing.<br><br>**What may not be claimed about this row.** `SPEC_CS.md` §18 forbids describing a merely visible failure as prevented or detected-and-punished, and this row is the corpus's clearest instance of one: some cheating is prevented, some is detected, and this is **merely visible**. No document may say the escape is closed, bounded by a penalty, deterred by reputation, or made expensive. It is none of those.<br><br>**What is still true, and it is the one thing the design keeps.** The escape is **slow and legible**, not instantaneous. A below-floor certificate is inert (D-009 rule 2), so no single message voids a hand: the quitter must stall for the whole hand deadline — **[R16]** whose value is `PROTOCOL.md` §8.4's and is not printed here — with the missing contribution visible the whole time, and the abort count rises. That is a difference in cost and in visibility, not in outcome, and it must not be reported as more. **One thing this row used to be able to claim and can no longer:** that the escape at least left the quitter exposed to the transport-layer consequences of any proof they generated on the way out. There are none (D-011 rule 3). The escape now costs a stall and nothing else. §7.4 records the trade and §9.1.2 limitation 4 carries it as a limitation. Revisited before real money, and not before the certificate, equivocation and dispute machinery has survived a full adversarial pass (D-010, closing paragraph). |
| X9 | Knock an opponent off the network right after a large bet | **OOS**, and **its payoff is deleted by D-010** | This row existed because D-005 handed the attacker something: the victim's forfeited commitment, distributed to the seats still in the hand, one of which was the attacker's. **Under D-010 there is nothing to collect.** The hand aborts, every stack returns to its start-of-hand value, and the attacker has spent a real out-of-protocol capability against the victim's connection (network DoS, or control of the relay that connection depends on — §3.5) to buy a wasted hand — the same outcome the victim's own flaky Wi-Fi would have produced for free. The row stays in the catalogue, and stays **OOS**, because the capability is unchanged and §6 still lists network denial of service as beyond the protocol's reach; what is gone is the *incentive*, and with it the one poker-specific aggravation that made this worse than generic DoS. Note the direction of the trade, stated plainly rather than presented as a win: D-005 closed X8 at the price of X9, and D-010 closes X9 at the price of X8. The reason for preferring this direction is that X9 harms an honest player and X8 only lets a dishonest one escape a loss (D-010). |
| X10 | A forged timeout certificate by all other dealt-in seats, stealing the subject's action | **DNA** (where `\|V\| >= 2`) | Under **D-006** a timeout takes effect only through a certificate signed by every member of `V(subject)`, the set of **all other dealt-in seats**. **[R19]** What the certificate names and how it chains are `PROTOCOL.md` §4.8's, not this row's. Nominally `\|V\| = n - 1`: **one seat at `n = 2`, two at `n = 3`, nine at `n = 10`** — but that table is a ceiling, not the operative rule. `V` is the other dealt-in seats **minus any seat a completed, valid certificate has already attributed in this hand** (`PROTOCOL.md` §8.3), so the operative quantity is `\|V\|` and every rule in this row is written on `\|V\|` rather than on `n`, per **D-008**. That is not a formality: an earlier draft let a seat be removed from `V` by an unproven accusation, which collapsed `\|V\|` to one at any table size — X30. Requiring all of `V` defeats a single false accuser. It does **not** defeat a coalition consisting of all of them, who are unanimous with themselves — available with two colluders at `n = 3`. The victim can broadcast their own signed action carrying the same parent hash, so any third party sees a conflict — but with no trusted clock (A13) nobody can establish which came first. **Detected, not resolvable.**<br><br>**Whenever `\|V\| < 2` the mechanism gives no protection at all and is therefore inert.** With `\|V\| = 1`, "unanimous" is one signature and unanimity is vacuous; **D-007** makes the heads-up action deadline advisory and produces no signed state transition from it. **The rule is scoped on `\|V\|`, never on the seat count, and both kinds have one disposition.** An earlier revision of this cell said a `kind = 1` certificate "is invalid at two seats and must be rejected by every receiver". **That sentence is withdrawn on both counts** (D-008, `PROTOCOL.md` §8.3): it was scoped on `n`, which is the scoping X30 walked through, and it used the word *invalid*, which `PROTOCOL.md` §8.3 withdrew precisely because it gave `kind = 1` and `kind = 2` two dispositions where D-009 rule 2 gives them one. The disposition is uniform and it is not an error: **a certificate whose required voter set has fewer than two members is silently ignored**, at any seat count and for either kind. **D-008** carries that floor to every seat count: a certificate whose required voter set has fewer than two members has no effect at all — not an error, not evidence, not chained, the deadline simply stays advisory. **D-009 rule 2** makes that unconditional, so it holds for `kind = 2` and for the hand deadline as much as for `kind = 1`: a below-floor certificate **does not end a hand**, and a hand that cannot proceed ends instead when `hand_deadline_ms` expires as a local timer every peer derives from the same signed `HAND_INIT` (§7.3(c), `PROTOCOL.md` §8.4). At `n = 2`, `\|V\| = 1` always, so heads-up is the permanent instance rather than the only one.<br><br>**The race between a late action and a certificate is settled only by the presence of an honest required voter; there is no cryptographic artefact that settles it.** An earlier draft carried this as a separate row claiming the race was structurally impossible, on the argument that a peer emitting both a `TIMEOUT_VOTE` and a real event for one stage had equivocated provably. That claim was false — the vote is signed by the voter and the action by the subject, two events by two different keys, which prove nothing against anyone — and after the `event_class` discriminator (`PROTOCOL.md` §5.2, §4.8) the two do not even occupy the same slot. The honest statement: if at least one required voter is honest and saw the action, no certificate forms and the action stands; if every required voter is dishonest, or none saw the action, a certificate forms. Nothing settles the race when a voter lies about what it saw. `PROTOCOL.md` **Q-08** records the `ACTION_SEEN` construction that would make a lying voter provable, and it is **not** adopted. (That question was this document's `OQ-C` until the letter series was reconciled onto `DECISIONS.md`'s — §9.2.)<br><br>This is a genuine, unfixed limitation of the design and it is carried forward as OQ8; for the `\|V\| < 2` case, which heads-up always is, the deadline is simply advisory (D-007, D-008) and there is no chip question left to carry, D-010 having taken restoration everywhere. Distinguish it from X30: X10 is a coalition that *legitimately is* the whole voter set; X30 was a single client *manufacturing* that position.<br><br>**What D-010 and D-011 remove from this row, and what they leave.** Removed by D-010: the chip half. A certificate against the subject used to end the hand with the subject's committed chips distributed to the seats that signed it, so a legitimate voter set that was also a coalition could *take* from the honest player it out-voted. It cannot now — the abort is neutral, every stack is restored, and the certificate names a seat without any consequence following from the name. Removed by D-011 rule 3: the escalation. A completed certificate names a seat, and a named seat used to be reachable by the layers that acted on names; no layer acts on one now, so the victim of a forged certificate keeps its seat and its connections as well as its chips. Left standing, and not softened: at `\|V\| >= 2` a coalition that is the whole voter set can still **steal the honest player's action**, folding a hand the player was about to defend, and no artefact settles the race. The classification stays **DNA** for exactly that reason — the theft of an action is real, it is detected, and nobody can be named for it. **What the row's worst case has become is one hand the honest player did not get to play.** |
| X12 | Colluding muck: a colluder with the winning hand mucks so the pot goes to their partner | **OOS** | Collusion, §6. It also interacts with an unresolved design question: if mucking is allowed at all, the transcript can only prove "the award was correct given the players who did not forfeit", not "no unrevealed hand was better" (`research/POKER_RULES.md` §A8). That weakens G8 in a precisely stateable way. Mandatory universal reveal removes the attack but changes the game and leaks strictly more than real poker. Unresolved — OQ6. |
| X13 | Table-advert bait and switch: advertised parameters differ from those actually played | **D&A** | The advert is signed and carries the full parameter set (blinds, stacks, schedule, timers, seats). The parameters that bind are the ones every participant agreed and signed before the first hand (`SPEC_CS.md` §4), and every event carries them transitively through the chain. A mismatch is rejected before any hand starts. |
| X14 | Protocol-version downgrade | **D&A** | `protocol_version` is inside every signed body and inside `ctx`. A peer offering an older version is refused rather than accommodated (A14). |
| X15 | Lobby spam: advert spam, pinning a table in every lobby forever, or flooding `LOBBY_CHAT` | **D&A** per peer (residual DoS is out of scope) | **Scope includes lobby chat**, which carries no security claim of any kind — see §6. **[R11]** The mechanisms that bound this row are their owners' and are not reproduced here: the chat message and its size cap are `PROTOCOL.md` §7.6's, the per-peer rate limits, the GossipSub validation mode, the message-id override and the freshness window are `NETWORK_STACK.md` §6.2, §6.4, §6.5 and §6.6's, and the advert-eviction rule is §10.3's. The earlier revision of this cell printed the numbers, which meant a tuning change in either owner silently made the threat model wrong. What this row classifies is the shape: **every lobby message is validated by the application before it is forwarded**, so junk is rejected rather than relayed and the forwarder pays a peer-score penalty; deduplication keys on content rather than on a sender-chosen sequence number, so one peer cannot republish identical content forever; and freshness is judged on age-since-receipt rather than on the sender's own clock. None of that is a penalty applied to a *proven protocol violation* — it is volume- and validity-keyed, identical for a buggy peer and a hostile one, and therefore untouched by D-010 point 3 and D-011 rule 3, which forbid only eviction driven by a protocol proof. Per-peer it is D&A; in aggregate an adversary with bandwidth defeats it, which is row 19. |
| X16 | Polluting `LOBBY_INFOHASH` with junk `IP:port` records | **OOS** | Structural: a DHT announce carries no proof of possession beyond the storing node's IP check, so anyone can write anything. Observed in the wild on a private random infohash within 24 minutes (§8). Cost is bounded — a bounded dial budget, deduplication, dropping private/reserved ranges, and the handshake filtering non-libp2p listeners — so the impact is dial timeouts, not a correctness failure. `SPEC_CS.md` §1 already declares the list a hint. |
| X17 | Sybil eclipse of the GossipSub lobby mesh | **OOS** | Sybil resistance is out of scope without an identity/reputation layer (§6). Mitigations: `mesh_outbound_min = 3` raises the cost of an eclipse by inbound-only Sybils, the snapshot fetch queries several independent peers, and the DHT provides a peer source independent of the mesh. None of these is a proof. |
| X18 | The relay reads game content | **CP** | End-to-end Noise/TLS terminated at the peers; the relay is a byte pipe holding no key share (**D-001**). |
| X19 | The relay forges or alters events | **CP** | Every event is application-signed and every receiver re-validates the signature and the hash chain independently (**D-001**). *Inherits A5, A6.* |
| X20 | The relay drops, delays or resets a target peer's connection | **OOS** | A liveness dependency and a DoS lever, acknowledged in **D-001**. It includes the *default* case, not only the malicious one: a public relay's circuit limits will reset a poker session mid-hand, and the **duration** limit is what a session reaches first. **[R12]** The limits, the byte accounting, the per-hand arithmetic and the required client behaviour are `NETWORK_STACK.md` §9.5 and §16.1's, and the derivation this cell reproduced — the same one §3.5 also reproduced, so the document carried it twice — is deleted under D-011 rule 1. It had already been wrong once by a factor of two, in both copies, which is the argument for having one. What this row classifies: the capability is real, it is out of the protocol's reach, and its consequence is bounded to what any disconnect costs, because **a relayed connection loss is treated exactly like any other disconnect** (§7) and under D-010 that is a neutral abort. Nothing about it block-lists or unseats anybody (D-011 rule 3), so a relay operator cannot escalate a reset into the loss of a victim's seat or key. |
| X21 | Abusing a D-002 volunteer relay's bandwidth | **OOS** (resource abuse), with mandatory mitigation | Circuit Relay v2 is **not protocol-selective**: its protocol names are compile-time constants and its behaviour decides accept-or-deny purely on resource limits, with no application ACL hook. Left alone, enabling the relay server makes the user an **open relay for the entire libp2p network**, IPFS traffic included, on their own line. **[R13]** The admission-control construction — which hook it installs into, the trait, the named type holding the admitted-peer set, and the requirement that *both* rate-limiter vectors be gated — is `NETWORK_STACK.md` §9.6's, and the copy this cell carried is deleted under D-011 rule 1. What this row classifies: admission control is **mandatory and not a hardening option**, because without it the user is running an open relay; it is nonetheless **mitigation, not prevention**, because an attacker running our own client is admitted by construction and still consumes capacity. Note the boundary that keeps this compatible with D-011 rule 3: relay admission is a *resource* decision about strangers, made before any table exists and keyed on whether a peer is one of ours — it is never driven by a protocol proof and never removes a peer from a table. Relaying is off by default and must be disclosed plainly before it is enabled (**D-002**, OQ7, OQ10). |
| X22 | State divergence caused by an honest implementation bug | **DNA** | `STATE_HASH` detects it and play stops (G10), but the transcript shows only that two clients disagree, not who is wrong — there is no signed event to attribute, because both peers believe they followed the rules. Resolving it requires human diagnosis. It is the reason A15 and the §26 property tests exist. Note that the *deliberate* version of the same divergence — a peer publishing a `state_hash` it did not derive — is X29, and no live rule distinguishes the two, which is why neither is attributable. |
| X23 | Timing side channel on the secret permutation or on `sk_i` | **OOS** for the play-money prototype, pending measurement | Only throughput was measured, never constant-time behaviour, and arkworks is not written with curve25519-dalek's constant-time discipline (`research/MENTAL_POKER.md` §9 risk 4). Declaring it out of scope is defensible for play money and **is not defensible for real money**. Settled by `dudect`-style analysis of `shuffle_deck` and `reveal_token`, or by an explicit decision. OQ4. |
| X24 | Endpoint compromise (malware reading the player's own cards or stealing their signing key) | **OOS** | §6, and assumption A8. |
| X25 | Out-of-band collusion (a voice channel, a shared screen, one person at two machines in a room) | **OOS** | §6. This is the single most damaging real-world attack on any poker system and no cryptography addresses it. |
| X26 | Multi-accounting / Sybil identities at one table | **OOS** | §6. Identity is free: a new profile is a new Ed25519 key. |
| X27 | Coercion of a player | **OOS** | §6. |
| X28 | Traffic analysis of relayed and DHT traffic | **OOS** | §6 and §8. The relay sees who talks to whom, when and how much; the DHT publishes presence to strangers on a schedule. |
| X29 | Fault any table on demand by publishing a false `state_hash` at a checkpoint | **DNA** | A `state_hash` is a one-field value in a message every peer must emit. A single peer that publishes a value it did not derive forces the `PROTOCOL.md` §6.3 case (c) path: the hand aborts with `cause = 4`, stacks are restored, and the table closes, with no attribution live. It needs no invalid signature and no divergent transcript, and "at least two peers disagreeing" is satisfied by one liar plus the honest victim. Bounded by three things — it costs the attacker the table and, in a tournament, their own equity in it; checkpoint 7 sits **before `SHOWDOWN_REVEAL`**, and no checkpoint is placed after a hole card has been opened **to anyone but its owner** (every player opens its own two cards at `DEAL_PRIVATE`, which precedes checkpoints 3–7, so the rule is about *public* opening and is stated that way rather than in the falsifiable shorter form an earlier revision used), so the attacker must commit while it still knows only its own hand; and the evidence, the unanimous transcript plus every peer's signed `STATE_HASH`, is written to the profile directory and **is preserved in a form sufficient for a human, or for a future adjudicator, to diagnose the divergence — no adjudication procedure is specified.** An earlier revision of this row claimed the evidence was "deterministically adjudicable offline by any third party running the reference engine over it, forever". **That claim is withdrawn.** No document in this corpus defines, versions against `protocol_version`, or authorises such a reference engine, and moving a derivation offline does not manufacture the observer-independent reference that this row's own argument says does not exist. Whether a canonical reference engine is named is an open decision, recorded in `docs/DECISIONS.md`'s open list. Until it is, this third bound is **evidence preservation, not recourse**: neither live nor offline adjudication is available, and neither is claimed. An earlier draft resolved case (c) by removing the odd peer out under a "unanimity minus one" rule; that rule was **deleted**, because it let `n-1` colluders take an honest player's committed chips at `n >= 3` (two colluders suffice at `n = 3`), and because there is no observer-independent derivation at run time by which anyone could be named — every peer derives with its own engine, so "attribute whoever disagrees" is a vote over the facts, which `SPEC_CS.md` §15 forbids. Not solved. Related: X8, X22, and **OQ-A** — the reference engine this row's withdrawn third bound would need.<br><br>**Under D-010 two of this row's bounds change, one for the better and one for the worse.** Better: the deleted "unanimity minus one" rule can no longer be missed, because there is nothing for it to have done — no proof and no attribution moves a chip, so even a correctly named liar would forfeit nothing, and `cause = 4`'s restoration is no longer a *disposition chosen among alternatives* but the only disposition the MVP has. The `cause = 4` chip question therefore shrinks from "who pays" to "does anything distinguish this abort from any other", and the answer is no; of what that question used to bundle together, the *settle-from-the-last-agreed-checkpoint* half is now `PROTOCOL.md` **Q-07** and the *adjudication* half is **OQ-A** (§9.2). Worse, and it must be said: this row used to be one of the two ways to escape a losing pot, and it was the expensive one — it cost the attacker the table. **X8 is now the cheap one**, so an attacker with that motive has no reason to come here at all. What survives as this row's own attack is the griefing use — faulting a table on demand, at the cost of one's own equity in it — and that is unchanged. |
| X30 | Shrinking the required voter set to one seat — itself — and then taking the subject's committed chips with a certificate that seat signs alone, at **any** table size | **D&A**, and only because of a rule our own client enforces | **The attack.** `V(subject)` was the other dealt-in seats minus any seat "named as the subject of an outstanding, older unmet deadline", and a seat is *named* by any peer emitting a `TIMEOUT_VOTE` against it — an assertion the protocol concedes is unprovable when the voter lies ("there is no artefact that settles the race when a voter lies about what it saw", `PROTOCOL.md` §8.3). Nothing bounded how many seats one client could name. Six-seat table, one modified client at seat 1: Mallory votes against seats 2, 3, 4 and 5 at one stage; at the next she declares seat 6 the subject, and `V(6) = {1,2,3,4,5} \ {2,3,4,5} = {Mallory}`. She emits the one required vote, assembles a complete certificate from her own signature, the stage completes, a `kind = 2` certificate aborts the hand with seat 6 attributed, and under **D-005** seat 6's committed chips are forfeited to the remaining seats — which is mostly to her.<br><br>**The earlier scoping on `n` did not stop it, and that is the point.** Every protection D-007 wrote was conditioned on `n == 2`; here `n` is six and stays six, so nothing rejected the certificate. The nominal `\|V\| = n - 1` table printed in three documents was never the operative rule — `\|V\|` was `n - 1 - \|excluded\|`, and the attacker controlled `\|excluded\|`. This is the A-1 attack reappearing above heads-up, and worse than A-1, which cost a folded hand rather than chips.<br><br>**The mitigation.** **D-008**: every rule that weakens, disables or gates the certificate is scoped on `\|V\|` and never on `n`; a certificate whose required voter set has fewer than two members **has no effect**; and **a seat leaves `V` only once a completed, valid certificate names it — being voted against is not exclusion.** The second half is what makes the first hold inductively: each exclusion now costs a completed certificate, and each such certificate needed `\|V\| >= 2` at the moment it formed, so `V` cannot be collapsed by assertion at any seat count. Normative in `PROTOCOL.md` §8.3/§8.4 and `STATE_MACHINE.md` §8.4; checkable by any verifier replaying the transcript, with no new cryptography.<br><br>**Why D&A and not CP, stated rather than rounded up.** The message stays constructible: a modified client can always sign and broadcast a one-signer certificate. What changed is that an honest client gives it no effect, so the state does not advance and no chips move; the artefact names its signers, so it is bound to a key. The attribution half is deliberately weak and must not be overstated — under D-008 an inert certificate is *not evidence of misbehaviour*, so no seat is sanctioned for emitting one and nothing is forfeited; what is transferable is the artefact, not a verdict. And the rejection rests on our own clients enforcing the `\|V\|` floor, an implementation obligation in the class of A12 and A14, not on A1–A7 — which is exactly why this row is not CP.<br><br>**Residual.** D-008 closes the *manufactured* `\|V\| = 1`, not the earned one. Where the voter set is honestly small or honestly hostile, X10 stands unchanged (**DNA**), and two or more seats going silent together is still X7's DNA case.<br><br>**D-010 deletes the prize, and the row's title is now historical.** "Taking the subject's committed chips" is what this attack was *for*; a completed certificate no longer moves a chip, so a modified client that succeeded in manufacturing `\|V\| = 1` today would end a hand neutrally and gain nothing that X8 does not give it for less effort. The class stays **D&A** and the floor stays in the client, for two reasons that survive the deletion of the payoff: the certificate is a chained event that **names a seat**, and a corpus that let one peer manufacture a naming would be putting a false attribution into the permanent record that D-010 point 2 says a human may later adjudicate on; and D-010 is explicitly revisitable before real money, at which point the payoff returns and this floor is the only thing standing between it and X30. A defence kept because the decision that removed its necessity is scheduled to be reconsidered is worth saying out loud rather than quietly dropping. |
| X31 | Two Sybil seats stall one collective stage so that **an honest voter's own two required timeout votes become an `EquivocationProof` against itself** — under D-005 that forfeited the honest player's committed chips and blocked their key; under **D-010** and **D-011 rule 3** it costs a wasted hand | **D&A**, and only because of a slot key our own client enforces | **When it was found this was the most damaging attack any review pass of this corpus had produced**, and it is recorded in its own right rather than folded into X7, because unlike every other row here the victim's *compliance* is the entire exploit. It needs no coalition majority, no cryptographic break, no modified victim and no capability against the victim's connection.<br><br>**The mechanism. [R9]** The wire shape that made it possible is `PROTOCOL.md` §4.8's and the slot it collided in is §5.2's; neither is reproduced here any more, and the tuples this cell used to print are deleted under D-011 rule 1 — printing a superseded key beside a live one is how a reader certifies the next X31 as clean. In classification terms: `TIMEOUT_VOTE` named the seat it was *about* in its body only, so **which seat a vote concerned did not change its slot**, while `PROTOCOL.md` §8.4 specifies two simultaneous subjects at one stage as *normal*. The protocol therefore both required an honest voter to vote against each seat that owed it something, and treated two distinct bodies in that one slot as proof of misbehaviour.<br><br>**The attack.** Identity is free (X26, §6): Mallory seats two keys, `M1` and `M2`, at one table. Both go silent at one collective crypto stage `s` — the capability X7 already concedes to everyone, at zero cost. Honest Bob's timers expire and he does exactly what the protocol asks: `TIMEOUT_VOTE{subject_seat = M1, sequence = s}` and `TIMEOUT_VOTE{subject_seat = M2, sequence = s}`. Two distinct bodies, one slot. Mallory holds both, wraps them as `EquivocationProof{accused = Bob}` — which by design verifies with no table state, no transcript and no knowledge of the game — and files it. `STATE_MACHINE.md` T55 aborts the hand, raises `Fault{Equivocation}`, writes `AbortRecord{kind: Equivocation, attributed: [Bob]}` and — **as the corpus stood under D-005** — applied the forfeiture formula that then existed: Bob's committed chips were distributed to the remaining seats, two of which are Mallory's, and the transport layer put Bob's key on its block list. Repeatable every hand, against a different honest seat each time, at the price of two free keypairs and silence.<br><br>**Both consequences are deleted — the first by D-010, the second only by D-011 rule 3, and the difference is worth recording.** D-010 removed the forfeiture formula, so the filed proof ends a hand neutrally and Bob's stack is restored. This cell then asserted that "Bob's key is not block-listed", and **that assertion was true of this document and false of the corpus**: `NETWORK_STACK.md` still called `block_peer` on an `EquivocationProof` in two places, and `STATE_MACHINE.md` T55/T56 still cited that consequence as live. A threat model that classified the attack on the strength of its own text, while the transport layer went on executing the payoff, is exactly the drift D-011 rule 1 exists to stop — and it was found by a sweep, not by reading this file. With D-011 rule 3 the claim is now true at every layer: Bob keeps his stack **and** his connections, and Mallory has spent two identities and a stall to buy one wasted hand, which X7 already gives her for nothing. The attack's economics collapse entirely. What survives is the *defect*: a specification that requires an honest peer to manufacture verifying evidence against itself is wrong on its own terms, the record is permanent and D-010 point 2 says a human may later adjudicate on it, and D-010 is revisitable before real money. That is why the mitigation below is a change to the canonical predicate rather than a note that it no longer matters.<br><br>**The mitigation is D-009 rule 1, now carried by D-011 rule 2. [R8]** The rule is normative in `PROTOCOL.md` §5.2 and the concrete key change — the subject becoming part of the key rather than of the body alone — is §5.2's and §5.3's to state; the index and tuple this cell used to print are deleted. What the row asserts is the classification consequence: two votes about two subjects occupy two slots, the predicate is not satisfied, and what Mallory files is not a proof of anything. D-011 rule 2 is what makes that durable rather than re-derived — one literal key, one site, `event_type` included — and the property is a standing obligation on every future message type, checked before it is added (G7, §5.5).<br><br>**Why D&A and not CP, stated rather than rounded up.** The artefact stays constructible — anyone can concatenate two of Bob's genuine signed votes and call the result a proof. What changed is that an honest client's predicate no longer matches, so the object verifies as nothing, the state does not advance, and no chips move. The rejection rests on **our own clients implementing the corrected slot key** — an implementation obligation in the class of A12 and A14, not on A1–A7 — which is exactly why this row is not CP. The attribution half is deliberately weak: a purported proof that fails the predicate is *not* evidence of misbehaviour by whoever filed it, so nothing is forfeited from Mallory either, and what is transferable is the artefact, not a verdict.<br><br>**Residual, and it is now the important half.** D-009 rule 1 closes the manufactured proof, not the stall behind it. Two seats going silent together still voids the hand for free, with nobody named and stacks restored — X7's DNA case, open as OQ-E — and Bob still gets no attribution against Mallory for it. **And the rule has since been violated twice more, taking the count to five.** The fourth was `STATE_HASH` re-emission during divergence recovery, **now closed**: a reconciliation round is its own stage with its own `sequence`, so the reconciling peer occupies a fresh slot rather than re-signing into an occupied one (`PROTOCOL.md` §4.9). The fifth is the terminal `HAND_ABORT`, and it is the sharpest of the five because it needs no Sybil pair and no dropped stream — it fires on the shipped heads-up configuration when an opponent simply goes quiet. It is catalogued in its own right as **X32** rather than left as a footnote here, and it is closed by D-011 rule 2. G7's table carries all five; §9.1.2 limitation 12 states why the property is asserted rather than demonstrated until the mirror test exists, and §5.5 is where it is caught. |

| X32 | **The terminus turns on the peer that reaches for it**: an opponent goes quiet at a collective stage, and the mechanism that exists to end the stalled hand — the terminal `HAND_ABORT` — is written at the stalled stage's own index, where the honest peer that already contributed to that stage has signed a different body. Its own abort is rejected, so the hand cannot end, **and the pair is a verifying `EquivocationProof` against the peer that emitted it** | **D&A**, and only because of a slot key our own client enforces | **The lesson this row exists to record: a mechanism built to end a stalled hand can itself become the evidence against the honest peer that emits it.** Every other row in this catalogue is an attack somebody mounts. This one is a *rescue path* that incriminates its user, and the attacker's whole contribution is to stop sending — the capability X7 already concedes to everyone for free. It was created by the fix to a different defect, which is the pattern worth naming: the flaw is never in the rule that was just written, it is in the path that rule newly made load-bearing.<br><br>**The two halves.** *(a) Liveness.* The abort chains from the last complete stage, so it lands at the stalled stage's index — there is no other index available, because the stalled stage never closed and nothing can chain from a `stage_hash` that does not exist. Every peer that contributed to that stage has already occupied that slot, so its own abort is a differing body at an occupied slot and is rejected by every receiver, its own client included. The only peers with a free slot there are the silent ones the abort exists to dispose of. On the shipped heads-up configuration there is no such peer that is also willing to emit, so **the hand cannot be ended at all** and no next hand can begin. *(b) Evidence.* The same two events satisfy the equivocation predicate exactly — same signer, same slot, different `event_hash` — and this is conduct the protocol **requires**, so it is D-009 rule 1's fifth violation (G7's table). The two halves compose into the four-pass pattern reconstituted with the chips removed: an honest peer follows the protocol, manufactures a proof against itself, and — until D-011 rule 3 — had its key block-listed at the transport layer. It kept its stack and lost the network.<br><br>**The mitigation is D-011 rule 2**, and the form of the mitigation is the point. D-009 rule 1 had already said the slot key must contain every field that legitimately varies for one signer at one stage. It was *prose*, and prose was re-derived differently by each editor, so `event_type` was never in the key and an abort therefore collided with an ordinary contribution at the same index. **D-011 rule 2 makes the key one literal tuple, written once, in `PROTOCOL.md` §5.2, including `event_type`; every other document points at it and none reproduces it.** With `event_type` in the key the terminal abort occupies a slot no contribution can occupy, so it is accepted, the hand ends, and no proof forms. **This is the sixth attempt at one property** — after the `chain_scope` discriminator, unchaining `DISPUTE`, putting the subject in the key, giving a reconciliation round its own stage, and D-009 rule 1 stated as a principle. The first five each closed one instance and left the property to be re-derived; the sixth replaces the principle with a definition that has a single site. If a seventh instance appears, D-011's own closing rule applies and the mechanism comes out of the MVP rather than acquiring a seventh rule.<br><br>**What it costs now, at each of the three stages of the fix.** Before D-010: the honest emitter's chips forfeited and its key blocked. After D-010, before D-011: chips restored, key still block-listed at the transport layer — a peer that played correctly loses its connections. After D-011 rules 2 and 3: **a wasted hand, and at most a connection the victim re-establishes.** The liveness half is not a chip defect and D-010 could not dissolve it; it needed rule 2.<br><br>**Why D&A and not CP, stated rather than rounded up.** The colliding pair stays constructible — anyone can take an honest peer's contribution and its abort and present them together. What changes is that with `event_type` in the key an honest client's predicate no longer matches, so the object verifies as nothing and the state does not advance. The rejection rests on **our own clients implementing the canonical key**, an implementation obligation in the class of A12 and A14, not on A1–A7, which is exactly why this row is not CP. It is the third row in this catalogue that is D&A only because of a rule this corpus got wrong first (with X30 and X31), and §9.3 caution 5 counts it.<br><br>**Residual.** Rule 2 closes the collision, not the stall behind it. The opponent who went quiet still wastes the hand for free — that is X7, and where they were losing it, X8. |
| X33 | **The fix that unfroze the hand made an agreed quantity per-receiver, and a line elsewhere read it into the next hand.** An opponent goes quiet, the hand ends by the terminal abort X32's fix made reachable — and two honest peers accept *different copies* of that abort, because the certificate path carries a name and the hand-deadline path carries none, with no precedence rule between them. A transition elsewhere then set a seat's status from the accepted copy's `attributed` field, so the next hand's `dealt_in` and `bb_seat` differ between honest peers, its collective stage never completes, and **the table never plays again** | **DNA** | **This row records a defect the corpus was carrying, and the lesson is a third shape to add to the two D-011 named.** D-011's two shapes were *a copy that drifted* and *a consequence that made an attack worth mounting*. Neither appears here. What appears is: **the defect is on the path the previous fix newly made load-bearing.** X32's fix — a witness-independent terminal stage — is what unfroze eleven phases, and it is the same fix that made "which copy of the abort did this peer accept" a per-receiver fact for the first time. The line that read `attributed` was *correct* while every completing peer necessarily held the same body. It stopped being correct the moment the terminus stopped requiring a witness, and nothing in the change touched the line that broke.<br><br>**Why it is worse than a stalled hand, which is the reason it is catalogued rather than noted.** X7 and X32 cost one hand. This costs the table permanently and silently: nothing is invalid, no signature fails, no proof forms, and each peer's own view is internally consistent. The two peers simply disagree about who is dealt in, so the collective stage that opens the next hand can never gather byte-identical bodies, and the failure surfaces as a table that hangs at hand `k+1` with no error to report. Stacks and the terminal value still agree, which is what makes it hard to see — the divergence is confined to two fields nobody hashes into anything that would mismatch first.<br><br>**Attacker capability required: none beyond silence**, which X7 already concedes to everyone at zero cost. The attacker does not choose which copy anybody accepts and does not need to; the fork is a property of the specification, not of anything the attacker sends. That is also why it is **DNA and not D&A**: the outcome is plainly *detected* — the table stops dead and the disagreement is visible in each peer's own `dealt_in` — but there is nobody to attribute it to. Both honest peers followed the protocol, and the peer that went silent did nothing that a network partition would not have done.<br><br>**The mitigation is D-012, and it is a rule about inputs rather than a new mechanism.** *No canonical state — nothing entering a state hash, a roster hash, a chained event body or the next hand's genesis — may be derived from a quantity that can differ between honest receivers. Canonical state changes only through a chained event that every participant has accepted; everything else is a local view.* Applied here it deletes the derivation rather than repairing it: a seat's status changes only through a chained event, and an abort's `attributed` field is evidence, which under **D-010** already has no automatic consequence — so reading a status from it was never going to be right. The same rule removed `seat_flags` from `roster_hash`, the other route by which a per-receiver status could have reached the genesis. `STATE_MACHINE.md` T46 and Q7 carry the transition half; `PROTOCOL.md` §4.10 already forbade it in terms — *"no receiver may derive a seat's state from it"* — and the defect was that one document said so while another did it.<br><br>**What it costs to fix — and this cell stated that cost wrongly, which is X34.** It read: *"Nothing now marks a seat `Absent` automatically. A seat that goes silent is dealt in again every hand and stalls each one until the hand deadline, until a human sits it out or leaves. That is a real liveness cost, paid deliberately."* Two things in that sentence were false and the third was unreachable. D-012's deletion cost **nothing at all** — marking a seat `Absent` never removed it from `HAND_INIT`'s required emitter set, so the liveness this row believed it was spending had already been gone for every pass — the stall was not a repetition but a **fixed point**, since every stack is restored on abort so no seat busts and no end condition fires, and the escape *"until a human sits it out or leaves"* had the silent seat as its grammatical subject, because `PLAYER_SIT_OUT` and `PLAYER_LEAVE` are single-writer by the seat itself. The mitigation for **that** is **D-013**, and it is X34's row.<br><br>**Residual.** D-012 closes the derivation, not the silence behind it — that is X7 — and the permanent liveness loss it was believed to have caused is X34. |
| X34 | **The table makes no progress, ever, and every peer's view of it is valid.** A seat goes silent. Nothing marks it absent — D-012 forbade the derivation that used to — so it stays a seat like any other, it is dealt in, being dealt in makes it a **required emitter** of the next hand's opening collective stage, it emits nothing, the stage stalls to the hand deadline, the hand aborts, and under **D-010** every stack is restored *including the silent seat's*. The next hand begins from the state the last one began from, bit for bit. No seat can bust, so no end condition can fire, so nothing terminates it | **DNA** | **This row is the correction of a cost, and that is why it is catalogued rather than footnoted.** X33's cell, and the D-012 preamble, priced D-012's fix as *“a silent seat is dealt in every hand and stalls each one to the hand deadline until a human acts”* — a slow table, paid deliberately. Three things in that sentence were wrong, and each is worse than the last. **First**, marking a seat absent never removed it from the required emitter set in the first place, because that set is defined to include occupied seats that are absent or sitting out; so the liveness D-012 believed it was spending had already been gone for every prior pass, and the deletion cost nothing at all. **Second**, the repetition is not a repetition, it is a **fixed point**: restoration on abort is total under D-010, so stacks never move, so no seat ever busts, so `\|dealt_in\|` never falls and no tournament-end condition is reachable. The table does not play slowly. It does not play. **Third**, the escape clause had the *silent* seat as its grammatical subject — `PLAYER_SIT_OUT` and `PLAYER_LEAVE` are single-writer by the seat itself — so “until a human acts” named the one party who by hypothesis is not there. There is no operator, no majority and no third party with a lever on this state.<br><br>**Attacker capability required: none beyond silence**, which X7 concedes to everyone at zero cost, and which is indistinguishable from a partition or a closed laptop. **DNA rather than V**: the stall is plainly detected — every peer watches the same stage fail to complete, at the same index, every ten minutes — and there is nobody to attribute it to, because the seat that went quiet did nothing a dead network would not have done. It is the second kind of DNA §5.1 names, like X33: the design has no name to offer rather than a culprit escaping one.<br><br>**The mitigation is D-013, and it removes the question rather than answering it.** *Liveness is inherited from the chain, not from a seat's status*: a seat is required to emit in hand `k+1` only if it signed at least one chained event during hand `k`, and for the first hand the required set is the signers of `TABLE_READY`. `PROTOCOL.md` §3.2 owns the set and its notation and this cell does not restate either. The circularity D-012 left is what this cuts — a status may change only through a chained event, and a silent seat emits nothing, so under D-012 nothing could ever change its status and it was required forever. Asking about participation instead of status has no such loop, because participation is a function of the accepted chain.<br><br>**What it costs, stated as a cost.** A silent seat stalls exactly one hand — the one it went silent in — and is skipped thereafter; it keeps its seat and its stack and the blinds eat it, which is D-005 unchanged; it now genuinely busts, so the tournament can end. It rejoins by signing a chained event, which requires it to be alive, and **nothing else can put it back** — §7.3(b) said otherwise for four passes and is corrected. No certificate, vote, proof or attribution is involved, deliberately: this is not the D-006 to D-008 machinery, which D-010 made inert and `OQ-F` still questions.<br><br>**Residual, and it is the whole of the next two rows.** D-013 anchors the required set on two things it did not previously depend on — the genesis the set is derived from (**X35**) and the accepted chain prefix the set is computed over (**X36**) — and both were per-receiver quantities when it landed. That is D-012's third shape applied to D-012's own successor, on the pass immediately after it, which is the strongest evidence this catalogue holds for the shape being real. |
| X35 | **Two honest players who join thirty seconds apart cannot verify each other's `TABLE_READY`, and the table never forms.** The founder's table advertisement is re-broadcast periodically and each re-broadcast must carry a strictly greater timestamp, so *which re-broadcast reached a joiner* changes the bytes that joiner hashes. That hash entered the table's genesis, and therefore its session identifier and every proof context derived from it, so two joiners who saw different re-broadcasts of the **same table** derived different genesis values. Every signature each of them produced was valid — against its own genesis, and no other | **DNA** | **No attacker, and no unusual timing.** Thirty seconds is a normal gap between two people clicking join on the same lobby row; the re-broadcast interval is the only quantity that has to elapse. The founder is honest, both joiners are honest, no message is dropped, modified or replayed, and the failure is total: the table cannot reach its first hand at all. This is the same class as X33 and X34 — canonical state derived from a quantity that differs between honest receivers, **D-012's rule** — and it is the third instance found in three consecutive passes, which is the argument for the rule being general rather than three patches.<br><br>**Why it went unseen for as long as it did, which is the part worth carrying.** The field was **carried and never checked**. `TABLE_READY` transported it, no receiver rule compared it against anything, and the divergence surfaced only downstream as a signature that would not verify — with nothing to point at the cause. A field that is hashed into the genesis but validated nowhere gives an implementer no failing assertion to read, only a table that will not start.<br><br>**DNA, and by the second of §5.1's two routes.** The failure is loudly detected — verification fails at every peer, immediately — and there is nobody to name, because both parties are honest and the divergence is a property of the specification. What the bucket records here is that the design had no name to offer, not that a culprit escaped one.<br><br>**The attacker-bearing half of the same field, catalogued here rather than as its own row.** Because nothing checked the value, nothing forbade the founder **re-signing a later advertisement with different table parameters** — a different blind schedule, a different starting stack — and forking the table between early and late joiners deliberately. That variant is D&A-shaped and would ordinarily earn its own row; it does not get one because it is not separately mitigable. One change closes both, and splitting them would imply two fixes exist.<br><br>**The mitigation is D-013's J1 rule.** The advertisement hash is removed from the genesis, the session identifier and the proof context, and replaced by a hash over the **agreed table parameters** and nothing else — the construction, its domain and its exact part list are `PROTOCOL.md` §3.1's and are not reproduced here (D-011 rule 1). Every joiner sees the same parameters whichever re-broadcast reached them, because the parameters are what they agreed to and the timestamp is not. The malicious-founder half closes as a side effect and, more to the point, stops being carried-and-ignored: a changed parameter now changes the hash, and the value is **checked** at the three sites `PROTOCOL.md` names.<br><br>**Residual.** The construction excludes the two time fields **by name** rather than by a general rule about mutable fields, so a future field added to the advertisement that varies per broadcast would re-open this row exactly. That is an implementation obligation in the class of A12 and A14, not a property of A1–A7, and it is the standing reason this row is not simply struck. |
| X36 | **Both peers finish the tournament alone, and each one's client tells its player it won.** The set that decides who must emit in the next hand is agreed between honest peers on every path where it does nothing, and per-receiver on the one path where it narrows — a hand that ended in an abort. Two peers that once disagree about who is dealt in each reject the other's copy of the next hand's opening event, so from that hand on each holds a set containing only itself; every collective stage is then self-completing; each plays out the drain path alone, busts the opponent it can no longer hear, and reaches a genuine tournament-won condition naming itself. **One dropped frame is enough.** No signature fails, no invariant fires, no deadline expires, no equivocation predicate matches, and chip conservation holds — separately and correctly — on both sides of the fork | **DNA / NP&ND** by case (see the disposition at the end of this cell) | **This row is why the NP&ND bucket exists, and the classification is the finding.** Every other row in this catalogue ends with somebody knowing something went wrong. This one does not. Each peer's chain is internally consistent, self-verifying and complete; each terminates normally; and the only object that would expose the disagreement — a cross-peer comparison at a checkpoint — is not placed on the hands this failure runs through, because a hand with one dealt-in seat reaches settlement without a deck commitment or a betting round, and those are what the checkpoint list is keyed on (`PROTOCOL.md` §6.2). **The tournament result itself is not checkpointed.** So the promise the divergence-recovery machinery makes — that the next checkpoint shows two differing values and the reconciliation runs — has no next checkpoint in the one regime where it is needed, and the regime is not exotic: a silent opponent produces on the order of forty consecutive such hands.<br><br>**Attacker capability required: none.** Not silence, not a Sybil, not a modified client — one lost frame at one stage of one aborted hand. The two peers are honest throughout and each behaves exactly as specified.<br><br>**Why the argument that was offered does not hold, since the same inversion could be made again.** The set is justified by the claim that the terminal stage of a chain is witness-independent, *so* two peers that reach it accepted the same prefix. That **inverts** the property. Witness-independence was introduced precisely so that peers holding **different** prefixes can still reach the terminus — that is what unfroze the stalled hand in the first place — so reaching it together is evidence of nothing about the prefixes. The narrowing path is the abort path, and the abort path is exactly where the prefixes may differ: the set is agreed where it is inert and per-receiver where it acts. **This is D-012's third shape for the third consecutive pass**, and this time on the fix that closed the second.<br><br>**A second undefined quantity decides which of two failures occurs**, and it is undefined in both owning documents, each deferring to the other: whether a peer's own emission counts toward its own participation record. If it counts, the outcome is this row. If it does not, the outcome is a table that pauses instead — bad, but observable. An implementer choosing by taste picks between a visible stall and a silent double winner.<br><br>**Mitigation: none. This row is open, and it is a blocker.** It is `DECISIONS.md` **K-1**, and the choice belongs to the owner rather than to any document's editor, because both candidate fixes give something up: ratify the stalled stage, which trades this failure for a stall that has to be lived with; or floor the required set at two members, which keeps tables alive and makes the last two seats mutually hostage. **`DECISIONS.md` K-3 is the cheaper half and is independent of that choice**: place a checkpoint after any hand that placed no other, and the fork stops being silent — it becomes a faulted table, which moves this row from **NP&ND to DNA** without deciding anything. That is the shape of fix this bucket asks for: not a lower probability, an observable failure.<br><br>**Until then, this document may not claim that every divergence between honest peers is at least detected.** §9.3's standing caution carries that retraction, and §5.4's sentence that no row occupies the worst bucket is withdrawn.<br><br>**DISPOSITION — K-1 and K-3 are both decided, and this row splits rather than moves whole.** The paragraphs above are kept as the record of the finding; what follows is the answer. **K-3 landed**: every hand now places checkpoint 8 at its boundary, including a drain hand, so the regime that had nothing comparable in forty consecutive hands now emits forty comparable values (`STATE_MACHINE.md` I32). **K-1 landed, and by detection rather than by prevention**: neither candidate fix in the paragraph above was taken. Flooring the set at two members was rejected because it deletes the drain — nobody busts, no end condition fires, and the last two seats are mutually hostage, which is the same fixed point from the other side. What was adopted is the **solitary-stage rule** (`PROTOCOL.md` §3.2; engine half `STATE_MACHINE.md` T62, T63, §9.3 condition 0.6, I33): a peer whose required emitter set has narrowed to one member may still deal, but a chained event of that hand from a seat outside the set is a **state divergence and not a rejection** — it freezes, completes no stage, awards no pot and evaluates no end condition, and the table then closes rather than thawing back into the same regime. **And the sub-question the paragraph above says is undefined in both documents is answered**: a peer's own emission **does** count into its own participation record, so the outcome is this row's and not a pause — which is why the rule had to be a detection.<br><br>**The two cases the split names.** **(a) K-1's own trace — one dropped frame, both peers transmitting throughout — is now DNA.** The peer that did not narrow keeps emitting; its first chained event of the first solitary hand reaches the peer that did, and that peer freezes on it. The failure is detected, play stops, and nobody is named — which is DNA's definition and its second kind, an honest divergence with no adversary in it. The detection is **unilateral**: it needs no reply, no quorum and no cooperation from the peer that is still emitting, so it holds even if that peer never speaks again. **(b) A *bidirectional* partition remains NP&ND**, and it is stated rather than folded into (a): if neither peer hears the other for the whole drain, both drain, both finish, and no contradiction reaches either. That residual is what D-013's drain does under a partition **however the set is defined**, it is indistinguishable at both peers from the case the drain exists for, and there is no wire evidence to separate them because by construction no wire carries anything. It is not a consequence of this row's defect and no repair of this row removes it.<br><br>**So the sentence this document owes is narrower than the one it withdrew, and narrower than "fixed".** It may now claim that a divergence between honest peers is detected **wherever either peer is still transmitting**, and it may not claim it under a total bidirectional partition. `Q-10` in `PROTOCOL.md` stays open and is labelled there as unclosable by ratification: agreeing who contributed to the stage that stalled needs a collective step at exactly the point collectivity failed. |
| X37 | **An honest player is ejected from the table by the anti-cheat, for an action that was legal when they took it.** D-014 removes a player who sends a provably illegal message: the hand is voided, the seat is dead and blinded off, and **the exit is one-way** (`STATE_MACHINE.md` T64, T65, I34). The attack — and the accident — is to make an honest player's legal message *look* illegal at somebody else's validator. It needs no forged signature and no modified client on the victim's side: it needs the accuser's view of the game state to differ from the victim's at the moment of judgement, which is a thing this corpus has produced **without any adversary at all** in four consecutive passes (X33, X34, X35, X36) | **DNA**, and the bucket is the finding | **This is the risk D-014 creates in its own right, and it is why that decision has two tiers rather than one.** D-010 banned automated eviction after four passes in which every severe defect ended the same way — an honest peer's chips forfeited and its key blocked, for following the protocol; the *forfeit* counts across those reports were 6, 5, 14 and 12. D-014 does not re-open that: it removes nobody on a timeout, a vote, a certificate, an `attributed` field or an equivocation predicate, and each of those is a judgement two honest peers can reach differently. What it **does** do is make the correctness of every validator load-bearing against a *person* for the first time: before D-014 a validator that was too strict rejected a message, and after it, the same validator ejects a player.<br><br>**How an honest legal action comes to look illegal, concretely, and it is not hypothetical.** Take **K-1**: two honest peers hold different required emitter sets after one dropped frame, therefore different `dealt_in`, therefore different `bb_seat` and different blind positions. Every action the victim takes is legal against the victim's state and out of turn against the accuser's. Under a tier-2 removal with no precondition, the peer whose state drifted ejects the peer whose state is right — and neither can tell which is which, because that is precisely what a divergence is. **A divergence like K-1 is the mechanism, and the mechanism already exists in the corpus.**<br><br>**Mitigation, and it is a precondition rather than a mitigation.** D-014's **tier 2**: illegality decidable only against game state — out of turn, below the minimum raise, larger than the stack, a showdown claim contradicting the board — may produce a removal **only** once the accused's message is judged against state fixed by a **checkpoint both peers have signed**. Before that point the violation is recorded and the hand is voided, and **nobody is removed**. That is exactly the K-1 case defused: a checkpoint both peers signed is a state they agreed on, so a divergence that arose after it is bounded by the events since it, and a divergence that arose before it cannot have been signed by both. `STATE_MACHINE.md` T64's guard carries the precondition as a conjunct — `judged_at_checkpoint == Some(n)` and checkpoint `n` reached agreement — rather than as prose, which is what makes it testable.<br><br>**What is not mitigated, stated rather than hidden.** Tier 1 needs no checkpoint and correctly needs none — a signature that does not verify, a non-canonical encoding, a failed shuffle proof are decidable from the message alone, and an honest peer cannot be framed by one without forging its signature. **But "tier 1" is a claim about a *validator*, not about a message.** A validator with a bug — a canonicalisation rule read one way here and another way there, an encoder that emits a form the decoder rejects, a proof verifier stricter than the prover — turns an honest message into tier-1 evidence, and tier 1 has no checkpoint to wait for. **`DECISIONS.md` C-1 to C-3 are three live instances of exactly that class**: the shipped `src/protocol/` signs a digest where §2.4 signs a domain-prefixed body, and its domain register matches §2.8 in **no** string. Two implementations disagreeing that way would each remove the other on sight, and each would be right by its own rules. That is why the gate below blocks shipping.<br><br>**The gate D-014 names, and this row is where this document records it: under every legal interleaving, no honest peer is ever evictable.** For every legal action an honest client can emit, under every legal interleaving of the protocol, no `CheatProven` may be producible against it. It is the **mirror** of `SPEC_CS.md` §25's cheater list — §25 asks that every named cheat be caught, and this asks that nothing else be — and it is the same shape as D-009 rule 1's mirror test, which was written after a rule was re-derived wrongly five times. It is `DECISIONS.md` **D-014-2**, §5.5 is where it is mapped beside the eleven §25 peers, and it **blocks shipping the feature, not writing it**. A removal mechanism whose mirror test has not been run is a mechanism whose failure mode is the one D-010 spent four passes deleting. |

**Extended catalogue counts: CP 6 · D&A 8 · DNA 6 · NP&ND 1 (X36) · V 1 (X8) ·
OOS 12 · split D&A/DNA 1 (X7) · total 35.**

Two rows moved and five were added across the last three passes. **X8 moved from
D&A to V** under D-010, and no row moved in the other direction or into CP.
**X32** was added by the Phase 2 gate (G1) and closed by D-011 rule 2. **X33** was
added by the second Phase 2 gate (H1) and closed by D-012. **X34, X35 and X36 are
new**, added by the Phase 3 gate: X34 is the corrected cost of D-012's own fix and
is closed by D-013; X35 is the genesis derived from a per-receiver lobby view and
is closed by D-013's J1 rule; **X36 is open and is a blocker** (`DECISIONS.md`
K-1). Like X30 and X31 before them, all five are defects the corpus was carrying
rather than attacks the design newly answers, so they enlarge the denominator.

**Three facts about these five are worth reading together rather than row by
row.** First, X33 was the first row here classified **DNA** for a reason that has
nothing to do with an attacker, and X34 and X35 are the second and third: the
peers behave honestly and the divergence is created by the specification, so
there is nobody to name. Second, **none of the last three requires any attacker
capability at all** — silence, a thirty-second gap between two joins, and one
dropped frame. The catalogue's centre of gravity has moved from what an adversary
can do to what the corpus does to itself. Third, **X36 is the first row in the
history of this document to be neither prevented nor detected**, and §5.1 gained
a bucket rather than stretching one to hold it.

**X11 is retired.** Its claim — that unanimity structurally excluded the race
between a late action and a timeout certificate — was false, and its substance is
folded into X10. The number is not reused, so that references to X11 elsewhere
resolve to a deletion rather than to a different attack.

### 5.4 Totals

| Bucket | §17 catalogue | Extended | Combined |
|---|---:|---:|---:|
| Cryptographically prevented (CP) | 11 | 6 | **17** |
| Detected and attributed (D&A) | 7 | 8 | **15** |
| Detected but not attributable (DNA) | 0 | 7 | **7** |
| Split D&A / DNA by case (X7) | 0 | 1 | **1** |
| **Split DNA / NP&ND by case (X36)** | 0 | 1 | **1** |
| **Visible, not prevented (V)** | 0 | 1 | **1** |
| **Not prevented and not detected (NP&ND)** | 0 | 0 | **0** |
| Out of scope (OOS) | 1 | 12 | **13** |
| **Total** | **19** | **36** | **55** |

Counted after the Phase 0 review corrections: X11 removed from CP, X29 added to
DNA, X7 reclassified from D&A to the split row. Then after the Phase 1
verification: **X30 added to D&A** (the manufactured single-voter certificate,
closed by D-008), taking the extended catalogue from 28 to 29 and the combined
total from 47 to 48. Then after the second verification pass: **X31 added to
D&A** (an honest voter's own required votes turned into an `EquivocationProof`
against itself, closed by D-009 rule 1), taking the extended catalogue from 29 to
30 and the combined total from 48 to 49. Then **D-010**, which moved no total at
all and moved exactly one row: **X8 from D&A to the new V bucket**, because the
mechanism that made it D&A — automated forfeiture — is deleted. Recording that as
a movement rather than as a re-wording is the point of this paragraph; the count
of D&A rows fell by one and the design did not get better at X8, it stopped
answering it. Then the **Phase 2 gate**: **X32 added to D&A** — the terminal
abort colliding with its own emitter's contribution, closed by D-011 rule 2 —
taking the extended catalogue from 30 to 31 and the combined total from 49 to
50. Then the **second Phase 2 gate**: **X33 added to DNA** — a per-receiver
`attributed` field read into the next hand's `dealt_in`, closed by D-012 — taking
the extended catalogue from 31 to 32 and the combined total from 50 to 51. It is
the first row added to DNA rather than to D&A since X29, and the reason is worth
stating in the counts: it needs no attacker, so there is no one for the transcript
to name. Then the **Phase 3 gate**, which added three at once and is the largest
single movement this table has recorded: **X34 to DNA** — the corrected cost of
D-012's fix, a table that makes no progress ever, closed by D-013; **X35 to
DNA** — a genesis derived from a per-receiver lobby view, closed by D-013's J1
rule; and **X36 to a bucket that did not exist**, taking the extended catalogue
from 32 to 35 and the combined total from 51 to 54. Then this pass, under **D-014** and the
dispositions of K-1, K-3 and K-9: **X37 added to DNA** — an honest player ejected by an anti-cheat
whose validator is too strict, which is D-014's own new risk and not an attack the design inherited
— taking the extended catalogue from 35 to **36** and the combined total from 54 to **55**; and
**X36 split rather than moved**, from NP&ND to **DNA / NP&ND by case**, because the solitary-stage
rule detects K-1's own trace — one dropped frame, both peers still transmitting — while the
bidirectional-partition residual is genuinely undetected and no repair of X36 removes it. **The
NP&ND column is at zero again and the sentence that no row occupies it is *not* restored**: a row
half-occupies it, the half is named in X36's cell, and a bucket that is empty by a split is not the
same claim as a bucket that is empty because nothing reaches it. Two further notes on the counts.
**D-014 moved no row between buckets**, which is the honest way to report it: it adds a payoff
under four D&A and CP labels without changing any label, exactly as D-011 rule 3 removed one. And
**X37 is the first row this catalogue has ever carried whose attacker is the design's own defence**
— every other row is something the design must survive, and this one is something the design does.
Recording it as a row rather than as a caveat inside D-014 is the same discipline that gave X8 the
V bucket rather than a footnote. **D-011 rule 3 moved no row into a different bucket at all**, and that is
the honest way to report it: it deleted a payoff that four rows carried, which
shrinks the worst case underneath four labels without changing any label. One
row counts in exactly one line of this table; X7 has its own line because its
class depends on the number of simultaneously silent seats and on `|V|`, and
forcing it into either bucket would overstate one case.

None of X30 to X36 makes the system safer than it was believed
to be: all seven are defects that successive review passes found in the corpus
itself, and **X37 is the eighth and the first that a decision created on
purpose** — it is the price of D-014 rather than a mistake in it, which is why it
is catalogued at the moment the decision lands instead of at the pass that would
otherwise have found it, each worse or more reachable than the one before, and each is counted here
as an attack the design has to answer rather than as a feature. The pattern the
counts do not show is that the first three are two failures wearing three faces —
a rule scoped on a quantity the attacker controls (X30), and a slot key coarser
than the behaviour the protocol mandates (X31, X32) — which is why D-008, D-009
and D-011 are written as general rules rather than as three patches, and why
D-011 rule 2 replaces the general rule with a single literal definition after the
general rule was re-derived wrongly five times.

**X33 is a third failure and not a fourth face of the first two**, which is why
D-012 is a new rule rather than a sixth application of an old one. Nothing about
it is a drifted copy or a scoping mistake, and its consequence is not a payoff
that made an attack worth mounting — there is no attacker to reward. It is the
shape a *fix* leaves behind: the terminal stage was made witness-independent to
unfreeze eleven phases, that change made "which copy of the abort did I accept" a
per-receiver fact, and a line written when it was not per-receiver went on reading
it. The lesson for a reviewer is procedural and belongs in the counts as much as
in a row: **after a fix, re-read the paths the fix newly made load-bearing, not
only the rule that was written.** The rule that was written is almost never where
the next defect is.

**X34, X35 and X36 are that lesson holding three passes running, and the third
time on the fix that closed the second.** X34 is D-012's own cost model, stated
wrongly at the moment D-012 was written and corrected only when a later pass
walked the path instead of re-reading the rule. X35 and X36 sit on paths **D-013**
newly made load-bearing: D-013 anchors the required emitter set on the genesis
(X35) and computes it over the accepted chain prefix (X36), and neither quantity
had to be agreed between honest receivers before it did. The corollary a reviewer
should take from three consecutive instances is sharper than the original lesson:
**the pass that fixes a defect is the pass most likely to create the next one**,
so a fix is not finished when the rule reads correctly — it is finished when the
paths it newly made load-bearing have been re-read against every rule that now
depends on them.

Read the CP column with A3 and A4 in mind. Rows 1–4 of the §17 table — four of the
seventeen CP entries, and the four that matter most to the integrity of the deck —
rest on the soundness of an unaudited 1779-line implementation of Bayer–Groth. If
that review fails, those four move to **NP&ND**, the worst bucket in the scheme.
**That bucket was occupied by X36 and the sentence that no row occupies it was
withdrawn; it is still withdrawn, and the reason has changed rather than
disappeared.** X36's split leaves the bucket at zero rows and **half** a row: the
case in which either peer is still transmitting is now detected (DNA), and the
case of a total bidirectional partition is not, and is not repairable by anything
this row could do. The two cases must not be read as the same, and the difference
runs the wrong way for comfort: rows 1–4 would fall into NP&ND *if an assumption
fails*, whereas **X36's residual is there now, under the assumptions exactly as
stated, and needs no attacker to get there**. What may be claimed after this pass
is the narrower sentence X36's cell states: a divergence between honest peers is
detected **wherever either peer is still transmitting**. What may not be claimed
is the sentence that was withdrawn.

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
| `CheaterEquivocation` | §5.2 row 16; X31; X32; G7 | `tests/adversarial/equivocation.rs` | **D&A** — two chained events by one key in one slot, with different `event_hash` values, produce a transferable `EquivocationProof`. **[R10]** The slot key is `PROTOCOL.md` §5.2's and is not reproduced in this table; the test binds against §5.2, not against a copy printed here, and a test written against a copy is how three of the five recurrences in G7's table survived review. The test must also assert the **false-positive** half of G7, and this half is not optional: no unchained pair — two honest lobby adverts, or a `JOIN_REQUEST` alongside the sender's own `RNG_REVEAL` — ever produces one. **And it must carry the mirror of the cheater, which is what D-009 rule 1 makes standing:** an *honest* peer, driven through every legal interleaving, never generates a proof against itself. **Three** interleavings must be in the suite by name, one per recurrence that a slot key had to be widened to close. (1) **X31's** — two seats silent at one collective stage, one honest voter emitting a required timeout vote against each — asserting the two votes occupy **different** slots. (2) **The reconciliation one** — a peer that misses an event, reconciles under `PROTOCOL.md` §6.3 step 3, and re-derives its checkpoint value — asserting its first and second emissions do not together satisfy the predicate; this now passes, because §4.9 gives a reconciliation round its own stage and `sequence`. (3) **X32's** — a collective stage stalls, the hand deadline expires, and the honest peer that already contributed to that stage emits the terminal `HAND_ABORT` — asserting that the abort **is accepted** (the liveness half: the hand actually ends) and that the abort and the contribution do **not** together satisfy the predicate (the evidence half). Interleaving (3) fails against any corpus whose slot key omits `event_type`, and it belongs in the suite as the test that pins D-011 rule 2 rather than as one written after the fix. **A suite that tests only the cheater passes while every one of these is live**, which is the whole reason the mirror is mandatory. Note that D-010 and D-011 rule 3 change what a verifying proof *costs* the victim and change nothing about this test: the mirror asserts the proof does not verify, never that its consequences are tolerable. |
| `CheaterReadOpponentCard` | §5.2 row 6; G1 | `tests/adversarial/hole_card_secrecy.rs` | **CP** — a coalition of `n-1` holding every message it legitimately received cannot output the victim's hole cards. *Inherits A1.* |
| `CheaterFutureBoard` | §5.2 row 7; X3; G2 | `tests/adversarial/street_gating.rs` | **CP** for opening a future street's index; **D&A** for the attempt — an early reveal token opens nothing and is rejected and attributed. *Inherits A1, A10.* |
| `CheaterDisconnect` | X7, X8, X30, X32, X33, X34, X36; §7 | `tests/adversarial/disconnect.rs` | **D&A with one silent seat where `\|V(subject)\| >= 2`; DNA with two or more silent seats; DNA wherever `\|V\| < 2`, which at `n = 2` is always** — matching X7's split class. The test must cover all three cases, and under **D-010** the chip assertion is now the same on all three and is the strongest one available: **no chips move and every stack is bit-identical to its start-of-hand value**, whatever the cause and whoever is attributed. The `\|V\| < 2` abort additionally carries `attributed = []` (`PROTOCOL.md` §8.4). A test that asserts forfeiture on any branch is testing a mechanism this version does not have. It must also cover **X30**, because D-008's floor is only as real as the check that enforces it: a client emitting `TIMEOUT_VOTE`s against several seats must **not** thereby shrink `V` — exclusion requires a completed, valid certificate — and a certificate assembled with `\|V\| < 2` must have no effect at **any** seat count, not merely at two. "No effect" is to be asserted at its strongest (D-009 rule 2): the certificate is not chained, produces no `AbortRecord`, moves no chips **and does not end the hand** — the test must show the hand still running afterwards and ending only when `hand_deadline_ms` expires. Under D-010 the reason for that last assertion is narrower than it was and the test comment must say so: the escape it protects is **not** closed any more (X8 is V), so what the assertion buys is that the escape costs the full deadline of visible stalling rather than one signature, and that no chained event names a victim on the strength of one peer's word (§7.3(c)). **And it must assert the liveness half of X32 on the shipped configuration**: with one seat silent at a collective stage and the hand deadline expired, the terminal abort emitted by a seat that already contributed to that stage is **accepted**, the hand ends, and the next hand begins. A disconnect suite that only checks chip outcomes passes on a table that can never start another hand.<br><br>**And it must assert X33's D-012 property, which the X32 assertion above does not reach.** Drive the two abort paths that can end one hand differently — a certificate that names a peer, and the hand deadline that names nobody — and assert that **every peer's next-hand `dealt_in` and `bb_seat` are identical** and that hand `k+1` actually completes its opening collective stage. The failing version of this test passes every chip assertion, every terminal-value assertion and the X32 liveness assertion, and then hangs at hand `k+1`; that is what makes X33 worth its own assertion rather than a comment. Assert it directly at the source too: **no code path reads a seat's status out of an `AbortRecord`**, for `attributed` or for any other field. **Three cases added by D-013, and the last of them is the one this test cannot currently express.** The disconnect suite must show that a silent seat stalls **exactly one** hand and is skipped from the next — not stalled repeatedly, which is **X34**, the fixed point D-012 left behind and D-013 removed; that such a seat is blinded off until it **busts**, since that is what lets a tournament with a silent seat end at all; and that it comes back **only** by signing a chained event, never by reconnecting a socket, which is §7.3(b) as corrected. The third is **X36** and it is open: a test can produce the fork — drop one frame at a stalled stage on an aborted hand — but there is no assertion available that fails, because both peers are internally consistent and no checkpoint compares them. The honest form is a **cross-peer** assertion the harness makes from outside the protocol, comparing the two peers' tournament results directly and failing if they disagree, marked as testing a defect rather than a property until `DECISIONS.md` K-1 is decided. A suite that only asserts each peer's own consistency passes this fork, which is exactly how it survived a gate. |

**What D-014 adds to the *Asserted outcome* column, stated once for the whole table
rather than in eleven cells.** Every outcome above still holds unchanged; each row
gains a second assertion where D-014's tiers reach it, and §5.2's re-classification
table is the mapping. `CheaterDuplicateAce`, `CheaterReplaceCard`,
`CheaterInvalidShuffle` and `CheaterReplayAction` are **tier 1** — the test must now
also assert that the emitter's seat is `Removed`, that the hand was voided with no
chip crossing seats, and that no route back in exists (`STATE_MACHINE.md` I34).
`CheaterIllegalRaise`, `CheaterFakeStack` and `CheaterFutureBoard`'s attempt half
are **tier 2** — the test must assert **both** halves: a removal *after* the
governing checkpoint agreed, and **no removal before it**, which is the half that
catches X37. `CheaterEquivocation` gains nothing and the test must assert that it
gains nothing: D-014 excludes an equivocation predicate by name, and a harness that
removes a seat there has reproduced X31 with a new trigger. `CheaterPredictableRNG`,
`CheaterReadOpponentCard` and `CheaterDisconnect` are outside both tiers —
statistical, information-theoretic and liveness claims respectively, none of them a
message whose illegality any peer can decide alone.

**The mirror of this table, which D-014 makes a gate rather than a nicety.** Every
row above asks *"is this named cheat caught?"* D-014 adds the opposite question and
makes it blocking, because the answer to it is what stands between an anti-cheat
and X37:

> **For every legal action an honest client can emit, under every legal
> interleaving, no honest peer is ever evictable.** No `CheatProven`
> (`STATE_MACHINE.md` T64, T65) may be producible against a client that followed
> the protocol.

It is `DECISIONS.md` **D-014-2**, its planned home is
`tests/adversarial/no_honest_eviction.rs`, and it **blocks shipping the removal
feature, not writing it**. Three properties make it a different test from anything
above rather than a re-run of them. It is **universally quantified over honest
behaviour**, so it cannot be discharged by a `proptest` over legal play that
happens to pass — the generator must enumerate the interleavings, including the
ones only a dropped frame produces. It must be run **across two independent
implementations wherever two exist**, because a tier-1 disagreement between two
correct-by-their-own-rules validators is the failure mode X37 names and a
self-consistent single implementation cannot see it — `DECISIONS.md` **C-1 to C-3**
are three live instances waiting in `src/protocol/`. And it must cover **tier 2 at
the boundary**: an action that is legal against the last agreed checkpoint and
illegal against a state that drifted after it must produce no removal, which is
the K-1 shape and is the precondition's whole purpose. This is the same shape as
D-009 rule 1's mirror test, and that one was written only after the rule it guards
had been re-derived wrongly five times.

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
and no reputation with teeth. Note the direct consequence, and note that **D-010
makes it load-bearing rather than incidental**: the "reputation penalty" for
repeatedly aborting hands is only as strong as the cost of a new identity, which
is zero. Under D-005 there were three sanctions: forfeiture, transport-layer
block-listing, and visibility. **D-010 deleted the first, D-011 rule 3 deleted
the second**, and the second was still live at the transport layer when D-010's
own sweep declared the job done. So the visible, attributable record of
aborts and the per-identity abort count in the lobby are now the **entire**
sanction against X7 and X8, and they are a sanction only in the sense that other
players may decline to sit with someone — a user decision, never a protocol
action. `SPEC_CS.md` §18 explicitly forbids claiming more, and OQ9 asks the
honest question this raises: whether a visible abort record is a meaningful
sanction at all when a fresh identity is a keypair away, and if it is not, saying
so in the UI rather than implying a reputation system exists. Two limited mitigations are worth noting without overstating them: the DHT
roster ties identities to IP addresses (§8), which is itself a privacy cost rather
than a feature, and a table's participants can choose whom they seat.

**Lobby chat.** `LOBBY_CHAT` (`PROTOCOL.md` §4/§7) carries **no security claim of
any kind**. It is signed by the sender's application key, which says only which key
emitted the bytes; it is not authenticated as coming from any particular *player*
beyond that key, and impersonation by `display_name` is trivial and expected —
identity is free (§6, Sybil), and a display name is a display string, never an
identifier. Chat text is never parsed, never an identifier, and never an input to a
state transition. It is the one message class that is pure attacker-controlled
UTF-8 reaching a UI, and the sanitisation that must therefore be applied to it —
which code points are rejected, and where the rule is stated at the message
rather than inherited — is **[R14]** `PROTOCOL.md` §7.6 and §9.4's, not this
document's. The code-point ranges this paragraph used to list are deleted under
D-011 rule 1: a threat model that prints a validation table invites an
implementer to code against the printed copy. What is classified here is only
that chat text is **never parsed, never an identifier, and never an input to a
state transition**, so a failure of that sanitisation is a UI defect and not a
game-integrity one. Spam and flooding are X15.

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

**Denial of service.** Explicitly out of the protocol's reach (§17 row 19). One
poker-specific aggravation deserves naming rather than being folded into a generic
"DoS is out of scope": the DHT roster is exactly the target list an attacker needs
to find the victim's IP in the first place (§8). **A second one has been deleted
by D-010 and is recorded here as deleted rather than quietly dropped**: under
D-005, an attacker who knocked a player offline at the moment they faced a large
bet collected that player's forfeited commitment (X9). There is nothing to
collect now — the hand aborts, every stack is restored — so timing a network
attack to a betting moment buys a wasted hand and nothing else. The capability is
unchanged and still out of scope; the *reason to use it against a poker table
specifically* is gone.

**Deliberate disconnection.** Covered in full in §7. A player can always force a
hand to abort by going silent at the worst possible moment, and this cannot be
prevented without breaking hole-card secrecy. **Under D-010 it also cannot be
punished**: the abort is neutral, so the player who disconnected to escape a
losing pot keeps their chips. That is X8, it is classified as visible and not
prevented, and it belongs in this section as much as in §7 — it is the one
in-protocol attack this design sees perfectly, records permanently, and does
nothing about.

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
**D-008**, which scope every certificate rule on `|V|` rather than on `n`, and
with **D-009 rule 2**, under which a certificate below the `|V| >= 2` floor is
inert everywhere and at every table size:

**And read all three under D-010, which changes the ending of every one of
them.** D-005's requirement that the game continues and that the absent seat is
blinded off is untouched, and cases (a) and (b) below are unaffected. What D-010
revises is D-005's *chip rule on abort*, which case (c) turns on: **an abort is
neutral.** Stacks are restored to their start-of-hand values, in every case, for
every cause, at every table size, whoever is attributed. The three-way split in
case (c) below therefore no longer distinguishes three chip outcomes — there is
one — and survives only as a statement about whether the transcript can name
anybody. Attribution is evidence, and nothing acts on it.

**And read all three under D-011, which changes two further things.** Rule 3
extends "nothing acts on it" from the engine to **every layer**, transport
included: no abort attribution, no certificate and no proof causes any part of
this system to disconnect, refuse, unseat or block-list anybody. Rule 2 is what
makes case (c)'s terminus reachable at all — until the slot key included
`event_type`, the abort that ends a stalled hand collided with its own emitter's
earlier contribution and could not be accepted (X32). Neither rule changes what
anybody keeps; the first shrinks what an abort can cost a peer, and the second
is the difference between a hand that ends and a table that stops.

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
whose voter set cannot be shrunk by assertion — under a rule our own clients
enforce rather than under A1–A7 (D-008, X30).

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

**(b) The player is not participating between hands.** From the next hand onward
they are simply **not a party to the cryptography**: not in the joint key, not
dealt cards, and nothing waits for them. Their seat is a chip pile that pays its
blinds and antes and folds when the action reaches it, draining one orbit at a
time until it busts. No new cryptography is needed, because a player who is never
dealt in never has to open anything.

**Two things in this case changed under D-013 and the previous wording was wrong
about both.** First, *who this case applies to*: it is not a seat carrying an
"absent" status, because no status governs this any more. It is a seat that
**signed no chained event during the previous hand**, and that is a fact about
the accepted chain rather than a fact about anybody's view of a player.
`PROTOCOL.md` §3.2 owns the definition; this document does not restate it. What
this buys is X34: the seat is skipped rather than waited for, so the table keeps
playing, and because it is blinded off it genuinely busts, so the tournament can
end.

**Second, and this is the sentence that was false: they do *not* rejoin
automatically.** This paragraph read *"On reconnect they rejoin the key-holder
set at the next hand boundary"* for four passes. Nothing rejoins anybody. Under
D-013 a seat returns to the required set by **signing a chained event**, which
requires it to be alive and willing — that is the entire re-entry test, and it is
deliberately the only one. A client whose socket reconnects has demonstrated
nothing; a client that signs has demonstrated the only thing this design asks
for. The transition and the message that carries it are `STATE_MACHINE.md`'s and
`PROTOCOL.md`'s respectively.

**The residual on that re-entry path is open and is recorded rather than
smoothed.** `DECISIONS.md` **K-3b** observes that the phases in which the
re-entry message can be accepted are a zero-width derived phase and a phase that
requires every seat to be silent, so the test D-013 defines may have no window in
which it can be taken. If that holds, a reconnecting player keeps their seat and
their stack and is blinded off correctly — case (b) is sound — but cannot come
back, which converts a temporary disconnection into a permanent one. That is a
liveness defect, not an integrity defect, and it is owned by `STATE_MACHINE.md`.

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

   **Under D-010 all three cases below have the same chip outcome — every stack
   is restored — so what follows distinguishes only what the transcript can say,
   not what anybody keeps.**

   * *one silent seat, with `|V(subject)| >= 2`* — the abort carries a signed
     record of which peer failed to publish. That record is **evidence and
     nothing else** (D-010 point 2): no forfeiture follows from it, no eviction,
     no block list. It is what a human, or a later version of this protocol,
     would adjudicate on. **D&A**, where the two letters now mean *detected and
     named*, not *detected and answered*.
   * *two or more seats silent simultaneously* — no certificate can be built,
     because each required voter set contains the other subject and D-008 allows
     a seat to leave `V` only once a completed certificate has named it. The
     behaviour is to abort with `attributed = []`, `cert_hash = None` and stacks
     restored: nobody is named, because a seat that did not vote may be silent,
     partitioned or simply slow. Under D-005 the reason for not naming was that
     forfeiting a partitioned honest seat's chips would be a positive-gain attack
     on an honest player; **under D-010 that reason is subsumed by a stronger
     one** — naming costs the named seat nothing anyway, so there is no case at
     all for guessing. It lets two colluding seats void a hand for free, which is
     now the same thing one seat can do alone (X8). `PROTOCOL.md` Q-02 and
     `STATE_MACHINE.md` Q3 stay open; OQ-E below is blocking, for the shape of
     the abort rather than for its chip disposition. **DNA.**
   * *any silent seat where `|V(subject)| < 2`, which at `n = 2` is always* — no
     deadline is enforceable at all (D-007, generalised to `|V|` by D-008). **A
     `kind = 2` certificate below the floor does not end the hand and has no
     effect of any kind** (D-008 point 2, made unconditional by **D-009 rule
     2**): it is not chained, it is not evidence, it produces no `AbortRecord`
     and no forfeiture, and an honest client silently ignores it. An earlier
     revision of this bullet said such a certificate "may still end the hand,
     because otherwise a vanished opponent deadlocks the table forever". **That
     sentence is withdrawn**, it contradicted this document's own X10, and the
     liveness argument behind it is not one this design accepts: `SPEC_CS.md`
     §19 ranks security above conveniently finishing a hand.

     The hand still ends, but by a different carrier and on a different clock.
     It ends when the **hand deadline** expires, as a local timer expiry that
     every peer reaches from the same signed `HAND_INIT` and the same relative
     duration, needing no voter set, no certificate and no unanimity. **[R15]**
     The two durations, and the body the resulting abort carries, are
     `PROTOCOL.md` §8.4's and §4.10's; the values and the field list this bullet
     used to print are deleted under D-011 rule 1. What matters to the
     classification is the ratio, not the numbers: the hand deadline is more
     than an order of magnitude longer than the crypto-step deadline, so the
     escape costs the attacker a long, visible stall rather than a message.

     **This is the terminus X32 broke, and it is why that row exists.** The
     terminal abort is the only thing that ends a hand here, and until D-011
     rule 2 it was written into the stalled stage's own slot — where the honest
     peer emitting it had already signed a contribution. Its own abort was
     therefore rejected, the hand could not end, and the pair verified as an
     `EquivocationProof` against the peer that tried to end it. Heads-up, which
     is the shipped mode and the permanent instance of `|V| < 2`, there was no
     peer able to emit it at all. **Read this bullet's liveness claim as
     conditional on `event_type` being in the slot key** (`PROTOCOL.md` §5.2,
     D-011 rule 2); it is not an independent guarantee.

     **Why the difference is a security property and not a detail about
     timing.** The chip arithmetic is identical either way, which is what made
     the contradiction easy to overlook; what is not identical is who can
     trigger it and at what cost. Under the withdrawn reading a heads-up player
     facing a losing pot emits **one self-signed certificate** naming the
     opponent as subject and the hand ends immediately with stacks restored —
     an on-demand hand-void button, available at will, at no cost, and
     unfalsifiable, because `PROTOCOL.md` §8.3 concedes there is no artefact
     that settles the
     race when a voter lies about what it saw, so nothing establishes that any
     deadline ever passed. Under the inert reading the same player must actually
     stall the table for the full ten minutes: a slow, visible escape that costs
     the attacker its own time and leaves the missing contribution plainly in
     the transcript.

     **D-010 weakens this argument and it is restated at its new strength rather
     than left standing at its old one.** The sentence that used to close this
     paragraph read: *"That is exactly the free in-protocol escape D-005 exists
     to close (X8), reopened in one message."* D-005 no longer closes that
     escape — D-010 reopened it deliberately (X8, classified **V**) — so
     inertness is **not** what stands between a losing player and their chips
     back. What inertness still buys is real but much smaller, and it is only
     this: the escape costs ten minutes of visible stalling instead of one
     message, so it is legible in the transcript and counts against the
     identity in the lobby, which are the only two mitigations X8 has. The
     length of `hand_deadline_ms` is therefore still load-bearing rather than a
     tuning knob, and **no rule anywhere in this corpus may let a below-floor
     certificate shorten it** — but the property it protects is now visibility
     and cost, not closure.

     A second thing turns on it, and it is why this document may not hold both
     readings: an accepted certificate would be a valid chained event **naming
     its victim** as the seat that failed, which is why X30's mitigation can say
     that under D-008 an inert certificate is *not evidence of misbehaviour*.
     That sentence is true only while the certificate is genuinely inert. And
     because the two readings differ on an engine's accept/reject decision for
     an event a modified client can emit at will, two conforming clients would
     compute different `state_hash` values at the next checkpoint and fault the
     table under `cause = 4` — X29, the one outcome this corpus classifies as
     unresolvable.

     The residual is unchanged and is not softened by any of the above, and
     **D-010 generalises it from a corner to the rule**: where `|V| < 2` the
     silent player recovers its commitment ten minutes later with nobody named,
     and everywhere else the silent player recovers its commitment ten minutes
     later *with* somebody named, which is the same recovery. This bullet used
     to describe the one corner of the design where the escape survived; it now
     describes the general case, and the escape is catalogued in its own right
     as X8. **DNA** for attribution, **V** for the outcome, and recorded as an
     unfixed limitation in §9.1 rather than hidden. The lettered question this
     document used to carry here — restore or forfeit where nobody can be named —
     is **answered corpus-wide by D-010**, which takes restoration everywhere, and
     the letter that carried it has been released to `DECISIONS.md`'s series
     (§9.2).

### 7.4 The chips on abort, and the exploit each option opens

The disposition of committed chips is a security decision, not an accounting one,
because each option opens a different attack. There are three, and the design has
now taken two of them in succession.

| Option | Exploit it opens |
|---|---|
| **Restore every stack to its start-of-hand value** | A free, in-protocol escape from a losing pot, available to **anyone, at will, with no special capability**. A player about to lose a big pot disconnects and gets their money back. |
| The absent player forfeits what they committed; it is distributed to the remaining players in proportion to their own contributions | A DoS incentive: an opponent who can knock a player offline right after a large bet collects it. **And, as five review passes measured, an attack surface: every mechanism that decides *who* forfeits — timeout certificates, equivocation proofs, dispute resolution, attribution — became a target, and each severe defect found in them ended with an *honest* peer's chips taken and its key blocked.** |
| Fault the table with restoration, when nobody can be attributed (`cause = 4`) | The same free escape as row 1, now reachable by a single peer publishing a wrong `state_hash` at a checkpoint (X29), and unavoidable, because there is no observer-independent derivation by which the liar could be named. |

**D-005 took the second. D-010 takes the first, for every cause and every table
size, and this section records the reversal rather than presenting the current
answer as though it had always been the answer.**

D-005's reasoning was that the two costs are not comparable: the rage-quit escape
(X8) is an in-protocol exploit anyone can use for free and must therefore be
closed, while knocking a peer off the network (X9) is an out-of-protocol attack
already listed as out of scope and requiring real capability against the victim's
connection. That reasoning was sound about X8 and X9 and **incomplete about the
third column of the table above**, which is what the five passes then filled in.
Forfeiture is not one rule; it is the terminus of a consensus protocol — who
failed, when, provably, agreed by peers with no clock and no third party — and
the corpus was found to have specified that protocol wrongly five times running,
each time in a way that cost somebody who had followed the rules
(A-1 → X30 → X31 → the `STATE_HASH` re-emission → X32). The cost of row 2 is
therefore not only X9. It is X9 plus the standing risk that the machinery
deciding the forfeiture is itself wrong, borne by honest players — and the fifth
instance showed the risk survives the deletion of the chips, because X32 took
the honest peer's *connection* through a transport layer that D-010's sweep had
not reached. It took D-011 rule 3 to make row 2's deletion complete.

**So the trade D-010 makes is: an exploit that lets a dishonest player escape a
loss is preferable to an exploit that takes an honest player's chips.** The
worst an adversary now achieves on any abort path is a wasted hand — the same
outcome as a flaky network connection, which the protocol has to survive anyway.
The whole attack class evaporates because there is nothing left to win.

**What is given up, in one sentence, without softening.** A losing player can
stall or disconnect and get their chips back, at will, at any table size, and
nothing in the protocol stops them, punishes them, or makes it expensive beyond
ten minutes of their own time. That is X8, it is classified **V** — visible, not
prevented — and `SPEC_CS.md` §18 forbids describing it as anything more.

**The third row is no longer a leftover; it is the whole rule.** On
`HAND_ABORT cause = 4` restoration used to be what remained when nobody could be
named — forfeiture there would have punished a party nobody can identify, since
attribution by "differs from the derivation" needs an observer-independent
reference derivation that does not exist at run time, and the only implementable
version is "attribute everyone who disagrees with me", which is a vote over the
facts and `SPEC_CS.md` §15 forbids it. That argument still holds and is still
worth keeping, because it is the reason `cause = 4` is not a special case any
more: **every** cause disposes of chips the way `cause = 4` always did. The two
lettered questions this document used to carry — which disposition is right on
`cause = 4`, and which is right where nobody can be named — asked exactly that,
and **D-010 answers both the same way**, for the whole protocol rather than for a
corner. §9.2 records what is left of each after the answer, and which letters
their labels have since been released to.

Chip conservation (G9) holds on every one of these paths, and it holds more
simply than before: the property test is that **every stack after an abort is
bit-identical to its start-of-hand value**, with no per-branch arithmetic to get
right.

### 7.5 Residual, unfixed

A malicious player can **always** force a hand to abort by going silent. They
gain no cards. **Under D-010 they also lose nothing**: the abort is neutral, so
where the silent player is the losing player they have escaped the pot, and where
they are merely griefing they have wasted everybody's time at no cost to
themselves. This is no longer confined to the corners of the design — it is the
general case, at every table size, for every cause, whether or not the transcript
names them. The only mitigations are social: the signed record of the abort, and
the per-identity abort count in the lobby. §6 explains why those have little
force when identity is free, and OQ9 asks whether they should be described as a
sanction at all.

The three-way split of §7.3(c) survives as a statement about **naming**, not
about chips. Where `|V(subject)| >= 2` and no other seat is silent, the
transcript names the peer that failed; where two or more seats are silent, or
wherever `|V| < 2` — which at two seats is always — it names nobody. Nothing
follows from either, which is why what remains open here — **OQ-E** — is a question
about the *shape* of the abort event rather than about who pays for it. The chip
half is not open at all: D-010 settled it.

**What is still not left open, and it is a smaller list than it was.** An
attacker does not reach the no-attribution corner deliberately at a larger table
by voting the voter set away — that was X30, and D-008 closes it by making
exclusion from `V` cost a completed certificate. **Both sentences below are true
only of clients that implement the rule**, which is why X30 and the below-floor
floor are D&A rather than CP (§9.3 caution 5); neither is a property an honest
peer gets from A1–A7 whatever its opponent is running. And no single message ends
a hand: a
below-floor certificate is inert, so the escape costs the full `hand_deadline_ms`
of visible stalling rather than one signature (D-009 rule 2, §7.3(c)). Both rules
are kept, and what each is now claimed to deliver is smaller than it was: not
that the escape is closed, but that it is **slow, self-inflicted and legible in
the transcript**, and that no chained event names a victim on one peer's
unsupported word. That is a claim about visibility and cost. It is not a claim
about prevention, and this document does not make one.

**A third thing is no longer left open, and it was not closed by either of
those rules.** The peer that *ends* the stalled hand can no longer be framed by
doing so. Until D-011 rule 2 the terminal abort collided with its own emitter's
contribution at the stalled stage, so the honest peer that reached for the
terminus was rejected, could not end the hand, and produced a verifying proof
against itself in the attempt — and until D-011 rule 3 that proof took its
connections. Both are closed (X32). The claim this section can now make about
the abort path is uniform: **whoever ends a stalled hand, honest or not, ends it
without incriminating themselves and without losing anything by it.** That is
narrower than it sounds — it says nothing about the stall, which remains free
(X7) and remains the escape (X8) — but it is the property whose absence made
five review passes end at the same place.

This is a permanent property of the chosen construction, and it is the price of
G1. It is also the price of D-010, and D-010's own text requires that the price
be documented and not hidden — which is what §7.4, X8 and §9.1.2 limitation 4 do.

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

#### 9.1.0 The four standing limitations, stated once

Four properties are load-bearing, are each the subject of a numbered owner
decision, and were each previously scattered across three or four places in this
file where they drifted apart. **This is the single place that states them.**
Everything later in §9.1 elaborates; nothing later contradicts, and where an
elaboration appears to, this block wins.

1. **A heads-up action deadline is advisory, and produces no signed state
   transition (D-007).** At two seats the required voter set has exactly one
   member — the opponent — so "unanimous" and "one peer's word" are the same
   sentence. The countdown is a UI element. There is no enforceable remedy
   against a stalling heads-up opponent except leaving the table, and no
   document may claim one. `SPEC_CS.md` §32 makes heads-up the first shipped
   mode, so **the first mode this project ships is the one in which the deadline
   machinery does not apply.** Catalogued at X10; elaborated at limitation 3.

2. **A timeout certificate whose required voter set has fewer than two members
   is inert, everywhere and at every table size (D-008, made unconditional by
   D-009 rule 2).** Not an error, not evidence, not chained, no `AbortRecord`,
   no terminating effect. An honest client silently ignores it. The floor is
   written on the size of the voter set and **never on the seat count**, because
   the seat count is not the quantity an attacker can manipulate — and a seat
   leaves the voter set only once a *completed, valid* certificate names it, so
   the set cannot be collapsed by assertion. That scoping is the whole of the
   fix; the version scoped on the seat count let one modified client manufacture
   a one-signer certificate at a six-seat table (X30).

3. **Honest behaviour never incriminates the honest peer, and the anti-replay
   slot key is the enforcement (D-009 rule 1, D-011 rule 2).** No sequence of
   emissions the protocol requires or permits of an honest peer may put two
   bodies in one slot, and therefore none may produce a valid
   `EquivocationProof` against that peer. The enforcement is not a review habit:
   the slot key is **one literal tuple, in `PROTOCOL.md` §5.2, including
   `event_type`**, and every other document — this one included — points at it
   and reproduces none of it. The rule was violated five times while it was
   prose (G7's table: lobby and join traffic, `DISPUTE`, `TIMEOUT_VOTE`,
   `STATE_HASH`, the terminal `HAND_ABORT`); D-011 rule 2 is the **sixth**
   attempt at the property and the first that is a definition rather than a
   principle. The honest limitation is that **nothing structural enforces it
   over future message types** — it is a discipline over the message grammar,
   checked by the mirror test of §5.5, and until that test exists and has been
   seen to fail on a deliberately coarsened key, the property is asserted rather
   than demonstrated. Catalogued at X31 and X32; elaborated at limitation 12.

4. **There is no automated forfeiture and no automated eviction, at any layer,
   and the accepted price is the rage-quit escape (D-010, D-011 rule 3).** An
   abort is neutral: every stack returns to its start-of-hand value, for every
   cause, at every table size, whoever is attributed. Attribution is evidence
   with no automatic consequence. **No proof, certificate or attribution causes
   any part of this system — engine or transport — to forfeit chips, unseat a
   seat, refuse a connection or block-list a key.** The price is stated without
   softening: **a losing player can stall or disconnect and get their chips
   back**, at will, at any table size, with no special capability, and nothing
   prevents it, punishes it or makes it expensive beyond the stall itself. That
   is X8, classified **V — visible, not prevented**, and `SPEC_CS.md` §18
   forbids describing it as anything more. The trade was taken because
   forfeiture and eviction were the two prizes that made four rounds of attacks
   worth mounting, and every severe defect they found ended with an *honest*
   peer's chips taken and its key blocked for following the protocol. Revisited
   before real money, by a new numbered decision, and not before the
   certificate, equivocation and dispute machinery has survived a full
   adversarial pass. Elaborated at limitation 4 and at deviation-register row 6.

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
| 5 | §4 / D-006 | Wherever the required voter set has fewer than two members — at two seats always — an action deadline is advisory and produces no signed transition (D-007, scoped on the voter set rather than on the seat count by D-008), so §4's hand-to-hand progress depends on the participating clients cooperating rather than on anything enforceable. A cryptographic-step deadline below the floor is inert too (D-009 rule 2), so a hand stalled there ends only when the much longer hand deadline expires. **[R17]** The two durations are `PROTOCOL.md` §8.4's and are not printed here. The wait is the deviation; the outcome is unchanged. | `PROTOCOL.md` §8.3, §8.4; this document §9.1.0 items 1 and 2 |
| 6 | §19 | §19 requires the MVP to define four things for the disconnect case: a **timeout**, a **hand abort**, **evidence of which peer failed**, and a **reputation penalty**. The first three are implemented. **The fourth is not, by decision (D-010, carried to every layer by D-011 rule 3).** No penalty is applied by the protocol *anywhere*: no forfeiture, no block-listing, no unseating, no refusal of a connection, no automatic consequence of any kind follows from a proof or from an attribution, at the engine or at the transport. The qualifier "at every layer" is not decoration — under D-010 alone this row was true of four documents and false of the fifth, and the transport layer went on block-listing keys on an `EquivocationProof` while this register said no penalty existed. What exists in its place is the signed record and a per-identity abort count in the lobby, on which other players may act as they choose. This is a deviation from §19 as written, and it is recorded rather than absorbed; §18's ban on claiming more than the design delivers is what makes recording it mandatory. Its price is the rage-quit escape (X8, §7.4). | `docs/DECISIONS.md` D-010 and D-011 rule 3, this document §9.1.0 item 4, X8, §7.4, §9.1.2 limitation 4 |

#### 9.1.2 The limitations themselves

1. **A malicious participant can always force a hand to abort** by going silent
   (X7). Inherent to n-of-n; the fix is forbidden by §19 (§7.2). **Under D-010
   they pay nothing for it**: the abort restores every stack, so a griefer wastes
   only its own time and a losing player recovers its commitment (X8).
2. **An absent seat cannot win the blind it posts** (D-005). A documented,
   deliberate deviation from TDA rules, forced by the same n-of-n property.
3. **Action deadlines are not enforceable against a determined opponent, and the
   failure mode differs by the size of the required voter set** (X10). This
   elaborates §9.1.0 items 1 and 2. Where
   `|V(subject)| >= 2`, a coalition of all the other dealt-in seats can steal one
   honest player's action through a certificate that is valid by construction; it
   is detectable and not adjudicable without a trusted clock. Where `|V| < 2` no
   action deadline is enforceable at all: **a certificate below `|V| = 2` has no
   effect** (D-008 point 2, made unconditional by D-009 rule 2) — it is not an
   error, not evidence, not chained, it produces no `AbortRecord` and no
   forfeiture, it cannot end a hand, and an honest client silently ignores it, so
   the deadline stays advisory, a stalling opponent cannot be punished, and the
   only remedy is to leave the table. **At two seats `|V| = 1` always, so
   heads-up deadlines are advisory permanently and by construction** (D-007): the
   countdown is a UI element that produces no signed transition. The scoping
   matters and is not cosmetic: the same rules written on `n` left a modified
   client able to manufacture `|V| = 1` at a six-seat table and take an honest
   player's chips — X30. **Neither case is solved**; what D-008 fixes is that only
   the honest cases remain, and what D-009 rule 2 fixes is that the inert
   certificate stays inert in every document rather than acquiring an effect in
   one of them. **D-010 and D-011 rule 3 remove what a certificate was worth
   without changing either of those rules**: a completed certificate at
   `|V| >= 2` still steals the subject's action, but it no longer takes the
   subject's chips, does not unseat them, and does not put their key on any
   layer's block list — so the worst case of this limitation is **a hand the
   honest player did not get to play**. The rules are kept because the
   certificate is still a chained event that *names* a seat, because a corpus
   that let one peer manufacture a naming would be writing a false attribution
   into the record D-010 point 2 says a human may later adjudicate on, and
   because D-010 is explicitly revisitable before real money — at which point
   the payoff returns and this floor is the only thing standing between it and
   X30.
4. **There is no automated forfeiture and no automated eviction, and the price of
   that is the rage-quit escape** (D-010, D-011 rule 3, X8). This elaborates
   §9.1.0 item 4, which states it; the limitation was chosen deliberately and has
   two halves that must be stated together.

   *What is removed.* An abort is neutral: stacks are restored to their
   start-of-hand values, for every cause, at every table size, whoever is
   attributed. Attribution is recorded as evidence and nothing acts on it. No
   peer is block-listed, unseated, refused a connection or penalised by any
   layer of this system on the strength of any proof — **and "any layer" is the
   half that D-010 did not deliver on its own.** D-010 point 3 was written as a
   protocol rule and swept through four documents; the fifth, which owns
   transport, still called `block_peer` on an `EquivocationProof` in two places
   and the state machine still unseated a seat on a self-contained proof in one.
   D-011 rule 3 deletes all three. Equivocation proofs and timeout certificates
   are still produced — they are how a human or a later version adjudicates —
   but consuming one never moves a chip, removes a player, or costs anyone a
   connection in this version.

   *What that costs.* **A losing player can stall or disconnect and get their
   chips back.** At will, at any table size, with no special capability, no
   coalition and no cryptographic break. Nothing in the protocol prevents it,
   punishes it, or makes it expensive beyond the ten minutes of `hand_deadline_ms`
   the player spends stalling. It is classified **V — visible, not prevented**
   (§5.1), the mitigations are exactly two — the signed transcript record and the
   per-identity abort count in the lobby, both social and neither with teeth
   (limitation 7, OQ9) — and `SPEC_CS.md` §18 forbids describing it as anything
   more. It is also a deviation from §19's fourth required element, recorded as
   register row 6.

   *Why the trade was taken.* Forfeiture was the first prize and automated
   eviction the second, and together they made five successive rounds of attacks
   worth mounting: every severe defect those rounds found ended with an
   **honest** peer's chips taken and its key blocked for following the protocol
   (X30, X31, X32). An exploit that harms
   an honest player is worse than one that lets a dishonest player escape a loss.
   The worst an adversary now achieves is a wasted hand, which is what a flaky
   connection produces anyway and what the protocol must survive regardless.

   *What is not claimed.* That this is safe for real money — it is not, and D-010
   is revisited before real money and not before the certificate, equivocation and
   dispute machinery has survived a full adversarial pass with no new defect of
   this class. That reversal will be a new numbered decision with its own
   evidence.

   *Superseded wording.* This item previously read that the rage-quit escape had
   reopened "in that corner only", where `|V| < 2`, and that the interim default
   preferred a liveness and fairness loss to a theft. The corner is now the
   general case (§7.5), and the interim default is a decision. The lettered
   question that asked which disposition is right at two seats is answered
   corpus-wide by D-010 taking restoration everywhere, and is recorded as closed in
   §9.2 rather than carried; the letter it used is now `DECISIONS.md`'s and names
   the reference engine.
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
   define one is an open decision in `docs/DECISIONS.md`. **OQ-A.**
6. **Collusion, endpoint compromise, screen sharing, Sybil identities, coercion,
   traffic analysis and DoS are outside the protocol's reach** (§6). This is not a
   temporary gap.
7. **Reputation has no teeth, and under D-010 it is the only sanction there is.**
   Identity is a locally generated keypair; the penalty for repeated aborts is
   visibility, nothing more. Under D-005 visibility was one sanction of two;
   D-010 deletes the other, so the signed abort record and the per-identity abort
   count in the lobby now carry the whole weight, against an attacker who can
   discard the identity for free. OQ9 asks whether that should be called a
   sanction at all in the UI.
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
12. **That honest behaviour never manufactures evidence against the honest peer
    is a property of the message set, not of the cryptography, and it has to be
    re-established for every message type that is ever added.** This elaborates
    §9.1.0 item 3, which states the rule; here is what it costs. The property
    has failed **five** times, in five different message types, each recurrence
    more reachable than the last: lobby and join traffic, `DISPUTE`,
    `TIMEOUT_VOTE` (X31, a Sybil pair), `STATE_HASH` (one dropped stream), and
    the terminal `HAND_ABORT` (X32 — nothing but an opponent going quiet, on the
    shipped heads-up configuration). **All five are closed**, the last two by
    giving a reconciliation round its own stage (`PROTOCOL.md` §4.9) and by
    D-011 rule 2. **The limitation is not that any of them is open. It is that
    the property has no structural enforcement**: it is a discipline over the
    message grammar, checked by the mirror test of §5.5, not implied by A1–A7,
    and a sixth message type added carelessly re-opens it.

    **What changed with D-011 rule 2, and why it is more than a sixth patch.**
    The first four fixes each widened a key and left the *rule* as prose, which
    every editor then re-derived — and re-derived wrongly, five times running.
    Rule 2 replaces the prose with **one literal tuple at one site**, including
    `event_type`, which every other document points at and none reproduces. This
    document's own former copy of that key is the argument for the change: it
    was stale in two fields at once, and a reader checking a new message type
    against it would have certified both X31 and X32 as clean (G7 [R4]).

    **D-010 and D-011 rule 3 bound the damage of a future recurrence and do not
    repair the property.** Because no proof moves a chip, unseats a seat or
    blocks a key at any layer, a sixth recurrence would cost the framed honest
    peer a hand rather than a stack and a network. But the specification would
    still frame it, the frame is permanent in the transcript, and D-010 point 2
    says a human may later adjudicate on exactly such records. Until the mirror
    test exists in `tests/adversarial/` — carrying all three named interleavings
    of §5.5 — and is seen to fail on a deliberately coarsened slot key, the
    property is asserted rather than demonstrated.
13. **`SmallRng` and `StdRng` are compiled into the binary and cannot be removed**
    without dropping libp2p features the connectivity design depends on (A7).
    `SPEC_CS.md` §7's prohibition is therefore enforced as a discipline over our
    own code — the source scan in `src/security/rng.rs`, which fails the build —
    and not by the dependency graph. No security property of this project may be
    stated as an absence from that graph (D-009 rule 3), because an absence
    verified in an isolated probe has now failed to survive integration three
    times. Confirmed for this revision: **no absence claim about `SmallRng`
    survives anywhere in this document** — every occurrence either asserts the
    generator is linked in, or quotes a withdrawn claim in order to withdraw it
    (A7). The claim that survived one document away, in `PROTOCOL.md` §4.4, has
    since been quoted and withdrawn there too (`research/PHASE2_GATE.md` M3,
    RESOLVED), so D-009 rule 3 is satisfied corpus-wide. **The limitation is
    permanent regardless**, because the generators remain in the binary and only
    the discipline over our own code keeps them unreachable from it.

#### 9.1.3 Restatements deleted under D-011 rule 1, and what deleting them costs

D-011 rule 1 gives this document one job — **classifications** — and forbids it
from restating a wire shape, a transition or a construction that another
document owns. Every restatement found in this file has been deleted and
replaced by a pointer naming the owning section. Each deletion is marked in
place with a bracketed tag so the change is checkable rather than asserted.

**Count: 25 restatements deleted, at 25 sites.**

| Tag | Site | What was restated | Owner it now points at |
|---|---|---|---|
| R1 | A11 | the `ctx` field list | `PROTOCOL.md` §4.5 |
| R2 | A13 | the timeout certificate's fields, emitter set and stage shape | `PROTOCOL.md` §4.8, `STATE_MACHINE.md` §8.4 |
| R3 | §3.5 | relay circuit limits, byte accounting, per-hand arithmetic | `NETWORK_STACK.md` §9.5, §16.1 |
| R4 | G7 | the equivocation predicate, the slot key as a tuple, and the per-type anti-replay rules for unchained traffic | `PROTOCOL.md` §5.2, §5.3 |
| R5 | G7 | D-009 rule 1's normative text, block-quoted | `PROTOCOL.md` §5.2 |
| R6 | G9 | invariant I1, the ledger identity, in full | `STATE_MACHINE.md` §10 |
| R7 | §5.2 row 16 | the slot key as a tuple | `PROTOCOL.md` §5.2 |
| R8 | X31 | the corrected anti-replay index and key | `PROTOCOL.md` §5.2, §5.3 |
| R9 | X31 | `TIMEOUT_VOTE`'s envelope fields as they stood | `PROTOCOL.md` §4.8 |
| R10 | §5.5 | the slot key, inside a test's asserted outcome | `PROTOCOL.md` §5.2 |
| R11 | X15 | chat size cap, per-peer rate limits, message-id override, freshness window | `PROTOCOL.md` §7.6; `NETWORK_STACK.md` §6.2, §6.4, §6.5, §6.6, §10.3 |
| R12 | X20 | relay circuit limits again — the same derivation §3.5 also carried | `NETWORK_STACK.md` §9.5, §16.1 |
| R13 | X21 | the relay admission-control construction | `NETWORK_STACK.md` §9.6 |
| R14 | §6 | the lobby-chat code-point rejection list | `PROTOCOL.md` §7.6, §9.4 |
| R15 | §7.3(c) | the two deadline durations and the terminal abort's body | `PROTOCOL.md` §8.4, §4.10 |
| R16 | X8 | the hand deadline's value | `PROTOCOL.md` §8.4 |
| R17 | register row 5 | the two deadline durations | `PROTOCOL.md` §8.4 |
| R18 | OQ3 | the `ctx` field list and its hasher | `PROTOCOL.md` §4.5, §2.8 |
| R19 | X10 | what the timeout certificate names and how it chains | `PROTOCOL.md` §4.8 |
| R20 | §5.2 row 18 | the enumeration of size caps and the over-cap response | `NETWORK_STACK.md` §6.5, §11.3 |
| R21 | X4 | the canonical container shape, the field-order rule, the gate's position relative to the signature check, the append-only index rule | `PROTOCOL.md` §2.8, `CRYPTOGRAPHY.md` §4.7 |
| R22 | A10 | the index map's input field list | `PROTOCOL.md` §4.5 |
| R23 | X2 | the index map's input field list, again | `PROTOCOL.md` §4.5 |
| R24 | §1.2 | the public relay's circuit limits, a third copy in this file | `NETWORK_STACK.md` §9.5, §16.1 |
| R25 | OQ12 | the relay byte estimates and caps the question is asking about | `NETWORK_STACK.md` §9.5, §16.1 |

**The number that matters is not 25 but four.** Four *facts* had already drifted
from their owner by the time they were deleted, across seven of these
twenty-five sites — and three of those seven were copies of a copy, the same
fact reproduced three times *inside this one file*. A rule that only stopped
cross-document restatement would have left the latter standing, and this file
was already drifting against itself.

*The four facts that had drifted.* **R4**, this document's copy of the slot key, was
stale in two fields at once: it omitted the subject, which D-009 rule 1 had
added, and `event_type`, which D-011 rule 2 adds. A reader checking a new
message type against the key *as this document printed it* would have certified
both X31 and X32 as clean. **R3, R12 and R24** were the public relay's circuit
limits, carried in three separate places in this file, and the byte figure was
**wrong by a factor of two in all three** until a review found it and the
correction reached two of the three. **R9** described a message shape that no
longer existed. **R7 and R10** printed shortened versions of the same stale key
as R4, one of them inside a test's asserted outcome — a test written against a
printed copy is how three of the five recurrences in G7's table survived review.

**What the deletions cost the reader, stated rather than presented as pure
gain.** This document is now less self-contained. A reader who wants to check
G7 against the message set, or G9's conservation claim against the invariant,
or X20's bound against the relay's actual limits, must open the owning document;
none of those checks can be completed inside this file any more. That is a real
loss of local verifiability and it is the price of the rule. It is judged worth
paying because the alternative was measured: **five review passes each found a
copy that had drifted from its owner**, and in three cases the drifted copy was
the thing that certified a live defect as clean. A reader who has to follow a
pointer is inconvenienced; a reader who checks against a stale copy is misled.

**The standing obligation.** No future revision of this file may reintroduce a
tuple, a field list, a constant, a transition, a size cap or a construction that
another document owns — not "for the reader's convenience", not "quoted for
completeness", not in a table, and not in an open question. Where a definition is
needed, name the section. Where two documents disagree, the owner wins, and this
document records the disagreement as a finding rather than picking a side.

### 9.2 Open questions

| # | Question | Blocks | Owner / where resolved |
|---|---|---|---|
| **OQ1** | Is `ziffle`'s Bayer–Groth implementation *sound*? 1779 lines, unaudited, one author. A line-by-line review of `MultiExpArg` and `SingleValueProductArg` against the paper is a **prerequisite**, not a nice-to-have. If it fails, fall back to `barnett-smart-card-protocol` with its three repos vendored. | Assumptions A3/A4; §5 rows 1–4; goals G3/G4 | `docs/CRYPTOGRAPHY.md`; Phase 5 |
| **OQ2** | Is the forked Fiat–Shamir transcript safe? Same review, plus a deliberate attempt to produce a proof valid under one sub-argument's challenges and invalid under the other's. | A4 | same as OQ1 |
| **OQ3** | Is the `ctx` binding *sufficient*? The construction itself is settled and normative in `PROTOCOL.md` §4.5, and A11 states the assumption. **[R18]** The field list and the hasher are §4.5's and §2.8's and are no longer reproduced here (D-011 rule 1) — an open question that prints the field set it is asking about will be answered against the printed copy. What remains open is whether that field set closes every replay avenue, and the mandatory regression test replaying a valid shuffle proof from hand `h` into hand `h+1` and asserting rejection. Our bug to make, not the library's. | A11; §5 row 13, X5 | `docs/PROTOCOL.md` §4.5; Phase 5 |
| **OQ4** | Are remote timing side channels in scope? Defensible to exclude for play money; **not** defensible for real money. Settled by `dudect`-style analysis of `shuffle_deck` and `reveal_token`, or by an explicit decision. | X23 | this document, revised |
| **OQ5** | Fuzzing the cryptographic deserialisers (§27), including `ziffle`'s `Transcript` `assert!` panic path. Until this lands, §17 row 17 carries a live remote-panic risk. | §5 row 17 | Phase 6 |
| **OQ6** | Mucking policy at showdown. Three options — mandatory universal reveal, TDA-faithful mucking with binding forfeiture, or delayed reveal at end of tournament — and each changes the cryptographic protocol, not just the engine. Option 3 reopens §19 because escrowed shares must survive a disconnect. Affects G8 and X12. | G8; X12 | `docs/DECISIONS.md`, then `docs/PROTOCOL.md` |
| **OQ7** | Relay admission policy: by `identify` protocol name (lets any stranger running our client in, which is the point) or by lobby presence (stricter, keeps a brand-new client out). Both `reservation_rate_limiters` and `circuit_src_rate_limiters` must be gated either way. | X21 | `docs/NETWORK_STACK.md` (D-002 open question) |
| **OQ8** | Is there any defence against a forged timeout certificate signed by all the other dealt-in seats (X10) that does not introduce a trusted clock? None is currently known. Where `\|V\| < 2` — heads-up always — the mechanism is inert and the deadline is advisory (D-007, D-008), so there is nothing left for this question to protect there; the *manufactured* `\|V\| = 1` is closed by D-008 (X30) rather than left to this question. **D-010 halves what the question is worth without answering it:** a forged certificate no longer takes the subject's chips, so what is at stake is a stolen action and a wasted hand. The question stays open at that reduced stake, and returns at full stake whenever D-010 is revisited. | X10 | `docs/PROTOCOL.md` |
| **OQ9** | Is a visible abort record a meaningful sanction when identity is free? If not, say so plainly in the UI rather than implying a reputation system exists. **D-010 promotes this from a fair question to a load-bearing one:** the abort record and the per-identity lobby abort count are now the *only* mitigations against X7 and X8, so if the answer is no, the honest statement is that those two attacks have no mitigation at all — which §9.1.2 limitation 4 already says, and which the UI must not contradict. | §6, §9.1.2 limitations 4 and 7 | `docs/PROTOCOL.md` |
| **OQ10** | Is it acceptable for a client to spend its own bandwidth relaying strangers' games (D-002)? Must be a visible, consenting setting, never silently on. | X21 | project owner (D-001 addendum) |
| **OQ11** | Should the epoch-rotating `LOBBY_INFOHASH` of §8.5 be adopted, at the cost of cross-version compatibility? The spec currently mandates one fixed constant, so this needs a numbered decision. | §8 | `docs/DECISIONS.md` |
| **OQ12** | What is the real per-hand byte count over **one relayed circuit, counting both directions together**, measured against the `Limit` a real relay returns? **The measurement must be bidirectional**, because the byte cap is one budget for the whole circuit; measuring one direction reports twice the headroom there is, which is the mistake this corpus already made in two places at once. **[R25]** The estimates, the caps and the arithmetic behind them are `NETWORK_STACK.md` §9.5 and §16.1's and are not restated here — an open question that carries its own copy of the numbers it is asking about will be closed against the copy. What this row asks is unchanged: measure it, bidirectionally, against a real relay's returned limit, and confirm what the owner document predicts — that the **duration** limit binds long before the byte cap does. | X20 | Phase 5 measurement, then Phase 8 |
| **OQ13** | Open-source licence. `zshuffle` was rejected partly on GPL-3.0-only; the recommended set is permissive. Needed before publication. | — | project owner (already open in `DECISIONS.md`) |

**The corpus-wide lettered open questions — `DECISIONS.md`'s letters, adopted
(R-2, H6).** These are lettered rather than numbered because they cut across
documents and each needs a numbered owner decision rather than a technical answer
from an editor.

**`DECISIONS.md` is authority for the `OQ-*` series, and this document adopts its
letters and its wording** (D-011 rule 1). Three letters exist corpus-wide —
**OQ-A**, **OQ-D**, **OQ-F** — and `PROTOCOL.md` §12 and `STATE_MACHINE.md` §11
have already adopted them. The parallel scheme that stood here, in which `OQ-A`,
`OQ-B` and `OQ-D` named different questions than the same letters name in
`DECISIONS.md`, is **deleted**. A reader following a letter from one document to
another now lands on the same question, which is the whole point of a letter.

**`DECISIONS.md`'s numbered open blockers are adopted on the same terms, and one
of them belongs in this table**: **K-1**, because it is the only item in the
corpus that changes a *classification* here rather than a rule elsewhere. This is
D-013's process finding being complied with rather than described. That finding
is that *a finding correctly assigned across an owner boundary is a finding
nobody owns* — the document that finds a defect is often not the document that
may fix it, so the finding sits in the finder's pages, invisible to the editor
who has to act. **K-4, the item that produced this revision, is the same failure
one layer down and in this document**: a previous pass filed it correctly against
`THREAT_MODEL.md`, and three passes went by with the superseded cost model still
in §9.3 and the superseded reconnect rule still in §7.3(b), while this file
carried exactly **one** occurrence of `D-013`. The coverage signature was
available the whole time and nobody read it, which is the argument for the count
being reported rather than the sweep being asserted.

**The coverage signature for this pass, reported rather than asserted, because that
is what the last one is about.** This document carries **27** occurrences of
`D-013`, **22** of `D-012` and — before this pass — **zero** of `D-014`, which is the
same signature K-4 was found by, one decision later and in the same file. **After it,
`D-014` is above thirty here and the corpus's zeroes are gone**: `CRYPTOGRAPHY.md` went
from zero to a sweep record plus a new §8.1 that states what its three tier-1 proofs do
and do not prove, and `CONTRIBUTING.md` and `DEPENDENCIES.md` from zero to sweep records
of their own (the `L8` paragraph below). It is
reported here rather than in a commit message because a count in a document can be
re-run by the next reviewer and a claim that a sweep happened cannot. **The same
check applied outward, since D-012's process rule is corpus-wide and a finding
correctly assigned across an owner boundary is a finding nobody owns:**
`CONTRIBUTING.md` and `DEPENDENCIES.md` carry dated sweep records that stop at
D-012 and **zero** occurrences of D-013 or D-014, while both cite the rule that
every decision sweep covers every document. That is not this document's to fix and
is filed as **L8** against those two; one row each recording *"no change, and here
is why"* discharges it, and leaving a record that reads *"swept against D-009 to
D-012"* two decisions later is precisely what J-5 is about.

**L8 is discharged, and this paragraph is updated rather than left standing, which is
the half of the hand-off J-4 is about.** Both files were swept in the pass that wrote
this sentence: `CONTRIBUTING.md` carries a second dated record, a fourth numbered
failure of its own §2.5 rule, and two new checklist items — one for D-013's *no progress
path reads a status word*, one for D-014's four admissibility questions; `DEPENDENCIES.md`
carries a D-013/D-014 sweep table whose one substantive finding is that D-014 makes
`ziffle`'s **false-reject** behaviour load-bearing against a person, so OQ-1 now discharges
two obligations rather than one. The count that mattered was the count: three passes of
*"named and not fixed"* were visible as **zero occurrences** the entire time, which is the
argument for reporting counts and the reason this document reports its own.

| # | Question, in `DECISIONS.md`'s wording | Where this document depends on it |
|---|---|---|
| **OQ-A** | *"A named, versioned reference engine, since several sections claim disputes are 'deterministically adjudicable by any third party running the reference engine' and no such engine is defined (review N2). Interim answer: withdraw the claim; the engine is defined when the crate has a tagged release."* | X29's third bound, which is **evidence preservation, not recourse**, until this is answered; §9.1.2 limitation 5; the withdrawn "deterministically adjudicable offline" claim wherever it appeared |
| **OQ-D** | *"A dispute path that does not require the accused peer's signature — circular at every table size, not only heads-up (D-007 point 4, review A-1). Interim answer under D-010: a dispute that cannot resolve ends the hand neutrally, so the circularity costs a hand rather than a stalemate."* | X7; §7.3(c). Formerly this document's `OQ-B` |
| **OQ-F** | *"Whether `TIMEOUT_VOTE`, `TIMEOUT_CERT` and `EquivocationProof` should still be produced in the MVP now that D-010 gives them no effect, or be deferred wholesale until the machinery is sound. Producing them keeps the transcript adjudicable later; deferring them removes four passes' worth of surface."* | X10, X30, X31, X32 and the `CheaterEquivocation` / `CheaterDisconnect` rows of §5.5 all describe machinery this letter asks whether to build at all. It changes what ships rather than what is true, so no classification here turns on it |
| **OQ-E** | **Blocking.** How is a timeout certificate constructed when two or more seats are simultaneously unresponsive, and whom does it attribute? `signers == participants \ {subject}` is unachievable, and attributing every non-voting seat punishes honest seats behind a partition. The interim behaviour is to abort with no attribution and no chip movement, which lets two colluding seats void a hand for free. Blocking for Phase 4. | X7 (DNA case); §7.3(c). **This letter is this document's own**, referenced by `PROTOCOL.md` Q-02 and `STATE_MACHINE.md` Q3 and defined nowhere else |
| **D-014-1** | **Open, and this document's row is discharged in this pass while the corpus's is not.** *"Integrate D-014 across the corpus: the two evidence tiers, the checkpoint precondition for tier 2, the one-way exit, the dead seat blinded off, and the information window. Touches all five specification documents."* | §5.1's D&A definition, §5.2's re-classification table, §5.3's **X37**, §5.5's tier column and the mirror gate. What this document does **not** own and must not be read as settling: the wire representation of a removed seat — `PROTOCOL.md` §6.1's `PublicTableState` hashes `sitting_out` and no longer hashes `absent`, so a `Removed` seat is canonical per-seat state that **no vector hashes**, and two peers that disagree about a removal would agree on every hashed vector. That is filed as `D-014-3`. **Corrected in this pass, and it is the discharge being wrong rather than incomplete:** §5.1's D&A cell and §5.2's row 13 both carried *a chain parent that does not exist* as **tier 1**, which fails the admissibility test — the parent is looked for in the receiver's own store, so one dropped frame removes an honest player. Both are corrected, row 13 is re-classified to **none** (CP already answers it), and **§5.1.1 is new**: the three-question test a proposed tier-1 addition must pass, stated here because this document owns the classification scheme and every other document points at it. `src/security/validation.rs` had deleted the clause first; `PROTOCOL.md` and `DECISIONS.md` D-014 are corrected in the same pass |
| **D-014-2** | **Blocking for shipping the removal, and it is this document's gate.** *"The mirror test D-014 requires: under every legal interleaving, no honest peer is evictable."* | **X37**, whose whole content is what happens if this is not run, and §5.5's mirror block, where it is specified as a test. Nothing here classifies the removal feature as safe until it passes; X37 is DNA today and stays DNA |
| **D-014-3** | **New in this pass, opened by D-014's own integration.** How does a removal reach canonical state? D-014's safety argument is that every honest peer reaches the same verdict from data it already holds — true of the **verdict** and not of the **holding**: whether the offending message reached this peer is per-receiver, and a seat's status is canonical state (D-012, `STATE_MACHINE.md` I30). So the *evidence* must itself be chained, which needs a message type, a stage, a chain position, and an answer for a peer that never received the offending message. **Owner: `PROTOCOL.md`.** | X37's mechanism half. Until it is answered, a removal derived from a locally observed fact is a D-012 violation wearing D-014's name, and this document classifies nothing on the assumption that it will be answered one way rather than another |
| **N8** | **New in this pass, small, and it is a citation that outran its source.** D-014 says the removal is explained to the other players by *"`SPEC_CS.md` §22's information window"*, and **§22 contains no such element**: its GUI tree is *network status*, *lobby*, and a *poker table* ending at `timer` and `protocol/security status`, with one further sentence forbidding the display of a cryptographically unverified card. An anti-cheat window naming a removed player, the tier and the evidence is **not among them**. The disposition is to **record it as a required addition to §22 rather than to cite it as existing** — `SPEC_CS.md` is the owner's document and is not edited from here — and to make sure no other document cites it as present: `STATE_MACHINE.md` T64's cell now says *required addition* in terms, and this row is the corpus's record of it. **What the addition must say is already fixed and is not re-opened here**: the removed player, the tier, what the evidence was, **and that the hand was voided and no chips changed hands** — the last clause is not decoration, since a void that reads as a loss is how a correct removal becomes a support complaint. | The user-facing half of **X37** and of §5.2's D-014 re-classification. Nothing in the catalogue turns on it — a missing window changes no attacker's payoff — which is exactly why it could sit uncited for a pass: **it is the one part of D-014 that no test would have caught**, because the corpus tests behaviour and this is the sentence a person reads |
| **K-1** | **CLOSED since this row was written, and the row is kept for the trail rather than as an open item.** The disposition is detection, not prevention: `PROTOCOL.md` §3.2's **solitary-stage rule**, with the engine half at `STATE_MACHINE.md` T62, T63, §9.3 condition 0.6 and I33. The sub-question this row calls undefined is answered — a peer's own emission **does** count toward its own participation record — and `DECISIONS.md` **K-3** landed too, so every hand now places a checkpoint. **X36 is split rather than moved** (DNA where either peer is still transmitting, NP&ND under a total bidirectional partition) and §5.4 records it as a split. The sentence this document may now make is the narrow one in X36's cell, and the wide one it withdrew stays withdrawn. | **X36**, and §9.3's standing caution, which is corrected in the same pass |
| **K-1 (as it stood)** | **Blocking, and this document's heaviest open item.** *“The required emitter set is a per-receiver quantity on the one path where it narrows, and its payoff is a silent permanent fork.”* The set is agreed between honest peers exactly where it is inert and per-receiver exactly where it acts — a stalled stage on an aborted hand — and the argument offered for it inverts the witness-independence property it rests on. Owner decision needed: ratify the stalled stage, or floor the set at two members. A second quantity, whether a peer's own emission counts toward its own participation record, is undefined in both owning documents and decides which of two failures occurs. | **X36**, the only **NP&ND** row in the catalogue, and the reason §9.3 carries a seventh qualification. This document cannot classify the row any higher until the decision is taken, and cannot claim that honest divergence is always detected while it stands. `DECISIONS.md` **K-3** would move it from NP&ND to DNA without deciding K-1, by checkpointing a hand that placed no other checkpoint. |

**The mapping from this document's former labels, recorded once so an old
cross-reference can still be followed, and not to be maintained:**

| Former label here | Now |
|---|---|
| `OQ-A` — restoration versus forfeiture where `\|V\| < 2` | **Closed by D-010**, and closed wider than it was asked: restoration on every path, at every table size, whether or not anybody is named. The letter `OQ-A` now names the reference engine |
| `OQ-B` — a dispute path not needing the accused's signature | **OQ-D** |
| `OQ-C` — an obligatory `ACTION_SEEN` before a vote | **`PROTOCOL.md` Q-08**, that document's local series |
| `OQ-D` — `cause = 4`: restore, forfeit, or settle from the last agreed checkpoint | Its **chip half is closed by D-010** (restoration). Of the rest, *settle from the last agreed checkpoint* is **`PROTOCOL.md` Q-07** and *how anyone establishes which peer diverged* is **OQ-A**. The letter `OQ-D` now names the dispute path |
| `OQ-E` | **unchanged**, and still this document's |

**Two assertions that stood in this paragraph's place are deleted because they had
become false (H6).** The first was that the lettered wording here was kept
*byte-identical* with `PROTOCOL.md` §12 and `STATE_MACHINE.md` §11, and that
rewording it locally would therefore break a shared canonical text. Both of those
documents have since reconciled onto `DECISIONS.md`'s letters and **neither holds
the rows that claim pointed at any more**, so the text this document believed it
was preserving byte-identity with no longer exists. Refusing to reword on that
ground had stopped protecting a shared wording and started protecting the
collision. The second assertion was that `OQ-C` is *"recorded in `PROTOCOL.md` §12
only"*, which is now wrong about the label as well as the location: that question
is `PROTOCOL.md` **Q-08**. The question itself is unchanged and still **not
adopted** — should a required voter publish a signed
`ACTION_SEEN { sequence, event_hash }` before it may vote, so that vote-and-seen
are two events by one key in one slot and a lying voter becomes provable — and
X10's honest statement of the race stands without it.

**The bookkeeping defect this section used to flag is closed rather than
re-flagged.** Three passes running, this document recorded that the letters
collided across the corpus and declined to fix it here, reasoning that one editor
had to resolve it everywhere at once. That editor has now been round:
`PROTOCOL.md` §12 and `STATE_MACHINE.md` §11 adopted `DECISIONS.md`'s letters,
this section is the third and last, and `DECISIONS.md` is authority under the
project's own ordering. Recording a collision for three passes while every
document kept its own scheme is the defect, not the fix; the fix is to adopt, and
the mapping table above is what an old cross-reference lands on.

**D-008 generalises the scope of OQ-E, and its wording is deliberately unchanged.**
It is written on seats "simultaneously unresponsive" because it predates D-008.
Read it on `|V|`: it arises wherever no certificate can complete because each
required voter set contains another subject. The same generalisation applied to
the question this document formerly lettered `OQ-A` — that one arose wherever
`|V| < 2`, which is at two seats always and above two seats only after completed
certificates have legitimately attributed the other seats — and that question is
closed by D-010 rather than carried.

**What D-010 settled, and what it did not.** The two questions this document
formerly lettered `OQ-A` and `OQ-D` both asked which chip disposition is right
when nobody can be attributed. **D-010 chose restoration, for every cause, at
every table size, whether or not anybody is named**, so both are answered by a
numbered owner decision, which is exactly what each said it needed; the mapping
table above records what is left of each. **OQ-E is not answered.** It asks how
the abort event is *shaped* when two or more seats are unresponsive and whom it
attributes, and D-010 removes the consequence of attribution without settling the
shape, so it stays blocking for Phase 4 for the reason the P3 finding gives — an
abort naming nobody still needs a required emitter set that every peer derives
identically. **D-011 rule 1 settles which document answers it, not what the answer
is**: `PROTOCOL.md` owns the terminal stage's shape, its `sequence` and what may
trigger it, and `STATE_MACHINE.md` follows. That removes this document from the
question entirely — a threat model that proposed an emitter set would be writing
the wire, which is what D-011 rule 1 forbids. What this document carries is the
classification: an abort that names nobody is **DNA**, and the liveness half of
getting it accepted at all is X32.

### 9.3 The standing caution

This document classifies **17 of 55** catalogued attacks as cryptographically
prevented. That number is meaningful only alongside eight qualifications, and it
must never be quoted without them:

1. Four of those seventeen — the deck-integrity rows — rest on **A3 and A4, which
   are not verified**. OQ1 is the gate.
2. Thirteen attacks are **out of scope entirely**, and they include the ones most
   likely to be used against a real game: collusion, endpoint compromise, and
   denial of service.
3. Six are **detected but not attributable**, and one more (X7) is attributable
   in only one of its three cases. Detection without attribution stops a hand; it
   does not name anybody, and under this design it does not move chips either.
   **Nor does attribution**, since D-010 — and since D-011 rule 3 it does not
   cost the named peer its seat or its connections either. The difference
   between the D&A and DNA buckets is now a difference in what the transcript
   records, not in what happens to anybody. **Three of the six — X33, X34 and
   X35 — are not attributable because there is no attacker at all**: honest peers
   diverge, or a table simply stops progressing, and the bucket is doing
   something different there. It records that the design has no name to offer,
   not that a culprit escaped one. That half of the bucket has grown from one row
   to three in two passes, and it is now the fastest-growing thing in this
   catalogue.
4. "Prevented" always means *under the stated assumptions*, never *impossible*.
5. **Three of the fifteen D&A rows — X30, X31 and X32 — are D&A only because our
   own client enforces a rule.** Their rejection rests on an implementation
   obligation in the class of A12 and A14, not on A1–A7, and in all three cases
   the rule is one this corpus got wrong first and corrected afterwards — twice
   for the same underlying property, which is why D-011 rule 2 replaces the
   principle with a single literal definition. None of the three may be read as
   a property the design has always had, and each is only as real as the client
   that implements the key or the floor.
6. **One attack is neither prevented, nor answered, nor out of reach: X8, the
   rage-quit escape, is classified V — visible, not prevented.** A losing player
   stalls or disconnects, the hand aborts, and their chips come back. The protocol
   sees it, records it permanently, counts it per identity in the lobby, and does
   nothing about it. It is the accepted price of D-010's removal of automated
   forfeiture and eviction, it is the reason five other rows lost their payoff, and
   it may not be described as bounded, deterred or expensive. `SPEC_CS.md` §18
   requires exactly this distinction to be drawn: some cheating is prevented, some
   is detected, and this one is merely visible.
7. **One divergence was neither prevented, nor detected, nor out of reach, and it
   needed no attacker: X36.** Two honest peers fork, each finishes the tournament
   alone, and each client tells its player it won. Qualifications 1 to 6 all
   restrict what *prevented* means; this one restricts what *detected* means, and
   it is the reason §5.1 gained the NP&ND bucket. **`DECISIONS.md` K-1 is now
   decided and the qualification narrows rather than lifts.** The solitary-stage
   rule makes the fork's own trace — one dropped frame, both peers still
   transmitting — **detected**: the peer that narrowed freezes on the first
   contradicting event, unilaterally, and the table closes rather than reaching a
   private "tournament won". So the sentence this document may now make is
   *divergence between honest peers is detected wherever either peer is still
   transmitting*. **The sentence it withdrew stays withdrawn**, because a total
   bidirectional partition still ends in two peers each draining the other with no
   wire evidence separating that from the case the drain exists for, and no repair
   of X36 removes it — it is what the drain does under a partition however the
   required set is defined. Any revision that restores the unqualified claim, in
   either direction, is a defect in that revision.
8. **One risk in this catalogue is created by the design's own defence, and it is
   the newest: X37.** D-014 removes a player who sends a provably illegal message,
   which makes **the correctness of every validator** load-bearing against a
   *person* rather than against a message. A validator that is too strict now
   ejects an honest player, and this corpus has produced state divergences between
   honest peers — K-1 among them — in four consecutive passes, which is exactly
   how an honest player's legal action comes to look illegal. The mitigation is a
   **precondition**, not a caveat: a tier-2 removal requires the accused's message
   to be judged against state fixed by a checkpoint both peers signed. Tier 1
   needs no checkpoint and correctly needs none, but "tier 1" is a claim about a
   validator and not about a message, and `DECISIONS.md` C-1 to C-3 are three live
   validator disagreements in the shipped `src/protocol/`. **The gate is D-014-2**
   — under every legal interleaving, no honest peer is evictable — it is the
   mirror of `SPEC_CS.md` §25's cheater list, and until it is run the removal
   feature may not be described as safe here or anywhere.

The count fell from 18 to 17 in the Phase 0 review: X11 claimed the race between a
late action and a timeout certificate was structurally excluded, and that claim was
false. A CP row removed because its argument did not hold is the review working as
intended, and the number is recorded as having moved rather than quietly restated.
The denominator has since risen seven times for the same reason in the other
direction: X30 to X36 are attacks the corpus was carrying without
knowing it, and adding them enlarges the total rather than the CP column. The
proportion classified as prevented falls when a review works — it has fallen from
18/47 to 17/55 across eight passes, and **not one of those passes made anything
cryptographically preventable that was not preventable before**. The last three
additions before X37 needed no attacker at all; **X37 needs no attacker either, and
it is the first whose cause is a decision this project took on purpose.**

**Neither D-010, nor D-011, nor D-012, nor D-013, nor D-014 moved the CP column at
all, and that is the point worth making about all five.** D-014 is the first to
give a payoff back rather than delete one — a tier-1 attacker now loses its seat —
and it still moved no row into CP, because what an adversary can *construct* is
unchanged by what happens to it afterwards. What it changed is the **worst case at
the end of a row**, which is where D-010 and D-011 rule 3 did their work too, in
the other direction. The three decisions should be read as one movement measured
three times: **the payoff at the end of a D&A row is the thing this corpus keeps
getting wrong**, first by making it too large (forfeiture, eviction on a predicate
that failed five times), then by deleting it entirely, and now by restoring the
part of it that rests on a message the accused signed. Removing automated forfeiture, then removing
automated eviction from every layer, did not make one attack cryptographically
preventable. D-010 made five attacks worthless and one attack free. D-011
rule 3 made four more worth less without moving a single label, and D-011 rule 2
closed a defect by widening a key rather than by adding a defence. The honest
summary is therefore not "the design got safer" but **"the design stopped
defending positions it could not hold, and stopped keeping copies of the rules
it does hold"** — paying for the first with X8 and for the second with a
document that is less self-contained (§9.1.3). A decision that improves a
corpus by deleting things should be visible in the counts as buckets moving
sideways, one row moving down, and the denominator going up. That is exactly
what §5.4 shows. **D-013 deletes rather than adds too** — it removes the question
of a seat's status from every progress rule instead of introducing machinery to
answer it — and it moved the CP column no more than the other three did.

**D-012 adds one row to DNA and nothing else, and the honest reading of that is
not reassuring.** It closes X33 by forbidding a derivation, so no attack becomes
preventable and no attacker loses a capability. **What it cost was stated here
wrongly, and the correction is worse than the claim**: this paragraph said the
price was that *"nothing marks a seat absent automatically any more, so a silent
seat is dealt in every hand and stalls each one to the hand deadline until a
human acts"* — a slow table, paid deliberately. It was not a slow table. It was a
fixed point in which the table never played again and no human could intervene,
because the only messages that could break the loop are single-writer by the
silent seat itself. That is **X34**, and its mitigation is **D-013**: liveness is
inherited from the chain rather than from a status, a silent seat stalls one hand
and is skipped, and it busts, so the tournament can end. The liveness price that
actually remains is one stalled hand per disconnection, which *is* a price paid
to avoid an integrity failure, in the order `SPEC_CS.md` §19 sets. No revision of
this file may restore the earlier sentence, and none may describe D-012 or D-013
as having made anything safer against an adversary.

**D-013 in turn adds two rows to DNA and one to a bucket that did not exist, and
that last one retracts a property this document has been claiming.** X34 and X35
are closed. **X36 is not**, and while it stands, the sentence *"every divergence
between honest peers is at least detected"* is **false** and may not be written
here in any form. Two honest peers can fork silently and both finish the
tournament believing they won it, with every check this corpus defines passing on
both sides. The count of qualifications on the CP number rises from six to seven
for that reason, and the seventh is the heaviest of them, because the other six
qualify what "prevented" means and this one qualifies what "detected" means.
`DECISIONS.md` **K-1** is the owner decision, and **K-3** is the smaller change
that would at least make the failure visible without deciding it.

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
open decision in `docs/DECISIONS.md`'s open list, carried corpus-wide as **OQ-A**
(§9.2); if it is defined, X29 and
limitation 5 may point at the definition and the stronger sentence becomes true
again. Until then, an offline-adjudication claim must not be quoted from this
document as a mitigation, here or anywhere else in the corpus.
