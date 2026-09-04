# STATE_MACHINE.md — the deterministic poker engine

Phase 0 specification document, required by `SPEC_CS.md` §29.

**Binding inputs**

| Source | What it supplies here |
|---|---|
| `SPEC_CS.md` §4 | automatic hand-to-hand progression, tournament preset, custom table parameters |
| `SPEC_CS.md` §11 | the required state fields, the rules list, "re-validate every incoming action" |
| `SPEC_CS.md` §13, §14, §15 | hash chain, replay/equivocation defences, `STATE_HASH` |
| `SPEC_CS.md` §19 | timeout, hand abort, evidence of which peer failed, no early decryption |
| `SPEC_CS.md` §26 | the property-test invariants |
| `SPEC_CS.md` §32 | heads-up first, then 3–6 with side pots and multiple all-ins |
| `SPEC_CS.md` §36 | do not smooth over a gap; mark it |
| `DECISIONS.md` D-005 | absent seat keeps its stack, pays blinds, takes no cards; mid-hand abort with evidence of which peer failed. **Its chip rule on abort — forfeiture — is revised by D-010 below**; the rest of it stands unchanged |
| `DECISIONS.md` D-006 | action timeout is auto check/fold, never an abort; timeout certificate |
| `DECISIONS.md` D-007 | corrects D-006: at two seats an action deadline is advisory, a fold-effect timeout certificate is forbidden, and no document may claim the certificate protects a two-seat table. **Its two-seat scope is superseded by D-008 below** — the rules are the same, the quantity they are scoped on is not |
| `DECISIONS.md` D-008 | generalises D-007: every rule that weakens, disables or gates the certificate is scoped on the **size of the required voter set `V`**, never on the seat count; a certificate whose voter set has fewer than two members has no effect; a seat leaves `V` only once a completed, valid certificate names it |
| `DECISIONS.md` D-009 | three systemic rules. **Rule 1:** no sequence of actions the protocol requires of an honest peer may produce a valid `EquivocationProof` against it — the slot key carries every field that legitimately varies, `TIMEOUT_VOTE`'s subject included; the key is one literal tuple in `PROTOCOL.md` §5.2.1 and this document reproduces no part of it (D-011 rule 2). **Rule 2:** a certificate below the `\|V\| >= 2` floor is inert in every document and at every table size — not chained, not evidence, no terminating effect, no `AbortRecord`, no chip movement, `kind = Crypto` and the hand deadline included; `PROTOCOL.md`'s reading wins (§5.2, §8.4 rule 6, T57, I29). **Rule 3:** state the discipline we enforce, never an absence — no claim in this document rests on the dependency graph |
| `DECISIONS.md` **D-010** | **an abort is neutral.** (1) Stacks are restored to their start-of-hand values; no chips move on an abort, in any direction, for any cause. (2) Attribution is recorded as evidence and has **no automatic consequence**. (3) **No automated eviction** — no seat is block-listed, unseated or penalised by the protocol on the strength of a proof. (4) **Superseded by D-015 below**: the point read that certificates and equivocation proofs *"are still **produced**, because they are how a human or a later version adjudicates"*, while consuming one never moves a chip or removes a player. They are not produced. Points 1–3 stand. Revises D-005's chip rule on abort; the accepted cost is that the rage-quit escape returns and must be stated plainly (§8.6, §8.7, §12) |
| `DECISIONS.md` **D-011** | **one normative owner per concept, and the slot key written down once.** **Rule 1:** `PROTOCOL.md` owns the wire — message shapes, the envelope, the chain, sequence numbers, the anti-replay slot, canonical bytes, receiver validation; **this document owns state and transitions** — phases, guards, legal actions, pots, invariants — and *references* the other's definitions by section number rather than restating them. Where the two disagreed, the owner won. **Rule 2:** the slot key is one literal tuple in `PROTOCOL.md` §5.2.1, `event_type` included, and no other document reproduces any part of it. **Rule 3:** D-010 point 3 binds every layer — no `block_peer`, no unseating, no allow/block list driven by a proof, anywhere. What it changed here: T55, T56 and T11's unseating deleted, §4.1's `stage_hash` formula and §5.2's restatement of the reconciliation round deleted and replaced by pointers, and every "blocked at the protocol layer" sentence removed (§13 items 20–26) |
| `DECISIONS.md` **D-012** | **canonical state is never derived from a per-receiver quantity.** Nothing that enters a state hash, a roster hash, a chained event body or the next hand's genesis may come from a value that can differ between honest receivers; canonical state changes only through a chained event every participant accepted, and everything else is a local view. What it changed here: **T46 no longer sets `status := Absent` from the accepted `HAND_ABORT` copy's `attributed`** (H1) — the terminal stage is witness-independent and there is no abort-versus-abort precedence rule, so that field is per-receiver and `PROTOCOL.md` §4.10 already forbade deriving a seat's state from it; **I30** is added as the invariant whose absence was the defect; §2.4, §5.2, §8.6, §8.7, §11 (Q7), §12 and §12.1 follow it. The two surviving restatements the gate named are replaced by pointers, together with four more found in the same sweep (H8 — §3.2, §5.2's T9 and T10, §7.8, §7.9, §8.2), and every count in the document is reconciled, with the retired transition numbers listed (§5.1, §9.5, §10) |
| `DECISIONS.md` **D-013** | **liveness is inherited from the chain, not from a seat's status.** *(J2)* **A seat is required to emit in hand `k+1` only if it signed at least one chained event during hand `k`; for the first hand the required set is the signers of `TABLE_READY`.** The wire half — `HAND_INIT`'s required emitter set — is `PROTOCOL.md` §4.4's and this document reproduces no part of it; **the engine half is `dealt_in`, and it is this document's** (§5.3 step 4, I31). What it changed here: `dealt_in` is gated on the same predicate, `signed_this_hand` is added as the canonical set that carries it (§2.6, §2.8), **I31** is added, **I30(c)**'s final clause is corrected from *"hence a `HAND_INIT` collective stage that completes"* to a byte-identity claim (J3), **Q7 is closed rather than widened again** and replaced by **Q8**, and every statement of the liveness cost in §5.2, §8.6, §8.7, §12 and §12.1 is rewritten, because the cost D-012 recorded — *"a stall that repeats every hand"* — was not the cost the machine had. D-013 also corrects that record: before it, **the table made no progress ever**, since every stack is restored at T46 so no seat could bust and no §9.3 condition could fire. The second half — `advert_hash` out of `GENESIS(0)`, `session_id` and `ctx` (J1) — is `PROTOCOL.md`'s and touches nothing here |
| `DECISIONS.md` **D-015** | **the timeout machinery is not produced in version 1, and this document therefore consumes nothing from it.** `TIMEOUT_VOTE`, `TIMEOUT_CERT` and `EquivocationProof` keep their wire definitions and their code points (`PROTOCOL.md` §4.8, §5.2.4) and **nothing emits one**, so no certificate and no proof ever reaches the engine. What it changed here: **`Event::TimeoutCertificate` is deleted from §4.1** and the six rows that consumed it — **T16, T22, T27, T34, T41, T44** — are deleted with it (§5.2); `certified_subjects` and `V(subject)` leave `TableState` and become §8.4's retained specification; **I29 is retired as vacuous**; §8.2, §8.4 and §8.5 are re-marked as defined-not-produced; and §12.1.1 is re-derived, which is the check that matters. **No exit moved** — every one of the twenty phases was already left by T4, T46, T47, T57 or T61, none of which is or ever was a certificate. What replaces the deleted rows is stated once and instantiated per row in §5.2: an **action** deadline is answered by the seat's own auto check/fold, a real single-writer `Action` event by the seat itself (T29–T33, §8.5), and a **cryptographic-step** deadline by `hand_deadline_ms` (T57), after which D-013's `signed_this_hand` predicate removes the silent seat from the next hand's required set |
| `DECISIONS.md` **open list, K-3 and K-7** | **K-3: a drain hand must place a checkpoint.** `PROTOCOL.md` §6.2's checkpoints 2–7 all require a `DECK_COMMIT` or a betting round, and §5.3 step 9 sends `\|dealt_in\| == 1` straight to `Settling`, so under D-013's steady state — about forty consecutive drain hands against a silent opponent (§12.1.2) — the corpus's only cross-peer detection route was inert and the tournament result itself was never checkpointed. What it changed here: **checkpoint `8`, the hand-boundary checkpoint** (§5.2's box), emitted at T45 and T46 on every hand; **T47 is gated on it on the settled path only**; **T61** is added as its timer-borne exit; `HandComplete` gains width on that path, which is **K-3b**'s answer — the phase in which `PLAYER_SIT_IN` is legal now has a window in which it can arrive; **I32** is added; §12.1 is re-derived. **K-7: the last status word on a progress path.** T59's exit from `Paused` was guarded on seats being *"willing"* while §5.3 step 4 decided the same question on participation, so three silent `Active` seats could pass the guard into a drain hand. The guard is restated as §5.3 step 4's own predicate, so there is one predicate and not two |
| `docs/research/POKER_RULES.md` | every NLHE rule, the TDA citations, the `RATED_SNG_POKERTH_V1` preset |
| `docs/research/MENTAL_POKER.md` | what the crypto layer can and cannot do, `n`-of-`n`, per-card reveal tokens |
| `docs/research/CRYPTO_LIBS.md` | canonical bytes (`minicbor` arrays, no maps, no floats), BLAKE3, the canonicality gate |
| `docs/research/GUI_STACK.md` | crypto must not run on the UI thread; the engine is called from both |

**What this document is not.** It contains no new API claims. Every crate-level fact it
relies on (`rs_poker =5.0.0` behind a façade, `minicbor` array encoding, `getrandom::SysRng`,
`ziffle` reveal tokens) is carried unchanged from the research documents together with the
verification recorded there. Nothing here was invented; where something is undecided it is
marked **OPEN QUESTION** rather than resolved.

**Counts.** Stated once, in §5.1 (phases and transitions) and §10 (invariants), and referenced
from everywhere else rather than repeated.

---

## 1. Position in the architecture

**Module this document governs** (`SPEC_CS.md` §23, interim mapping until
`docs/ARCHITECTURE.md` exists at the start of Phase 2): `src/poker/` — `state.rs`, `engine.rs`,
`actions.rs`, `pots.rs`, `tournament.rs`, `evaluator.rs`.

`SPEC_CS.md` §1 requires the network layer never to decide poker rules and never to create
cards, and it requires the poker rules and the deck security to be separate layers. That cuts
both ways, and this document takes the second direction seriously:

> **The engine knows no cryptography, and the cryptographic layer knows no poker.**

The engine never sees a group element, a proof, a secret key or a reveal token. It sees
*verdicts* (`ShuffleVerified`, `CardsOpened`, `RevealTokensPublished`) and it issues
*requests* (`RequestShuffle`, `RequestOpen { indices, audience }`). It identifies cryptographic
objects only by 32-byte BLAKE3 hashes, which are opaque to it.

Conversely the crypto layer never learns that index 7 is "the button's second hole card" or
that the flop may not be opened yet. It opens what it is asked to open, for the audience it is
told, and it refuses nothing on poker grounds. Street gating is entirely the engine's job —
`MENTAL_POKER.md` §7 states this explicitly: *"ziffle gives per-card tokens; withholding them
until FLOP/TURN/RIVER is our state machine's job… ziffle has no concept of 'too early'."*

This is why the crypto-waiting phases in §5 are **first-class states** rather than a boolean
flag. The engine must be able to sit in `AwaitingShuffle` for a second, or forever, without any
poker knowledge leaking downwards and without any crypto knowledge leaking upwards.

```
        GUI  ──reads──►  TableState        (never writes)
                             ▲
        LocalView (my hole cards, my key share, timers) — NEVER hashed, NEVER signed
                             ▲
   protocol/ordering ──Event──►  ENGINE  ──Effect──►  mental_poker / scheduler / net
   (total order, signatures,          pure
    canonicality, replay)          step(s,e)
```

---

## 2. The state type

### 2.1 Two state objects, and why the split is load-bearing

`SPEC_CS.md` §11 says *"the same inputs must produce an identical state on every client."*
That is only achievable if nothing per-node is in the state. Two things are per-node and must
therefore be excluded by construction, not by discipline:

* **My own hole cards.** Alice's state has `Ah Kh`; Bob's does not (`SPEC_CS.md` §9). If hole
  cards were a field of the canonical state, no two peers would ever agree on a `STATE_HASH`.
* **My own transport view** — which peer is relayed, which is direct, RTT, `PeerId` of the
  socket. `SPEC_CS.md` §20 is explicit that a `PeerId` is not an identity, so it has no place
  in a canonical poker state at all.

So:

* **`TableState`** — canonical. Byte-identical on every honest peer after the same event
  sequence. This is what is CBOR-encoded and BLAKE3-hashed for `SPEC_CS.md` §15's `STATE_HASH`.
* **`LocalView`** — per-node, never hashed, never signed, never sent. Holds my hole cards, my
  secret key share, in-flight reveal tokens, arm/disarm timers, connection quality, UI state.

Invariant **I24** below makes this a testable property rather than a convention.

### 2.2 Primitive types

```rust
pub type SeatIdx    = u8;          // 0 .. config.seats-1, a ring
pub type CardIndex  = u8;          // 0 .. 51, a position in the shuffled deck
pub type Chips      = u64;         // one denomination, smallest unit 1, never negative
pub type Hash       = [u8; 32];    // BLAKE3-256 (CRYPTO_LIBS.md §3)
pub type PlayerId   = [u8; 32];    // Ed25519 application signing public key (SPEC_CS.md §20)
pub type TableId    = [u8; 32];

/// index = rank*4 + suit ; rank 0=Two .. 12=Ace ; suit 0=c 1=d 2=h 3=s.
/// This encoding is canonical: it is hashed, so it may never change.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Card(u8);               // 0..=51

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Street { PreFlop, Flop, Turn, River }
```

Chips are integers with exactly one denomination (`POKER_RULES.md` A0). There are **no floats
anywhere in the state** — `CRYPTO_LIBS.md` §4.6 rule 3 forbids them in any signed or hashed
type, and the odd-chip rule (A8) is defined over integer remainder precisely so that no rounding
decision ever exists.

### 2.3 `TableConfig` — frozen before the first card

Twenty of its twenty-three fields come from the signed table advertisement (`SPEC_CS.md` §4, whose
wire form is `PROTOCOL.md` §7.2's `LOBBY_TABLE_AD`, and each of the twenty carries its `n(k)` field
number below), so all participants agreed on them before any cryptographic material existed. None
of the twenty-three changes during the table's life. The three that are not advertised are named
rather than left to inference: `protocol_version` and `table_id` come from the envelope and from
`GENESIS(0)` (`PROTOCOL.md` §3.1), and `auto_action_limit` is `PROTOCOL.md` §13's
`MAX_CONSECUTIVE_AUTO_ACTIONS`, two-sided by being a constant rather than by being advertised.

```rust
pub struct TableConfig {
    pub protocol_version:        u16,
    pub table_id:                TableId,
    pub preset_id:               PresetId,        // n(2)  — §7.2 rule 3's closed two-value enum
    pub game:                    Game,            // n(0)  Nlhe
    pub mode:                    Mode,            // n(1)  TournamentSngPlayMoney | CashPlayMoney
    pub seats:                   u8,              // n(11) max_players, NEVER the seated count
    pub min_players_to_start:    u8,              // n(12)
    pub start_stack:             Chips,           // n(9)
    pub first_small_blind:       Chips,           // n(13) BlindSchedule n(2)
    pub ante:                    Chips,           // n(6)
    pub blind_raise_mode:        BlindRaiseMode,  // n(13) BlindSchedule n(0)
    pub blind_raise_every_hands: u32,             // n(13) BlindSchedule n(1)
    pub small_blind_cap:         Chips,           // n(13) BlindSchedule n(3)
    pub button_rule:             ButtonRule,      // n(20)
    pub odd_chip_rule:           OddChipRule,     // n(21)
    pub showdown_policy:         ShowdownPolicy,  // n(22) — see §7.7, OPEN QUESTION
    pub action_timeout_ms:       u32,             // n(14)
    pub action_grace_ms:         u32,             // n(15) — §7.2's name; a consensus constant
    pub crypto_step_timeout_ms:  u32,             // n(16) — every DeadlineKind::Crypto duration
    pub hand_deadline_ms:        u32,             // n(17) — derived bound, never a constant:
                                                  //       >= HAND_DEADLINE_MIN(seats), PROTOCOL.md
                                                  //       §8.2, and <= n(17)'s cap
    pub join_deadline_ms:        u32,             // n(18)
    pub hand_delay_ms:           u32,             // n(19) — DISPLAY ONLY, see §8.3
    pub auto_action_limit:       u8,              // NOT an advert field: PROTOCOL.md §13's
                                                  // MAX_CONSECUTIVE_AUTO_ACTIONS (D-006 §4)
}
```

**No value is written down in this struct, only field numbers (D-011 rule 2).** The numbers
themselves live in `PROTOCOL.md` §13 for `RATED_SNG_POKERTH_V1` and in `PROTOCOL.md` §7.2's field
table for the per-field ranges every `CUSTOM` table is admitted under. Seven of these lines carried
the preset's value as a comment until this pass; `G7-S1` is what a second copy of a preset value
does in a document nothing can test it against, and a comment drifts exactly as a listing does.

**Two fields changed here, and both were `G7-S2`.** `action_timeout_grace_ms` is renamed
`action_grace_ms`, which is the name `PROTOCOL.md` §7.2, §8.2 and §13 all use — it was the only
site in the corpus carrying the other name, and §13 requires that no value have two names.
`crypto_step_timeout_ms` is **added**: it was in no `TableConfig`, yet `n(16)` is a signed advert
field and a part of `table_params_hash`, `DeadlineKind::Crypto` is armed by eleven transitions
whose duration comes from it (§2.6), and §9.4 enforces a bound whose largest term is a multiple of
it. The engine document armed a timer with no source and enforced a bound it could not compute.

**`TableConfig` is a projection of the advertisement, not the advertisement.** It holds the twenty
advertised parameters `step` reads and nothing else from the payload. `n(3) table_name`,
`n(7) min_buyin`, `n(8) max_buyin`,
`n(10) players`, `n(23) password_required`, `n(24) deck_suite`, `n(25) founder_app_key`,
`n(26) founder_peer_id`, `n(27)` and `n(28)` are absent because no transition reads them; they
belong to the lobby and join layers. Three of those — `n(7)`, `n(8)` and `n(24)` — are nevertheless
parts of `table_params_hash`, so **`table_params_hash` is computed over the advertisement and never
over this struct** (`PROTOCOL.md` §3.1's box, twenty-five parts in a fixed order). A peer that
reconstructs the hash from `TableConfig` derives a different value from every conforming peer, and
since the hash is in `GENESIS(0)` and in `PLAYER_LIST` it can then join no table at all.

`hand_delay_ms` is deliberately **not** an engine input. It is a presentation delay so a human
can see the result; the protocol never waits for it. See §8.3. It is in this struct because
`PROTOCOL.md` §8.2 makes it a **term** in two durations the engine's own config must be able to
supply — the hand-boundary `next_deadline_ms` and `HAND_DEADLINE_FLOOR(n)` — which is a budget,
not a wait (§2.6, §8.3).

`SPEC_CS.md` §4 permits a table founder to replace the preset with custom parameters. The engine
therefore validates the config before leaving `Seating` and refuses to start a table whose config
fails; a table that cannot be validated is closed rather than played (§9.4).

### 2.4 `Seat`

```rust
pub struct Seat {
    pub status:                   SeatStatus,
    pub player:                   Option<PlayerId>,
    pub stack:                    Chips,
    pub start_stack_this_hand:    Chips,
    pub committed_round:          Chips,
    pub committed_hand:           Chips,
    pub acted_this_round:         bool,
    pub folded:                   bool,
    pub all_in:                   bool,
    pub dealt_in:                 bool,
    pub consecutive_auto_actions: u8,
    pub revealed_hole:            Option<[Card; 2]>,   // public showdown result only
    pub mucked:                   bool,
}

pub enum SeatStatus {
    Empty,
    Active,      // a live client, plays hands
    SittingOut,  // client present, flagged out (D-006 §4). Pays blinds/antes, takes no cards.
    Absent,      // client gone (D-005).            Pays blinds/antes, takes no cards.
    Leaving,     // announced PLAYER_LEAVE at a hand boundary (T58). Not dealt in, posts
                 // nothing; removed and its stack taken off the table at the next hand
                 // init (§5.3 step 0), which is where ledger_out moves.
    Removed,     // D-014: removed for cause on self-authenticating evidence (T64, T65).
                 // Pays blinds/antes and takes no cards, exactly as Absent was specified
                 // to; unlike every other variant it is ONE-WAY (I34).
    Busted,      // stack == 0, eliminated, position retained for the dead button
}
```

**`Removed` is new and it is D-014's, and it is the first status in this enum that is absorbing.**
D-014 point 3 says a removed seat's *"seat becomes dead and its stack is blinded off, precisely as
D-005 treats an absent seat, until the stack is gone"*, and point 4 says the exit is one-way. Both
halves need a variant of their own: `Absent` is unreachable by construction (below) and would
anyway be re-enterable through T59, `SittingOut` is the seat's own declaration about itself and is
re-enterable by the same transition, and `Leaving` posts nothing and empties the seat at the next
hand init — which would take the stack off the table and change `ledger_out`, the one thing D-014
point 3 exists to prevent. So `Removed` behaves like `Absent` for the blinds (§5.3 steps 6–7) and
like nothing else for re-entry: **T59 refuses it, and no transition assigns any other status to a
seat that holds it** (I34).

**Two things this variant owes and does not have, and both are `PROTOCOL.md`'s (D-011 rule 1).**
First, a **wire representation**: `PROTOCOL.md` §6.1's `PublicTableState` carries `sitting_out` and
no longer carries `absent`, so `Removed` is canonical per-seat state that **no vector hashes**. Two
peers that disagree about a removal would therefore agree on every hashed vector — a removed seat
posts the same blinds and holds the same stack as an `Active` one that folds every hand — and the
disagreement would surface late and indirectly, through `pots.eligible`, or not at all. Second, the
**provenance**: I30(a) requires a status to move only through an event every participant accepted
as chain content, so the *evidence* must be chained, not merely received. Both are filed as
`D-014-3` in `DECISIONS.md`'s open list, and §5.2's D-014 box states what this document does in the
meantime.

`Leaving` exists because a departure is **announced** at one hand boundary and **applied** at the
next hand init: `PROTOCOL.md` §4.4 makes `HAND_INIT`'s `ledger_delta` the one place either ledger
counter may move, so the engine cannot empty the seat the moment the message arrives without moving
`ledger_out` outside that artefact. Unlike `SittingOut` and `Absent`, which pay blinds and drain, a
`Leaving` seat posts nothing: it is on its way off the table with the stack it has.

`SittingOut` and `Absent` behave **identically to the engine**. They are separate variants only
so the GUI and the reputation counter can tell "chose to sit out" from "vanished"; no transition
in §5 branches on which of the two it is. That is deliberate: D-005 and D-006 §4 prescribe the
same chip behaviour for both, and an engine that treated them differently would be inferring
liveness, which is not knowable deterministically.

**`Absent` has no producing transition, that is not an omission (H1, D-012), and since D-013 it is
not a gap either.** T46 used to set it from the accepted `HAND_ABORT` copy's `attributed`; that
line is deleted, because which copy a peer accepts is a quantity honest receivers can differ on and
a seat's `status` is canonical state. What changed under **D-013** is not that something now sets
the variant — nothing does — but that **nothing needs to**. The question "what marks a seat
`Absent`?" was asked for two passes as though the answer were what let the table keep playing; it
never was (J2). A vanished seat is skipped because it stopped signing, not because a status field
says so, and `dealt_in` reads the participation set directly (§5.3 step 4, I31). Status is now what
it says it is — *a seat's own declaration about itself* — and the two routes into non-participation
are both the seat's own chained events: `PlayerSitsOut` (T59), or `auto_action_limit` auto-actions
marking it `SittingOut` (§8.5).

**The variant stays, and it is a trap that must be labelled rather than tidied away (J5).**
`SeatStatus::Absent` is **unreachable in this version, deliberately**, and five live mechanisms
still read it: `PROTOCOL.md` §6.1's `absent` vector inside every `state_hash`, §4.4's `bb_seat`
constraint, §5.3 step 4's `dealt_in` filter, §5.3 steps 6–7's ante and blind posting, and T59's
sit-back-in guard. All five are correct with a permanently-false vector, and none of them is what
keeps the table alive. **An implementer who finds a status the engine never sets and wires it to a
local liveness signal — a dropped connection, a silent peer, a missed heartbeat — has forked the
chain at every checkpoint**, because that signal is per-receiver and `status` is canonical
(D-012, I30). Retiring the variant would remove `PublicTableState.absent` from `state_hash`, which
is a wire change and `PROTOCOL.md`'s call, not a tidying this document may make. See §5.2's note
under T58/T59, §8.6, I30 and I31.

### 2.5 `DeckState` — the crypto-waiting bookkeeping

Hashes only. No group elements, no proofs, no ciphertexts. The `MaskedDeck` (3432 bytes per
`MENTAL_POKER.md` §5.1) and every proof live in the `mental_poker` layer's own store keyed by
`hand_id`; putting them in `TableState` would put multi-kilobyte crypto blobs inside every
`STATE_HASH` for no benefit.

```rust
pub struct DeckState {
    pub participants:   SeatSet,                       // seats in this hand's n-of-n key
    pub keys_published: SeatSet,
    pub agg_key_hash:   Option<Hash>,
    pub shuffle_order:  Vec<SeatIdx>,                  // fixed before the chain starts
    pub shuffles_done:  u8,                            // index into shuffle_order
    pub chain:          Vec<ShuffleRecord>,            // one per completed, verified shuffle
    pub deck_commit:    Option<Hash>,                  // hash of the final MaskedDeck
    pub deal_map:       DealMap,                       // §7.8, fixed before the chain starts
    pub tokens:         BTreeMap<CardIndex, SeatSet>,  // who has published a token for which index
    pub opened:         BTreeMap<CardIndex, Card>,     // publicly opened cards only
}

pub struct ShuffleRecord { pub shuffler: SeatIdx, pub deck_hash: Hash, pub proof_hash: Hash }
```

`BTreeMap`, not `HashMap`: iteration order enters the canonical encoding, and `HashMap` order is
not deterministic. `SeatSet` is a `u16` bitset over seat indices, so it has no ordering question
at all.

### 2.6 `TableState` — the whole thing

```rust
pub struct TableState {
    // ---- identity and chain position (SPEC_CS.md §12, §13, §14) -------------
    pub config:              TableConfig,
    pub sequence:            u64,
    pub previous_event_hash: Hash,

    // ---- phase ------------------------------------------------------------
    pub phase:               Phase,      // §5

    // ---- hand identity and blind level ------------------------------------
    pub hand_id:             u64,        // from 1
    pub level:               u32,        // derived from hand_id, cached, asserted by I25
    pub small_blind:         Chips,
    pub big_blind:           Chips,

    // ---- seat ring and button ---------------------------------------------
    pub seats:               Vec<Seat>,  // len == config.seats; index IS the seat, order IS the ring
    pub button_pos:          SeatIdx,    // a POSITION, may be an empty seat (dead button)
    pub sb_pos:              SeatIdx,    // a POSITION, may be empty -> dead small blind
    pub bb_seat:             SeatIdx,    // always an occupied seat

    // ---- betting ----------------------------------------------------------
    pub street:              Street,
    pub board:               Vec<Card>,           // len ∈ {0,3,4,5}, cap 5
    pub current_bet:         Chips,
    pub last_full_raise:     Chips,
    pub player_to_act:       Option<SeatIdx>,
    pub aggressor:           Option<SeatIdx>,     // last bettor/raiser this street (TDA 17-A)
    pub history:             Vec<ActionRecord>,   // this hand, in order

    // ---- cryptographic progress -------------------------------------------
    pub deck:                DeckState,
    pub rng_beacon:          RngBeacon,           // §7.9, seating + initial button

    // ---- time, without a clock (D-006, D-008) ------------------------------
    pub deadline:            Option<Deadline>,
    //   certified_subjects: DELETED by D-015. It existed only to compute the size of
    //   V(subject) for the six certificate rows, and nothing writes it now that they
    //   are gone: no certificate is produced, so no seat can ever enter the set. A
    //   SeatSet that is provably empty on every trace is not state. 8.4 keeps the
    //   field's definition and its rule 7 as the specification a later version
    //   restores; restoring it means restoring the rows first, never the other way
    //   round. See 2.8's deletion record and I29.

    // ---- demonstrated participation (D-013) --------------------------------
    pub signed_this_hand:    SeatSet,   // every seat that has signed an event this peer accepted
                                        // as content of THIS hand. Cleared at hand init AFTER
                                        // step 4 has read it. It is the liveness gate: §5.3
                                        // step 4 and PROTOCOL.md §4.4's required emitter set
    pub readmit:             SeatSet,   // PROTOCOL.md §4.9's readmission set A: seats carried in
                                        // by a stale event of a finished hand. It widens the
                                        // ACCEPTED emitter set of the next HAND_INIT stage and
                                        // never a REQUIRED one (P2). It is NOT unioned into
                                        // signed_this_hand, it does not reach dealt_in, and it
                                        // does not reach solitary_since. Read once, at §5.3
                                        // step 4, where it is handed to the protocol layer;
                                        // cleared at step 8 (P2-e)

    // ---- the solitary regime (PROTOCOL.md §3.2, K-1 / K-9) -----------------
    pub solitary_since:      Option<u64>,  // the FIRST hand_id this peer EVER dealt with
                                           // |P(k-1)| == 1 — signed_this_hand alone, never
                                           // widened by readmit (P2-e). MONOTONE: written once, never
                                           // cleared, not even when P grows again (N4).
                                           // It is a FLOOR, not an answer: which hands were
                                           // solitary is PROTOCOL.md §4.0 step 10b's
                                           // retained record, and this field only
                                           // guarantees it never rejects a hand that
                                           // record admits (§2.6's lemma, I33(a))
    pub solitary_contradicted: bool,       // T62, T63 and T50-when-solitary set it; only
                                           // T53 clears it, and only on a reconciliation
                                           // two seats signed (N1); §9.3 condition 0.6
                                           // reads it

    // ---- the checkpoint store (PROTOCOL.md §6.2, §4.9) ---------------------
    pub checkpoints:         CheckpointStore,   // THREE slots, never more; §2.6's box

    // ---- the chip ledger (invariant I1, I28) -------------------------------
    pub ledger_in:           Chips,     // Σ buy-in of the seats Seating accepted; set once, at the
                                        // first hand init, and never moved again (P8, §9.4, Q6)
    pub ledger_out:          Chips,     // Σ removed stack over every accepted seat exit

    // ---- results and evidence ---------------------------------------------
    pub settlement:          Option<Settlement>,  // last completed hand
    pub finish_order:        Vec<SeatIdx>,        // busted seats, first busted first
    pub faults:              Vec<FaultRecord>,    // SPEC_CS.md §19 evidence
    pub abort:               Option<AbortRecord>,
    pub table_faulted:       bool,                // set once, by T54 only; §9.3 condition 0.5
}

pub struct ActionRecord {
    pub sequence: u64, pub seat: SeatIdx, pub street: Street,
    pub action: Action, pub amount_to: Chips, pub was_auto: bool,
}

pub struct Deadline {                 // a DESCRIPTION, not a timestamp
    pub kind:        DeadlineKind,    // Action | Crypto — exactly PROTOCOL.md §4.8's kinds 1 and 2
    pub subject:     SeatIdx,
    pub sequence:    u64,             // the sequence this deadline is attached to
    pub parent_hash: Hash,            // the event the deadline chains from
    pub duration_ms: u32,             // from config, by the box at the end of this section;
                                      // the engine never adds it to anything
}

pub enum DeadlineKind { Action, Crypto }

pub struct CheckpointStore {          // PROTOCOL.md §6.2 and §4.9; THREE slots and no container
    pub live:     Option<CheckpointState>,  // the checkpoint of the CURRENT chain that is open
    pub agreed:   Option<CheckpointState>,  // the most recent checkpoint of the current chain
                                            // whose STATE_ACK stage completed here (D-014 tier 2)
    pub boundary: Option<CheckpointState>,  // hand k−1's checkpoint 8, retained across the
                                            // boundary because PROTOCOL.md §4.9 accepts events
                                            // into it until TERMINAL(k) is fixed here (N6, P1)
}

pub struct CheckpointState {          // PROTOCOL.md §6.2 and §4.9's checkpoint-8 box
    pub hand_id:   u64,               // the CHAIN this checkpoint belongs to (§4.9's "Chain"):
                                      // hand k for checkpoint 8 of hand k, 0 for checkpoint 1
    pub number:    u16,               // §6.2's checkpoint number, 1..=8
    pub sequence:  u64,               // s_ckpt — the stage index of its STATE_HASH stage,
                                      // which is what §4.1 derives `round` from
    pub required:  SeatSet,           // the emitter set §6.2 gives this checkpoint
    pub heard:     SeatSet,           // seats whose STATE_HASH for it this peer has accepted
    pub own:       Hash,              // THIS peer's own state_hash for this checkpoint, fixed
                                      // when the record is opened and never rewritten. It is
                                      // what T49 and T50 compare an arriving value against (Q4)
    pub dissent:   Option<Hash>,      // the FIRST accepted state_hash for this checkpoint that
                                      // differs from `own`; None while the stage is unanimous.
                                      // `own` and `dissent` are T50's retained evidence and
                                      // `dissent.is_none()` is T51's unanimity test (Q4)
    pub acked:     SeatSet,           // seats whose STATE_ACK for it this peer has accepted
    pub agreed:    Option<Hash>,      // the checkpoint_hash, set by T51 when the STATE_ACK
                                      // STAGE completes — `acked ⊇ required`, not the first ack
}
```

**`checkpoints` is the field T49, T50 and T51 were already written against and no revision had
declared.** Their side-effect cells say *"record the signer in the checkpoint's agreement set"* and
*"record `checkpoint_hash`"*, and there was nowhere to record either. It is canonical state — a
pure function of the events this peer accepted, with no clock and no arrival fact in it — and like
`signed_this_hand` it is **not** a field of `PublicTableState`
(`PROTOCOL.md` §6.1) and this document does not ask for one: hashing the record of a comparison
into the value being compared is circular. `heard` and `required` are what **T47**'s gate on the
boundary checkpoint reads (§5.2, checkpoint 8).

> ### The checkpoint store — normative, and this is `P1`
>
> **The defect.** The field was `Option<CheckpointState>`, *"at most one is open at a time"*, and
> `PROTOCOL.md` §4.9 now makes hand `k`'s boundary checkpoint **outlive hand `k`**: a checkpoint-8
> `STATE_ACK` of chain `k` is accepted *"until `TERMINAL(k+1)` is fixed at this receiver"* (§4.9's
> `N6` box), and a checkpoint-8 `STATE_HASH` of chain `k` is compared — and on mismatch *"enters
> §6.3 at step 1"* — after `HAND_INIT(k+1)` has closed the window. One slot cannot hold both, and
> the slot is overwritten by hand `k+1`'s first checkpoint, so **three things were broken at once**:
> T51 could never set `agreed` for a boundary checkpoint, which left D-014's tier-2 precondition
> (*a completed `STATE_ACK` stage*) unsatisfiable at exactly the boundary `N6` was filed to fix;
> T50's `|checkpoint.required| == 1` conjunct was unreadable once the slot was gone, so `N1`'s
> second half never fired on the stale route; and a **non-solitary** receiver's stale boundary
> mismatch had no engine carrier at all. The last of the three is reachable on a **healthy** table,
> through the forwarding reorder (`PROTOCOL.md` §1.5) that §4.9's own `N6` argument relies on.
>
> **The structure, and it is not a container.** `CheckpointStore` is **three named `Option` slots**,
> allocated once with `TableState` and never grown: `live`, `agreed`, `boundary`. There is no `Vec`,
> no map, and nothing keyed on a quantity a sender chooses — which is what §27 forbids and what
> matters here, because this store is fed from the network.
>
> **The bound, stated as §27 asks.** **At most three `CheckpointState` values exist at any moment,
> for the whole life of the table**, each of fixed size: two `u64`, one `u16`, three
> `SeatSet`s of `MAX_SEATS` bits, one `Hash` and two `Option<Hash>` — **65 bytes per record more
> than the field it replaces and still not one byte an event chooses** (`Q4-e` below). No incoming
> event allocates a fourth, and
> no `hand_id`, `checkpoint` number or `sequence` an event carries can cause an allocation of any
> kind: an event that names no slot in the store is a `Rejection` (I21) and the state is
> bit-identical. That is the same rule `PROTOCOL.md` §4.0 step 10a states for its own store, in the
> same words, and for the same reason.
>
> ### What the record has to hold for the guards to be computable — normative, and this is `Q4-e`
>
> **The defect, and it is `P1`'s other half.** `P1` gave the store two slots whose records outlive
> the state they were derived from, and left the record holding `values: u8`, *"count of DISTINCT
> `state_hash` values seen"*, and **no `state_hash` at all**. Three guards were then uncomputable
> on those two slots and one of them was uncomputable everywhere:
>
> * **T49** reads *"the value equals this peer's own derivation at that checkpoint"*. On `live` an
>   implementation could re-derive it from `TableState`; on `agreed` and on `boundary` it cannot,
>   because the state that produced the value is gone — hand `k`'s boundary state is precisely
>   what hand `k+1` has already overwritten. The guard had **no readable term** on the two slots
>   `P1` added for it, which is the whole of what `P1` was filed to make readable.
> * **T50** reads *"two distinct `state_hash` values now exist"*. A **count** cannot be maintained
>   without the values: to decide whether an arriving hash increments it you must compare it
>   against what was already seen, and `values: u8` holds nothing to compare against. The counter
>   was therefore not a derived quantity but an undefined one, on every slot including `live`.
> * **T51** reads *"every value in `c` agrees"*. Same field, same gap.
>
> **The fields.** `values: u8` is **deleted** and replaced by `own: Hash` and
> `dissent: Option<Hash>`. `own` is this peer's own `state_hash` for the checkpoint, written once,
> when the record is opened — §6.2's points for checkpoints 1–7 and T45/T46 for checkpoint 8, the
> same moment §3.4 has the engine publish that body — and **never rewritten**, so a record in
> `boundary` still carries the value hand `k` was compared on while hand `k+1` runs. `dissent` is
> the first accepted value for that checkpoint that differs from `own`, and `None` while the stage
> is unanimous.
>
> **Why two values suffice and a set is not needed.** This peer's own copy is always one of the
> values in the stage — it publishes it (§3.4; I32(a) for checkpoint 8) — so the number of distinct values is exactly
> `1 + dissent.is_some()`, and *"two distinct values exist"* is *"some accepted value differs from
> `own`"*. A third distinct value adds a third signature to a divergence already detected and
> changes no guard: T50 has already fired, the phase is `Diverged`, and T50's own state excludes
> the row from firing again. Retaining every value would be a container fed from the network,
> which is what §27 and §2.6's bound forbid; retaining two is what the evidence clause needs —
> *"retain both signed values as evidence"* is `own` and `dissent`, and it is now the literal
> content of the record rather than an instruction with no field behind it.
>
> **The bound this adds, stated where the bound is.** One `Hash` and one `Option<Hash>` per
> record, three records, fixed for the life of the table; no `Vec`, no map, and nothing keyed on a
> quantity a sender chooses. `dissent` is written at most once per record — `or`, never
> reassignment — so no event can make a record grow.
>
> **What this does not do.** It adds no gate, no wait and no phase: every one of the three guards
> is re-derived below over the same triggers and the same targets, and §12.1's twenty rows are
> re-derived against it in §12.1.1. And it stays out of `PublicTableState` for the reason the
> store already does — hashing the record of a comparison into the value being compared is
> circular, and `own` **is** the value being compared.
>
> **The lifetime of each slot, and it is what makes three sufficient.**
>
> * **`live`** holds the checkpoint of the **current** chain that is open. Opening a checkpoint
>   overwrites it, under the supersession rule below — a hand's checkpoints are strictly ordered and
>   §6.2 places them one at a time — so the rule the single slot had is kept, scoped to one chain.
>   **Checkpoints `1`–`7` are opened where `PROTOCOL.md` §6.2's table places them and not by a row of
>   §5.2**, which is unchanged from before this pass and is why no transition is listed for them:
>   the engine writes `live` when it publishes its own `STATE_HASH` body for that point (§3.4), and
>   `own` is the hash of that body — one write, at the opening, for every checkpoint number.
>   Checkpoint 8 of hand `k` is the exception and is opened at a transition, **T45** or **T46**; it
>   stays in `live` for the whole of `HandComplete`, which is what **T47**'s gate reads.
> * **`agreed`** holds the most recently **superseded** checkpoint of the current chain whose
>   `STATE_ACK` **stage** completed here. It exists because checkpoints supersede one another
>   inside a hand while **T64**'s tier-2 guard has to be able to read one that has already been
>   superseded; without it that guard is unreadable for every checkpoint below the live one.
>   **It is written when `live` is overwritten and not when the stage completes** — opening a new
>   checkpoint of the current chain does `if live.agreed.is_some() { agreed := live }` before
>   `live := new` — so no record is ever in two slots at once and `named(h, n)` has one answer.
>   T51 writes `agreed` **on the record**, never into this slot. Cleared at hand init. It never
>   holds a `number == 8`, because checkpoint 8 is the last checkpoint of its chain and nothing
>   supersedes it — which is what makes I32(d)'s *one record with `number == 8` at a time*
>   checkable.
> * **`boundary`** holds hand `k−1`'s checkpoint 8. It is written at hand init — `boundary :=
>   live.take()`, in **T47** and **T61**, replacing the `checkpoint := None` those rows carried —
>   and it is dropped at the **next** T45 or T46, which is the moment `TERMINAL(k)` is fixed at this
>   peer and therefore the exact moment §4.9 stops accepting events into it.
>
> **T10 is the one hand init that moves nothing, and saying so costs one line.** The first hand init
> runs from `Seating`, where the only checkpoint that can have been placed is **checkpoint 1**, in
> the setup chain (`hand_id = 0`). It is not a boundary checkpoint and there is no hand `0` to
> retain it for, so T10 leaves `live` alone — hand 1's checkpoint 2 overwrites it in the ordinary
> way — and `boundary` stays `None` until the first T47 or T61. Nothing reads `boundary` before then:
> `named(h, 8)` finds no record for any `h` a checkpoint-8 event can name, so such an event is a
> `Rejection`, which is the correct answer for a checkpoint-8 copy of a hand that has not happened.
>
> **Three is sufficient and it is provably not four, and no record is in two slots.** Two boundary
> checkpoints never coexist: hand `k`'s is released at the same transition that opens hand `k+1`'s,
> because both are keyed on `TERMINAL(k+1)` being fixed. `live` and `agreed` belong to one chain,
> are disjoint by the supersession rule above, and are cleared together at hand init. So the maximum
> is one per rôle, the rôles are three, and `named(h, n)` matches at most one slot — which is the
> property T49, T50 and T51 rest on and I32(d) asserts.
>
> **The engine's retention is a superset of the wire's acceptance windows, deliberately.** §4.9
> closes the checkpoint-8 `STATE_HASH` window at a complete `HAND_INIT(k+1)` and the `STATE_ACK`
> window at `TERMINAL(k+1)`; the engine holds the record until the later of the two. The engine
> therefore never lacks a slot for an event the wire admits, and never invents acceptance for one
> the wire has already dropped — the wire decides admission and the engine decides nothing about
> it. An implementer who reverses the inequality re-creates `P1`.
>
> **Which slot an event names, and why `Event::StateHash` and `Event::StateAck` now carry
> `hand_id`.** With one slot there was one answer; with three the engine must select, and the
> selector is `(hand_id, number)`: a `StateHash` or `StateAck` naming hand `h` and checkpoint `n`
> is matched against the slot whose record has `hand_id == h ∧ number == n`, and against no other.
> `hand_id` is **not a new wire field**: it is in the envelope of every chained event and inside
> `TO_BE_SIGNED` (`PROTOCOL.md` §2.3, §2.4), so the protocol layer reads it off an envelope it has
> already validated, exactly as §4.1 already says it does for `sequence`. Nothing is added to the
> wire and nothing is guessed.
>
> **Two read accessors, so no row invents a search.**
>
> * **`named(h, n)`** := the record in the store with `hand_id == h ∧ number == n`, or `None`. At
>   most one slot can match, by the lifetimes above. **T49**, **T50** and **T51** read it, and an
>   event that names no record is a `Rejection` (I21) — never an insertion.
> * **`agreed_checkpoint(h)`** := the record in the store with `hand_id == h` and `agreed.is_some()`
>   and the greatest `number`, or `None`. **T64** and **T65** read it and nothing else does.
>
> Both scan three slots. No other row in §5.2 may look into the store by any other route, which is
> D-011 rule 2's discipline applied inside one document: one selector, written once.
>
> **What this store does not do, said because it is the thing that would have to be argued.** It
> adds **no gate and no wait**: `agreed` and `boundary` are read-only bookkeeping after the boundary
> has been left, no transition blocks on either, and the only gate in the document is T47's, which
> reads `live` exactly as it did before. §12.1's twenty rows are therefore re-derived against the
> store and none of them moves — which is a check run in §12.1 and not a claim made here.

**`DeadlineKind` has exactly two variants, and `Join` is not one of them (N9(b)).** An earlier
draft carried a third, `Join`, for T4. `PROTOCOL.md` §4.8 defines only `1` = action deadline and
`2` = cryptographic-step deadline, and §8.4 rules on the gap in terms: *"A certificate kind for a
join deadline does not exist in this document, and none should be invented to fill the gap — there
is no gap."* The engine follows that ruling rather than contradicting it: the variant is deleted,
T4's trigger is a lobby-layer abandonment and not a certificate, and the engine's event alphabet
now contains no variant that no wire message can produce.

**`table_faulted` is new, and it is what makes `PROTOCOL.md` §6.4 reachable.** §6.4 is normative
that `cause = 4` **closes** the table — *"it is closed, no further hand is dealt"* — and this
document had no state in which that was recorded and no end condition that read it, so a
`StateDivergence` abort settled at T46 and T47 dealt the next hand. It is set by **T54** and by
nothing else, it is never cleared, and it is read by **§9.3 condition 0.5**. It is canonical state
because both peers derive it from the same completed reconciliation stage, so it enters
`STATE_HASH` like any other field. Nothing else in the corpus faults a table: `cause = 1`,
`2` and `3` all leave it playing, which is what "no timeout ends the tournament" means (§8.1,
D-006 §5).

**`solitary_since` and `solitary_contradicted` are new, they are the engine half of
`PROTOCOL.md` §3.2's solitary-stage rule (K-9), and neither is hashed.** Like `checkpoints`
and `signed_this_hand` they are **not** fields of `PublicTableState`
(`PROTOCOL.md` §6.1) and this document does not ask for one. That is deliberate and it is the
answer to the objection an implementer will raise first: *"is a per-receiver freeze not exactly
what D-012 forbids?"* It is not, and the distinction is the one D-012 itself draws. D-012 forbids
canonical state being **derived** from a quantity honest receivers can differ on, because such a
state forks the chain. Whether the contradicting event reached *this* peer is such a quantity — and
what it produces here is a peer that **emits nothing, completes no stage and writes no chained
event**. `Diverged` is already entered per-receiver at T50 on exactly the same footing, and
`faults` is already a per-receiver record. Detection state that stops a peer is a different object
from canonical state that binds every peer, and the corpus already contains the class; what it did
not contain, before this pass, was the sentence saying so.

**Why the field is monotone, and why the contiguity argument it replaces was false (N4).** The
obvious construction is a retained per-hand map — `(hand_id, was_solitary, P(hand_id))`, LRU-capped
— and **`PROTOCOL.md` §4.0 step 10b's retained hand record is exactly that map**, on the receiver
side, at the point the late event is evaluated. What stood here kept a *second* answer to the same
question and justified it with an interval: *"the solitary regime is contiguous, so the set of hands
this peer dealt solitary is `solitary_since ..= hand_id`"*. **The interval claim is false, and it is
false for the one reason the regime can be left at all.** `P` grows, inside the regime, through an
accepted `0x0804 PLAYER_SIT_IN`; nothing stops it shrinking again at the next hand init when the
seat that sat in stops signing. Hands 5–7 solitary, hand 8 not, hands 9–11 solitary is a legal
history, the solitary hands are a *union* of intervals, and `solitary_since := None` at hand 8
erased the first one. A contradicting event naming hand 6 then arrives with `PROTOCOL.md`'s record
saying *solitary* and this document's guard evaluating `9 <= 6`: **the wire says freeze and the
engine says reject**, and what is discarded under I21 is precisely the evidence the freeze exists
for. That is N4.

**The resolution is one memory and one floor, and not two memories.** `RetainedHand.was_solitary`
(`PROTOCOL.md` §4.0 step 10b) is the **sole authority** on whether hand `k` was solitary at this
receiver; this document keeps no second answer and asserts none. `solitary_since` becomes
**monotone** — written at the first hand init that reads `|signed_this_hand| == 1` (§5.3 step 8,
`P2-e`) and **never cleared, at any hand init, for any reason** — and it is a floor rather than an
answer:

> **The set it is read off is `signed_this_hand` and not a widened one, and that is `P2-e` and the
> whole of `K1`'s payoff.** The regime `PROTOCOL.md` §3.2 defines is *the required emitter set of
> the stage has one member*, and since `P2` the required set is `P(k-1)` exactly: `A` widens the
> stage's **accepted** emitter set and never its required one (§4.9). A peer holding
> `P(k-1) == {self}` and a non-empty `A` therefore still self-completes stage 0 against a set of
> one and is still in the regime — and reading the regime off `signed_this_hand ∪ readmit` gave
> `|admitted| == 2` at exactly that peer, wrote nothing, and left `solitary_at(k)` false for the
> whole episode. **T62 and T63 were then dead for every hand of it**, which is K-1's solitary
> tournament win restored through the engine after the wire had closed it. The union is deleted
> from both readers in this pass; what is left is one set with one reader each.

> **`solitary_at(k)` := `solitary_since == Some(j) ∧ j <= k + 1 ∧ k <= hand_id`.** One predicate,
> read by **T62** and **T63** — the two rows that consume a `SolitaryDivergence`, which is the only
> event that names a hand other than the current one — and by nothing else in this document. **T50
> does not read it**: the checkpoint it compares is named by its own `(hand_id, number)` and its
> regime test is read off that checkpoint's `required` set (§5.2), which needs no memory at all.
>
> **`j <= k + 1` is `N-1e` and it is exactly one hand of slack, neither more nor less.** The form
> that stood here was `j <= k`, and it was **false for the one class of hand the fix exists for**.
> `PROTOCOL.md` §3.2's regime test is a **disjunction** — `P(k-1) == {self} ∨ P(k) == {self}` — and
> K-1's own walk lands on the second disjunct first: the hand `P` narrows in has three seats in
> `P(k-1)` and one in `P(k)`, so nothing is written at hand `k`'s init and `solitary_since` becomes
> `Some(k+1)` at hand `k+1`'s. Evaluating `j <= k` gave `k + 1 <= k` and the engine dropped the
> best-evidenced contradiction the wire can deliver. It is written `j <= k + 1` rather than
> `j - 1 <= k` because `j` is a `u64` and the two are the same inequality without the underflow;
> `j >= 1` always, since the field is written only at a hand init and `hand_id >= 1` there.
>
> **Lemma — the two memories are ordered, which is what N4 asks for, restated over the
> disjunction.** If `PROTOCOL.md`'s retained record says hand `k` was solitary at this receiver,
> then `solitary_at(k)` holds. *Proof,* by the disjunct the record fired on.
>
> * **First disjunct, `P(k-1) == {self}`.** `P(k-1)` is the value §5.3 step 4 reads at hand `k`'s
>   init, so that init read a one-member set and step 8 executed
>   `solitary_since := solitary_since.or(Some(k))`. Hence `solitary_since == Some(j)` with
>   `j <= k <= k + 1`.
> * **Second disjunct, `P(k) == {self}`.** `P(k)` is the value §5.3 step 4 reads at hand `k+1`'s
>   init, by the same rule one hand later, so **hand `k+1`'s init** read a one-member set and wrote
>   `solitary_since := solitary_since.or(Some(k+1))`. Hence `j <= k + 1`, which is the slack and the
>   whole of it.
>
> Monotonicity is the rest of both cases — no other step assigns the field, so `j` can never rise
> above what it was first set to — and `k <= hand_id` holds because `hand_id` never decreases and
> hand `k` was reached. ∎
>
> **The second case rests on an ordering inside T47 and it is worth naming, because it is the one
> thing that could break the lemma silently.** §9.3's end conditions are evaluated **after** hand
> init has run, so step 8 writes `solitary_since` even on the boundary at which the table closes.
> Without that ordering a peer could record hand `k` as solitary by the second disjunct, close the
> table at that same boundary, and hold `solitary_since == None` — and **T63**, whose whole purpose
> is to retract a tournament result inside `TableClosed`, would never fire. The ordering is stated
> in §9.3's first line and is not restated here as a rule; what is stated here is that the lemma
> depends on it.
>
> **Corollary.** The engine never rejects an event the wire freezes on — which is the failure N4
> names — and the conjunction of the two tests *is* the wire's test: the engine's is a superset,
> the wire's is exact.
>
> **The residual, named rather than argued away, and it is not the off-by-one.** §5.3 step 8 reads
> `signed_this_hand` at hand `m`'s init, and since K-3b that set spans the **boundary window** — it
> is `P(m-1)` together with every seat heard from between the two hand inits. So a seat that signed
> only in the window makes the set two-membered, step 8 writes nothing, and the floor can be `None`
> for a hand whose record says solitary by the second disjunct. **It costs no detection and the
> reason is structural**: a non-empty window means another seat signed a chained event this peer
> accepted, so hand `m` is not solitary at this peer either, and hand `m`'s stage 0 is required of a
> set containing that seat. If the seat is really there the fork does not exist; if it is not, stage
> 0 stalls and the hand aborts loudly at `hand_deadline_ms`. What the floor loses in that corner is
> a freeze on an **older** hand, and what replaces it is a stall on the **current** one. Widening
> step 8 to read the boundary checkpoint's `required` snapshot — which is `P(m-1)` exactly, and
> which the store above now retains — would close it; it is **not** done in this pass, because a
> second read of a second quantity for one question is the defect K-7 and L7 each cost a pass, and
> the residual is loud where the off-by-one was silent. It is recorded on `DECISIONS.md`'s open
> list rather than fixed here.

**What the floor costs, said plainly because it is a weakening, and it now costs one hand more.**
`solitary_at(k)` is true for a hand inside a gap between two solitary episodes — hand 8 above —
which this document alone can no longer exclude, and since `N-1e` it is true for **one hand before
the first solitary episode** as well. Both cost nothing reachable, for one reason: a
`SolitaryDivergence` exists only where §4.0 step 10b's record says *solitary*, and for hand 8, and
for hand `j - 1` where the regime was entered on the first disjunct, that record says otherwise, so
the event is disposed of on the wire and never reaches `step`. **The engine's floor never
manufactures a freeze; it can only fail to reject one the wire has already decided on** — which is
the whole shape of the ordering and the reason a floor is the right object here. What the floor does
still exclude is the case worth excluding — a peer that has **never** been solitary holds `None`,
and T62 cannot fire on it at all. That is also why the freeze needs `hand_id >= 1` and why §12.1's
*no hand started* row stays unreachable from T62: with `j >= 1` the slack reaches at worst hand `0`,
which no record names.

**The two memories forget differently, and both errors point the same way.** The wire's record is an
LRU of `MAX_RETAINED_HAND_RECORDS` hands (`PROTOCOL.md` §13 owns the figure); this field is one
`u64` that forgets nothing. An
evicted hand answers *"not solitary"* and its late events are dropped — a missed detection, which
`PROTOCOL.md` §3.2 states and bounds — while the floor can only ever admit more. Neither error can
produce a freeze on the *wrong* hand. Two memories whose errors point in opposite directions is the
object N4 objected to; this pair is ordered, and the ordering is asserted as I33(a) rather than
argued here.

**One consequence to carry forward when `D-014-3` lands.** T64 and T65 take a seat out of
`signed_this_hand`, so a removal can itself shrink `P` to one member and put this peer into the
solitary regime at the next hand init. The monotone floor covers that entry with no edit, because it
records the first solitary hand however the regime was entered, and the per-hand answer stays
`PROTOCOL.md`'s.

`config.join_deadline_ms` stays in `TableConfig` because it is a signed parameter of the
advertisement that the **lobby layer** runs its formation timer from (`PROTOCOL.md` §4.3). No
`Deadline` value is ever built from it and `step` never reads it; it is carried so that every peer
agrees on the number the layer above is using, and so that §9.4's config validation can bound it.

**Where `Deadline.duration_ms` comes from — one source per stage kind, and this is `G7-S2`.**
`PROTOCOL.md` §8.2 owns `next_deadline_ms` and its three rows; no value of theirs is restated here.
What this document owes, and did not have, is the mapping from those rows to the `TableConfig`
fields the engine hands the scheduler, because `duration_ms` was documented as *"from config"* and
one of the two fields it needs was not in `TableConfig` at all:

| The stage the deadline covers | `kind` | `duration_ms` |
|---|---|---|
| a betting action | `Action` | `config.action_timeout_ms + config.action_grace_ms` |
| any cryptographic contribution, and a `STATE_HASH` / `STATE_ACK` round | `Crypto` | `config.crypto_step_timeout_ms` |
| the hand boundary — the `HAND_INIT` that follows `hand_delay_ms` | `Crypto` | `config.hand_delay_ms + config.crypto_step_timeout_ms` |

**Three sources, two `DeadlineKind`s, and the third row is deliberately not a third kind.**
`PROTOCOL.md` §4.8 defines two deadline kinds and the wire carries `next_deadline_ms` as a number,
so a boundary deadline is a `Crypto` deadline whose duration includes the display delay a peer is
*allowed* to spend before publishing `HAND_INIT` (§8.3). Adding a third `DeadlineKind` here would
put a value on the wire §4.8 has no code for.

**Every arming site now has a source, and the third row has none because the engine does not arm
it.** §5.2 arms `Action` at four sites — T26, T29, T31, T37 — and `Crypto` at eleven — T2, T7,
T10, T14, T18, T19, T32, T33, T38, T39 and §5.3's `|dealt_in| >= 2` branch, which is the arming
that follows hand init on both the T10 and the T47 path. All fifteen take one of the first two
rows. **The boundary row belongs to the layer that also runs `hand_deadline_ms`** (§8.2's second
timer): the stage it covers is `HAND_INIT` itself, and no transition of this document is executing
while that stage is open. The engine's obligation for it is exactly to *carry the two fields*, which
is why `hand_delay_ms` is in `TableConfig` despite being display-only (§2.3, §8.3).

`config.hand_deadline_ms` is the other field no `Deadline` is built from: like `join_deadline_ms` it
is carried so that every peer agrees on the number the layer above is using, and so that §9.4 can
bound it. `step` reads neither.

### 2.7 `SPEC_CS.md` §11 conformance table

| §11 requires | Field | Type |
|---|---|---|
| `table_id` | `config.table_id` | `[u8; 32]` |
| `hand_id` | `hand_id` | `u64` |
| `button` | `button_pos` | `SeatIdx`, a **position**, may be empty |
| `small blind` / `big blind` | `small_blind`, `big_blind` | `Chips` |
| `player order` | `seats` | `Vec<Seat>`; the index *is* the ring order |
| `stack` | `Seat::stack` | `Chips` |
| `current bet` | `current_bet` | `Chips` |
| `pot` | **derived** `pots(&self) -> Vec<Pot>` | see below; it calls `build_pots(committed_hand, folded, dealt_in)` (§7.5) |
| `side pots` | **derived**, same function | `Vec<Pot { size, eligible }>` |
| `street` | `street` | `Street` |
| `player_to_act` | `player_to_act` | `Option<SeatIdx>` |
| `minimum_raise` | **derived** `current_bet + last_full_raise` | `Chips` |
| `last_full_raise` | `last_full_raise` | `Chips` |
| all-in states | `Seat::all_in` | `bool` per seat |
| fold states | `Seat::folded` | `bool` per seat |
| `board` | `board` | `Vec<Card>`, cap 5 |
| betting history | `history` | `Vec<ActionRecord>` |

**Justification for `pot` and `side pots` being derived rather than stored.** `POKER_RULES.md`
A7 requires pots to be a pure function of `committed_hand[]` and `folded[]`, never incrementally
mutated. Storing them would create a second source of truth that can drift from the commitments,
and drift between two independent implementations is precisely what `SPEC_CS.md` §15 turns into a
false dispute. Deriving them makes `Σ pots + Σ refunds == Σ committed` exact by construction
(invariant I6) and makes `STATE_HASH` well-defined without hashing a redundant field. The GUI
calls `pots()` for display; the settlement code calls the same function; there is one algorithm.

`minimum_raise` is derived for the same reason: it is `current_bet + last_full_raise` by
definition, so storing it is storing a fact twice.

### 2.8 Fields added beyond §11, each with its justification

| Field | Why it must exist |
|---|---|
| `phase` | §11 names `street` but not the cryptographic waits. Without an explicit phase the engine cannot *sit and wait* for the crypto layer, which is the whole point of §1's layering. |
| `Seat::committed_round` | `to_call` and `can_reopen` are undefined without it (`POKER_RULES.md` A5, appendix). |
| `Seat::committed_hand` | `build_pots` input (A7), and the quantity an abort returns to the seat that put it in (§8.6; under D-010 it is never redistributed). |
| `Seat::acted_this_round` | Encodes the big blind's option; round termination and `can_reopen` both read it (A2, A5, appendix). |
| `sb_pos`, `bb_seat` separate from `button_pos` | The dead button (A1.3) rotates three independent positions; one field cannot express a dead button *and* a dead small blind. |
| `Seat::start_stack_this_hand` | Abort settlement (D-005) and the simultaneous-bust-out tie-break (A1.3) both need the value as of hand start. |
| `Seat::dealt_in` | D-005: an absent seat pays blinds and takes no cards. Without this field "pays but is not a party to the cryptography" is inexpressible. |
| `SeatStatus` | D-005 / D-006 §4: sitting-out and absent seats keep their stacks and drain. |
| `Seat::consecutive_auto_actions` | D-006 §4: after `auto_action_limit` consecutive auto-actions the seat is marked sitting out. **Never incremented in version 1 (D-015, §8.5)**: the counter was written by T34 from a certificate, and an auto-action a seat signs itself is indistinguishable from a human's, so no peer may count another seat's. The field and `auto_action_limit` are retained for the version that restores the certificate; **an implementer must not resurrect the counter from a local guess**, because a per-receiver count driving a `status` is exactly D-012's prohibition (I30). |
| `Seat::revealed_hole`, `Seat::mucked` | Showdown results are public and identical everywhere; a verifier re-computes the award from them. |
| `deadline` | D-006 requires the deadline to be **explicit state**, not a wall-clock read inside the engine. |
| ~~`certified_subjects`~~ | **Deleted by D-015, and the deletion is recorded here rather than the row removed.** The field's justification was D-008: the required voter set `V` shrinks **only** by a completed, valid certificate, never by an assertion, and without the field the engine could not compute `\|V(subject)\|` deterministically. That justification is intact and it is now **inert**: with T16, T22, T27, T34, T41 and T44 deleted, no transition reads `V(subject)` and no transition writes the set, so it is empty on every trace this engine can produce. **A field no transition writes is not state**, which is the same rule this document has applied four times to an event variant no transition consumes (`RevealRejected`, `StateAck`, `TimeoutCertificate{Join}`, `Event::EquivocationProof`). It leaves `TableState` (§2.6), the hand-init reset that cleared it (§5.3 step 8), T57's *"unchanged"* note, and T64's `-= {subject}` clause. **What a later version must restore, in order:** the rows first, then this field with §8.4 rule 7 unchanged, then I29. Restoring the field alone gives an engine a set nothing fills; restoring the rows alone gives them a `V` they cannot compute — and the second failure is the one that reopens N3, because an implementer who cannot compute `V` will reach for `deck.participants` and the seat count. |
| `signed_this_hand` | **D-013.** The liveness gate is *demonstrated participation in the agreed chain*, and no other field expresses it. `status` cannot: a silent seat emits nothing, so nothing can change its status, so it stayed a required emitter forever (J2) — that circularity is the whole of what D-013 removes. It is maintained by one rule that is not attached to any single transition: **on accepting any event as content of hand `k`, `signed_this_hand ∪= {sender_seat}`**, applied by `step` before the transition table is consulted, for every accepted event of every kind including a rejected-then-superseded one's replacement. It is read exactly once, at §5.3 step 4, and cleared at §5.3 step 8. It is canonical state — a pure function of the events this peer accepted, with no clock, no timer and no per-message arrival fact in it. Like `certified_subjects` it is **not** a field of `PublicTableState` (`PROTOCOL.md` §6.1) today; unlike `certified_subjects`, a disagreement about it is not invisible, because it lands in `HAND_INIT`'s `n(8) dealt_in`, which every receiver of a collective stage recomputes and rejects on mismatch. **This document now does ask for one, and that is K-3's second half.** The mutual-rejection argument covers the case where the two peers still consider each other required emitters; it does not cover the case where each has concluded the other is not, which is `DECISIONS.md` **K-1**, and there the stage self-completes at each peer and nothing is compared at all. Putting the field in `PublicTableState` is what makes checkpoint 8 (§5.2) compare the quantity the fork is actually in. The decision is `PROTOCOL.md`'s under D-011 rule 1 and is recorded on `DECISIONS.md`'s open list, not taken here. The one input to it that is **not** agreed by construction is named in **Q8**, and I31 is what a harness asserts. |
| `checkpoints` | **K-3, and its shape is `P1`.** T49, T50 and T51 have always had side effects that write a checkpoint's agreement set and its `checkpoint_hash`, and no field held either, so three transitions wrote to nothing. It becomes load-bearing rather than merely missing with checkpoint 8: **T47**'s gate on the settled path reads `heard ⊇ required` on `checkpoints.live`, which holds hand `k`'s boundary checkpoint for the whole of `HandComplete` (§5.2), and that gate is what puts a tournament result through a cross-peer comparison before `TableClosed` absorbs it. It is canonical state and outside `PublicTableState` for the reason given under §2.6: a checkpoint is a comparison of `state_hash` values, and putting the record of the comparison inside the value compared is circular. **It is three slots and not one**, because `PROTOCOL.md` §4.9 now accepts checkpoint-8 events of chain `k` after hand `k+1` has started; the bound, the lifetime of each slot and the proof that three suffice are §2.6's box. **And the record carries `own` and `dissent` rather than `values: u8`, which is `Q4-e`**: with two slots whose state has been overwritten, *"equals this peer's own derivation"* and *"two distinct values now exist"* had no term in the record to read, so `P1`'s two new slots carried three guards none of which could be computed on them. §2.6's `Q4-e` box is the derivation. |
| `readmit` | **`P2-e`, which supersedes `N-5e`.** `PROTOCOL.md` §4.9 defines a **readmission set `A`** — a stale `PLAYER_SIT_IN` of a finished hand's boundary window, or a stale checkpoint-8 `STATE_HASH` that **agrees** with this receiver's retained value, carries its sender into the next hand init instead of being dropped — and since `P2` §4.9 states the consequence as an **acceptance** and not a requirement: `R(HAND_INIT, m+1)` stays `P(m)`, and `A` widens that one stage's **accepted** emitter set to `P(m) ∪ A`. The field is the engine's holder of the same set, and what it exists for is **not** a predicate: it is read once, at §5.3 step 4, where it is handed to the protocol layer as that stage's accepted-emitter widening, and cleared at step 8. **It is in no guard in this document and it reaches neither `dealt_in` nor `solitary_since`** — the union `N-5e` asked for is deleted, because a set written from a stale event for which `PROTOCOL.md` §4.0 step 10a is skipped is replayable, and a replayable input to a **required** emitter set stalls stage 0 once per hand for as long as the record is retained (`P2`). Its bound is one `SeatSet`: `\|A\| <= MAX_SEATS` by construction, because it is a set of seats. **The one thing only this document can supply is the filter**, and it is why the field is not simply deleted in favour of the wire's copy: **T67**'s guard excludes a `Removed` seat, which the wire has no status to test, so without it §4.9's `A` is the re-entry route D-014's one-way exit forbids (I34(a)). It is **not** a field of `PublicTableState` and this document does not ask for one; unlike `signed_this_hand` nothing it touches is hashed at all, because since `P2` it touches no derived quantity — a disagreement between two peers about `A` costs an accepted copy at one stage and cannot move `dealt_in`, a pot or a `state_hash`. |
| `sequence`, `previous_event_hash` | §12/§13/§14: monotonic sequence and parent hash are what make replay, reordering and equivocation detectable. |
| `level` | Tournament layer (§9); cached derivation of `hand_id`, asserted by I25. |
| `deck` | The crypto-waiting bookkeeping. Hashes only; see §2.5. |
| `rng_beacon` | §7 commit/reveal for seat order and initial button (`MENTAL_POKER.md` §6). |
| `aggressor` | TDA 17-A order of show (`POKER_RULES.md` A8). |
| `settlement` | Lets any peer re-check "the winner and the pot size were computed correctly" (§13) from the transcript alone. |
| `finish_order` | Tournament result; needs a total order with no floor person (A1.3). |
| `faults`, `abort` | §19 requires *evidence of which peer failed*, in the transcript, signed. |
| `table_faulted` | `PROTOCOL.md` §6.4 is normative that `cause = 4` **closes** the table — *"no further hand is dealt"* — and no other field expresses it: `abort` is overwritten by the next abort, and `faults` records a divergence a later reconciliation may have resolved. Set by **T54** only, never cleared, read by §9.3 condition 0.5. It is **not** a field of `PublicTableState` (`PROTOCOL.md` §6.1), and this document does not ask for one: §6.1's list is that document's and the field does not need to be in it, because both peers derive it from the **same completed reconciliation stage**, which is chain content every participant validated for itself, so there is no route by which two honest receivers hold different values. **The second reason this row used to give is withdrawn (K-3):** it read that T54 is the last transition of the last hand and *"no further checkpoint is ever emitted"*, which was true when checkpoints only ever sat inside a hand and is false now — T54 reaches `HandComplete` through T46, and every entry to `HandComplete` opens checkpoint 8 (§5.2). That changes nothing about the field, because the derivation argument above never depended on the absence of a checkpoint; it is corrected rather than left standing, because an argument from *"nothing is compared after this point"* is exactly the shape K-3 was. |
| `ledger_in`, `ledger_out` | Invariant I1 is a **ledger identity**, not a constant (§10). In cash mode a seat may buy in or cash out at a hand boundary, so `Σ stack + Σ committed_hand` is not constant and the two ledger totals are what it is measured against. They must be inside `STATE_HASH`, or two peers can disagree about the ledger and never detect it. `PROTOCOL.md` §6.1 carries them in `PublicTableState` for the same reason. |

### 2.9 `LocalView` — explicitly outside the canonical state

```rust
pub struct LocalView {
    pub me:               SeatIdx,
    pub my_hole:          Option<[Card; 2]>,     // SPEC_CS.md §9
    pub my_key_share:     SecretKeyHandle,       // zeroizing; CRYPTO_LIBS.md §5
    pub pending_tokens:   Vec<PendingToken>,
    pub armed_timer:      Option<TimerHandle>,   // the ONLY place a clock exists
    pub transport:        TransportView,         // direct/relayed, RTT — SPEC_CS.md §20, §22
    pub ui:               UiState,
}
```

Nothing in `LocalView` is serialised into an event, hashed into `STATE_HASH`, or read by
`step()`. `SPEC_CS.md` §9's requirement that a foreign hole card never exist in plaintext in a
client's memory is met structurally: there is no field anywhere that could hold one, and the
crypto layer can only produce a reveal token with the *local* secret share.

---

## 3. The determinism contract

### 3.1 The signature

```rust
pub fn step(state: &TableState, event: &Event) -> Result<Step, Rejection>;

pub struct Step {
    pub state:   TableState,   // the new state
    pub effects: Vec<Effect>,  // requests to the outside world; do not affect state
    pub derived: Vec<Event>,   // events the engine itself produced; see §3.4
}
```

`step` is a pure function. Given the same `(state, event)` it returns the same `Step` on every
machine, in every build, forever. Concretely, inside the `poker` crate:

* **no clock** — `std::time::{Instant, SystemTime}`, `chrono`, any OS time source: forbidden;
* **no randomness** — `getrandom`, `rand`, `HashMap`'s `RandomState`: forbidden;
* **no I/O** — no filesystem, no sockets, no logging that could affect control flow;
* **no floats** — `f32`/`f64` appear nowhere, in state or in computation;
* **no `HashMap`/`HashSet`** in anything reachable from `TableState` — `BTreeMap`/bitsets only;
* **no crypto types** — no group elements, no proofs, no keys; hashes are opaque `[u8; 32]`;
* **no address-dependent behaviour** — no pointer values, no `Vec` capacity, no allocator state;
* **no panics on network-derived input** — every fallible path returns `Rejection`;
  `#![forbid(unsafe_code)]`.

The "no clock" rule is now true **without exception** anywhere in the chain-building path. It had
one violation: `PROTOCOL.md` §4.8's `CERT_SETTLE_MS`, a wall-clock settle window every peer had to
wait out before chaining a timeout certificate. That constant is deleted and the certificate stage
is collective instead (C-10 of `PHASE0_FIXPLAN.md`, `PROTOCOL.md` §4.8 as amended); see §8.4.

`Rejection` is not an error to be logged and forgotten: it is the §11 requirement
*"the poker engine must not trust that the counterparty sends legal actions; re-validate every
incoming action locally."* A rejected event leaves the state **bit-identical** (invariant I21).

### 3.2 What is allowed to be non-deterministic, and where it lives

| Non-determinism | Where it lives | How it enters the engine |
|---|---|---|
| Wall-clock time | scheduler task, `LocalView::armed_timer` | never directly. **Since D-015 it enters as an ordinary chained event and not as a signed artefact of time at all**: a seat's own auto check/fold, which is an `Event::Action` by that seat (§8.5), or `Event::HandDeadlineAbort` (T57, T61). The row read *"only as a signed `TimeoutCertificate` event (D-006)"*; that event no longer exists (§4.1) |
| OS randomness | `mental_poker` (`getrandom::SysRng`, `CRYPTO_LIBS.md` §1) and the RNG beacon | as committed then revealed 32-byte values, already agreed |
| Network arrival order | `protocol` ordering buffer, keyed on the four envelope fields `PROTOCOL.md` §2.3 defines and §5.2 names for this purpose — **not reproduced here** (H8) | the engine sees one total order; out-of-order events are buffered or rejected before `step` (`PROTOCOL.md` §4.0 step 12) |
| Duplicate / replayed messages | `protocol` replay filter — the anti-replay slot key is **one literal tuple in `PROTOCOL.md` §5.2.1** and its stored form is §5.3's; this document reproduces no part of either (D-011 rule 2) | rejected before `step` (`PROTOCOL.md` §4.0 step 10a); if one slips through, I21 makes it a no-op |
| Signature validity | `protocol/signatures.rs`, `ed25519-dalek` `verify_strict` | an event reaches `step` only after its signature and canonicality gate pass |
| Cryptographic verification | `mental_poker` | as `ShuffleVerified` / `ShuffleRejected` / `CardsOpened` verdicts |
| My hole cards, my key share | `LocalView` | never |
| Transport (direct vs relayed, RTT, `PeerId`) | `net`, `LocalView::transport` | never (D-001: a relay is a byte pipe and must not be visible to the rules) |
| Hand-strength evaluator internals | `poker/evaluator.rs` façade over `rs_poker =5.0.0` | only the *derived* result — winner set per pot and chip deltas. `POKER_RULES.md` A′4 constraint 1: a third-party rank score must never enter the hashed state. |
| Frame timing, animations, `hand_delay_ms` | GUI | never |

**The ordering buffer's key and the anti-replay slot key are two different tuples, and confusing
them is the M2 error (H8).** The first decides *where in the total order* an event goes; the second
decides *whether a signer has already occupied a slot*, and is the predicate an `EquivocationProof`
is built on. Neither is this document's, and neither is reproduced above: the ordering key is
`PROTOCOL.md` §5.2's, over envelope fields §2.3 defines, and the slot key is §5.2.1's one literal
tuple, stored as §5.3 bounds it. The previous revision printed the ordering key's four fields in
the table cell above, unattributed, one column from the replay-filter row — which is exactly the
adjacency that invites a reader to take it for the slot key and re-derive D-009 rule 1 from the
wrong tuple. **Residual, recorded rather than resolved:** `PROTOCOL.md` §5.2 attributes the
ordering buffer to *"`STATE_MACHINE.md` §3.2"*, while this row now points at `PROTOCOL.md`. Under
D-011 rule 1 the wire's owner owns it — the buffer runs in `protocol`, over envelope fields, before
`step` is ever called, and the engine cannot see it — so the pointer's direction is the right one
and the sentence in §5.2 is the one to change. §13 carries it as an objection.

### 3.3 Canonical bytes and `STATE_HASH`

`SPEC_CS.md` §15 defines `state_hash = HASH(canonical_serialized_public_state)`. Per
`CRYPTO_LIBS.md`:

* encoding is `minicbor` 2.3.0 with `#[cbor(array)]` on every hashed struct — definite-length
  arrays, fixed field order, **never maps**, so determinism is structural and there is nothing
  to sort (`CRYPTO_LIBS.md` §4.6);
* every byte string field uses `with = "minicbor::bytes"`, or a `Vec<u8>` silently encodes as a
  CBOR array of integers;
* field indices `#[n(..)]` are append-only forever;
* the hash is BLAKE3 (`CRYPTO_LIBS.md` §3.2), domain-separated;
* every received encoding passes the decode–re-encode–compare canonicality gate
  (`CRYPTO_LIBS.md` §4.7) **before** any signature check, and signatures are verified over the
  exact received bytes, never over a re-encoding.

**What `STATE_HASH` covers is `PublicTableState`, not `TableState`, and `PROTOCOL.md` §6.1 is
canonical for its field list.** This document previously said the hash "covers `TableState` in
full, including `phase`, `deck` and `deadline`", which contradicts that list: `PublicTableState` is
an explicit, exhaustive `#[cbor(array)]` projection, and `phase`, `deadline`,
`history`, `faults`, `abort`, `settlement` and `finish_order` are **not** in it (`certified_subjects` stood
in this list and is deleted from `TableState` altogether by D-015, so it is not excluded from the
projection — it does not exist) — of `DeckState`
only `deck_commitment` is. Two documents giving different answers about the bytes that are hashed
and compared is the M1 defect class in the one object the whole divergence path is built on, so the
weaker and canonical statement is the one that stands:

> `state_hash` is taken over `PublicTableState` exactly as `PROTOCOL.md` §6.1 defines it, at the
> checkpoints §6.2 enumerates and at no other point. It covers nothing from `LocalView`. Every
> field of `PublicTableState` is a pure function of `TableState`, so two honest peers that have
> processed the same event prefix produce the same hash; a mismatch triggers the §15 dispute path
> and the game does not continue silently.

**The projection is transcribed from one list, and naming which one is `P7`.** `PublicTableState`
is `#[cbor(array)]`, so **field order is the encoding**: two transcriptions in different orders are
two different `state_hash` values for one state, and every checkpoint in the corpus then reports a
divergence between two peers that agree about the game. The corpus currently contains **three**
listings of that struct's contents and only one of them is a field order:

> **Normative for the engine: `PROTOCOL.md` §6.1's table, read top to bottom, is the field order the
> engine builds `PublicTableState` in.** No other listing anywhere is a field order and none may be
> transcribed as one — not `PROTOCOL.md` §2.9's D-012 sweep enumeration, which is a list of what was
> swept, and not this document's own §5.2 checkpoint-8 box, which names what the boundary checkpoint
> must be *about* and reproduces no part of the hash (D-011 rule 1). An implementer writing
> `src/protocol/messages.rs` reads §6.1's table and nothing else.

That is a rule about **which list the engine reads**, which is this document's to state, and it is
deliberately not a ruling on the disagreement itself: §6.1's table places the four `Vec<bool>` flag
vectors between `committed_this_hand` and `current_bet` and appends `signed_this_hand` last, while
§2.9's enumeration (l. 795) lists `signed_this_hand` before *"the `Vec<bool>` flag vectors"*. Both
are `PROTOCOL.md`'s, and under D-011 rule 1 that document says which of its own two lists survives
and **deletes the other** — §2.9's is a list of what was swept and is the one to go. **That filing
is now real, which it was not when this paragraph first claimed it**: the previous pass wrote *"the
defect is recorded on `DECISIONS.md`'s open list in this pass"* and no row was ever added. It is
`P7-w`, against `PROTOCOL.md` §2.9 and §6.1, and the correction is recorded here rather than made
silently because an unfiled cross-owner defect that a document *says* it filed is worse than one it
never mentioned: the next reader stops looking. What this document
guarantees is that it adds no fourth listing and that the engine's transcription has exactly one
source — and that guarantee holds whichever way §2.9 is resolved, because the engine does not read
§2.9.

The rest of `TableState` is still canonical — it is derived identically by every peer from the same
accepted events, and I22 asserts it — it is simply not *compared* at a checkpoint. The practical
consequence for an implementer is that a disagreement confined to an unhashed field is detected
through its effects (a collective stage a peer's copy fails to match, §3.4) rather than at the next
checkpoint. Whether any of those fields should be added to `PublicTableState` is `PROTOCOL.md`
§6.1's to decide, not this document's; §2.8 is the register of what is outside the projection and
why, and it carries the deletion record for the one field D-015 removed from `TableState`.

### 3.4 Derived events

Some transitions have no author. `HAND_COMPLETE`, `HAND_INIT` for the next hand, and the
settlement step are *computed*, not decided (`SPEC_CS.md` §4's "not driven by whoever clicks
first"). The engine emits them in `Step::derived`; the protocol layer wraps each one in that
peer's own signed envelope and emits it as a **collective** stage, so every peer signs a
byte-identical body and a peer that derives something different is rejected at the stage rather
than detected later at a checkpoint. There is no writer and no authority; there is also no
unsigned event. **Emitting a body and applying it are two different moments**, and for
`HAND_COMPLETE` they are two different transitions: the body is published on entry to `Settling`
and applied at T45 when the stage completes (§4.1). Only two of the four derived events have a
message at all — `Settle` and `NextHand`; `DealComplete` and `AbortSettle` are internal and carry
no `Effect::Publish` (§4.1).

This is the stage-kind principle of `PHASE0_FIXPLAN.md` §0.2 applied to the hand boundary: *a
stage whose body is a pure function of the state before it is collective; a stage whose body
carries a choice its emitter is entitled to make is single-writer.* `HAND_INIT`, `HAND_COMPLETE`
and `HAND_ABORT` carry no choice, so a single writer would hold a veto over the hand-to-hand
transition — an authority over exactly what `SPEC_CS.md` §4 protects. The cost is `n` copies of a
small message instead of one, and the hand-startup estimate of `CRYPTOGRAPHY.md` §6.5 absorbs one
extra collective round trip for it. `PROTOCOL.md` §3.2, §4.4 and §4.10 carry the same rule.

**One stage is neither shape, and it is the terminal `HAND_ABORT` — every one of them, whatever it
attributes (P3).** `PROTOCOL.md` §3.2 makes it a **witness-independent terminal stage**: no
required emitter set, closing on the first copy that verifies and passes §4.10's acceptance gate.
Every peer still derives it, signs it and emits it, so it has a collective stage's *emission*
pattern and none of a single writer's authority. It has to be that way because the stage exists
precisely when one seat has gone silent, so any set that names emitters either contains that seat —
and deadlocks — or is derived from what each peer happened to accept — and forks. The principle
above is not violated, because its reason is that a single writer's *silence* blocks an automatic
transition, and here no peer's silence blocks anything. The shape, the terminal value and the
acceptance gate are §3.1, §3.2 and §4.10's, and this document restates none of them (D-011
rule 1); §4.1 names the four facts the engine consumes.

**A checkpoint's own `STATE_HASH` body travels by the same route, and K-3 is what forces this to be
said.** `state_hash` is a pure function of accepted state (§3.3), so a checkpoint emission is
derived content in exactly this sense: the engine hands the body to the protocol layer through
`Effect::Publish`, which signs it under the local peer's application key and emits it as one copy
of the collective stage `PROTOCOL.md` §6.2 places. It is **not** one of §4.1's four derived
*events* — those are inputs the engine feeds itself, and a `STATE_HASH` comes back as
`Event::StateHash` from the wire like any other peer's. The document had no need to say this while
every checkpoint was emitted by the protocol layer at a point it could recognise on its own;
checkpoint 8 is emitted at a transition (T45, T46), so the emission is now an engine effect and
`DerivedEvent` carries the body.

The question this document previously left open here — what occupies `sender_public_key` in an
unsigned derived event, and whether peers counter-sign — is **closed**: `sender_public_key` is the
emitting peer's own application key, there is no unsigned envelope to design, and the
counter-signature *is* the mechanism, because every peer signs. See §11.

---

## 4. The event and effect alphabet

### 4.1 Events — the only inputs

```rust
pub enum Event {
    // ---- table lifecycle ----
    PlayerSeated   { seat: SeatIdx, player: PlayerId, buyin: Chips },
    PlayerLeft     { seat: SeatIdx },
    PlayerSitsOut  { seat: SeatIdx },
    PlayerSitsIn   { seat: SeatIdx },

    // ---- distributed randomness beacon (SPEC_CS.md §7, §16) ----
    RngCommit      { seat: SeatIdx, commitment: Hash },
    RngReveal      { seat: SeatIdx, value: [u8;32], salt: [u8;32] },

    // ---- verdicts from the cryptographic layer (opaque to poker rules) ----
    KeyPublished        { seat: SeatIdx, key_hash: Hash },
    KeyRejected         { seat: SeatIdx, reason: CryptoFault },
    ShuffleVerified     { seat: SeatIdx, deck_hash: Hash, proof_hash: Hash },
    ShuffleRejected     { seat: SeatIdx, reason: CryptoFault },
    RevealTokensPublished { seat: SeatIdx, indices: Vec<CardIndex> },
    CardsOpened         { indices: Vec<CardIndex>, cards: Vec<Card> },
    RevealRejected      { seat: SeatIdx, index: CardIndex, reason: CryptoFault },

    // ---- poker actions ----
    Action  { seat: SeatIdx, action: Action },     // Action::{Fold, Check, Call, Bet(to), Raise(to)}
    Show    { seat: SeatIdx },
    Muck    { seat: SeatIdx },

    // ---- time, as a signed artefact only (D-006, D-007) ----
    //   TimeoutCertificate { subject, kind, sequence, parent_hash, signers }
    //   DELETED by D-015 with the six rows that consumed it. Nothing on the wire
    //   produces a TIMEOUT_CERT, so nothing can ever construct this event. The
    //   struct is retained in 8.4 as the specification a later version restores.

    // ---- time, as a completed collective stage (D-008 point 2, D-009 rule 2) ----
    HandDeadlineAbort  { hand_id: u64, stalled_sequence: u64, parent_hash: Hash },
    FormationAbandoned { },

    // ---- the solitary-stage rule (PROTOCOL.md §3.2, §4.0 step 12a, §6.3 step 1) ----
    SolitaryDivergence { hand_id: u64, seat: SeatIdx, event_hash: Hash },

    // ---- deferred readmission (PROTOCOL.md §4.9's set A, §4.0 step 10b) ----
    Readmitted { seat: SeatIdx },

    // ---- proven cheating, removal for cause (D-014) ----
    CheatProven { subject: SeatIdx, tier: CheatTier, evidence_hash: Hash,
                  judged_at_checkpoint: Option<u16> },

    // ---- SPEC_CS.md §15 checkpoint and dispute path (PROTOCOL.md §6) ----
    StateHash  { seat: SeatIdx, hand_id: u64, checkpoint: u16, sequence: u64, state_hash: Hash },
    StateAck   { seat: SeatIdx, hand_id: u64, checkpoint: u16, agreed: Hash, checkpoint_hash: Hash },
    Dispute    { seat: SeatIdx, kind: DisputeKind, at_sequence: u64 },

    // ---- engine-derived (§3.4) ----
    DealComplete,        // internal only — never published; see below
    Settle,
    AbortSettle,
    NextHand,
}

pub enum DisputeKind { StateHashMismatch, MissingEvent, Equivocation }

pub enum CheatTier { SelfContained, StateDependent }   // D-014's tier 1 and tier 2
```

**`SolitaryDivergence` is the carrier K-9 records as missing, and it is the same kind of variant as
`HandDeadlineAbort` and `FormationAbandoned`: a situation the wire decides and the engine
consumes.** `PROTOCOL.md` §4.0 step 12a is where the situation is decided — a chained event of a
hand this peer dealt solitary, from a seat outside `P(k-1)`, other than `0x0804 PLAYER_SIT_IN` —
and §4.0 step 12a's disposition is that the event is *"not rejected, not applied, and not counted
into any `P`"*. The engine therefore never sees the offending event itself, which is the point: it
could be of any of thirty-nine types, and applying it is precisely what must not happen. What
reaches `step` is the fact, with the hand it names, the seat that signed it, and its `event_hash`
so the evidence is in the transcript record.

> **What this document needs from `PROTOCOL.md`, stated as a dependency and not as a request
> (K-9, and the reason this pass coordinates the two halves).** T62's guard is written in the
> **past tense** — *the hand the event names*, not the hand this peer is currently in — and it is
> useless unless §4.0 delivers the event that far. §4.0's pipeline drops a chained event naming a
> hand this receiver has finished, for the ordinary and previously harmless reason that it is late,
> and a solitary hand is `|dealt_in| == 1`: it self-completes every stage and passes through
> `Settling` and `HandComplete` at the speed of local computation, because `hand_delay_ms` is
> deliberately not an engine input (§8.3). **The contradicting event therefore arrives, in the
> normal case, after the hand it contradicts has closed.** `PROTOCOL.md` is adding the staleness
> step this pass — a chained event whose `hand_id` names a completed hand is evaluated rather than
> dropped, and §3.2's regime test stated in the past tense. **If that step lands anywhere after
> §4.0 step 12, or is scoped on the receiver's current `hand_id`, T62 is unreachable and K-9 is not
> discharged** — the rule would be correct and never fire, which is exactly the shape the Phase 3
> gate found. The engine side is written to the past-tense reading and asserts it in I33(a); an
> editor who changes the wire side to the present tense must delete T62 in the same edit and say
> what replaces it.

**`Readmitted` is the carrier for `PROTOCOL.md` §4.9's readmission set `A` (`N-5e`, as superseded by
`P2-e`).** It
is the same kind of variant as `SolitaryDivergence`, `HandDeadlineAbort` and `FormationAbandoned`:
a situation the wire decides and the engine consumes. §4.0 step 10b decides it — a stale chained
event of a finished hand that is either a `0x0804 PLAYER_SIT_IN` in that hand's boundary window or
a checkpoint-8 `STATE_HASH` whose value **equals** this receiver's retained
`checkpoint8_state_hash` — and its disposition is that the event is *"not applied, enters no
`stage_hash`, completes no stage"*. The engine therefore never sees the offending event either, for
the same reason as `SolitaryDivergence` and with the same benefit: it could be one of two types
belonging to a hand that is over, and applying it is precisely what must not happen. What reaches
`step` is the fact, and the fact is one seat index. §5.3 step 4 reads it and §5.3 step 8 clears it.

> **What this document needs from `PROTOCOL.md`, and since `P2` it is smaller than it was.**
> §4.9's set `A` and this field are **one set with two holders**: that document owns the membership
> rule and this one owns the filter and the hand-off, and neither restates the other (D-011
> rule 1). The dependency is that step 10b must **deliver** each admission as an event — one
> `Readmitted` per seat it adds — because a set the wire maintains privately and the engine cannot
> filter is how a `Removed` seat re-enters (I34(a), T67's guard, and the one thing this row adds to
> §4.9 rather than restating it).
>
> **The warning that stood here is discharged, and naming what discharged it is the point (`P2`).**
> It read that `A` is cleared at every hand init while §4.0 step 10a is **skipped** for stale-hand
> events, so a replayed — not forged, merely re-sent — agreeing checkpoint-8 `STATE_HASH` re-enters
> its sender into `A` once per hand, and that at a receiver which had narrowed that seat out the
> next stage 0 was then **required** of a seat that would not sign and stalled for
> `hand_deadline_ms`, once per hand, for the `MAX_RETAINED_HAND_RECORDS` hands §5.3 of that document retains a record
> for, with no key needed. `PROTOCOL.md` §4.9 has landed the fix and it is not duplicate
> suppression: **`A` no longer enlarges a required emitter set at all.** `R(HAND_INIT, m+1)` is
> `P(m)`; `A` widens that stage's **accepted** emitter set; and an extra accepted copy completes no
> stage and blocks none, so replaying the stale event now costs a map lookup. **A producer for
> `Readmitted` may therefore be wired**, which the previous pass forbade.
>
> **What this document had to change to take the same half, and it is the part a reader should
> check rather than assume.** The engine's copy of `A` was read into `dealt_in` **and** into the
> regime test, so the wire's fix on its own would have left the engine still enlarging a required
> set from a replayable input — the two halves of one peer disagreeing about who must speak before
> stage 0 completes, which is the disagreement that is supposed to be *between* peers. Both reads
> are deleted in this pass (§5.3 steps 4 and 8, `P2-e`). What is left is a hand-off: the set is
> handed to the protocol layer at hand init as the accepted-emitter widening for that one stage,
> and it appears in no guard here.

**`Event::StateHash` and `Event::StateAck` carry `hand_id`, and it is not a new wire field (`P1`).**
Both name the **chain** the checkpoint belongs to — hand `k` for checkpoint 8 of hand `k`,
`PROTOCOL.md` §4.9's *"Chain"* paragraph — and it is read off the envelope the protocol layer has
already validated, exactly as `sequence` is: `hand_id` is one of §2.3's envelope fields and one of
§2.4's signed bytes. Nothing is added to the wire and nothing is guessed. The reason the engine now
needs it is §2.6's store: with one checkpoint slot there was one answer to *which checkpoint is this
about*, and with three there is a selector, `(hand_id, number)`. **The clause this deletes is named
rather than left standing**: T50's cell used to argue that no past-tense test is *expressible* on
`Event::StateHash` because it carries no `hand_id`. Half of that is now false and the half that
mattered is not — T50 still reads the regime off the named checkpoint's own `required` set, and it
still does so because that set **is** the regime for the stage being compared, fixed when the
checkpoint opened and therefore not grown by the copy that contradicts it. `hand_id` selects the
checkpoint; it does not become a second regime memory, and a future edit that makes it one has
re-created the two-memories defect N4 removed.

**`CheatProven` is D-014's carrier, and it is an *accepted chained event*, not a local verdict.**
This is the whole of the difference between D-014 and D-010's forfeiture, and it is the rule the
removal newly makes load-bearing rather than a restatement of the decision. D-014's safety argument
is that *"every honest peer reaches the same verdict from data it already holds"* — true of the
**verdict** and not of the **holding**: whether the offending message reached this peer is a
per-receiver fact, and `status` is canonical state that enters `state_hash` (I30). A peer that
removes a seat on a message a second honest peer never received has forked the table, and it has
forked it in the one direction D-012 exists to forbid. So the engine takes the removal from a
chained event carrying the evidence, exactly as it takes an abort from a terminal `HAND_ABORT`, and
**`judged_at_checkpoint` is D-014's tier-2 precondition made checkable**: it is `Some(n)` for a
`StateDependent` tier and names the checkpoint whose agreed state the accused's message was judged
against, and `None` for `SelfContained`, which needs none. The wire half — which message type
carries it, which stage it occupies, its chain position, and how a peer that never received the
offending message obtains it — is `PROTOCOL.md`'s and is filed as **D-014-3**. Until it lands, this
document specifies the transitions and an implementation has no event to feed them, which is the
honest state to leave it in: the alternative is an engine that removes seats on a locally observed
fact, and that is the defect, not the feature.

`StateHash`, `StateAck` and `Dispute` are the engine's side of `PROTOCOL.md` §6.3's four-step
divergence procedure; without them the engine could not represent a terminal state the protocol
can reach, and the two would diverge by construction (C-6).

> **`Event::StateHash` is also the carrier for the stale boundary mismatch at a receiver that was
> *not* alone, and that carrier had no engine and no wire half before this pass (`P1`, third
> face).** `PROTOCOL.md` §4.9 and §4.0 step 10b say that a checkpoint-8 `STATE_HASH` of a finished
> hand `k` whose value **differs** from this receiver's retained `checkpoint8_state_hash(k)`
> *"enters §6.3 at step 1"*, and — where the record says the hand was solitary — routes to step 12a
> with it. The solitary route has a carrier, `SolitaryDivergence`, and the other route had none:
> `SolitaryDivergence` is guarded on the regime, and step 10b's own text says the event *"never
> reaches step 13"*, so at a peer with company the wire made a finding the engine could not be told
> about. **It is reachable on a healthy table**, because a peer's checkpoint-8 `STATE_ACK` and its
> `HAND_INIT(k+1)` copy are emitted on the same trigger and forwarding (`PROTOCOL.md` §1.5) can
> deliver another peer's `HAND_INIT(k+1)` first — which is the reorder §4.9's own `N6` box is built
> on.
>
> **The engine half is complete and it needed no new variant.** A stale checkpoint-8 `STATE_HASH`
> is a `state_hash` published for a checkpoint this peer still holds — §2.6's `boundary` slot — so
> the event is `Event::StateHash` with `hand_id = k` and `checkpoint = 8`, and the row that
> consumes it is **T50**, unchanged in every respect except that it selects the checkpoint by name
> instead of assuming there is only one. Nothing is applied by it: T50 freezes.
>
> **What is owed from the other side, stated as a dependency and not as a request (D-011 rule 1).**
> §4.0 step 10b must **deliver** such an event to the engine as `Event::StateHash`, rather than
> disposing of it inside the pipeline. That is compatible with every other thing step 10b says
> about it — it is still not *applied*, it still enters no `stage_hash`, and it still counts into
> no `P` of an initialised hand, because T50's entire side effect is a freeze and a `Fault` record
> — but the sentence *"never reaches step 13"* is written as an exhaustive prohibition and one
> clause of it has to move. **If it does not, this document has a row that no event can reach, on
> the path §4.9's `N6` fix newly made ordinary**, and that is the same shape as `L4`: a rule that is
> correct and never fires. It is recorded on `DECISIONS.md`'s open list in this pass.

**`Event::TimeoutCertificate` is deleted from the alphabet, and that is D-015's disposition here.**
`PROTOCOL.md`'s header box rules that `TIMEOUT_VOTE` and `TIMEOUT_CERT` are **defined but not
produced in version 1**: no conforming client emits either, and a receiver drops both at §4.0
step 6 before the body is decoded. So no `TIMEOUT_CERT` reaches this layer, `LocalView`'s
scheduler assembles none (§8.2), and the event can be constructed by nothing. **An engine
variant no wire rule produces is the same defect as an engine row no event can reach** — the
class this document has now closed six times (`RevealRejected`, `StateAck`,
`TimeoutCertificate{Join}`, the pre-hand rows T8 and T12, `Event::EquivocationProof`, and this)
— so the variant goes with the six rows that consumed it: **T16, T22, T27, T34, T41 and T44**
(§5.2). `DeadlineKind` survives, because `state.deadline` still carries it and §8.2 still arms
timers with it; what is gone is the signed artefact, never the description of a wait.

**Two things follow, and both are checkable.** **The engine still has no clock** — stronger than
before, because the one signed artefact of time it ever consumed is not built; time now reaches
it only as `Event::Action` (a seat's own auto check/fold, §8.5) and `Event::HandDeadlineAbort`
(T57, T61), both ordinary chained events. And **no phase lost an exit**: no certificate row was
ever an exit under total silence — a certificate needs `|V|` peers to speak — so §12.1.1's
column is unchanged, and §12.1.1 re-derives all twenty rows against that claim rather than
asserting it.

**`Event::EquivocationProof` is deleted from the alphabet, and that is G2's disposition.**
`PROTOCOL.md` §5.2.4 rules in a box that names this document: a verifying `EquivocationProof` is
retained as evidence in every phase, and *"**Nothing consumes it.** There is no transition, in any
document, that takes an `EquivocationProof` as its input event."* `PROTOCOL.md` owns the wire and
therefore owns what a message may cause; this document owns the transitions and follows the box
(D-011 rule 1). An engine variant no transition consumes is the C-6 defect in its own right — the
mirror of the `TimeoutCertificate{Join}` and `RevealRejected` cases this document has closed four
times — so the variant goes with the transitions that consumed it (T55, T56, both deleted; §5.2).

Two things follow and both are checkable rather than rhetorical. **The clause that stood first —
*the proof is still produced, verified, broadcast in a `DISPUTE` and retained forever* — is
withdrawn by D-015**: `PROTOCOL.md` §5.2.4 now rules that nothing produces one either, so what was
true at the protocol layer and irrelevant to the engine is now false at both. **Nothing the engine
does changes, and that is the checkable half**: it held no proof before and holds none now. And **an equivocation
still ends the hand, by the ordinary chained route**: the two conflicting copies are one stage cell,
the receivers that accepted different first copies now hold different state, the next checkpoint
shows two `state_hash` values, `Diverged` follows, and the hand ends at T54, T60 or T57 like any
other. Nothing was lost by deleting the shortcut except the shortcut, which is exactly what P2
said and what §5.2.4 now states as the corpus's answer.

**`round` is a derived quantity, not a wire field, and this is where the derivation is written
down (G5).** `PROTOCOL.md` §4.9 is the owner of the `STATE_HASH` payload and it declares exactly
three fields — `n(0) checkpoint`, `n(1) state_hash`, `n(2) transcript_head` — with **no `round`**;
rounds are separated by `sequence` and by nothing else. This document does not read a field that
does not exist and does not ask for one to be added, because `sequence` already carries the
information:

> **`round := sequence − s_ckpt`**, where `s_ckpt` is the stage index of the checkpoint's own
> `STATE_HASH` stage — `CheckpointState::sequence` of the record the event's `(hand_id, checkpoint)`
> names in §2.6's store, which every peer holds because it opened that checkpoint itself.
> `round == 0` is therefore the checkpoint emission of `PROTOCOL.md` §6.2;
> `round >= 1` is a reconciliation round of `PROTOCOL.md` §4.9's normative box, a new stage
> chained from the disputed checkpoint's `stage_hash`, carrying the value the peer derives *after*
> exchanging the events it was missing.

`Event::StateHash` therefore carries `sequence`, which the protocol layer reads off the envelope
it already validated, and the guards below compute `round` from it. §5.2's rows are scoped on that
derived value: T49, T50 and T51 read `round == 0`, T53 and T54 read `round >= 1`. An implementer
adds nothing to the wire and guesses nothing.

**Why a separate `sequence` is what makes reconciliation safe, and it is `PROTOCOL.md`'s rule, not
this document's.** §4.9 states it normatively and this document points at it rather than restating
it: a reconciliation `STATE_HASH` is a new collective stage at a new `sequence`, over the
checkpoint's emitter set, so a peer that reconciles writes a **different body into a different
slot** and no pair of its emissions satisfies §5.2's predicate. That is P1's closure and it lives
in §4.9. The shape this document must never reintroduce is the one the previous revision carried —
each peer *re-emitting* its `STATE_HASH` for the disputed checkpoint — which is a second, different
body in a capacity-one slot and manufactures a verifying proof against every honest peer that
reconciled.

**The objection this paragraph used to carry is closed.** It read that §6.3 step 3 ends *"Play
resumes from the checkpoint with no further action"* and that T53 and T54 were therefore triggered
by an artefact the wire does not define. `PROTOCOL.md` has since rewritten §6.3 step 3 and put the
reconciliation round in §4.9 as a normative box, so the trigger exists and §13 no longer carries
the request (G5(b)).

**Two of the four derived events are published and two are not, and the previous revision had the
split wrong.** `Settle` and `NextHand` correspond to wire stages — `HAND_COMPLETE` (`0x0801`) and
`HAND_INIT` (`0x0303`). `DealComplete` and **`AbortSettle`** correspond to none, and must not be
published:

* **`DealComplete`** was used by T26 without being declared. Its guard is that every participant
  has published every reveal token the deal owes, which *is* the completion of `PROTOCOL.md`'s
  `DEAL_PRIVATE` collective stage. That stage has already closed on the wire when the guard holds,
  so publishing anything here would be a second copy of a stage that is already complete.
* **`AbortSettle`** was listed as published, and there is **no message for it**: `PROTOCOL.md`
  §4.11 has 39 types and none is an abort-settlement. The wire event already happened — it is the
  terminal `HAND_ABORT` that took the engine into `HandAborted` — and T46 only *applies* it. A
  derived event claimed to be published with no message type to carry it is the C-6 defect from the
  other direction, so it is corrected here rather than left.

Both are pure internal steps — derived from public state, so every peer takes each at the same
point in the same order — and `Step::derived` carries them with no accompanying `Effect::Publish`.

**`Settle` is the one derived event whose transition waits for its own stage, and that is a change
in this revision.** T45 previously fired on the local derivation and awarded the pots immediately,
while `HAND_COMPLETE` is a **collective** stage that completes only when every seat of the
`HAND_INIT` set has been heard (`PROTOCOL.md` §3.2). A seat that went silent between the last
reveal and that stage therefore left the engine with the pots awarded and the chain with no
`TERMINAL(k)` — and `PROTOCOL.md` §4.10's precedence rule, which lets a peer that does *not* hold a
complete `HAND_COMPLETE` accept an abort for that hand, would then have restored stacks the engine
had already paid out. The split is now: the settlement is **computed** and this peer's own copy
**published** on entry to `Settling` (T30, T42), and it is **applied** at T45 when the stage
completes. `Settling` gains T57 as its exit for the case where it never does (§12.1 row 15).
Nothing about the arithmetic changes; what changes is that the award is a function of accepted
chain content rather than of one peer's derivation, which is §3.1's contract.

**`HandDeadlineAbort` is the certificate-free carrier D-008 point 2 needs, and it is still not a
clock read.** D-009 rule 2 makes a certificate below the `|V| >= 2` floor inert, so the only thing
that ends a hand nobody can finish is `hand_deadline_ms` — and §13 of the previous revision recorded
that no event existed to carry it. This is that event, and its rule is:

> The protocol layer raises `HandDeadlineAbort` **when, and only when, it has accepted — through
> `PROTOCOL.md` §4.10's acceptance gate — one verifying
> `HAND_ABORT{cause = 1, attributed = [], cert_hash = None}` event, from any peer, whose `hand_id`
> is the live hand.** The local `hand_deadline_ms` timer — which lives in `LocalView::armed_timer`
> like every other timer, runs from `TERMINAL(k−1)` (§8.2, R-1), and is the same relative duration
> on every peer (`PROTOCOL.md` §8.2) — decides only when *this* peer emits *its own copy*. It never
> changes state on its own.

**The stage this closes is `PROTOCOL.md` §3.2's *witness-independent terminal stage*, and this
document follows that section rather than restating it (D-011 rule 1).** The shape, its acceptance
rule, its `sequence`, the loose parent check and the terminal value are all `PROTOCOL.md`'s and are
defined in §3.1, §3.2 and §4.10. What the engine needs from them is four facts, referenced here and
specified there:

1. **It fires.** The stage has **no required emitter set** and closes at a receiver on the first
   copy that verifies and passes §4.10's gate (`PROTOCOL.md` §3.2), so no peer ever waits for the
   seat that went silent. This is P3's disposition and it is `PROTOCOL.md`'s ruling; the shape this
   document proposed was adopted, the `stage_hash` formula it proposed was not needed and was not
   adopted.
2. **The terminal value is agreed without agreeing about the hand.** `TERMINAL(k)` on the abort
   path is `ABORT_TERMINAL(k)`, a function of `GENESIS(k)` alone (`PROTOCOL.md` §3.1). Two peers
   that disagree about which stage stalled, about what arrived at it, and even about the abort's
   own `sequence` still compute the identical `TERMINAL(k)`, so `GENESIS(k+1)` exists on every
   abort path at every table size.
3. **The abort sits at the stalled stage's own `sequence` and no longer collides there (G1).** An
   emitter that already contributed at the stalled stage signs a second `event_class = 0` body at
   that `(sequence, sender)`; those are two slots and not one, because `event_type` is in
   `PROTOCOL.md` §5.2.1's slot key. That key is the corpus's one definition of the anti-replay slot
   and **this document reproduces no part of it** — not the tuple, not a subset, not a paraphrase
   (D-011 rule 2). Every emission this document specifies is checked against it; `PROTOCOL.md`
   §4.11's per-type table row 36 is where that check is recorded for this one.
4. **`stalled_sequence` and `parent_hash` are a record, not a guard.** `PROTOCOL.md` §4.10 makes
   the abort's chain position deliberately loose: a receiver whose own chain head differs still
   accepts it, because two peers can honestly disagree about which stage stalled and a strict check
   would deadlock. **The engine therefore does not gate on them** — T57's guard reads `hand_id` and
   that a hand is live, records `(stalled_sequence, parent_hash)` in the `AbortRecord`, and does
   nothing else with them. The previous revision's guard, *"is this peer's chain head"*, is deleted:
   it would have re-imposed at the engine exactly the strictness §4.10 removed at the receiver.

**The body's fields are `PROTOCOL.md` §4.10's**, and the two that matter to the engine are
`n(4) deltas`, all zeroes, and `n(5) final_stacks`, the start-of-hand stacks — which is D-010's
chip rule made checkable at the receiver rather than trusted at the emitter. `AbortRecord::owed` is
**empty on this path**, and that is deliberate: which seats the stalled stage was still owed an
artefact from is exactly the quantity peers disagree about, so it must not enter canonical state.
The evidence D-010 point 2 preserves is not lost — the absence *is* the evidence and it is readable
from the chain by anyone holding the transcript — it is simply per-observer, which is why it lives
in the transcript and not in `TableState`. `owed` was populated for the certificate-borne aborts T27, T41 and T44, where the certificate
named the subject and the requirement and the set was agreed. **Those three rows are deleted
(D-015), so `owed` is empty on every abort this version produces** — T57 and T61 set `owed: []` by
construction and always did. The field is retained in `AbortRecord` for the version that restores
the rows; **it must not be repopulated from a local view of who owed what**, because that is the
per-receiver quantity the paragraph above refuses.

Two further properties, carried over unchanged. **The engine still contains no clock** (§3.1,
§8.2): `step` sees an accepted stage — the comparison this sentence used to draw, *"exactly as it
sees a completed certificate stage"*, no longer has a second term (D-015), and the property is
stronger without it, since the certificate stage was the one accepted stage that asserted a time; the
buffering of a premature abort is `PROTOCOL.md` §4.10's, at the layer that holds the timer, and no
buffered event ever reaches `step`. And **a replay cannot move the abort**: `step` validates
`hand_id` and that a hand is live, and the first accepted abort ends the hand, so a second is a
`Rejection` that leaves the state bit-identical (I21).

**What is lost, stated rather than hidden.** An ordinary collective stage gave one honest peer a
veto over a *premature* abort: every receiver recomputes the body, so a peer that had accepted the
late action the would-be aborter did not see refused to complete the stage. A witness-independent
stage has no such veto. What replaces it is `PROTOCOL.md` §4.10's acceptance gate, which is weaker
and is enough: a receiver **buffers** an abort until its own `hand_deadline_ms` expires, so an early
abort ends the hand no earlier than the receiver would have ended it anyway, and a peer that emits
one gains exactly what going silent would have gained it. Under D-010 the outcome either way is a
neutral hand, which is why a weaker veto is affordable here and would not have been under
forfeiture.

**Every `HAND_ABORT` takes this shape, not only the unattributed ones.** The previous revision said
the terminal stage was witness-independent *whenever `attributed` is empty* and kept the ordinary
collective form otherwise. `PROTOCOL.md` §3.2 and §4.11 rule wider: `HAND_ABORT` is the one message
type of the witness-independent kind, for every `cause` and whatever it attributes. That is
simpler and this document follows it — so T15, T16, T21, T22, T27, T41, T44, T48, T54, T57 and T60
all end their hand through one stage shape, and none of them can be blocked by the silence of the
seat it names.

**`FormationAbandoned` is the lobby layer's counterpart for a table that never starts.** It is
raised when `PROTOCOL.md` §4.3's formation timer expires — `TABLE_READY` not complete within
`join_deadline_ms` of the first `JOIN_ACCEPT` — and it carries no fields because there is nothing
to attribute and nothing to check: it names no seat, produces no `Fault` and moves no chip. It is
the one event in the alphabet raised by a purely local timer with no signed artefact behind it, and
that is sound **only** because of where it is admissible (T4, phases 1–3 and `Diverged` before any
hand has started, P5): no `HAND_INIT` has
happened, so `ledger_in == ledger_out == 0` (I1, I28), no buy-in exists, no card exists, and the
only reachable next phase is the absorbing `TableClosed`. Two peers reaching it at different
wall-clock moments therefore disagree about nothing that is ever hashed or compared — the last
checkpoint was checkpoint 1 or none at all — and a peer that has not reached it simply never
receives a `TABLE_READY` and reaches it in turn. It is admissible nowhere else, and once a hand is
live it is a `Rejection`.

**`Show` has no accepting transition, and that is deliberate rather than an omission.** T36 rejects
it in a betting phase; in `AwaitingShowdownReveal` a seat that shows does so by publishing its own
reveal token, and the state change arrives as `CardsOpened` (T42) — a declaration that adds nothing
to the public record would be a second, unverifiable statement of the same fact. The variant stays
in the alphabet because it is `Muck`'s opposite in `PROTOCOL.md` §4.6's policy-gated showdown pair
(`SHOWDOWN_REVEAL` against `SHOWDOWN_MUCK`) and Q-01 has not been answered; under either policy the
engine's answer to a `Show` event is the same, a `Rejection` that leaves the state bit-identical
(I21). An implementer should not go looking for the row that consumes it.

**`PlayerLeft`, `PlayerSitsOut` and `PlayerSitsIn` are hand-boundary events only.**
`PROTOCOL.md` §4.10 makes `PLAYER_LEAVE`, `PLAYER_SIT_OUT` and `PLAYER_SIT_IN` single-writer stages
"at a hand boundary, and **only** there", and M4 deleted the courtesy clause that let a leave be
sent mid-hand. So the engine admits these three events in `Seating`, `HandComplete` and `Paused`
(T3, T58, T59) and nowhere else; in any live-hand phase they are a `Rejection` (T17, T35). A peer
that vanishes mid-hand announces nothing at all — D-005's "a client that vanishes must be handled
identically" is why the announcement can never be a precondition — and it is handled by the
deadline path of §8, never by a leave event.

`Action::Bet(to)` and `Action::Raise(to)` carry the player's **total commitment for the round**,
never an increment. `POKER_RULES.md` A3 cites TDA 43-B verbatim for this, and it removes an
entire ambiguity class from the wire format. There is deliberately **no `AllIn` variant**: an
all-in is a `Call`, a `Bet(max_to)` or a `Raise(max_to)` depending on context, and the legality
rules in §6 differ between those three cases (a player who may not reopen may not go all-in *as a
raise* at all).

### 4.2 Effects — the only outputs

```rust
pub enum Effect {
    RequestKeySetup { hand_id: u64, participants: SeatSet },
    RequestShuffle  { hand_id: u64, shuffler: SeatIdx, prev_deck: Option<Hash> },
    RequestOpen     { hand_id: u64, indices: Vec<CardIndex>, audience: Audience },
    ArmDeadline     { deadline: Deadline },
    DisarmDeadline,
    Publish         (DerivedEvent),
    Settled         (Settlement),      // for GUI and the transcript
    Fault           (FaultRecord),     // for the reputation counter and the GUI
}

pub enum Audience {
    /// tokens for a seat's own hole indices: everyone EXCEPT the owner publishes.
    /// Publicly broadcast and still safe — see §7.8.
    HoleOf(SeatIdx),
    /// board: everyone publishes.
    Public,
    /// showdown: the OWNER publishes its own token, completing the set.
    OwnerOf(SeatIdx),
}
```

`Effect::Publish(DerivedEvent)` hands the derived **body** to the protocol layer, which wraps it
in an envelope signed under the local peer's own application key and emits it as one copy of a
collective stage (§3.4). The engine never signs anything and never sees a key.

Effects never affect the state. Replaying a transcript with the effect handler disabled must
reproduce the identical final state — that is a property test (I22).

---

## 5. The phase state machine

### 5.1 The 20 phases

**The counts, stated once here and referenced elsewhere: 20 phases, 57 live transitions, 33
invariants (§10).** They were 63 and 34 before D-015, which deletes six rows and retires I29.

**The transition numbering runs T1–T67 and ten numbers are retired, so 67 minus 10 is 57 and the
gap is explained rather than an omission.** A number is retired when its row is deleted, and it is
never reused, because four documents cross-reference transition numbers by number and silently
renumbering them is worse than a gap. Anyone counting `| T` rows in §5.2 should find 57; anyone
looking for one of these ten should find this table and stop looking.

| Retired | Was | Deleted by | Why |
|---|---|---|---|
| **T8** | `AwaitingSeatRngCommit` × `TimeoutCertificate{Crypto}` → subject unseated | P4, third pass (D-010) | its trigger is a wire message that cannot exist in the setup chain (`PROTOCOL.md` §8.4); a stalled beacon now closes the table by T4 with nobody named |
| **T12** | `AwaitingSeatRngReveal` × `TimeoutCertificate{Crypto}` → subject unseated | P4, third pass (D-010) | same message, same disposition |
| **T55** | any phase except `TableClosed` × `EquivocationProof` → `HandAborted` | G2, fourth pass (D-011) | `PROTOCOL.md` §5.2.4: **nothing consumes an `EquivocationProof`**, in any document |
| **T56** | no hand live × `EquivocationProof` → record a `Fault` | G2, fourth pass (D-011) | same ruling |
| **T16** | `AwaitingKeySetup` × `TimeoutCertificate{Crypto}` → `HandAborted`, subject attributed | **D-015** | nothing produces a `TIMEOUT_CERT` (`PROTOCOL.md` §4.8, §8.3), so the trigger cannot be constructed. The stall ends at **T57** with nobody named (§12.1.1 row 4) |
| **T22** | `AwaitingShuffle` × `TimeoutCertificate{Crypto}` → `HandAborted` | **D-015** | same trigger, same disposition; **T57** (row 5) |
| **T27** | `AwaitingDeal` × `TimeoutCertificate{Crypto}` → `HandAborted` | **D-015** | same; **T57** (row 6) |
| **T34** | any betting phase × `TimeoutCertificate{Action}` → the auto check/fold | **D-015** | same trigger. **This one has a replacement rather than a fallback**: the seat's own client emits `ACTION_CHECK` / `ACTION_FOLD` when its own timer expires, and **T29–T33** consume it (§8.5). What is lost with the row is `was_auto` and `consecutive_auto_actions` |
| **T41** | `next_reveal(S)` × `TimeoutCertificate{Crypto}` → `HandAborted` | **D-015** | same; **T57** (rows 8, 10, 12) |
| **T44** | `AwaitingShowdownReveal` × `TimeoutCertificate{Crypto}` → `HandAborted` | **D-015** | same; **T57** (row 14) |

**Six of the ten retirements are D-015's and they are all one deletion, not six**: the six rows
were the whole of this document's consumption of the timeout machinery, and the four that
preceded them — T8, T12, T55, T56 — were the same class of defect caught one instance at a time.
What D-015 does that those four did not is remove the **producer**, so no seventh instance can
appear: there is no longer a wire message for a future row to be written against.

The counts changed from the pre-fix-plan figures (19 / 47 / 26) by the additions of
C-6 (the `Diverged` phase and the divergence transitions), C-6's `RevealRejected` transition, A-7
(I27) and C-9 (I28). The transition count went from 55 to 56 in the first Phase 1 verification
pass, which added **T56** so that an `EquivocationProof` arriving when no hand is live was consumed
rather than rejected (C-6 PARTIAL, verification finding N5); the same pass widened T55 from
`Diverged` to every phase except `TableClosed` without changing the count, and took the invariants
from 28 to 29 with **I29**, the D-008 voter-set floor. Both of those rows are gone again — see the
fourth-pass bullet below.

The second pass took it from 56 to 59, all three additions forced by rulings made elsewhere and
none of them a new design:

* **T57**, the hand-deadline abort. D-009 rule 2 makes a below-floor certificate inert, so the
  `|V| < 2` branches of T16, T22, T27, T41 and T44 are deleted and the hand ends instead on
  `PROTOCOL.md` §8.4's `hand_deadline_ms` path. T57 is that path's transition and
  `Event::HandDeadlineAbort` (§4.1) is the carrier §13 recorded as missing.
* **T58** and **T59**, the hand-boundary seat events. `PROTOCOL.md` §4.10 makes `PLAYER_LEAVE`,
  `PLAYER_SIT_OUT` and `PLAYER_SIT_IN` legal at a hand boundary and only there (M4), and this
  document had no row for them in the one phase where they *are* legal, `HandComplete`, while
  carrying two rows for them mid-hand where they are not.

T4's trigger changed in the same pass — from a `TimeoutCertificate{Join}` that no wire message can
produce to `Event::FormationAbandoned` — without changing the count.

The third pass, under **D-010**, took it from 59 to **57** by deleting **T8** and **T12** (P4):
`PROTOCOL.md` §8.4 rules in terms that `TIMEOUT_VOTE` and `TIMEOUT_CERT` are *"never emitted in the
setup chain (`hand_id = 0`)"*, and the seating beacon is in that chain (`PROTOCOL.md` §2, §3.1), so
those two rows were triggered by a wire message that cannot exist where they live. **The numbers 8
and 12 are retired and are not reused**, because four documents cross-reference transition numbers
and silently renumbering them is worse than a gap. Two transitions changed scope in the same pass
without changing the count: **T57** gains `Diverged` (P5), and **T4** gains `Diverged` when no hand
has started. No transition was added.

The fourth pass, under **D-011**, took it from 57 to **56**: two rows deleted, one added.

* **T55 and T56 are deleted (G2).** `PROTOCOL.md` §5.2.4 rules that **nothing consumes an
  `EquivocationProof`** — no transition in any document takes one as its input event — so the two
  rows that did are gone, `Event::EquivocationProof` is gone from §4.1, and
  `AbortKind::Equivocation` is gone from §8.6. The numbers **55 and 56 are retired and are not
  reused**, on the same rule that retired 8 and 12.
* **T60 is added (`PROTOCOL.md` §6.3 case (b)).** §6.3 now gives its two unresolved terminuses
  different bodies and different consequences — case (b) aborts with `cause = 1` and leaves the
  table playable, case (c) aborts with `cause = 4` and faults it — and T54 produced case (c)'s
  outcome for both. Two outcomes need two rows.

Two rows changed shape in the same pass without changing the count. **T11** no longer unseats
(G4): a mismatched `RngReveal` is a `Rejection` and the beacon stalls to T4, which is the
disposition P4 already chose for the two rows deleted beside it. **T57**'s guard drops the
chain-head check (G1): `PROTOCOL.md` §4.10 checks the terminal abort's position loosely, on
purpose, and an engine guard that re-imposed strictness would restore the deadlock that check was
loosened to remove.

The fifth pass, under **D-012**, adds and deletes **no transition** — the count stays at **56** —
and takes the invariants from 29 to **30**:

* **T46 loses a side effect (H1).** It no longer sets `status := Absent` from the accepted
  `HAND_ABORT` copy's `attributed`. The row survives, its restoration rule is untouched, and no
  number is retired: what changed is one clause of one cell. See §5.2's note under T58/T59.
* **I30 is added**, the invariant whose absence *was* H1: seat status moves only through a chained
  event, is derived from no per-receiver quantity, and agrees across peers at a hand boundary.

Both are one ruling, D-012's: *no canonical state may be derived from a quantity that can differ
between honest receivers.* The pass also replaced two restatements with pointers (H8) and corrected
the transition count in §9.5, neither of which changes anything the engine does.

The sixth pass, under **D-013**, again adds and deletes **no transition** — the count stays at
**56** — and takes the invariants from 30 to **31**:

* **§5.3 step 4 gains a conjunct (J2).** `dealt_in[s]` now requires `s ∈ signed_this_hand` as of
  the previous hand, which is D-013's rule applied to the one quantity this document owns. No row
  of §5.2 changes: hand init is a procedure T10 and T47 run, not a transition.
* **I31 is added**, and it is the counterpart of I30 rather than a second copy of it: I30 says a
  *status* is chain-derived, I31 says the *participation set* is, and that the two are no longer
  the same question.
* **I30(c) loses its last clause (J3)**, which asserted that agreement on the status vector gives
  "a `HAND_INIT` collective stage that completes". It does not: agreement makes the stage
  *completable*. That clause is also what hid J2 for two passes, and it carried a test instruction,
  so a harness built on it was unfalsifiable exactly where the defect lived.

The seventh pass, against `DECISIONS.md`'s open list, takes the transitions from 56 to **57** and
the invariants from 31 to **32**:

* **T61 is added (K-3).** It is the hand-boundary counterpart of T57: the `hand_deadline_ms` abort
  for the hand that has **not started yet**, covering the interval from `TERMINAL(k)` to
  `GENESIS(k+1)`'s first stage. It exists because checkpoint 8 gives `HandComplete` width on the
  settled path, and a phase with width owes a termination argument whose *Fires after* column names
  a timer and never a peer (§12.1 note 3). It needs no new timer and no new event: `PROTOCOL.md`
  §8.2 already starts hand `k+1`'s deadline at `TERMINAL(k)`, and `Event::HandDeadlineAbort` (§4.1)
  already carries it.
* **T45, T46, T47 and T59 change without changing the count.** T45 and T46 gain the checkpoint-8
  emission; T47 gains the gate; T59's `Paused` exit stops reading a status word of its own and
  reads §5.3 step 4's predicate instead (**K-7**).
* **I32 is added** — every hand places a checkpoint — which is the assertion whose absence *was*
  K-3: §12.1's walk asks whether each hand ends, and a hand that ends with nothing comparable
  emitted passes it.

The eighth pass, against `DECISIONS.md` **K-9**, **L7** and **D-014**, takes the transitions from
57 to **62** and the invariants from 32 to **34**:

* **T62 and T63 are added (K-9).** They are the engine half of `PROTOCOL.md` §3.2's solitary-stage
  rule, which §5.3 step 9 previously had no phase for and no transition that consumed. **No new
  phase**: the target is `Diverged`, which is what §6.3 step 1's freeze already is, and §6.3's
  second entry condition says so in terms. **Two rows because there are two outcomes** — the rule
  established when T60 was added — and the second outcome is the one the count would otherwise
  hide: **`TableClosed` stops being absorbing for exactly this one event class** (T63). It has to,
  or the rule loses a race it cannot afford: the contradicting event arrives after a solitary hand
  has closed, forty of those hands close in about five minutes (§12.1.2), and a peer that reached
  `TableClosed` first would reject the only evidence that its "tournament won" was one half of a
  fork.
* **T64, T65 and T66 are added (D-014).** A removal for cause voids the hand and takes the offender
  out of every set at once. Three rows, because the disposition differs by what is live: a hand
  (T64, which voids it), a boundary (T65, which does not), and **the setup chain (T66, which
  removes nobody)** — the roster's seat vector is frozen at `TABLE_READY` (`PROTOCOL.md` §3.1), so
  a removal there would fork the genesis, and the beacon still stalls to T4 exactly as G4 and P4
  left it. That is D-014 checked against the two rulings it looks like it reopens, and it does not
  reopen them.
* **§9.3 condition 2 is restated on one predicate (L7)** and **condition 0.6 is added**; no
  transition changes. **T59's `Paused` exit was K-7's mirror of the same defect and was fixed a
  pass earlier**, which is why L7 was found: two predicates for one question survive in pairs.
* **I33 and I34 are added** — the solitary freeze's own assertion, and the one-way exit D-014
  point 4 requires.

**The ninth pass, against `N1` and `N4`, adds no transition and no invariant and changes three
guards, and every one of the three is on the path the eighth pass newly made load-bearing.** The
count stays at **62** transitions and **34** invariants.

* **T53 gains a conjunct (N1).** The freeze's own repair path was satisfiable by the frozen peer
  alone: a reconciliation round's required set at a solitary peer is `{self}`, so the stage
  completed, agreed with itself, released the freeze and cleared the latch — for ever. T53 now
  requires the completed stage to carry **two distinct signers**. T54 and T60 need nothing: both
  require two distinct values to remain, which one signer cannot produce.
* **T50 gains a side effect (N1).** A solitary peer contradicted by a `state_hash` **mismatch**
  rather than by an out-of-set event set no latch at all, so it thawed on the timer and re-froze at
  the next checkpoint 8, every `hand_deadline_ms`. T50 now sets `solitary_contradicted` when
  `|checkpoint.required| == 1`, read off the checkpoint being compared rather than off a memory, because
  `Event::StateHash` carries no `hand_id` and the checkpoint's own required set **is** the regime
  for the stage being compared.
* **`solitary_since` becomes monotone and `I33(a)` is rewritten (N4).** It was cleared on regime
  exit while `PROTOCOL.md` §4.0 step 10b's retained record was not, so after a re-entry the wire
  froze on a hand the engine rejected. The contiguity argument that justified one `u64` is deleted
  as false; the record is the sole authority, this field is a monotone **floor** under it, and
  §2.6's lemma — asserted as I33(a) — is that the floor never rejects a hand the record admits.
* **`I33(b)` gains an exemption and `I33(c)` is re-scoped (N1).** (b) forbade the frozen peer every
  publication, including the reconciliation traffic its only repair consumes; the §6.3 step 2–3
  messages are now exempt by name. (c) was scoped on the latch, which both loops leave untouched or
  clear; it is now scoped on the **freeze**.

**And it records the one thing D-014 changes that no transition shows.** D-010 point 3 forbade
automated peer removal after four passes in which every severe defect ended with an honest peer's
chips forfeited; D-014 narrows that ban to its sound core and hands this document a status that is
**absorbing**. Every other status in §2.4 is recoverable, and every previous defect in this
document's removal machinery — T8, T11, T12, T55 — was a removal that could not be undone by the
peer it was wrong about. `Removed` is that same object, and what makes it admissible this time is
not that the engine is more careful but that the evidence is a message the accused **signed**. If
the removal is ever driven by anything else — a timer, a vote, a certificate, an `attributed`
field, a locally observed fact — it is the D-010 defect again and the variant must go with it.

**And it corrects the record, which matters more than the edit.** D-012 recorded the cost of
deleting T46's marking as *"a stall that repeats every hand"*. That is not what the machine did.
Marking a seat `Absent` never removed it from `HAND_INIT`'s required emitter set — the set named
absent and sitting-out seats in its *inclusion* clause — so the stall was never bounded by anything:
T46 restores every stack, so no seat busts, so no §9.3 condition can fire, so the table repeated an
identical hand of one `hand_deadline_ms` forever. **The table made no progress at all, and no participant could
end it.** Every statement of that cost in this document is rewritten rather than annotated.

**The tenth pass, against `P1`, `N-1e`, `N-5e` and `P7`, added one transition and no invariant, and
changed one field, one predicate and four guards.** The count went from 62 to **63** transitions
and stayed at **34** invariants.

**The eleventh pass, against `Q1-e`, `Q4-e`, `P7` and `N1-a`, adds no transition, no invariant and no
phase.** It **deletes** one derivation (`admitted`), **replaces** one field of `CheckpointState`
with two, and **re-derives** three guards. The counts are unchanged at **63** transitions and
**34** invariants, and no row of §12.1's twenty is displaced.

* **The checkpoint field becomes a three-slot store (`P1`, blocking).** `Option<CheckpointState>`,
  *"at most one is open at a time"*, could not hold what `PROTOCOL.md` §4.9 now requires — hand
  `k`'s boundary checkpoint outliving hand `k` — so **three** rules failed at once: T51 could not
  set `agreed` for a boundary checkpoint and D-014's tier-2 precondition was unsatisfiable there;
  T50's `N1` conjunct was unreadable on the stale route; and a non-solitary receiver's stale
  boundary mismatch had **no carrier at all**, on a path a healthy table reaches through
  forwarding. §2.6 carries the store, its bound and the lifetime of each slot; T45, T46, T47, T49,
  T50, T51, T61, T64 and T65 read or write it by name.
* **`Event::StateHash` and `Event::StateAck` carry `hand_id` (`P1`).** No wire field is added — it
  is an envelope field the protocol layer has already validated (§4.1) — and it is a **selector**,
  never a second regime memory. T50's clause that no past-tense test was expressible on this event
  is withdrawn by name rather than left standing.
* **T51 waits for the `STATE_ACK` *stage* (`P1`).** `CheckpointState` gains `acked`, and `agreed`
  is set when `acked ⊇ required`. The guard read the `STATE_HASH` stage while §2.6's own field
  comment and D-014's tier-2 precondition both named the ACK stage, and there was no field in
  which a completed ACK stage was representable.
* **The solitary floor gains one hand of slack (`N-1e`, blocking).** `solitary_at(k)` becomes
  `solitary_since == Some(j) ∧ j <= k + 1 ∧ k <= hand_id`, and §2.6's lemma is restated over
  `PROTOCOL.md` §3.2's **disjunction** rather than its first disjunct. The old form rejected the
  exact event §4.0 step 10b freezes on for K-1's own fork hand.
* **T67 and `readmit` are kept and their union is deleted (`P2-e`, superseding `N-5e`).** §4.9's
  `A` no longer enlarges a required emitter set, so there are no longer two `R(HAND_INIT(m+1))` to
  reconcile — there is one, `P(m)`, and `signed_this_hand` is this document's holder of it. §5.3
  step 4 derives `dealt_in` from `signed_this_hand` alone and step 8 reads the same set for the
  solitary regime; `readmit` is handed to the protocol layer at step 4 and appears in no guard.
  T67 survives for its **filter** and for nothing else: the wire has no `status` with which to keep
  a `Removed` seat out of `A` (I34(a)).
* **§3.3 names the one list the engine transcribes `PublicTableState` from (`P7`).**
  `PROTOCOL.md` §6.1's table, and no other listing anywhere, including this document's own §5.2
  box. No transition changes; the encoding does.

```rust
pub enum Phase {
     1 Seating,                //  waiting for players
     2 AwaitingSeatRngCommit,  //  crypto wait: §7 commit
     3 AwaitingSeatRngReveal,  //  crypto wait: §7 reveal
     4 AwaitingKeySetup,       //  crypto wait: n-of-n key for this hand
     5 AwaitingShuffle,        //  crypto wait: the shuffle chain
     6 AwaitingDeal,           //  crypto wait: hole-card reveal tokens
     7 BettingPreFlop,
     8 AwaitingFlopReveal,     //  crypto wait: board
     9 BettingFlop,
    10 AwaitingTurnReveal,     //  crypto wait: board
    11 BettingTurn,
    12 AwaitingRiverReveal,    //  crypto wait: board
    13 BettingRiver,
    14 AwaitingShowdownReveal, //  crypto wait: showdown hands
    15 Settling,               //  pure: pots, evaluation, award
    16 HandComplete,           //  the hand boundary: checkpoint 8, and the only window in
                               //  which a seat-state event is legal (§5.2, T58/T59, T61)
    17 HandAborted,            //  SPEC_CS.md §19 / D-005; settles neutrally (D-010, §8.6)
    18 Paused,                 //  §5.3 step 4 would deal fewer than two seats in
    19 TableClosed,            //  terminal, and absorbing for every event except one (T63)
    20 Diverged,               //  SPEC_CS.md §15 / PROTOCOL.md §6.3 — a freeze, not a terminal
}
```

`Diverged` is appended rather than inserted so no existing discriminant moves. It is entered from
**any** phase the moment two distinct `state_hash` values exist for one checkpoint, and it is a
**freeze**: no card opens, no action applies, no chips move (`PROTOCOL.md` §6.3 step 1). It is not
terminal — transcript reconciliation can return the table to the phase it held at the checkpoint.

**Since K-9 it has a second entry condition and it is not a checkpoint comparison** (`PROTOCOL.md`
§6.3 step 1's second paragraph, §3.2's solitary-stage rule): a peer that dealt a hand with
`|P(k-1)| == 1` and then receives a chained event of that hand from a seat outside `P(k-1)` freezes
here, on **T62**, with no second `state_hash` involved. That is one phase serving two triggers, and
it is one phase on purpose: what §6.3 step 1 specifies is the *freeze* — no card, no action, no
chip, no stage completed — and both triggers want exactly it. The disposition afterwards is also
§6.3's, steps 2 to 4, which are already T52, T53, T54 and T60. What the second trigger adds is a
latch, `solitary_contradicted` (§2.6), because the two triggers differ in one respect that matters:
a checkpoint divergence that times out costs the table a hand and the table plays on, whereas a
**solitary** divergence that times out would deal another solitary hand, freeze again on the next
contradicting event, and repeat every `hand_deadline_ms` forever — J2's fixed point, one link
further out. §9.3 condition 0.6 is where the latch is read and where that loop is cut.

**Nine** of the twenty — 2, 3, 4, 5, 6, 8, 10, 12 and 14 — are cryptographic waits, and one
(15) computes the result and then waits for the terminal stage that records it. **Since K-3 one
more waits, and it is the phase that previously had no width at all:** 16 waits, on the settled
path only, for the hand-boundary checkpoint (§5.2's checkpoint-8 box). They are
first-class states because the engine has to be able to make no
progress at all, indefinitely, while remaining a valid hashable state that every peer agrees on,
and because the *only* legal exits from them are a crypto verdict, the hand
deadline (T57), or a fold-out that makes the pending cryptography unnecessary. **The list used to
carry a fourth entry, *a timeout certificate*, and D-015 removes it**: no certificate is produced,
so a cryptographic wait now ends in a verdict, a timer or a fold and in nothing else — which is
what §12.1.1's column already said and what the six deleted rows never contributed to. A boolean "waiting" flag could not
express which artefact is owed by whom, which is exactly what §19 requires as evidence.

### 5.2 The transition table

Notation. `m` = number of dealt-in seats (one symbol for this quantity across the whole corpus;
`PROTOCOL.md` §4.5 and `CRYPTOGRAPHY.md` §2.4 use the same letter — C-3). `live(s)` = dealt-in ∧
¬folded. `contenders(s)` =
live ∧ ¬all_in. `round_closed(s)` is the `POKER_RULES.md` A2 predicate: every contender has
`acted_this_round` **and** `committed_round == current_bet` (or is all-in for less). Effects are
abbreviated. A transition not listed does not exist; any event arriving in a state with no
matching row is a `Rejection` and leaves the state bit-identical (I21, I13).

**`V(subject)` appears in no guard in this table, and that is D-015.** The required voter set of
§8.4 was read by six rows and by nothing else; with those rows deleted there is no guard left
that needs it, no guard that reads `certified_subjects`, and no guard that reads a certificate
of any kind. **D-008's scoping rule survives the deletion and is restated as a bar rather than
as an instruction**, because it is what a later version must not get wrong a second time: a
guard in this document that reads `m == 2`, `|deck.participants| == 2`, or any seat count in
order to decide whether a deadline has force is a defect — the quantity is the size of the
voter set, never the size of the table, and the version scoped on the seat count let one
modified client manufacture a one-signer certificate at six seats (N3, `THREAT_MODEL.md` X30).
§8.4 keeps the definition `V(subject) = deck.participants \ ({subject} ∪ certified_subjects)`
as the specification a later version restores, together with rules 1–7 and the floor.

**The six `TimeoutCertificate` rows are deleted — T16, T22, T27, T34, T41 and T44 (D-015).**
Nothing on the wire produces a `TIMEOUT_CERT` (`PROTOCOL.md` §4.8, §8.3), so the event of §4.1
cannot be constructed and six rows could never fire. They are removed rather than left as dead
rows, which is this document's standing disposition for a row no event can reach.

**What replaces each, stated per row so that no wait is left without an answer.** The five
`kind = Crypto` rows and the one `kind = Action` row are replaced by two different things, and
the difference is §8.1's, not a detail of this table:

| Deleted row | Phase it fired in | What the row did | What replaces it |
|---|---|---|---|
| **T16** | `AwaitingKeySetup` (4) | abort on a subject that owed a key | **T57**, the hand deadline — the stage stalls, `hand_deadline_ms` expires, `HandDeadlineAbort` ends the hand with `attributed = []` and stacks restored. The silent seat is then outside `signed_this_hand`, so §5.3 step 4 does not deal it in next hand (D-013) |
| **T22** | `AwaitingShuffle` (5) | abort on a shuffler that did not shuffle | **T57**, same route. A seat skipped by step 4 is not in `deck.participants`, so it is not in `shuffle_order` and cannot stall the chain a second time (I20) |
| **T27** | `AwaitingDeal` (6) | abort on a subject owing reveal tokens | **T57**, same route |
| **T41** | `next_reveal(S)` (8, 10, 12) | abort on a subject owing this street's tokens | **T57**, same route |
| **T44** | `AwaitingShowdownReveal` (14) | abort on a subject owing its own token | **T57**, same route |
| **T34** | any betting phase (7, 9, 11, 13) | apply the subject's auto check/fold, derived from a certificate | **T29–T33, unchanged** — the seat emits its **own** `Action`. This is the substantive replacement and the only one that is not a timer |

**T34's replacement is the part worth reading twice, because it looks like a liveness loss and
is not.** The row existed so that a table could keep playing when a human stopped answering.
Under D-015 the *client* answers instead: publishing a decryption share was always an automatic
client step that never waited for the human (§8.1), and so is emitting an action — when the
seat's own `action_timeout_ms` expires its client signs `Check` if `to_call == 0` and `Fold`
otherwise, exactly the decision T34 derived, and gossips it as an ordinary single-writer event
at that seat's own stage. The engine consumes it through **T29–T33** like any other action, with
**no new row, no new event and no new guard**. What is gone with T34 is not the auto-action but
the *derivation* of it by other peers: the seat signs its own, so there is no vote, no voter
set, no unanimity, no shared clock, and — the point D-006 to D-008 never reached — no way for
anyone else to manufacture one. `was_auto` and `consecutive_auto_actions` are discussed in
§8.5, which is where the one behavioural loss is recorded.

**The case T34 covered and the seat's own auto-action does not** is the seat whose client is
gone or is deliberately silent. That case never had an action to apply: it is the right-hand
column of §8.1, it reaches **T57** like every other silence, and D-013 removes the seat from the
next hand's required set. So the two columns of §8.1 are covered by two mechanisms that already
existed, and the certificate sat between them covering neither.

**The two pre-hand rows were already gone and stay gone (P4).** T8 and T12 carried a
`TimeoutCertificate{Crypto}` in the seat-order beacon, which is in the setup chain
(`hand_id = 0`), and `PROTOCOL.md` §8.4 rules that a vote or a certificate is *never emitted*
there; the beacon table below carries what a stalled beacon does instead (T4). D-015 makes that
deletion general rather than special: **no phase of this document, in the setup chain or in a
hand, has a certificate row any more**, so the P4 argument no longer has to be made one phase
at a time.

**One guard is *not* removed and must not be**: T57's. It never read a certificate, a voter set
or `|V|`, and it is now the sole terminus for every cryptographic stall at every table size —
which is the state §8.4's below-the-floor rule already put a `|V| < 2` table in, generalised to
all of them.

#### Seating and start-of-table

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T1 | `Seating` | `PlayerSeated` | seat empty ∧ occupied+1 < `min_players_to_start` ∧ config valid | `Seating` | — |
| T2 | `Seating` | `PlayerSeated` | occupied+1 == `min_players_to_start` | `AwaitingSeatRngCommit` | `ArmDeadline{Crypto}`; request commits from all seated |
| T3 | `Seating` | `PlayerLeft` | seat occupied | `Seating` | seat → `Empty`, stack returned to nothing (no chips exist yet) |
| T4 | `Seating` \| `AwaitingSeatRngCommit` \| `AwaitingSeatRngReveal` \| `Diverged` | `FormationAbandoned` | no `HAND_INIT` has happened — equivalently `hand_id == 0` ∧ `ledger_in == 0` | `TableClosed` | no `Fault`, no attribution, no chips: the table simply never started |
| T5 | `Seating` | any other | — | `Seating` | `Rejection` |

**T4 is a lobby-layer timer, not a certificate, and that is now settled (N9(b)).** The previous
revision triggered T4 on a `TimeoutCertificate{Join}` and recorded an objection saying the trigger
*should* be a local timer, deferring the change to `PROTOCOL.md`. `PROTOCOL.md` has since ruled, in
§8.4 and in terms: *"A certificate kind for a join deadline does not exist in this document, and
none should be invented to fill the gap — there is no gap."* Keeping the old trigger would leave
the engine's alphabet containing a variant no wire message can produce, which is a contradiction
rather than an open question, so the trigger is now `Event::FormationAbandoned` (§4.1),
`DeadlineKind::Join` is deleted (§2.6), and the objection in §13 is closed rather than carried.

T4 is the engine side of the case `PROTOCOL.md` §4.3 covers: the founder vanishes after issuing
`JOIN_ACCEPT`s and before `TABLE_READY` completes. Per §4.3, formation is abandoned when
`TABLE_READY` has not completed within `join_deadline_ms` of the first `JOIN_ACCEPT`; every holder
of a `JOIN_ACCEPT` discards it and its seat reservation, and **no chips move, because a buy-in
enters the ledger only at the first `HAND_INIT`** (§9.4, invariant I1). The advertisement is not
revoked by anyone — `LOBBY_TABLE_REMOVE` requires the table key, which is exactly what is missing —
and it disappears from every lobby by `AD_TTL_MS` (`NETWORK_STACK.md` §10.3). The engine's
`TableClosed` and the lobby's TTL expiry are the same event seen from two layers. No certificate,
no voter set and no signature from anybody is involved, so D-008's floor has nothing to gate here
and the row carries none.

**Why T4's scope is four phases and not one, and the one thing it still needs from `PROTOCOL.md`.**
`PROTOCOL.md` §8.4 is normative that the hand deadline "covers hands only" and says nothing about a
stall in the setup chain, and §8.4's box is equally normative that no join-deadline certificate
exists and none is to be invented — *"What transition replaces it, out of which phase, and with what
guard is `STATE_MACHINE.md`'s"*. This is that transition. Since **P4 deleted T8 and T12** the
seat-order beacon has no other terminus at all, and `PROTOCOL.md` §3.1 puts the beacon in the setup
chain, `hand_id = 0`, **after** `TABLE_READY`.

**Named default, which the engine builds on:** T4 is admissible throughout the setup chain, so a
beacon that does not complete within `join_deadline_ms` of `TABLE_READY` abandons formation exactly
as a `TABLE_READY` that never completes does. It is safe for the same three reasons and no others —
no `HAND_INIT`, no buy-in in the ledger, no card — and it names nobody.

**The residual, stated because it is the one place two documents still describe the same interval
differently.** `PROTOCOL.md` §3.1 puts the beacon inside the setup chain; §8.4 describes the setup
chain as *"`JOIN_REQUEST` through `TABLE_READY`"* and rests the whole of its coverage claim —
*"every stall from `JOIN_REQUEST` to the last hand is covered, with no gap"* — on §4.3's
abandonment rule, which is written against `TABLE_READY` alone. The interval from `TABLE_READY` to
the beacon's completion therefore falls outside §4.3's literal wording. It is covered twice over in
practice — by this row, and, were this row absent, by hand 1's own `hand_deadline_ms`, which §8.2
starts at `TABLE_READY` — so nothing stalls; what is missing is one sentence in `PROTOCOL.md` §4.3
extending its timer to the whole `hand_id = 0` chain. §13 carries it as one of the two outstanding
objections.

**`Diverged` is in T4's scope for the same reason and only for that reason (P5).** A table can
reach `Diverged` from checkpoint 1, which `PROTOCOL.md` §6.2 places after `TABLE_READY` and
therefore before any `HAND_INIT`. `Diverged`'s ordinary exits (T53, T54, T60) need every required
signer,
so one silent peer would freeze a table that has not started — the P5 freeze, at its cheapest. The
guard is unchanged and is what makes it safe: `hand_id == 0 ∧ ledger_in == 0`, so no chip, no card
and no commitment exists, and the only reachable next phase is the absorbing `TableClosed`. Where a
hand *is* live, `Diverged` is left by T57 instead, not by T4.

#### Seat-order randomness beacon (`SPEC_CS.md` §7, `MENTAL_POKER.md` §6)

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T6 | `AwaitingSeatRngCommit` | `RngCommit` | seat seated ∧ not yet committed | `AwaitingSeatRngCommit` | record commitment |
| T7 | `AwaitingSeatRngCommit` | `RngCommit` | all seated seats have committed | `AwaitingSeatRngReveal` | `ArmDeadline{Crypto}`; request reveals |
| T9 | `AwaitingSeatRngReveal` | `RngReveal` | the opening verifies against this seat's recorded commitment — the construction is `PROTOCOL.md` §4.4's and is not reproduced here (H8) — ∧ not yet revealed | `AwaitingSeatRngReveal` | record |
| T10 | `AwaitingSeatRngReveal` | `RngReveal` | all revealed | `AwaitingKeySetup` | compute `seed` per `PROTOCOL.md` §4.4, whose construction this cell does **not** restate (H8 — what it restated was incompatible with §4.4, see §13 item 30); derive seat permutation and initial `button_pos` from it (§7.9); `hand_id := 1`; **hand init** (§5.3); `RequestKeySetup`; `ArmDeadline{Crypto}` |
| T11 | `AwaitingSeatRngReveal` | `RngReveal` | commitment mismatch | `AwaitingSeatRngReveal` | **`Rejection`**, state bit-identical (I21): the opening is not recorded, so the beacon does not complete. `Fault{RngCommitmentMismatch}` — evidence only, with a self-contained proof (the commitment and the bad opening). **No unseating** (G4, D-010 point 3) |

**T11 no longer unseats, and that was the one peer-removal edge left in this document (G4).** It
read *"→ `Seating`, subject unseated"*. Against D-010 point 3, against §8.6 and §12, and against
the reasoning eight lines below that rejects the alternative fix for T8 and T12 *because* it would
buy an unseating the protocol may not perform. There is no carve-out for "no chips exist yet":
D-010 point 3 is written on the proof, not on the stake, and a document that removes a peer in one
row while stating in two others that it has no peer-removal transition is unimplementable as
written. So the mismatched `RngReveal` is a `Rejection` — the disposition P4 already chose for the
two adjacent rows — and the three beacon faults now have **one** disposition between them rather
than two:

* `RngCommitmentMismatch` (T11): rejected, recorded as a `Fault`, seat stays seated;
* `NoRngCommit` / `NoRngReveal`: unreachable since P4 and removed from `CryptoFault`.

A seat whose opening does not match its commitment therefore stalls the beacon exactly as a seat
that says nothing does, and the table closes by **T4** at `join_deadline_ms` with nobody named and
no chips in existence. What is lost is the fast path that removed the bad opener and let the other
seats form a table without it; what is gained is that this document performs no removal anywhere,
which is checkable in one grep rather than argued. Nothing else changes: the `Fault` record is
still produced, still signed, still permanent, and still the strongest artefact the beacon can
yield, because a bad opening is self-proving.

**T8 and T12 are deleted, and a stalled beacon now closes the table with no attribution (P4).**
They read *"`AwaitingSeatRngCommit` | `TimeoutCertificate{Crypto}` | subject has not committed ∧
`|V| ≥ 2` → `Seating`, subject unseated, `Fault{NoRngCommit}`"* and the same for the reveal. Both
are triggered by a wire message that cannot exist where they live: `PROTOCOL.md` §8.4 is normative
that *"`TIMEOUT_VOTE` and `TIMEOUT_CERT` are defined only over stages of a hand chain and are never
emitted in the setup chain (`hand_id = 0`)"*, and `PROTOCOL.md` §3.1 and §2 both put the seating
beacon in the setup chain. Keeping them would leave the engine's alphabet containing a variant no
legal message produces, which is the defect §8.4's box was written to remove and which this
document has now closed six times (`RevealRejected`, `StateAck`, `TimeoutCertificate{Join}`, here,
`Event::EquivocationProof` and `AbortKind::Equivocation` (§5.2 G2), and — under D-015 —
`Event::TimeoutCertificate` itself, whose deletion is the seventh instance and the first that
removes a variant because the **wire** stopped producing it rather than because this document
stopped consuming it).

**The behavioural change this makes, named rather than left to be discovered.** A seat that
commits and never reveals can no longer be unseated, at any table size, and
`Fault{NoRngCommit}` / `Fault{NoRngReveal}` are now **unreachable**: they are removed from the
`CryptoFault` variants the engine can produce, and an implementer should not write the arms that
raise them. One seat can therefore stall any *forming* table until `join_deadline_ms` expires and
T4 closes it. That is a griefing cost, not an integrity cost, and it is the same disposition D-010
gives everywhere else: no chips exist yet (`ledger_in == 0`, I1, I28), no card exists, no buy-in
has been made, and nobody is named. The alternative — carving the beacon out of `PROTOCOL.md`
§8.4's box and defining a `TIMEOUT_VOTE` envelope at `hand_id = 0` — would reopen the sentinel
question `PROTOCOL.md` §2.3 settled, to buy an unseating that D-010 point 3 forbids the protocol
from performing anyway. Two rows deleted is the smaller and the consistent answer.

The beacon runs **once per table**, not per hand. Per `MENTAL_POKER.md` §6, the shuffle chain
*is* the per-hand randomness; a commit/reveal beacon is needed only for the non-deck randomness
(seat assignment and the initial button), which is precisely what T6–T11 cover.

#### Per-hand key setup

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T13 | `AwaitingKeySetup` | `KeyPublished` | seat ∈ `deck.participants` ∧ not yet published | `AwaitingKeySetup` | record |
| T14 | `AwaitingKeySetup` | `KeyPublished` | all participants published | `AwaitingShuffle` | compute `agg_key_hash`; fix `deck.shuffle_order` — the `dealt_in` seats in **ascending seat order**, which is `PROTOCOL.md` §4.4's rule for `SHUFFLE_STEP` and not a free choice here — and `deck.deal_map` (§7.8, the map itself being `PROTOCOL.md` §4.5's) **before** any shuffle; `RequestShuffle{first}`; `ArmDeadline{Crypto}` |
| T15 | `AwaitingKeySetup` | `KeyRejected` | — | `HandAborted` | `Fault{BadKeyProof}`; `AbortRecord` names the seat |
| T17 | `AwaitingKeySetup` | `PlayerLeft`/`PlayerSitsOut` | — | `AwaitingKeySetup` | **`Rejection`** — both are hand-boundary-only stages (`PROTOCOL.md` §4.10); a hand is live here. A seat that has gone is handled by T16 or T57, never by an announcement |

Entering `AwaitingKeySetup` runs the **hand init** procedure of §5.3, which posts antes and
blinds. Blinds are therefore posted *before* any cryptography, by every seat that owes them
including `Absent` and `SittingOut` seats. That is D-005 in one sentence: an absent seat is a
chip pile that pays and takes no cards.

#### The shuffle chain

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T18 | `AwaitingShuffle` | `ShuffleVerified` | `seat == shuffle_order[shuffles_done]` ∧ chains from the previous `deck_hash` ∧ more shufflers remain | `AwaitingShuffle` | append `ShuffleRecord`; `shuffles_done += 1`; `RequestShuffle{next}`; re-`ArmDeadline{Crypto}` |
| T19 | `AwaitingShuffle` | `ShuffleVerified` | last shuffler | `AwaitingDeal` | `deck_commit := deck_hash`; `RequestOpen{hole indices of every participant, HoleOf(owner)}`; `ArmDeadline{Crypto}` |
| T20 | `AwaitingShuffle` | `ShuffleVerified` | wrong seat, or wrong parent deck hash | `AwaitingShuffle` | `Rejection` + `Fault{OutOfTurnShuffle}` |
| T21 | `AwaitingShuffle` | `ShuffleRejected` | — | `HandAborted` | `Fault{InvalidShuffleProof}` — `SPEC_CS.md` §8: the hand must not continue |

#### The private deal

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T23 | `AwaitingDeal` | `RevealTokensPublished` | indices ⊆ hole indices of seats **other than** the publisher | `AwaitingDeal` | record in `deck.tokens` |
| T24 | `AwaitingDeal` | `RevealTokensPublished` | publisher included **its own** hole index | `AwaitingDeal` | `Rejection` + `Fault{SelfRevealTooEarly}` — publishing your own token pre-showdown would let everyone open your hand |
| T25 | `AwaitingDeal` | `RevealTokensPublished` | any index ∉ this hand's hole indices | `AwaitingDeal` | `Rejection` + `Fault{TokenForUnauthorisedIndex}` — this is the §10 "early board" attack |
| T26 | `AwaitingDeal` | derived `DealComplete` | ∀ participant `P`, ∀ hole index `i` of `P`: every participant `≠ P` has published a token for `i` | `BettingPreFlop` | `street := PreFlop`; open the betting round (§5.4); `player_to_act` per A2; `ArmDeadline{Action}` |

`DealComplete` is derived from purely **public** information (T26's guard), so every peer decides
it at the same point in the event order. It is not "I received my cards" — that is per-node and
would break determinism.

#### Betting (one row set, instantiated for `BettingPreFlop`, `BettingFlop`, `BettingTurn`, `BettingRiver`)

Let `S` be the current betting phase, `next_reveal(S)` be `AwaitingFlopReveal`,
`AwaitingTurnReveal`, `AwaitingRiverReveal` for `PreFlop`, `Flop`, `Turn` and
`AwaitingShowdownReveal` for `River`.

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T28 | `S` | `Action` | `seat != player_to_act`, or action ∉ `legal_actions(state, seat)` (§6) | `S` | **`Rejection`**, state bit-identical, `Fault{IllegalAction}` attributable to the signer (§11) |
| T29 | `S` | `Action{Fold}` | legal ∧ `\|live\| ≥ 3` after the fold, or `\|live\| == 2` and the round is not closed | `S` | `folded := true`; advance `player_to_act`; `ArmDeadline{Action}` |
| T30 | `S` | `Action{Fold}` | legal ∧ exactly one live seat remains | **`Settling`** | `DisarmDeadline`; **no reveal is requested at all** — see §8.5 and D-005 case 1; compute the settlement and `Publish` this peer's own `HAND_COMPLETE` body (§3.4) |
| T31 | `S` | `Action{Check\|Call\|Bet\|Raise}` | legal ∧ ¬`round_closed` after applying | `S` | move chips; update `current_bet`, `last_full_raise`, `aggressor`; `acted_this_round := true`; advance `player_to_act`; `ArmDeadline{Action}` |
| T32 | `S` | `Action{…}` | legal ∧ `round_closed` ∧ `S != BettingRiver` | `next_reveal(S)` | `DisarmDeadline`; `RequestOpen{street indices, Public}`; `ArmDeadline{Crypto}` |
| T33 | `S` | `Action{…}` | legal ∧ `round_closed` ∧ `S == BettingRiver` ∧ `\|live\| ≥ 2` | `AwaitingShowdownReveal` | compute `showdown_order` (§7.7); `RequestOpen{hole indices of each live seat, OwnerOf(seat)}`; `ArmDeadline{Crypto}` |
| T35 | `S` | `PlayerLeft`/`PlayerSitsOut`/`PlayerSitsIn` | — | `S` | **`Rejection`** — all three are hand-boundary-only stages (`PROTOCOL.md` §4.10). A seat that leaves mid-hand announces nothing: it is still a key holder, nothing is unblocked by its departure, and the effect is felt at the next crypto wait as silence, not as an event |
| T36 | `S` | `Show`/`Muck` | — | `S` | `Rejection` — showdown declarations are only legal in `AwaitingShowdownReveal` |

T32 has one further guard worth stating separately because it is the all-in run-out:

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T37 | `next_reveal(S)` | `CardsOpened` | street cards appended ∧ `\|contenders\| ≥ 2` | the betting phase for that street | reset the round (§5.4); `player_to_act` per A2; `ArmDeadline{Action}` |
| T38 | `next_reveal(S)` | `CardsOpened` | street cards appended ∧ `\|contenders\| < 2` ∧ street < River | the **next** reveal phase, betting skipped | `RequestOpen{next street, Public}`; `ArmDeadline{Crypto}` — `POKER_RULES.md` A2: remaining streets are still dealt because they decide the pots |
| T39 | `next_reveal(S)` | `CardsOpened` | river opened ∧ `\|contenders\| < 2` ∧ `\|live\| ≥ 2` | `AwaitingShowdownReveal` | `RequestOpen{hole indices, OwnerOf(seat)}`; `ArmDeadline{Crypto}` |
| T40 | `next_reveal(S)` | `CardsOpened` | opened indices ≠ exactly the indices this street owes | `next_reveal(S)` | `Rejection` + `Fault{WrongRevealSet}` |

#### Showdown, settlement, hand end

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T42 | `AwaitingShowdownReveal` | `CardsOpened` | all required hands opened (§7.7) | `Settling` | fill `Seat::revealed_hole`; `DisarmDeadline`; compute the settlement and `Publish` this peer's own `HAND_COMPLETE` body (§3.4) |
| T43 | `AwaitingShowdownReveal` | `Muck` | `showdown_policy == TdaMuckWithForfeiture` ∧ seat is not the last unmucked | `AwaitingShowdownReveal` | `mucked := true` — an irrevocable forfeiture of every pot (§7.7) |
| T45 | `Settling` | `Settle` | **the `HAND_COMPLETE` collective stage of this hand is complete** — every seat of the `HAND_INIT` set has been heard (`PROTOCOL.md` §3.2), which is what fixes `TERMINAL(k)` (§3.1) | `HandComplete` | apply the settlement computed on entry to `Settling`: `build_pots` (A7), evaluate, award, refund uncalled excess, split with odd chips (A8), mark busts, extend `finish_order`; `Settled(settlement)`; **open checkpoint 8 and `Publish` this peer's own `STATE_HASH` body for it** (the box below) — §2.6's supersession rule applies as at any checkpoint opening, then `checkpoints.boundary := None`, which is what releases hand `k−1`'s record at the exact moment `TERMINAL(k)` is fixed (§2.6), then `checkpoints.live := Some(CheckpointState{ hand_id: k, number: 8, required: signed_this_hand, own: <the `state_hash` of the body just published>, dissent: None, heard: ∅, acked: ∅, agreed: None })`, **`required` read before hand init clears the set** and **`own` written here and never again (`Q4-e`)** — it is the value T49 and T50 compare against, and writing it at the opening is what keeps it readable after hand `k+1` has overwritten the state it came from |
| T46 | `HandAborted` | derived `AbortSettle` | — | `HandComplete` | **restoration, on every branch and for every `AbortKind` (D-010, §8.6)**: `∀ s: stack[s] += committed_hand[s]`, so every stack equals its `start_stack_this_hand` and no chip crosses between seats (I27, I2); `settlement.aborted := true`; `Fault` records already present. **No seat's `status` changes here** — the line that set `Absent` from `abort.attributed` is deleted (D-012, H1); see the note under the seat table below and I30. **Open checkpoint 8 and `Publish` this peer's own `STATE_HASH` body for it**, exactly as T45 does and with T45's slot discipline — `checkpoints.boundary := None`, then `checkpoints.live := Some(…)` — **the checkpoint is emitted and compared on this path too; what it does not do here is gate T47** (the box below) |
| T47 | `HandComplete` | derived `NextHand` | **the boundary gate is discharged** — hand `k` reached `HandComplete` through **T46**, in which case there is no gate, **or** it reached it through **T45** and checkpoint 8's `STATE_HASH` stage is complete (`checkpoints.live.heard ⊇ checkpoints.live.required`) — ∧ then §9.3's end conditions decide the branch | `AwaitingKeySetup` \| `Paused` \| `TableClosed` | rotate the dead button (A1.3), advance the blind level (§9.2), reset the hand, run **hand init** (§5.3), **then** `checkpoints.boundary := checkpoints.live.take()` and `checkpoints.agreed := None` — hand `k`'s boundary checkpoint is **moved down, not dropped**, and it is dropped at the next T45/T46 (§2.6). The clause this replaces was `checkpoint := None`, and it is `P1` |
| T61 | `HandComplete` with an undischarged gate \| `Diverged` when no hand is live and `hand_id > 0` | `HandDeadlineAbort` | `hand_id == state.hand_id + 1` — the abort names the hand that has **not started**, which is the hand whose `hand_deadline_ms` covers this window (§8.2). **That is the whole guard**, for T57's reason: the abort's chain position is a record and not a claim, checked loosely at the receiver by `PROTOCOL.md` §4.10, and an engine guard that re-imposed strictness would restore the deadlock that check was loosened to remove (G1) | `HandAborted` | run **hand init** (§5.3) for hand `k+1` first, exactly as T47 does and including the derived `HAND_INIT` publication, so `hand_id`, `level`, the button and `signed_this_hand` advance identically on both paths; **then the same slot move T47 makes** — `checkpoints.boundary := checkpoints.live.take()`, `checkpoints.agreed := None` — because the boundary this row leaves behind is a boundary `PROTOCOL.md` §4.9 still accepts events into (`P1`); then `AbortRecord{kind: HandDeadline, attributed: [], owed: [], observed_by: ∅, sequence: stalled_sequence, parent_event_hash: parent_hash}` — the two are **recorded, not verified**; **no `Fault` against anybody**, and from `Diverged` the `Fault{StateDivergence}` T50 recorded is retained and `table_faulted` keeps whatever value it holds; `DisarmDeadline`; restoration follows at T46 (§8.6, I27) |

#### Checkpoint 8 — the hand-boundary checkpoint, and the width it gives `HandComplete` (K-3)

**The defect.** `PROTOCOL.md` §6.2 places checkpoints *"at these points and no others"*, and
numbers 2 to 7 all require a `DECK_COMMIT` or a betting round to have happened. A drain hand has
neither: §5.3 step 9 sends `|dealt_in| == 1` straight to `Settling` with no key setup, no shuffle
and no reveal. Under D-013 that is not a corner — §12.1.2 derives **about forty consecutive drain
hands** for a heads-up table whose opponent has gone silent, which is the MVP's shipped regime
(§9.5). So the corpus's only cross-peer detection route was inert in exactly the regime D-013 made
the standard one, and the **tournament result was never checkpointed at all**: the last hand of a
drained-out table is a drain hand, and §9.3 condition 1 fires on its boundary.

> ### Checkpoint `8` — the hand-boundary checkpoint
>
> **Checkpoint `8` is placed at every hand boundary, on every hand, and it is the only checkpoint
> that a hand which deals no cards places at all.** It is emitted the moment `TERMINAL(k)` is fixed
> at this peer — as a side effect of **T45** on the settled path and of **T46** on the aborted
> path, that is, on entry to `HandComplete`. It needs no deck, no `DECK_COMMIT` and no betting
> round, which is why a `|dealt_in| == 1` drain hand places it, and why a hand that stalled at
> `HAND_INIT` and ended at T57 places it too.
>
> **What it covers.** The hand-boundary public state and nothing else: `hand_id = k`, the per-seat
> `stack` vector, occupancy and the roster order, the per-seat `status` vector (I30), `button_pos`,
> `sb_pos`, `bb_seat`, `level`, `ledger_in`, `ledger_out`, `finish_order`, `settlement.aborted`,
> and **`signed_this_hand` as of hand `k`** — the participation set D-013 makes hand `k+1`'s
> `dealt_in` and required emitter set out of. No card, no `deck`, no `LocalView` quantity, nothing
> derived from an abort's `attributed`, `owed` or `observed_by`, and no checkpoint record of its
> own (D-012, I30, I31, §2.6). The field list and the encoding are `PROTOCOL.md` §6.1's
> `PublicTableState`; **this document names what the checkpoint must be *about* and reproduces no
> part of the hash** (D-011 rule 1). **One of those quantities is not in §6.1 today and the
> checkpoint is worth little without it**: `signed_this_hand` must join `PublicTableState`, and
> — **that was done, and then undone.** It became §6.1's field 28 and was deleted again
> when measurement showed two honest peers on a lossy link cannot agree about it, so a
> hash containing it manufactured the divergence it was meant to detect. `PROTOCOL.md`
> §6.1 carries the figures. The engine still maintains the set under this name for its
> own use; what is gone is the requirement that two peers agree about it. The rest of
> this paragraph is left as written because the reasoning it records is sound and only
> its premise failed:
> that is the half of K-3 that belongs to `PROTOCOL.md` and is recorded on `DECISIONS.md`'s open
> list rather than decided here (§13 item 40). Until it is there, checkpoint 8 compares a hand
> boundary and not the participation set, which catches a stack or a button that has forked and
> misses the input K-1 is actually about.
>
> **Why `signed_this_hand` is the part that matters.** Every other quantity in that list is a
> function of the previous hand boundary and is already bound into `GENESIS(k)`, which is what T57's
> note establishes at length. The participation set is the **one** input to hand `k+1` that two
> honest peers can hold differently — that residual is **Q8** (§11) — and checkpoint 8 is where it
> stops being a quantity each peer reads alone and becomes one they compare.
>
> **Required emitter set, and the comparison that is not scoped on it.** The required set is
> `P(k) = signed_this_hand` as of hand `k`, the same participation-derived set as hand `k`'s
> `HAND_COMPLETE` (`PROTOCOL.md` §3.2, §4.10), so a seat that stopped signing in an earlier hand is
> not in it and cannot hold it. **T49 and T50 are not scoped on that set**: a `STATE_HASH` for
> checkpoint 8 from *any* seat is compared, in-set or not. That is deliberate and it is the whole
> value of the checkpoint — two peers that have each concluded the other is not a required emitter
> still collide here.
>
> **What a mismatch does.** Exactly what a mismatch at checkpoints 1 to 7 does, and nothing new:
> two distinct `state_hash` values for checkpoint 8 take the table to `Diverged` at **T50**, from
> whatever phase it has reached by then; the resolution procedure is `PROTOCOL.md` §6.3's; the
> terminuses are T53, T54, T60, and — for a divergence opened at a boundary, where no hand is live
> — **T61**.
>
> **And the mismatch can arrive after the boundary has been left, which is `P1` and which the
> single checkpoint slot dropped on the floor.** Hand `k`'s checkpoint 8 is retained in
> `checkpoints.boundary` until `TERMINAL(k+1)` is fixed (§2.6), because `PROTOCOL.md` §4.9 accepts
> a checkpoint-8 `STATE_HASH` of chain `k` until this receiver completes `HAND_INIT(k+1)` and a
> checkpoint-8 `STATE_ACK` of chain `k` until `TERMINAL(k+1)`. **Both are ordinary, non-adversarial
> arrivals**: every peer emits its `STATE_ACK` and its `HAND_INIT(k+1)` copy on the same trigger,
> and forwarding (`PROTOCOL.md` §1.5) reorders them whatever order they were written in. So T50
> can freeze on hand `k`'s boundary while the table is in `AwaitingKeySetup` of hand `k+1`, and
> T51 can complete hand `k`'s `STATE_ACK` stage after hand `k+1` has begun — which is what gives
> D-014's tier-2 finding a precondition at a hand boundary at all. Neither is a new phase, a new
> wait or a new terminus: T57 covers the first, because a hand is live, and the second changes no
> phase.
>
> **One thing is new since N1, and it is on the size of the set rather than on who may emit.** T49
> and T50 are still not *scoped* on `P(k)` — any seat's copy is compared, which is the paragraph
> above and the whole value of this checkpoint — but T50 now also **latches** the divergence when
> the compared checkpoint's own `|required| == 1`. That is the case this box was written for: the peer that concluded
> it was alone published the only required copy, and the copy that contradicts it came from the
> seat it had written off. Comparing against a set of one and losing is not a hand's worth of
> trouble, it is the fork, and §9.3 condition 0.6 is where it ends.
>
> **What it gates, and what it deliberately does not.** On the **settled** path (T45), T47 waits
> for the checkpoint's `STATE_HASH` stage to complete over `P(k)`. On the **aborted** path (T46) it
> gates nothing: the checkpoint is still emitted and still compared, and T47 fires as it always did.
> The asymmetry is not a convenience. On the settled path every member of `P(k)` demonstrated itself
> by emitting hand `k`'s `HAND_COMPLETE` copy, so the set the gate waits on has just proved it is
> there; on the aborted path `P(k)` contains, by construction, the seat whose silence ended the
> hand, and gating on it would be the exact defect §12.1 exists to prevent — **a terminus that
> depends on the participation of the peer whose non-participation is the reason for it**.
>
> **Its liveness exit is a timer and never a peer.** A seat can still stop between emitting hand
> `k`'s `HAND_COMPLETE` copy and its checkpoint-8 copy. **T61** closes that window on hand `k+1`'s
> `hand_deadline_ms`, which `PROTOCOL.md` §8.2 already starts at `TERMINAL(k)` — no new timer, no
> new `DeadlineKind`, no new event and no new wire message. T61 runs hand init for `k+1` and leaves
> for `HandAborted`, so the boundary that follows it is an **aborted-path** boundary and is
> ungated. **A gated boundary can therefore be followed only by an ungated one**, which is the
> whole termination argument for the phase and is checkable by reading two rows.
>
> **T61 runs hand init and then does not take step 9's branch.** Its `Next` is `HandAborted`
> whatever `|dealt_in|` came out of step 4, because the hand it initialised is over before it
> began: T46 restores every stack it just committed to antes and blinds, so nothing moved (I27),
> and the whole of hand `k+1` is one aborted hand of the kind the corpus already accounts for.
> `hand_id` and the blind level advance by one, exactly as they do for any hand that aborts. That
> is what makes the following boundary a **different** one — `P(k+1)` is the set that signed during
> hand `k+1`, which the seat that held boundary `k` open did not — and it is why the argument above
> is about two consecutive boundaries and not about one repeating.

**K-3b: `HandComplete` gains width, and this is the decision the gate asked for.** The phase was
zero-width — T47 was *"derived, immediate, no external input"* — and it is one of only two phases
in which `PLAYER_SIT_IN` is legal, the other being `Paused`, which needs every seat to have stopped
signing. Nothing could ever be interposed between entering the phase and leaving it, so T58 and
T59's `HandComplete` branches were unreachable rows and D-013's *"it rejoins by signing … that is
the entire test"* had no window in which the test could be taken. **The phase now has width on the
settled path**, which is where the window is wanted: under D-013's steady state every hand ends at
T45 (a drain hand goes `Settling → HandComplete`), so a table with a silent seat offers a boundary
window on **every** hand, and a table playing normally offers one on every hand that is not
aborted.

**Two things follow that a reader should check rather than take on trust.**

1. **A seat returning from silence needs no `PLAYER_SIT_IN` at all — and since `P2-e` that is true
   for a second reason, because the first one was true only inside a window that can be
   zero-width.** Since H1 nothing sets `Absent`, so a seat that went
   silent is still `Active` to every reader of the field (§12). Its own checkpoint-8 `STATE_HASH`
   is a chained event that T49 accepts — from any seat, in-set or not — so accepting it adds the
   seat to `signed_this_hand` under §2.8's rule, and §5.3 step 4 deals it in at the next boundary.
   That is D-013's readmission rule with the window it was missing — **and the window is the part
   that could not be relied on**: it closes at a complete `HAND_INIT(k+1)`, which at a peer in the
   solitary regime is required of `{self}` and self-completes, so at exactly the peer D-013's
   steady state produces the window has **zero width** and the returning seat can never be heard.
   `PROTOCOL.md` §4.9's readmission set `A` is what closes that, and **T67** and §5.3 step 4's
   hand-off are its engine half (`P2-e`): a copy that arrives after the close is carried into the
   next hand init instead of being dropped, where it makes the returning seat's own
   `HAND_INIT(m+1)` copy **acceptable** — and being accepted is what puts it into `P(m+1)` and
   makes it a required emitter, and dealable, at hand `m+2`. **The latency is one hand and it is
   `P2`'s price**: the readmitted seat is not `dealt_in` at `m+1`, because `dealt_in ⊆ P(m)` and
   `A` is not in `P(m)`. The width remains the fast path; `A` is the one that
   works when the width is nil. `PLAYER_SIT_IN` is needed only
   for the seat that *deliberately* sat out, whose `status` is `SittingOut` and which only T59 can
   return to `Active`; that seat is by definition present, and the settled-path window is where it
   sends.
2. **The width costs nothing in the bound §12.1.2 publishes.** Both of D-013's cases are re-derived
   there against these rows and come out at the same numbers — one `hand_deadline_ms` for a seat
   that went silent at a boundary, two for one that went silent mid-hand — because the boundary that
   follows an aborted hand is ungated, and the boundary that follows a settled hand is gated on a
   set every member of which has just emitted.

#### Failed cryptographic verification of a reveal (`PROTOCOL.md` §4.10 `cause = 3`)

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T48 | `AwaitingDeal` \| `next_reveal(S)` \| `AwaitingShowdownReveal` | `RevealRejected` | — | `HandAborted` | `Fault{InvalidRevealProof}`; `AbortRecord` names the seat and the index |

T48 exists because `Event::RevealRejected` was previously consumed by no transition at all: a
failed Chaum–Pedersen DLEQ verification was silently discarded and the hand stalled to a timeout
instead of aborting, which attributed the wrong thing. It matches T21's treatment of
`ShuffleRejected`, `PROTOCOL.md` §4.10 `cause = 3` and `CRYPTOGRAPHY.md` §8 rule 1.

#### The hand deadline — the abort that needs no certificate (D-008 point 2, D-009 rule 2)

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T57 | any phase in which a hand is live — 4–14 (`AwaitingKeySetup` … `AwaitingShowdownReveal`, betting included), **`Settling` (15)**, and **`Diverged` (20) when a hand is live** | `HandDeadlineAbort` | `hand_id` matches the live hand ∧ `stalled_sequence` is a stage index of that hand. **That is the whole guard**: the abort's chain position is a record of where its emitter believed the hand stopped and is checked loosely at the receiver by `PROTOCOL.md` §4.10, so the engine must not re-impose a strict check (G1) | `HandAborted` | `DisarmDeadline`; `AbortRecord{kind: HandDeadline, attributed: [], owed: [], observed_by: ∅, sequence: stalled_sequence, parent_event_hash: parent_hash}` — the two are **recorded, not verified**; **no `Fault` against anybody** — from `Diverged` the `Fault{StateDivergence}` T50 already recorded is retained, and `table_faulted` keeps whatever value it holds (T57 never sets it); restoration (§8.6, I27) |

**T57 is the answer to "what happens when a hand cannot proceed", and under D-015 it is the
*only* answer rather than the residual one.** The four situations below were the cases that a
certificate could not settle. With no certificate produced at all, the list collapses to a single
line and the four become instances of it rather than exceptions to something else:

> **Every cryptographic stall, at every table size, in every phase in which a hand is live, ends
> at T57.** There is no other terminus for one, because there is no other event that can end a
> stalled stage. The seat that stalled is named nowhere; every stack is restored (I27).

The four cases are kept because each names a *shape* of stall an implementer will meet, and
because a later version that restores the certificate must re-derive which of them it changes:

1. **`|V(subject)| < 2`.** Was heads-up always. **Now every table**, because `|V|` is not
   computed at all — D-015 generalises the below-the-floor disposition to the whole corpus, and
   §8.4's floor rule survives as the reason it is safe to do so rather than as a live gate.
2. **Two or more seats silent at one stage.** No unanimity was reachable, because each voter set
   contained the other subject. **Now not a distinguishable case**: one silent seat and five
   silent seats take the same path. Q3 / OQ-E / `PROTOCOL.md` Q-02 stays open for the later
   version and blocks nothing here.
3. **A required voter that is present but will not vote.** **Now vacuous** — there are no voters.
   The residue of the case is real and is not a deadline problem: a seat that is present, keeps
   publishing its shares, and simply refuses to act stalls one hand to T57 and is then outside
   `signed_this_hand`, so D-013 skips it from the next (§5.3 step 4).
4. **A reconciliation that never completes (P5).** Unchanged by D-015 and unrelated to it: T53,
   T54 and T60 need every required signer's reconciliation round, and T57 is the exit when one
   never arrives — see the `Diverged` note below.

In all four the hand ends with nobody named and every seat receiving exactly its own
`committed_hand` back (I27). What it costs is the wait: `hand_deadline_ms` is **not a constant** —
it is a per-table parameter bounded below by `HAND_DEADLINE_MIN(n)` (`PROTOCOL.md` §8.2, `G4-P3`,
`G5-Q6`), and that bound already budgets a whole legal hand of human action time plus one reopening,
so a stalled hand takes tens of minutes to end rather than seconds at **every** configuration a
conforming joiner will accept. The figure for any one table is the founder's advertised `n(17)` and
appears in this document nowhere (D-011 rule 2). `PROTOCOL.md` §8.3 records that
trade in the same words and prefers it to an effect one signature could manufacture; this document
does not re-derive it. After T46 the next hand begins automatically (T47): **no timeout of any kind
ends the tournament or the cash game** (§8.1, D-006 §5).

**T57's guard names the chain head, not the armed stage deadline, and that is what lets one row
serve both `AwaitingKeySetup` and `Diverged`.** The emitter picks that pair from its own chain
head — the successor of the last stage complete *at it*, and that stage's `stage_hash`. In a
betting or crypto phase that is what `state.deadline` was armed against; in `Diverged` the
checkpoint stage never completed, so it is the stage before it. Neither is checked by the receiver
and neither needs to be agreed: `PROTOCOL.md` §4.10 accepts an abort whose chain position differs
from the receiver's own, precisely because two peers can honestly disagree about which stage
stalled, and nothing downstream reads the value — `ABORT_TERMINAL(k)` is a function of `GENESIS(k)`
alone (`PROTOCOL.md` §3.1). The pair is recorded in the `AbortRecord` and is a statement about
where *this emitter* believed the hand stopped, nothing more.

**T57 fires, and the reason it could not is deleted rather than defaulted (P3, G1).** Two rulings in
`PROTOCOL.md` do it and both are theirs, not this document's:

* **§3.2's third stage shape.** `HAND_ABORT` is a *witness-independent terminal stage* with **no
  required emitter set**, closing on the first copy that verifies and passes §4.10's gate. The old
  cell — the `HAND_INIT` set minus `attributed` — left the silent seat inside the required set, so
  the abort that exists to dispose of a vanished seat waited for that seat. The named default this
  document offered instead — the same set minus every seat the stalled stage still owes — was not
  derivable from agreed state and forked `TERMINAL(k)`. Both horns are gone, and the `stage_hash`
  formula this document proposed for the third shape is not needed and was not adopted, because
  §3.1 takes the terminal value from `GENESIS(k)` instead.
* **§5.2.1's slot key, which contains `event_type`.** The abort sits at the stalled stage's own
  `sequence`, so its emitter has usually already signed a `class = 0` body at that
  `(sequence, sender)`. Under the previous key those were one slot: the abort was rejected at every
  receiver including its own emitter, and was simultaneously a verifying proof against the honest
  peer the protocol *required* to emit it. That is G1, and it is closed by the key rather than by a
  rule about this message.

It is D-010 that made both available — with restoration the body's `final_stacks` is
`start_stack_this_hand`, agreed since `GENESIS(k)`, whereas under forfeiture it was a function of
`attributed` and of the pot and two peers could derive two bodies. `owed` is `[]` on this path for
the same reason. Nothing here is a default and there is nothing left for an implementer to guess.

**Where T57 is not admissible, and what covers those phases instead.** Phases 1–3 have no live
hand — `hand_id == 0`, no `HAND_INIT`, and `GENESIS(1)` not yet derivable — so T57's guard cannot
hold there whatever a timer says, and a stall there is **T4's**. Hand 1's `hand_deadline_ms` window
does open at `TABLE_READY` and therefore spans phases 2–3 (§8.2), and **nothing there depends on
which of the two timers is shorter**: `n(17)` and `n(18)` are independent advertised parameters with
independent caps (`PROTOCOL.md` §7.2), so a conforming advert may order them either way. If the
hand-deadline timer expires first, the `HandDeadlineAbort` it produces meets no admissible row —
T57 needs a live hand and T61 needs `HandComplete` or `Diverged` with `hand_id > 0`, and phases 2–3
are neither — so it is a `Rejection` that changes nothing and **T4 still closes the table**. The
exit is T4's on both orderings (§8.2, §12.1.1 rows 2 and 3). **`Settling` *is* in
scope, and that is new in this revision**: it waits for the `HAND_COMPLETE` stage (T45), a seat can
go silent between the last reveal and its own copy of that stage, and hand `k`'s deadline is still
running there because `TERMINAL(k)` is exactly what has not been fixed. `HandAborted`,
`HandComplete`, `Paused` and `TableClosed` have no live hand — and since K-3 that is a statement
about T57's scope only, not about coverage: `HandComplete` can now wait, and it is **T61**, not
T57, that ends the wait, on the deadline of the hand that has not started (the checkpoint-8 box
above). A stalled `HAND_INIT` **is** covered, and that is
R-1's fix: the hand deadline runs from `TERMINAL(k−1)` rather than from `HAND_INIT` (§8.2), and the
phase at that moment is `AwaitingKeySetup`, which is in T57's scope — so a `HAND_INIT` stage that
never completes ends the hand like any other stall, with no new row.

**`Diverged` is now in scope, and the exclusion that stood here is deleted (P5).** It read
*"`Diverged` … ends at T53 or T54 and a hand-deadline abort must not race with reconciliation."*
That left phase 20 with no liveness exit at all: T53, T54 and T60 all require **every** required
signer
to have emitted its reconciliation round, T57 was inadmissible, and no timer covered the phase — so
one silent peer froze the table forever with `Σ committed_hand` locked. The hand deadline is
therefore **not disarmed on entry to `Diverged`**, and on expiry T57 ends the hand there with the
same body, the same restoration and the same absence of attribution as anywhere else. Three things
make that safe rather than a new race:

* The chip disposition is T54's exactly. Under D-010 every abort restores every stack to
  `start_stack_this_hand`, so the timer decides nothing about chips that the phase had not already
  decided. The `Fault{StateDivergence}` record T54 would write already exists: T50 wrote it on
  entry, and T57 retains it.
* The race the exclusion feared is bounded by the floor rather than by a figure.
  `hand_deadline_ms` is never below `HAND_DEADLINE_MIN(n)` (`PROTOCOL.md` §8.2), against
  a reconciliation that exchanges a handful of missing events over an already-open mesh; a genuine
  reconciliation finishes inside it by orders of magnitude, and inside the floor, so the margin does
  not depend on what the founder advertised above it. A reconciliation that does not is the
  case T57 exists for.
* Nothing is decided *about a peer*. The abort names nobody, so a peer that was merely slow to
  reconcile loses a hand, not chips and not its seat.

**T57 from `Diverged` does *not* fault the table, and that is a deliberate difference from T54.**
`PROTOCOL.md` §6.4 is explicit that the table fault is `cause = 4`'s alone — *"it is the only cause
after which no further hand is dealt"* — and T57 emits `cause = 1`. So `table_faulted` stays
`false`, §9.3 condition 0.5 does not fire, and the table deals hand `k+1`. The previous revision
said the opposite ("the table stays faulted") and was wrong on the owner document's own rule.

**Why that is safe, and what the residual is.** Every quantity that survives an aborted hand is a
function of the state at the *previous* hand boundary, which is bound into `roster_hash(k)` and
hence into `GENESIS(k)` and is therefore agreed: restoration puts every stack back to
`start_stack_this_hand` (I5, I27), the hand's own working state — `board`, `folded`,
`committed_*`, `history`, `deck` — is reset by hand init, `button_pos`
advances from the previous button and the previous occupancy (A1.3), `finish_order` is extended
only at T45 and an aborted hand busts nobody, and `ledger_out` moves only in hand init step 0 from
`Leaving` marks set at a hand boundary. So the mid-hand disagreement that opened the divergence is
**discarded** rather than carried forward, and `GENESIS(k+1)` is a function of `GENESIS(k)` alone
(`PROTOCOL.md` §3.1) whatever the peers made of the middle of hand `k`.

The distinction §6.4 draws is then the right one: `cause = 4` is the case in which the transcripts
are byte-identical and the *derivation* still differs, which is a client disagreement that will
recur next hand and therefore must close the table; a `cause = 1` timeout inside `Diverged` is
indistinguishable from an ordinary network stall and does not imply any such thing. The residual is
that a table can be pushed through this path repeatedly — divergence, stall, neutral abort, next
hand — at `hand_deadline_ms` per hand. That is the residual §12 records for an adversary that keeps
*sending*, and it is unaffected by D-013: a peer that publishes a bad `state_hash` every hand is
signing chained events every hand, so it never stops being a required emitter and never gets
skipped. D-013 bounds the cost of a seat that goes **silent**; it does nothing about one that stays
noisy, and this path is the clearest example of the difference.

`Diverged` entered before any hand — from checkpoint 1, which `PROTOCOL.md` §6.2 places after
`TABLE_READY` — has no hand deadline running, and is covered by T4 instead (see the note under the
seating table).

**A third way into `Diverged` opens with checkpoint 8, and it is T61's second scope.** A boundary
checkpoint can diverge while no hand is live: T50's scope is *any* phase except `Diverged` and
`TableClosed`, so a conflicting checkpoint-8 `STATE_HASH` accepted in `HandComplete` — or later, in
`Paused` — freezes the table there. Neither T4 (`hand_id == 0 ∧ ledger_in == 0`) nor T57 (a live
hand) is admissible in that state, so it would have been phase 20 losing its exit again, from the
new direction this fix opens. **T61 is that exit**: hand `k+1`'s `hand_deadline_ms` is running,
having started at `TERMINAL(k)` (§8.2), and T61 leaves for `HandAborted` with the same body, the
same restoration and the same absence of attribution as T57, retaining the `Fault{StateDivergence}`
T50 wrote and leaving `table_faulted` alone — `PROTOCOL.md` §6.4 reserves the table fault for
`cause = 4`, and T61 emits `cause = 1`, exactly as T57 does from a live-hand `Diverged`. With T4,
T57 and T61 the three intervals are covered with no gap: before any hand, inside a hand, and
between two hands. Every reachable `Diverged` has an exit.

#### Hand-boundary seat changes (`PROTOCOL.md` §4.10)

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T58 | `HandComplete` \| `Paused` | `PlayerLeft` | seat occupied ∧ `status ∉ {Leaving, Removed}` — **`Removed` is excluded and it is not a detail (D-014, I34)**: `Leaving` empties the seat at the next hand init and moves `ledger_out` by the stack it takes off the table, so without this conjunct a removed player leaves voluntarily one boundary later and walks off with the chips D-014 point 3 exists to blind off | unchanged | `status := Leaving`. **No chip moves and neither ledger counter changes here**: the seat is removed and `ledger_out` incremented at the next **hand init** (§5.3 step 0), which T47 runs before §9.3's end conditions are evaluated, so a table every seat has left is empty by the time condition 0 looks at it |
| T59 | `HandComplete` \| `Paused` | `PlayerSitsOut` / `PlayerSitsIn` | seat occupied ∧ `status ∈ {Active, SittingOut, Absent}` — a seat may sit back in from `Absent`, which is D-006 §4's "sit back in at a hand boundary". **`Removed` is not in the set and never will be (D-014 point 4, I34)**: this is the transition the one-way exit is one-way *against*, and it is the only place in this table a removed seat could otherwise have re-entered | unchanged, **except** `Paused` + `PlayerSitsIn` when **§5.3 step 4's own predicate, evaluated on the state this transition leaves behind, would yield `\|dealt_in\| ≥ 2`** → `AwaitingKeySetup` | `status := SittingOut` / `Active`, effective immediately because no hand is live; on the `Paused` exit, **hand init** (§5.3) |

**T59's `Paused` exit stops asking whether a seat is *willing* and asks §5.3 step 4 instead
(K-7).** The guard read *"at least two seats are again **willing** and have chips"*, where
*willing* expanded to `status == Active ∧ stack > 0`. That is a status word on a progress path —
the last one left after D-013's sweep — and worse, it is a **second** predicate for a question §5.3
step 4 already answers three lines later in the same transition's side effects, on a different
basis. The two disagree by construction: one seat signs a `PLAYER_SIT_IN`, three seats that have
been silent for hands are still `Active` with chips, the old guard counted four and let the table
out of `Paused`, and step 4 then yielded `|dealt_in| == 1` and a drain hand. The table left a phase
that owes nobody anything in order to play a hand nobody is contesting.

**The fix is not to replace one word with another; it is to stop having two predicates.** The
guard *is* §5.3 step 4, referenced and not restated (D-011 rule 1): the exit fires exactly when
running step 4 on the post-transition state would deal two or more seats in, which after this
transition means `|{s : stack[s] > 0 ∧ status[s] == Active ∧ s ∈ signed_this_hand}| ≥ 2`. Written
that way the guard and step 4 cannot disagree, at any table size, under any interleaving, because
there is one predicate and one evaluation of it.

**Why `status` survives inside that predicate, and why that is not the thing D-013 removed.**
D-013 replaced status as the **liveness gate** — the thing that decides whether a seat can be
waited for — because a silent seat can never change its own status, so a gate on status was a fixed
point (J2). It did not, and could not, remove `status` as the record of a choice a seat made *with
its own signature*: `SittingOut` is set by T59 on that seat's own `PLAYER_SIT_OUT` and by §8.5's
auto-action limit, both of them chained events (I30(a)), and a seat that asked not to be dealt in
must not be dealt in merely because it is also signing. So the predicate reads both — participation
**and** the seat's own declared willingness — and the defect was never the word `Active`, it was
the second copy.

**What `signed_this_hand` holds in `Paused`, since the guard now reads it.** §5.3 step 8 clears the
set at hand init and §2.8's rule accumulates it on every accepted event; between the two, the set
is *"every seat heard from since the last hand init"*, and no hand init runs while the table is
paused. So a `PLAYER_SIT_IN` accepted in `Paused` puts its sender in the set, and two seats each
sending one is exactly what the exit requires — demonstrated participation, by two peers, with no
timer and no status assertion in it. That is also the answer to how a paused table ever restarts: a
seat that is already `Active` still sends `PLAYER_SIT_IN`, whose guard admits `Active`, and the
effect that matters is not the no-op status assignment but the signature.

**`Absent` has no producer in this document at all, and T46 is where its last one was deleted
(H1, D-012).** T46 set it for every seat in `abort.attributed`. `attributed` is a field of the
`HAND_ABORT` **copy that this receiver happened to accept**, and since P3 made the terminal stage
witness-independent — it closes on the first copy that verifies and passes `PROTOCOL.md` §4.10's
gate, and §4.10 states no precedence rule between two `HAND_ABORT` copies — two honest peers can
accept different copies of the same terminal stage. The certificate path (T16, T22, T27, T41, T44)
carries `attributed = [subject]`; the hand-deadline path (T57) carries `[]`. So one peer marked a
seat `Absent` and another did not, from the same hand, with nothing wrong at either.

That difference does not stay local, and it is visible in two places. `PROTOCOL.md` §6.1 puts
`sitting_out` and `absent` in `PublicTableState`, so the two peers' next `state_hash` differs and
T50 takes the table to `Diverged` — and §6.1's own rule, that those vectors *"must be a
deterministic function of accepted chained events"*, is the rule T46 broke, with §6.1 deferring to
this document for which events set them. Where the abort falls after the hand's last checkpoint,
which is the interleaving H1 describes, no checkpoint intervenes and the damage lands on the next
hand instead: `dealt_in` is `false` for an `Absent` seat (§5.3 step 4) and
`PROTOCOL.md` §4.4 requires `bb_seat` to be an occupied, non-absent seat, so the two peers derive
different `n(8) dealt_in` and different `n(3) bb_seat` for hand `k+1`. `HAND_INIT` is a
**collective** stage in which every receiver recomputes every field and rejects a copy that
differs, so each peer rejects the other's: the stage never completes, hand `k+1` stalls to the hand
deadline, T57 aborts it with `attributed = []` — which changes nothing — and hand `k+2` begins from
the same divergence. **The table never completes another hand.** That is phase 4 losing its exit
again, reached from a new direction, and §12.1's row 4 does not see it because the row asks whether
*this* hand ends, and this hand does end.

**The line is deleted rather than repaired, and D-012 is why.** *No canonical state may be derived
from a quantity that can differ between honest receivers.* A seat's `status` is canonical — it
feeds `dealt_in`, `bb_seat` and the next hand's genesis — and which abort copy a peer accepted is
exactly such a quantity. `PROTOCOL.md` §4.10 already ruled it out in terms: *"no receiver may derive
a seat's state from it"*; T46 was the corpus's one violation of that sentence, and with the line
gone the prohibition is true everywhere with no exception. Under D-010 an abort's attribution is
evidence with no automatic consequence, so deriving a status from it was never going to be right;
the two remaining repairs — an agreed basis for the marking, or an abort-versus-abort precedence
rule — are respectively the thing §3.1 deliberately erased and a consensus protocol, which is what
D-010 was adopted to stop building. **A seat's status changes only through a chained event**, and
the invariant that makes this checkable rather than argued is **I30**.

**What sets `Absent`, then: nothing in this document.** Two routes to a *non-participating* seat
survive on paper and **one of them is gone in version 1**: `PlayerSitsOut` (T59), which the seat
itself sends and which is unchanged; and `auto_action_limit` consecutive auto-actions marking a
seat `SittingOut` at the next hand boundary (§8.5), which needed T34 and is therefore
**unreachable under D-015** — the counter is never incremented, so the marking never fires.
`SeatStatus::Absent` therefore keeps its variant, its definition (§2.4) and its behaviour — a seat
in it pays blinds and antes, takes no cards and drains (§5.3 steps 4, 6, 7) — and has no transition
that enters it. That is stated rather than tidied away, because `SittingOut` and `Absent` behave
identically to the engine (§2.4), so the *state* the deleted line produced is still reachable —
what is gone is the ability to put a seat into it **without that seat's own signature**.

**And a seat that has genuinely vanished can send neither, which is exactly the case D-005 is
about.** T59 needs the seat; §8.5's `auto_action_limit` route needed a certificate and under
D-015 never runs at all, at any table size — the sentence that stood here scoped it on
*"needs `|V| >= 2` and therefore never runs in the MVP's heads-up regime (§9.5)"*, and the
scoping is now unnecessary. For two passes this document treated that as the open
problem and called it **Q7**: *what marks a seat `Absent`?*

**That was the wrong question, and D-013 answers the right one instead (J2).** Marking a seat
`Absent` would never have helped, because `PROTOCOL.md` §4.4's required emitter set for `HAND_INIT`
named absent and sitting-out seats **in its inclusion clause**. No status removed a seat from it.
The status field was never the liveness gate, and asking what would set it was asking about the
wrong mechanism — which is why two passes of correct reasoning about `Absent` left the table
frozen. The right question is *what removes a seat from a collective stage's required emitter set*,
and D-013's answer does not mention status at all:

> **A seat is required to emit in hand `k+1` only if it signed at least one chained event during
> hand `k`.** For the first hand, the required set is the signers of `TABLE_READY`.

**What this document owns of that rule is `dealt_in`, and narrowing the emitter set alone would not
have worked without it.** A seat that is not a required emitter of `HAND_INIT` but is still
`dealt_in` is still a party to the hand's `n`-of-`n` key (§2.5, `deck.participants`), so the stall
would simply move from `HAND_INIT` to `AwaitingKeySetup` and cost the same whole `hand_deadline_ms`. So §5.3
step 4 reads the same predicate the wire does: `dealt_in[s]` requires `s ∈ signed_this_hand` as of
the previous hand. One predicate, two consumers, no second definition (D-011 rule 1).

**Why this is D-012-clean.** "Did seat `s` sign an event this peer accepted as content of hand `k`"
is a function of the accepted chain, not of a timer, a connection state or a heartbeat. It is not
`attributed`, not `owed`, not `observed_by`. Where the stage completed, it is exactly `stage_hash`
membership (`PROTOCOL.md` §3.2) and every peer holding that prefix agrees on it by construction.
The one input that is *not* agreed by construction is a contribution to the stage that stalled —
no `stage_hash` ratifies it — and that residual is stated rather than hidden: it is **Q8** (§11), it
is `PROTOCOL.md`'s to close, and its failure mode is a `HAND_INIT` stage that does not complete,
which is loud and costs a hand, not a chain fork, which is silent and costs the table.

**What it costs.** A seat that goes silent stalls the hand it went silent in and, if it went silent
after signing something in that hand, the hand after it as well — one `hand_deadline_ms` each. From
then on it is skipped: not dealt in, no cards, no key share, and the blinds and antes eat its stack
until it busts, which is D-005 as the owner stated it. §12.1 derives the exact bound.

T58 and T59 exist because the three seat-state messages are legal **only** at a hand boundary
(`PROTOCOL.md` §4.10, M4) and this document previously had rows for them only mid-hand, where they
are illegal, and in `Seating` (T3). Without these rows the one phase in which a `PLAYER_LEAVE` is a
legal wire message was the phase in which the engine's own "a transition not listed does not exist"
rule discarded it — the defect T48 fixed for `RevealRejected`.
T59's second branch is the `Paused` + `PlayerSitsIn` bullet the previous revision carried as prose
below the table; it is a numbered row now so that I13 can see it.

**Why T58 marks rather than removes.** `PROTOCOL.md` §4.4 is normative that `ledger_in` and
`ledger_out` "change **only** at a hand boundary and only through" `HAND_INIT`'s `n(11)
ledger_delta`, which every receiver recomputes and rejects on mismatch. So the exit is *announced*
at T58 and *applied* at the next hand init, where it becomes one negative entry in that vector —
one place where the ledger moves, one artefact that records it, and I28 unchanged. Between the two
the seat keeps its stack, so I1 and I5 hold continuously, and it can never be dealt into a hand in
the meantime: step 0 empties it before step 4 assigns `dealt_in`, so a `Leaving` seat posts no
blind, takes no card and joins no `deck.participants`.

#### Checkpoint, divergence and dispute (`SPEC_CS.md` §15, `PROTOCOL.md` §6.3)

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T49 | any phase except `Diverged`, `TableClosed` | `StateHash` | **`c := checkpoints.named(e.hand_id, e.checkpoint)` exists** ∧ `round == 0` ∧ **`e.state_hash == c.own`** | unchanged | `c.heard ∪= {e.seat}`. **`named(h, n)` is the three-slot selector of §2.6**: with one slot the phrase *"that checkpoint"* had one referent, and since `P1` a checkpoint-8 `StateHash` of hand `k` can arrive while hand `k+1` is live. An event naming no slot is a `Rejection` and the state is bit-identical (I21) — no slot is created for it, which is §2.6's bound. **The guard's second change is `Q4-e`**: it read *"the value equals this peer's own derivation at that checkpoint"*, which on `checkpoints.live` an implementation could satisfy by re-deriving from `TableState` and on `agreed` and `boundary` it could not, because the state that produced the value has been overwritten — the guard had no readable term on the two slots `P1` added for it. `c.own` is that derivation, fixed when the record was opened, so the comparison is the same one and is now computable wherever the record is. **T49 and T50 are exhaustive and disjoint on `round == 0`**, which is what makes the pair a total function of the arriving value and removes the ordering dependence the counter had |
| T50 | any phase except `Diverged`, `TableClosed` | `StateHash` | **`c := checkpoints.named(e.hand_id, e.checkpoint)` exists** ∧ `round == 0` ∧ **`e.state_hash != c.own`** — some accepted value for that checkpoint differs from this peer's own, which is *"two distinct values exist"* stated in the one form the record can compute (`Q4-e`) | **`Diverged`** | `c.heard ∪= {e.seat}`; **`c.dissent := c.dissent.or(Some(e.state_hash))`** — written at most once per record, so the retained evidence is the pair `(c.own, c.dissent)` and *"retain both signed values as evidence"* is now the literal content of the record instead of an instruction with no field behind it (`Q4-e`); freeze: no `RequestOpen`, no `ArmDeadline`, no chip movement; **the hand deadline is not disarmed** (P5, T57); `Fault{StateDivergence}`; **and `solitary_contradicted := true` when `\|c.required\| == 1` — this conjunct is N1's second half and since `P1` it is readable on the stale route as well as the live one**. It is read off the **named checkpoint's own required set** and not off a remembered regime, because `c.required` **is** the regime for the stage being compared — `P(k-1)` at checkpoints 1–7 and the `P(k)` snapshot at checkpoint 8 (`PROTOCOL.md` §4.9), fixed when the checkpoint opened and therefore not grown by the very copy that contradicts it. A one-member required set with two distinct values in it is exactly *a peer comparing its state against a set of one and being contradicted from outside that set*, which is T62's situation reported by a different message: two distinct values cannot exist for one checkpoint unless a seat other than this one signed one of them. Without the conjunct, a solitary peer that diverges by **hash** rather than by **emitter** leaves through T57 or T61, restores, deals another solitary hand, meets the same mismatch at the next checkpoint 8 and freezes again, every `hand_deadline_ms`, forever — J2's fixed point on the one path that set no latch, with **I33(c) passing throughout because it was scoped on a latch this trigger never set**. **The conjunct is what keeps it off healthy tables**: where two or more seats were required to publish the checkpoint, a mismatch still costs one hand and the table plays on, exactly as it did before N1. **And since `P1` this row is the carrier for the stale boundary mismatch at a receiver that was not alone** (§4.1): the checkpoint is in `checkpoints.boundary`, the conjunct is false there because `\|c.required\| >= 2`, and the disposition is the ordinary one — one hand, not the table. **And the guard is re-derived rather than restated, because the term it read did not exist (`Q4-e`).** *"Two distinct `state_hash` values now exist"* was carried by `values: u8`, a **count** — and a count cannot be maintained without the values, since deciding whether an arriving hash increments it requires comparing it against what was already seen. The field held nothing to compare against, so the guard was undefined on every slot, `live` included. `e.state_hash != c.own` is the same predicate: this peer's own copy is always one of the values in the stage, because it publishes it (§3.4 and §6.2 for checkpoints 1–7, I32(a) for checkpoint 8), so the number of distinct values is exactly `1 + c.dissent.is_some()`. A **third** distinct value adds a signature to a divergence already detected and changes nothing — the phase is `Diverged` and this row's own state exclusion keeps it from firing again — which is why two slots suffice and a container fed from the network, which §27 forbids, is not needed |
| T51 | any phase except `Diverged`, `TableClosed` | `StateAck` | **`c := checkpoints.named(e.hand_id, e.checkpoint)` exists** ∧ `c.heard ⊇ c.required` ∧ **`c.dissent.is_none()`** — the `STATE_HASH` stage of the named checkpoint is complete and unanimous, and unanimity is *no accepted value differed from `c.own`*, which is the same re-derivation `Q4-e` gives T49 and T50 and the third guard the old record could not compute | unchanged | `c.acked ∪= {e.seat}`; **and when `c.acked ⊇ c.required` the `STATE_ACK` *stage* is complete: `c.agreed := Some(e.checkpoint_hash)`, written on the record `c` and into no slot** — which slot `c` sits in is §2.6's business and this row does not move it. **Two things here are `P1` and both were defects rather than refinements.** (a) `agreed` is set when the **ACK stage** completes and not on the first ack, which is what §2.6's own field comment always said and what D-014's tier-2 precondition — *a completed `STATE_ACK` stage* — actually asks for; there was no `acked` set, so the completed stage was not a representable object at all. (b) The checkpoint is selected by name, so a checkpoint-8 `STATE_ACK` of hand `k` arriving after hand `k+1` has started **can** set `agreed` — which is precisely what `PROTOCOL.md` §4.9's `N6` box accepts *"until `TERMINAL(k+1)` is fixed"* and what the single slot made impossible, leaving D-014 tier 2 unsatisfiable at every hand boundary |
| T52 | `Diverged` | `Dispute` | — | `Diverged` | record the declaration and its `at_sequence`; still frozen |
| T53 | `Diverged` | `StateHash` | `round >= 1` ∧ the reconciliation-round stage for the disputed checkpoint is complete over its required signer set ∧ **the completed stage carries values signed by at least two distinct seats** ∧ every value in it agrees | the phase held at the checkpoint | resume; nothing was opened and no chips moved while frozen; the hand deadline keeps running; **`solitary_contradicted := false`** — this row is the only thing in the document that clears it (§2.6, §9.3 condition 0.6) |
| T54 | `Diverged` | `StateHash` | `round >= 1` ∧ the reconciliation-round stage is complete ∧ two distinct `state_hash` values remain in it ∧ **every `transcript_head` in it is equal** — `PROTOCOL.md` §6.3 case (c), byte-identical transcripts and still-different derived states | `HandAborted` | `Fault{StateDivergence}` (already present from T50); `AbortRecord{kind: StateDivergence, attributed: []}` (`cause = 4`); **`table_faulted := true`** — `PROTOCOL.md` §6.4 closes the table, and §9.3 condition 0.5 is where that is applied; **restoration** (§8.6, I27) |
| T60 | `Diverged` | `StateHash` | `round >= 1` ∧ the reconciliation-round stage is complete ∧ two distinct `state_hash` values remain in it ∧ **two distinct `transcript_head` values remain in it** — `PROTOCOL.md` §6.3 case (b), a peer is missing events it cannot obtain | `HandAborted` | `Fault{StateDivergence}` (already present from T50); `AbortRecord{kind: UnobtainableEvents, attributed: []}` (`cause = 1`, `cert_hash = None`); **`table_faulted` unchanged** — case (b) does **not** fault the table; **restoration** (§8.6, I27) |
| T62 | any phase except `TableClosed` | `SolitaryDivergence` | **`solitary_at(e.hand_id)`** (§2.6) — the monotone floor, under `PROTOCOL.md` §4.0 step 10b's retained record, which is what decides that the hand the event names *was* dealt with `\|P(k-1)\| == 1` and what makes the event exist at all; §2.6's lemma is that the floor never rejects a hand the record admits, and I33(a) asserts it (N4). **The test is in the past tense and that is the whole of it**: the event's `hand_id`, never the receiver's current phase or current hand (K-9; `PROTOCOL.md` §4.0's staleness step is what delivers it) | **`Diverged`** | freeze, **identically to T50** and by the same reading of `PROTOCOL.md` §6.3 step 1: no `RequestOpen`, no `ArmDeadline`, no chip movement, no stage completed, no pot awarded, no §9.3 end condition evaluated; **the hand deadline is not disarmed**, so T57 and T61 keep their scope; `Fault{SolitaryDivergence}` recording `e.seat` and `e.event_hash` as the evidence; **`solitary_contradicted := true`**. The offending event itself is **not applied and not counted into `signed_this_hand`** (`PROTOCOL.md` §4.0 step 12a), so `P` does not grow from it |
| T63 | `TableClosed` | `SolitaryDivergence` | same guard as T62 | `TableClosed` — **unchanged, and this is the one event class for which this phase is not absorbing** | `Fault{SolitaryDivergence}`; **`solitary_contradicted := true`**; **`settlement.tournament_winner := None`** — the result is *retracted*, because the only thing this peer computed after the contradiction it had not yet received was a tournament won against a seat that was signing against it. No chip moves, no phase changes, nothing reopens, and **`TableClosed` is still absorbing for every other event type** — I13's cell is where that hole is named |
| T64 | any phase in which a hand is live — 4–15, and 20 when a hand is live | `CheatProven` | `hand_id > 0` ∧ (`tier == SelfContained` ∨ (`tier == StateDependent` ∧ `judged_at_checkpoint == Some(n)` ∧ **`agreed_checkpoint(hand_id)` is `Some(c)` with `c.number >= n`** — §2.6's three-slot accessor, and the change from *"checkpoint `n` of this hand reached `agreed.is_some()`"* is `P1`: checkpoints of one hand supersede one another in the store, so the exact-`n` form was unreadable for every `n` below the live checkpoint and this guard could not be discharged at all. `c.number >= n` is **not** a weakening, because `transcript_head` is in `PublicTableState` (`PROTOCOL.md` §6.1) and chains through every earlier stage, so an agreed checkpoint at `n' >= n` of the same hand is agreement over a prefix that contains checkpoint `n`'s stage. `c.required` is what carries §4.9's *"the emitter set contained both the accused and this receiver"*, and **`src/security/validation.rs` witnesses that clause and this row's `c.number >= n` clause as well** — `AgreedCheckpoint` carries `emitters`, `hand_id` and `number`, `covering` is its only constructor and refuses with `AccusedNotAnEmitter` and `ReceiverNotAnEmitter`, and `Tier2Finding::new` refuses with `CheckpointTooEarly` unless `against.number() >= fixed_at_checkpoint`. So the whole tier-2 precondition is **enforced by construction rather than described**, and a caller holding only a local view has nothing to pass. The sentence that stood here said the type *"cannot witness"* the emitter clause; it was false when `P6` landed and is false twice over now that the position half has landed as well (`G6-R3`; the position half was `Q2`, and `Q2` is closed by the same type; and since `G7-S8` `PROTOCOL.md` §4.10's tier-2 row states the precondition in the **same unit as this guard** — it read *at or before the offending event's `sequence`*, which neither this guard nor `src/security/validation.rs` evaluates, so `CheckpointState.sequence` is kept for §4.1's `round` derivation and for nothing else))) ∧ `status[subject] != Removed` | `HandAborted` | **the hand is voided, neutrally and by the existing mechanism**: `AbortRecord{kind: ProvenCheat, attributed: [subject], owed: [], observed_by: ∅}`, restoration follows at T46 (§8.6, I27), so no chip crosses between seats; `Fault{ProvenCheat}` carrying `tier` and `evidence_hash`; **`status[subject] := Removed`**; `dealt_in[subject] := false`; `deck.participants -= {subject}`; `signed_this_hand -= {subject}`; if `player_to_act == Some(subject)` then `None`. **The information window is driven by the `Fault` record and needs no new effect** — and it is a **required addition to `SPEC_CS.md` §22, not an element §22 contains**: §22's GUI tree ends at *protocol/security status* and lists no anti-cheat window, so this row records the requirement and does not cite it as existing (N8; `THREAT_MODEL.md` §9.2 and `DECISIONS.md`'s open list carry it, and the spec is the owner's document and is not edited from here). What the engine owes is unchanged either way: `Effect::Fault(FaultRecord)` is already declared *"for the reputation counter and the GUI"* (§4.2), and the record carries the subject, the tier and the evidence hash, which is exactly the three things D-014 says the window must name. **The window must also say the hand was voided and that no chips changed hands** (D-014), and that is not a claim the GUI makes on its own — it is I27, asserted after the T46 this row leads to, so nobody reads a void as a loss |
| T65 | `HandComplete` \| `Paused` \| `Diverged` when no hand is live | `CheatProven` | as T64 | unchanged | the same removal side effects as T64 **minus the abort**: there is no live hand to void, so no `AbortRecord` and no restoration — `Fault{ProvenCheat}`, `status[subject] := Removed`, the four set removals, and the same `Effect::Fault` the window is built from. The seat is skipped from the next hand init onward by §5.3 step 4, which already reads `status == Active`. **The window here says the removal and *not* a void**, because there was no hand to void — a removal at a boundary costs the table nothing at all |
| T66 | `Seating` \| `AwaitingSeatRngCommit` \| `AwaitingSeatRngReveal` | `CheatProven` | `hand_id == 0` | unchanged | `Fault{ProvenCheat}` and **nothing else — no seat is removed** (see the box below). The beacon stalls to **T4** exactly as it did before D-014, no chips exist to conserve (`ledger_in == 0`), and the table never starts |
| T67 | any phase except `TableClosed` | `Readmitted` | `hand_id > 0` ∧ the seat is occupied ∧ `status[e.seat] ∉ {Removed, Empty}` | **unchanged** | `readmit ∪= {e.seat}` and **nothing else**. **This is `N-5e`, as `P2-e` reshapes it, and it is deliberately the smallest row in the table**: it moves no chip, changes no phase, completes no stage, evaluates no end condition, and the set it writes is read at exactly one place, §5.3 step 4, and cleared at step 8. **Since `P2` the set it writes reaches no guard at all** — not `dealt_in`, not `solitary_since`, not §9.3 — because `A` widens hand `m+1`'s **accepted** emitter set and never its required one; §5.3 step 4 hands the set to the protocol layer and derives nothing from it. That is what makes this row's replay behaviour inert: a re-sent stale event writes a seat that is already in the set, and the set now has no effect that repeats. The `Removed` conjunct is D-014's one-way exit (I34) — a set a removed seat could re-enter by any route is the re-entry T59 was closed to make impossible, and `PROTOCOL.md` §4.9's `A` is such a route unless this guard excludes it, which is the one thing this row adds to §4.9's rule rather than restating it. The phase exclusion is `TableClosed` and only that: a readmission after the table has closed readmits nobody to anything, and the phase's one non-absorbing event is T63's, which is a retraction and not an admission |

> **The solitary regime in the engine — normative, and this is the engine half of `PROTOCOL.md`
> §3.2 (K-9).** `PROTOCOL.md` owns the wire half, the required emitter set `P(k)` and its
> notation; this section owns the phase, the transition and what leaves it, and restates neither
> (D-011 rule 1).
>
> **Which peer, and which hand.** This peer is in the **solitary regime** for hand `k` when the
> value of `signed_this_hand` that §5.3 step 4 read at hand `k`'s init — `P(k-1)`, the required
> emitter set of every collective stage of hand `k` (`PROTOCOL.md` §4.4) — has exactly one
> member. It is entered at hand init, and the **regime** is left at the first hand init at which
> `|signed_this_hand| > 1`, which inside the regime follows an accepted `0x0804 PLAYER_SIT_IN`, or
> the **accepted `HAND_INIT` copy** of a seat §4.9's readmission set carried back in — one hand
> after the readmission, because that copy is what puts the seat into `P` (`P2-e`).
>
> **The set the regime is read off is the *required* emitter set and never the accepted one, and
> that is `P2-e` and the correction this pass makes to this box.** The question the regime asks is
> *"whose agreement am I comparing my state against"*, and since `P2` the answer is `P(k-1)`: a
> stage whose required set is `{self}` completes at this peer alone whatever its accepted set is,
> so a peer with a non-empty `A` compares its state against nothing and is in the regime. The
> sentence that stood here read the regime off `P(k-1) ∪ A` and gave the opposite answer for
> exactly the peer K-1 is about — a fork in which each side has the other's stale, *agreeing*
> checkpoint-8 `STATE_HASH` in its `A` sees `|admitted| == 2`, records no regime, and **T62 and
> T63 never fire on either side.** The union is deleted from the read at §5.3 step 8.
>
> **Leaving the regime does not erase the record, and the distinction is N4.** `solitary_since`
> is the first hand this peer *ever* dealt solitary and is never cleared (§2.6): a regime that is
> left can be re-entered — the seat that sat in can fall silent again — so the solitary hands are
> a union of intervals, and a field that reset on the way out lost the earlier episode while
> `PROTOCOL.md` §4.0 step 10b's retained record kept it. Which hands were solitary is that
> record's answer; `solitary_at(k)` is this document's floor beneath it, and §2.6's lemma is that
> the floor never rejects a hand the record admits.
>
> **Why `|P(k-1)| == 1` is the test and not "`P(k-1) == {me}`", which is what §3.2 says.** `step`
> is pure over `TableState`, and `TableState` has no `me`: the seat index of the local peer lives
> in `LocalView` (§2.9), which `step` may not read, and reading it would break I22 and make every
> peer's derivation of its own state a different function. The two forms are nevertheless the same
> predicate here, and the reason is K-1's answered sub-question: **a peer's own emission counts
> into its own `signed_this_hand`**, and this peer emits its own `HAND_INIT` copy at every hand
> init (T47 and T61 both publish it), so `self ∈ signed_this_hand` from the first step of every
> hand. A one-member set therefore has the local seat as its member, necessarily. That is the
> whole reason a rule written about *self* is expressible in a state type that has no self, and it
> is worth checking rather than believing: if a future edit ever lets a peer skip its own
> `HAND_INIT` copy, this guard silently starts naming somebody else and T62 fires on the wrong
> hands.
>
> **What the freeze is.** `Diverged`, on **T62**, with the T50 body: nothing opens, nothing
> applies, no chip moves, no collective stage **of the hand** completes, no pot is awarded and
> §9.3 is not evaluated. **The reconciliation exchange is not frozen and never was** — T52
> records a `DISPUTE` from this phase, and §6.3 steps 2 and 3 are what the frozen peer is
> waiting *for*; I33(b) states the exemption and why a freeze that suppressed it would have no
> reachable repair. **That is the whole point**, and it is the property §3.2 buys with it — a peer that
> has narrowed to one member may not finish a tournament in silence while another seat is signing
> against it.
>
> **What releases it, and there are exactly four exits.** (1) **T53** — the reconciliation stage
> for the last checkpoint completes carrying one value **and carrying signatures from at least two
> distinct seats**: the divergence was real and is repaired, play resumes at the phase held, and
> T53 **clears `solitary_contradicted`**. It is the only thing that clears it. Note that this exit
> exists *because of K-3*: a solitary hand now places checkpoint 8 (T45, T46), so there is a stage
> for the reconciliation round of `PROTOCOL.md` §4.9 to chain from. Before K-3 there was none, and
> §3.2 and §6.3 still say so — *"There is no checkpoint on that path to compare hashes at"* — which
> is stale by one pass and is filed as a cross-owner item. (2) **T54** — case (c), which already
> faults the table. (3) **T60** — case (b). (4) **T57** or **T61**, the timers, if nothing
> reconciles. Exits 2, 3 and 4 all reach `HandComplete` through T46, where **§9.3 condition 0.6**
> reads the latch and closes the table.
>
> **The two-signer conjunct on exit 1 is N1, and without it the freeze releases itself.** A
> reconciliation round's required emitter set is the emitter set of the checkpoint it re-derives
> (`PROTOCOL.md` §4.9), and at a **solitary** peer that set is `{self}`: the peer publishes one
> re-derived value, the stage is complete over its required set, every value in it agrees
> vacuously because there is only one, and T53 fires. The freeze is lifted, the latch is cleared,
> the peer deals another solitary hand, the next contradicting event freezes it again, and the
> table oscillates for as long as the fork lasts — **J2's fixed point reached through the release
> path instead of through the timer**, which is exactly the shape the latch was installed to close
> and which the latch cannot see, because this loop clears it every time round. The conjunct is
> written on the **signers of the completed stage** rather than on the size of the required set,
> deliberately: whatever `PROTOCOL.md` settles for that set, a stage the frozen peer can satisfy
> **by itself** must not release the freeze, and a signer count is the one form of the test that
> does not move when the set's definition does.
>
> **`N1-a` is chosen here, because an implementer writing T53 has to choose and the gate says so:
> the reconciliation round admits an out-of-set copy, exactly as round 0 does.** The question is
> whether `PROTOCOL.md` §4.9's checkpoint-8 admission — *"accepted, compared and retained from any
> occupied roster seat"* — extends to the **reconciliation rounds** of that checkpoint, `sequence`
> `BOUNDARY_CHECKPOINT_BASE + 2r`. `DECISIONS.md` records both answers as safe and names the state that is not: the
> exit described and unreachable. **The choice is *admit*, and the reason is that the round is the
> same stage.** §4.9 admits the out-of-set copy at round 0 because *"its body is a claim about who
> the participants are"*, and a reconciliation round re-derives that same body over that same
> checkpoint; the reason does not weaken at `r = 1`, so refusing there would be two rules for one
> stage, separated by nothing but a `sequence` offset. What it buys is that **exit 1 exists in the
> case the freeze exists for**: at a solitary peer the second signature can only come from outside
> `P`, so under the other answer T53 is unreachable exactly where T62 fires, and every solitary
> freeze ends at T54, T60, T57 or T61 and therefore at `TableClosed` through §9.3 condition 0.6 —
> a table closed on a fork that could have been repaired. **What it does not buy, and this is why
> admitting costs nothing:** the admission alone releases nothing. T53 additionally requires the
> round's stage to be **complete**, to carry **two distinct signers**, and to be **unanimous**, and
> `A` — which is what carries the out-of-set seat into the next hand — cannot thaw a freeze either
> (§4.9, §5.3 step 4). So the widened acceptance can only ever supply the second signature that
> I33(b)'s exemption already assumes exists; it cannot manufacture agreement. **The engine is
> written for this answer and states it rather than tolerating both**, because *"both answers are
> safe"* was true of the engine's rows and not of the reader: an implementer who assumed the other
> answer would write T53 and never generate the trace that exercises it. The sentence is
> `PROTOCOL.md` §4.9's to write (D-011 rule 1) and is recorded on `DECISIONS.md`'s open list
> against that section with this choice named.
>
> **It costs nothing anywhere else, and that is checkable rather than a hope.** Where the
> reconciliation set has two or more members the stage cannot complete without two signers, so the
> conjunct is implied and inert; it binds only where the set is a singleton, which is only ever the
> solitary case. **T54 and T60 need no such conjunct** and this is the check that says why: both
> require *two distinct `state_hash` values to remain in the completed stage*, and one signer fills
> one slot with one value, so neither can fire on a stage one peer completed alone. T53 was the
> only exit a frozen peer could satisfy against itself, which is why it was the one that had to
> move.
>
> **What the conjunct leaves owed to `PROTOCOL.md`, stated rather than assumed.** A second signer
> can only appear in that stage if a reconciliation-round `STATE_HASH` from a seat **outside** the
> frozen peer's `P` is admissible — the same widening §4.9's checkpoint-8 box already makes for the
> checkpoint itself (*"accepted, compared and retained from any occupied roster seat"*). If §4.9
> extends that admission to the rounds, exit 1 is reachable and the fork can genuinely repair; if
> it does not, exit 1 is unreachable in the solitary case and every solitary freeze ends at exit 2,
> 3 or 4 and therefore, through condition 0.6, at `TableClosed`. **Both dispositions are safe and
> the engine is written for either**; which one holds is `PROTOCOL.md`'s to say, and it is recorded
> in `DECISIONS.md`'s open list rather than decided here (D-011 rule 1, D-013's process rule).
>
> **Why the latch, and it is the check this fix owes rather than an extra.** It is set by T62, by
> T63 and — since N1 — by **T50 whenever `|checkpoint.required| == 1`**, because a checkpoint
> whose required emitter set is a single seat is a comparison that peer conducted alone, and a
> mismatch in it is the same contradiction T62 consumes, reported by a different message. Exits 3
> and 4 leave the table playable by design — case (b) *"costs the table a hand, not the table"* (§9.3
> condition 0.5's note), and a hand-deadline abort never faults anything. Applied to a solitary
> divergence that is wrong, and wrong in the shape this document has now been caught by four
> times: the table plays on, deals another solitary hand, meets the next contradicting event,
> freezes again, times out again, **every `hand_deadline_ms`, forever**. Every phase would have an
> exit, every hand would end, and the table would make no progress — J2's fixed point exactly, on
> the path K-1's fix newly made load-bearing. The latch is what makes the freeze terminal unless
> something actually reconciles, and `SPEC_CS.md` §19's ranking is what licenses it: *"Security má
> přednost před pohodlným dokončením handy."*
>
> **What it does not do.** It does not touch the drain. A peer that is genuinely alone receives no
> contradicting event, T62 never fires, and §12.1.2's forty drain hands and their ending are
> unchanged — which is the property §3.2 insists on, because deleting the drain is what the
> rejected alternative did.
>
> **K-1's two interleavings, re-walked against the corrected floor (`N-1e`).** Three seats, `A`
> (this peer), `B`, `C`. Hand `k-1` completes, `P(k-1) = {A,B,C}`. `C` emits `PLAYER_LEAVE` in the
> boundary window; `A` accepts it, `B` does not. Hand `k` stalls at stage 0 because `A` and `B`
> derive different `n(8) dealt_in`, aborts at T57, and T46 opens checkpoint 8 for hand `k` with
> `required = P(k)` — `{A}` at `A` and `{B}` at `B`, since `PLAYER_LEAVE` enters no `P` (§5.3
> step 4(ii)) and the terminal `HAND_ABORT` enters none either (I31(c)). `A`'s `HAND_INIT(k+1)`
> self-completes, and §5.3 step 8 writes **`solitary_since := Some(k+1)`** — by the **second**
> disjunct, because `P(k-1)` had three members and `P(k)` has one. `PROTOCOL.md` §4.0 step 10b's
> retained record for hand `k` says `was_solitary`.
>
> * **Variant 1 — `B`'s checkpoint-8 `STATE_HASH` for hand `k` arrives first.** It is stale, so
>   step 10a is skipped and step 10b compares it against the retained
>   `checkpoint8_state_hash(k)`; it **differs**, and the record says solitary, so step 12a emits
>   `SolitaryDivergence { hand_id: k }`. **T62's guard is now `Some(k+1) ∧ k+1 <= k+1`, which
>   holds.** The freeze fires **on the fork hand**, on the first contradicting event, and the latch
>   is set. Under the old floor `k+1 <= k` was false, the event was dropped with no `Fault` record,
>   and the freeze waited for `B`'s `HAND_INIT(k+1)` copy — **one hand late, and only if that copy
>   ever arrived**.
> * **Variant 2 — `B`'s `HAND_INIT(k+1)` copy is dropped and the checkpoint-8 copy is the only
>   contradiction `A` ever receives.** Identical: step 10b, step 12a, `SolitaryDivergence
>   { hand_id: k }`, T62 fires, latch set, §9.3 condition 0.6 closes the table at the next boundary.
>   **Under the old floor this variant detected nothing at all** — `A` drained to §9.3 condition 1
>   and entered `TableClosed` naming itself, and a copy arriving after that missed T63 on the same
>   guard, so `settlement.tournament_winner` was never retracted. That is K-1's payoff surviving on
>   the one hand §3.2 says the evidence is strongest for, and it is what the one character bought.
>
> **And the same walk exercises `P1` on its second face, which is worth noticing because the two
> fixes meet here.** Variant 1's contradicting copy is also a checkpoint-8 `STATE_HASH` that names
> a checkpoint `A` still holds — hand `k`'s, in `checkpoints.boundary`. Had `A` been at a table
> where it was *not* alone, `SolitaryDivergence` would not have been produced and **T50** on that
> retained record is the row that catches it; before this pass there was no record to catch it
> with, and no row either.

> **D-014's removal, and the two rulings it looks like it reopens but does not.**
>
> **It does not reopen G4 or P4.** T11 stopped unseating a seat whose RNG opening does not match
> its commitment, and T8 and T12 were deleted, both under D-010 point 3. A commitment mismatch is
> *exactly* D-014 tier 1 — two messages the accused signed, decidable from those messages alone —
> so the narrowing D-014 makes would put the unseating back. **T66 is the row that refuses it, and
> the reason is not D-010 at all**: `PROTOCOL.md` §3.1 freezes the seat vector of `roster_hash(k)`
> at `TABLE_READY` for the life of the table (K-2), the beacon is in the setup chain **after**
> `TABLE_READY`, and `GENESIS(k)` is a function of `roster_hash(k)`. Removing a seat there does not
> punish anybody — it **forks the genesis**. The cost of refusing is nil: no `HAND_INIT` has
> happened, `ledger_in` is `0`, no card exists, and T4 closes the table with nobody named (§12.1
> row 1). D-014 buys nothing before hand 1 because there is nothing yet to protect.
>
> **It does not reopen D-010, and the boundary is the evidence and not the severity.** No removal
> on a timeout, on a missing publication, on an `EquivocationProof`, on `attributed`, on a vote or
> on a certificate — D-014 says so and this table obeys it: every one of T15, T16, T21, T22, T27,
> T34, T41, T44, T48, T54, T57, T60 and T61 keeps exactly the disposition it had, and **not one of
> them gains a removal**. `CheatProven` is a separate event with a separate row, and if an
> implementation ever derives it from the trigger of any of those rows it has rebuilt D-010's
> forfeiture with a new name.
>
> **Why the offender's chips stay on the table.** D-014 point 3: the seat is dead and the stack is
> blinded off until it is gone. That is not leniency, it is **I1**. `ledger_in − ledger_out` is the
> right-hand side of the ledger identity, `ledger_out` moves only in hand init step 0 and only for
> a seat that announced `PLAYER_LEAVE` (I28), and taking a removed seat's stack off the table would
> move the left-hand side without moving the right. So the removal moves **no chips at all** — T64
> and T65 are the only rows in this table that change a `status` and move nothing — and the stack
> drains through steps 6 and 7 exactly as a silent seat's does, busts at step 2 in a bounded number
> of hands, and §9.3 condition 1 fires normally (§12.1.1's decreasing measure, unchanged).
>
> **Why the offender leaves every set at once, and what it costs.** Under **D-013** the offender
> would drop out of `P` anyway the moment it stopped signing, and out of `dealt_in` at the boundary
> after that — one or two hands (§12.1.2). T64 takes **the same exit, at once and for cause**, so
> nothing new is being invented about what a non-participating seat is; what is new is only the
> *timing* and the fact that it is not reversible. The cost is one hand voided, which is what an
> abort already costs.

**T53 and T54 are triggered by an event, not by a phrase.** Their Trigger column previously read
"transcript reconciliation completes, derived states now agree / still differ", which is a
condition and not an input: an engine is a function of `(state, event)`, and no event in §4.1 said
"reconciliation completed". The trigger is now the completion of the **reconciliation-round
`STATE_HASH` stage** that `PROTOCOL.md` §4.9 defines normatively. That stage's shape, its
`sequence`, its emitter set and why it is not a second copy of the checkpoint are §4.9's to state
and are **not restated here** (D-011 rule 1): the engine consumes what §4.9 produces. §4.1 gives
the one thing that is this document's — the derivation `round := sequence − s_ckpt` — and the rows
are scoped on it. T53 fires when the round-`r` stage completes, **carries values signed by at least two distinct
seats**, and every value in it agrees; T54 and T60 when it completes and two distinct values remain.
All three are chain content, so every peer reaches the same verdict from the same events. **The
two-signer conjunct on T53 is N1 and it is the only one of the three that needs stating**, because
it is the only one a peer can satisfy alone: T54 and T60 both require *two distinct values to
remain* in the completed stage, and one signer fills one slot with one value, so a stage completed
by a solitary peer against itself cannot fire either of them. Before the conjunct it could fire T53
— agreeing with itself, releasing the freeze and clearing the latch — which is the release path
§9.3 condition 0.6 does not see. T49 and T50 are scoped to exclude `Diverged`,
and to `round == 0`, precisely so these rows own the reconciliation rounds.

**T60 is new, and it is the transition `PROTOCOL.md` §6.3 case (b) asks for.** §6.3 now
distinguishes two unresolved terminuses and gives them different wire bodies: case (b) — one peer
is missing events every holder refuses to serve — aborts with `cause = 1`, `attributed = []`,
`cert_hash = None`, and **the table is not faulted**; case (c) — byte-identical transcripts,
still-different derived states — aborts with `cause = 4` and the table **is** faulted (§6.4). Both
were previously T54, which produced `cause = 4` for both and faulted the table on a case that §6.3
says must not fault it. Two outcomes need two rows, because I13 reads edges and their side effects,
not prose.

**How the engine tells them apart, and why it needs no new signal.** §6.3 step 3 says the engine
needs no signal beyond the completed reconciliation stage, and it does not: `PROTOCOL.md` §4.9's
`STATE_HASH` payload carries `n(2) transcript_head` beside `n(1) state_hash`. Case (b) is exactly
the case in which the transcripts still differ after reconciliation, so the round's
`transcript_head` values differ; case (c) is exactly the case in which they are byte-identical, so
they agree. Both quantities are in the same completed collective stage, so every peer holding that
stage classifies identically — which is the property T54 needed and the reason the classification
can live here rather than being asserted by whichever peer speaks first. Nothing is derived from
what any single peer *says* about the transcripts; it is derived from what they all signed.

**The objection that stood here is closed, and the shape it warned against must stay closed.**
Two revisions ago these rows were specified against `PROTOCOL.md` §6.3 step 3's *"Play resumes
from the checkpoint with no further action"* — no wire event, hence no trigger. §6.3 step 3 has
been rewritten and §4.9 now carries the reconciliation round in a normative box, so the artefact
exists and §13 no longer carries the request. What must not come back, in this document or in any
other, is the shape P1 named: each peer *re-emitting* its own `STATE_HASH` for the disputed
checkpoint. That is a second, different body in a capacity-one slot and it manufactures a verifying
`EquivocationProof` against every honest peer that reconciled — D-009 rule 1's fourth recurrence.
The predicate, the slot key and the reason live in `PROTOCOL.md` §5.2 and §4.9; this document
points at them and checks its own emissions against them (§4.1).

**T55 and T56 are deleted, and nothing in this document consumes an `EquivocationProof` (G2).**
`PROTOCOL.md` §5.2.4 states the disposition in a normative box that is canonical for the corpus:
*"**Nothing consumes it.** There is no transition, in any document, that takes an
`EquivocationProof` as its input event. It ends no hand, moves no chip, unseats nobody,
block-lists nobody, refuses nobody a seat, and produces no `AbortRecord`."* `PROTOCOL.md` owns the
wire and therefore owns what a message may cause; this document owns the transitions and follows
it (D-011 rule 1). The two rows go, `Event::EquivocationProof` goes with them (§4.1),
`AbortKind::Equivocation` goes with that (§8.6), **`Fault{Equivocation}` becomes unreachable and is
removed from the fault alphabet** — the same disposition P4 gave `Fault{NoRngCommit}` and
`Fault{NoRngReveal}`, and an implementer should not write the arm that raises it — and the count
falls to 55 before T60 brings it back to 56 (§5.1).

**Three separate things were wrong with the rows, and each on its own was blocking.** They are
listed because a reader checking this edit should be able to check it against the gate rather than
against a summary:

1. **T55 ended a hand on an unchained event.** An `EquivocationProof` reaches a peer inside a
   `DISPUTE`, which is `chain_scope = 0`, and `PROTOCOL.md` §5.2's ordering rule forbids such an
   event terminating a stage or ending a hand. The failure is concrete and is P2's: two honest
   peers, one applying a certificate first and one applying a proof first, derive two different
   terminal bodies and two different post-hand states, with the attacker supplying only the
   delivery order.
2. **`AbortKind::Equivocation` had no wire representation.** §4.10's `cause` enumeration is `1`–`4`
   and `5` is deleted and not reused, so T55 produced an `AbortRecord` no legal `HAND_ABORT` could
   carry. That is the C-6 class — an engine alphabet containing an outcome no message produces — in
   its sixth appearance in this document, after `RevealRejected`, `StateAck`,
   `TimeoutCertificate{Join}`, `Fault{NoRngCommit}` and `Fault{NoRngReveal}`.
3. **Both rows asserted a transport consequence that does not exist.** "the accused is blocked at
   the protocol layer (`PROTOCOL.md` §5.2)" was false when it was written and is false now: §5.2.4
   says block-lists nobody, D-010 point 3 forbids the eviction, and D-011 rule 3 extends the ban to
   every layer including `NETWORK_STACK.md`. Every sentence of that shape is deleted from this
   document; §8.6 and §12 carried the same claim and no longer do.

**What an equivocation costs now, so that "nothing consumes it" is not read as "nothing happens".**
The two conflicting copies occupy one stage cell, so whichever arrives second at a given peer is
rejected under `PROTOCOL.md` §4.0; peers that accepted different first copies hold different state;
the next checkpoint shows two `state_hash` values; T50 takes the table to `Diverged`; and the hand
ends at T53, T54, T60 or T57 like any other divergence. The equivocator therefore still loses the
hand it corrupted — it just loses it through the chained path, which every peer can place in the
total order, instead of through a shortcut that only some peers could. And if no peer's state ever
disagreed, the equivocation cost nobody anything, which is the honest description of that case.

**What is genuinely given up.** A proof that arrives when nothing has diverged now changes no
engine state at all. Under the deleted rows it produced a `Fault{Equivocation}` record in
`TableState`. It is still verified, still broadcast in a `DISPUTE`, still retained beside the
transcript forever, and still shown to the user — all at the protocol layer (`PROTOCOL.md` §5.2.4,
§3.3). What it no longer does is enter canonical state, and that is correct rather than
regrettable: a value that only some peers hold must not enter a hash every peer compares, and
whether a proof reached a given peer is exactly such a value.

**The one thing this deletion must not be read as retiring is D-009 rule 1.** The property — *no
sequence of emissions the protocol requires or permits of an honest peer may produce a verifying
proof against it* — is `PROTOCOL.md` §5.2.3's, it is stated there as canonical, and this document
points at it and restates none of it. It still binds every emission specified here, and it is still
what a false accusation would violate: a proof naming an honest key is a lie in a permanent public
record whether or not any transition reads it. The standing test is unchanged and is the mirror of
`SPEC_CS.md` §25's `CheaterEquivocation` — *an honest peer never generates a proof against itself
under any legal interleaving* — which lives in the adversarial suite, not in a document. Every
chained emission this document specifies is checked against §5.2.1's key before it is added; that
check is what rejected P1's shape, and `PROTOCOL.md` §4.11's per-type table rows 33, 34 and 36 are
where the results are recorded.

**There is no peer-removal transition, and no unanimity-minus-one guard anywhere in this
document.** That is now a decision rather than a judgement call: **D-010 point 3 forbids the
protocol from unseating, block-listing or penalising any peer on the strength of a proof**, so no
such transition may be added later without a new numbered decision. An earlier draft of
`PROTOCOL.md` §6.3 resolved case (c) — byte-identical transcripts,
still-different derived states — by removing the one peer that disagreed with the other `n-1`.
That rule is deleted (A-6): no signature distinguishes an implementation bug from a client lying
about its own derived state, there is no observer-independent derivation at run time, and
"attribute whoever disagrees with the derivation" is a vote over the facts, which `SPEC_CS.md` §15
forbids. It also let `n-1` colluders take an honest player's committed chips, available already at
`n = 3` with two colluders. T54 is therefore the only outcome of case (c) at **every** `n`: abort,
no attribution, restoration, table faulted. One liar can fault any table; that is a liveness and
griefing cost, accepted in preference to an integrity cost, and `THREAT_MODEL.md` X29 carries it.

The evidence — the unanimous transcript plus every peer's signed `STATE_HASH` — is written to the
profile directory, where **it is preserved and is sufficient for a human, or for a future
adjudicator, to diagnose the divergence; no adjudication procedure is specified.** Live
adjudication is not available and is not claimed; neither is offline adjudication. The chip
disposition is settled — restoration, D-010 — and what remains open is the adjudication procedure,
which is **OQ-A** under the letters `DECISIONS.md` owns and `PROTOCOL.md` §12 has adopted (R-2);
§11's closed table records why the chip question is no longer open, and §11's note on the label
change.

**A stronger claim stood here and is withdrawn (N2), in the same form and for the same reason as
`PROTOCOL.md` §6.4 withdrew it.** It read: *"is publicly adjudicable **offline** by anyone who runs
the reference engine over it."* **There is no reference engine.** No document in this corpus
defines one, versions one against `protocol_version`, or says how two parties establish that they
ran the same one — and this document is the one the sentence appealed to. What it specifies is
*the* deterministic engine as a thing every client implements, which is exactly the plurality
§6.3 case (c) exists to describe, not a normative artefact a third party runs; §1 of this document
defers even the source layout to a `docs/ARCHITECTURE.md` that does not exist until Phase 2, so
there is not even a named binary. Moving the derivation
offline does not create the observer-independent reference that T54's own reasoning establishes
does not exist; it only changes when the disagreement happens. Defining that reference is **OQ-A**
(`DECISIONS.md`'s open list; `PROTOCOL.md` §12 adopts the letter), and until it is defined the artefact supports diagnosis, not adjudication —
`SPEC_CS.md` §36 forbids the stronger word.

`Paused` and `TableClosed`:

* `Paused` arms no deadline and moves no chips. It is left by T59 (`PlayerSitsIn`, when §5.3
  step 4's own predicate would deal two or more seats in — K-7, the note under T58/T59), which runs
  hand init and therefore also applies any seat marked `Leaving` by T58. **A `Paused` table that every seat has
  left stays `Paused`**, and the previous revision's claim that it "closes under §9.3" is
  **withdrawn**: §9.3's conditions are evaluated at T47 and T47 does not run from `Paused`, so
  nothing evaluates condition 0. That is idle rather than frozen — no hand is live, nothing is
  committed, no deadline is armed and no peer is owed anything — so §12.1's walk is unaffected;
  what it costs is a `TableState` that never reaches `TableClosed` and a `ledger_out` that never
  records the departed stacks. **Named default:** the engine leaves it there and the client closes
  the window. Adding a `Paused × PlayerLeft → TableClosed` row would move `ledger_out` outside hand
  init step 0, which I28 and `PROTOCOL.md` §4.4 both forbid, so the smaller answer is to say what
  happens rather than to add the row.
* `TableClosed` is absorbing. Every event is a `Rejection`: there is no state left to change and no
  chips left to move. Evidence still accumulates beside the transcript at the protocol layer, which
  is where retention lives (`PROTOCOL.md` §3.3, §5.2.4), so nothing is lost by the engine declining
  it. `SPEC_CS.md` §4 and D-006 §5 both forbid a timeout from producing this state during play;
  only a won tournament, a **faulted table** (§9.3 condition 0.5, from T54), an unstarted table
  (T4) or every seat leaving reaches it.

### 5.3 Hand init (the procedure entered on T10 and T47)

Pure, no events, no clock. In this exact order:

0. **Apply the seat exits announced since the previous hand init.** Every seat with
   `status == Leaving` (T58) becomes `Empty`: `ledger_out += stack`, then `stack := 0` and
   `player := None`. **There is no seat-entry clause here (P8).** `ledger_in` moves exactly once
   per table, at the first hand init (T10), where it is the sum of the buy-ins of the seats that
   `Seating` accepted through T1/T2; **no seat may be added after `Seating` in this version**, in
   either mode — see §9.4 and Q6 in §11. This is therefore the **only** place either ledger counter
   changes, and the per-seat vector of changes is exactly `PROTOCOL.md` §4.4's `n(11) ledger_delta`
   in the derived `HAND_INIT` — positive for the initial buy-ins at hand 1, negative for a departing
   stack thereafter, recomputed and checked by every receiver. I1 holds across the step because both
   sides move by the same amount; I28 holds because this step runs only here.

   The clause deleted here read *"Every accepted seat entry adds its buy-in: `stack := buyin`,
   `ledger_in += buyin`, `status := Active`"*. It was unreachable and had been since the document
   was written: `Event::PlayerSeated` is consumed by T1 and T2 only, both in `Seating`, and §5.2's
   own rule is that an event arriving in a state with no matching row is a `Rejection`. Worse, the
   message type it implied does not exist — `PROTOCOL.md` §4.10 declares `PLAYER_SIT_OUT`,
   `PLAYER_SIT_IN` and `PLAYER_LEAVE` and no `PLAYER_SEAT`, and §4.11's table has no row for one.
   Describing a capability the alphabet does not have is the half-state P8 names; the alternative
   was to invent a wire message, a stage kind and a transition for a mode the MVP does not ship.
1. `hand_id += 1` (or `:= 1` on T10). `level`, `small_blind`, `big_blind` := the closed form of
   §9.2 evaluated at `hand_id`.
2. Every seat with `stack == 0` becomes `Busted`; it keeps its position (the dead button needs
   positions, not players) and is appended to `finish_order` if not already there.
3. Rotate the button per A1.3: `bb_seat := succ_active(bb_seat)`, `sb_pos := old bb_seat`,
   `button_pos := old sb_pos`; then apply the TDA 34-B heads-up adjustment if exactly two seats
   remain with chips.
4. **Derive `dealt_in` from the required emitter set, and from that set alone.**
   `dealt_in[s] := (status == Active ∧ s ∈ signed_this_hand)` for every seat with `stack > 0`.
   `SittingOut`, `Absent`, `Removed` and `Busted` seats get `dealt_in = false` (D-005, D-006 §4,
   D-014), **and so does a seat that signed nothing in the hand just ended (D-013, J2).**
   `signed_this_hand` still holds hand `k`'s value at this point — it **is** `P(k)`, which is hand
   `k+1`'s required emitter set — and step 8 clears it, which is why these steps are in this order
   and not the other. **`readmit` is not read into this**, and the same step hands it over:

   > **`dealt_in ⊆ P(m)` is exact, and the readmission set is not in it — normative, and this is
   > `P2-e`.** *(`PROTOCOL.md` §4.9 writes the pair of hands as `m` and `m+1`; this step's `k` and
   > `k+1` are the same two, and the box keeps §4.9's letters so the two halves can be read against
   > each other word for word.)* §4.9 defines a **readmission set `A`** — a stale `PLAYER_SIT_IN` of a
   > finished hand's boundary window, or a stale checkpoint-8 `STATE_HASH` that **agrees** with this
   > receiver's retained value, carries its sender in instead of being dropped. Since `P2` that set
   > widens the **accepted** emitter set of hand `m+1`'s stage 0 to `P(m) ∪ A` and leaves the
   > **required** set at `P(m)` (§4.9, §4.4). This step therefore reads `signed_this_hand` alone,
   > and §4.4's *"`dealt_in` is a subset of `P(k-1)`, always"* holds with no union to check it
   > against.
   >
   > **What this step does with `readmit` instead, and it is a hand-off and not a predicate.** The
   > set is read once, here, and handed to the protocol layer as the accepted-emitter widening for
   > the `HAND_INIT(m+1)` stage this hand init derives (§3.4); step 8 clears it. It is read off
   > `TableState` exactly as `signed_this_hand` is read off it for `P(m)` — two fields, one for the
   > required set and one for the accepted widening — so nothing is added to the interface and no
   > guard in this document reads `readmit` at all. What the engine contributes that the wire cannot
   > is **T67**'s filter: the wire has no `status`, so without that row §4.9's `A` is a route by
   > which a `Removed` seat re-enters, which is what I34(a) forbids.
   >
   > **Why the union that stood here was the defect and not a convenience (`P2`).** `A` is written
   > by `PROTOCOL.md` §4.0 step 10b, which runs on a **stale** event, and step 10a's anti-replay is
   > *skipped* for exactly those events. So one signed, perfectly **agreeing** checkpoint-8
   > `STATE_HASH`, re-sent by anybody — no key, no forgery, and forwarding it is a thing §1.5 permits
   > — re-entered its sender into `A` on every replay; `A` is cleared at every hand init and refilled
   > once per hand, so a set that is idempotent within one hand is **not** idempotent across the
   > sequence of them. Under the deleted union that seat became a **required** emitter of hand
   > `m+1`'s stage 0 at this receiver and at no other, stage 0 stalled to the full hand deadline —
   > which is one signed constant per table and **not** a function of the seat count: what §8.2
   > scales is its *floor*, `HAND_DEADLINE_FLOOR(n)`, so the stall is at least that floor at every
   > seat count and is the founder's advertised `n(17)` in fact — and it repeated **once
   > per hand for the `MAX_RETAINED_HAND_RECORDS` hands `PROTOCOL.md` §5.3 retains a record for**. The wire deleted the
   > union in the pass before this one; the engine still held it, so the amplifier still had its
   > input and the two halves of one peer disagreed about who must speak before stage 0 completes.
   >
   > **What it costs, stated rather than glossed, and it is one hand.** A seat readmitted through
   > `A` is **not `dealt_in` at hand `m+1`**. Its own `HAND_INIT(m+1)` copy is *accepted*, which
   > puts it into `signed_this_hand` for chain `m+1` under §2.8's rule, so it is a required emitter
   > — and dealable — at hand `m+2`. D-013's readmission promise is kept with one hand of latency,
   > and what that promise owes is *a moment at which a returning seat can be heard*, which being
   > accepted into `P(m+1)` is. The alternative was the stall above, once per hand, for a seat that
   > need never have been there.
   >
   > **The mirror argument the deleted union made, and why deleting it does not re-open the case it
   > answered.** It read: widening only the emitter set makes a readmitted seat *a required emitter
   > of a hand it takes no cards in*, which stalls stage 0 for a seat that has nothing to say. That
   > is true of a **required** set and it is exactly why `P2` does not widen one. Under the rule
   > above the readmitted seat is required of nothing at `m+1`; it is merely *allowed* to speak,
   > and an accepted copy that completes no stage and blocks none stalls nothing. The two halves
   > still move together — they simply both stay at `P(m)`.

   **And one thing the readmission set does not do: it does not thaw a freeze.** `PROTOCOL.md` §4.9 is
   normative that *"`A` does not thaw a freeze"*, and the engine agrees by construction rather than
   by a clause — `solitary_contradicted` is cleared by **T53** and by nothing else (§2.6, I33(c)),
   and T67 does not touch it.

   **Two sentences K-1's fix owes this step, one each, and neither is a new rule (K-9).**
   **(i) A peer's own emissions count into its own `signed_this_hand`.** `step` adds
   `sender_seat` on every event it accepts as content of the current hand, and it does not
   special-case the local seat — so this peer is in its own set from its own `HAND_INIT` copy
   onward, every hand. `PROTOCOL.md` §3.2 settles it on the wire side by reductio: the base case
   is *the signers of `TABLE_READY`*, and under the other reading a peer is never in its own `P`
   and therefore never a permitted emitter of the `HAND_INIT` it is required to emit. The engine
   agrees by construction rather than by a clause, and §5.2's solitary box is where the
   consequence is used.
   **(ii) A `PLAYER_LEAVE` enters no `P`.** It is accepted at a boundary (T58) and it adds nothing
   to `signed_this_hand`; the alternative would make every announced departure a required emitter
   of the hand after it, which is the frozen table D-013 exists to remove, reached through the one
   message whose whole meaning is that the sender is going. `PLAYER_SIT_OUT` and `PLAYER_SIT_IN`
   are **not** covered by this exclusion and do count — a seat that sits back in is demonstrating
   participation, and that is D-013's entire readmission rule.

   **On the first hand (T10) the set is the signers of `TABLE_READY`**, which is D-013's base case;
   the engine reaches it without a special case, because `TABLE_READY` is the collective stage that
   ends `Seating` and its copies are accepted as chain content before T10 runs.

   **This conjunct is the liveness gate, and it is the same predicate `PROTOCOL.md` §4.4 uses for
   `HAND_INIT`'s required emitter set** — that document owns the emitter set and this one owns
   `dealt_in`, and neither restates the other (D-011 rule 1). Narrowing only the emitter set would
   not have worked: a seat that need not sign `HAND_INIT` but is still `dealt_in` is still in
   `deck.participants` (§2.5), so the stall would move from `HAND_INIT` to `AwaitingKeySetup` and
   cost the identical `hand_deadline_ms`. Narrowing only `dealt_in` would not have worked either,
   because §4.4's set includes non-dealt-in occupied seats. Both are needed and they read one
   predicate.

   **The seat is skipped, not removed.** It keeps its seat, its `status`, its position in the ring
   and its stack; step 3's `succ_active` still gives it the blinds in turn and steps 6 and 7 still
   post them, so it drains exactly as D-005 requires and — unlike before D-013 — it genuinely
   **busts**, at step 2 of some later hand, which is what makes §9.3's end conditions reachable
   again (§12.1).

   **It rejoins by signing.** No transition, no vote, no certificate and no human at another seat
   puts it back: it is in hand `k+1`'s set iff it signed in hand `k`, so a client that reconnects
   and signs a chained event is dealt in again at the next boundary, and a client that does not,
   is not. That is the whole readmission rule.
5. Per seat: `start_stack_this_hand := stack`; `committed_round := 0`;
   `committed_hand := 0`; `acted_this_round := false`; `folded := false`; `all_in := false`;
   `revealed_hole := None`; `mucked := false`.
6. Post **antes** (`config.ante`, 0 in the preset): every seat with `stack > 0` that is
   `Active`, `SittingOut`, `Absent` or **`Removed`** posts `min(ante, stack)` into
   `committed_hand`. **`Removed` is in that list and its omission was a defect**: §2.4 says the
   status *"behaves like `Absent` for the blinds (steps 6–7)"* and D-014 point 3 requires the
   offender's stack to be **blinded off** rather than confiscated, which is what makes §12.1.1's
   decreasing measure apply to it and what makes I1 hold with no term moving at the removal. Step 7
   was already correct because it is positional and reads no status; step 6 read a status word and
   listed three of the four. In the preset the ante is 0, so the defect is inert there — which is
   exactly why it needed finding by reading rather than by playing (§9.4's custom tables set it
   non-zero).
   An ante does **not** enter `committed_round` and does **not** change `current_bet` — it is not
   a bet and nothing is owed against it. A seat whose stack is consumed by the ante is `all_in`
   if it is dealt in, and simply at zero if it is not.
7. Post blinds per A1.1: `sb_pos` posts `min(SB, stack)` **if that position is occupied by a seat
   with chips**, else the small blind is dead and simply not posted; `bb_seat` posts
   `min(BB, stack)`. Both go into `committed_round` and `committed_hand`.
   `current_bet := BB` — the **nominal** big blind, regardless of what the BB could actually post.
   `last_full_raise := BB`. `acted_this_round` stays `false` for both blinds, which is what gives
   the big blind its option (A1.1 step 5, A2).
8. `street := PreFlop`; `board.clear()`; `aggressor := None`; `history.clear()`;
   `deck := DeckState::new(participants = dealt_in seats)`; `settlement := None`; `abort := None`;
   **`certified_subjects := ∅` is deleted with the field (D-015, §2.8).** Its reason is retained
   for the version that restores it: the exclusion set is per hand, so `|V|` starts each hand at
   `|dealt_in| − 1` and can only be reduced again by a completed, valid certificate within that
   hand (D-008 point 3); carrying it across a hand boundary would let exclusions accumulate over a
   session and reach `|V| < 2` without any single hand ever paying the floor. **Nothing else in
   this step changes**, and in particular the two clears below are untouched — they carry D-013
   and `P2-e`, not the certificate.
   **`signed_this_hand := ∅`** and **`readmit := ∅`** — the participation set and the readmission
   set are both per hand, and step 4 has already read each of them, for two different purposes.
   Clearing either earlier would deal nobody in; not clearing `signed_this_hand` at all would make
   participation permanent and restore the frozen table D-013 removes. **They are cleared in one
   place and at one moment and they are nevertheless two quantities, which is `P2-e`**:
   `PROTOCOL.md` §4.9 says `A` is *"read at exactly one place — the next hand init this receiver
   runs — and is cleared there"*, and a `readmit` that outlived a hand init would widen a second
   stage's accepted set on the strength of an event about a hand two boundaries back.

   **Hand init is the only place it is cleared, so the set spans the boundary window and a paused
   table (K-3b, K-7).** Between two hand inits it means *"every seat heard from since the last hand
   init"*, which includes the checkpoint-8 `STATE_HASH` copies of §5.2's box, the seat-state events
   T59 accepts, and everything accepted while the table sat in `Paused`. That is what gives a
   returning seat a way back in without a vote or a human (D-013), and it is what T59's `Paused`
   exit reads. **The one seat-state event excluded is `PlayerLeft` (T58)** — step 4(ii) — and the
   sentence that stood here named T58 and T59 together, which was wrong on T58's half.

   **`solitary_since` is maintained here, in the same step and from the same read (K-9, N4).**
   Before this step clears `signed_this_hand`: if
   `|signed_this_hand| == 1` then `solitary_since := solitary_since.or(Some(hand_id))`.
   **It reads `signed_this_hand` and never `signed_this_hand ∪ readmit`, and that is `P2-e` and the
   half of `K1` this document owed.** The regime `PROTOCOL.md` §3.2 defines is *the required
   emitter set of the stage has one member*, and since `P2` the required set is `P(k-1)` exactly —
   `A` widens the **accepted** set and never the required one — so a peer holding
   `P(k-1) == {self}` and a non-empty `readmit` is **still** comparing its state against a set of
   one and its stage 0 still self-completes. The union that stood here made that peer read
   `|admitted| == 2`, wrote nothing, and left `solitary_at(k)` false for every hand of the episode:
   **T62 and T63 were dead exactly where K-1 lives**, and the solitary tournament win the wire had
   closed came back through the engine. One set, one comparison, and `readmit` appears in no guard.
   **One quantity, two direct readers** — step 4, which turns it into `dealt_in`, and this
   step — and step 9 then branches on `dealt_in`, so nothing downstream reads a second predicate.
   **There is no `else` branch, and deleting it is N4's fix**: the field is monotone and is never
   cleared, because a regime that is left can be re-entered, and the `else` that cleared it erased
   the earlier episode while `PROTOCOL.md` §4.0 step 10b's record kept it — the wire froze on a hand
   the engine then rejected. §2.6 carries the floor, the lemma that orders the two memories, and the
   reason the interval argument this replaces was false. That is the entire bookkeeping, and it is
   one comparison on a set the step already holds.
9. Branch:
   * `|dealt_in| == 0` → `Paused` (no blinds are posted; step 6 and 7 are skipped);
   * `|dealt_in| == 1` → the single dealt-in seat wins every posted blind and ante with no cards
     and no cryptography: go straight to `Settling`. This is how an absent field drains
     correctly, and **since D-013 it is the path a heads-up table takes on every hand after its
     opponent has gone silent** — no key setup, no shuffle, no reveal, nothing that could wait on
     the seat that is not there. **It is also the path that placed no checkpoint at all until
     K-3**: a hand that reaches `Settling` from here holds no `DECK_COMMIT` and closes no betting
     round, so `PROTOCOL.md` §6.2's checkpoints 2 to 7 are every one of them unreachable on it.
     What it does place is **checkpoint 8**, at T45, like every other hand (§5.2's box), and on
     this path that is the only comparison of state two peers ever make;
   * `|dealt_in| ≥ 2` → `AwaitingKeySetup`, `RequestKeySetup`, `ArmDeadline{Crypto}`.

   **The branch K-9 is about, and it is the drain branch read with `P` instead of `dealt_in`.**
   `|dealt_in| == 1` is *not* the solitary regime and the two must not be collapsed: `dealt_in` is
   who takes cards, `P(k-1)` is who must emit, `dealt_in ⊆ P(k-1)`, and a table with one dealt-in
   seat and three sitting-out seats that are all still signing is a perfectly ordinary drain hand
   with four required emitters and nothing solitary about it. The regime is `|P(k-1)| == 1`, it is
   recorded at step 8, and **it does not change this branch at all**: a solitary peer still deals,
   still goes straight to `Settling`, still places checkpoint 8 at T45. That is `PROTOCOL.md`
   §3.2's ruling and the reason for it — *"a table whose other seats are genuinely gone must drain
   them, or a tournament never ends and D-013 buys nothing"* — and the rejected alternative,
   flooring `|P|` at 2, is what would have changed it. What the regime adds is not a branch here
   but a **transition that can fire later**: T62, on a chained event of a hand this peer dealt
   solitary from a seat outside `P(k-1)`. Nothing in this step is conditional on it.

   **What K-9 found missing was exactly that transition, and this is the sentence that used to be
   wrong by omission.** Before this pass, `|dealt_in| == 1 → Settling` was the whole of the
   solitary path in the engine, `grep -c solitary` over this document returned **0**, and
   `PROTOCOL.md` §3.2 was carrying a normative rule with no phase to reach and no row to consume
   it. `Diverged` is the phase, T62 is the row, and §5.2's solitary box is where they are stated.

**Where `signed_this_hand` is accumulated, and where `readmit` is not.** `readmit` is written by
**T67** and by nothing else — it is an ordinary transition, because the fact it records arrives as
an event of its own and is not a property of some other event this peer accepted. `signed_this_hand`
is the opposite and is not in any transition row, because it is not any one
transition's business: `step` adds `sender_seat` to it on **every** event it accepts as content of
the current hand, before consulting §5.2's table, **with exactly three exclusions and no others**.
A `Rejection` adds nothing — a rejected event is not chain content, and admitting one would let a
peer buy a seat's participation by sending it garbage. A **terminal `HAND_ABORT`** adds nothing
(I31(c)): it enters no `stage_hash`, and two honest peers routinely accept copies signed by
different seats, so counting its signer would put the liveness gate back on a per-receiver quantity
and is a D-012 violation. A **`PlayerLeft`** adds nothing (step 4(ii)). And a seat whose `status`
is `Removed` is never added at all (D-014): the removal takes it out of every set at once, and a
set it could re-enter by signing would be a re-entry route T59 was closed to make impossible.
`HAND_INIT`'s own copies count, which is why a seat that vanishes mid-hand costs a second hand
rather than one (§12.1). **A `SolitaryDivergence` adds nothing either, and that one is not an
exclusion but a consequence**: the offending event is never accepted as chain content — `PROTOCOL.md`
§4.0 step 12a says it is *"not rejected, not applied, and not counted into any `P`"* — so `step`
never sees a `sender_seat` for it and there is nothing to exclude. If it did count, the
contradicting seat would join `P`, the next hand would not be solitary, and the rule would disarm
itself on the first event it fired on.

### 5.4 Opening and resetting a betting round

On entering a betting phase from a reveal phase (T37):

* `∀ s: committed_round[s] := 0; acted_this_round[s] := false`;
* `current_bet := 0`; `last_full_raise := BB` (A4: the min-bet yardstick resets to the big blind
  each street, it does not carry over); `aggressor := None`;
* `player_to_act := ` first to act per A2 — heads-up `bb_seat`, otherwise
  `succ_live(button_pos)`, skipping folded and all-in seats.

On entering `BettingPreFlop` from `AwaitingDeal` (T26) the round state is the one hand init left
behind (blinds already committed, `current_bet == BB`, nobody has acted), and
`player_to_act := button_pos` heads-up, `succ_live(bb_seat)` otherwise.

`POKER_RULES.md` A1.2 is emphatic that the heads-up post-flop order is the *inverse* of the
3+-handed order and must be an explicit two-player branch with unit tests on both, even though
the general formula happens to give the right answer. This document adopts that instruction as
normative.

---

## 6. Legal-action computation

This is the single function both the GUI and the validator call. `SPEC_CS.md` §11 requires every
incoming action to be re-validated locally; §17 requires assuming a completely modified client
("do not assume the GUI will not let him click that"). Therefore the GUI calls
`legal_actions` to *draw* buttons and the engine calls the identical function to *accept* an
event. There is exactly one implementation.

```rust
pub struct LegalActions {
    pub fold:  bool,
    pub check: bool,
    pub call:  Option<Chips>,      // exact chips that move, = min(to_call, stack)
    pub bet:   Option<Range>,      // total-commitment range, only when current_bet == 0
    pub raise: Option<Range>,      // total-commitment range, only when current_bet > 0
}
pub struct Range { pub min_to: Chips, pub max_to: Chips }   // inclusive, contiguous
```

### 6.1 Preconditions

`legal_actions(s, p)` returns the empty set unless **all** of:

* `s.phase ∈ {BettingPreFlop, BettingFlop, BettingTurn, BettingRiver}`;
* `s.player_to_act == Some(p)`;
* `s.seats[p].dealt_in ∧ ¬folded ∧ ¬all_in`.

An empty set is a valid answer and means "this seat may do nothing right now".

### 6.2 The quantities

```
to_call      = current_bet.saturating_sub(committed_round[p])
max_to       = committed_round[p] + stack[p]            // total commitment if all-in
min_raise_to = current_bet + last_full_raise            // POKER_RULES A4 (TDA 43-A, 47-A)
can_reopen   = !acted_this_round[p]
             || (current_bet - committed_round[p]) >= last_full_raise    // POKER_RULES A5
```

`can_reopen` deliberately does **not** track "who was the last aggressor" and needs no per-player
"amount faced when I last acted" field: because every completed voluntary action leaves
`committed_round[p] == current_bet` (or leaves `p` all-in and unable to act again), the quantity
`current_bet - committed_round[p]` **is** the increment `p` has faced since `p` last acted. The
cumulative-short-all-in case of TDA 47-A therefore falls out for free (`POKER_RULES.md` A5,
worked example 3).

### 6.3 The exact answer

```
fold  = true                                     // always legal, TDA imposes no restriction
check = (to_call == 0)
call  = if to_call > 0 { Some(min(to_call, stack[p])) } else { None }

bet   = if current_bet == 0 && stack[p] > 0 {
            Some(Range { min_to: min(BB, max_to), max_to })
        } else { None }

raise = if current_bet > 0 && can_reopen && max_to > current_bet {
            Some(Range { min_to: min(min_raise_to, max_to), max_to })
        } else { None }
```

Both ranges are **contiguous and inclusive**. Every integer in `[min_to, max_to]` is legal and
nothing outside is. The `min(..., max_to)` collapses the range to the single point `max_to`
exactly when the player cannot afford the minimum, which is the all-in-for-less-than-a-minimum
case; the range never has a hole.

Restated as the validator predicate, which is what `step` actually evaluates:

```
Fold                       ⇔ preconditions hold
Check                      ⇔ to_call == 0
Call                       ⇔ to_call >  0                    (moves min(to_call, stack))
Bet(n)                     ⇔ current_bet == 0 ∧ ((n >= BB ∧ n <= max_to) ∨ n == max_to) ∧ n > 0
Raise(t)                   ⇔ current_bet >  0 ∧ can_reopen
                             ∧ ((t >= min_raise_to ∧ t <= max_to) ∨ t == max_to) ∧ t > current_bet
```

### 6.4 Five consequences that implementations get wrong

1. **There is no "all-in" action.** All-in is `Call` when `stack ≤ to_call`, `Bet(max_to)` when
   `current_bet == 0`, and `Raise(max_to)` when `current_bet > 0 ∧ can_reopen`. Nothing else.
2. **When `¬can_reopen` and `stack > to_call`, all-in is ILLEGAL.** `POKER_RULES.md` A5: *"`p` may
   not raise, not even all-in for more — an attempted raise is an illegal action and is rejected."*
   The GUI's all-in button must therefore be **disabled** in this state, not clamped. This is the
   one place where the natural GUI affordance is a rules violation, and §17 says the engine must
   reject it regardless of what the GUI does.
3. **`current_bet` is the nominal big blind even when the BB could not post it.** With blinds
   50/100 and a BB holding 60, `current_bet == 100`: UTG calls 100, and a raise is to at least
   200 (A1.1 rule 3, verified against PokerTH's `setHighestSet(2*getSmallBlind())`).
4. **Over-calling is not a special case.** Behind a short all-in of 125 with `current_bet == 200`,
   `to_call = 200 - committed_round[p]`. There is no code path that calls the short amount.
5. **Folding for free is legal.** `to_call == 0` and `Fold` is still accepted. It is never
   correct, and the GUI should not offer it, but a modified client may send it and the engine
   must have a defined answer rather than a `Rejection` (which would be an equivocation about
   what the rules are).

### 6.5 GUI quick-bets (advisory, not rules)

`SPEC_CS.md` §22 requires 1/2-pot, pot and all-in buttons. These are *conveniences* that compute a
candidate and then clamp it into the range above; they are not part of the rules and are not
validated by the engine beyond the range check.

```
pot_now = Σ_s committed_hand[s]                    // = pots().total(), pre-refund
half    = clamp(committed_round[p] + to_call + (pot_now + to_call)/2, min_to, max_to)
potbet  = clamp(committed_round[p] + to_call + (pot_now + to_call),   min_to, max_to)
allin   = max_to
```

If `raise` is `None`, all three buttons are disabled — see consequence 2.

---

## 7. Normative engine rules for the hard cases

All of these are `POKER_RULES.md` restated as engine obligations. Where that document cites TDA
verbatim, the citation is the authority; where it marks **[OUR CHOICE]**, the choice is repeated
here so the engine specification is self-contained.

### 7.1 Blinds and heads-up

* `BB = 2 × SB` always (`config` and §9.2).
* SB posts `min(SB, stack)`, BB posts `min(BB, stack)`; either is `all_in` immediately if the
  stack does not cover it, *before any voluntary action*.
* `current_bet := BB` nominal; `last_full_raise := BB`.
* Posting a blind is **not an action**: `acted_this_round` stays `false` for both blinds. This is
  the sole mechanism giving the big blind its option, so it must not be "optimised" away.
* Heads-up (exactly two seats with chips): `sb_pos == button_pos`, `bb_seat` is the other seat;
  pre-flop the button acts first, post-flop the big blind acts first and the button acts last.
  Implement as an explicit two-player branch (A1.2).

### 7.2 Button rotation, dead button, busts

Adopted rule (A1.3, **[OUR CHOICE]**, TDA 32 dead button, explicitly *not* PokerTH's moving
button):

```
next_bb_seat    = succ_active(bb_seat)      // next seat clockwise with chips
next_sb_pos     = old bb_seat               // a POSITION; may now be empty -> dead small blind
next_button_pos = old sb_pos                // a POSITION; may now be empty -> dead button
```

A dead small blind is simply not posted and the pot is one small blind lighter. A dead button is
still a well-defined index and is used by the post-flop action order (A2) and the odd-chip rule
(A8). At two players, apply TDA 34-B: adjust the button if the rotation would give one player the
big blind twice in a row.

A seat whose stack reaches 0 is eliminated at the **end** of the hand and is not dealt into the
next one. Simultaneous bust-outs are ranked by `start_stack_this_hand`, larger finishing higher;
ties by seat order clockwise from `button_pos`, earlier finishing higher (**[OUR CHOICE]**: a
decentralised game needs a total order that is a pure function of public state).

### 7.3 Minimum bet, minimum raise, `last_full_raise`

```
min bet          : n >= BB, or n == max_to
min raise        : to >= current_bet + last_full_raise, or to == max_to

after a legal Bet(n) or Raise(to):
    increment = to - current_bet                       // for Bet, current_bet is 0
    if increment >= last_full_raise { last_full_raise = increment }    // a FULL raise
    // else: a short all-in. last_full_raise is NOT changed.
    current_bet = max(current_bet, to)
```

The `else` branch is TDA 47-A's second sentence: short all-ins never move the min-raise yardstick,
however many of them stack up. `last_full_raise` is monotonically non-decreasing within a round,
so TDA 43-A's "largest prior full raise" and 47-A's "last full valid raise" coincide and one
variable suffices. Reset to `BB` at every street boundary.

**Regression test, mandatory.** TDA Illustration Addendum Rule 47 Example 2 (blinds 50/100,
post-flop A bets 300, B all-in 500, C all-in 650, D all-in 800, E calls 800): F's `min_raise_to`
must be 1100.

### 7.4 Incomplete all-in and reopening

```
can_reopen(p) ⇔ !acted_this_round[p] ∨ (current_bet - committed_round[p]) >= last_full_raise
```

If false, `p`'s only legal actions are `Fold` and `Call` (§6.4 consequence 2).

**Regression tests, mandatory** — the three worked examples of `POKER_RULES.md` A5 together
exercise every branch:

1. Example 3-A: short all-in of 7500 over 4000 does **not** reopen for A (`3500 < 4000`); the BB
   still may raise via `!acted_this_round`.
2. Example 3-B: the BB's full raise to 11500 **does** reopen for A (`7500 ≥ 4000`).
3. Examples 1 / 1-A: cumulative short all-ins (25 + 75) total a full raise and reopen for A, but
   not for C (`75 < 100`), and C's threshold flips at exactly `current_bet == 225`.

### 7.5 Side pots

Derived, never incrementally mutated (A7):

```
fn build_pots(committed: &[Chips], folded: &[bool], dealt_in: &[bool])
        -> (Vec<Pot>, Vec<(SeatIdx, Chips)>) {
    let mut levels: Vec<Chips> = committed.iter().copied().filter(|&c| c > 0).collect();
    levels.sort_unstable();  levels.dedup();
    let (mut pots, mut refunds, mut prev) = (Vec::new(), Vec::new(), 0);
    for lvl in levels {
        let contributors: Vec<SeatIdx> = seats().filter(|&s| committed[s] > prev).collect();
        let step = lvl - prev;
        let size = step * contributors.len() as Chips;
        let eligible: Vec<SeatIdx> = contributors.iter().copied()
                                        .filter(|&s| dealt_in[s] && !folded[s]).collect();
        if contributors.len() == 1 {
            refunds.push((contributors[0], size));       // uncalled excess, never a pot
        } else if eligible.is_empty() {
            // nobody at this level can win it: return each contributor's own stake for
            // this level. Reachable only when every contributor above the dealt-in seats'
            // commitment levels is an absent or sitting-out blind poster (D-005).
            for &s in &contributors { refunds.push((s, step)); }
        } else {
            pots.push(Pot { size, eligible });
        }
        prev = lvl;
    }
    (pots, refunds)
}
```

* Pot 0 is the main pot; pots `1..` are side pots in ascending all-in order. TDA 21: each is
  split separately, and this algorithm satisfies that by construction.
* A folded player's chips **are** in the pots; the folded player is in no `eligible` set.
* **`eligible` filters on `dealt_in` as well as on `folded` (P7), and that is what implements
  D-005.** §5.3 step 4 leaves `Absent`, `SittingOut` and `Busted` seats with `dealt_in = false`,
  step 5 sets `folded := false` for *every* seat, and steps 6 and 7 have `Absent` and `SittingOut`
  seats with chips post antes and blinds. Without the `dealt_in` filter such a seat is a
  contributor with `folded == false` and lands in `eligible` for the main pot — a seat that by
  **I20** holds no deal-map index and has `revealed_hole == None`, so the evaluator would be asked
  for a hand that does not exist. §12 states as *implemented* that an absent seat cannot win the
  blind it posts; this line is where it is implemented, and before P7 nothing was. **The filter
  reads `dealt_in`, never a status**, which is why H1 does not touch it: since H1 no transition
  produces `Absent` (§2.4), and `SittingOut` — reached by T59 or by §8.5's auto-action limit — is
  the state with identical engine behaviour, so the reachable construction of this case is a
  sitting-out seat in the blinds.
* The single-contributor test is simultaneously the uncalled-bet refund rule, the
  "everyone folds to a bet" rule, and the "A bets 300, B raises all-in to 900, A folds" rule.
* **A level with two or more contributors and an empty `eligible` set is now legally reachable**,
  and it is refunded rather than awarded. It needs a non-participating seat committed above
  every dealt-in seat's commitment — e.g. a dealt-in seat all-in for an ante of 20 while two
  `SittingOut` seats in the blinds have posted 50 and 100, so the level from 20 to 50 has
  contributors `{SB, BB}` and no eligible seat. Each contributor gets back exactly the `step` it
  put in at that level. That is the same idea as the single-contributor refund — money nobody
  eligible can win is uncalled — and it is *not* an absent seat winning: it recovers its own
  uncontested stake and wins nothing. Chips are conserved by construction, so **I6** is unchanged
  and **I7**'s `eligible ≠ ∅` stays true of every pot that exists.
* The engine must still **assert** `eligible ≠ ∅` on every constructed `Pot`. That assertion is now
  a real check rather than a formality, because the branch above is what keeps it true; if a
  byzantine peer ever produces a `Pot` with an empty `eligible`, split it pro rata among its
  contributors rather than destroying chips (A7).
* Side pots require at least three seats with distinct commitment levels, so heads-up
  (`SPEC_CS.md` §32's first mode) exercises `build_pots` only in its one-pot-plus-refund form.
  The multi-pot paths become reachable only in the 3–6 player extension, and must be tested
  there rather than assumed.

### 7.6 Split pots and odd chips

```
share     = pot.size / |W|        // integer division
remainder = pot.size % |W|        // 0 <= remainder < |W|
```

Each winner gets `share`. The `remainder` odd chips go one each, clockwise starting from the first
seat left of `button_pos`, to the first `remainder` members of `W` in that order (TDA 20-A,
generalised from one chip to `remainder`; **[OUR CHOICE]** to use seat order and not
high-card-by-suit, because seat order is a pure function of public state already in the hash
chain, needs no card data, and cannot disagree with the evaluator).

`button_pos` may be an empty seat under the dead button; that is fine, it is still an index.

Suits **never** break a tie. The wheel `A-2-3-4-5` is the lowest straight. `Q-K-A-2-3` is not a
straight. Only five cards count, so a third pair never breaks a tie (A10).

### 7.7 Showdown

* If all but one player folded, there is **no showdown**: the last live seat wins every pot it is
  eligible for and reveals nothing. This is also the path that lets a hand complete when a peer
  has vanished (§8.5).
* If at least one player is all-in and all betting is complete, TDA 16 applies: every live seat
  must table, no mucking.
* Otherwise TDA 17-A order of show: `showdown_order` starts with `aggressor` if the river had a
  bet or raise, otherwise with the seat that acts first on the river (A2), then clockwise over the
  live seats.

**OPEN QUESTION, carried forward unresolved from `POKER_RULES.md` A8.** Whether a beaten player
may muck is a genuine conflict between `SPEC_CS.md` §13 (anyone can verify the winner and the pot
were computed correctly) and §9/§35 (hole cards stay sealed until the rules force a reveal). The
three candidate resolutions — mandatory universal reveal; TDA-faithful mucking with a binding
signed forfeiture; delayed reveal at end of tournament — each change the *cryptographic* protocol,
not just the engine. The decision belongs to `PROTOCOL.md` / `DECISIONS.md` and is not taken here.

This state machine is therefore written **policy-parametric**: `config.showdown_policy` selects
between `MandatoryReveal` and `TdaMuckWithForfeiture`, and T43 exists only under the latter. The
MVP runs `MandatoryReveal` because it is the only option that needs no additional cryptography and
no escrow of shares — that is an implementation-order convenience, **not** a resolution of the
question, and the option must not be deleted from the type.

**What a build does with a `showdown_policy` it has not implemented is not left to be guessed
(`PROTOCOL.md` Q-01).** `PROTOCOL.md` §4.6 declares `SHOWDOWN_MUCK` with a policy-gated emitter
set and pins `showdown_policy = MANDATORY_REVEAL` in `RATED_SNG_POKERTH_V1`, so the two branches
are live in the wire format while only one is live in the MVP. The rule is the one §9.4 already
applies to every other config parameter: **the config is validated before leaving `Seating`, and a
table whose `showdown_policy` this build does not implement goes to `TableClosed` and is never
played.** There is no third behaviour — no playing a `TDA_MUCK` table with T43 quietly absent,
which would make two clients settle the same showdown differently, and no local repair of a signed
advertisement. An implementation may therefore ship one branch, but it may not ship one branch and
accept both values.

### 7.8 The deal map — fixed before the shuffle

`MENTAL_POKER.md` §6 is explicit that the index→recipient mapping must be fixed **before** the
shuffle chain starts, or a malicious last shuffler could argue afterwards about which index is
whose card. It is therefore computed in T14, from public state only, with no randomness.

**The map itself is `PROTOCOL.md` §4.5's and is not reproduced here** (D-011 rule 1, found in this
pass's H8 sweep rather than by the gate, which flagged only `CRYPTOGRAPHY.md` §2.4's copy). §4.5
closes with *"This section owns the map; no other document may restate it independently"*, and the
previous revision of this section restated it anyway, in a code block, while citing the sentence
that forbids it. What this document owns is the two engine facts that are transitions, not layout:
the map is computed **at T14**, before any `RequestShuffle` is issued, and it is a pure function of
`dealt_in` and `button_pos` at hand init, so every peer derives the identical map with no
randomness and no negotiation. The dealt-in count is `m` in every document of this corpus, and
`m ≤ 10`, so at most 25 of the 52 indices are ever used.

**No burn cards** (**[OUR CHOICE]**). A burn exists to defeat marked-card reading of a physical
deck. Here it would consume an index that would then have to be provably never opened, adding a
rule with no security value. This is not a free per-document choice: a burn costs a deck position
and changes the map, and the map is hashed into `index_map_hash` inside `DECK_COMMIT`, so two
conforming clients with different maps produce a guaranteed `DECK_COMMIT` mismatch every hand — a
manufactured `SPEC_CS.md` §15 dispute at every hand, which after A-3/A-6 faults the table (T54).
The choice is recorded in the deviation register of `THREAT_MODEL.md` §9.1.1.

Indices `2m+5 … 51` are **never** opened. A reveal token for any of them is a protocol violation
(T25), and so is a token for a future street's index (`SPEC_CS.md` §10) or for a hole index other
than at its proper moment (T24).

**Why publishing hole-card reveal tokens publicly is safe.** To open seat `P`'s card at index `i`,
all `n` shares are needed. Every participant except `P` publishes its token for `i`, publicly.
`P` completes the set with its own share. A third seat `Q` sees `n-1` tokens — everyone's but
`P`'s — and holds only its own, which is already among them; `Q` is missing `P`'s token and cannot
open. This is the `n`-of-`n` property that `MENTAL_POKER.md` verified empirically (tests T7d/T7e:
one share alone yields `None`). It matters here because it lets `DealComplete` (T26) be decided
from public information, which is what makes the deal deterministic.

At showdown the *owner* publishes its own token (`Audience::OwnerOf`), completing the set for
everyone. That is the only moment a seat ever publishes a token for its own card, and T24 rejects
it at any earlier moment.

### 7.9 The randomness beacon

**Recorded spec deviation.** `RNG_COMMIT` / `RNG_REVEAL` run **once per table** (the setup chain),
not per hand as `SPEC_CS.md` §16 reads. The deviation is recorded — not absorbed — in the single
deviation register of `THREAT_MODEL.md` §9.1.1, row 2, together with `PROTOCOL.md` §4.4 and
`CRYPTOGRAPHY.md` §7.3. The argument for it is below and is unchanged.

Per `MENTAL_POKER.md` §6: the shuffle chain already *is* the per-hand distributed randomness, so
`SPEC_CS.md` §7's substantive requirement is met structurally — the final deck order is the
composition of all `n` secret permutations and is uniform as long as **one** player is honest.
The last shuffler gains nothing, because it sees only ciphertexts under the aggregate key and has
no information to bias toward.

The commit/reveal beacon (T6–T11) exists for the *non-deck* randomness only: seat assignment and
the initial button position. **The engine does not compute either value** — it sees only the
verdicts of T6–T11, and the commitment check in T9 and T11 is a verdict handed to it, not an
expression it evaluates.

**The two constructions are `PROTOCOL.md`'s and are not reproduced here (H8, D-011 rule 1).**
`commitment_i` and `seed` are defined in **`PROTOCOL.md` §4.4**, over the hash constructor and the
domain-string register of **`PROTOCOL.md` §2.8**; the retired domain strings, including this
document's own earlier `p2p-poker/seat-beacon/v1`, are listed under that register. The previous
revision printed both formulas here, with the reason attached: *"the constructions are printed here
because this document previously printed a third, incompatible version of both and that is what made
them diverge (C-2)."* That reason is the argument against the copy, not for it — a defect caused by
a copy was being repaired with a corrected copy, which is precisely the drift D-011 rule 1 exists to
stop, and the corrected copy is one edit of §4.4 away from being a fourth incompatible version. The
pointer cannot drift. Nothing about the engine changes: no transition in §5.2 reads either formula.

`session_id` is the object `SPEC_CS.md` §14 and §20 call the *session nonce*; the name
`session_nonce` is retired and there is one name, `session_id`, defined by `PROTOCOL.md` §4.3. That
sentence is kept because it is a naming ruling this document must not contradict, not a
construction.

Failure to reveal, or a mismatched opening, is an attributable fault with a self-contained proof.
`r_i` and `salt_i` come from `getrandom::SysRng` (`CRYPTO_LIBS.md` §1 — `OsRng` no longer
exists in the current release line; that rename is row 1 of the `THREAT_MODEL.md` §9.1.1
register).

---

## 8. Timeouts, disconnect and abort

### 8.1 The distinction that governs everything here

D-006 corrects a conflation in D-005 and this document adopts the correction verbatim:

| | Human absent, client running | Client gone or withholding |
|---|---|---|
| Betting decision | auto check/fold | auto check/fold |
| Decryption shares | published normally | **not published** |
| The hand | **continues to the end** | cannot open further cards |
| Consequence | next street, next hand | abort, attribute in the transcript, restore every stack, next hand (D-010) |

Publishing a reveal token is an automatic client step, not a human decision. A player who walks
away from the keyboard therefore keeps cooperating cryptographically: the board opens on schedule
and the hand plays out. Only a client that is gone or actively withholding reaches the abort path.

**No timeout of any kind ends the tournament or the cash game** (D-006 §5). The worst a timeout
can do is abort one hand, after which the next begins immediately and automatically.

### 8.2 The engine has no clock

`state.deadline` is a *description* — `{kind, subject, sequence, parent_hash, duration_ms}` — not
a timestamp. `step` never adds `duration_ms` to anything and never reads a clock. The `ArmDeadline`
effect hands the description to the scheduler in `LocalView`; the scheduler is the only component
that knows what time it is.

**When a local timer fires the scheduler does not change state — and under D-015 what it does
instead depends on which timer it was.** The paragraph that stood here said it *"signs and
gossips a `TIMEOUT_VOTE`"* and, on collecting votes from all of `V(subject)`, *"assembles a
`TIMEOUT_CERT` and feeds the engine the `TimeoutCertificate` event of §4.1"*. **None of that
happens.** `PROTOCOL.md`'s header box makes both messages defined-but-not-produced; §4.1's event
is deleted; there is nothing to assemble and nothing to feed. The two live cases:

* **`state.deadline.kind == Action`.** The scheduler makes *this peer's own seat* act, and only
  when the deadline is this seat's: it emits an ordinary `ACTION_CHECK` when `to_call == 0` and
  `ACTION_FOLD` otherwise, signed by this seat, at this seat's own stage. The engine sees it as
  `Event::Action` and consumes it through **T29–T33**. **A peer never emits anything on another
  seat's expired action deadline** — that was the certificate's job and it is not reassigned;
  the other seat's own client does it, or the other seat is silent and the hand stalls.
* **`state.deadline.kind == Crypto`.** The scheduler emits nothing at all. A decryption share
  cannot be published on another seat's behalf, so there is no auto-action to derive; the stage
  stalls and the whole-hand timer below is what ends it.

**So the engine's inputs are unchanged in kind and reduced by one variant**, and the "no clock"
contract is stronger than it was: a timer now produces either an ordinary action by the seat
that owed one, or nothing.

**There is a second timer, and the engine does not arm it either.** The whole-hand limit
`hand_deadline_ms` is the protocol layer's to arm and to cancel, not the engine's. `state.deadline`
describes the *current stage's* deadline and is armed and disarmed by the transitions in §5.2; the
hand limit is not in `TableState` at all, because nothing in the engine reads it and putting it
there would be storing a duration twice (`config.hand_deadline_ms` already carries it). When it
fires, the scheduler again changes no state: it makes this peer emit its own copy of the
`HAND_ABORT` stage of `PROTOCOL.md` §4.10, and the engine sees the result only as
`Event::HandDeadlineAbort` once one verifying copy has been accepted (§4.1, T57). Both timers
therefore obey the same rule — **a timer produces a message, never a transition** — and §3.1's
"no clock" holds without exception.

**The hand deadline runs from `TERMINAL(k−1)`, not from `HAND_INIT`, and that is now
`PROTOCOL.md`'s ruling rather than this document's request (R-1).** `PROTOCOL.md` §8.2 is
normative:

> the timer for hand `k` starts when the peer accepts the event that fixes `TERMINAL(k-1)` — the
> completing copy of hand `k−1`'s `HAND_COMPLETE` stage, or hand `k−1`'s terminal `HAND_ABORT`, or,
> **for a table's first hand, the completing copy of the `TABLE_READY` stage**.

**This document followed its own earlier wording and now follows theirs.** The previous revision
said the first hand's timer starts at the checkpoint-1 `STATE_ACK` stage that follows
`TABLE_READY`. `PROTOCOL.md` starts it at `TABLE_READY` itself, which is `TERMINAL(0)` (§3.1), and
that is the binding answer: one rule — *the timer starts at the previous chain's terminal* — with
no special case for `k = 1`. The engine's behaviour is unchanged either way, because nothing in
`step` reads the start point; the sentence is corrected so that two documents do not name two
different artefacts for one timer.

Why it matters that the anchor is an agreed event, in `PROTOCOL.md`'s words rather than restated
here: §4.10's acceptance gate has a receiver **buffer** an early terminal abort until its own
deadline expires, and buffering is bounded only if every peer's timer started from the same
artefact. `HAND_INIT` is not such an artefact — peers accept different copies of it at different
moments, and a peer that never accepts one would never start.

**What it covers in this document's phases, stated because the interval is wider than a hand.** The
window for hand 1 opens at `TABLE_READY`, which is before the seat-order beacon, so it spans phases
2 and 3 as well. **T57 is still not admissible there and must not be made so**: its guard requires a
live hand, and before T10 there is none — `hand_id == 0`, no `HAND_INIT`, and `GENESIS(1)` is not
even derivable, since it contains `roster_hash(1)` and the seat indices come from the beacon
(`PROTOCOL.md` §3.1). A stall in phases 2–3 is **T4's**, and the numbers make that reachable first:
`join_deadline_ms` and `hand_deadline_ms` are independent advertised parameters — `n(18)` and
`n(17)`, with independent caps (`PROTOCOL.md` §7.2) — so a conforming advert may order them either
way, and **the coverage of phases 2–3 neither does nor may depend on the ordering**. Whichever
expires first, the exit is T4's: a `HandDeadlineAbort` arriving in phases 1–3 satisfies no
transition in this document, since T57 requires a live hand and T61 requires `HandComplete` or
`Diverged` with `hand_id > 0`, so it is a `Rejection` and the table still closes at
`join_deadline_ms`. **The sentence that stood here computed a margin from the preset's
`join_deadline_ms` and from `HAND_DEADLINE_FLOOR(2)`** and concluded that T4 always fires first; the
arithmetic held for `RATED_SNG_POKERTH_V1` and the conclusion was false for a `CUSTOM` table that
advertises a long join window — which is the only kind the MVP ships (§9.5). The margin is deleted
rather than repaired, because the exit never needed it.

For hands `k ≥ 2` the window opens at `TERMINAL(k−1)` and the engine's phase across the interval
containing `HAND_INIT` is `AwaitingKeySetup` — both T10 and T47 run hand init and land there —
which is in T57's scope. So a `HAND_INIT` that never completes ends the hand exactly as any other
stall does, with no new transition and no new event. That closes the gap R-1 named: §9.4 concedes
that `HAND_INIT` can fail to complete when peers hold different accepted T58 events and derive
different `ledger_delta`, and under the old start point nothing covered it from hand 2 onward.

### 8.3 `hand_delay_ms` is not engine state

`n(19) hand_delay_ms` is a **display** delay so a human can see the result. Its preset value is
`PROTOCOL.md` §13's and is not repeated here, and the name is `PROTOCOL.md` §7.2's: the
`hand_delay_sec` that stood in this heading and in the two sentences below it was one value under a
second name in a second unit, which is the shape `hand_deadline_sec = 600` survived in (`G7-S1`).
The protocol does not wait for it, and nothing in
this document ever will: making a display delay an engine state would reintroduce a clock. A GUI
still animating hand `h` simply lags behind a state that has already advanced; that is a rendering
concern.

**The sentence that stood here — *"T47 fires as soon as the terminal event of hand `h` is in the
chain"* — is corrected rather than annotated (K-3).** It is now true only where hand `h` ended at
T46. Where it ended at T45, T47 additionally waits for hand `h`'s boundary checkpoint to complete
(§5.2's checkpoint-8 box), and under total silence the phase is left by T61 instead. **That is not
`hand_delay_ms` under another name and must not be implemented as a delay**: the wait is on a
collective stage of chain content, it ends the moment the last required copy arrives — which on a
healthy table is one round trip, far inside the display delay a GUI is animating anyway — and its
fallback is a timer the protocol layer already runs (`PROTOCOL.md` §8.2). Peers may still publish
hand `h+1`'s key setup immediately; what they may not do is treat the boundary as having passed
before the stage says so, because that is the comparison the checkpoint exists to make.

**`hand_delay_ms` is a term in two durations, and that is not a contradiction with the heading.**
`PROTOCOL.md` §8.2 budgets `hand_delay_ms + crypto_step_timeout_ms` for the hand-boundary stage and
carries `hand_delay_ms` in `HAND_DEADLINE_FLOOR(n)`. Budgeting for a delay a peer is *permitted* to
take is not taking it: no transition here waits, and §2.6's box is where the two durations that read
the field are written down. That is also why the field is in `TableConfig` (§2.3) despite `step`
never reading it.

### 8.4 The timeout certificate — retained specification, not produced in version 1 (D-015)

> **Normative. Nothing in this section is reachable in version 1, and it is kept rather than
> deleted for one reason: it is the part of this machinery that took five review passes to get
> right.** No `TIMEOUT_CERT` is produced (`PROTOCOL.md` §4.8, §8.3), `Event::TimeoutCertificate`
> is deleted from §4.1, the six rows that consumed it are deleted from §5.2, and
> `certified_subjects` is deleted from `TableState` (§2.6, §2.8). **Read everything below as the
> specification a later version implements**, and read §8.4's rules 1–7 in particular as the
> thing that must come back **with** the rows and never after them: an implementer who restores
> the rows without rule 4, rule 6 and rule 7 has rebuilt the N3 attack, and an implementer who
> restores them without `certified_subjects` cannot compute `V` at all and will reach for the
> seat count, which is the D-008 defect by another route.
>
> **What governs a missed deadline in version 1 is §8.5 and T57**, in that order: an action
> deadline by the seat's own auto check/fold, a cryptographic-step deadline by
> `hand_deadline_ms`. Neither needs a voter set, a quorum or a shared clock, which is why
> neither has ever produced a defect of this class.

```rust
pub struct TimeoutCertificate {   // NOT constructed in version 1; retained specification
    pub table_id: TableId, pub hand_id: u64,
    pub subject: SeatIdx, pub kind: DeadlineKind,
    pub sequence: u64, pub parent_hash: Hash,
    pub signers: SeatSet,            // Ed25519 signatures carried in the envelope
}
```

**The required voter set, defined once.** Every rule in this section, and every guard in §5.2,
reads this quantity and nothing else:

```
V(subject) = deck.participants \ ({subject} ∪ certified_subjects)
```

`certified_subjects` is canonical state (§2.6) and holds exactly the seats that a **completed,
valid certificate has already named in this hand**. It is emptied at hand init (§5.3). Nothing
else removes a seat from `V`. In particular **being voted against is not exclusion**: a seat that
some peer has emitted a `TIMEOUT_VOTE` about, and whose deadline never produced a certificate,
stays in `V` and its signature is still required.

Validity, checked by `step` before any effect (T34, T41, T44…):

1. `table_id`, `hand_id` match the current state (§14 replay defence);
2. `sequence` and `parent_hash` equal `state.deadline`'s — so a certificate cannot be replayed
   into a different position in the hand;
3. `subject == state.deadline.subject`;
4. `signers == V(subject)` — **unanimity of every seat still in the voter set**. No quorum
   reduction is permitted, and no seat may be dropped from `V` other than by rule 7;
5. every signature verifies (`verify_strict`) over the canonical bytes;
6. **if `|V(subject)| < 2` the certificate is rejected, whatever its `kind`** — `Action` (the
   `ActionDeadline` of `PROTOCOL.md` §8.3) and `Crypto` (the `CryptoDeadline`) alike. It is inert:
   not accepted, not chained, not evidence, no `Fault`, no `AbortRecord`, no entry
   in `certified_subjects`, and the state is bit-identical afterwards (I21). Neither T34 nor
   T16/T22/T27/T41/T44 is reachable when the voter set is one seat or none. The hand does not end
   here; if it can no longer proceed it ends at `hand_deadline_ms` (T57);
7. on acceptance, and **only** on acceptance of a certificate that passed rules 1–6 with
   `|V(subject)| ≥ 2`, `certified_subjects |= {subject}`. A rejected certificate, an incomplete
   one, and a bare vote all leave `certified_subjects` bit-identical (I21).

**Rule 6 is scoped on `|V|`, not on the seat count, and that is the whole point (D-008).** The
earlier form of this rule read *"if `|deck.participants| == 2`"*, and so did `PROTOCOL.md` §8.3's
two normative sentences. That scoping was the defect. The attack does not need a two-seat table;
it needs a **one-member voter set**, and for as long as `V` could be shrunk by assertion a
modified client could manufacture one at any table size: vote against four seats at a six-seat
table, declare the fifth, and `V` is `{attacker}` — a complete certificate on one signature, the
victim attributed, and, under the pre-D-010 rules, the victim's committed chips forfeited to the
attacker. **D-010 has since removed the chips from that outcome**, so what the attack still buys is
a false attribution record and a voided hand; rules 4, 6 and 7 are kept because a false record
against an honest key is still worth refusing, and because `certified_subjects` still gates T34,
which moves the action. Every protection was scoped on
the seat count, and the seat count was still six, so nothing rejected it. Rules 4, 6 and 7 above
close it in three pieces: the floor follows `|V|`, `V` shrinks only by completed certificates, and
each such certificate had to clear the floor itself. The shrinkage is therefore inductive and
cannot be bought with assertions.

**What unanimity is, and what it does not buy.** With no exclusions yet in the hand,
`|V| = |dealt_in| − 1`; each accepted certificate then costs the set one more member:

| Size of `V(subject)` | What "unanimous" means | Effect |
|---:|---|---|
| 0 or 1 | one seat, or none — either way a single interested party, or nobody | **no effect at all** (rule 6): the certificate is rejected, of either kind. A hand that can no longer proceed ends at `hand_deadline_ms` instead (T57) |
| 2 | both remaining seats | the floor: the minimum at which the certificate says anything |
| 9 | all nine | a ten-seat table's first deadline |

The rows are on the voter set, not on the table. A ten-seat table sits in the last row at its
first deadline of a hand and can be in the first row later in the same hand; a two-seat table is
in the first row always. That is the difference D-008 turns on.

`|V| ≥ 2` unanimity is what stops one peer stealing the action from a player who was about to act.
At `|V| = 1` it degenerates to a single signature by the one party with an interest in the
outcome, which is why D-007 makes the deadline **advisory** there — a UI countdown that produces
no signed state transition — and forbids a fold-effect timeout certificate.
`PHASE0_FIXPLAN.md` §0.3 extends the same reasoning to `kind == Crypto`: the underlying result —
*a peer with no trusted clock and no third party cannot be taken at its word that a deadline
passed* — does not depend on what the certificate does afterwards, and a crypto-deadline
certificate is strictly more valuable to an attacker, because it aborts the hand and attributes the
victim. Under D-010 it no longer takes the victim's chips — the abort is neutral — and the argument
stands on the false attribution alone, which is enough to keep the floor.

**D-008 point 2 is applied literally, and the carve-out that stood here is deleted (M1, D-009
rule 2).** The previous revision let a `kind == Crypto` certificate below the floor still end the
hand, stripped of its attribution, on the ground that liveness required it. That reading is
withdrawn. The ruling, which is binding and is `PROTOCOL.md` §8.3's reading:

> A certificate below the `|V| >= 2` floor is **inert in every document and at every table size**:
> not chained, not evidence, no terminating effect, no `AbortRecord`, no chip movement. It is
> silently ignored. This applies to `kind == Crypto` and to the hand deadline as much as to anything else.

Three things were wrong with the carve-out, and D-010 has narrowed only the first of them.
**(1) It is an effect one signature can manufacture**: a losing heads-up player got a one-message,
on-demand hand void, instead of having to stall the hand for the full `hand_deadline_ms` — visible,
attributable and costly to itself. Under D-010 both the carve-out and T57 restore stacks, so what
the carve-out would now buy is speed and deniability rather than chips; the `hand_deadline_ms` stall is
still the price this document charges, because paying it is what makes the escape *visible*, which
is the only mitigation D-010 leaves.
**(2) The accepted certificate is a chained event naming its subject**, so it is a transcript
record against a seat that nothing establishes failed — `PROTOCOL.md` §8.3 concedes there is no
artefact that settles the race when a voter lies. **(3) It is consensus-critical.** It is the
engine's accept predicate for an event a modified client can emit at will, and `PROTOCOL.md` gave
the opposite answer; two conforming peers would have reached different phases from the same event,
produced two `state_hash` values at the next checkpoint, and faulted the table with `cause = 4` —
the one fault the corpus classifies as unresolvable. Liveness is not owed here: `SPEC_CS.md` §19
ranks security above finishing a hand conveniently.

What replaces it is not a gap. A hand that can no longer proceed ends at `hand_deadline_ms` with
`attributed = []`, `cert_hash = None` and stacks restored, carried by `Event::HandDeadlineAbort`
and **T57** — the certificate-free carrier the previous revision recorded in §13 as missing, and
respecified in §4.1 as a witness-independent terminal stage so that it can actually fire (P3). The
chip arithmetic is identical to the deleted branch's; what changes is the trigger and the wait,
one `hand_deadline_ms` instead of one `crypto_step_timeout_ms`. **The rage-quit
escape is now open everywhere, not only below the floor**, because D-010 makes every abort neutral;
that is D-010's stated and accepted cost, it is recorded in §8.6, §8.7 and §12, and the below-floor
chip question — whether an unfinishable hand restores or forfeits — is **closed by it** (§11).

**What unanimity does not settle: the race between a late action and a certificate.** The
previous claim here — *"There is never a state in which both a valid action and a valid certificate
exist for the same parent"* — is **false** and is withdrawn. Nothing in the protocol records that a
voter accepted an action, so nothing prevents a voter from voting for a timeout it knows did not
occur. The accurate statement:

> If at least one required voter is honest and saw the action, no certificate forms and the action
> stands. If every required voter is dishonest, or none saw the action, a certificate forms. There
> is no artefact that settles the race when a voter lies about what it saw.

A vote and the subject's own action are two events signed by **two different keys**, so they are no
proof against anyone, and the equivocation predicate of `PROTOCOL.md` §5.2 does not apply to the
pair. See `PROTOCOL.md` Q-08 — the question `PHASE0_FIXPLAN.md` raised as `OQ-C` — which asks whether a
voter should have to publish a signed `ACTION_SEEN` before it may vote; that is a design change and
is **not adopted**.

**A seat's own contribution and its vote are not mutually exclusive.** A vote carries
`event_class = 1` and therefore occupies its own slot rather than the stage slot it is about
(`PHASE0_FIXPLAN.md` §0.1, `PROTOCOL.md` §4.8). A seat may hold both its own contribution at a
collective stage `s` and a vote about stage `s`; they are different event classes and neither
implicates the other. Any reading of T27, T41, T44 or this section that treats the pair as an
equivocation is wrong.

**Nor are a voter's two votes about two subjects at one stage (M2, D-009 rule 1).** `PROTOCOL.md`
§8.4 specifies two simultaneous subjects at one stage as a normal state — every cryptographic stage
is collective, so two seats failing at one stage is the ordinary shape of the case — and §4.8
entitles a voter whose timers expired against both to emit a vote about each. `PROTOCOL.md` §5.2's
slot key therefore carries the vote's subject, so the two votes are in two slots and no proof can
be built from them. **May a voter vote about two subjects at one stage? Yes, and the question is
closed rather than defaulted**; it was open only while §4.8's emitter gate ("has not accepted any
valid event for that stage") and its receiver check ("…from `subject_seat`") stated different
conditions, and §4.8 now makes the per-subject condition normative in both places (M5). Under the
withdrawn reading no `kind == Crypto` vote was ever legal at a collective stage and the whole
cryptographic-step deadline path was unreachable, which would have made T16, T22, T27, T41 and T44
dead rows. They are not; the path exists.

**Why the quorum is never reduced.** Two clients both genuinely gone cannot sign each other's
certificates, so no action certificate forms. That is correct behaviour, not a deadlock: the
escalation is the `hand_deadline_ms` limit, which needs no certificate from anybody (T57).
Reducing the quorum to make progress would reintroduce exactly the thing D-006 was designed to
prevent — a subset of players asserting facts about another player.

"The *remaining* signers" means `V(subject)` as rule 4 defines it, and the only way a seat becomes
non-remaining is rule 7. A seat that is silent, partitioned, slow, or merely the subject of
somebody's unmet vote is **still a required signer**, and a certificate that omits its signature
is invalid however plausible its absence looks. This is the load-bearing half of D-008: the
temptation to treat "obviously gone" as "excluded" is precisely what turns a partition into a
confiscation, and the engine has no way to tell the two apart.

**The certificate arrives as a collective stage.** `PROTOCOL.md` §4.8 as amended makes
`TIMEOUT_CERT` a **collective** stage whose required emitter set is `V(subject)`: each voter emits
its own certificate carrying the same votes in the same order, and `stage_hash` is the collective
form over `event_class = 2`. There is no timer, no tie-break and no assembler privilege, and
`CERT_SETTLE_MS` is deleted (§3.1). The engine's `TimeoutCertificate` event is **unchanged** by
this, but `signers` is now derivable two ways — from the stage's own emitter set and from the
embedded votes — and `step` requires **both to agree**; a mismatch is a `Rejection`.

One residual, stated rather than hidden: a stage can now close two ways, by the subject's own
event or by certificates, and which one happened is chain content. Two peers agree on it only if
at least one required voter is honest and refuses to vote after accepting the action. At
`|V| >= 2` that is the honest-voter assumption already in force. At `|V| < 2` there is no such
voter, and rule 6 means no certificate takes effect, so the ambiguity cannot arise.

**OPEN QUESTION Q3, still open, now with a defined interim rule.** The hand-deadline certificate's
signer set when several seats are simultaneously unresponsive. `signers == V(subject)` becomes
unachievable if two seats are gone. The construction the previous draft called
"defensible" — a certificate naming a *set* of subjects — is **not** adopted, and neither is
`PROTOCOL.md` §8.4's earlier fallback of attributing every non-voting seat: a seat that did not
vote may be silent, partitioned or merely slow, and attributing a partitioned honest seat puts a
false record in the transcript for nothing. Under D-010 it would take no chips — which is why the
question below is now smaller than it was — but a rule that names the wrong peer is still a rule
this document will not adopt.

**D-008 removes the third candidate answer as well.** `PROTOCOL.md` §8.4 previously resolved the
simultaneous case by *excluding* from `V` any seat already named as the subject of an outstanding,
older unmet deadline. That rule is deleted: it made `V` shrinkable by assertion, which is the N3
attack, and it bought nothing the interim rule below does not already provide. So the simultaneous
case now always falls through to the interim rule, and no exclusion happens until some certificate
actually completes at `|V| ≥ 2` (rule 7).

The **interim rule**, which is a safe default and not a solution: if unanimity cannot be reached
before `hand_deadline_ms` expires, the hand aborts with `kind = HandDeadline`, `attributed = []`
and stacks restored. **That rule is T57**, and since D-009 rule 2 it is no longer only the
simultaneous-subject case's answer — it is the answer for every stall no certificate with an effect
can settle, `|V| < 2` included. Its settlement is §8.6's one rule, which under D-010 is the same
rule every abort now takes: every seat gets exactly its `committed_hand` back and no chip crosses
between seats (invariant **I27**). Nobody is named. The cost, recorded: two
colluding seats can void a hand for free by going silent together. Q3 stays open, is carried as
**OQ-E** in `THREAT_MODEL.md` §9.2 as a **blocking** question for Phase 4, and is the same question
as `PROTOCOL.md` Q-02.

**What is now closed within Q3, and what is not.** An implementer needs no guess about *how the
hand ends* when several seats are unresponsive: T57 ends it, with a named carrier, a defined guard
and a defined settlement. What stays open is whether a multi-subject deadline should ever
**attribute** anybody. Under D-010 that question is now narrower than the sentence below it was
written for: attributing somebody would put a name in the transcript, not chips in a stack, so it
no longer decides who pays. It is kept open because a later version may give attribution teeth
again, and the shape of the artefact has to be right before it does — the question that would let
the protocol charge a rage-quitter rather than
restoring every stack — and that decision is `PROTOCOL.md` Q-02's, not this document's. Building
the interim rule now costs nothing if it is answered later: an answer adds a transition and a
non-empty `attributed`, it does not change T57's shape.

### 8.5 Auto check/fold, and the fold-out escape hatch

**The auto-action is emitted by the seat itself, and that is D-015's substantive change to this
document.** When a seat's own action deadline expires its client signs **`Check` if
`to_call == 0`, else `Fold`** — never fold a hand that could check for free (D-006 §1) — and
gossips it as an ordinary `ACTION_CHECK` or `ACTION_FOLD` at that seat's own stage. The engine
consumes it through **T29–T33**, exactly as it consumes a human's action, and it is a real entry
in `history`. **This is single-writer by the seat that owed the action**, so there is no vote, no
voter set, no unanimity, no shared clock and no way for any other peer to produce one; D-006
point 2's two halves — *"a real, signed protocol event in the transcript"* and *"every peer
derives it identically"* — are satisfied by the first alone, and the tension the certificate
existed to resolve does not arise.

**The paragraph that stood here read *"On a valid action-timeout certificate (T34) the engine
applies…"* and is withdrawn with T34** (§5.2, D-015).

**`was_auto` and `consecutive_auto_actions` are the one behavioural loss, and it is stated
plainly.** `was_auto` was set by T34 from the certificate; an action a seat signs itself is
indistinguishable on the wire from one its human took, so **no peer can set `was_auto` for
another seat**, `consecutive_auto_actions` is never incremented by any peer's observation, and
**no seat is ever marked `SittingOut` by the deadline path**. `config.auto_action_limit` is
therefore not reached in version 1. Two consequences, both accepted:

* A player who walks away is **not** automatically sat out after three auto-folds; their client
  keeps folding them and their stack drains on the blinds instead, which is the same end state
  by a slower route and is D-005's absent-seat behaviour unchanged.
* The transcript no longer marks which actions were automatic. That is a **display** loss, not a
  correctness one: no rule in this document reads `was_auto`, and the field is kept in `history`
  for the seat's **own** client to set locally, where it is a per-receiver quantity and must not
  enter `PublicTableState` (D-012). An implementer who hashes it has reintroduced exactly the
  class D-012 forbids.

A seat may still sit out **voluntarily** at a hand boundary (`PlayerSitsOut`, T59), which is a
genuine choice by that seat and needs no certificate; and a client that is gone entirely emits
no auto-action either, stalls the hand to T57, and is dropped from the next hand's required set
by D-013.

**An opponent who is present but simply refuses to act cannot be punished inside the protocol at
all, at any table size.** This paragraph used to be scoped on `|V(subject)| < 2` and to say that
heads-up was its common case; **under D-015 it is unscoped**, because there is no certificate at
any `|V|`. The hand ends at `hand_deadline_ms` with nobody named (T57), the stalling seat is then
outside `signed_this_hand` so D-013 skips it from the next hand and its stack drains on the
blinds, and the only immediate remedy for the other players is to leave the table. That is the
same statement §9.1.0 item 1 of `THREAT_MODEL.md` made about heads-up, now true everywhere — and
it is a widening of a *stated limitation*, not of an exploit: the certificate's own effect was
one hand's action, D-010 had already taken its chips away, and what an attacker gains by the
widening is a slower loss rather than a new one.

T30 is the escape hatch that D-005 case 1 names explicitly: if folding leaves exactly one live
seat, the engine goes **straight to `Settling`** with no reveal request at all, even when a peer
has vanished and is blocking every possible opening. A player who quits to escape a losing pot
does not escape if the others simply fold.

### 8.6 Abort, attribution and restoration (D-005, as revised by D-010)

Barnett–Smart is `n`-of-`n`: any dealt-in seat that stops publishing makes it impossible for
**anyone** to open any further card, board included (`MENTAL_POKER.md` §8). The hand cannot be
finished by the remaining players, and the standard robustness upgrade — `t`-of-`n` threshold
ElGamal — is **refused**, because with `t < n` any `t` colluding players could decrypt every hole
card at the table, violating `SPEC_CS.md` §35 and §19 directly.

So the abort path is the answer, and §19 prescribes its shape: timeout, abort, evidence, penalty.
**D-010 keeps the first three and deletes the fourth**, for the MVP and until a new numbered
decision says otherwise.

```rust
pub struct AbortRecord {
    pub hand_id:            u64,
    pub phase_at_abort:     Phase,
    pub kind:               AbortKind,   // NoKey | InvalidShuffleProof | NoShuffle |
                                         // NoDealTokens | NoBoardTokens | NoShowdownToken |
                                         // InvalidRevealProof | HandDeadline |
                                         // UnobtainableEvents | StateDivergence |
                                         // ProvenCheat (D-014, T64)
    pub attributed:         Vec<SeatIdx>,
    pub owed:               Vec<(SeatIdx, Requirement)>,  // exactly what each seat failed to produce
    pub observed_by:        SeatSet,                      // certificate signers
    pub sequence:           u64,
    pub parent_event_hash:  Hash,
}
```

**Every `AbortKind` maps to a `cause` `PROTOCOL.md` §4.10 defines, and the mapping is written down
because the previous revision had a kind with no `cause` at all (G2).** `AbortKind` is the engine's
finer classification; `cause` is the wire's. Several kinds share a `cause`, which is fine; a kind
with no `cause` is a defect.

| `AbortKind` | Producing transition | `cause` | `attributed` | `cert_hash` |
|---|---|---|---|---|
| `NoKey` | T16 | `1` | `[subject]` | `Some` |
| `NoShuffle` | T22 | `1` | `[subject]` | `Some` |
| `NoDealTokens` | T27 | `1` | `[subject]` | `Some` |
| `NoBoardTokens` | T41 | `1` | `[subject]` | `Some` |
| `NoShowdownToken` | T44 | `1` | `[subject]` | `Some` |
| `HandDeadline` | T57 | `1` | `[]` | `None` |
| `UnobtainableEvents` | T60 | `1` | `[]` | `None` |
| `InvalidShuffleProof` | T21 | `2` | the shuffler | — (`n(3) evidence` carries the failing proof) |
| `BadKeyProof` | T15 | **no exact value — see below** | the seat | — (`n(3) evidence` carries the failing `DECK_INIT`) |
| `InvalidRevealProof` | T48 | `3` | the seat and the index | — (same) |
| `StateDivergence` | T54 | `4` | `[]` | `None` |
| `ProvenCheat` | T64 | **see below** | `[subject]` | `None` (— `n(3) evidence` carries the message the subject signed) |

**`ProvenCheat` is the second kind whose `cause` is not exactly defined, and it is named here for
the same reason `BadKeyProof` is (D-014).** §4.10's enumeration is `1` failure to publish, `2`
invalid shuffle proof, `3` invalid reveal proof, `4` divergence. Where the evidence *is* a failing
shuffle or reveal proof the mapping is exact — `2` and `3`, and those two aborts already exist as
T21 and T48, which is why D-014 adds no abort path for them and only adds the removal. Where it is
a bad signature, a non-canonical encoding, an out-of-range field or a post-checkpoint illegal
action, none of the four fits by its label. **`cause = 4` is specifically wrong** and is worth
saying: §6.4 makes `4` close the table (§9.3 condition 0.5), and a D-014 removal leaves the table
**playing** — that is the whole difference between removing a cheat and faulting a table.
**Named default the engine builds on:** `cause = 2` with the offending message in `n(3) evidence`,
on §4.10's own grouping of `2` and `3` as *"the two causes a single chained event proves on its
own"* — which is D-014's tier-1 definition written a decision early. Whether §4.10 widens `2` or
adds a value is `PROTOCOL.md`'s, and it is part of **D-014-3**. Until then a reader must not infer
from `cause = 2` that a shuffle was involved, and this is now the second row that carries that
warning.

**Attribution here is not the `attributed` field H1 was about, and the distinction is the whole of
why D-014 is admissible where D-010 was not.** `attributed` on a certificate-borne or
deadline-borne abort is a per-receiver record of *who was heard*, which is why D-012 forbids
deriving a `status` from it and why T46's marking was deleted. `attributed` on a `ProvenCheat` is
the signer of a message in `n(3) evidence` that every peer holding the abort also holds. The field
is the same field; the quantity behind it is not. **The test that separates them is one question:
can two honest receivers of the same abort body disagree about the name?** For `NoKey` they can.
For `ProvenCheat` they cannot, because the name is recoverable from the evidence in the body — and
that is exactly the property D-014 calls self-authenticating.

`UnobtainableEvents` is new in this revision and is `PROTOCOL.md` §6.3 case (b): the transcripts
still differ after reconciliation because one peer cannot obtain events every holder refuses to
serve. It takes `cause = 1` because that is what §6.3 assigns it, and it is the **only** kind that
records a case (b) terminus — which matters, because it is also the only unresolved divergence that
does **not** fault the table. `Equivocation` is deleted: it had no `cause`, since `5` is deleted
from the wire and not reused, and nothing consumes an `EquivocationProof` (G2, §5.2).

**`BadKeyProof` is the one kind whose `cause` is not exactly defined, and it is named rather than
rounded off.** §4.10's enumeration is `1` failure to publish a required cryptographic contribution,
`2` invalid **shuffle** proof, `3` invalid **reveal** proof, `4` divergence. A `DECK_INIT` whose
key-share proof does not verify is none of those by its label, and it is not `1` — the seat did
publish, it published something invalid, which is the opposite failure. **Named default the engine
builds on:** it is emitted as `cause = 2` with the failing `DECK_INIT` in `n(3) evidence`, because
§4.10 groups `2` and `3` as *"the two causes a single chained event proves on its own"* and that
description fits exactly. Whether §4.10 widens `2`'s label to cover every invalid proof in the
deck-setup chain, or adds a value, is `PROTOCOL.md`'s (§13's second objection). Until then a reader
should not infer from `cause = 2` that a shuffle was involved.

This is a signed protocol event in the transcript, so every participant can independently verify
*who* failed, *what* they owed and *when* — `SPEC_CS.md` §19's "evidence of which peer failed",
made checkable rather than asserted. Attribution is by **missing artefact**, never by liveness
guesswork: the missing `(seat, card_index)` reveal token is publicly visible, and everyone else's
signed events prove they did their part. **`owed` is empty on the T57 path** and carries the
certificate's own requirement on the T27 / T41 / T44 paths; the reason is §4.1's, and it is that a
set peers disagree about must not enter a hashed body.

#### The chip rule: one branch, for every cause (D-010)

```
for s in all seats:  stack[s] += committed_hand[s]
```

That is the whole of it. Every seat ends the aborted hand with `stack[s] == start_stack_this_hand[s]`
(I5, I27), no chip crosses between seats in any direction, and `Σ stack` before and after the
accepted `AbortSettle` is equal (I2). It does not branch on `AbortKind`, on `attributed`, on `|V|`,
on the seat count or on who published what. **T46 applies it unconditionally.**

**What this deletes, listed so the deletion can be checked rather than taken on trust.** The
previous revision carried, and this one does not:

1. **The forfeiture formula** — `culprits`, `forfeit = Σ committed_hand[c]`, `others`, `base`,
   the pro-rata `share[s] = forfeit * committed_hand[s] / base`, the clockwise remainder
   distribution from `succ(button_pos)`, and its three branches (`culprits is empty`,
   `others is empty`, and the redistribution). All gone; the odd-chip tie-break is no longer
   reached from here at all, and §7.6's A8 rule is now used only at settlement.
2. **The `culprits == {}` special case.** There is no special case, because there is only one case.
   I27 survives as an invariant but is now implied by the rule rather than carving an exception out
   of it.
3. **The split of `AbortKind` into "runs the formula" and "does not".** `StateDivergence` and
   `HandDeadline` were the two exceptions; now no kind is an exception.
4. **The precondition paragraph** — *"a seat's chips are forfeited only when `abort.attributed`
   names it, and a certificate may name a seat only when `|V| ≥ 2`"* — and its normative box, *"no
   chip crosses between seats on the strength of a certificate whose `|V| < 2`"*. Both are now
   vacuous: no chip crosses between seats on the strength of anything.
5. **The DoS cost paragraph** — *"an opponent able to knock a player offline right after a big bet
   would collect that bet"*. That incentive existed only because forfeiture existed. Knocking a
   peer offline now costs the attacker a hand and gains it nothing, which is the same as any other
   dropped connection.
6. **Every automatic consequence of attribution.** The previous revision kept one — T46's
   `status := Absent` — and this one does not: it is deleted under H1 and D-012, so the list has no
   exception left. The next block says why.

#### Attribution is evidence, and it now causes no state change at all

D-010 point 2: the transcript still records which peer failed, and nothing acts on it
automatically. Concretely, in this engine:

* an `AbortRecord{attributed: [X]}` moves **no chips**, in either direction;
* it produces **no block-listing and no unseating**, here or at any other layer. The sentence this
  bullet used to carry — that `PROTOCOL.md` §5.2 blocks an equivocator's key at the transport layer
  — was **false**, is deleted, and was one of G2's three findings: §5.2.4 says block-lists nobody,
  D-010 point 3 forbids the eviction, and D-011 rule 3 binds every layer including
  `NETWORK_STACK.md`. There is no peer-removal transition in this document, T11's was the last one
  and it is deleted (G4), and D-010 point 3 forbids adding one;
* it produces **no penalty of any kind** the protocol applies. `SPEC_CS.md` §18 already forbade
  claiming more than visibility in play money; D-010 makes visibility the entirety of it;
* it changes **no seat's `status`**. The one exception that stood here — T46 setting
  `status := Absent` for every seat in `abort.attributed` — is **deleted (H1, D-012)**. It was
  defended as D-005's absent-seat rule rather than a penalty, and that defence was sound about
  *penalties* and beside the point about *canonicality*: `attributed` is a field of whichever
  `HAND_ABORT` copy a receiver accepted, the terminal stage is witness-independent and has no
  abort-versus-abort precedence rule, so two honest peers accept copies with different `attributed`
  and derive different `dealt_in` and `bb_seat` for the next hand. The reasoning, what it costs and
  what is left in its place are in §5.2's note under T58/T59; the invariant is **I30**;
  `PROTOCOL.md` §4.10's *"no receiver may derive a seat's state from it"* is now true with no
  exception anywhere in the corpus.

**So an abort changes exactly three things, and a status is not one of them:** stacks are restored
to `start_stack_this_hand` (the rule above), `settlement.aborted` is set, and the `Fault` and
`AbortRecord` entries already produced by the transition that reached `HandAborted` stay in the
transcript. Everything else about the next hand is a function of the previous hand boundary, which
is what makes `GENESIS(k+1)` derivable from `GENESIS(k)` alone (`PROTOCOL.md` §3.1).

**What that costs, and the cost this document stated wrongly for two passes.** The previous
revision said the cost was *"a stall that repeats every hand"*, and read that as slow. It was not
slow, it was **frozen**: nothing removed the silent seat from `HAND_INIT`'s required emitter set,
T46 restores every stack so no seat could ever bust, and §9.3's end conditions all read a stack, a
fault or an empty table — so none of them could fire. The table repeated one identical hand of one
`hand_deadline_ms`, forever, and no participant had an action that changed it (`PLAYER_SIT_OUT` and
`PLAYER_LEAVE` are single-writer by the seat itself, so the only human who could sit the silent
seat out was the silent one). That was J2 and it is corrected here rather than annotated.

**Under D-013 the cost is bounded and the seat drains.** The abort still changes exactly the three
things above; what changed is *elsewhere*, at the next hand init, where `dealt_in` reads
demonstrated participation instead of status (§5.3 step 4). The silent seat is skipped from the
hand after it stops signing, keeps its seat and its stack, pays its blinds and antes, and busts.
§12.1 gives the bound: one `hand_deadline_ms` if it vanished at a hand boundary, two if it vanished
mid-hand, and normal speed thereafter.

**What an implementer still must not do is close it locally.** Not from the abort's `owed` list and
not from its `attributed`, because both are quantities peers disagree about (§4.1, P3, D-012), and
not from a dropped connection or a missed heartbeat, because those are not chain content at all.
Deriving canonical state from any of them forks the chain, which is a strictly worse failure than a
slow table. The one basis that is admissible is the one D-013 names, and the single input to it
that is not agreed by construction is **Q8** in §11.

**One attribution path does not depend on `V` at all, and it is still correct that way:** the
direct crypto verdicts T15, T21 and T48, where the artefact the seat published is itself invalid
and every peer checks it against the same proof. That is not a vote, so it needs no floor. The
second path that stood here — `AbortKind::Equivocation`, T55 — is **deleted** (G2): nothing
consumes an `EquivocationProof` in any document (`PROTOCOL.md` §5.2.4), the kind has no `cause`
value on the wire, and the abort it produced ended a hand on an unchained event. `AbortKind` has
lost the variant.

**D-009 rule 1 still binds this document, and deleting the consumer did not retire it.** With T55
gone, no transition here reads a proof, so no *engine* outcome now depends on §5.2.3's property.
What still depends on it is the record: a proof naming an honest key is a permanent, signed, public
accusation, and D-010 point 2 keeps proofs precisely so a human or a later version can adjudicate
from them. An artefact kept for adjudication that mandatory honest behaviour can manufacture is
worthless in exactly the case it exists for. So **every chained emission this document specifies is
checked against `PROTOCOL.md` §5.2.1's slot key before it is added**, this document reproduces no
part of that key (D-011 rule 2), and the standing test is unchanged: the honest-peer mirror of
`SPEC_CS.md` §25's `CheaterEquivocation`. That check is what rejected P1's shape rather than
patching it, and `PROTOCOL.md` §4.11's per-type table rows 33, 34 and 36 record its results for the
three emissions that needed a rule of their own.

#### The cost of D-010, stated plainly because §18 requires it

**The rage-quit escape is open, on every path, at every table size.** A player who is losing a big
pot can go silent; the hand aborts; the chips come back. There is no longer any configuration —
not `|V| ≥ 2`, not a completed certificate, not a divergence — in which quitting
costs what folding would have cost. D-005 closed that with forfeiture and **D-010 reopens it
knowingly**, because the measured alternative is worse: four adversarial passes produced four
different ways for the forfeiture machinery to take chips from an *honest* player (A-4, N3, M2,
P1), and an exploit that harms an honest player is worse than one that lets a dishonest player
escape a loss.

What is left in its place is not enforcement and must not be described as such:

* every abort is in the transcript, signed, permanently, with the stalled stage and what it was
  owed visible to anyone who reads the chain;
* the client shows a per-identity abort count in the lobby, so players can decline to sit with
  someone who does it;
* sitting a repeat aborter out is a **user** decision, never a protocol action.

`SPEC_CS.md` §18 asks for exactly this trade to be explicit rather than hidden: some cheating is
prevented, some is detected, and this one — escaping a losing hand — is merely visible.

### 8.7 The rule that overrides convenience

> **No anti-disconnect mechanism may expose an absent player's hole cards.**
> (`SPEC_CS.md` §19; D-005.)

Enforced in three independent ways:

1. **Structurally.** A reveal token for seat `P`'s share can only be produced from `P`'s secret
   key, which exists only in `P`'s `LocalView`. No other client has the material. This is a
   property of the construction, not a policy the engine could get wrong.
2. **By the state machine.** `HandAborted` issues **no** `RequestOpen` effect. An aborted hand
   opens nothing that was not already open (invariant I23).
3. **By refusing the tempting fix.** No `t`-of-`n` threshold, ever (`MENTAL_POKER.md` §8).

The permanent limitation this leaves, which `THREAT_MODEL.md` must state and which no part of this
document may be read as denying: **a malicious player can always force a hand to abort by going
silent.** It cannot steal cards by doing so. Whether it can escape a losing pot by doing so used to
depend on the path, and the three-way split that stood here — escape closed at `|V| >= 2`, open
below the floor, open on the divergence path — is **deleted**, because under D-010 the answer is
uniform:

> **The escape is open on every path, at every table size, for every cause.** Every abort restores
> every stack to `start_stack_this_hand` (§8.6), so there is no configuration in which quitting
> costs what folding would have cost. A player who is losing may stall, or drop, and get their
> chips back. This is D-010's accepted cost and it is stated here, in §8.6 and in §12 rather than
> being left to be inferred from the arithmetic.

What varies between the paths is only the **price in time and visibility**, and that is worth
keeping straight because it is the whole of the remaining deterrent:

* **On the `hand_deadline_ms` path (T57)** the quitter must stall for the full deadline — never less
  than `HAND_DEADLINE_MIN(n)` (`PROTOCOL.md` §8.2) — and the
  stall, the stage it stalled at and the abort are all in the transcript.
* **On a certificate path at `|V| >= 2`** the hand ends sooner and the quitter is *named* in
  `attributed`. It is named and nothing more: since H1 the naming no longer marks the seat `Absent`
  either, because the copy a peer accepted is a per-receiver quantity (§8.6, D-012, I30). Being
  named changes nothing about the next hand. **What does change it, under D-013, is the quitter's
  own silence**: it is dealt out of the hand after the one it stopped signing in, and thereafter
  the blinds take its stack whether any abort ever named it or not (§5.3 step 4). That is not a
  penalty the protocol applies to the record — it is the seat no longer being there, and no
  attribution, certificate or proof is consulted to establish it.
* **On the divergence path (T54)** a single peer that publishes a `state_hash` it did not derive
  ends the hand at once and also faults the table, so the escape is fast but costs the attacker the
  table it was playing at. `THREAT_MODEL.md` X29 carries it.

Beyond the chips it is a griefing vector, and the only mitigations are social — a visible count of
attributable aborts per identity, and other players declining to sit — never cryptographic. In play
money that visible count is the whole penalty, and `SPEC_CS.md` §18 forbids claiming more.

---

## 9. The tournament layer

### 9.1 `RATED_SNG_POKERTH_V1`

**The preset's values are `PROTOCOL.md` §13's, and they are not written down here — `G7-S1` and
`G7-S5`.** §13 is the single normative home for every two-sided constant in the corpus: a client is
built from it, and a `LOBBY_TABLE_AD` claiming the name `"RATED_SNG_POKERTH_V1"` is checked against
it field by field by `PROTOCOL.md` §7.2 rule 3, which makes the name a claim about exactly those
values. The provenance is `POKER_RULES.md` Part B — PokerTH's `GAME_TYPE_RANKING` constants, which
its `ServerGame::CheckSettings` enforces, plus the `[OUR CHOICE]` values that have no PokerTH
analogue — and it is recorded in §13 beside the values it justifies.

**What stood here was a second listing of twenty-one values, and it had drifted.** It was written
in seconds where §13 is in milliseconds, it omitted `crypto_step_timeout_ms` entirely while §9.4
enforced a bound whose largest term is a multiple of it (`G7-S2`), and it carried
`hand_deadline_sec = 600` — a figure `PROTOCOL.md` §8.2 had already shown to be below
`HAND_DEADLINE_FLOOR(10)`, and below that floor's *action* term taken alone. A client built from
this section would have aborted **every** hand at T57 with `cause = 1` and `attributed = []`, and
it could not have joined a §13 client's table either, since `hand_deadline_ms` is a part of
`table_params_hash` (`PROTOCOL.md` §3.1).

**The listing is deleted rather than corrected, and the reason is the whole lesson of `G7-S1`.**
A second copy of a preset value cannot be tested: the healthy-table walk passes at `600 000` too,
because what that figure cannot pay for is the *human* action term and not the protocol's, so no
timing check in this corpus would ever have named it. A corrected digit buys a copy that drifts
again at the next change; a citation buys one place to change. The same treatment is applied to
every other numeric restatement of a table parameter in this document, found by grep over the
whole file rather than by a section list — a section list is what let this one survive the
`G4-P3-e` sweep.

What this document says about the preset is all of the following, and no number is in it:

* the fields the engine reads are §2.3's `TableConfig`, each carrying its `LOBBY_TABLE_AD` field
  number;
* `hand_delay_ms` is display-only and is not an engine input (§8.3);
* `hand_deadline_ms` is a per-table parameter and never a constant, bounded below by
  `HAND_DEADLINE_MIN(seats)` and above by `n(17)`'s cap (`PROTOCOL.md` §8.2, §7.2), and §9.4 is
  where this engine checks it;
* `action_grace_ms` is a **consensus constant, not a UX detail**: every peer times out
  independently with no shared clock, so all peers must use the identical value or they will
  disagree about whether a deadline expired. It is part of the signed advertisement (`n(15)`) and a
  part of `table_params_hash`;
* the preset is not playable by the MVP, which is §9.5.

### 9.2 Blind level — derived, never incremented

```
e               = config.blind_raise_every_hands
level(h)        = 1 + (h - 1) / e                        // integer division, h from 1
small_blind(h)  = min(config.first_small_blind * 2^(level(h) - 1), config.small_blind_cap)
big_blind(h)    = 2 * small_blind(h)
```

**Three config fields and no literals (D-011 rule 2).** The form that stood here wrote the preset's
`11`, `50` and `50_000` into the formula, which made it a fourth copy of three values
`PROTOCOL.md` §13 owns and made it silently wrong for every `CUSTOM` table — the only kind the MVP
ships (§9.5). Level 1 covers hands `1 .. e`, level 2 hands `e+1 .. 2e`, and so on, until the
doubling passes `small_blind_cap` and is clamped there for the rest of the tournament.
`blind_raise_every_hands >= 1` is what makes the division defined and §9.4 is where the engine
checks it. `RATED_SNG_POKERTH_V1` derives its cap from the *starting* player count (`PROTOCOL.md`
§13); a `CUSTOM` table advertises the cap directly. Either way it is a signed parameter and
nothing recomputes it as seats bust, so it is a constant for the whole tournament.

`level`, `small_blind` and `big_blind` are cached in the state for cheap access, but they are
**computed from `hand_id`** at every hand init and asserted against the closed form (invariant
I25). They are never incremented, because an incremented counter can drift between peers after a
rejected or replayed event and a derived value cannot.

### 9.3 End conditions

Evaluated at T47, **after hand init has run** (§5.3) — step 0 has applied the announced seat exits,
step 2 has marked the busts and step 4 has computed `dealt_in` — in this order:

0. **Table empty** — no seat is occupied. → `TableClosed`. This is the case "every seat has left":
   each departure was announced by T58 and applied in step 0, so by the time the conditions are
   evaluated the seats are `Empty` and there is nothing to pause for.
0.5. **Table faulted** — `table_faulted` (§2.6) is set. → `TableClosed`. **This condition is new
   and it is what makes `PROTOCOL.md` §6.4 reachable**: §6.4 is normative that `cause = 4` closes
   the table — *"it is closed, no further hand is dealt"* — and this document had no condition that
   read it, so a `StateDivergence` abort settled at T46 and T47 dealt the next hand, contradicting
   the owner document. Only **T54** sets the flag. `PROTOCOL.md` §6.3 case (b), which is **T60**,
   deliberately does not: a peer that could not obtain events costs the table a hand, not the
   table. And no timeout sets it, so `SPEC_CS.md` §4 and D-006 §5 — no timeout ends the game —
   are untouched: the only way to reach this condition is a divergence that survived
   reconciliation with byte-identical transcripts, which is a client-disagreement bug or a lie,
   never a slow peer. It is numbered `0.5` rather than renumbering `1`–`4`, on the same rule that
   retired transition numbers 8, 12, 55 and 56.
0.6. **Solitary divergence not reconciled** — `solitary_contradicted` (§2.6) is set. →
   `TableClosed`. **This condition is what stops K-9's freeze from being undone by the timer that
   exists to unfreeze phases.** T62 froze this peer because another seat signed a chained event of
   a hand this peer dealt believing itself the only required emitter; T57 or T61 then ends that
   hand on the deadline, T46 restores, and without this condition T47 would deal the next one —
   solitary again, contradicted again, another whole `hand_deadline_ms` again, forever. **Since N1 the latch has a third
   setter and the clearing rule has a conjunct**, and both belong here because both decide whether
   this condition is ever reached. The setter is **T50 when `|checkpoint.required| == 1`**: a peer
   whose checkpoint required one signature — itself — can be contradicted by a *`state_hash`
   mismatch* as well as by an out-of-set event, and before N1 that path set no latch, so the table
   thawed and re-froze at the next checkpoint 8 for ever. Where two or more seats were required,
   nothing changes: the mismatch costs one hand and the table plays on. **Since `P1` that setter is
   also *reachable*, which is a separate claim from its being correct and was the whole of `P1`'s
   second face**: the copy that contradicts a boundary checkpoint usually arrives after the boundary
   has been left, and with a single checkpoint slot the required set the conjunct reads had already
   been overwritten by the next hand's first checkpoint — so N1's conjunct was right and unevaluable,
   and this condition was never reached by that route at all. §2.6's `boundary` slot is what makes
   the guard evaluable, and it is the only thing that changed. The conjunct is on **T53**, which is still the only thing that clears the latch, and which
   now requires the reconciliation stage to carry **two distinct signers** — because a
   reconciliation set that is `{self}` is a stage the frozen peer completes alone, agreeing with
   itself, and a freeze that lifts itself is not a freeze. So the latch is cleared only where the
   reconciliation stage completed carrying one value **signed by somebody else too**, i.e. where the
   two peers now hold the same state and the seat that contradicted this one is back in `P`.
   It is numbered `0.6` on the rule that numbered `0.5` and retired transition numbers 8, 12, 55
   and 56. It does **not** set `table_faulted`: that flag is T54's alone and means
   `PROTOCOL.md` §6.4's `cause = 4`, which this is not.
1. **Tournament won** — exactly one seat has `stack > 0`. Append the remaining seats to
   `finish_order`, set `settlement.tournament_winner`, → `TableClosed`.
2. **Nobody dealt in** — `|dealt_in| == 0` → `Paused`. **This is §5.3 step 9's own first branch and
   not a second predicate for the same question (L7).** What stood here read
   `|{s : status == Active ∧ stack > 0}| == 0`, a status-computed progress path three lines from a
   participation-computed one, and it is the mirror of the defect **K-7** deleted from T59's
   `Paused` exit one pass earlier. It was inert — the old condition strictly implies the new one,
   so the table paused either way — and *"inert because one predicate happens to imply the other"*
   is precisely what K-7's own argument refuses as a defence: the fix is not to replace one word
   with another, it is to stop having two predicates. The new form is also **weaker on purpose**:
   a table of three `Active` seats with chips that have all stopped signing now pauses at this
   condition instead of dealing a hand that step 9 would have paused anyway one branch later.
3. **One seat dealt in, others with chips** — `|dealt_in| == 1`, the drain case of §5.3 step 9, and
   the hand is played as an uncontested blind steal.
4. Otherwise — `|dealt_in| ≥ 2` → `AwaitingKeySetup` for hand `hand_id + 1`.

**Conditions 2, 3 and 4 are now one predicate read three ways, and that is the whole of L7's
fix.** They are §5.3 step 9's three branches, in step 9's order, on step 9's quantity; this section
adds nothing to them and no longer contradicts them anywhere. Conditions 0, 0.5, 0.6 and 1 are
different in kind — they are about the *table*, not about who is dealt into the next hand — and
they are evaluated first, which is what keeps condition 1 (tournament won, exactly one seat with
chips) from being pre-empted by condition 3's drain branch when that seat is the only one left.
**The ordering matters and it did not change**: hand init runs first, so step 2 has marked the
busts and step 4 has computed `dealt_in` before any condition here is read.

**What this fix newly makes load-bearing, checked rather than assumed.** The way *into* `Paused`
and the way *out* of it now read the same predicate: condition 2 is `|dealt_in| == 0`, and T59's
exit is *"§5.3 step 4's own predicate, evaluated on the state this transition leaves behind, would
yield `|dealt_in| ≥ 2`"* since K-7. Before this pass they were two different predicates on two
different quantities, and the gap between them — `Active` seats with chips that had stopped
signing — was a table that could leave `Paused` for a drain hand it should not have been dealt.
§12.1's row 18 is where that is discharged and it is unchanged, because neither predicate is an
exit under total silence.

No timeout, disconnect or abort reaches condition 1 (D-006 §5). A tournament ends only when one
player holds every chip, or when every seat has left, which is condition 0.

**And no timeout reaches condition 0.6 either, which is the check `SPEC_CS.md` §4 and D-006 §5
require of every new closing condition.** The latch is set by **T62**, **T63** and **T50-when-solitary**,
and every one of those three rests on another seat's signature: T62 and T63 on a chained event
signed by a seat outside `P`, and T50 on the existence of **two distinct `state_hash` values for a
checkpoint one seat was required to publish**, which cannot arise unless a seat other than this one
signed one of them. It is cleared
by T53, which since N1 needs two signers of its own. A timer decides *when* a frozen table reaches
T47 — T57 or T61, as it always has — but no timer can set the latch and no timer can close a table
that has not been contradicted by a signed event from another seat.
The two closing conditions this document has added since D-010 therefore rest on the same kind of
artefact: 0.5 on a completed reconciliation stage (T54), 0.6 on a chained event this peer accepted
as canonical for a hand it dealt. Neither rests on silence, and adding one that did would be the
defect D-006 §5 names.

**Condition 1 is now reached through the boundary gate, and that is what checkpoints the tournament
result (K-3).** These conditions are evaluated at T47, and on the settled path T47 does not fire
until checkpoint 8's `STATE_HASH` stage is complete over `P(k)` (§5.2's box). The last hand of a
tournament is a settled hand — condition 1 reads a stack, and stacks move only at T45 — so the
final `final_stacks`, `busted` and `finish_order` are compared across every seat still
participating **before** `TableClosed` absorbs and starts rejecting everything. Previously the
result of the whole table was the one quantity in the corpus that nothing ever compared: the last
hand of a drained-out table is a `|dealt_in| == 1` drain hand, which placed no checkpoint, and the
phase that follows it rejects the checkpoint traffic that would have caught a disagreement.

What this does **not** claim is that the result is verified against a seat that is not there. On a
table drained by a silent opponent `P(k)` is the one remaining seat and the stage completes at it
alone; the comparison is worth exactly as much as the number of peers still signing, which is
honest rather than reassuring. What it does buy is that a **fork** — two peers each playing their
own drain sequence to their own "tournament won", which is `DECISIONS.md` K-1's payoff — collides
at checkpoint 8 of the first hand on which their states differ, because T49 and T50 compare a
`STATE_HASH` from any seat and not only from a required emitter.

### 9.4 Cash mode and custom tables

`Mode::CashPlayMoney` differs in exactly three ways, all confined to T47 and hand init:

* the blind level never advances — `small_blind`/`big_blind` are the config values for every hand;
* condition 1 of §9.3 does not exist; the table runs until every seat leaves;
* a seat may be **removed** at a hand boundary and only there: `PLAYER_LEAVE` is a hand-boundary
  single-writer stage and only that (`PROTOCOL.md` §4.10, M4), so a mid-hand copy is not "recorded
  and applied later" — it is a `Rejection` that never reaches the ledger at all (T35, T17, §4.1).

**A seat may not be added after `Seating`, in either mode, in this version (P8).** The previous
revision described mid-session buy-in as a live capability and cited a message type called
`PLAYER_SEAT` for it. There is no such message: `PROTOCOL.md` §4.10 declares `PLAYER_SIT_OUT`
(`0x0803`), `PLAYER_SIT_IN` (`0x0804`) and `PLAYER_LEAVE` (`0x0805`) and nothing else, and §4.11's
table has 39 rows and no row for one. Nor was there a transition: `Event::PlayerSeated` is consumed
by T1 and T2 only, both in `Seating`, so §5.3 step 0's seat-entry clause was unreachable and cash
mode's `ledger_in` could only ever be set once and then stand while `ledger_out` grew. The half-state
— a capability described in prose, absent from the alphabet — is the defect; it is closed by
**removing the description**, not by inventing a message type, a stage kind and a transition for a
mode the MVP does not ship. §11's **Q6** carries the question with a named default, in the same shape
§11's Q5 already uses for rebuys.

The consequence for cash mode, stated rather than glossed: a cash table forms in `Seating` exactly
as a tournament table does, and thereafter can only lose seats. `ledger_in` is fixed at the first
hand init; `ledger_out` grows as seats leave; I1's right-hand side is therefore **non-increasing**
after hand 1 in both modes, and it is constant in tournament mode. A player who wants to join a
running cash table opens a new one.

**The ledger moves only in hand init step 0.** `ledger_in` takes the sum of the accepted seats'
buy-ins at the **first** hand init and never moves again; a seat marked `Leaving` by T58 is removed
at a later hand init and increments `ledger_out` by that seat's stack. Both are recorded in the
derived `HAND_INIT` of that hand — `PROTOCOL.md` §4.4's
`n(11) ledger_delta`, a per-seat `Vec<(u8, i64)>` ascending by seat, positive for a buy-in and
negative for a departing stack, recomputed by every receiver and rejected on mismatch. The vector
and the step are the same thing seen from two layers: every receiver recomputes it from the
accepted T58 events it holds, and a copy that disagrees does not complete the
stage. Because both counters change only in that one step and only in this direction, invariant I28
holds and
invariant I1's right-hand side is constant *within* a hand, which is what makes I1 checkable
mid-hand at all. In tournament mode no exit changes the totals in play before the tournament ends,
so the right-hand side never changes and I1 degenerates to the old constant-total form. **A
`HAND_INIT` stage that fails to complete because two peers derived different `ledger_delta` vectors
is covered by the hand deadline** — which since R-1 runs from `TERMINAL(k−1)` rather than from
`HAND_INIT` itself (§8.2) — and ends at T57 like any other stall.

`SPEC_CS.md` §4 lets a founder replace the preset with custom parameters, which must be in the
signed advertisement so that every joiner sees exactly what they are joining and all participants
agree before the first card exists. The engine validates the config before leaving `Seating` and
refuses to start otherwise:

```
every range in PROTOCOL.md §7.2's field table, over the same fields
                                                       // §7.2 rule 2, by reference
hand_deadline_ms >= HAND_DEADLINE_MIN(seats)           // §7.2 rule 2a, the same predicate over the
                                                       // same fields; PROTOCOL.md §8.2 owns the
                                                       // formula and no part of it is restated
                                                       // here (D-011 rule 2). The upper half of
                                                       // the bound is n(17)'s cap, §7.2's table
start_stack >= 2 * (2 * first_small_blind + ante)      // at least one full orbit
small_blind_cap >= first_small_blind
blind_raise_every_hands >= 1                           // §9.2's divisor
ante <= first_small_blind
showdown_policy is one this build implements           // Q-01, §7.7, §11
```

**The first two entries are `PROTOCOL.md` §7.2's admission check by reference; the last five are
this engine's and are in no other document.** Four literal range lines stood here —
`2 <= seats <= 10`, `2 <= min_players_to_start <= seats`, `first_small_blind >= 1` and
`action_timeout_ms >= 5_000` — every one a copy of a range §7.2's field table already carries, and
each is deleted rather than kept in sync: a bound written in two documents is a bound that
disagrees with itself on the pass nobody sweeps, which is `G7-S1` stated as a rule. `n(11)`,
`n(12)` and `n(14)` carried their ranges all along; `first_small_blind` is the one that became
safe to delete only in this pass, because `G7-S3` made §7.2 rule 2 enforce
`small_blind == blind_schedule.first_small_blind` and `n(4)` carries the `>= 1` — without that
identity the engine would have been deleting a check on a field the joiner never bounded.

The five that remain are conditions §7.2 does **not** carry, each about whether a hand can be
*played* rather than whether an advert is well formed — one full orbit of chips, a cap at or above
the first blind, a defined divisor for §9.2, an ante inside the small blind, and a showdown policy
this build has code for. **One of the five is a check the owner's table should carry and does
not**: `n(13) BlindSchedule`'s `every_n_hands` is a `u16` with no stated lower bound, so a
conforming advert may carry `0`, which makes §9.2's `level(h)` a division by zero at every peer
that computes it. This line refuses such a table at the engine, and the missing range belongs to
`PROTOCOL.md` §7.2; it is reported for that owner's list under D-013 rather than fixed from here.

A config that fails goes to `TableClosed` and is never played. There is no repair path — repairing
a signed advertisement locally would mean two peers playing different games.

**The deadline line is `G4-P3-e`, and this pass moves it a second time — `Q6` and `G7-S4`.** Two
earlier forms were wrong in the same direction. `hand_deadline_ms >= 10 * action_timeout_ms` was a
multiple of the wrong term and sat below `HAND_DEADLINE_FLOOR(n)` at every seat count, so it
admitted exactly the configurations `PROTOCOL.md` §8.2 proves unplayable: a legal no-re-raise hand
cannot finish inside them, every hand ends at T57 with `cause = 1` and `attributed = []`, and no
seat is ever named for it. The form that replaced it, `>= HAND_DEADLINE_FLOOR(seats)`, was right
about the term and one `REOPENING_COST` short of the bound the joiner applies: `G5-Q6` raised §7.2
rule 2a to `HAND_DEADLINE_MIN(n)` and this line did not follow, so **between the floor and the
admitted minimum this engine started tables that every conforming joiner refuses** — a founder
whose own client plays a table nobody may enter, and a table on which the first re-raise reaches the
abort `P3` exists to prevent. The right-hand side is now `HAND_DEADLINE_MIN(seats)`.

**`seats` here is `max_players`, never the live seated count** — §8.2 requires that, because
`hand_deadline_ms` is two-sided and a bound derived from a count that moves during the table's life
is not a constant every peer holds identically. It is `n(11)`, and §2.3's field says so.

**The sameness claim is now true, and it is a derivation rather than an assertion (`G7-S4`).** What
stood here said the engine's check and the joiner's were *"the same predicate over the same fields"*
and cited *"§9.4 rule 2a"*. Both halves were false: the predicates differed by one
`REOPENING_COST(n)`, and `PROTOCOL.md` §9.4 is *Collection bounds* and contains no numbered rule at
all — the joiner's rules are §7.2's, and citing §9.4 for them is `G6-R7`. They are the same
predicate now because this line does not **state** the bound: it names `HAND_DEADLINE_MIN`, which
`PROTOCOL.md` §8.2 defines and §7.2 rule 2a applies, over `seats = n(11)` and the four other advert
fields the formula reads. There is nothing here left to drift from, which is the only form of *the
same predicate* that survives a pass nobody sweeps — and it is what makes a table this engine agrees
to start one a conforming joiner agrees to enter.

### 9.5 `SPEC_CS.md` §32 — heads-up first

All 20 phases and all 57 live transitions (§5.1) are reachable with `seats = 2` **except**:

* the multi-pot form of `build_pots` (§7.5) and the odd-chip distribution over more than two
  winners (§7.6), both of which require three distinct commitment levels or three tied winners;
* **the six certificate rows — T16, T22, T27, T34, T41 and T44 — which are no longer
  *unreachable heads-up* but simply gone (D-015, §5.2).** The bullet that stood here said T34 was
  unreachable at `|V| < 2` and the five `Crypto` rows with it, so *"the first mode this project
  ships is the one in which the deadline machinery does not apply"*. That is now true of every
  mode and for a stronger reason: the machinery is not built. **T57 is reachable and is the only
  *mid-hand* abort path any stall takes; since K-3 the boundary has one of its own, T61, and it
  too reads no certificate and no voter set.** What heads-up loses relative to a larger table is
  now nothing at all on this axis, which removes the one place §9.5 had to argue that the shipped
  mode was the weakest one;

**T62 and T63 are reachable heads-up and are the *only* configuration in which K-1's trace is the
normal case rather than a corner one**, which is the opposite of the pattern above: `|P|` narrows
to one member soonest at two seats, so the solitary regime is a heads-up regime first. T64, T65 and
T66 read no voter set and no seat count and are reachable at every table size, including two.

The heads-up-specific rules — button-is-small-blind, inverted post-flop order, TDA 34-B button
adjustment — are exercised *only* heads-up, so both branches need explicit tests from the start
(A1.2).

**No shipped mode has deadline machinery, and heads-up is no longer the special case (D-015).**
The paragraph that stood here argued that `SPEC_CS.md` §32's heads-up-first requirement made the
MVP ship *"exactly the configuration in which `|V|` is 1 in every hand"*, so that §8.4 and §8.5
were **dead code in the MVP** and first became live at three dealt-in seats. **The conclusion
survives and the argument is deleted**: the machinery is not built at any table size, so it is
not dead code in the MVP — it is not code. `consecutive_auto_actions` is never incremented
(§8.5) and `auto_action_limit` never fires, at two seats or at ten. **The one deadline path this
version runs is T57**: a stall of any kind ends the hand at `hand_deadline_ms` with nobody named
and stacks restored, so T57 and §8.6's one restoration rule are on the critical path and no
certificate ever is. Four consequences for the test plan:

* the acceptance test must assert that a `TIMEOUT_CERT` or `TIMEOUT_VOTE` frame of **either**
  kind is *dropped at the receiver* — not merely absent, and not accepted-but-stripped, which is
  what the pre-D-009 text would have produced. **The assertion moved down a layer with D-015**:
  it used to be *"the engine rejects it"* and is now *"`PROTOCOL.md` §4.0 step 6 drops it and the
  engine never sees it"*, which is checkable at the receiver rather than in `step`;
* the heads-up acceptance test must also cover T57 end to end: one seat goes silent, no certificate
  forms, the hand ends on the hand deadline with `attributed == []`, and every stack equals its
  `start_stack_this_hand` afterwards (I27, I2). Since P3 it must additionally assert that **the
  terminal stage closed against the silent seat's silence** — one peer's copy was enough, no peer
  waited for the seat that stalled, and both peers computed the identical `TERMINAL(k)`. A test that
  only checks the stacks would have passed while T57 could never fire. **Since D-013 it must run one
  hand further, and that extension is the one that would have caught J2**: assert that hand `k+1`
  deals *only* the remaining seat in, completes without waiting for anything, and that the silent
  seat's stack is strictly smaller after it. A test that stops at the abort passes on a table that
  never plays again;
* a **restoration test that is not a heads-up test**: at `seats >= 4`, a certificate-borne abort at
  `|V| >= 2` naming a seat must leave that seat's stack equal to its `start_stack_this_hand` and
  every other stack likewise (D-010, §8.6). Asserting only that the abort happened would pass while
  the forfeiture formula was still running;
* ~~the certificate paths must be tested where `|V| >= 2` before they are relied on~~ — **deleted
  by D-015: there are no certificate paths.** What replaces it is the test that the *absence* is
  real, and it is cheap: feed a well-formed `TIMEOUT_CERT` and a well-formed `TIMEOUT_VOTE` frame
  into the receiver and assert both are dropped at `PROTOCOL.md` §4.0 step 6, with no fault
  recorded, no state change, and no allocation — the mirror of §5.3's *"no store is created"*
  rule. A deferred mechanism that is untested for being deferred is how it comes back untested;
* the D-008 adversarial test is **retired with the machinery and its construction is recorded so
  it can be rebuilt** (D-015). It was: at `seats >= 4`, a client that emits votes against several
  seats and then presents a certificate whose `signers` is a single seat must be rejected by
  §8.4 rule 4, because none of those seats entered `certified_subjects` (rule 7) and `V`
  therefore never shrank; asserting only that a well-formed certificate is *accepted* at
  `n >= 3` would have passed while N3 was live. **It is not a heads-up test and not a happy-path
  multi-seat test**, and that is the property to preserve: a later version restoring the
  certificate restores this test with it, before the rows, not after.

**`RATED_SNG_POKERTH_V1` is fully specified in `PROTOCOL.md` §13 — there and in no other document,
which is `G7-S5` — and is not playable by the MVP.** It pins `seats` and `min_players_to_start`
both at `MAX_SEATS`, while `SPEC_CS.md` §32 requires two-player heads-up
as the first supported mode and §1.3 scopes the MVP at `nlhe/2-6`. The MVP ships `CUSTOM` tables;
`RATED_SNG_POKERTH_V1` becomes playable when `nlhe/7-10` lands. The Phase 8 acceptance test
therefore uses a **`CUSTOM` two-seat table** (`NETWORK_STACK.md` §12, D-003/D-004), and a reader
who takes the preset as the MVP's acceptance configuration will build the wrong test.
`PROTOCOL.md` §13 carries the same statement under its preset block, and since this pass §9.1
carries no values of its own for it to disagree with.

The 3–6 player extension adds no new phase and no new transition. It changes only the guards that
count seats — plus T34, which starts existing once a hand has `|V| >= 2`. That is the concrete sense in which this machine
"does not make the multi-player extension impossible" (§32).

---

## 10. Invariants for the property tests

`SPEC_CS.md` §26 requires `proptest`/`quickcheck` invariants. Each is a predicate over
`TableState` (and, where noted, over a `(state, event, state')` triple), asserted after **every**
transition including rejected ones. **33 live invariants**, I1–I34 with **one retired number, I29**,
and no other gaps (§5.1). I29 is retired by D-015 rather than renumbered, on the same rule the
retired transition numbers follow: four documents cite invariants by number, so a gap is safer than
a silent renumber. Its row below carries the retirement and what a later version must restore.

The nine §26 named invariants map to I1, I10, I9, I15, I6, I8, I7, I12 and I21 respectively; the
eight of `POKER_RULES.md` A0 map to I1, I6, I10, I9, I4, I8, I7 and I14.

| # | Invariant | Predicate |
|---|---|---|
| **I1** | Chip conservation as a **ledger identity** | `Σ_s stack[s] + Σ_s committed_hand[s] == ledger_in − ledger_out` (see the block below) |
| **I2** | Step-local conservation | for every accepted event, total chips before == total chips after. Since **D-010** this is trivially true of every abort, on every branch and for every `AbortKind`: §8.6 returns `Σ committed_hand` to the seats that put it in and redistributes nothing, so no `AbortSettle` can move the total or move a chip between seats. The one step at which the total legitimately changes is **hand init step 0** (§5.3), where a seat exit moves `ledger_out` by exactly the amount it removes from the table, so I1 holds across it; T58 itself moves nothing |
| **I3** | No underflow | every `Chips` arithmetic uses checked or saturating operations; `stack`, `committed_*` are never decremented below 0; a `u64` wrap is a test failure, not a wrap |
| **I4** | Commitment ordering | `∀s: committed_round[s] ≤ committed_hand[s] ≤ start_stack_this_hand[s]` |
| **I5** | Stack accounting | `∀s: stack[s] + committed_hand[s] == start_stack_this_hand[s]` throughout a hand, for every seat that posted anything |
| **I6** | Pot exactness | `Σ pots().size + Σ pots().refunds == Σ_s committed_hand[s]`, exactly, at every point |
| **I7** | Pot eligibility | `∀ pot: eligible ⊆ contributors ∧ eligible ∩ folded == ∅ ∧ eligible ∩ ¬dealt_in == ∅ ∧ eligible ≠ ∅`. The **`dealt_in` clause is new (P7)** and is the assertion that would have caught an `Absent` or `SittingOut` blind poster sitting in `eligible` for a pot that **I20** guarantees it holds no cards for; §12 states that deviation as implemented and §7.5's filter is what implements it. `eligible ≠ ∅` still holds, and now holds *because* §7.5 refunds a level whose eligible set is empty rather than constructing a pot for it |
| **I8** | No folded winner, and no winner that was not dealt in | `settlement.award[s] > 0 ⇒ ¬folded[s] ∧ dealt_in[s]`. The second conjunct is P7's: a seat that took no cards can never receive an award, only a refund of its own uncontested stake |
| **I9** | Board shape | `\|board\| ≤ 5` and `\|board\| == {PreFlop:0, Flop:3, Turn:4, River:5}[street]` whenever `phase` is a betting phase |
| **I10** | Card uniqueness | the multiset `board ∪ ⋃_s revealed_hole[s]` contains no duplicate, and every element is in `0..=51` |
| **I11** | Index discipline | `deck.opened.keys() ⊆ deal_map.all_indices()`, and each index appears at most once |
| **I12** | Street monotonicity | `street` never decreases, and advances only `PreFlop→Flop→Turn→River`; no street is skipped |
| **I13** | Phase discipline | every `(phase, phase')` pair appears in the §5.2 table; there is no other edge. **A row whose State column is a scope rather than one phase — T4, T48, T49, T50, T51, T57, T58, T59, T61, T62, T64, T65, T66 — is read as instantiated at every phase in that scope**, so `(any live phase, HandAborted)` on `HandDeadlineAbort` (T57) — **`(Diverged, HandAborted)` included, which is P5's liveness exit** — `(HandComplete, HandAborted)` and `(Diverged, HandAborted)` on the same event where no hand is live (T61, K-3's boundary exit), and `(Seating \| AwaitingSeatRngCommit \| AwaitingSeatRngReveal \| Diverged, TableClosed)` on `FormationAbandoned` (T4) are *in* the table and are not rejections. I13 constrains edges, not the width of a row, and it is **not** a soundness check on the event that took an edge: it cannot tell a justly produced artefact from a manufactured one, and no invariant in §10 can. The two rows this clause used to name, T55 and T56, are deleted (G2) and the numbers are retired. **Three edges are new in this pass and one of them changes what this invariant is usually read as saying.** `(any phase except TableClosed, Diverged)` on `SolitaryDivergence` (T62) and `(any live-hand phase, HandAborted)` / `(HandComplete \| Paused \| Diverged-no-hand-live, unchanged)` on `CheatProven` (T64, T65) are ordinary widenings. **`(TableClosed, TableClosed)` on `SolitaryDivergence` (T63) is not**: `TableClosed` was described in §5.1 as absorbing, and it is now absorbing for thirty-eight of the thirty-nine event types and not for the thirty-ninth. That is a deliberate hole and I13 is where a reader will look for it, so it is named here rather than left to be discovered: the alternative — a `Rejection`, which I21 requires to leave the state bit-identical — would mean a peer that reached `TableClosed` first keeps a "tournament won" it computed alone against a seat that was signing against it, and discards the only evidence that says so. That evidence arrives late by construction, because a solitary hand self-completes at the speed of local computation and forty of them close in about five minutes (§12.1.2) |
| **I14** | `player_to_act` validity | `player_to_act.is_some() ⇔ phase ∈ Betting*`; and when some, that seat is `dealt_in ∧ ¬folded ∧ ¬all_in` |
| **I15** | Never bet more than the stack | every accepted action moves `≤ stack[p]`; `∀ accepted Bet/Raise(t): t ≤ committed_round[p] + stack[p]` |
| **I16** | `current_bet` definition | `current_bet == max_s committed_round[s]` during a betting round, **except** pre-flop where it is the nominal `BB` (A1.1 rule 3) |
| **I17** | `last_full_raise` behaviour | monotonically non-decreasing within a betting round; equals `BB` at the start of every street |
| **I18** | Round closure | `phase` leaves a betting phase only when the A2 predicate holds, or exactly one live seat remains |
| **I19** | All-in consistency | `all_in[s] ⇒ stack[s] == 0`; and `dealt_in[s] ∧ ¬folded[s] ∧ stack[s] == 0 ∧ committed_hand[s] > 0 ⇒ all_in[s]` |
| **I20** | Absent seats hold nothing | `¬dealt_in[s] ⇒ s ∉ deck.participants ∧ deal_map assigns s no index ∧ revealed_hole[s] == None` |
| **I21** | Invalid input is a no-op | for every `Rejection`, `state' == state` **bit-identically**, including `sequence` and `previous_event_hash`. Covers §26's "an invalid signature never changes the state" and §11's local re-validation |
| **I22** | Determinism | `step(s,e)` is pure: two calls give equal `Step`; replaying a transcript with effects disabled reproduces the identical final state and `STATE_HASH`; no `std::time`, no RNG, no `HashMap` iteration reachable from `TableState` |
| **I23** | No early reveal | `deck.opened.keys() ⊆ opened_by(phase)`, where `opened_by` is empty before `AwaitingDeal`, the hole indices during the deal, and grows by exactly one street's indices at each reveal phase. In particular `phase == HandAborted ⇒ deck.opened` gains no entry (§8.7) |
| **I24** | Canonical/local separation | no field of `LocalView` is reachable from `TableState`; two peers with the same event prefix and different `LocalView` produce equal `STATE_HASH` |
| **I25** | Blind level is derived | `(level, small_blind, big_blind) == closed_form(hand_id)` of §9.2, at every hand boundary |
| **I26** | Finishing order is a total order | `finish_order` has no duplicates, contains exactly the busted seats, and is consistent with the A1.3 ordering (`start_stack_this_hand` descending, then seat order clockwise from `button_pos`) |
| **I27** | **No abort moves chips between seats** (D-010) | *every* abort, whatever its `AbortKind` and whatever its `attributed`, returns exactly `committed_hand[s]` to every seat `s`, so `stack[s] == start_stack_this_hand[s]` for every seat after the accepted `AbortSettle`, and no chip crosses between seats in any direction. The previous form was scoped on `attributed == []` and carved an exception out of a forfeiture formula; the formula is deleted (§8.6) and the scope is gone with it. Assert it after **T46**, on every path that reaches it — T15, T16, T21, T22, T27, T41, T44, T48, T54, T57 and T60 — not only on the two that used to restore |
| **I28** | The ledger is monotone and moves only at a hand boundary | `ledger_in` is monotonically non-decreasing, `ledger_out` is monotonically non-decreasing, and both change **only in hand init step 0** (§5.3), which runs at T10 and T47 — never inside a hand, and never in T58, which only marks a seat `Leaving`. That is what keeps I1's right-hand side constant mid-hand, and it is what makes `PROTOCOL.md` §4.4's `ledger_delta` a complete record of every change |
| ~~**I29**~~ | The voter-set floor (D-008) — **retired as vacuous by D-015** | The invariant asserted, after every transition, that `certified_subjects ⊆ deck.participants`, that a seat entered it **only** on a certificate accepted under §8.4 rules 1–6 with `\|V(subject)\| ≥ 2`, and that every accepted `TimeoutCertificate` had `\|V(subject)\| ≥ 2` **and** `signers == V(subject)`. **All three quantifiers are now empty**: no certificate is accepted, no seat enters the set, and the set itself is deleted from `TableState` (§2.6, §2.8). A harness cannot assert it and a `proptest` cannot falsify it, so it is retired rather than left standing as an invariant that passes because nothing reaches it — which is the shape this document has twice recorded as worse than no invariant (I30's deleted clause, `L4`'s rule that never fired). **What it protected is protected by construction instead:** the N3 attack needed a certificate to have an effect, and no certificate reaches the engine (§4.0 step 6). **What replaces it in the harness is I21**, asserted against the N3 frame sequence — several votes, then a single-signature certificate — which must leave the post-state bit-identical. **Restoring it is step three of §2.8's ordering** and never step one: an implementer who restores I29 before the rows has written an assertion about a set nothing fills. |
| **I30** | **Seat status is agreed, and moves only through a chained event** (D-012, H1) | Three parts. **(a) Provenance.** `status[s]` changes only in a transition whose input is an event every participant accepted as chain content: T1/T2 (`Empty → Active` in `Seating`), T3 (`→ Empty`, before any chips exist), T58 (`→ Leaving`), T59 (`→ SittingOut` / `→ Active`), T34's `auto_action_limit` marking applied at the next hand boundary (§8.5), and hand init steps 0 and 2 — `Leaving → Empty` and `stack == 0 → Busted` — which run at T10 and T47, T45 having already marked the busts of the hand it settled. Every one of those is a pure function of state fixed at a hand boundary or of an event every participant accepted as chain content. **That list is exhaustive; a status assignment anywhere else is a defect**, and `Absent` appears nowhere in it, which is H1's edit seen from the invariant side. **(b) No derivation from a per-receiver quantity.** No transition derives a `status` from `abort.attributed`, from `abort.owed`, from `observed_by`, from a `Fault` record, from `deck.tokens`, or from any other value two honest receivers can hold differently — which is the whole of D-012 applied to this field, and which is why T46's `status := Absent` is deleted (§5.2, §8.6). **(c) Cross-peer agreement at a hand boundary.** Two peers that have accepted the same event prefix hold the **identical `status` vector** in `HandComplete`, hence identical `dealt_in` and `bb_seat` from §5.3 for hand `k+1`. **The clause that used to follow — "hence a `HAND_INIT` collective stage that completes" — is deleted as false (J3).** Agreement on the body makes the stage *completable*; completing it additionally requires every member of the required emitter set to emit, which is a liveness property and is outside this invariant's scope — §12.1 is where it is discharged and D-013 is what makes it discharge. The two are worth keeping apart: identical `dealt_in` at every peer is perfectly consistent with a stage that never completes, and before D-013 that was the *normal* case rather than an edge one, so the deleted clause was not merely imprecise — it was the sentence that hid J2 for two passes, and because it carried a test instruction, a harness asserting it passed on every trace where the stage completed and was never run against the trace where it did not. An invariant that is unfalsifiable exactly where the defect lives is worse than no invariant. This is the engine's half of `PROTOCOL.md` §6.1's rule that `sitting_out` and `absent` — which are in `PublicTableState` and therefore in every `state_hash` — *“must be a deterministic function of accepted chained events”*; §6.1 defers **which** events set them to this document, and (a) is that list. Because the two vectors are hashed, a violation surfaces twice: loudly at the next checkpoint as a `state_hash` mismatch (T50, `Diverged`), and — if the abort came after the last checkpoint of the hand, which is the H1 interleaving — as a `HAND_INIT` stage that never completes. (c) is the part no invariant asserted before, and **that absence was the H1 defect**: the fork it would have caught sat one link downstream of `TERMINAL(k)`, in a field §12.1's walk does not look at because the hand it belongs to does end. Assert (a) and (b) after every transition in the single-peer harness; assert (c) in the multi-peer harness at every `HandComplete`, and generate it directly rather than by random play — the interleaving needs one peer to complete the terminal stage on a certificate-borne abort while another completes it on T57's, which legal play produces only when a message is dropped. **Assert (c) as byte-identity of the two peers' derived `HAND_INIT` bodies**, not as stage completion: that is what (c) actually establishes, it is checkable in a harness, and it is strictly stronger evidence about H1 than a claim about completion |
| **I31** | **Participation is chain-derived, per hand, and is the liveness gate** (D-013, J2) | Four parts. **(a) Provenance.** `signed_this_hand` gains a seat **only** when this peer accepts an event of the current hand signed by that seat, and loses every member exactly once per hand, at §5.3 step 8. `readmit` gains a seat **only** through **T67**, on `PROTOCOL.md` §4.9's decision, and is cleared in the same step — one writer each, one clearing site for both, and neither is ever written from a timer or a connection fact. A `Rejection` adds nothing (I21 already requires the post-state to be bit-identical, and this field is part of it). No timer, no connection state, no heartbeat, no `observed_by` and no field of an abort ever writes it. **(b) It gates `dealt_in`, and nothing else gates it.** After every hand init, `dealt_in[s] ⇒ status[s] == Active ∧ stack[s] > 0 ∧ s ∈ signed_this_hand-as-of-the-previous-hand`, and no transition sets `dealt_in` outside hand init. This is the invariant form of §5.3 step 4 and it is what makes the skip checkable rather than argued. **The union that stood here — `admitted = signed_this_hand ∪ readmit` — is deleted, and deleting it is `P2-e`**: `A` widens hand `m+1`'s **accepted** emitter set and never its required one (`PROTOCOL.md` §4.9), so `dealt_in ⊆ P(m)` is exact and this clause has one term where it had two. **Assert the deletion rather than the union, because that is now the falsifiable direction**: on a trace containing a `Readmitted` event for a seat outside `signed_this_hand`, require that seat to be `¬dealt_in` at the hand init that follows and `dealt_in` at the one after, once its own accepted `HAND_INIT` copy has put it into `P`. An implementation that kept the union passes every ordinary test — `readmit` is empty on every trace with no returning seat — and fails exactly that one, which is why the directed case in §10 is a seat coming back from silence at a peer that had narrowed it out, and not random play. **(c) The terminal abort is excluded, and this is the part an implementer will get wrong.** A terminal `HAND_ABORT` enters no `stage_hash` (`PROTOCOL.md` §3.2), the stage it closes is witness-independent, and two honest peers routinely accept copies signed by different seats — so **counting a terminal `HAND_ABORT`'s signer as participation is a per-receiver derivation and a D-012 violation**, and it would put the whole liveness gate back on the quantity H1 was about. Assert directly: accept a terminal `HAND_ABORT` and require `signed_this_hand` unchanged. **(d) Cross-peer agreement.** Two peers that have accepted the same event prefix hold the identical `signed_this_hand`, hence identical `dealt_in` for hand `k+1`. Where the stage completed this holds by construction, since the set is exactly `stage_hash` membership; the residual case — a contribution to the stage that *stalled*, which no `stage_hash` ratifies — is **Q8** and is `PROTOCOL.md`'s to close. Assert (a), (b) and (c) after every transition in the single-peer harness; assert (d) in the multi-peer harness at every `HandComplete`, together with I30(c) and by the same byte-identity check on the derived `HAND_INIT` bodies |
| **I32** | **Every hand places a checkpoint, and the boundary gate is discharged by a timer and never by a peer** (K-3) | Four parts. **(a) Coverage.** For every `hand_id` the engine reaches `HandComplete` for, `checkpoints.live` is `Some` with `number == 8` and `hand_id == k` on entry, on **both** the T45 and the T46 path, and this peer published its own `STATE_HASH` body for it. There is no hand — drain hand, hand that stalled at `HAND_INIT`, hand aborted at T57, or hand that reconciled through T53 — after which nothing comparable was emitted. This is the assertion whose absence *was* K-3, and it is the one §12.1's walk structurally cannot make: that walk asks whether each hand **ends**, and a hand that ends with nothing emitted passes it. **(b) The gate is one-sided.** T47 is blocked on `checkpoints.live.heard ⊇ checkpoints.live.required` only where the phase was entered from **T45**; entered from **T46** the gate is absent, so `HandComplete` after an aborted hand is left with no external input, exactly as it was before this pass. Assert directly: reach `HandComplete` by both routes with a required emitter silent, and require the abort route to advance and only the settled route to wait. **(c) No two gated boundaries in a row.** T61 runs hand init before leaving for `HandAborted`, so the boundary that follows a T61 firing is reached through T46 and is ungated by (b). Assert over a trace, not over a state: no two consecutive `HandComplete` entries are both gated. That is the whole termination argument for phase 16 and it is worth a machine check rather than a reading, because the failure it excludes — a boundary that stalls, times out, and stalls again on the same set — is precisely the fixed point J2 was, one link further out. **(d) The store is bounded, and its retention is a superset of the wire's acceptance windows (`P1`).** Assert over a trace, after every transition: **at most three `CheckpointState` values exist**, one per rôle, and no two of them carry `number == 8` at the same time. Assert the lifetimes directly, because each one is a different failure: `checkpoints.boundary` is `Some` with `hand_id == k` from the hand init that follows `TERMINAL(k)` until the next T45 or T46, and `None` outside that interval; a `StateHash` or `StateAck` naming a `(hand_id, checkpoint)` no slot holds is a `Rejection` that allocates nothing (I21). **And assert the pair that is the point of the part**: a checkpoint-8 `STATE_ACK` of hand `k` delivered after `HAND_INIT(k+1)` has completed must still reach `agreed.is_some()` on hand `k`'s record — which is D-014's tier-2 precondition at a hand boundary, and which the single slot made unreachable — and a checkpoint-8 `STATE_HASH` of hand `k` that differs, delivered in the same window, must reach T50. Generate both directly rather than by random play: they need the forwarding reorder of `PROTOCOL.md` §1.5, which is ordinary on a healthy table and which no single-peer trace produces. **And assert the two fields the pair rests on (`Q4-e`)**: `own` is written exactly once per record, at the opening, and is bit-identical afterwards for the record's whole life — an implementation that re-derived it from the current `TableState` instead would pass every single-hand trace and fail every boundary one, which is the same forwarding-reorder trace; `dissent` is written at most once, so `heard`, `own` and `dissent` together are the complete divergence evidence and no third value is retained. |
| **I33** | **A solitary peer completes nothing after it has been contradicted** (K-9, `PROTOCOL.md` §3.2) | Four parts. **(a) The regime record is a monotone floor, and it is ordered against the wire's record (N4).** `solitary_since == Some(j)` iff hand `j` was the **first** hand this peer dealt with `\|signed_this_hand\| == 1` — `P(hand_id-1)`, and **not** `P(hand_id-1) ∪ A`, which is `P2-e`: since `P2` the readmission set widens an accepted emitter set and never a required one, so a peer with a non-empty `A` and `P == {self}` is still comparing its state against a set of one and must still record the regime. **Assert that directly, because it is the clause `K1` died on**: deliver a `Readmitted` into a peer whose `P` has narrowed to itself and require `solitary_since` to be written at the next hand init anyway — under the deleted union it was not, and T62 and T63 were then dead for the whole episode — as read at §5.3 step 8; it is `None` iff no such hand exists; it is written **only** at §5.3 step 8 and is **never cleared**. **The interval property this clause used to assert — that every hand in `j ..= hand_id` was solitary — is deleted as false**: a regime that is left can be re-entered, so the solitary hands are a union of intervals, and asserting the interval is what let this document's memory and `PROTOCOL.md` §4.0 step 10b's disagree in the one direction that loses evidence. What replaces it is the ordering, and unlike the interval it is an assertion a harness can run against both memories at once: **for every hand `k` whose retained record says `was_solitary`, `solitary_at(k)` holds** (§2.6's lemma). **The predicate is `j <= k + 1`, not `j <= k`, and the one hand of slack is `N-1e`**: `PROTOCOL.md` §3.2's regime test is the disjunction `P(k-1) == {self} ∨ P(k) == {self}`, the write site sees only the first disjunct, and on the second the floor is written one hand late — which is the hand K-1's own walk lands on first, so the old form was false for exactly the class of hand the rule exists for. **The directed case is therefore the second disjunct and not the first**: build a hand whose `P(k-1)` has three seats and whose `P(k)` has one, deliver a contradicting event naming that hand, and require T62 to fire on it rather than on the hand after it. A harness that only ever enters the regime through the first disjunct passes against both forms of the predicate and proves nothing. Generate it directly rather than by random play — enter the regime, leave it through an accepted `PLAYER_SIT_IN`, re-enter it, then deliver a contradicting event naming a hand from the **first** episode: before this pass the wire froze on it and the engine rejected it, and no single-peer trace showed the disagreement because each memory was self-consistent. **(b) The freeze is complete, with exactly one exempt message class, and naming it is N1's third half.** After T62 or T63, and for as long as `solitary_contradicted` holds: no `stage_hash` **of the hand** is completed, no `settlement.award` changes, `Σ stack` is unchanged, `deck.opened` gains no entry, and no `Effect::Publish` is produced — **except the reconciliation exchange of `PROTOCOL.md` §6.3 steps 2 and 3: this peer's `DISPUTE`, its reconciliation-round `STATE_HASH` and the matching `STATE_ACK`.** The exemption is not a weakening and it is not optional: the clause as it stood forbade the frozen peer to publish the one thing its **only** release path consumes, so T53 could never fire, and the freeze this invariant describes would have had a repair exit that nothing could reach — which is the shape `L4` found for T62 and which N1's own fix newly made load-bearing by making T53 the exit that has to work. Nothing about the exemption re-opens the freeze: a reconciliation value opens no card, applies no action, moves no chip, awards no pot, evaluates no end condition and completes no stage **of the hand**; it completes a stage *about* the hand, which is what a repair is. **And it is what makes T53's two-signer conjunct satisfiable at all** — the frozen peer's own value is one of the two signatures, the seat that contradicted it supplies the other. This is the assertion `PROTOCOL.md` §3.2's guarantee reduces to — *"completes no stage, awards no pot, and evaluates no end condition"* — read with §6.3's own reconciliation traffic excluded, as §6.3 step 1's freeze already reads it (T52 records a `DISPUTE` while frozen and always has). It is the only one of the four a single-peer harness can check on its own; check the exemption too, by requiring that the *only* publications on a frozen trace are those three types. **(c) The latch is one-way except through a reconciliation two seats signed, and the assertion is scoped on the freeze rather than on the latch (N1).** `solitary_contradicted` is set by **T62**, by **T63** and by **T50 when `\|checkpoint.required\| == 1`**, is cleared by **T53 and by nothing else**, and while it is set §9.3 reaches condition 0.6 before conditions 1 to 4. **The form that stood here — *no trace contains two `HandComplete` entries with the latch set and a hand dealt between them* — is satisfied by the oscillation it was written to exclude**, and in two separate ways: a divergence detected by T50 sets no latch at all, and a T53 that fires on a stage the frozen peer completed alone clears the latch on the way round, so both loops leave the clause vacuously true while the table freezes and thaws for ever. Assert instead, over a trace: **between any two entries into `Diverged` with a hand dealt between them there is a completed reconciliation stage carrying values signed by at least two distinct seats.** That is falsifiable against both triggers and both exits, where the old form was falsifiable against neither. **This is the anti-fixed-point clause and it is the reason the invariant exists** — the failure it excludes is a table that freezes, then times out or reconciles with itself, thaws, deals another solitary hand and freezes again every `hand_deadline_ms`, which is J2's shape on the path K-1's fix newly made load-bearing, and every row of §12.1 passes while it happens. **(d) The drain is untouched.** On a trace in which no `SolitaryDivergence` arrives **and no checkpoint mismatch occurs**, the state is bit-identical to the same trace run against the pre-K-9 engine. Assert it by construction: `solitary_since` is read only through `solitary_at`, and `solitary_at` is read only by T62 and T63 — one row per contradicting event, and neither on any path a drain hand takes. **The qualifier is N1's**: T50 now sets the latch too, on `\|checkpoint.required\| == 1`, so the trace class this clause is stated over is one in which *neither* contradiction arrives; a drain hand that meets a `state_hash` mismatch is by construction not a drain nobody contradicted. This is a **multi-peer** invariant in parts (b) and (c) and the directed case is the one below. |
| **I34** | **A removed seat never re-enters, and removing it moves no chips** (D-014) | Three parts. **(a) Absorbing.** `status[s] == Removed` implies `status'[s] == Removed` after every transition, without exception — T59 excludes it from its guard set, T58 excludes it, hand init steps 0 and 2 do not reach it, and §8.5's `auto_action_limit` marking does not apply to a seat that is not `dealt_in`. It is the only status in §2.4 with this property, and it is the property D-014 point 4 asks for. Assert directly by enumeration over the transition table, not by random play: the whole content of the clause is that **no** row assigns anything else. **(b) It is out of every set at once.** After T64 or T65: `s ∉ deck.participants`, `s ∉ signed_this_hand`, `dealt_in[s] == false`, `player_to_act != Some(s)`, and `dealt_in[s]` is false after every subsequent hand init. **`step` never adds a `Removed` seat back to `signed_this_hand`** — that is the one place the set could otherwise let it back in, since §5.3 accumulates on every accepted event. **(c) The chips are covered by I1 and by nothing new.** T64 and T65 change a `status` and move **no chips at all**: neither `ledger_in` nor `ledger_out` moves (I28 — they move only in hand init step 0, and only for a `Leaving` seat), the stack stays on the table and drains through §5.3 steps 6–7, and the hand T64 voids is restored by T46 like every other abort (I27). So `Σ stack + Σ committed_hand == ledger_in − ledger_out` holds across a removal with no term of it changing, which is **I1** — the invariant D-014 point 3 is written to preserve, and the reason the offender's chips are blinded off rather than confiscated. Assert I1 immediately before and after every `CheatProven` and require both sides equal and *unchanged*. |

**I1 in full.** The old form — `total_chips = players_at_start × start_stack`, constant for the
table's whole life — is **false in cash mode**, where a seat may buy in or cash out at a hand
boundary (§9.4). The invariant is a ledger identity instead:

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

I5 (`stack[s] + committed_hand[s] == start_stack_this_hand[s]`) is unchanged and is what makes I1
checkable *within* a hand, since the right-hand side cannot move mid-hand.

**Nine cases below** deserve dedicated adversarial tests rather than random generation, because a
`proptest` generator that only produces legal actions will never reach them. The eleven named
cheaters of `SPEC_CS.md` §25 are mapped to catalogue rows, test modules and asserted outcomes in
one place — `THREAT_MODEL.md` §5.5 — and the three notes below are subordinate to that map rather
than a second one:

* **I21** against `CheaterIllegalRaise`, `CheaterReplayAction`, `CheaterFakeStack`,
  `CheaterEquivocation` (`SPEC_CS.md` §25);
* **I23** against `CheaterFutureBoard` — a reveal token for a turn index published during the flop
  betting round must be rejected by T25 and leave `deck.opened` untouched;
* **I20** against a `CheaterDisconnect` who leaves mid-hand and whose seat must be shown to receive
  no index and no token in the following hand;
* ~~**I29** against a voter-set-collapse client at `seats >= 4`~~ — **retired with I29 (D-015)**,
  and the construction is recorded in the bullet above rather than lost. What stands in its place
  is **I21** against the same client: the N3 frames — several votes, then a single-signature
  certificate — must leave the post-state **bit-identical** to the pre-state, because they are
  dropped before the engine sees them. That is a weaker assertion about a stronger fact, and it
  is the one the shipped code can actually make;
* **I27** against every abort path, and specifically against a *named* one: at `seats >= 4`, a
  certificate-borne abort naming a seat must leave that seat's stack at `start_stack_this_hand`
  (D-010). A generator over legal play reaches aborts rarely and reaches attributed aborts almost
  never, so this is a directed test, not a `proptest`;
* **I7** and **I8** against the P7 construction, which random legal play does not produce either:
  three or more dealt-in seats, a non-participating seat in the blinds, and a dealt-in seat all-in
  below that blind, asserting that the non-participating seat appears in no `eligible` set and
  receives no award. Set that seat up through **T59** (`PlayerSitsOut`), not by writing `Absent`
  into the state: since H1 no transition produces `Absent`, and `SittingOut` is the reachable state
  with identical engine behaviour (§2.4);
* **I30(c)** against the H1 interleaving, which is a **multi-peer** test and is the only one in this
  list that a single-peer harness cannot express: three seats, one silent at a collective crypto
  stage, a `kind = 2` certificate completing at peer `A` while peer `B`'s copy of it is dropped and
  `B` reaches the same hand's terminal stage through T57 instead. Assert that both peers hold the
  identical `status` vector afterwards, that both compute the same `dealt_in` and `bb_seat` for hand
  `k+1`, and that the two peers' derived `HAND_INIT` **bodies are byte-identical**. Under the
  deleted T46 clause this test failed; it is written here because the defect it catches was
  invisible to every single-peer assertion in this table. **The assertion is byte-identity, not
  stage completion (J3)**: the previous wording asserted that the stage completes, which the stage
  does not owe this invariant and — before D-013 — could not deliver, so the test would have been
  written to pass on the traces where the defect is absent and never run on the one where it is
  not. Whether the stage completes is §12.1's obligation and is tested by the case below;
* **I31** against the case D-013 exists for, and it is the one test that would have caught J2:
  three seats, one of which stops sending at a hand boundary and never sends again. Assert that
  hand `k` stalls to `hand_deadline_ms` and aborts (T57 → T46 → T47); that hand `k+1` deals the
  remaining two seats in and **completes normally**, with the silent seat `¬dealt_in`, absent from
  `deck.participants` (I20) and posting its blinds and antes when the ring reaches it; that its
  `stack` strictly decreases on every orbit; that it reaches `Busted` at §5.3 step 2 in a bounded
  number of hands; and that §9.3 then fires. Run the same case with the seat going silent
  **mid-hand** and assert the bound is **two** stalled hands rather than one — that difference is
  real, it is `HAND_INIT`'s own copy counting as participation, and a harness that only tests the
  boundary case will not see it. Then run it once more with the silent client **returning** and
  signing one chained event, and assert it is dealt in again at the next boundary with no vote,
  certificate or human intervention anywhere in the trace. None of this is reachable by random
  legal play, because random legal play never stops sending.
* **I32** against the drain regime, and it is a **multi-peer** test because a checkpoint is a
  comparison and a single-peer harness cannot fail one. Two seats, one silent from a boundary; play
  the forty drain hands §12.1.2 derives and assert that every one of them opened checkpoint 8 and
  published a `STATE_HASH` for it, that none of them opened checkpoints 2 to 7, and that the hand
  which fires §9.3 condition 1 completed its checkpoint-8 stage **before** the phase became
  `TableClosed`. Then run the same trace with the second peer's state forced to differ by one chip
  at hand 3 — the K-1 shape, two peers each holding `P = {self}` — and assert that both reach
  `Diverged` at checkpoint 8 of hand 3 rather than each playing on to its own "tournament won".
  Finally, force the gate to stall: let a seat emit its `HAND_COMPLETE` copy and then stop, and
  assert **T61** fires at `hand_deadline_ms`, that the next boundary is ungated, and that the trace
  contains no two consecutive gated boundaries (I32(c)). **And run I32(d) as its own case, because it is the one
  the single slot silently failed (`P1`)**: with two peers and no fault at all, delay one peer's
  checkpoint-8 `STATE_ACK` behind the other's `HAND_INIT(k+1)` copy — which is what forwarding does
  by itself (`PROTOCOL.md` §1.5) — and assert hand `k`'s record still reaches `agreed.is_some()`, so
  a D-014 tier-2 finding at that boundary has its precondition. Then force the same reorder with a
  **differing** value and assert **T50** fires on `checkpoints.boundary` while hand `k+1` is live.
  Neither case involves an adversary, and neither is reachable in a single-peer harness.
* **I33** against K-1's own trace, and it is **multi-peer** and cannot be anything else, because
  the defect is two peers each of which is internally consistent. Two seats; drop exactly one
  `HAND_INIT` copy at hand `k` so the two peers derive different `dealt_in`; let both peers narrow
  to `P = {self}`; run the drain. Assert that the first chained event of a solitary hand that
  reaches the other peer produces **T62** and not a `Rejection`, that the receiving peer completes
  no stage and awards no pot afterwards (I33(b)), and that **neither peer ever reaches §9.3
  condition 1**. Then run the tail of it: let the contradicting event arrive only after the
  receiving peer has reached `TableClosed`, and assert **T63** fires there and
  `settlement.tournament_winner` is retracted — that ordering is the normal one, not the corner
  case, so a harness that only delivers the event mid-drain tests the easy half. Then run the
  anti-fixed-point case, which is the one worth writing first: after T62, hold both peers silent,
  let the deadline fire, and assert the table reaches `TableClosed` through §9.3 condition 0.6
  rather than dealing hand `k+1` — a harness that stops at "the phase was left" passes the
  freeze-thaw loop I33(c) exists to exclude. Finally assert I33(d): the same forty-hand drain with
  **no** contradicting event is bit-identical to the pre-K-9 engine's.
* **I34** against D-014, and the important half of it is the negative one. Remove a seat at a
  boundary (T65) and then try every route back in: `PlayerSitsIn` (T59), `PlayerLeft` (T58),
  signing a chained event of the next hand, a checkpoint-8 `STATE_HASH`, and **`Readmitted` from §4.9's readmission set (T67, `P2-e`)** — assert `status` is
  still `Removed`, neither `signed_this_hand` nor `readmit` contains it, and it is `¬dealt_in` at every
  subsequent hand init. **The `Readmitted` route is the one to run first and the reason it exists at
  all is T67's guard**: since `P2` nothing else in this document reads `readmit`, so if that guard
  were dropped the set would carry a `Removed` seat straight into `PROTOCOL.md` §4.9's accepted
  emitter set with no engine test anywhere behind it. Assert I1 across the removal with **both sides unchanged**, and assert the
  removed seat's stack strictly decreases every orbit and it busts, so §9.3 condition 1 is still
  reachable. And run the mirror D-014 makes the gate on the whole feature: **for every legal action
  an honest client can emit, under every legal interleaving, no `CheatProven` is producible against
  it.** That is `DECISIONS.md` **D-014-2**, it is the mirror of `SPEC_CS.md` §25's cheater list, and
  it blocks shipping the feature rather than writing it — a validator that is too strict now ejects
  an honest player rather than rejecting a message, and `THREAT_MODEL.md` carries the risk in its
  own right.

---

## 11. Open questions carried forward

None of these is resolved here. Each is recorded so it cannot be lost, and each now carries a
**named default the engine may build against meanwhile** — an implementer should never have to
guess what the engine does while a question is open, only know that the behaviour is interim.

| # | Question | Owner document |
|---|---|---|
| Q1 | **Mucking at showdown.** Mandatory universal reveal, TDA-faithful muck with signed forfeiture, or delayed reveal at end of tournament. Each changes the cryptographic protocol, not just the engine. Carried unresolved from `POKER_RULES.md` A8. `config.showdown_policy` keeps both branches alive; the MVP's use of `MandatoryReveal` is implementation order, not a decision. **Named default:** the MVP implements `MandatoryReveal` and, per Q-01 below, **refuses** a table configured for a policy it has not implemented rather than playing it with T43 absent. | `PROTOCOL.md` / `DECISIONS.md` |
| Q3 | **Not blocking version 1 (D-015): no certificate is produced, so there are no signers to determine and every stall — one seat silent or five — ends at T57 alike. The question returns intact with the machinery, and the interim rule below is now the only rule.** **Hand-deadline certificate signers when several seats are simultaneously unresponsive.** `V(subject)` is unachievable with two seats gone. A certificate naming a *set* of subjects is **not** adopted; neither is attributing every non-voting seat; and neither is **excluding** a seat from `V` for being the subject of an older unmet deadline, which D-008 deletes because it let `V` be shrunk by assertion. **Interim rule (§8.4), implemented and numbered: T57**, and since P3 it is a rule that can actually fire — the terminal stage is witness-independent (§4.1), so it does not wait for the seat whose silence caused it. The hand ends at `hand_deadline_ms` with `attributed = []` and no chip movement (I27). **D-010 shrinks what is left open:** attributing somebody would now put a name in the transcript rather than chips in a stack, so this no longer decides who pays, only what the record says. It is kept open because a later version may give attribution teeth again and the artefact has to be right before it does. Carried as **OQ-E** and blocking for Phase 4. | `PROTOCOL.md` Q-02, `THREAT_MODEL.md` §9.2 |
| Q5 | **Rebuys, add-ons and late registration.** Out of scope for the MVP and absent from `RATED_SNG_POKERTH_V1`, but they would change §5.3 and §9.3 and should be designed for rather than retrofitted. **Named default: not supported.** No config parameter selects them, §9.3's end conditions do not admit a re-entry, and §9.3's end conditions do not admit a re-entry, and since P8 hand init step 0 has no seat-entry clause at all (Q6). | `DECISIONS.md` |
| **Q6** | **May a seat be added after `Seating`?** Cash mode's whole difference from tournament mode is that `ledger_in` can move again, and nothing in the corpus can move it: there is no `PLAYER_SEAT` message in `PROTOCOL.md` §4.10 or §4.11, no hand-boundary stage for one, and `Event::PlayerSeated` is consumed only by T1 and T2 in `Seating` (P8). **Named default, implemented: not supported.** A cash table forms in `Seating` and thereafter only loses seats; §5.3 step 0 has no seat-entry clause and §9.4 no longer describes one; I1's right-hand side is non-increasing after the first hand in both modes. Adding it needs a message type, a stage kind under §3.2's principle, and a `HandComplete \| Paused × PlayerSeated` row — in that order, and not before a mode that needs it ships. | `PROTOCOL.md` §4.10/§4.11, `DECISIONS.md` |
| **Q8** | **What ratifies a contribution to the stage that stalled?** D-013 gates both `HAND_INIT`'s required emitter set and `dealt_in` on *"seat `s` signed at least one chained event during hand `k`"*. For every stage that **completed**, that set is exactly `stage_hash` membership (`PROTOCOL.md` §3.2) and two peers holding the same prefix agree on it by construction. For the **one stage that stalled** — the reason the hand aborted — no `stage_hash` exists, so "seat `s` contributed there" is, strictly, *who was heard*, which is the quantity P3 refused when it built the terminal stage. It cannot simply be excluded: a hand that stalls at `HAND_INIT` completes no stage at all, so excluding it makes the next hand's set empty and the table `Paused` (§5.3 step 9) instead of playing. **Named default, implemented: a contribution to the stalled stage counts**, and the terminal `HAND_ABORT` does **not** (I31(c)) — that one exclusion is not optional, because the abort enters no `stage_hash` and its copies are routinely signed by different seats at different peers. The residual is bounded and loud rather than silent: two peers that disagree derive different `n(8) dealt_in`, each rejects the other's `HAND_INIT` copy, the stage does not complete and the hand aborts at the deadline — a hand lost and recoverable through §6.3's event request, not a chain fork. Closing it properly is a wire question — what ratifies a partial stage — and it is `PROTOCOL.md`'s. **Since K-3 the loudness has a second and better site**: `signed_this_hand` is inside checkpoint 8 (§5.2's box), so a disagreement about it is compared at the boundary of *every* hand, including the hands on which `HAND_INIT` self-completes because each peer believes the other is not required — which is `DECISIONS.md` **K-1**'s shape and the one interleaving the mutual-rejection argument above does not cover. The residual is unchanged and still `PROTOCOL.md`'s to close; what changed is that its failure mode is now detected rather than merely argued to be self-limiting. **Since K-1 the sub-question this row used to carry is answered and the claim of loudness above is corrected.** *"Bounded and loud rather than silent"* was **false**: run the mutual-rejection argument twice and both peers narrow to `{self}`, after which every collective stage self-completes and neither needs the other again — one lost hand is the first step of a permanent silent fork, not the whole cost. What replaces the claim is not a ratification but a detection, `PROTOCOL.md` §3.2's **solitary-stage rule**, whose engine half is §5.2's solitary box, T62, T63, §9.3 condition 0.6 and I33. And the sub-question is settled: **a peer's own emission counts into its own `signed_this_hand`** (§5.3 step 4(i)), and **a `PLAYER_LEAVE` counts into no `P`** (step 4(ii)). `Q8` itself stays open, is the same question as `PROTOCOL.md`'s `Q-10`, and is now labelled there as unclosable by ratification: agreeing the stalled stage needs a collective step at the point collectivity failed. | `PROTOCOL.md` §3.2, §4.4, `DECISIONS.md` |
| **Q-01** | **Is `showdown_policy = TDA_MUCK` offered at all?** This is `PROTOCOL.md` Q-01 seen from the engine, and it is listed here because §12 of that document names the engine's showdown path as what turns on it. It is *not* a second copy of Q1 above: Q1 asks which resolution the corpus should adopt, Q-01 asks whether the config value ships. **Named default, so no implementer has to guess what a build does with a value it does not implement:** §9.4's config validation is the gate, and a build validates `showdown_policy` against what it actually implements — a table whose value it does not implement **goes to `TableClosed` and is never played**, exactly as any other failing config parameter does, with no repair path. The MVP implements `MandatoryReveal` only, so an MVP client refuses a `TDA_MUCK` table rather than playing it with T43 silently missing. The value stays in the type (§7.7) and T43 stays specified. | `PROTOCOL.md` Q-01, `DECISIONS.md` |

**A note on the letters, because two of the rows below carry labels that have since moved (R-2).**
`DECISIONS.md` is authority for the corpus-wide `OQ-*` series and `PROTOCOL.md` §12 has adopted its
letters, so **OQ-A** now names the reference-engine question, **OQ-D** the dispute path that does
not need the accused's signature, and **OQ-F** whether the certificate and proof machinery is
produced at all. This document's own former uses of `OQ-A` and `OQ-D` named different questions;
both of those questions are closed, so the labels are annotated in place rather than reused, and
every live cross-reference in this document — §5.2, §8.4, §12 — is written against the adopted
scheme. `OQ-E` is `THREAT_MODEL.md`'s and is unchanged.

**And one collision avoided rather than tolerated, because the review series has now reached the
same letters this table uses.** The Phase 3 gates label their findings `Q1`…`Q4`, `P1`…`P8`,
`N1`…`N9`, and this table already holds a live `Q1` (mucking) and a closed `Q4` (`STATE_HASH`
frequency) that mean something else entirely. The two gate findings this document acted on in the
eleventh pass are therefore written **`Q1-e`** — the engine half of `PROTOCOL.md`'s `P2`, which
`DECISIONS.md` also carries as `P2-e` — and **`Q4-e`**, the `CheckpointState` fields, following the
`-e` suffix `N-1e`, `N-5e` and `P2-e` already use for *the half owed to this document*. A bare `Q1`
or `Q4` anywhere in this document means the row in this table and nothing else.

**Closed, recorded so the history is not lost.**

| # | Question as it stood | The answer |
|---|---|---|
| **Q7** | **What marks a seat `Absent` at all?** Carried for two passes, widened once by H1, and asked on the assumption that marking a seat `Absent` was what let the table keep playing after a seat vanished. | **Closed by D-013, and closed by dissolving it rather than answering it (J2).** Marking a seat `Absent` would never have helped: `PROTOCOL.md` §4.4's required emitter set names absent and sitting-out seats **in its inclusion clause**, so no status ever removed a seat from it. The status field was not the liveness gate and the question was about the wrong mechanism — which is how two passes of correct reasoning about `Absent` left the table frozen rather than slow. D-013 puts the gate on demonstrated participation in the chain instead (§5.3 step 4, I31), and **nothing marks a seat `Absent`, nothing needs to, and the variant stays unreachable on purpose** (§2.4, J5). What is still open is not this question but the narrower **Q8** above, which is about the participation predicate's one unratified input, not about a status. |
| Q4 | Whether `STATE_HASH` is exchanged at every phase boundary or only at hand boundaries. `SPEC_CS.md` §15 says "after critical transitions" without enumerating them, and this document made every phase hashable so that either policy would be implementable. | **Closed** by `PROTOCOL.md` §6.2, which enumerates them normatively — seven checkpoints, "at these points and no others": after `TABLE_READY`, after `DECK_COMMIT`, after each of the four betting rounds closes, and immediately before `SHOWDOWN_REVEAL`. It is neither of the two candidate policies but a third, chosen for a reason this document must not undo: **no checkpoint is placed after a hole card has been opened to anyone but its owner**, because a checkpoint after the cards are known is an abort trigger the loser can pull with full information (§6.3 case (c), `THREAT_MODEL.md` X29). The engine's part is unchanged — every phase is hashable — but the *placement* is settled and is `PROTOCOL.md`'s, not an open choice. **Reopened once and closed again, wider (K-3):** those seven were the wrong *set*, not the wrong policy — every one of 2 to 7 needs a `DECK_COMMIT` or a betting round, so a `\|dealt_in\| == 1` drain hand placed none, and D-013 makes forty of those in a row the normal case. An **eighth** is added at the hand boundary (§5.2's box), which needs neither. It does not disturb the reason above: the boundary carries no hole card and no card of any kind, so the rule that no checkpoint follows a public opening is untouched, and Q4's answer — the placement is `PROTOCOL.md`'s — is untouched too. The row is `PROTOCOL.md`'s to add and is on `DECISIONS.md`'s open list as K-3. |
| **OQ-A** (as this document formerly used the letter; `DECISIONS.md` now uses `OQ-A` for the reference engine — see the note below) | Where `\|V\| < 2`, does an unfinishable hand restore stacks or forfeit? Restoration lets a losing player escape by going silent; forfeiture lets an opponent take an honest player's committed chips by asserting a deadline that did not pass. | **Closed by D-010**, and closed wider than the question was asked: an abort is neutral **on every path and at every table size**, not only below the floor, so there is no longer a configuration in which the two candidate answers differ. The escape D-010 reopens is its stated, accepted cost — §8.6 and §12 say so plainly rather than treating it as a residual. |
| **OQ-D** (as this document formerly used the letter; it is now `PROTOCOL.md` **Q-07**) | `HAND_ABORT cause = 4` / `AbortKind::StateDivergence`: restore, forfeit an unnamed party's commitment, or settle from the last `STATE_ACK`-agreed checkpoint? | **Closed by D-010** for its chip half: restoration, like every other abort (T54, T57, T60, §8.6). Of what is left, the *settle-from-the-last-agreed-checkpoint* half is `PROTOCOL.md` **Q-07** and the *adjudication* half — there is no procedure by which anyone establishes which peer diverged — is **OQ-A**. |
| Q2 | The envelope of an *unsigned* derived event (`HAND_COMPLETE`, `HAND_INIT`, `Settle`): what occupies `sender_public_key`, and whether peers counter-sign. | **Closed** by C-5 of `PHASE0_FIXPLAN.md`. There is no unsigned derived event. `HAND_INIT`, `HAND_COMPLETE` and `HAND_ABORT` are **collective signed** stages: every present peer computes the byte-identical body, signs its own copy under its own application key — which is what `sender_public_key` holds — and emits it. The counter-signature the question asked for *is* the mechanism, so the transcript carries attributable agreement by construction, and a peer that derives something different is rejected at the stage rather than detected later at a checkpoint. See §3.4. |

Two more questions in this document's earlier drafts were resolved rather than carried: the
`CERT_SETTLE_MS` settle window is deleted and the certificate stage is collective (§3.1, §8.4,
`PROTOCOL.md` Q-04 closed), and the "there is never a state in which both a valid action and a
valid certificate exist" claim is withdrawn as false rather than repaired (§8.4).

---

## 12. What this document does not claim

Per `SPEC_CS.md` §18 and its closing paragraph, and per §36's instruction not to smooth over a gap:

* This state machine does **not** make cheating impossible. It makes a defined set of
  *protocol-level* faults either impossible (an unverified card can never enter `board` or
  `revealed_hole`, because the engine only ever accepts a `CardsOpened` verdict from the crypto
  layer) or *detected and attributed* (illegal action, out-of-turn shuffle, early reveal token,
  missing artefact).
* It has nothing to say about collusion between players who share their hole cards out of band,
  malware on a player's machine reading that player's own cards, screen sharing, multi-accounting,
  physical coercion, traffic analysis, or denial of service. Those are endpoint and real-world
  attacks (`SPEC_CS.md` §18) and no state machine addresses them.
* A malicious player can always force a hand to abort by going silent (§8.6). That is a permanent
  property of the `n`-of-`n` construction, not a bug to be fixed later, and the alternative
  (`t`-of-`n`) is refused because it is strictly worse. **Under D-010 that abort also lets the
  quitter escape a losing pot, on every path, at every table size** — there is no configuration in
  which quitting costs what folding would have cost. That is D-010's stated and accepted cost, not
  an oversight; §8.6 gives the reasoning and §8.7 gives what is left in its place, which is
  visibility and nothing else.
* **No proof, certificate or attribution in this document moves a chip or removes a player**
  (D-010 points 2 and 3). An `AbortRecord` names a seat; that name is evidence in the transcript.
  It does not forfeit, does not unseat, does not block-list, does not penalise, and **since H1 it
  changes no seat's `status` either**: T46's `status := Absent` is deleted, because which
  `HAND_ABORT` copy a peer accepted is a per-receiver quantity and D-012 forbids canonical state
  being derived from one (§5.2, §8.6, I30). Attribution now causes **no state change at all**.
  Anything in another document that reads a consequence into an attribution is describing a version
  of this protocol that does not exist yet.
* **There is no enforceable deadline of any kind, at any table size (D-015), and this bullet is
  unscoped where it used to be scoped on `|V| < 2`.** An opponent who is present and stalls, or who
  goes silent mid-hand, cannot be punished inside the protocol: no certificate is produced, so the
  action deadline produces no signed transition by anyone but the stalling seat itself, and the
  crypto deadline produces nothing at all. The hand still ends — at `hand_deadline_ms`, never below
  `HAND_DEADLINE_MIN(n)` (`PROTOCOL.md` §8.2), with nobody attributed and stacks restored (T57) —
  but nothing about it is a punishment. **What changed with D-015 is the scope of the statement and
  not the statement**: it was already true of every heads-up table, and heads-up is the first
  shipped mode (§9.5), so the regime the MVP runs in is unaltered. **What is gained for that is
  named rather than implied**: the `|V| >= 2` case was the one in which a coalition of every other
  dealt-in seat could steal an honest player's action through a certificate valid by construction
  (`THREAT_MODEL.md` X10), and that attack has no artefact any more. The old scoping rule survives
  as a bar on future edits (§5.2): a rule written on the seat count is a defect even now that no
  rule is written on `|V|` either.
* **Liveness is not owed in general, and this document still does not claim it in general.** An
  adversary that keeps sending — voting, stalling, equivocating, diverging — can cost the table a
  hand at a time, repeatedly, and two colluding seats can do it at any table size (Q3).
  `SPEC_CS.md` §19 ranks security above finishing a hand conveniently and D-009 rule 2 applies that
  ranking literally, in preference to an effect a single signature could manufacture.
  **What *is* claimed is checked phase by phase in §12.1: every phase has a reachable exit under
  every adversarial behaviour, including a peer that simply stops — and, since D-013, the cycle
  those exits form makes progress rather than repeating a fixed point.**
* **A seat that goes silent costs at most two hands, and this is the claim the previous revision
  got wrong in the other direction.** It said "a stall costs `hand_deadline_ms` per hand and can be
  repeated every hand", which reads as a bound on grief and was not one: nothing removed the silent
  seat from `HAND_INIT`'s required emitter set, T46 restores every stack so nothing could bust, and
  no §9.3 condition could fire — **the table repeated one identical hand of one `hand_deadline_ms` forever, and no
  participant had an action that could end it**, because `PLAYER_SIT_OUT` and `PLAYER_LEAVE` are
  single-writer by the seat itself, so the only human who could sit the silent seat out was the
  silent one. That was J2. Under **D-013** the corrected statement is:

  > **A seat that goes silent stalls one hand — the hand it stopped signing in — and, if it stopped
  > after signing something in that hand, the hand after it as well; from then on it is skipped.
  > Every later hand is played among the seats that are actually there, at normal speed. The
  > skipped seat keeps its seat and its stack, posts its blinds and antes when the ring reaches it,
  > cannot win the money it posts, and busts. Its stack strictly decreases every orbit, so §9.3's
  > end conditions become reachable and the table ends.**

  The cost of one vanished seat is therefore **one `hand_deadline_ms` if it vanished at a hand
  boundary, two if it vanished mid-hand** — one or two whole deadlines, whatever the founder
  advertised — and nothing after that. §12.1 derives it and gives the bust bound. **K-3 adds a third case and no time**: a seat
  that vanishes after emitting the hand's `HAND_COMPLETE` copy stalls the boundary checkpoint
  instead of the next hand, for the same one `hand_deadline_ms` it always cost, and T61 closes it
  (§12.1.2). The worst case over all three is still two, because a boundary can only be gated after
  a hand that completed, and a hand that completed did not also stall. An implementer must still not close the
  remaining hand or two locally, from a dropped connection or a missed heartbeat: those are not
  chain content, and the local fix forks the chain (D-012, I31).
* ~~**Conversely, the certificate's protection at `|V| >= 2` is not a protection against
  collusion.**~~ **Withdrawn by D-015: there is no protection to qualify.** The claim was that a
  certificate needed *every* seat still in the voter set to sign, so `|V|` colluders defeated it —
  two of them at a three-seat table — and that this document claimed only that one could not be
  produced unilaterally. Nothing is produced at all now. The residue worth keeping is the reading
  discipline it was making: `SPEC_CS.md` §18 forbids reading a protection into a mechanism beyond
  what it delivers, and a mechanism that is not built delivers nothing, which is easier to state
  honestly than a mechanism that is built and inert.
* **A single peer that publishes a `state_hash` it did not derive can fault any table at any
  time**, at every `n`, and recover its own commitment (T54, §8.6) — as can any peer on any other
  abort path, since D-010. No peer is named, because
  naming one would require an observer-independent derivation that does not exist at run time. The
  evidence is preserved and is sufficient for a human, or for a future adjudicator, to diagnose the
  divergence; **no adjudication procedure is specified, offline or live, and none is claimed**.
  This sentence previously read "the evidence is adjudicable offline and not live", which asserted
  a reference engine that does not exist; it is withdrawn here as `PROTOCOL.md` §6.4 withdrew it,
  and defining that reference is OQ-F (`PROTOCOL.md` §12). The chip half of the old OQ-D is closed
  by D-010 (§11); the adjudication half is not solved and is OQ-F. `THREAT_MODEL.md` X29 carries
  the griefing cost.
* **This document specifies *the* deterministic engine as a thing every client implements — not a
  reference implementation a third party can be pointed at.** No section of it defines a named,
  versioned binary, §1 defers even the source layout to a `docs/ARCHITECTURE.md` that does not
  exist before Phase 2, and nothing here says how two parties would establish that they ran the
  same one. Any claim elsewhere in the corpus that
  evidence is "adjudicable by anyone who runs the reference engine over it" is appealing to
  something this document does not provide, and is to be read as withdrawn (N2, OQ-F).
* An absent seat cannot win the blind it posts. This is a **deliberate, documented deviation from
  TDA rules** (D-005), forced by the fact that opening an absent player's cards at showdown would
  require exactly the mechanism `SPEC_CS.md` §19 forbids. The practical difference is small;
  the deviation is real and is stated rather than hidden.
* **A seat *is* blinded off automatically when its client vanishes, which is D-005's other half,
  and it is met — but not through a status.** Nothing in this document enters `SeatStatus::Absent`
  and nothing needs to (§2.4, J5): the marking that once did was derived from the accepted abort
  copy's `attributed`, a per-receiver quantity D-012 forbids, and under D-013 the seat is skipped
  because it stopped signing, not because a field says it is away. The effect D-005 asks for — the
  seat keeps its stack, takes no cards, pays its blinds and antes, drains and busts — is delivered
  by §5.3 step 4's `dealt_in` gate and steps 6 and 7's positional posting. What is **not** claimed
  is that the seat's `SeatStatus` reflects its absence: to every reader of that field a vanished
  seat is still `Active`, and the GUI must derive "away" from `dealt_in` and the participation set,
  never from `status`. The two routes into `SittingOut` are unchanged and are still the seat's own
  (`PlayerSitsOut`, §8.5's auto-action limit).
* **Every recorded deviation is listed once**, in the register at `THREAT_MODEL.md` §9.1.1. This
  document is a source for four of its rows and restates none of them: the once-per-table randomness
  beacon (§7.9, row 2), no burn cards (§7.8, row 3), the absent seat that pays and cannot win
  (row 4 — implemented in §7.5's `dealt_in` filter since P7, which is where it previously was not;
  since D-013 the seat reaches that state by having stopped signing, rather than through its own
  `PlayerSitsOut` or an abort's attribution — which changes what puts it there and nothing about
  what happens to it), and the advisory deadline where `|V| < 2` (§8.4, §9.5, row 5 — recorded as
  the "advisory heads-up deadline", which is that row's common case; D-008 widens its scope from the
  seat count to the voter set and the register wording should follow). **A fifth row is now owed:
  the neutral abort of D-010**, which deviates from D-005's forfeiture rule and from the live-poker
  analogue that a player who cannot act loses what they bet. **The sixth row this bullet used to
  ask for — "no seat is marked absent automatically" — is withdrawn rather than filed.** It was
  written as a deviation from D-005 on the belief that D-005's blinding-off had been lost with
  T46's marking. It had not: what D-005 asks for is that the game go on and the absent seat's stack
  be eaten by the blinds, and D-013 delivers exactly that without any marking (§5.3 step 4). The
  seat's `SeatStatus` not reflecting its absence is an internal fact about which field carries the
  gate, not a deviation from a rule of poker, and filing it as one would have put a non-deviation
  in a register whose value is that everything in it is real. A deviation that is not in that
  register has not been recorded, whatever any single document says about it.
* **The hand-boundary checkpoint detects a disagreement; it does not decide one, and it is worth
  exactly the number of peers still signing (K-3).** Checkpoint 8 makes every hand — drain hands
  and aborted hands included — emit one comparable value, which is what the corpus had none of in
  D-013's steady state. What follows a mismatch is `PROTOCOL.md` §6.3's reconciliation and, at its
  worst terminus, a faulted table with **nobody named** (T54, §12's bullet above). So the claim is
  *"two peers that have forked find out"*, and specifically not *"the fork is resolved"*, *"the
  peer at fault is identified"*, or *"the result is verified"* in any sense stronger than that every
  seat which was still signing derived the same thing. On a table drained by a silent opponent the
  boundary stage completes at the one remaining seat, and a comparison with nobody is not a check.
  An implementer must not read the gate on T47 as an assurance about the tournament result; it is
  an assurance that the result was **offered** for comparison before the phase stopped accepting
  events, which is the most a terminal phase can offer.
* The open questions in §11 are open. Anything downstream that assumes an answer to one of them is
  assuming something this document did not say.

---

---

### 12.1 Termination — every phase, every adversarial behaviour, including a peer that stops

**This is stated as a table because the last four revisions each believed it and were each wrong
about a different phase.** The exit named for each phase is the one that fires when *nothing else
can happen* — no further event from any peer, ever. Adversarial behaviour that is not silence is
strictly easier: an event that arrives is either accepted, in which case the phase advances, or
rejected under I21, in which case the state is bit-identical and the silence case still applies. So
the total-silence column is the whole proof obligation, and each row names the transition that
discharges it.

The phase numbers are this document's own (§5.1): **17 is `HandAborted` and 18 is `Paused`.**

| # | Phase | Exit under total silence | Fires after | Terminates in |
|---:|---|---|---|---|
| 1 | `Seating` | **T4** `FormationAbandoned` — a local lobby timer: no certificate, no signature, nobody named | `join_deadline_ms` from the first `JOIN_ACCEPT` | `TableClosed`, absorbing |
| 2 | `AwaitingSeatRngCommit` | **T4**, same row, guard `hand_id == 0 ∧ ledger_in == 0` | `join_deadline_ms` from `TABLE_READY` (§5.2's named default) | `TableClosed` |
| 3 | `AwaitingSeatRngReveal` | **T4**, same. **This is also G4's row**: a seat whose opening does not match its commitment is now a `Rejection` (T11) rather than an unseating, so the beacon stalls and lands here | as above | `TableClosed` |
| 4 | `AwaitingKeySetup` | **T57** `HandDeadlineAbort`. **Includes a `HAND_INIT` that never completes**, because the deadline runs from `TERMINAL(k−1)` (§8.2, R-1) | `hand_deadline_ms` from `TERMINAL(k−1)` | `HandAborted` → T46 → `HandComplete` → T47 |
| 5 | `AwaitingShuffle` | **T57** | as above | as above |
| 6 | `AwaitingDeal` | **T57** | as above | as above |
| 7 | `BettingPreFlop` | **T57** | as above | as above |
| 8 | `AwaitingFlopReveal` | **T57** | as above | as above |
| 9 | `BettingFlop` | **T57** | as above | as above |
| 10 | `AwaitingTurnReveal` | **T57** | as above | as above |
| 11 | `BettingTurn` | **T57** | as above | as above |
| 12 | `AwaitingRiverReveal` | **T57** | as above | as above |
| 13 | `BettingRiver` | **T57** | as above | as above |
| 14 | `AwaitingShowdownReveal` | **T57** | as above | as above |
| 15 | `Settling` | **T57**. `Settling` waits for the `HAND_COMPLETE` stage to complete (T45), and a seat that goes silent between the last reveal and its own copy of that stage would otherwise hold it open forever — hand `k`'s deadline is still running, because `TERMINAL(k)` is exactly what has not been fixed | `hand_deadline_ms` from `TERMINAL(k−1)` | `HandAborted`; **no award has been applied**, so restoration is coherent |
| 16 | `HandComplete`, entered from **T46** | **T47**, derived `NextHand`. No external input; the boundary checkpoint is emitted and compared here but gates nothing on this path (§5.2) | immediately | `AwaitingKeySetup` \| `Paused` \| `TableClosed`, per §9.3 |
| 16 | `HandComplete`, entered from **T45** | **T61** `HandDeadlineAbort`, naming hand `k+1`. T47 is gated here on checkpoint 8 completing over `P(k)`, so under total silence it is T61 that fires — a timer, not a peer | `hand_deadline_ms` from `TERMINAL(k)`, which `PROTOCOL.md` §8.2 already starts there | `HandAborted` → T46 → `HandComplete` **entered from T46**, which is the row above and is ungated (I32(c)) |
| 17 | `HandAborted` | **T46**, derived `AbortSettle`, guard `—`. No external input | immediately | `HandComplete` |
| 18 | `Paused` | **idles, and that is correct**: no hand is live, no chips are committed, no deadline is armed, `Σ committed_hand == 0`. Left by T59 on a `PlayerSitsIn`, under §5.3 step 4's own predicate since K-7. A `Paused` table every seat has left **stays** `Paused` — §9.3 is evaluated only at T47 and T47 does not run from here — which is idle, not frozen, and is corrected wording rather than a new exit | — | nothing is owed to anybody; a client closes the window |
| 19 | `TableClosed` | terminal; nothing is owed and no exit is needed. **Since K-9 it is absorbing for thirty-eight event types and not for `SolitaryDivergence`** (T63), whose Next is `TableClosed` again — so the phase acquires a self-edge and **no** exit, and this row's obligation is unchanged | — | — |
| 20 | `Diverged`, hand live | **T57** — the hand deadline is **not** disarmed on entry (T50, T62) and phase 20 is in T57's scope | `hand_deadline_ms` from `TERMINAL(k−1)`, still running | `HandAborted`; `table_faulted` **unchanged**. **Where the entry was T62, `solitary_contradicted` is set and §9.3 condition 0.6 closes the table two transitions later (K-9)**; where it was T50 the table plays on as before — **unless that checkpoint had a one-member required set, in which case N1 sets the same latch and the same two transitions close the table**. It is the regime that decides the disposition, never which of the two rows detected it |
| 20 | `Diverged`, no hand started | **T4**, guard `hand_id == 0 ∧ ledger_in == 0` — reachable because checkpoint 1 sits after `TABLE_READY` and before any `HAND_INIT`. **T62 cannot reach this row**: `solitary_since` is written only at hand init, so the solitary freeze needs `hand_id ≥ 1` | `join_deadline_ms` | `TableClosed` |
| 20 | `Diverged`, no hand live, `hand_id > 0` | **T61** — reachable since K-3, because checkpoint 8 sits at a hand boundary, so T50 can freeze the table where neither T4's guard (`hand_id == 0`) nor T57's (a live hand) holds. **Since K-9 this is also the row T62 lands in most of the time**, because a solitary hand self-completes and the contradicting event arrives after it has closed, and **since `P1` T50 reaches it too**, on the retained boundary checkpoint of the hand just ended | `hand_deadline_ms` from `TERMINAL(k)` | `HandAborted`; then `table_faulted` **unchanged** (`cause = 1`, `PROTOCOL.md` §6.4) and the table plays on **unless** `solitary_contradicted`, in which case §9.3 condition 0.6 gives `TableClosed` |

**Five things the table asserts that a reader should check rather than take on trust.**

1. **Phases 4–15 and 20 no longer depend on an artefact that cannot be accepted.** That was G1: the
   terminal `HAND_ABORT` collided with its own emitter's contribution at the stalled stage, so every
   receiver rejected it and T57 could never fire — on the MVP's shipped regime, which is *every*
   heads-up stall, not a corner. It is closed in `PROTOCOL.md` §5.2.1 by putting `event_type` in the
   slot key, and this document reproduces no part of that key. No row above rests on a rule this
   document invented.
2. **No exit reads a certificate, a voter set, or `|V|` — and since D-015 there is no certificate
   and no voter set for one to read.** T4, T45, T46, T47, T57 and T61 are the whole of the column
   and none of them ever touched `V`. That was already what made the table true heads-up, where
   `|V| = 1` in every hand and every certificate was inert (D-009 rule 2); it is now true for the
   stronger reason that the six rows which did read `V` are deleted (§5.2). It is also why the
   table does not change shape between two seats and nine, and why deleting those rows moved no
   exit — the re-derivation is §12.1.1's D-015 table.
3. **No exit requires the silent peer to do anything.** T4 is a local timer; T45, T46 and T47 are
   derived from state; T57's and T61's stage has no required emitter set (`PROTOCOL.md` §3.2), so
   it closes on the first copy from any peer. A peer that stops cannot hold any phase open, which is
   the property the whole table exists to establish. **T47 is the one exit that can now be
   blocked, and that is why T61 exists**: on the settled path it waits for checkpoint 8's stage, so
   it is no longer a valid entry in the *Exit under total silence* column at all, and the row above
   names T61 instead. The gate is admissible only because it is one-sided (I32(b)) and only because
   the timer behind it was already running (§8.2).
4. **Every exit leads to a phase that has an exit, and the walk terminates.** Rows 1–3 and the
   second row of 20 reach the absorbing `TableClosed` directly. Rows 4–15 and the first row of 20
   reach `HandAborted`, which T46 leaves unconditionally, reaching `HandComplete` **on its ungated
   entry**, which T47 leaves unconditionally for `AwaitingKeySetup` (back to row 4), `Paused`
   (row 18, which owes nobody anything) or `TableClosed`. The cycle 4 → 17 → 16 → 4 makes progress
   on the one counter that matters: `hand_id` strictly increases across it, and §9.3's conditions
   are re-evaluated every time round.

   **The gated entry to 16 adds one edge and closes it in the same step (K-3).** The only way into
   a gated `HandComplete` is T45, and its only silent exit is T61, which runs hand init — so
   `hand_id` strictly increases across that edge too — and lands in `HandAborted`, from which T46
   reaches the **ungated** entry to 16. So a gated boundary is followed by an ungated one, always,
   and the walk cannot sit on the gate twice for the same set. That is I32(c), it is asserted over
   a trace rather than over a state, and it is the check this note would have owed J2 one link
   earlier if the obligation had been written down then.
5. **The cycle's *next* hand can start, and — since D-013 — the cycle is not a fixed point.**
   These are two claims and both were false until this pass. Every row above ends a *hand*; what
   the next hand starts from is a link the walk does not look at, which is why two separate defects
   hid there.

   **H1 was the first.** Two peers that completed hand `k`'s terminal stage on different
   `HAND_ABORT` copies held different `status` vectors under T46's deleted marking, so their
   `HAND_INIT` copies for hand `k+1` differed in `dealt_in` and `bb_seat`, each rejected the
   other's, and row 4 was reached again with the same divergence. With the marking deleted, every
   field of `HAND_INIT` is a function of the previous hand boundary, and **I30(c)** is the
   assertion that says so.

   **J2 was the second, and D-012's fix did not touch it.** Even with every peer agreeing on every
   field, `HAND_INIT` is a *collective* stage: agreement makes it **completable**, not complete.
   Completion needs every member of the required emitter set to emit, and `PROTOCOL.md` §4.4's set
   was *"every seat that will be `dealt_in`, plus every occupied seat that is absent or sitting out"*
   — which no status removes a seat from. So the silent seat was a required emitter of hand `k+1`
   too, and of `k+2`, and so on. Each iteration cost `hand_deadline_ms`, T46 restored every stack,
   nothing busted, and §9.3's conditions — every one of which reads a stack, the fault flag or an
   empty table — could not fire. **Every row of the table above passed, and the table played no
   further hand, ever.** That is the difference between a termination argument and a liveness one,
   and it is why I30(c)'s final clause, which asserted completion, is deleted as false (J3): an
   invariant that asserts the thing the table cannot deliver hides exactly the trace that would
   have shown it.

   **Under D-013 the cycle has a strictly decreasing measure.** A seat that stops signing is not in
   hand `k+1`'s required emitter set (`PROTOCOL.md` §4.4) and is not `dealt_in` (§5.3 step 4), so
   `HAND_INIT(k+1)` completes among the seats that are there; but it still holds a position in the
   ring, so steps 6 and 7 still take its ante and its blind when the ring reaches it, and step 9's
   award gives that money to a seat that is. Its `stack` therefore strictly decreases at least once
   per orbit and is bounded below by zero, so step 2 marks it `Busted` in a finite number of hands
   and §9.3 condition 1 (tournament) or the seat simply ceasing to post (cash) follows. **`hand_id`
   was never the measure that mattered; a stack is.**

**Two exits that are new in this revision, so the table is not read as unchanged.** Row 3's exit
changed because T11 stopped unseating (G4): the previous revision left `AwaitingSeatRngReveal` on a
commitment mismatch by removing the seat, which was a peer removal D-010 point 3 forbids; it now
stalls to T4 like every other beacon failure. And the first row of 20 no longer faults the table:
`PROTOCOL.md` §6.4 reserves that for `cause = 4`, so a divergence that times out ends the hand and
the table deals the next one (§5.2, T57's `Diverged` note), whereas a divergence that reconciles
into case (c) closes the table through T54 and §9.3 condition 0.5.

**What the table does and does not claim, said plainly, and corrected.** It is a *termination*
argument: every phase has a reachable exit. The previous revision then read that as "slow is the
worst case", which was wrong — one whole `hand_deadline_ms` a hand, every hand, **forever**, is a passing row here
and it is not slow, it is a table that never plays again (J2). The table's rows were all true and
the sentence drawn from them was not. Since **D-013** the stronger claim is available and is made
below, and the honest scope of the table itself is: *no phase is frozen; whether the sequence of
hands makes progress is a separate question, and it is answered in the two blocks that follow.*

---

#### 12.1.1 Progress under D-013, K-3, K-9, L7, D-014 and D-015 — the twenty phases re-derived, and the exit for each

**Read the D-015 table first and the D-014 table second.** The first is this pass's
re-derivation and is where the exits are reported; the second is the previous pass's and is
kept because its *Changed by* column carries five rulings the first does not restate. Neither
supersedes the other and they agree row for row — which is the check, since D-015 removed six
transitions between them.

**D-013 changed no exit.** It narrows `dealt_in` at §5.3 step 4 and narrows `PROTOCOL.md` §4.4's
required emitter set on the same predicate, and neither is read by T4, T45, T46, T47 or T57.

**K-3 changes exactly one, and adds one transition to carry it.** `HandComplete` acquires a gate on
the settled path, so T47 stops being that phase's silent exit there, and **T61** — the hand-boundary
counterpart of T57 — becomes it. Nineteen of the twenty rows are untouched; phase 16 splits by
entry, and phase 20 gains the boundary case T50 can now reach.

**K-9, L7 and D-014 change no exit at all, and the reason is worth stating before the table rather
than discovered in it.** All five new transitions — T62, T63, T64, T65, T66 — are triggered by an
**event**, and this table's whole subject is what happens when no event ever arrives again. An
event-triggered row can never be an entry in the *Exit under total silence* column, so it cannot
displace one; the check it owes instead is the other one, and it is discharged in the table's
right-hand column: **does it lead anywhere that has an exit?** T62 and T63's target, `Diverged` and
`TableClosed`, are both already in the table. T64's target, `HandAborted`, is row 17. T65 and T66
change no phase. L7 changes a condition inside T47, not T47.

**What does change is two destinations, and one of them is the point of the pass.** §9.3 gains
condition 0.6, so the `Diverged` rows' *Terminates in* column now branches on
`solitary_contradicted`; and `TableClosed` acquires a self-edge (T63) without acquiring an exit,
which leaves row 19's obligation exactly where it was.

**`P1`, `N-1e`, `P2-e` and `Q4-e` change no exit at all, and the re-derivation is a check against
that claim rather than a restatement of it.** All four were run against every one of the twenty
rows below, and the three questions each had to answer are the three this section keeps apart.

* **`P1` — the checkpoint store.** *Does any slot add a wait?* **No.** T47's gate is the only gate
  in the document and it reads `checkpoints.live`, exactly the object it read before; `agreed` and
  `boundary` are written at a hand init and read by T50, T51, T64 and T65, none of which blocks a
  phase. *Does it add a phase, or a way into one?* It adds **two ways into `Diverged` and one into
  nothing at all**: T50 can now fire on `checkpoints.boundary` while hand `k+1` is live (phase 20,
  hand live → T57) or while the table sits at the next boundary (phase 20, no hand live → T61),
  and both destinations are rows that already exist and already have exits. T51 changes no phase.
  *Does it add an allocation an event can drive?* **No** — three fixed slots, and an event naming
  no slot is a `Rejection` (I21, §2.6's bound).
* **`N-1e` — the floor.** It is a **guard on an event-triggered row**, T62 and T63, and this
  section's whole subject is what happens when no event arrives. It cannot appear in the *Exit
  under total silence* column and cannot displace one. What it changes is the **destination**
  column, in the direction that removes destinations rather than adding waits: a contradiction the
  engine used to drop now sets the latch one hand earlier, so §9.3 condition 0.6 is reached from
  the fork hand instead of from the hand after it.
* **`P2-e` — the readmission set, with its union deleted.** T67 changes no phase and is
  event-triggered, so it is not an exit. What it used to touch was **the quantity three steps of
  hand init read**, and that is what this pass removed: since `P2` the set reaches `dealt_in`,
  `solitary_since` and §9.3 not at all, so the check *widening `dealt_in` cannot make a phase
  unleavable* is no longer needed — nothing widens `dealt_in`. **Every exit in the column below is
  T4, T45, T46, T47, T57 or T61 and none of them reads `dealt_in`, `signed_this_hand` or
  `readmit`**, which is note 2's property re-checked against the narrowed predicate. **And the
  progress cost that stood here is discharged rather than carried**: the previous pass recorded
  that `P2` made the stall unbounded — a replayed agreeing checkpoint-8 `STATE_HASH` re-entered
  its sender into a **required** emitter set once per hand, so §12.1.2's *two hand deadlines and
  then normal speed* did not hold on any trace containing one. `PROTOCOL.md` §4.9 removed the
  required-set enlargement and this pass removes the engine's copy of it, so the amplifier has no
  input on either side and §12.1.2's bound is unconditional again. What a readmission costs now is
  **one hand of latency for the returning seat** — it is accepted at `m+1` and dealt in at `m+2` —
  and that is a schedule, not a stall: no phase waits on it, because no required set contains it.
* **`Q4-e` — the checkpoint record's two new fields.** *Does it add a wait?* **No.** It changes the
  **terms** of T49, T50 and T51 and none of their triggers, targets or side-effect shapes; the only
  gate in the document is T47's and it reads `heard ⊇ required`, which `Q4-e` does not touch. *Does
  it add a way into a phase?* **No** — T50 reaches `Diverged` on exactly the traces it reached it
  on before, since `e.state_hash != c.own` is the same predicate the count was trying to express;
  what changes is that it is now computable on `checkpoints.boundary` and `checkpoints.agreed`,
  where before it was computable nowhere. *Does it add an allocation an event can drive?* **No** —
  `dissent` is written at most once per record, by `or`, and the three records are still fixed.

**None of the three touches a *Fires after* cell, so the column below is unchanged from the ninth
pass and the twenty rows are re-derived rather than rewritten.**

**N1 changes no exit either, and it changes the same column twice more.** T50 now sets the latch
when the divergence is a solitary one, so a `Diverged` row entered by a checkpoint mismatch at a
solitary peer terminates in `TableClosed` where it previously terminated in another hand; and T53
now needs two distinct signers, so the *release* from `Diverged` stops being something the frozen
peer can produce alone. Neither touches the *Exit under total silence* column, for the reason given
above and once more: both rows need an event, and this table's subject is what happens when no event
ever arrives. The *Changed by* column below carries all
five rulings, and the re-derivation is a check against them, not a rewrite:

**`G7-S1`, `G7-S2`, `G7-S4` and `Q6` change no exit either, and one of them removes an argument two
rows were leaning on.** All four are about durations and about where they are written down, and a
duration cannot be an exit: every entry in the *Exit under total silence* column is a transition,
and the timer that fires it belongs to the protocol layer (§8.2). Three of the four touch nothing
here — §9.1's second listing of the preset is deleted, `crypto_step_timeout_ms` gives the eleven
`ArmDeadline{Crypto}` sites the source they were already assumed to have (§2.6), and §9.4's
validator moves from `HAND_DEADLINE_FLOOR(seats)` to `HAND_DEADLINE_MIN(seats)`, which changes which
configurations reach phase 1 at all and not what any phase does once it is reached.

**The fourth is rows 2, 3 and the last row 20, and there the argument was wrong although the exit
was right.** §5.2 and §8.2 both justified T4's coverage of phases 2–3 by computing that the preset's
`join_deadline_ms` expires before hand 1's `hand_deadline_ms` window — true under
`RATED_SNG_POKERTH_V1` and **false** for a `CUSTOM` table that advertises a long join window, since
`n(17)` and `n(18)` are independent parameters with independent caps (`PROTOCOL.md` §7.2) and
`CUSTOM` is the only kind the MVP ships. The exit never needed the ordering: a `HandDeadlineAbort`
arriving in phases 1–3 satisfies **no** row of §5.2 — T57 requires a live hand, T61 requires
`HandComplete` or `Diverged` with `hand_id > 0` — so it is a `Rejection`, and T4 closes the table on
its own timer whichever expires first. Those three rows are unconditional now where they rested on a
preset figure before, which is a strengthening and not a change of exit. It is also the answer to
the fifth check below, run against this pass: the thing this pass removed is a *number*, and the
consumers of a number are arguments, not guards — so the sweep's obligation was to find every
argument that had quietly become load-bearing on one, and rows 2 and 3 are where one had.

**D-015 removes six transitions, and this is the section that has to prove no phase lost its
way out.** Removing a mechanism can orphan a path, so the re-derivation below is a check and
not a restatement. The argument in one line, then the table that discharges it phase by phase:

> **A certificate was never an exit under total silence, so removing every certificate row
> cannot remove an exit.** A `TimeoutCertificate` needed `|V(subject)|` peers to sign and gossip
> votes and then certificates. This column's subject is what happens when **no event ever
> arrives again**, and an event-triggered row can never be an entry in it. Every one of the
> twenty-one rows below was already discharged by **T4, T46, T47, T57 or T61**, and none of the
> five is or ever was a certificate row: T4 is a local lobby timer, T57 and T61 are
> `hand_deadline_ms`, T46 and T47 are derived and fire immediately.

**That argument is necessary and not sufficient, and the second half is what the deleted rows
actually cost.** A row that is not an exit can still be a *destination* — the check T62–T66 had
to pass — and the six deleted rows were destinations: five of them (T16, T22, T27, T41, T44)
led to `HandAborted`, row 17, and one (T34) stayed inside its betting phase. **Deleting a
destination cannot orphan a path either**, because the phases that reached it keep every other
route out of themselves; what it can do is make a phase *slower to leave*, and it does exactly
that in five of the twenty rows. That cost is real, it is one `hand_deadline_ms` instead of one
`crypto_step_timeout_ms`, it is stated in the table's right-hand column rather than absorbed,
and it is the same cost §8.3 of `PROTOCOL.md` already charged every `|V| < 2` table — which,
since heads-up is the shipped mode, was already every table this project runs.

**The third check, the one the eleventh gate sharpened: list the consumers of every check this
pass removed, including consumers the pass itself added.** D-015 removes
`Event::TimeoutCertificate`, six rows, `certified_subjects`, `V(subject)` as a live quantity,
and I29. Their consumers, enumerated and re-derived:

* **`consecutive_auto_actions` and `auto_action_limit`** were written by T34 alone. Their
  consumer is the `SittingOut` marking at the next hand boundary, which is therefore
  unreachable — named in §8.5 as the one behavioural loss, and named again in I30(a), whose
  exhaustive provenance list still contains the route and now contains a route nothing takes.
  **That is deliberate and it is not the `L4` shape**: I30(a)'s list is a prohibition on
  assignments *elsewhere*, and a prohibition with one fewer instance is still binding.
* **`certified_subjects`'s consumers** were the six guards, hand init step 8's clear, T57's
  "unchanged" note, T64's subtraction and I29. All five sites are edited; §2.8 carries the
  deletion record and the restore ordering.
* **The `|V| < 2` disposition's consumer** was T57, and T57 gains rather than loses: it was the
  terminus for the below-the-floor case and is now the terminus for all of them. **Nothing was
  added behind the removed guard in this pass**, which is the composition the rule warns about,
  and the check is recorded as run rather than assumed: no transition, invariant or §9.3
  condition was introduced by D-015.

**Twenty phases, twenty-one rows — phase 16 splits by entry and phase 20 by regime, as before.
The `Exit` column is identical to the previous pass's; the `Certificate row deleted from this
phase` column is the new one, and it is what makes the check auditable.**

| # | Phase | Exit under D-015 | Certificate row deleted from this phase | Does the deletion change the exit? |
|---:|---|---|---|---|
| 1 | `Seating` | **T4**, local lobby timer → `TableClosed` | none | **No.** T4 is a local timer and reads nothing |
| 2 | `AwaitingSeatRngCommit` | **T4**, guard `hand_id == 0 ∧ ledger_in == 0` → `TableClosed` | none — T8 was already deleted by `P4` | **No** |
| 3 | `AwaitingSeatRngReveal` | **T4**, same → `TableClosed` | none — T12 was already deleted by `P4` | **No.** D-015 generalises `P4`'s argument: it now holds because no certificate exists anywhere, not because the setup chain is special |
| 4 | `AwaitingKeySetup` (includes a `HAND_INIT` that never completes) | **T57** → `HandAborted` | **T16** | **No.** T57 was already this phase's exit under total silence. What changes is the *fast* path: a certified stall used to end in `crypto_step_timeout_ms` and now ends in `hand_deadline_ms` |
| 5 | `AwaitingShuffle` | **T57** → `HandAborted` | **T22** | **No**, same. A seat skipped by §5.3 step 4 is not in `shuffle_order` (I20), so the second hand does not stall here at all |
| 6 | `AwaitingDeal` | **T57** → `HandAborted` | **T27** | **No**, same; a skipped seat is owed no reveal token |
| 7 | `BettingPreFlop` | **T57** → `HandAborted` | **T34** (the row set is instantiated for all four betting phases) | **No**, and this is the row to read: T34's *function* is replaced rather than lost — the seat's own auto check/fold arrives as `Event::Action` and is consumed by **T29–T33**, so a betting phase whose human is absent still advances at `action_timeout_ms` and never reaches T57 |
| 8 | `AwaitingFlopReveal` | **T57** → `HandAborted` | **T41** | **No**; slower stall only |
| 9 | `BettingFlop` | **T57** → `HandAborted` | **T34** | **No**, as row 7 |
| 10 | `AwaitingTurnReveal` | **T57** → `HandAborted` | **T41** | **No**; slower stall only |
| 11 | `BettingTurn` | **T57** → `HandAborted` | **T34** | **No**, as row 7 |
| 12 | `AwaitingRiverReveal` | **T57** → `HandAborted` | **T41** | **No**; slower stall only |
| 13 | `BettingRiver` | **T57** → `HandAborted` | **T34** | **No**, as row 7 |
| 14 | `AwaitingShowdownReveal` | **T57** → `HandAborted` | **T44** | **No**; slower stall only. T43 (`Muck`) is untouched |
| 15 | `Settling` | **T57** → `HandAborted`, no award applied | none | **No.** `Settling` never had a certificate row — the settlement is computed on entry and the phase waits only for the `HAND_COMPLETE` stage |
| 16 | `HandComplete`, entered from **T46** | **T47**, derived, immediate → `AwaitingKeySetup` \| `Paused` \| `TableClosed` | none | **No.** T47 needs no external input and its guard is empty on this path |
| 16 | `HandComplete`, entered from **T45** | **T61**, `HandDeadlineAbort` for hand `k+1` → `HandAborted` → T46 → the ungated entry above | none | **No.** T47's gate reads `checkpoints.live.heard ⊇ required`, and neither term is a certificate, a voter set or `\|V\|` |
| 17 | `HandAborted` | **T46**, derived, immediate → `HandComplete` | none | **No.** This is the *destination* five deleted rows led to; a destination with fewer inbound edges keeps every outbound one |
| 18 | `Paused` | idles; left by T59 when §5.3 step 4's predicate would deal two or more seats in; nothing is owed to anybody | none | **No.** D-015 adds no way in and takes none away. §8.5's `SittingOut` marking is now unreachable, which removes one way a seat could *become* non-participating — it does not change `Paused`'s entry, which reads `\|dealt_in\| == 0` (L7) and not a status |
| 19 | `TableClosed` | terminal; no exit is owed | none | **No** |
| 20 | `Diverged`, hand live | **T57** → `HandAborted` | none | **No.** T57 covers `Diverged` with a live hand and reads no certificate |
| 20 | `Diverged`, no hand live, `hand_id > 0` | **T61** → `HandAborted` | none | **No** |
| 20 | `Diverged`, no hand started | **T4**, guard `hand_id == 0 ∧ ledger_in == 0` → `TableClosed` | none | **No** |

**Twenty-one rows, five distinct exits, and not one of them reads a certificate, a voter set,
`|V|`, `certified_subjects` or `consecutive_auto_actions`.** That is the same property the
column below asserts against `signed_this_hand`, `readmit`, `dealt_in` and a status, checked
again against the quantities this pass deleted. **The one near-counter-example, stated because
a reader will find it first:** rows 7, 9, 11 and 13 name T29–T33 as what keeps a betting phase
moving, and T29–T33 are event-triggered, so they are not exits under total silence — which is
why every one of those rows still names **T57** in the `Exit` column. The auto check/fold is a
*speed* property, not a termination one, and conflating the two is how a document ends up
claiming liveness it does not have.

**And the fourth obligation — name every path back to playing and check each needs something
the frozen peer cannot supply alone — is unaffected**, because D-015 deletes no detection, no
latch and no repair: T50, T53, T62, T63 and §9.3 condition 0.6 are untouched, and none of them
ever read a certificate.

| # | Phase | Exit under D-013, K-3 and K-9 | Changed by D-013 / K-3 / K-9 / L7 / D-014? |
|---:|---|---|---|
| 1 | `Seating` | **T4**, local lobby timer → `TableClosed` | no. D-013's base case (the required set for hand 1 is the signers of `TABLE_READY`) is fixed *in* this phase and read at T10. **D-014 adds nothing here and that is deliberate (T66)**: no removal is available in the setup chain, so this exit is untouched |
| 2 | `AwaitingSeatRngCommit` | **T4**, guard `hand_id == 0 ∧ ledger_in == 0` → `TableClosed` | no |
| 3 | `AwaitingSeatRngReveal` | **T4**, same → `TableClosed` | no. **This is the row D-014 looks like it reopens.** A commitment mismatch is tier-1 evidence, so the narrowing would put T11's deleted unseating back; **T66 refuses it**, because `roster_hash(k)`'s seat vector is frozen at `TABLE_READY` (`PROTOCOL.md` §3.1) and removing a seat here forks the genesis rather than punishing anybody. The beacon still stalls to T4, G4 and P4 stand, and nothing is lost: `ledger_in == 0`, so there are no chips to protect |
| 4 | `AwaitingKeySetup` (**includes a `HAND_INIT` that never completes**) | **T57** → `HandAborted` | **the exit is the same; what it leads back to is not.** This is the phase J2 lived in: the stage stalled here every hand, forever. Now it stalls at most twice for one silent seat, and hand `k+2` onward completes here in a round trip. **K-9 and D-014 change nothing here**: T62 and T64 are in this phase's scope, but both need an event, and a phase whose silent exit is a timer cannot lose it to a row that needs somebody to speak |
| 5 | `AwaitingShuffle` | **T57** → `HandAborted` | no. A skipped seat is not in `deck.participants` (I20), so it is not in `shuffle_order` and cannot stall the chain |
| 6 | `AwaitingDeal` | **T57** → `HandAborted` | no; same reason — it is owed no reveal token |
| 7 | `BettingPreFlop` | **T57** → `HandAborted` | no. It is `¬dealt_in`, so it is never `player_to_act` (I14) |
| 8 | `AwaitingFlopReveal` | **T57** → `HandAborted` | no |
| 9 | `BettingFlop` | **T57** → `HandAborted` | no |
| 10 | `AwaitingTurnReveal` | **T57** → `HandAborted` | no |
| 11 | `BettingTurn` | **T57** → `HandAborted` | no |
| 12 | `AwaitingRiverReveal` | **T57** → `HandAborted` | no |
| 13 | `BettingRiver` | **T57** → `HandAborted` | no |
| 14 | `AwaitingShowdownReveal` | **T57** → `HandAborted` | no |
| 15 | `Settling` | **T57** → `HandAborted`, no award applied | no. `HAND_COMPLETE`'s required set narrows with the same predicate, so a seat that stopped signing last hand does not hold this stage open either. **This is the phase a solitary hand passes through** (§5.3 step 9 sends `\|dealt_in\| == 1` straight here) and K-9 does **not** change that: the drain is preserved deliberately, `PROTOCOL.md` §3.2 refuses the alternative that would have deleted it, and the freeze is a later transition on a later event, not a branch taken here |
| 16 | `HandComplete`, entered from **T46** | **T47**, derived, immediate → `AwaitingKeySetup` \| `Paused` \| `TableClosed` | **K-9 and L7 change the branch, not the exit.** T47 still fires immediately and still needs no external input; what it decides has one more condition before it (0.6, the solitary latch → `TableClosed`) and one restated on `\|dealt_in\|` (2, L7). Both are inside T47's guard-and-branch, so the *Fires after* column still reads *immediately* and the row is still discharged by a derived event and never by a peer. And: **§9.3's conditions become reachable (D-013).** Condition 1 needs exactly one seat with chips, and until D-013 no stack could ever change on a stalled table. Now the skipped seat's stack strictly decreases every orbit. **K-3 leaves this entry ungated on purpose**: `P(k)` after an aborted hand contains the seat whose silence ended it |
| 16 | `HandComplete`, entered from **T45** | **T61** `HandDeadlineAbort` for hand `k+1` → `HandAborted` → T46 → the ungated entry above | **new with K-3, and it is the one exit this pass changed.** T47 is gated on checkpoint 8 completing over `P(k)`, so it is no longer the silent exit; T61 is, on hand `k+1`'s `hand_deadline_ms`, which §8.2 already starts at `TERMINAL(k)`. The gate is what gives T58 and T59 a window (K-3b) and what puts a tournament result through a comparison before `TableClosed` |
| 17 | `HandAborted` | **T46**, derived, immediate → `HandComplete` | no |
| 18 | `Paused` | idles; left by T59 when §5.3 step 4's predicate would deal two or more seats in; nothing is owed to anybody | **one new way in under D-013, and it is correct.** If *every* seat stops signing, hand `k+1`'s `dealt_in` is empty and §5.3 step 9 goes to `Paused` — an abandoned table that idles instead of burning a whole `hand_deadline_ms` a hand forever. It is not a new exit and it owes nobody anything. **K-7 changed the way *out*** and **L7 now changes the way *in* to match**: §9.3 condition 2 read `\|{s : status == Active ∧ stack > 0}\| == 0`, a second predicate for the question step 9 already answers, and it is now `\|dealt_in\| == 0`. Entry and exit finally read one quantity. No exit moves — neither predicate is an exit under total silence, which is why the defect was inert and why it survived K-7's own sweep |
| 19 | `TableClosed` | terminal; no exit is owed | **K-9 changes what "absorbing" means here without changing this row.** T63 gives the phase a self-edge on `SolitaryDivergence` — the result is retracted, the latch is set, no chip moves, no phase changes — so every other event is still a `Rejection` and the phase still owes no exit. It is recorded because §5.1 called the phase absorbing without qualification and a reader checking this table against that word must find the qualification somewhere |
| 20 | `Diverged`, hand live | **T57** → `HandAborted` | **new with K-9: the entry, not the exit.** T62 is a second way into this phase and the hand deadline is not disarmed on it either, so T57 covers it unchanged. What differs is downstream: `table_faulted` is still unchanged, but where the entry was T62 the latch is set and §9.3 condition 0.6 closes the table at the boundary this abort reaches. That is the difference between a checkpoint divergence, which costs a hand, and a solitary divergence, which must cost the table or repeat forever. **N1 corrects one word of that and the correction is the point of it: it is not the *entry* that decides, it is the *regime*.** T50 sets the latch too when the disputed checkpoint's required set has one member, so a checkpoint mismatch at a solitary peer *is* a solitary divergence and closes the table, while the same mismatch at a peer with company still costs one hand. Before N1 that one path detected the fork, froze, thawed on the timer and re-froze at the next checkpoint 8, for ever, with this row passing |
| 20 | `Diverged`, no hand live, `hand_id > 0` | **T61** → `HandAborted` | **new with K-3, extended by K-9, and this is the row the solitary freeze usually lands in.** K-3 opened it: checkpoint 8 sits at a boundary, so T50 can freeze the table where neither T4's guard nor T57's holds. K-9 makes it the common case rather than the corner one — a solitary hand self-completes, so the contradicting event almost always arrives after that hand closed and finds the peer at a boundary. `table_faulted` unchanged (`cause = 1`); `TableClosed` follows through condition 0.6 when the latch is set. **`P1` widens the way *in* and not the way out**: T50 can now fire on `checkpoints.boundary`, so a stale boundary mismatch reaches this row instead of being dropped, and T61 is its exit exactly as it is for a live checkpoint |
| 20 | `Diverged`, no hand started | **T4**, guard `hand_id == 0 ∧ ledger_in == 0` → `TableClosed` | no. **T62 cannot reach this row**, because `solitary_since` is written only at §5.3 step 8 and therefore requires `hand_id ≥ 1`; the row's guard and the freeze's precondition are disjoint by construction rather than by inspection |

**None of the twenty-one rows' exits reads `signed_this_hand`, `readmit`, `dealt_in`, a
checkpoint record's `own` or `dissent`, a
status, a certificate or `|V|`** — which is the same property note 2 above asserts, checked again against every new
predicate. Three near-counter-examples, all of them clean, and each is worth one line because a
reader will find them before finding the reasoning. a compared checkpoint's `required` **is**
`signed_this_hand`: not a counter-example, because T47 is not an exit under total silence on the
path where it reads the set, which is exactly why the row for that path names T61 instead. **§9.3
condition 2 now reads `dealt_in` (L7)**: not a counter-example, because §9.3 decides where T47
*goes*, not whether it fires, and a branch destination that reads a participation set cannot hold a
phase open — the previous form of the condition read a status and was no safer, only less
consistent. **§9.3 condition 0.6 reads `solitary_contradicted` (K-9)**: not a counter-example for
the same reason, and it is the one entry in this whole table whose effect is to make a phase
*stop* rather than continue — it removes destinations, it adds no wait. A seat that stops signing
cannot hold a phase open, it cannot hold the *table* open, it cannot hold the *boundary* open, and
it cannot — this pass's check — hold the **freeze** open either: T62's target has the same exits it
had before T62 existed.

**A fourth obligation, written down beside the other three, because K-9 is the first fix that could
have failed it.** Termination asks whether a phase can be left; progress asks whether the sequence
of hands advances; I32 asks whether anything was said that two peers could disagree about out loud.
The fourth is:

> **A fifth check, and this pass is the first that had to run it against itself.** The rule the
> last six gates have been graded on is *the worst defect is on the path the previous fix newly
> made load-bearing*, and the eleventh gate sharpened it: **one fix can delete a guard while
> another adds a disposition behind it, in the same pass**. So the obligation is now stated as an
> ordering rather than a comparison with what came before: **after every addition in a pass has
> landed, list the consumers of every check the same pass removed — including consumers the pass
> itself added — and re-derive each one.** This pass removed two things and added three, and the
> composition it had to check is named where it lives: T50's cell no longer argues from
> `Event::StateHash` carrying no `hand_id`, because an earlier pass gave it one (§4.1); T64's guard
> no longer names an exact checkpoint number, because the store cannot hold one (T64); and §5.3
> step 8's regime read moved from `signed_this_hand` to `admitted`, because §5.3 step 4's
> predicate widened in the same edit — which was the one of the three that a harness would not
> have caught, since `readmit` is empty on every trace that does not contain a returning seat.
>
> **And this pass is the one in which that third composition came due, in the exact shape the
> rule predicts.** `PROTOCOL.md`'s `P2` **removed** a check — `A` no longer enlarges a required
> emitter set — and the consumer it unlatched was not in that document at all: §5.3 step 8's
> regime read, which the previous pass had widened to `admitted` *because* the required set was
> then `P(m) ∪ A`. With the wire's half landed and the engine's not, a peer holding
> `P == {self}` and one stale agreeing checkpoint-8 copy read `|admitted| == 2`, wrote no
> `solitary_since`, and **T62 and T63 were dead for the whole episode** — K-1's payoff returned
> through the engine after the wire had closed it. Both reads are deleted here. The lesson is the
> ordering restated once more: **the consumers to re-derive after a removal are the ones the
> *previous* pass added because the removed check existed**, and they can live in the other
> owner's document.
>
> **A detection must not be undone by the mechanism that ends the hand it detected in.** Every
> terminus in this table is a timer, and every timer leads through `HandAborted` and `HandComplete`
> back to a table that deals again. A freeze whose only purpose is to stop a peer from playing on
> is therefore, by default, a pause of one `hand_deadline_ms` — and a peer that resumes into the same regime meets
> the same contradiction and freezes again, forever, while every row above passes. That is J2's
> fixed point wearing a detection's clothes, and **§9.3 condition 0.6 and I33(c) are what discharge
> it.** The obligation generalises past K-9: any future rule that stops a peer must name what stops
> it *staying* stopped, or it buys one hand and gives it back.
>
> **N1 is the first defect this obligation caught, and it caught it by being read one clause too
> narrowly.** The clause said *the mechanism that ends the hand* — the timer — and the two paths
> that were still open were neither of them a timer: **T50**, a second detection that set no latch,
> and **T53**, the freeze's own *repair* path, which a solitary peer satisfied against itself. So
> the obligation is restated in the form the next fix should be checked against: **name every path
> back to playing — the timer, every other detection of the same fork, and the repair the fix
> installed for itself — and check that each of them needs something the frozen peer cannot
> supply alone.** A repair a peer can perform on itself is not a repair; it is the loop with a
> better name.

**The three intervals, and that they now have no gap between them.** T4 covers the setup chain, from
`JOIN_ACCEPT` to hand 1; T57 covers a live hand, from `HAND_INIT(k)`'s stage to `TERMINAL(k)`; T61
covers the boundary, from `TERMINAL(k)` to `GENESIS(k+1)`'s first stage. The third interval existed
before this pass and needed nothing, because nothing waited in it. Giving `HandComplete` width is
what created the obligation, and T61 is what discharges it — which is D-012's third shape stated in
advance for once rather than discovered by the next gate: **the defect is on the path the previous
fix newly made load-bearing**, and the path this fix makes load-bearing is the hand boundary.

#### 12.1.2 How long a table with one silent seat takes to make progress

**Before D-013: never.** Derived above and in §12: one `hand_deadline_ms` per hand, unbounded, no stack
changes, no end condition fires, no participant has an action that ends it.

**The bound below was conditional on one fix this document does not own (`P2`); it is not any
more, and saying what changed costs three lines.** `PROTOCOL.md` §4.9's readmission set `A` is
cleared at every hand init while §4.0 step 10a is skipped for stale-hand events, so a **replayed**
— not forged — agreeing checkpoint-8 `STATE_HASH` re-enters its sender into `A` once per hand. When
`A` enlarged a **required** emitter set that made the next stage 0 stall for `hand_deadline_ms`,
once per hand, for the `MAX_RETAINED_HAND_RECORDS` hands the retained record admits — on such a trace every number below
was wrong and the table made no progress at all, which is D-013's own fixed point restored by the
fix that kept D-013's promise. **`A` no longer enlarges a required set on either side**: §4.9
widens the *accepted* emitter set of one stage and §5.3 steps 4 and 8 read `signed_this_hand`
alone (`P2`, `P2-e`). An accepted copy completes no stage and blocks none, so the replay costs a
map lookup and no wait, a producer for `Readmitted` may be wired (§4.1), and the numbers below are
unconditional. A readmission's own cost is **one hand of latency** for the returning seat, which is
a schedule and not a stall.

**Under D-013, the bound is one or two hand deadlines, and then normal speed.** The two cases are
distinguished by *when* the seat stopped, and the difference is `HAND_INIT`'s own copy counting as
participation:

* **Silent from a hand boundary** — the client is already gone when hand `k` begins. It signed
  nothing in hand `k`; hand `k` stalls at `HAND_INIT` and aborts at `hand_deadline_ms`. Hand `k+1`
  skips it. **One stalled hand: one `hand_deadline_ms`, which heads-up is at least
  `HAND_DEADLINE_MIN(2)` and is whatever the founder advertised above it (`PROTOCOL.md` §8.2).**
* **Silent mid-hand** — it signed `HAND_INIT(k)`, then stopped. Hand `k` stalls at whatever stage
  it reached and aborts. Hand `k+1` still requires it, because it *did* sign a chained event during
  hand `k`; hand `k+1` stalls at `HAND_INIT` and aborts. Hand `k+2` skips it. **Two stalled hands:
  two `hand_deadline_ms`, so at least twice `HAND_DEADLINE_MIN(2)` heads-up.** D-013's own summary says "exactly one hand"; that is the first case, and the
  second costs one more. It is recorded rather than rounded away, and it is worth one directed test
  (§10, I31).
* **Silent after the hand completed** — it signed hand `k`'s `HAND_COMPLETE` copy and then stopped,
  which is the case checkpoint 8's gate newly exposes. Hand `k` does **not** stall; the boundary
  does, because `P(k)` contains the seat. T61 fires at `hand_deadline_ms` and runs hand init for
  `k+1`, so the boundary that follows is ungated (I32(c)) and hand `k+2` skips the seat.
  **One stalled boundary: one `hand_deadline_ms`, at least `HAND_DEADLINE_MIN(2)` heads-up** — the same one `hand_deadline_ms` this case cost before the
  gate existed, when it was spent on hand `k+1` stalling at `HAND_INIT` instead. The gate moved
  where the wait happens; it did not add one.

**The bound is therefore unchanged by K-3, and that is a claim to check rather than accept.** The
reason is structural and is worth stating in one line: **a gated boundary can only follow a hand
that completed, and a hand that completed cannot also have stalled.** In the mid-hand case the hand
stalls and the boundary that follows it is reached through T46 and is ungated; in the
after-completion case the boundary stalls and the hand did not. No trace pays for both, so the
worst case over all three cases is **two `hand_deadline_ms`, exactly as D-013 published it**.

**Then the table plays.** Every hand from that point completes among the seats that are there, at
whatever speed they play. The skipped seat drains: heads-up under `RATED_SNG_POKERTH_V1` it posts
one blind per hand, alternating SB and BB, so it loses `3 × small_blind(h)` every two hands, with
`small_blind` doubling every `blind_raise_every_hands` hands (§9.2). **Worked against
`PROTOCOL.md` §13's starting stack and blind schedule — and holding only against those, since every
figure in this paragraph is derived from them rather than being a parameter of its own** — that is
825 chips over level 1, 1 650 over level 2, 3 300 over level 3 (5 775 after 33 hands) and the
remaining 4 225 inside level 4 at 600 a hand. **It busts at about hand 40**, and §9.3 condition 1
then closes the table with the remaining player as the winner. Each of those hands is a
`|dealt_in| == 1` drain hand: no key setup, no shuffle, no reveal, straight to `Settling` (§5.3
step 9), so the wall-clock cost is one `hand_delay_ms` plus one round trip. **About five minutes at
§13's value.**

**Each of those forty hands now places checkpoint 8, and the gate on it costs nothing there
(K-3).** A drain hand ends at T45, so its boundary is gated; the set it is gated on is
`P(k) = signed_this_hand`, and on a drain hand the only seat that signed anything is the one that is
there. The stage completes at the lone seat in the same step, T47 fires, and the wall-clock estimate
above is unchanged. What the forty checkpoints buy is the thing the regime had none of: forty
comparable values, so a second peer running its own drain sequence with a different state collides
at the first hand on which the two differ, instead of both playing to a private "tournament won"
(`DECISIONS.md` K-1). And the fortieth boundary is where §9.3 condition 1 fires, so the tournament
result is compared before `TableClosed` rather than after it.

So the whole cost of an opponent vanishing at a heads-up MVP table is **one or two `hand_deadline_ms`
of stall — tens of minutes at the heads-up admitted minimum under §13's timings, and proportionately
more at any table whose founder advertised above it — then about five minutes of drain, then the
tournament ends.** The
drain figure is `hand_delay_ms` times forty and is untouched by `G4-P3`; only the stall scales with
the deadline. At larger tables the other seats play
real poker throughout and only the vanished seat's chips move to them.

**K-9 does not change that number, and the reason is the one §3.2 insists on.** The freeze fires
only on an event that arrives, and an opponent that has genuinely vanished sends none: T62 never
runs, the forty drain hands run exactly as derived above, and the tournament ends in about five
minutes. **Where the opponent has *not* vanished — K-1's trace, one dropped `HAND_INIT` copy and
both peers still transmitting — the number changes completely and is meant to:** the first
contradicting event freezes this peer, one `hand_deadline_ms` passes, and §9.3 condition 0.6 closes
the table. **One stalled hand and a closed table instead of five minutes and a false winner**, and
`SPEC_CS.md` §19's ranking is the authority for preferring it. The two cases are separated by wire
evidence and by nothing else, which is the whole design of the rule: a peer that is alone hears
nothing and finishes, a peer that is wrong hears the contradiction and stops.

**D-014's cost is one voided hand, and it does not touch this bound either.** A removed seat is a
seat that has stopped participating, which is the case §12.1.2 already measures; the difference is
that it stops at the removal instead of one or two hands later, so a removal strictly *shortens*
the interval this section bounds. Its stack drains on the same schedule and busts at the same
place, because it posts the same blinds — that is I34(c) and it is why the chips are blinded off
rather than taken.

**What is still not claimed.** This is a bound on one seat going silent and staying silent. It is
not a bound against an adversary that keeps *sending*: a peer that stalls to the deadline, returns
to sign one event, and stalls again costs a hand each time and never stops being a required
emitter, and two colluding seats can do the same at any table size (Q3). D-013 removes the fixed
point, not the ability to waste hands. `SPEC_CS.md` §19 and D-009 rule 2 both accept that trade
knowingly, and §8.7 gives what is left in its place, which is visibility.

**Where the previous revisions failed this check, recorded so the fix can be verified rather than
believed.** The Phase 2 gate found phases 4–14 and 20 with **no reachable exit** — the whole of the
regime in which chips are committed — because T57's artefact was unacceptable at every receiver
under G1. The revision before that left phase 20 with no exit at all (P5). The one before that
could not complete the `HAND_ABORT` stage, because its required emitter set contained the silent
seat (P3). Three phases, three revisions, one class of defect: **a terminus that depends on the
participation of the peer whose non-participation is the reason for it.** Every row above is
checked against exactly that question, which is why the *Fires after* column names a timer or the
word *immediately*, and never a peer.

**And the fourth revision failed a check this table does not perform (J2).** Every row passed and
the table never played again, because a *hand's* terminus and a *table's* progress are different
obligations and only the first was ever written down. The class is one step out from the previous
three: **a next hand that depends on the participation of the peer whose non-participation is the
reason the last one ended.** §12.1.1 checks the rows against the first question and §12.1.2 against
the second, and they are kept apart on purpose — collapsing them is what I30(c)'s deleted clause
did, and it cost a pass.

**And the fifth revision passed both and was still blind in a third direction (K-3).** Every phase
had an exit and the sequence of hands made progress, and forty consecutive hands went by with
**nothing emitted that any peer could compare**. Neither question asks that: a hand that ends
having produced no comparable value ends, and the table that plays forty of them progresses. So a
third obligation is written down here beside the other two, and **I32** is its assertion:

> **Every hand must place a checkpoint.** Termination asks whether a phase can be left, progress
> asks whether the sequence of hands advances, and this asks whether anything was *said* that two
> peers could disagree about out loud. A regime that satisfies the first two and fails the third is
> not a slow table or a stuck one — it is two tables that no longer know they are one.

The three are kept separate for the same reason the first two are: each is discharged by a
different artefact — the *Fires after* column, the strictly decreasing stack, and now the
checkpoint — and a document that collapses any pair of them loses the trace where the surviving
question is silent.

## 13. Objections to the fix plan

**None to `PHASE0_FIXPLAN.md`.** Every instruction it addresses to this document was applied as
written, with one exception now overruled from above: §0.3's crypto-deadline liveness carve-out,
which this document carried into §8.4 rule 6, is deleted by **D-009 rule 2** (item 4 below).
`DECISIONS.md` outranks the fix plan, and the previous revision had an evidence document overriding
a decision document. **One objection is recorded at the end of this section, addressed to
`PROTOCOL.md` §4.3**; the three the previous revision carried there are **closed** — §3.2 and §4.10
carry the witness-independent terminal stage, §4.9 and §6.3 carry the reconciliation round, and
§8.2 starts the hand deadline at `TERMINAL(k−1)`. Each was ruled on and this document follows the
ruling rather than its own earlier wording, which is D-011 rule 1 working as intended.

Two of the fix plan's instructions were **incomplete rather than wrong**, and are recorded here so
the next reviewer can check the completion rather than discover it:

1. **C-6 mandates the transition `Diverged | EquivocationProof -> HandAborted`, and it is now
   deleted rather than completed.** The previous revision added `Event::EquivocationProof` to carry
   it (T55) and then widened the row to every live phase (T56). `PROTOCOL.md` §5.2.4 has since
   ruled that **nothing consumes an `EquivocationProof`**, in a box that names this document, so
   the transition C-6 mandated no longer exists at any layer: the two rows, the event variant and
   `AbortKind::Equivocation` are all deleted (G2, §5.2). The fix plan's instruction is superseded
   by a decision above it, not left unapplied — D-010 removed the consequence and D-011 rule 1
   gives the ruling to the wire's owner.
2. **C-6 adds `Event::StateAck` and specifies no transition consuming it**, which is the exact
   defect the same ruling raises against `Event::RevealRejected`. T51 was added so a `StateAck`
   arriving at a checkpoint that agrees is an accepted no-op rather than a `Rejection`; without it
   the event would be unreachable and I13 would forbid it. T49 was added for the same reason on the
   agreeing branch of `StateHash`, since C-6 specifies only the conflicting branch.

The transition count of §5.1 includes both completions. Removing them would take it from 59 to 57
and leave two declared events that no row consumes.

---

**Applied in the Phase 1 verification pass, with what each change rests on.**

1. **D-008 rescoping (verification finding N3).** Every rule and guard that gated the timeout
   certificate on the seat count now reads `|V(subject)|`. The changed guards are **T8**,
   **T12** (each gains the `|V| ≥ 2` floor, with `V` taken over the seated set because
   `deck.participants` does not exist yet), **T16**, **T22**, **T27**, **T41**, **T44** and
   **T34**, whose `|deck.participants| ≥ 3` is replaced by `|V(subject)| ≥ 2`. (The second pass
   then removed the below-floor branches those five rows carried; see item 4.)
   §8.4 gains rule 7 and the `certified_subjects` field (§2.6, §2.8, §5.3),
   without which "minus any seat already named" would be local belief rather than state. §8.5,
   §8.6, §8.7, §9.5, §10 (I2, I13), §11 (Q3, OQ-A) and §12 are rescoped to match, and **I29** is
   added because the rule is exactly the kind a `proptest` over legal play never exercises.
2. **The equivocation proof outside a divergence (C-6 PARTIAL, verification finding N5) —
   superseded.** That finding asked for T55 to be widened and T56 added, because `PROTOCOL.md`
   then specified a `cause = 5` abort with no divergence precondition. `cause = 5` is deleted from
   the wire and nothing consumes a proof, so the widening has nothing left to widen and both rows
   are gone (G2). The **retention** requirement the finding rested on is unchanged and is met at
   the layer that owns it: `PROTOCOL.md` §5.2.4 keeps every verifying proof in every phase, at
   every table, whether or not a hand is live. I13's scope-valued-row clause stays, because T4,
   T48, T49–T51 and T57–T59 still need it.
3. **A-8, the `max_circuit_bytes` correction: nothing to apply here.** This document carries no
   byte budget, no relay figure and no per-hand traffic estimate — it is checked, not assumed:
   `max_circuit_bytes`, `131072`, `128 KiB`, `8 979` and the phrase "per direction" do not occur in
   it. The corrected figure — a **bidirectional** total for the whole circuit, so `2 × 8 979 B` per
   hand and roughly half the hands previously claimed — belongs to `NETWORK_STACK.md`,
   `CRYPTOGRAPHY.md`, `PROTOCOL.md` and `THREAT_MODEL.md`. This paragraph exists so that a later
   reader does not have to re-derive the absence.

---

**Applied in the second Phase 1 verification pass (`PHASE1_VERIFY2.md`), under D-009.**

4. **D-009 rule 2 / M1 — a below-floor certificate is inert everywhere.** The disagreement between
   this document and `PROTOCOL.md` §8.3 about what a `kind = Crypto` certificate does below the
   floor is settled in `PROTOCOL.md`'s favour, which is the engine's accept predicate for an event
   a modified client can emit at will. **Every place that gave one a terminating effect is
   changed, and this is the complete list:** §5.2's second floor bullet; §8.4 **rule 6**; the
   below-floor branches of **T16**, **T22**, **T27**, **T41** and **T44**, each of which now
   carries `|V(subject)| ≥ 2` in its guard instead; invariant **I29(b)**; and, following them,
   §8.4's "how this document reads D-008 point 2" paragraph, §8.4's unanimity table, §8.6's second
   restoration bullet and its I2 sentence, §8.7's `|V| < 2` bullet, §9.5, invariant **I2**, §11's
   Q3 and OQ-A rows and §12's `|V| < 2` bullet. What a stalled hand does instead is **T57**, with
   `Event::HandDeadlineAbort` (§4.1) as the carrier — the gap the previous revision recorded here
   as open, now closed rather than carried.
5. **N9(b) / T4 — the join deadline.** `PROTOCOL.md` §8.4 has ruled that no join-deadline
   certificate kind exists and none is to be invented, so `DeadlineKind::Join` is deleted (§2.6),
   T4 is triggered by `Event::FormationAbandoned`, T2 arms a `Crypto` deadline rather than a `Join`
   one, and T4's scope is widened to the whole setup chain with the reason stated (§5.2). The
   objection this section carried against T4 is therefore closed, not carried forward.
6. **N2 — the two "adjudicable offline" sentences.** Both are withdrawn, in §5.2's T54 note and in
   §12, in the same form `PROTOCOL.md` §6.4 used, and §12 gains an explicit statement that this
   document does not supply the reference engine the withdrawn claim appealed to.
7. **M2 / D-009 rule 1 — the equivocation predicate.** No engine change was needed and none was
   made. §5.2 and §8.6 **name the dependency** rather than asserting the conclusion, and since G2
   the dependency is narrower still: no transition here reads a proof, so what rests on
   `PROTOCOL.md` §5.2.3's property is the honesty of a permanent public record rather than any
   engine outcome. The key itself is one tuple in `PROTOCOL.md` §5.2.1 and this document
   reproduces no part of it (D-011 rule 2).
8. **M4 — `PLAYER_LEAVE` and its siblings.** They are hand-boundary stages and only that, so **T17**
   and **T35** become rejection rows and **T58**/**T59** are added for the boundary phases, where
   this document previously had no row at all. §9.4's ledger paragraph and invariants **I2** and
   **I28** follow.
9. **`STATE_HASH`'s scope (§3.3).** The claim that the hash covers `TableState` "in full" is
   withdrawn in favour of `PROTOCOL.md` §6.1's canonical `PublicTableState` list, and §2.8's
   `certified_subjects` justification is weakened to match.
10. **Five smaller things an implementer would otherwise have had to guess**, found by reading the
    document as one, and closed in the same pass. **(a)** T26's trigger `DealComplete` was not
    declared in §4.1; it is, with the reason it is the one derived event that is never published.
    **(b)** T53 and T54 were triggered by a phrase — "transcript reconciliation completes" — rather
    than by an event; they now consume the `StateHash` that `PROTOCOL.md` §6.3 step 3 already has
    peers re-emit — **superseded by item 14 below, which found that emission to be a second body in
    the checkpoint's own slot (P1)**. **(c)** `Event::Show` has no accepting row, which is now stated as deliberate
    rather than left to be searched for. **(d)** `SeatStatus::Absent` had lost its only producer
    when T35 became a rejection; it was set at T46 from `abort.attributed` — **superseded by H1:
    that marking is deleted under D-012 and the variant now has no producing transition at all,
    which is a recorded consequence rather than the defect this item fixed (§2.4, §5.2, §8.6, I30);
    `PROTOCOL.md` §4.10's "no receiver may derive a seat's state from it" is what T46 was violating
    while this item read it as authority for the marking.** **(e)** §9.3 gained end condition 0, the empty table, which
    its own closing sentence assumed and its numbered list did not contain.
11. **The open list in §11 was pruned and given defaults.** **Q4** — where `STATE_HASH` is
    exchanged — is **closed**: `PROTOCOL.md` §6.2 enumerates seven checkpoints "at these points and
    no others", which is neither of the two candidate policies this document was still weighing, so
    the row moved to the closed table rather than sitting open against a settled rule. Every
    question that remains open now carries a **named default the engine may build against** — Q1
    and Q-01 (refuse a `showdown_policy` this build does not implement), Q3 (T57), Q5 (not
    supported), and the two chip questions this document then labelled `OQ-A` and `OQ-D`
    (restoration) — so no open question leaves the engine's behaviour undefined, only interim.
    **Both chip questions have since been closed by D-010** and moved to the closed table (item 12
    below); the two letters now name different, corpus-wide questions and §11 records the change.

---

**Applied in the third Phase 1 verification pass (`PHASE1_VERIFY3.md`), under D-010.**

12. **D-010 — the abort is neutral, and this is the complete list of what was deleted.** §8.6 is
    rewritten to one unconditional rule, `stack[s] += committed_hand[s]` for every seat. Deleted
    with it: the **forfeiture formula** (`culprits`, `forfeit`, `others`, `base`, the pro-rata
    `share`, the clockwise remainder distribution, and all three of its branches); the
    **`culprits == {}` special case**, which no longer needs to exist; the **split of `AbortKind`
    into kinds that run the formula and kinds that do not**; the **forfeiture precondition
    paragraph** and its normative box about certificates below the floor; and the **DoS cost
    paragraph** about an opponent knocking a player offline after a big bet, whose incentive
    existed only because forfeiture did. Every transition that referenced forfeiture is restated:
    **T46** (restoration on every branch), **T54**, **T57**, and §5.2's `|V| >= 2` floor
    bullet. (T55 was restated in the same pass and is deleted outright in this one — G2.) **I27** is widened from unattributed aborts to *every* abort, **I2** is restated
    because it is now trivially true of the abort path, and **I29(b)**'s stake is corrected from
    chips to a false record. §8.4's justifications for rules 4, 6 and 7 are restated on the false
    attribution rather than on the chips, and the rules themselves are **kept** — they still gate
    T34, which moves the action, and a false record against an honest key is still worth refusing.
    §8.7's three-way split of "can the quitter escape" is deleted and replaced by one sentence:
    the escape is open everywhere, which is D-010's stated cost.
13. **P3 — T57 can now fire.** The `HAND_ABORT` stage was respecified in §4.1 as a
    **witness-independent terminal stage** with no emitter set at all, so it closes on the first
    verifying copy from any peer and no peer waits for the seat whose silence caused it. D-010 is
    what makes it possible — with restoration the body's `final_stacks` is `start_stack_this_hand`,
    agreed since `GENESIS(k)`. `owed` is `[]` on that path, because it is exactly the set peers
    disagree about. The named default this document carried — the `HAND_INIT` set minus the seats
    the stalled stage still owes — is **deleted**, not refined: it forked the chain. *(The
    `stage_hash` formula this item proposed for the new shape was not adopted and is deleted in the
    D-011 pass: `PROTOCOL.md` §3.1 takes the terminal value from `GENESIS(k)` instead, so the stage
    needs no hash at all — item 22.)*
14. **P1 / G5 — the reconciliation `STATE_HASH` is a separate stage, and `round` is derived rather
    than carried.** `PROTOCOL.md` §4.9 owns both facts: the reconciliation round is its own
    collective stage at its own `sequence`, and the payload has three fields and no `round`.
    `Event::StateHash` therefore carries `sequence`, and §4.1 defines `round := sequence − s_ckpt`
    for the guards of T49–T54. The previous revision declared `round: u16` as if it were a wire
    field, which would have been a fifth place where this document and `PROTOCOL.md` describe one
    payload in two ways (D-011 rule 1).
15. **P5 — `Diverged` has a bounded exit.** The hand deadline is **not** disarmed on entry (T50),
    **T57's scope gains `Diverged`** when a hand is live, and **T4's gains it** when none has
    started. The sentence excluding T57 from `Diverged` is deleted. Under D-010 T54's and T57's
    *chip* dispositions are identical, so the timer decides nothing about chips that the phase had
    not decided, and the `Fault{StateDivergence}` T50 recorded is retained. The clause that said
    the table also stays faulted is **corrected in this pass**: `PROTOCOL.md` §6.4 reserves the
    table fault for `cause = 4`, and T57 emits `cause = 1` (§5.2).
16. **P4 — T8 and T12 are deleted.** `PROTOCOL.md` §8.4 forbids a vote or a certificate in the
    setup chain and the seat-order beacon is in it. The count goes 59 → **57**; the numbers 8 and 12
    are retired and not reused. `Fault{NoRngCommit}` and `Fault{NoRngReveal}` become unreachable and
    are removed. A stalled beacon now closes the table by T4, with nobody named and no chips in
    existence — a behavioural change, named in §5.2 rather than discovered later.
17. **P7 — `eligible` filters on `dealt_in`.** `build_pots` takes `dealt_in` as a third argument
    (§7.5); a level with two or more contributors and an empty eligible set is refunded to its
    contributors rather than awarded, which is the same idea as the uncalled-excess rule and keeps
    `eligible ≠ ∅` true of every constructed pot. **I7** gains
    `eligible ∩ ¬dealt_in == ∅` and **I8** gains `dealt_in[s]`. §12's claim that an absent seat
    cannot win the blind it posts is now implemented; before this it was stated and not implemented.
18. **P8 — mid-session seat entry is deferred, not described.** §5.3 step 0's seat-entry clause is
    deleted, §9.4 stops citing a `PLAYER_SEAT` message that does not exist in `PROTOCOL.md` §4.10 or
    §4.11, and **Q6** carries the question with "not supported" as its named default. `ledger_in`
    moves once, at the first hand init.
19. **R-1 — the hand deadline starts one stage earlier.** It runs from `TERMINAL(k−1)` rather than
    from `HAND_INIT` (§8.2). That closes the interval containing `HAND_INIT` itself, which §9.4
    concedes can fail to complete; the phase across it is `AwaitingKeySetup`, which is in T57's
    scope, so no new transition is needed. **The first-hand anchor is corrected in this pass**: it
    is the completing copy of the `TABLE_READY` stage — `TERMINAL(0)` — and not checkpoint 1's
    `STATE_ACK`, because `PROTOCOL.md` §8.2 has now ruled and one rule with no special case for
    `k = 1` is what it ruled.

---

---

**Applied in the Phase 2 gate pass (`PHASE2_GATE.md`), under D-011.**

20. **G2 — T55 and T56 are deleted and nothing here consumes an `EquivocationProof`.**
    `PROTOCOL.md` §5.2.4's box is canonical and names this document: *"Nothing consumes it … It
    ends no hand, moves no chip, unseats nobody, block-lists nobody … and produces no
    `AbortRecord`."* Deleted with the two rows: `Event::EquivocationProof` (§4.1),
    `AbortKind::Equivocation` (§8.6), the `AbortRecord{kind: Equivocation}` those rows produced —
    which had no `cause` value on the wire, since `5` is deleted — and **every sentence in this
    document asserting that a proof blocks a key at the transport layer**, in §5.2 and §8.6, which
    was false when written and is forbidden by D-010 point 3 and D-011 rule 3. The numbers 55 and
    56 are retired. The count goes 57 → 55, and T60 takes it to **56**.
21. **G4 — T11 no longer unseats.** It read *"commitment mismatch → `Seating`, subject unseated"*,
    three rows above §8.7's statement that this document has no peer-removal transition and eight
    lines above P4's own reasoning that an unseating is something *"D-010 point 3 forbids the
    protocol from performing anyway"*. The mismatched `RngReveal` is now a `Rejection` with a
    `Fault{RngCommitmentMismatch}` record, the beacon stalls, and **T4** closes the table with
    nobody named — the disposition P4 already chose for the two rows deleted beside it. The three
    beacon faults now have one disposition between them rather than two. **This document now
    contains no transition that removes a peer**, which is checkable in one search rather than
    argued.
22. **G1 / P3 — the terminal stage, conformed to `PROTOCOL.md` rather than specified here.** §4.1's
    `stage_hash` formula for the witness-independent stage is **deleted**: `PROTOCOL.md` §3.2 adopted
    the shape and §3.1 takes the terminal value from `ABORT_TERMINAL(k) = f(GENESIS(k))` instead, so
    the formula is not needed. **T57's guard loses the chain-head check**: §4.10 checks the abort's
    position loosely on purpose, and an engine guard that re-imposed strictness would restore the
    deadlock the loosening removed. `stalled_sequence` and `parent_hash` are recorded in the
    `AbortRecord` and gate nothing. The collision half of G1 is closed by `event_type` entering
    `PROTOCOL.md` §5.2.1's key, which this document points at and does not reproduce (D-011 rule 2).
    **Every `HAND_ABORT` now takes the witness-independent shape**, not only the unattributed ones,
    which is wider than this document previously said and is §3.2's ruling.
23. **G5 — `StateHash::round` is derived, not read.** `PROTOCOL.md` §4.9's payload has three fields
    and no `round`, so `Event::StateHash` carries `sequence` and §4.1 defines
    `round := sequence − s_ckpt`. Nothing is asked of the wire. The two stale objections G5(b)
    named are closed rather than left standing beside live ones.
24. **`PROTOCOL.md` §6.3 case (b) has a transition — T60 — and §6.4 has an end condition.** §6.3
    now gives its two unresolved terminuses different bodies and different consequences, and this
    document produced case (c)'s outcome for both. T60 classifies on `transcript_head`, which
    §4.9's payload already carries, so no new signal is needed and both peers classify from the
    same completed stage. §6.4's *"it is closed, no further hand is dealt"* had no representation
    at all here: `TableState::table_faulted` (§2.6) and §9.3 **condition 0.5** are it, set by T54
    alone.
25. **R-1's first-hand anchor and the label scheme.** The hand deadline's first-hand anchor is
    `TABLE_READY`, per `PROTOCOL.md` §8.2 (item 19). And every `OQ-*` cross-reference in this
    document is rewritten against the letters `DECISIONS.md` owns and `PROTOCOL.md` §12 has adopted
    (R-2): the reference-engine question is **OQ-A**, the settle-from-checkpoint question is
    `PROTOCOL.md` **Q-07**, and the `ACTION_SEEN` question is **Q-08**. §11 records the two labels
    this document formerly used for different questions rather than silently reusing them.
26. **`Settling` gains an exit, and the settlement is applied when its stage completes, not when
    one peer derives it.** Found by the §12.1 walk, not by the gate. `HAND_COMPLETE` is a
    **collective** stage (`PROTOCOL.md` §3.2) and `TERMINAL(k)` is its `stage_hash` (§3.1), so a
    seat that goes silent between the last reveal and that stage leaves it incomplete. Under the
    previous rows T45 fired on the local derivation and **awarded the pots**, then T47 started hand
    `k+1` — whose `hand_deadline_ms` never starts, because §8.2 anchors it on a `TERMINAL(k)` that
    was never fixed. That is phase 4 with no exit, reached from a completed showdown, and it is the
    same class as G1 and P5. Worse, `PROTOCOL.md` §4.10's precedence rule lets a peer that does not
    hold a complete `HAND_COMPLETE` accept an abort for that hand, which would restore stacks the
    engine had already paid out. The fix is one row and one scope: the settlement is computed and
    published on entry to `Settling` (T30, T42), **applied** at T45 when the stage completes, and
    **T57's scope gains `Settling`** for the case where it never does — at which point no award has
    been applied and restoration is coherent. `Settling` stops being "a pure computation left in the
    same `step`" and becomes what it always was on the wire: a wait for a collective stage.
27. **`AbortSettle` is not published, and never had a message.** §4.1 listed it among the derived
    events "wrapped by the protocol layer and emitted as collective stages"; `PROTOCOL.md` §4.11 has
    39 types and none of them is an abort-settlement. The wire event already happened — it is the
    terminal `HAND_ABORT` that took the engine into `HandAborted` — and T46 only applies it. It is
    now internal, like `DealComplete`, with no `Effect::Publish`.
28. **§12.1 — the termination walk is written down.** Every phase, its exit under total silence,
    the timer or derivation that fires it, and where it lands. It is a table rather than a claim
    because three consecutive revisions each asserted the property and each had a different phase
    with no exit (P3, P5, G1).

---

**Applied in the second Phase 2 gate pass (`PHASE2_GATE2.md`), under D-012.**

29. **T46 stops setting `status := Absent`, and the invariant that would have caught it is added
    (H1, blocking).** `attributed` is a field of the `HAND_ABORT` copy a receiver accepted. Since
    P3 the terminal stage is witness-independent — first verifying copy closes it — and neither
    §3.2 nor §4.10 gives a precedence rule between two `HAND_ABORT` copies, so two honest peers can
    accept copies whose `attributed` differs: `[subject]` on the certificate paths (T16, T22, T27,
    T41, T44), `[]` on T57, T60 and T54. Stacks and `TERMINAL(k)` still agreed, which is why the
    §12.1 walk did not see it; what forked was the *next* hand, through `dealt_in` (§5.3 step 4) and
    `bb_seat` (`PROTOCOL.md` §4.4's non-absent constraint), so each peer rejected the other's
    `HAND_INIT` copy, the collective stage never completed, and the table never played again.
    **The line is deleted**, not repaired: D-012 forbids canonical state being derived from a
    quantity honest receivers can differ on, `PROTOCOL.md` §4.10 already said *"no receiver may
    derive a seat's state from it"*, and under D-010 attribution is evidence with no automatic
    consequence — so a status derived from it was never right. **I30** is added; §2.4, §5.2's
    T58/T59 note, §8.6, §8.7, §11's Q7 (widened from "when the abort names nobody" to "at all"),
    §12 and §12.1 item 5 follow. The cost is Q7's and is not new: a silent seat is dealt in again
    every hand and stalls each one, which was already the shipped behaviour heads-up (§9.5); every
    table size now behaves as heads-up already did. `SeatStatus::Absent` keeps its variant and its
    definition and has **no producing transition** in this version, which is stated in §2.4 rather
    than tidied away.
    **The cost sentence in this item is wrong and is corrected by item 32 below (J2).** It is left
    standing as the record of what was believed at the time: the stall was not repetition, it was a
    fixed point, because no status ever removed a seat from `HAND_INIT`'s required emitter set. The
    *edit* this item describes was right and is untouched; only its cost model was wrong.
30. **Six restatements replaced by pointers — H8's two, and four the gate did not find.** §7.9's
    `commitment_i` and `seed` go to `PROTOCOL.md` §4.4 and §2.8 — the previous revision printed them
    *and printed the argument against printing them*, that a copy is what made them diverge (C-2),
    which is the argument for a pointer and not for a corrected copy. §3.2's ordering-buffer key goes
    to `PROTOCOL.md` §2.3 and §5.2, with the one-line separation the gate asked for: it is **not**
    §5.2.1's anti-replay slot key, and the two tuples sitting one table row apart is how a reader
    re-derives D-009 rule 1 from the wrong one. The two the gate did not find are in this document's
    own tables: **T9's guard** printed `H(value‖salt) == commitment` and **T10's side effect**
    printed `seed = BLAKE3(v₁‖…‖vₙ)` — a *third* incompatible form of the beacon, without
    `derive_key` and without the length prefix, sitting in the transition table while §7.9 printed
    the corrected one twenty sections away. **§7.8's deal map** goes to `PROTOCOL.md` §4.5, which
    closes with *"This section owns the map; no other document may restate it independently"* — a
    sentence the previous revision cited while restating the map beneath it. And **§8.2's
    `TimeoutAssertion`**, named over the tuple `(table_id, hand_id, subject, kind, sequence,
    parent_hash)`: the wire message is `TIMEOUT_VOTE` and its six fields are `subject_sequence`,
    `subject_seat`, `subject_event_type`, `parent_event_hash`, `deadline_ms` and `kind`
    (`PROTOCOL.md` §4.8), so both the name and the field list had already drifted — a copy nobody
    could check, because nothing else in the corpus used either. What this document keeps of each
    site is the part that is a transition or an engine type: when the deal map is computed (T14) and
    what it is a function of; that the engine consumes a completed certificate as the `Event` of
    §4.1 and never a vote; that the beacon's opening is checked before it is recorded (T9) and the
    seed derived once every opening is in (T10).
31. **Every count reconciled, and the retired numbers listed (the gate's one bookkeeping error).**
    §5.1 said 56 transitions and §9.5 said 57; §5.1 was right and §9.5 was stale by one edit, which
    is exactly the failure §5.1's own "stated once here and referenced elsewhere" rule exists to
    prevent. Both now read **56 live transitions**, and §5.1 carries a **retired-numbers table** —
    T8, T12, T55, T56, with what each was, what deleted it and why — so that the gap between the
    highest number T60 and the live count 56 is explained rather than looking like an omission.
    §10's invariant count goes to **30** with I30, its "four of these deserve dedicated adversarial
    tests" is corrected to **seven** (it listed six before I30 was added), and the I7/I8 test
    construction is rewritten to build its non-participating seat through T59, since `Absent` is no
    longer reachable.
    **The numbers in this item are the fifth pass's and have since moved three times** — first to 57
    live transitions with T61 and 32 invariants with I32, then to 62 with T62–T66 and **34
    invariants** with I33 and I34, and to **63 live transitions** with T67 (`N-5e`, reshaped by `P2-e`) — and
    **since D-015 to 57 live transitions and 33 invariants**, by the deletion of T16, T22, T27, T34, T41, T44
    and the retirement of I29 (§5.1, §10, §12.1.1), the
    invariant count unchanged because `P1`'s assertion is a fourth part of I32 rather than a new
    row. All are stated in §5.1 and §10.
    The item is left standing as the record of the reconciliation, not as a current count; §5.1's
    "stated once here" rule means this paragraph was never the place to read one.

---

---

**Applied in the third Phase 2 gate pass (`PHASE2_GATE3.md`), under D-013.**

32. **`dealt_in` is gated on demonstrated participation, and the table can make progress again
    (J2, blocking).** `PROTOCOL.md` §4.4's required emitter set for `HAND_INIT` was *"every seat
    that will be `dealt_in`, plus every occupied seat that is absent or sitting out"*, so **no seat
    status ever removed a seat from it** — which means the previous pass's cost model was wrong in
    kind, not in degree: a silent seat did not make the table slow, it made the table **stop**, for
    good, with no participant able to end it. D-013 replaces the status question with a
    participation one, and §5.3 step 4 applies its engine half:
    `dealt_in[s] := (status == Active ∧ s ∈ signed_this_hand)`. Narrowing the emitter set alone
    would not have worked — a non-emitting seat that is still `dealt_in` is still in
    `deck.participants` and the stall moves from `HAND_INIT` to `AwaitingKeySetup` — which is why
    this half is this document's and is not a restatement of the other. Added: `signed_this_hand`
    (§2.6, §2.8), its accumulation rule and its clearing at §5.3 step 8, and **I31**. No transition
    is added, deleted or re-guarded.
33. **I30(c)'s final clause is deleted as false (J3).** It read *"hence a `HAND_INIT` collective
    stage that completes"*. Agreement on the body makes the stage **completable**; completion needs
    every required emitter to emit. The clause is also what hid J2 for two passes, and it carried a
    **test instruction**, so a harness asserting it would pass on every trace where the stage
    completes and never be run against the trace where it does not — unfalsifiable exactly where
    the defect lived. The invariant now ends at byte-identical `dealt_in` and `bb_seat`, states that
    completion is a liveness property outside its scope, and **the test instruction is changed to
    byte-identity of the two peers' derived `HAND_INIT` bodies** (§10, and the same correction in
    the adversarial-test list).
34. **Every statement of the liveness cost is rewritten rather than annotated** — §5.2's note under
    T58/T59, §8.6, §8.7, §12's two bullets, §12.1's notes 4 and 5 and its closing paragraph. The
    sentence *"slow is the worst case"* is withdrawn; §12.1 gains **§12.1.1**, the twenty phases
    re-derived under D-013 with the exit for each, and **§12.1.2**, the bound on how long a table
    with one silent seat takes to make progress. The bound is **one `hand_deadline_ms` if the seat
    vanished at a hand boundary and two if it vanished mid-hand** — D-013's "exactly one hand" is
    the first case; the second costs one more, because `HAND_INIT`'s own copy counts as
    participation — then normal speed, then the seat busts in about forty hands heads-up under the
    preset and §9.3 condition 1 closes the table.
35. **Q7 is closed by dissolution, not answered, and Q8 replaces it.** *What marks a seat `Absent`?*
    was the wrong question for two passes: marking a seat `Absent` never removed it from the emitter
    set, so the answer would not have helped. Nothing marks it, nothing needs to, and
    `SeatStatus::Absent` stays **unreachable on purpose** with the trap labelled at its declaration
    (§2.4, J5) rather than retired — retiring it would remove `PublicTableState.absent` from
    `state_hash`, which is a wire change and `PROTOCOL.md`'s call. **Q8** carries the one input to
    the participation predicate that is not agreed by construction: a contribution to the stage that
    *stalled*, which no `stage_hash` ratifies. It is filed against `PROTOCOL.md` and recorded in
    `DECISIONS.md`'s open list in this pass, per D-013's process rule.
36. **The `Absent`-deviation row this document owed the register is withdrawn (§12).** It was filed
    on the belief that D-005's blinding-off had been lost with T46's marking. It had not: D-005 asks
    that the game continue and the absent seat's stack be eaten by the blinds, and D-013 delivers
    exactly that. Which field carries the gate is an internal fact, not a deviation from a rule of
    poker, and filing it as one would put a non-deviation in a register whose whole value is that
    everything in it is real.

**Applied in the seventh pass, against `DECISIONS.md`'s open list.**

37. **Checkpoint 8, the hand-boundary checkpoint (K-3, blocking).** `PROTOCOL.md` §6.2's
    checkpoints 2 to 7 all require a `DECK_COMMIT` or a betting round, and §5.3 step 9 sends
    `|dealt_in| == 1` straight to `Settling`, so under D-013's steady state — about forty
    consecutive drain hands (§12.1.2) — nothing comparable was emitted at all and the tournament
    result was never checkpointed. **The checkpoint is added at the one place a hand that deals no
    cards still reaches**: the boundary, emitted at T45 and T46 on entry to `HandComplete`, over
    the boundary public state and `signed_this_hand` and nothing else (§5.2's box). **T61** is
    added as its timer-borne exit, **I32** as its assertion, and §12.1 is re-derived — including
    the row T50 can now reach, `Diverged` with no live hand and `hand_id > 0`, which is the hole
    this fix would otherwise have opened and which T61's second scope closes.
38. **`HandComplete` gains width, on the settled path (K-3b).** The phase was zero-width and is one
    of only two in which `PLAYER_SIT_IN` is legal, so T58 and T59's `HandComplete` branches were
    unreachable rows and D-013's *"it rejoins by signing"* had no window. It waits for checkpoint 8
    on the T45 path and does not on the T46 path, and the asymmetry is the whole safety argument:
    the settled path's `P(k)` has just demonstrated itself, the aborted path's contains the seat
    whose silence ended the hand. A seat returning from silence needs no `PLAYER_SIT_IN` in any
    case — since H1 it never left `Active`, and its own checkpoint-8 `STATE_HASH` is the chained
    event that readmits it. **The claim that the width alone is sufficient is corrected by item 43
    (`P2-e`)**: the window closes at a `HAND_INIT` that a solitary peer self-completes, so at the
    peer D-013's steady state produces it is zero-width, and §4.9's readmission set is what carries
    the returning seat across it.
39. **T59's `Paused` exit stops asking whether a seat is *willing* (K-7).** The guard read
    `status == Active ∧ stack > 0` while §5.3 step 4 answered the same question on participation
    three lines later in the same transition, so three silent `Active` seats could carry the table
    out of `Paused` into a `|dealt_in| == 1` drain hand. **The fix is not a better word but one
    predicate**: the guard *is* step 4, referenced and not restated. `status` survives inside that
    predicate and that is not what D-013 removed — D-013 removed status as the **liveness gate**,
    because a silent seat cannot change its own status; it did not remove it as the record of a
    choice a seat made with its own signature, which is what `SittingOut` is (I30(a)).
40. **The wire half of K-3 is `PROTOCOL.md`'s and is already on the open list.** §6.2 is normative
    that its checkpoints are *"at these points and no others"*, so checkpoint `8` does not exist
    until that document adds the row, and `signed_this_hand` is not in `PublicTableState` (§6.1)
    until that document puts it there. Both are named in `DECISIONS.md`'s **K-3**, which is where
    D-013's process rule requires a defect assigned across an owner boundary to be recorded, and
    this document specifies only what it owns: which transition emits, which gates, what a mismatch
    does to a phase, and what the phase then owes termination.

---

**Applied in the tenth pass, against `P1`, `N-1e`, `N-5e` and `P7` (items 41–43), and in the
eleventh, against `Q1-e`, `Q4-e`, `P7` and `N1-a` (items 43–46 — 43 is rewritten by the eleventh, not
added by it).**

41. **The checkpoint store, three slots, with its bound stated (`P1`, blocking).** The field was
    `Option<CheckpointState>` and `PROTOCOL.md` §4.9 now makes hand `k`'s boundary checkpoint
    outlive hand `k`. One slot broke three rules at once and each had a different victim: **T51**
    could not set `agreed` for a boundary checkpoint, so **D-014's tier-2 precondition was
    unsatisfiable at every hand boundary** — the exact fault `N6` was filed to remove, restored by
    the document `N6` did not touch; **T50**'s `N1` conjunct was unreadable once the slot was
    overwritten, so the latch that closes a forked table was correct and unevaluable; and a
    **non-solitary** receiver's stale boundary mismatch had **no carrier at all**, which is
    reachable on a healthy table through the forwarding reorder §4.9's own fix relies on. §2.6
    carries `CheckpointStore` — `live`, `agreed`, `boundary` — the **bound of three values for the
    life of the table**, the lifetime of each slot, and the proof that three suffice; `hand_id`
    joins `CheckpointState` and both checkpoint events, off the envelope and not off the wire;
    `acked` joins `CheckpointState`, because *a completed `STATE_ACK` stage* was not a
    representable object; T45, T46, T47, T49, T50, T51, T61, T64 and T65 name the slot they read;
    I32 gains part **(d)**.
42. **The solitary floor gains one hand of slack (`N-1e`, blocking).** `solitary_at(k)` was
    `j <= k` and `PROTOCOL.md` §3.2's regime test is a **disjunction**; the write site sees only
    the first disjunct, so on the second the floor is written one hand late — and the second is the
    disjunct K-1's own walk lands on first. The predicate is now `j <= k + 1`, §2.6's lemma is
    proved over both disjuncts, the ordering it depends on (hand init before §9.3's conditions) is
    named, and the residual the boundary window leaves is stated rather than argued away. I33(a)
    carries the assertion and the directed case, which is the **second** disjunct: a harness that
    only enters the regime through the first passes against both forms of the predicate.
43. **The readmission set loses its union, and that is `P2-e` superseding `N-5e`.** `N-5e` gave
    §4.9's `A` an engine half by reading `admitted := signed_this_hand ∪ readmit` into `dealt_in`
    and into the solitary-regime test, because §4.4 then made `R(HAND_INIT, m+1) = P(m) ∪ A`.
    `PROTOCOL.md`'s `P2` deleted that union from the wire — `A` widens an **accepted** emitter set
    and never a required one — and this document still held it, which is the whole of `Q1-e`. Both
    reads are deleted: step 4 derives `dealt_in` from `signed_this_hand` alone, step 8 writes
    `solitary_since` on `|signed_this_hand| == 1`. **The measured consequence of holding it was
    two blockers at once**: a replayed stale `PLAYER_SIT_IN` or agreeing checkpoint-8 `STATE_HASH`
    — no key, once per hand, valid for `MAX_RETAINED_HAND_RECORDS` hands — made `n(8) dealt_in` differ from every peer
    and stalled stage 0 for a whole `hand_deadline_ms` per hand; and because `|admitted| = 2` at exactly
    the peer K-1 is about, `solitary_since` was never written and **T62 and T63 were dead for the
    whole episode**, restoring the solitary tournament win through the engine after the wire had
    closed it. `readmit` and **T67** are kept, in no guard, for one reason: T67's
    `status ∉ {Removed, Empty}` conjunct is the filter the wire cannot apply (I34(a)). §2.6, §2.8,
    §4.1, §5.2, I31(b), I33(a), §12.1.1 and §12.1.2 follow, and §12.1.2's bound stops being
    conditional.
44. **The checkpoint record gets the fields its guards read (`Q4-e`).** `CheckpointState` held
    `values: u8` and no `state_hash`, so T49's *"equals this peer's own derivation"* had no
    readable term on the `agreed` and `boundary` slots `P1` added for it, and T50's *"two distinct
    values now exist"* had none anywhere — a count of distinct values cannot be maintained without
    the values. `values` is deleted, `own: Hash` and `dissent: Option<Hash>` replace it, and the
    three guards are re-derived: `e.state_hash == c.own`, `e.state_hash != c.own` with
    `c.dissent := c.dissent.or(...)`, and `c.dissent.is_none()`. Two values suffice because this
    peer's own copy is always one of them (§3.4; I32(a) for checkpoint 8); a container fed from the
    network is what §27
    forbids. §2.6 carries the box and the bound.
45. **One list, and the engine transcribes `PublicTableState` from it (`P7`).** The struct is
    `#[cbor(array)]`, so field order **is** the encoding, and the corpus carries three listings of
    its contents. §3.3 now states that `PROTOCOL.md` §6.1's **table** is the one the engine builds
    from and that no other listing anywhere is a field order — not §2.9's sweep enumeration, and
    not this document's own §5.2 checkpoint-8 box, which names what the checkpoint is *about*.
    Which of `PROTOCOL.md`'s two lists survives is that document's under D-011 rule 1 and is now
    genuinely on `DECISIONS.md`'s open list as **`P7-w`** — the previous pass said it had filed it
    and had not, and §3.3 records that correction rather than making it silently. What is settled
    here is that the engine has exactly one source and adds no fourth listing.
46. **`N1-a` is chosen rather than left to the implementer (`N1-a`).** T53's exit needs a
    reconciliation-round `STATE_HASH` signed by a seat other than the frozen peer, and at a
    solitary peer that seat is outside `P`; whether §4.9's out-of-set admission extends from the
    checkpoint to its reconciliation rounds decides whether the exit **exists**. The choice is
    **admit**, recorded in §5.2's solitary box with its reason — the round re-derives the same body
    over the same stage, so §4.9's own reason does not weaken at `r >= 1`, and the admission alone
    releases nothing because T53 still requires completeness, unanimity and two distinct signers.
    The one sentence that states it is `PROTOCOL.md` §4.9's and the choice is recorded against that
    row on `DECISIONS.md`'s open list.

---

**Five objections addressed to `PROTOCOL.md`, and two of them are new with this pass and are what
the engine's own fixes now depend on.**

* **§4.0 step 10b — the stale boundary mismatch needs a carrier at a receiver that was not alone
  (`P1`).** Step 10b says such an event *"enters §6.3 at step 1"* and also that it *"never reaches
  step 13"*; the second sentence makes the first unimplementable, because `SolitaryDivergence` is
  guarded on the regime and is the only thing step 12a produces. **Named default this document
  builds on:** the event is delivered to the engine as `Event::StateHash` with the `hand_id` it
  names, and **T50** consumes it against §2.6's retained boundary record. Nothing else about step
  10b changes — the event is still not applied, enters no `stage_hash` and grows no `P`, because
  T50's whole side effect is a freeze and a `Fault` record. Without it this document has a row no
  event can reach, on the path §4.9's `N6` fix newly made ordinary, which is `L4`'s shape.
* **§4.0 step 10b — the readmission set must still be delivered, and the reason has changed
  (`P2-e`).** It used to be that §4.4 read `A` into `R(HAND_INIT, m+1)` and the engine had to read
  the same required set; since `P2` neither does, and the engine reads `A` into no set at all. The
  delivery is still owed, for a smaller and sharper reason: **the wire has no `status`**, so
  T67's `status ∉ {Removed, Empty}` conjunct is the only filter in the corpus that keeps §4.9's `A`
  from being the re-entry route D-014's one-way exit forbids (I34(a)). **Named default:** one
  `Readmitted { seat }` per admission, consumed by **T67**. The replay bound that was owed with it
  has landed: `A` enlarges no required emitter set on either side, so a producer may now be wired
  (§4.1, §12.1.2).

The three that stood here before are closed: §3.2
and §4.10 carry the witness-independent terminal stage (item 22), §6.3 step 3 and §4.9 carry the
reconciliation round (item 23), and §8.2 starts the hand deadline at `TERMINAL(k−1)` (item 19). Two
remain from the previous pass and one is new with H8, and the engine builds against a named default
for each:

* **§4.10 — `cause` has no value for an invalid key proof.** T15 aborts on a `DECK_INIT` whose
  key-share proof does not verify, and §4.10's four causes are "failure to publish", "invalid
  shuffle proof", "invalid reveal proof" and "divergence". The seat published; what it published is
  invalid; no label fits. This document's named default is `cause = 2` with the offending event in
  `n(3) evidence`, on §4.10's own reasoning that `2` and `3` are the causes a single chained event
  proves on its own (§8.6). The smallest fix is to widen `2`'s label from *invalid shuffle proof*
  to *invalid proof in the deck-setup chain*, which changes no field and no code point; adding a
  fifth value would be a wire change and is larger. It is the same class as the `cause = 5` defect
  G2 closed — an engine outcome with no message that carries it — caught this time from the engine
  side before it shipped.

* **§4.3 — the formation timer stops one stage too early.** §4.3 abandons formation when
  `TABLE_READY` has not completed within `join_deadline_ms` of the first `JOIN_ACCEPT`. §3.1 puts
  the **seat-order beacon** in the same setup chain, *after* `TABLE_READY`, and §8.4 rests its
  coverage claim — *"every stall from `JOIN_REQUEST` to the last hand is covered, with no gap"* —
  on §4.3 while describing the setup chain as "`JOIN_REQUEST` through `TABLE_READY`". The interval
  from `TABLE_READY` to the beacon's completion is therefore outside §4.3's literal wording, and
  since P4 deleted T8 and T12 the beacon has no other terminus of its own. **Named default this
  document builds on, stated in §5.2 under T4:** formation is abandoned if the beacon has not
  completed within `join_deadline_ms` of `TABLE_READY`, exactly as for a `TABLE_READY` that never
  completes, and for the same three reasons — no `HAND_INIT`, no buy-in in the ledger, no card. It
  is safe either way, because hand 1's own `hand_deadline_ms` also spans that interval (§8.2) and
  is five times longer, so nothing can freeze there; what is missing is the sentence that says so.
  Extending §4.3's timer to the whole `hand_id = 0` chain is the smallest form of it.

* **§5.2 — the ordering buffer is attributed to this document, and it is not this document's**
  (new with H8). `PROTOCOL.md` §5.2 reads *"`STATE_MACHINE.md` §3.2 removes network arrival order
  with an ordering buffer keyed on `(table_id, hand_id, sequence, previous_event_hash)`"*, and §3.2
  of this document used to print the same tuple with no pointer at all — so the corpus had the key
  written twice and owned nowhere, which is the shape D-011 rule 1 exists to remove. Under that
  rule it is the wire owner's: the buffer runs in `protocol`, over four envelope fields §2.3
  defines, entirely before `step` is called, and no transition in §5.2 of this document can observe
  it. **Named default this document builds on:** §3.2 now points at `PROTOCOL.md` §2.3 and §5.2 and
  reproduces no part of the tuple, and the engine's contract is unchanged either way — it consumes
  one total order and has no opinion on how the order was produced. The smallest fix on the other
  side is for §5.2 to state the key as its own rather than cite this document for it. The pointer
  is currently circular, which is why it is recorded here rather than left to be noticed.
