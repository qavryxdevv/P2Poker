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
| `DECISIONS.md` **D-010** | **an abort is neutral.** (1) Stacks are restored to their start-of-hand values; no chips move on an abort, in any direction, for any cause. (2) Attribution is recorded as evidence and has **no automatic consequence**. (3) **No automated eviction** — no seat is block-listed, unseated or penalised by the protocol on the strength of a proof. (4) Certificates and equivocation proofs are still *produced*, because they are how a human or a later version adjudicates, but consuming one never moves a chip or removes a player in this version. Revises D-005's chip rule on abort; the accepted cost is that the rage-quit escape returns and must be stated plainly (§8.6, §8.7, §12) |
| `DECISIONS.md` **D-011** | **one normative owner per concept, and the slot key written down once.** **Rule 1:** `PROTOCOL.md` owns the wire — message shapes, the envelope, the chain, sequence numbers, the anti-replay slot, canonical bytes, receiver validation; **this document owns state and transitions** — phases, guards, legal actions, pots, invariants — and *references* the other's definitions by section number rather than restating them. Where the two disagreed, the owner won. **Rule 2:** the slot key is one literal tuple in `PROTOCOL.md` §5.2.1, `event_type` included, and no other document reproduces any part of it. **Rule 3:** D-010 point 3 binds every layer — no `block_peer`, no unseating, no allow/block list driven by a proof, anywhere. What it changed here: T55, T56 and T11's unseating deleted, §4.1's `stage_hash` formula and §5.2's restatement of the reconciliation round deleted and replaced by pointers, and every "blocked at the protocol layer" sentence removed (§13 items 20–26) |
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

Every field comes from the signed table advertisement (`SPEC_CS.md` §4), so all participants
agreed on it before any cryptographic material existed. It never changes during the table's life.

```rust
pub struct TableConfig {
    pub protocol_version:        u16,
    pub table_id:                TableId,
    pub preset_id:               PresetId,        // RatedSngPokerthV1 | Custom
    pub game:                    Game,            // Nlhe
    pub mode:                    Mode,            // TournamentSngPlayMoney | CashPlayMoney
    pub seats:                   u8,              // 2..=10
    pub min_players_to_start:    u8,              // <= seats
    pub start_stack:             Chips,
    pub first_small_blind:       Chips,
    pub ante:                    Chips,           // 0 in RATED_SNG_POKERTH_V1
    pub blind_raise_mode:        BlindRaiseMode,  // DoubleEveryNHands
    pub blind_raise_every_hands: u32,             // 11
    pub small_blind_cap:         Chips,           // seats * start_stack / 2
    pub button_rule:             ButtonRule,      // DeadButton
    pub odd_chip_rule:           OddChipRule,     // FirstSeatLeftOfButton
    pub showdown_policy:         ShowdownPolicy,  // see §7.7 — OPEN QUESTION
    pub action_timeout_ms:       u32,             // 20_000
    pub action_timeout_grace_ms: u32,             //  5_000
    pub hand_deadline_ms:        u32,             // 600_000
    pub join_deadline_ms:        u32,             // 120_000
    pub hand_delay_ms:           u32,             // 7_000 — DISPLAY ONLY, see §8.3
    pub auto_action_limit:       u8,              // consecutive auto-actions -> SittingOut (D-006 §4)
}
```

`hand_delay_ms` is deliberately **not** an engine input. It is a presentation delay so a human
can see the result; the protocol never waits for it. See §8.3.

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
    Busted,      // stack == 0, eliminated, position retained for the dead button
}
```

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
    pub certified_subjects:  SeatSet,   // seats named by a COMPLETED, VALID certificate
                                        // this hand; the only seats removable from V (D-008)

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
    pub duration_ms: u32,             // from config; the engine never adds it to anything
}

pub enum DeadlineKind { Action, Crypto }
```

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

`config.join_deadline_ms` stays in `TableConfig` because it is a signed parameter of the
advertisement that the **lobby layer** runs its formation timer from (`PROTOCOL.md` §4.3). No
`Deadline` value is ever built from it and `step` never reads it; it is carried so that every peer
agrees on the number the layer above is using, and so that §9.4's config validation can bound it.

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
| `Seat::consecutive_auto_actions` | D-006 §4: after `auto_action_limit` consecutive auto-actions the seat is marked sitting out. |
| `Seat::revealed_hole`, `Seat::mucked` | Showdown results are public and identical everywhere; a verifier re-computes the award from them. |
| `deadline` | D-006 requires the deadline to be **explicit state**, not a wall-clock read inside the engine. |
| `certified_subjects` | D-008: the required voter set `V` shrinks **only** by a completed, valid certificate, never by an assertion. Without this field the engine cannot compute the size of `V(subject)` deterministically, and "minus any seat already named" would have to be re-derived from unsigned local belief — which is exactly the assertion D-008 forbids. It is canonical state, entered only by §8.4 rule 7, so every peer derives it identically from the same accepted events. It is **not** a field of `PublicTableState` (`PROTOCOL.md` §6.1), so a disagreement about `V` is not visible at a checkpoint; what prevents one is rule 7 plus the fact that a certificate is a collective stage every peer validates for itself (§8.4), and what would expose one is that stage failing to complete. An earlier revision of this row said the field "must be inside `STATE_HASH`"; that claim is withdrawn rather than defended, because §6.1's list is canonical and does not carry it. Adding it there is `PROTOCOL.md`'s call and this document does not pre-empt it. |
| `sequence`, `previous_event_hash` | §12/§13/§14: monotonic sequence and parent hash are what make replay, reordering and equivocation detectable. |
| `level` | Tournament layer (§9); cached derivation of `hand_id`, asserted by I25. |
| `deck` | The crypto-waiting bookkeeping. Hashes only; see §2.5. |
| `rng_beacon` | §7 commit/reveal for seat order and initial button (`MENTAL_POKER.md` §6). |
| `aggressor` | TDA 17-A order of show (`POKER_RULES.md` A8). |
| `settlement` | Lets any peer re-check "the winner and the pot size were computed correctly" (§13) from the transcript alone. |
| `finish_order` | Tournament result; needs a total order with no floor person (A1.3). |
| `faults`, `abort` | §19 requires *evidence of which peer failed*, in the transcript, signed. |
| `table_faulted` | `PROTOCOL.md` §6.4 is normative that `cause = 4` **closes** the table — *"no further hand is dealt"* — and no other field expresses it: `abort` is overwritten by the next abort, and `faults` records a divergence a later reconciliation may have resolved. Set by **T54** only, never cleared, read by §9.3 condition 0.5. It is **not** a field of `PublicTableState` (`PROTOCOL.md` §6.1), and this document does not ask for one: §6.1's list is that document's and the field does not need to be in it, because T54 is the *last* transition of the last hand this table plays — the next T47 reaches `TableClosed` and no further checkpoint is ever emitted, so there is no `STATE_HASH` in which two peers could hold different values for it. Both derive it from the same completed reconciliation stage in any case. |
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
| Wall-clock time | scheduler task, `LocalView::armed_timer` | never directly — only as a signed `TimeoutCertificate` event (D-006) |
| OS randomness | `mental_poker` (`getrandom::SysRng`, `CRYPTO_LIBS.md` §1) and the RNG beacon | as committed then revealed 32-byte values, already agreed |
| Network arrival order | `protocol` ordering buffer keyed by `(table_id, hand_id, sequence, previous_event_hash)` | the engine sees one total order; out-of-order events are buffered or rejected before `step` |
| Duplicate / replayed messages | `protocol` replay filter (§14) | rejected before `step`; if one slips through, I21 makes it a no-op |
| Signature validity | `protocol/signatures.rs`, `ed25519-dalek` `verify_strict` | an event reaches `step` only after its signature and canonicality gate pass |
| Cryptographic verification | `mental_poker` | as `ShuffleVerified` / `ShuffleRejected` / `CardsOpened` verdicts |
| My hole cards, my key share | `LocalView` | never |
| Transport (direct vs relayed, RTT, `PeerId`) | `net`, `LocalView::transport` | never (D-001: a relay is a byte pipe and must not be visible to the rules) |
| Hand-strength evaluator internals | `poker/evaluator.rs` façade over `rs_poker =5.0.0` | only the *derived* result — winner set per pot and chip deltas. `POKER_RULES.md` A′4 constraint 1: a third-party rank score must never enter the hashed state. |
| Frame timing, animations, `hand_delay_ms` | GUI | never |

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
an explicit, exhaustive `#[cbor(array)]` projection, and `phase`, `deadline`, `certified_subjects`,
`history`, `faults`, `abort`, `settlement` and `finish_order` are **not** in it — of `DeckState`
only `deck_commitment` is. Two documents giving different answers about the bytes that are hashed
and compared is the M1 defect class in the one object the whole divergence path is built on, so the
weaker and canonical statement is the one that stands:

> `state_hash` is taken over `PublicTableState` exactly as `PROTOCOL.md` §6.1 defines it, at the
> checkpoints §6.2 enumerates and at no other point. It covers nothing from `LocalView`. Every
> field of `PublicTableState` is a pure function of `TableState`, so two honest peers that have
> processed the same event prefix produce the same hash; a mismatch triggers the §15 dispute path
> and the game does not continue silently.

The rest of `TableState` is still canonical — it is derived identically by every peer from the same
accepted events, and I22 asserts it — it is simply not *compared* at a checkpoint. The practical
consequence for an implementer is that a disagreement confined to an unhashed field is detected
through its effects (a collective stage a peer's copy fails to match, §3.4) rather than at the next
checkpoint. Whether any of those fields should be added to `PublicTableState` is `PROTOCOL.md`
§6.1's to decide, not this document's; see the note on `certified_subjects` in §2.8.

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
    TimeoutCertificate { subject: SeatIdx, kind: DeadlineKind,      // kind: Action | Crypto
                         sequence: u64, parent_hash: Hash, signers: SeatSet },

    // ---- time, as a completed collective stage (D-008 point 2, D-009 rule 2) ----
    HandDeadlineAbort  { hand_id: u64, stalled_sequence: u64, parent_hash: Hash },
    FormationAbandoned { },

    // ---- SPEC_CS.md §15 checkpoint and dispute path (PROTOCOL.md §6) ----
    StateHash  { seat: SeatIdx, checkpoint: u16, sequence: u64, state_hash: Hash },
    StateAck   { seat: SeatIdx, checkpoint: u16, agreed: Hash, checkpoint_hash: Hash },
    Dispute    { seat: SeatIdx, kind: DisputeKind, at_sequence: u64 },

    // ---- engine-derived (§3.4) ----
    DealComplete,        // internal only — never published; see below
    Settle,
    AbortSettle,
    NextHand,
}

pub enum DisputeKind { StateHashMismatch, MissingEvent, Equivocation }
```

`StateHash`, `StateAck` and `Dispute` are the engine's side of `PROTOCOL.md` §6.3's four-step
divergence procedure; without them the engine could not represent a terminal state the protocol
can reach, and the two would diverge by construction (C-6).

**`Event::EquivocationProof` is deleted from the alphabet, and that is G2's disposition.**
`PROTOCOL.md` §5.2.4 rules in a box that names this document: a verifying `EquivocationProof` is
retained as evidence in every phase, and *"**Nothing consumes it.** There is no transition, in any
document, that takes an `EquivocationProof` as its input event."* `PROTOCOL.md` owns the wire and
therefore owns what a message may cause; this document owns the transitions and follows the box
(D-011 rule 1). An engine variant no transition consumes is the C-6 defect in its own right — the
mirror of the `TimeoutCertificate{Join}` and `RevealRejected` cases this document has closed four
times — so the variant goes with the transitions that consumed it (T55, T56, both deleted; §5.2).

Two things follow and both are checkable rather than rhetorical. **The proof is still produced,
verified, broadcast in a `DISPUTE` and retained forever** — that is `PROTOCOL.md` §5.2.4's, at the
protocol layer, and the engine's not holding it changes nothing about it. And **an equivocation
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
> `STATE_HASH` stage — the `n(0) checkpoint`'s stage, which every peer holds because the phase was
> entered from it. `round == 0` is therefore the checkpoint emission of `PROTOCOL.md` §6.2;
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
in the transcript and not in `TableState`. `owed` stays populated for the certificate-borne aborts
(T27, T41, T44), where the certificate names the subject and the requirement and the set is agreed.

Two further properties, carried over unchanged. **The engine still contains no clock** (§3.1,
§8.2): `step` sees an accepted stage, exactly as it sees a completed certificate stage; the
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

**The counts, stated once here and referenced elsewhere: 20 phases, 56 numbered transitions, 29
invariants (§10).** They changed from the pre-fix-plan figures (19 / 47 / 26) by the additions of
C-6 (the `Diverged` phase and the divergence transitions), C-6's `RevealRejected` transition, A-7
(I27) and C-9 (I28). The transition count went from 55 to 56 in the first Phase 1 verification
pass, which added **T56** so that an `EquivocationProof` arriving when no hand is live was consumed
rather than rejected (C-6 PARTIAL, verification finding N5); the same pass widened T55 from
`Diverged` to every phase except `TableClosed` without changing the count, and took the invariants
from 28 to 29 with **I29**, the D-008 voter-set floor. Both of those rows are gone again in this
pass — see the fourth-pass bullet below.

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
    16 HandComplete,
    17 HandAborted,            //  SPEC_CS.md §19 / D-005; settles neutrally (D-010, §8.6)
    18 Paused,                 //  fewer than two seats willing to be dealt in
    19 TableClosed,            //  terminal, absorbing
    20 Diverged,               //  SPEC_CS.md §15 / PROTOCOL.md §6.3 — a freeze, not a terminal
}
```

`Diverged` is appended rather than inserted so no existing discriminant moves. It is entered from
**any** phase the moment two distinct `state_hash` values exist for one checkpoint, and it is a
**freeze**: no card opens, no action applies, no chips move (`PROTOCOL.md` §6.3 step 1). It is not
terminal — transcript reconciliation can return the table to the phase it held at the checkpoint.

**Nine** of the twenty — 2, 3, 4, 5, 6, 8, 10, 12 and 14 — are cryptographic waits, and one
(15) computes the result and then waits for the terminal stage that records it. They are
first-class states because the engine has to be able to make no
progress at all, indefinitely, while remaining a valid hashable state that every peer agrees on,
and because the *only* legal exits from them are a crypto verdict, a timeout certificate, the hand
deadline (T57), or a fold-out that makes the pending cryptography unnecessary. A boolean "waiting" flag could not
express which artefact is owed by whom, which is exactly what §19 requires as evidence.

### 5.2 The transition table

Notation. `m` = number of dealt-in seats (one symbol for this quantity across the whole corpus;
`PROTOCOL.md` §4.5 and `CRYPTOGRAPHY.md` §2.4 use the same letter — C-3). `live(s)` = dealt-in ∧
¬folded. `contenders(s)` =
live ∧ ¬all_in. `round_closed(s)` is the `POKER_RULES.md` A2 predicate: every contender has
`acted_this_round` **and** `committed_round == current_bet` (or is all-in for less). Effects are
abbreviated. A transition not listed does not exist; any event arriving in a state with no
matching row is a `Rejection` and leaves the state bit-identical (I21, I13).

`V(subject)` is the **required voter set** of §8.4, defined once there and used unexpanded in
every guard below:

```
V(subject) = deck.participants \ ({subject} ∪ certified_subjects)
```

**No guard in this table is scoped on the seat count (D-008).** Wherever a guard needs to know
whether the certificate machinery applies, it reads the size of `V(subject)`, never
`deck.participants` and never `m`. The two are not interchangeable: `V` is the set an attacker
must shrink to forge a certificate, and D-008 point 3 with the `certified_subjects` field is what
stops it being shrunk by assertion, whereas the seat count is large and stays large while the
forgery succeeds — which is exactly how the N3 attack passed every check. A guard in this document
that reads `m == 2` or `|deck.participants| == 2` is a defect, and the last one — T34's — was
removed in this pass.

**On the `TimeoutCertificate` rows (T16, T22, T27, T34, T41, T44).** A timeout vote and
the voter's own
contribution at the same stage are **not** mutually exclusive, and nothing in this table may be
read as saying they are. A vote carries `event_class = 1` and so occupies its own slot rather than
the stage slot it is about; a seat may hold both its own contribution at a collective stage `s`
and a vote about stage `s`, and neither implicates the other (§8.4, `PROTOCOL.md` §4.8,
`PHASE0_FIXPLAN.md` §0.1). Since M2 the slot is finer still — `PROTOCOL.md` §5.2's slot key carries
the vote's `subject_seat` — so one voter may also hold two votes about two subjects at one stage.
None of this is engine behaviour: the engine neither emits nor inspects votes, it consumes
completed certificates. It is stated here because the rows below would otherwise be read as
implying an exclusivity the protocol does not have.

**The `|V| ≥ 2` floor, stated once for every `TimeoutCertificate` row (D-008, D-009 rule 2).**
Every row below whose trigger is a `TimeoutCertificate` carries the §8.4 validity rules, rule 6
included, without restating them. In consequence:

* `|V(subject)| ≥ 2` — the certificate has an effect: `kind == Action` produces the auto-action
  (T34), and `kind == Crypto` ends the hand with an `AbortRecord` **naming the subject as
  evidence**. Under **D-010** naming moves no chips: the abort restores every stack to
  `start_stack_this_hand` exactly as an unattributed one does (§8.6), and the name is a transcript
  record with no automatic consequence beyond the D-005 absent-seat marking at T46, which is not a
  penalty (§8.6).
* `|V(subject)| < 2` — the certificate is **inert, of either kind**. It is rejected: not accepted,
  not chained, not evidence, no `Fault`, no `AbortRecord`, no entry in
  `certified_subjects`, and the state is bit-identical afterwards (I21). No row below fires. This
  is D-009 rule 2, which settles M1 by making `PROTOCOL.md` §8.3's reading binding on this document
  too; the earlier text, under which a `kind == Crypto` certificate below the floor still ended the
  hand, is deleted. A hand that consequently cannot proceed ends on the `hand_deadline_ms` path
  instead — **T57** — with `attributed = []` and stacks restored.
* On acceptance of a certificate with `|V(subject)| ≥ 2`, `certified_subjects |= {subject}`. That
  is the **only** way a seat leaves `V`, at any table size (D-008 point 3). A seat merely voted
  against stays in `V`, so a vote cannot shrink the set that has to agree with it.
* **There are no pre-hand certificate rows any more (P4).** T8 and T12 carried a
  `TimeoutCertificate{Crypto}` in the seat-order beacon, which is in the setup chain
  (`hand_id = 0`), and `PROTOCOL.md` §8.4 rules that a vote or a certificate is *never emitted*
  there. Both rows are deleted rather than defended; see the beacon table below for what a stalled
  beacon does instead.

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
| T9 | `AwaitingSeatRngReveal` | `RngReveal` | `H(value‖salt) == commitment` ∧ not yet revealed | `AwaitingSeatRngReveal` | record |
| T10 | `AwaitingSeatRngReveal` | `RngReveal` | all revealed | `AwaitingKeySetup` | `seed = BLAKE3(v₁‖…‖vₙ)`; derive seat permutation and initial `button_pos`; `hand_id := 1`; **hand init** (§5.3); `RequestKeySetup`; `ArmDeadline{Crypto}` |
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
and — in this pass — `Event::EquivocationProof` and `AbortKind::Equivocation`, §5.2 G2).

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
| T14 | `AwaitingKeySetup` | `KeyPublished` | all participants published | `AwaitingShuffle` | compute `agg_key_hash`; fix `deck.shuffle_order` and `deck.deal_map` (§7.8) **before** any shuffle; `RequestShuffle{first}`; `ArmDeadline{Crypto}` |
| T15 | `AwaitingKeySetup` | `KeyRejected` | — | `HandAborted` | `Fault{BadKeyProof}`; `AbortRecord` names the seat |
| T16 | `AwaitingKeySetup` | `TimeoutCertificate{Crypto}` | subject owes a key ∧ **`|V(subject)| ≥ 2`** | `HandAborted` | `Fault{NoKey}`; `AbortRecord{kind: NoKey, attributed: [subject]}`; `certified_subjects := certified_subjects ∪ {subject}` |
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
| T22 | `AwaitingShuffle` | `TimeoutCertificate{Crypto}` | subject is the expected shuffler ∧ **`|V(subject)| ≥ 2`** | `HandAborted` | `Fault{NoShuffle}`; `AbortRecord{kind: NoShuffle, attributed: [subject]}`; `certified_subjects := certified_subjects ∪ {subject}` |

#### The private deal

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T23 | `AwaitingDeal` | `RevealTokensPublished` | indices ⊆ hole indices of seats **other than** the publisher | `AwaitingDeal` | record in `deck.tokens` |
| T24 | `AwaitingDeal` | `RevealTokensPublished` | publisher included **its own** hole index | `AwaitingDeal` | `Rejection` + `Fault{SelfRevealTooEarly}` — publishing your own token pre-showdown would let everyone open your hand |
| T25 | `AwaitingDeal` | `RevealTokensPublished` | any index ∉ this hand's hole indices | `AwaitingDeal` | `Rejection` + `Fault{TokenForUnauthorisedIndex}` — this is the §10 "early board" attack |
| T26 | `AwaitingDeal` | derived `DealComplete` | ∀ participant `P`, ∀ hole index `i` of `P`: every participant `≠ P` has published a token for `i` | `BettingPreFlop` | `street := PreFlop`; open the betting round (§5.4); `player_to_act` per A2; `ArmDeadline{Action}` |
| T27 | `AwaitingDeal` | `TimeoutCertificate{Crypto}` | subject owes tokens ∧ **`|V(subject)| ≥ 2`** | `HandAborted` | `Fault{NoDealTokens}`; `AbortRecord{kind: NoDealTokens, attributed: [subject]}` listing exactly which `(seat, index)` pairs were owed; `certified_subjects := certified_subjects ∪ {subject}` |

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
| T29 | `S` | `Action{Fold}` | legal ∧ `|live| ≥ 3` after the fold, or `|live| == 2` and the round is not closed | `S` | `folded := true`; advance `player_to_act`; `ArmDeadline{Action}` |
| T30 | `S` | `Action{Fold}` | legal ∧ exactly one live seat remains | **`Settling`** | `DisarmDeadline`; **no reveal is requested at all** — see §8.5 and D-005 case 1; compute the settlement and `Publish` this peer's own `HAND_COMPLETE` body (§3.4) |
| T31 | `S` | `Action{Check\|Call\|Bet\|Raise}` | legal ∧ ¬`round_closed` after applying | `S` | move chips; update `current_bet`, `last_full_raise`, `aggressor`; `acted_this_round := true`; advance `player_to_act`; `ArmDeadline{Action}` |
| T32 | `S` | `Action{…}` | legal ∧ `round_closed` ∧ `S != BettingRiver` | `next_reveal(S)` | `DisarmDeadline`; `RequestOpen{street indices, Public}`; `ArmDeadline{Crypto}` |
| T33 | `S` | `Action{…}` | legal ∧ `round_closed` ∧ `S == BettingRiver` ∧ `|live| ≥ 2` | `AwaitingShowdownReveal` | compute `showdown_order` (§7.7); `RequestOpen{hole indices of each live seat, OwnerOf(seat)}`; `ArmDeadline{Crypto}` |
| T34 | `S` | `TimeoutCertificate{Action}` | `subject == player_to_act` ∧ **`|V(subject)| ≥ 2`** (§8.4 rule 6; D-007 as generalised by D-008 — this transition does not exist when the required voter set is one seat, whatever the seat count) ∧ signers == `V(subject)` ∧ `sequence`/`parent_hash` match `state.deadline` | as T29–T33 | apply `Check` if `to_call == 0`, else `Fold` (D-006 §1); `was_auto := true`; `consecutive_auto_actions += 1`; `certified_subjects := certified_subjects ∪ {subject}`; if it reaches `auto_action_limit`, mark the seat `SittingOut` **effective at the next hand boundary** |
| T35 | `S` | `PlayerLeft`/`PlayerSitsOut`/`PlayerSitsIn` | — | `S` | **`Rejection`** — all three are hand-boundary-only stages (`PROTOCOL.md` §4.10). A seat that leaves mid-hand announces nothing: it is still a key holder, nothing is unblocked by its departure, and the effect is felt at the next crypto wait as silence, not as an event |
| T36 | `S` | `Show`/`Muck` | — | `S` | `Rejection` — showdown declarations are only legal in `AwaitingShowdownReveal` |

T32 has one further guard worth stating separately because it is the all-in run-out:

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T37 | `next_reveal(S)` | `CardsOpened` | street cards appended ∧ `|contenders| ≥ 2` | the betting phase for that street | reset the round (§5.4); `player_to_act` per A2; `ArmDeadline{Action}` |
| T38 | `next_reveal(S)` | `CardsOpened` | street cards appended ∧ `|contenders| < 2` ∧ street < River | the **next** reveal phase, betting skipped | `RequestOpen{next street, Public}`; `ArmDeadline{Crypto}` — `POKER_RULES.md` A2: remaining streets are still dealt because they decide the pots |
| T39 | `next_reveal(S)` | `CardsOpened` | river opened ∧ `|contenders| < 2` ∧ `|live| ≥ 2` | `AwaitingShowdownReveal` | `RequestOpen{hole indices, OwnerOf(seat)}`; `ArmDeadline{Crypto}` |
| T40 | `next_reveal(S)` | `CardsOpened` | opened indices ≠ exactly the indices this street owes | `next_reveal(S)` | `Rejection` + `Fault{WrongRevealSet}` |
| T41 | `next_reveal(S)` | `TimeoutCertificate{Crypto}` | subject owes tokens for this street ∧ **`|V(subject)| ≥ 2`** | `HandAborted` | `Fault{NoBoardTokens}`; `AbortRecord{kind: NoBoardTokens, attributed: [subject]}`; `certified_subjects := certified_subjects ∪ {subject}` |

#### Showdown, settlement, hand end

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T42 | `AwaitingShowdownReveal` | `CardsOpened` | all required hands opened (§7.7) | `Settling` | fill `Seat::revealed_hole`; `DisarmDeadline`; compute the settlement and `Publish` this peer's own `HAND_COMPLETE` body (§3.4) |
| T43 | `AwaitingShowdownReveal` | `Muck` | `showdown_policy == TdaMuckWithForfeiture` ∧ seat is not the last unmucked | `AwaitingShowdownReveal` | `mucked := true` — an irrevocable forfeiture of every pot (§7.7) |
| T44 | `AwaitingShowdownReveal` | `TimeoutCertificate{Crypto}` | subject owes its own token ∧ **`|V(subject)| ≥ 2`** | `HandAborted` | `Fault{NoShowdownToken}`; `AbortRecord{kind: NoShowdownToken, attributed: [subject]}`; `certified_subjects := certified_subjects ∪ {subject}` |
| T45 | `Settling` | `Settle` | **the `HAND_COMPLETE` collective stage of this hand is complete** — every seat of the `HAND_INIT` set has been heard (`PROTOCOL.md` §3.2), which is what fixes `TERMINAL(k)` (§3.1) | `HandComplete` | apply the settlement computed on entry to `Settling`: `build_pots` (A7), evaluate, award, refund uncalled excess, split with odd chips (A8), mark busts, extend `finish_order`; `Settled(settlement)` |
| T46 | `HandAborted` | derived `AbortSettle` | — | `HandComplete` | **restoration, on every branch and for every `AbortKind` (D-010, §8.6)**: `∀ s: stack[s] += committed_hand[s]`, so every stack equals its `start_stack_this_hand` and no chip crosses between seats (I27, I2); `∀ s ∈ abort.attributed: status := Absent` (D-005 — a non-participation marking, not a penalty; see §8.6); `settlement.aborted := true`; `Fault` records already present |
| T47 | `HandComplete` | derived `NextHand` | see §9.3 end conditions | `AwaitingKeySetup` \| `Paused` \| `TableClosed` | rotate the dead button (A1.3), advance the blind level (§9.2), reset the hand, run **hand init** (§5.3) |

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
| T57 | any phase in which a hand is live — 4–14 (`AwaitingKeySetup` … `AwaitingShowdownReveal`, betting included), **`Settling` (15)**, and **`Diverged` (20) when a hand is live** | `HandDeadlineAbort` | `hand_id` matches the live hand ∧ `stalled_sequence` is a stage index of that hand. **That is the whole guard**: the abort's chain position is a record of where its emitter believed the hand stopped and is checked loosely at the receiver by `PROTOCOL.md` §4.10, so the engine must not re-impose a strict check (G1) | `HandAborted` | `DisarmDeadline`; `AbortRecord{kind: HandDeadline, attributed: [], owed: [], observed_by: ∅, sequence: stalled_sequence, parent_event_hash: parent_hash}` — the two are **recorded, not verified**; **no `Fault` against anybody** — from `Diverged` the `Fault{StateDivergence}` T50 already recorded is retained, and `table_faulted` keeps whatever value it holds (T57 never sets it); `certified_subjects` unchanged; restoration (§8.6, I27) |

**T57 is the answer to "what happens when a hand cannot proceed and no certificate can form",
and that is now a state the engine represents rather than a gap.** Under D-009 rule 2 a
certificate below the floor is inert, so four situations that previously ended at a certificate, or
did not end at all, now end here, and they are the whole of the list:

1. **`|V(subject)| < 2`.** Heads-up this is every stall, since `|V| = 1` in every heads-up hand; at
   a larger table it is reached once completed certificates have named enough seats (§8.4 rule 7).
2. **Two or more seats silent at one stage.** Neither voter set can reach unanimity, because each
   contains the other subject, and D-008 deletes the exclusion rule that used to paper over it —
   `PROTOCOL.md` §8.4's "two simultaneous subjects therefore deadlock, by design". This is Q3 /
   OQ-E / `PROTOCOL.md` Q-02, and T57 is its written interim behaviour, not its solution.
3. **A required voter that is present but will not vote.** Unanimity is never reduced (§8.4), so a
   single silent voter is enough to stop every certificate at that stage.
4. **A reconciliation that never completes (P5).** In `Diverged`, T53, T54 and T60 all need every
   required signer's reconciliation round; one that never arrives used to freeze the table with
   chips committed and no exit at all. T57 is now that exit — see the `Diverged` note below.

In all four the hand ends with nobody named and every seat receiving exactly its own
`committed_hand` back (I27). What it costs is the wait: `hand_deadline_ms` is 600 000 ms in
`RATED_SNG_POKERTH_V1` against `action_timeout_ms` = 20 000 and a crypto step of the same order, so
a stalled hand now takes ten minutes to end rather than seconds. `PROTOCOL.md` §8.3 records that
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
does open at `TABLE_READY` and therefore spans phases 2–3 (§8.2), but `join_deadline_ms` is
120 000 ms against its 600 000 ms, so T4 always reaches those phases first. **`Settling` *is* in
scope, and that is new in this revision**: it waits for the `HAND_COMPLETE` stage (T45), a seat can
go silent between the last reveal and its own copy of that stage, and hand `k`'s deadline is still
running there because `TERMINAL(k)` is exactly what has not been fixed. `HandAborted`,
`HandComplete`, `Paused` and `TableClosed` have no live hand. A stalled `HAND_INIT` **is** covered, and that is
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
* The race the exclusion feared is bounded by the numbers. `hand_deadline_ms` is 600 000 ms against
  a reconciliation that exchanges a handful of missing events over an already-open mesh; a genuine
  reconciliation finishes inside it by orders of magnitude. A reconciliation that does not is the
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
`committed_*`, `history`, `certified_subjects`, `deck` — is reset by hand init, `button_pos`
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
hand — at `hand_deadline_ms` per hand, which is the same liveness cost §12 already records for a
stall anywhere else, and no worse.

`Diverged` entered before any hand — from checkpoint 1, which `PROTOCOL.md` §6.2 places after
`TABLE_READY` — has no hand deadline running, and is covered by T4 instead (see the note under the
seating table). Between those two, every reachable `Diverged` has an exit.

#### Hand-boundary seat changes (`PROTOCOL.md` §4.10)

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T58 | `HandComplete` \| `Paused` | `PlayerLeft` | seat occupied ∧ `status != Leaving` | unchanged | `status := Leaving`. **No chip moves and neither ledger counter changes here**: the seat is removed and `ledger_out` incremented at the next **hand init** (§5.3 step 0), which T47 runs before §9.3's end conditions are evaluated, so a table every seat has left is empty by the time condition 0 looks at it |
| T59 | `HandComplete` \| `Paused` | `PlayerSitsOut` / `PlayerSitsIn` | seat occupied ∧ `status ∈ {Active, SittingOut, Absent}` — a seat may sit back in from `Absent`, which is D-006 §4's "sit back in at a hand boundary" | unchanged, **except** `Paused` + `PlayerSitsIn` when at least two seats are again willing and have chips → `AwaitingKeySetup` | `status := SittingOut` / `Active`, effective immediately because no hand is live; on the `Paused` exit, **hand init** (§5.3) |

**`Absent` has exactly one producer, and it is T46 (D-005).** The previous revision set it in T35,
on a mid-hand `PlayerLeft` — a message that does not exist (M4) — which left the real case, a seat
that simply stops answering, setting nothing. It is now set where the corpus says it is set:
`PROTOCOL.md` §4.10, *"The only seats affected are those attributed, which move to the D-005 absent
state."* So an abort's `attributed` seats become `Absent` when that abort settles, and from the
next hand they keep their stacks, pay blinds and antes, take no cards and drain until they bust
(§5.3 steps 4, 6, 7). **Marking a seat `Absent` is not a D-010 point 3 eviction**, and the
difference is checkable: the seat keeps its stack, its position and its `player`, no chip moves at
the marking, and it returns to `Active` at any hand boundary on its own `PlayerSitsIn` (T59). It is
the mechanism that makes D-005's "the game continues for everyone else" true, and D-010 leaves
D-005's absent-seat rule standing in terms.

It follows that a `HandDeadline` abort and a `StateDivergence` abort make
**nobody** absent, since their `attributed` is empty by construction — which is the same fact as
"they name nobody", seen in the seat table. **That is the whole of Q7 (§11):** T57 is the only abort
a heads-up stall can take, so heads-up nothing is ever marked `Absent`, the silent seat is dealt in
again next hand, and the table stalls again. An implementer must not close that by deriving `Absent`
from the abort's `owed` list — that list is empty on the T57 path precisely because it is the
quantity peers disagree about (§4.1, P3), and deriving canonical state from it forks the chain.
The other route to a drained seat is unchanged:
`auto_action_limit` consecutive auto-actions mark a seat `SittingOut` at the next hand boundary
(§8.5), and that route exists only where `|V| >= 2`.

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
| T49 | any phase except `Diverged`, `TableClosed` | `StateHash` | `round == 0` ∧ the value equals this peer's own derivation at that checkpoint | unchanged | record the signer in the checkpoint's agreement set |
| T50 | any phase except `Diverged`, `TableClosed` | `StateHash` | `round == 0` ∧ two distinct `state_hash` values now exist for one checkpoint | **`Diverged`** | freeze: no `RequestOpen`, no `ArmDeadline`, no chip movement; **the hand deadline is not disarmed** (P5, T57); retain both signed values as evidence; `Fault{StateDivergence}` |
| T51 | any phase except `Diverged`, `TableClosed` | `StateAck` | every required `StateHash` for this checkpoint is present and agrees | unchanged | the checkpoint passes; record `checkpoint_hash` |
| T52 | `Diverged` | `Dispute` | — | `Diverged` | record the declaration and its `at_sequence`; still frozen |
| T53 | `Diverged` | `StateHash` | `round >= 1` ∧ the reconciliation-round stage for the disputed checkpoint is complete over its required signer set ∧ every value in it agrees | the phase held at the checkpoint | resume; nothing was opened and no chips moved while frozen; the hand deadline keeps running |
| T54 | `Diverged` | `StateHash` | `round >= 1` ∧ the reconciliation-round stage is complete ∧ two distinct `state_hash` values remain in it ∧ **every `transcript_head` in it is equal** — `PROTOCOL.md` §6.3 case (c), byte-identical transcripts and still-different derived states | `HandAborted` | `Fault{StateDivergence}` (already present from T50); `AbortRecord{kind: StateDivergence, attributed: []}` (`cause = 4`); **`table_faulted := true`** — `PROTOCOL.md` §6.4 closes the table, and §9.3 condition 0.5 is where that is applied; **restoration** (§8.6, I27) |
| T60 | `Diverged` | `StateHash` | `round >= 1` ∧ the reconciliation-round stage is complete ∧ two distinct `state_hash` values remain in it ∧ **two distinct `transcript_head` values remain in it** — `PROTOCOL.md` §6.3 case (b), a peer is missing events it cannot obtain | `HandAborted` | `Fault{StateDivergence}` (already present from T50); `AbortRecord{kind: UnobtainableEvents, attributed: []}` (`cause = 1`, `cert_hash = None`); **`table_faulted` unchanged** — case (b) does **not** fault the table; **restoration** (§8.6, I27) |

**T53 and T54 are triggered by an event, not by a phrase.** Their Trigger column previously read
"transcript reconciliation completes, derived states now agree / still differ", which is a
condition and not an input: an engine is a function of `(state, event)`, and no event in §4.1 said
"reconciliation completed". The trigger is now the completion of the **reconciliation-round
`STATE_HASH` stage** that `PROTOCOL.md` §4.9 defines normatively. That stage's shape, its
`sequence`, its emitter set and why it is not a second copy of the checkpoint are §4.9's to state
and are **not restated here** (D-011 rule 1): the engine consumes what §4.9 produces. §4.1 gives
the one thing that is this document's — the derivation `round := sequence − s_ckpt` — and the rows
are scoped on it. T53 fires when the round-`r` stage completes and every value in it agrees; T54
and T60 when it completes and two distinct values remain. All three are chain content, so every
peer reaches the same verdict from the same events. T49 and T50 are scoped to exclude `Diverged`,
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

* `Paused` arms no deadline and moves no chips. It is left by T59 (`PlayerSitsIn`, when at least
  two seats are again willing to be dealt in and at least two have chips), which runs hand init and
  therefore also applies any seat marked `Leaving` by T58. **A `Paused` table that every seat has
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
4. `dealt_in[s] := (status == Active)` for every seat with `stack > 0`.
   `SittingOut`, `Absent` and `Busted` seats get `dealt_in = false`. (D-005, D-006 §4.)
5. Per seat: `start_stack_this_hand := stack`; `committed_round := 0`;
   `committed_hand := 0`; `acted_this_round := false`; `folded := false`; `all_in := false`;
   `revealed_hole := None`; `mucked := false`.
6. Post **antes** (`config.ante`, 0 in the preset): every seat with `stack > 0` that is
   `Active`, `SittingOut` or `Absent` posts `min(ante, stack)` into `committed_hand`.
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
   **`certified_subjects := ∅`** — the exclusion set is per hand, so `|V|` starts each hand at
   `|dealt_in| − 1` and can only be reduced again by a completed, valid certificate within that
   hand (D-008 point 3). Carrying it across a hand boundary would let exclusions accumulate over a
   session and reach `|V| < 2` without any single hand ever paying the floor.
9. Branch:
   * `|dealt_in| == 0` → `Paused` (no blinds are posted; step 6 and 7 are skipped);
   * `|dealt_in| == 1` → the single dealt-in seat wins every posted blind and ante with no cards
     and no cryptography: go straight to `Settling`. This is how an absent field drains
     correctly;
   * `|dealt_in| ≥ 2` → `AwaitingKeySetup`, `RequestKeySetup`, `ArmDeadline{Crypto}`.

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
  blind it posts; this line is where it is implemented, and before P7 nothing was.
* The single-contributor test is simultaneously the uncalled-bet refund rule, the
  "everyone folds to a bet" rule, and the "A bets 300, B raises all-in to 900, A folds" rule.
* **A level with two or more contributors and an empty `eligible` set is now legally reachable**,
  and it is refunded rather than awarded. It needs an absent or sitting-out seat committed above
  every dealt-in seat's commitment — e.g. a dealt-in seat all-in for an ante of 20 while the
  `Absent` small and big blinds have posted 50 and 100, so the level from 20 to 50 has
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
whose card. It is therefore computed in T14, from public state only, with no randomness:

Let `D = [d₀ … d_{m-1}]` be the dealt-in seats in clockwise order starting from
`succ(button_pos)` — normal deal order, small blind first — with `m = |dealt_in|`. Then

```
hole card 1 of dⱼ = index j
hole card 2 of dⱼ = index m + j
flop              = indices 2m, 2m+1, 2m+2
turn              = index   2m+3
river             = index   2m+4
```

with `m ≤ 10`, so at most 25 of 52 indices are ever used. `PROTOCOL.md` §4.5 owns this map;
`CRYPTOGRAPHY.md` §2.4 reproduces it and was corrected to match (C-3).

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
the initial button position. The engine does not compute either value — it sees only the verdicts
of T6–T11 — but the constructions are printed here because this document previously printed a
third, incompatible version of both and that is what made them diverge (C-2). The canonical forms,
owned by `PROTOCOL.md` §4.4 and its §2.8 domain-string register:

```
commitment_i = h("p2p-poker v1 rng-commit", [ table_id, session_id, committer_app_public_key, r_i, salt_i ])

seed = h("p2p-poker v1 rng-beacon", [ r_1, …, r_n ])          ascending by seat index
```

where `h` is `PROTOCOL.md` §2.8's constructor: `blake3::derive_key(domain, b"p2p-poker/v1")` as
the key, then for each part an 8-byte big-endian length prefix followed by the part. **The
separator is the length prefix; no other separator, delimiter or padding exists.** `session_id` is
the object `SPEC_CS.md` §14 and §20 call the *session nonce*; the name `session_nonce` is retired
and there is one name, `session_id`, defined by `PROTOCOL.md` §4.3.

The strings `p2p-poker v1 rng-seed` and `p2p-poker/seat-beacon/v1` — the latter this document's own
earlier construction, unprefixed and not domain-separated through `derive_key` — are **retired and
never valid**. They are listed as such under the register in `PROTOCOL.md` §2.8 so the mistake
cannot recur silently.

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

When a local timer fires, the scheduler **does not change state**. It signs a `TimeoutAssertion`
naming `(table_id, hand_id, subject, kind, sequence, parent_hash)` and gossips it. Only when a
peer holds assertions from *every* required signer — that is, from all of `V(subject)` as §8.4
defines it, with no seat dropped for looking unresponsive — does it assemble a
`TimeoutCertificate` and feed that to `step` as an ordinary event.

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
`join_deadline_ms` is 120 000 ms against `hand_deadline_ms`'s 600 000 ms, so T4 fires long before
hand 1's window closes and hand 1 always opens with at least 480 000 ms of its window left.

For hands `k ≥ 2` the window opens at `TERMINAL(k−1)` and the engine's phase across the interval
containing `HAND_INIT` is `AwaitingKeySetup` — both T10 and T47 run hand init and land there —
which is in T57's scope. So a `HAND_INIT` that never completes ends the hand exactly as any other
stall does, with no new transition and no new event. That closes the gap R-1 named: §9.4 concedes
that `HAND_INIT` can fail to complete when peers hold different accepted T58 events and derive
different `ledger_delta`, and under the old start point nothing covered it from hand 2 onward.

### 8.3 `hand_delay_sec` is not engine state

`RATED_SNG_POKERTH_V1` carries `hand_delay_sec = 7` from PokerTH's config default. It is a
**display** delay so a human can see the result. The protocol does not wait for it: T47 fires as
soon as the terminal event of hand `h` is in the chain, and peers may publish hand `h+1`'s key
setup immediately. A GUI still animating hand `h` simply lags behind a state that has already
advanced; that is a rendering concern, and making it an engine state would reintroduce a clock.

### 8.4 The timeout certificate (D-006, as corrected by D-007 and generalised by D-008)

```rust
pub struct TimeoutCertificate {
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
the carve-out would now buy is speed and deniability rather than chips; the ten-minute stall is
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
600 000 ms instead of the crypto step's 30 000 ms under `RATED_SNG_POKERTH_V1`. **The rage-quit
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

On a valid action-timeout certificate (T34) the engine applies **`Check` if `to_call == 0`, else
`Fold`**. Never fold a hand that could check for free (D-006 §1). The auto-action is a real entry
in `history` with `was_auto = true`, derived identically by every peer from the same state, so it
stays deterministic and verifiable (§11, §13). `consecutive_auto_actions` increments; on reaching
`config.auto_action_limit` the seat is marked `SittingOut`, **effective at the next hand
boundary**, and thereafter behaves as a D-005 absent seat: keeps its stack, pays blinds and antes,
takes no cards, drains until it busts. It may sit back in at a hand boundary.

**Whenever `|V(subject)| < 2`, none of this happens.** There is no timeout certificate with any
effect there, of either kind (§8.4 rule 6, D-007 as generalised by D-008 and applied literally by
D-009 rule 2), so T34 never fires, `consecutive_auto_actions` never increments from a timeout, and
the seat is never marked `SittingOut` by the deadline path. It can still sit out **voluntarily**
(`PlayerSitsOut`, at a hand boundary — T59), which is a genuine choice by that seat and needs no
certificate. The practical consequence is that an opponent who simply stalls cannot be punished
inside the protocol at all: the hand ends at `hand_deadline_ms` with nobody named (T57), and the
only remedy is to leave the table.

Heads-up is the common case of this and the one the MVP ships (§9.5), but it is **not** the only
one: `|V|` also reaches 1 at a larger table once enough seats have been named by completed
certificates within the same hand. Each such exclusion cost a certificate that itself cleared the
`|V| ≥ 2` floor (§8.4 rule 7), so nobody can arrange it unilaterally — but the state is reachable
honestly, at any table size, and this paragraph is scoped on `|V|` rather than on the seat count
for exactly that reason.

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
                                         // UnobtainableEvents | StateDivergence
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
6. **Every automatic consequence of attribution except one**, and the exception is named in the
   next block.

#### Attribution is evidence, and the one state change it still causes

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
  claiming more than visibility in play money; D-010 makes visibility the entirety of it.

**The one exception, and why it is not a penalty.** T46 sets `status := Absent` for every seat in
`abort.attributed`. That is D-005's absent-seat rule, which D-010 explicitly leaves standing, and
it is what makes "the game continues for everyone else" true: from the next hand the seat is not a
party to the cryptography, so no further hand waits for it. It is not eviction, and the difference
is checkable rather than rhetorical — the seat **keeps its stack**, **keeps its position** (the
dead button needs positions), **keeps its `player`**, is dealt no cards, pays its blinds as dead
money like any `SittingOut` seat, and returns to `Active` at any hand boundary on its own
`PlayerSitsIn` (T59). No chip moves at the marking, and nothing about it is irreversible.

**And it does not fire on the path the MVP actually runs.** T57's `attributed` is empty by
construction, so a heads-up stall marks nobody absent and the next hand deals the silent seat in
again. Every hand then stalls for `hand_deadline_ms`. That is not a defect introduced here — §12
already records that a stall "can be repeated every hand" — but an implementer must not close it
locally by deriving `Absent` from the abort's `owed` list. That list is exactly the quantity peers
disagree about (§4.1, P3); deriving canonical state from it forks the chain, which is a strictly
worse failure than a slow table. Closing it needs an agreed basis that does not exist yet, and it
is carried as **Q7** in §11.

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

* **On the `hand_deadline_ms` path (T57)** the quitter must stall for the full deadline — 600 000 ms
  under the preset — and the stall, the stage it stalled at and the abort are all in the transcript.
* **On a certificate path at `|V| >= 2`** the hand ends sooner and the quitter is *named* in
  `attributed`, and marked `Absent` for the next hand (§8.6). It is named, not charged.
* **On the divergence path (T54)** a single peer that publishes a `state_hash` it did not derive
  ends the hand at once and also faults the table, so the escape is fast but costs the attacker the
  table it was playing at. `THREAT_MODEL.md` X29 carries it.

Beyond the chips it is a griefing vector, and the only mitigations are social — a visible count of
attributable aborts per identity, and other players declining to sit — never cryptographic. In play
money that visible count is the whole penalty, and `SPEC_CS.md` §18 forbids claiming more.

---

## 9. The tournament layer

### 9.1 `RATED_SNG_POKERTH_V1`

From `POKER_RULES.md` Part B, derived from PokerTH's `GAME_TYPE_RANKING` constants (which its
`ServerGame::CheckSettings` *enforces*, making them the strongest available evidence of what the
rated preset means):

```
preset_id                = "RATED_SNG_POKERTH_V1"
game                     = NLHE
mode                     = TOURNAMENT_SNG_PLAY_MONEY
seats                    = 10
min_players_to_start     = 10          [OUR CHOICE] — PokerTH autostarts when full
start_stack              = 10000
first_small_blind        = 50
big_blind                = 2 * small_blind
ante                     = 0           PokerTH has no ante concept at all
blind_raise_mode         = DOUBLE_EVERY_N_HANDS
blind_raise_every_hands  = 11
small_blind_cap          = 50000       = seats * start_stack / 2
password                 = none
button_rule              = DEAD_BUTTON               [OUR CHOICE] TDA 32, not PokerTH
odd_chip_rule            = FIRST_SEAT_LEFT_OF_BUTTON TDA 20-A
action_timeout_sec       = 20
action_timeout_grace_sec = 5           [OUR CHOICE] PokerTH uses 2 on a trusted clock
hand_delay_sec           = 7           display only (§8.3)
hand_deadline_sec        = 600         [OUR CHOICE] no PokerTH analogue
join_deadline_sec        = 120         [OUR CHOICE] no PokerTH analogue
payouts                  = none (finishing place only)
```

`action_timeout_grace_sec` is a **consensus constant, not a UX detail**: every peer times out
independently with no shared clock, so all peers must use the identical value or they will
disagree about whether a deadline expired. It is part of the signed advertisement.

### 9.2 Blind level — derived, never incremented

```
level(h)        = 1 + (h - 1) / 11                       // integer division, h from 1
small_blind(h)  = min(50 * 2^(level(h) - 1), 50_000)
big_blind(h)    = 2 * small_blind(h)
```

Level 1 covers hands 1–11, level 2 hands 12–22, and so on; level 11 would be 51 200 and is clamped
to the cap. The cap uses the *starting* player count, so it is a constant for the whole tournament.

`level`, `small_blind` and `big_blind` are cached in the state for cheap access, but they are
**computed from `hand_id`** at every hand init and asserted against the closed form (invariant
I25). They are never incremented, because an incremented counter can drift between peers after a
rejected or replayed event and a derived value cannot.

### 9.3 End conditions

Evaluated at T47, **after hand init step 0 has applied the announced seat exits**
(§5.3), in this order:

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
1. **Tournament won** — exactly one seat has `stack > 0`. Append the remaining seats to
   `finish_order`, set `settlement.tournament_winner`, → `TableClosed`.
2. **Nobody willing** — `|{s : status == Active ∧ stack > 0}| == 0` → `Paused`.
3. **One willing, others with chips** — the drain case of §5.3 step 9 applies and the hand is
   played as an uncontested blind steal.
4. Otherwise → `AwaitingKeySetup` for hand `hand_id + 1`.

No timeout, disconnect or abort reaches condition 1 (D-006 §5). A tournament ends only when one
player holds every chip, or when every seat has left, which is condition 0.

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
2 <= seats <= 10
2 <= min_players_to_start <= seats
first_small_blind >= 1
start_stack >= 2 * (2 * first_small_blind + ante)      // at least one full orbit
small_blind_cap >= first_small_blind
blind_raise_every_hands >= 1
action_timeout_ms >= 5_000                             // PokerTH's non-LAN floor
hand_deadline_ms  >= 10 * action_timeout_ms
ante <= first_small_blind
showdown_policy is one this build implements            // Q-01, §7.7, §11
```

A config that fails goes to `TableClosed` and is never played. There is no repair path — repairing
a signed advertisement locally would mean two peers playing different games.

### 9.5 `SPEC_CS.md` §32 — heads-up first

All 20 phases and 57 transitions (§5.1) are reachable with `seats = 2` **except**:

* the multi-pot form of `build_pots` (§7.5) and the odd-chip distribution over more than two
  winners (§7.6), both of which require three distinct commitment levels or three tied winners;
* **T34**, the action-timeout transition, which §8.4 rule 6 and D-007 make unreachable whenever
  `|V(subject)| < 2` — and at two seats `|V|` is 1 in every hand, so T34 is unreachable for the
  whole of the heads-up mode;
* **T16, T22, T27, T41 and T44** for the same reason and by the same rule: every certificate is
  inert at `|V| = 1`, so no certificate-borne abort is reachable heads-up either. **T57 is
  reachable and is the only abort path a heads-up stall takes.**

The heads-up-specific rules — button-is-small-blind, inverted post-flop order, TDA 34-B button
adjustment — are exercised *only* heads-up, so both branches need explicit tests from the start
(A1.2).

**The first shipped mode is the one where the deadline machinery does not apply.** `SPEC_CS.md`
§32 requires heads-up first, and heads-up is exactly the configuration in which `|V|` is 1 in every
hand — so D-007 makes the action deadline advisory, forbids a fold-effect timeout certificate, and
D-009 rule 2 makes the crypto-deadline certificate inert there as well, so it neither attributes,
nor ends the hand. Everything §8.4 and §8.5 say about certificates, auto check/fold
and `consecutive_auto_actions` is therefore **dead code in the MVP** and first becomes live once
some hand has `|V| >= 2`, which first happens at three dealt-in seats. **The one deadline path the
MVP does run is T57**: heads-up, a stall of any kind ends the hand at `hand_deadline_ms` with
nobody named and stacks restored, so T57 and §8.6's one restoration rule are on the MVP's
critical path even though no certificate ever is. Four consequences for the test plan:

* the heads-up acceptance test must assert that a certificate of **either** kind is *rejected* —
  not merely absent, and not accepted-but-stripped, which is what the pre-D-009 text would have
  produced;
* the heads-up acceptance test must also cover T57 end to end: one seat goes silent, no certificate
  forms, the hand ends on the hand deadline with `attributed == []`, and every stack equals its
  `start_stack_this_hand` afterwards (I27, I2). Since P3 it must additionally assert that **the
  terminal stage closed against the silent seat's silence** — one peer's copy was enough, no peer
  waited for the seat that stalled, and both peers computed the identical `TERMINAL(k)`. A test that
  only checks the stacks would have passed while T57 could never fire;
* a **restoration test that is not a heads-up test**: at `seats >= 4`, a certificate-borne abort at
  `|V| >= 2` naming a seat must leave that seat's stack equal to its `start_stack_this_hand` and
  every other stack likewise (D-010, §8.6). Asserting only that the abort happened would pass while
  the forfeiture formula was still running;
* the certificate paths must be tested where `|V| >= 2` before they are relied on, because no
  heads-up run exercises them;
* the D-008 case needs its own adversarial test, and it is **not** a heads-up test and **not** a
  happy-path multi-seat test: at `seats >= 4`, a client that emits votes against several seats and
  then presents a certificate whose `signers` is a single seat must be rejected by §8.4 rule 4,
  because none of those seats entered `certified_subjects` (rule 7) and `V` therefore never
  shrank. Asserting only that a well-formed certificate is *accepted* at `n >= 3` would pass while
  N3 was live, so that test proves nothing about this.

**`RATED_SNG_POKERTH_V1` is fully specified in §9.1 and is not playable by the MVP.** It pins
`seats = 10` and `min_players_to_start = 10`, while `SPEC_CS.md` §32 requires two-player heads-up
as the first supported mode and §1.3 scopes the MVP at `nlhe/2-6`. The MVP ships `CUSTOM` tables;
`RATED_SNG_POKERTH_V1` becomes playable when `nlhe/7-10` lands. The Phase 8 acceptance test
therefore uses a **`CUSTOM` two-seat table** (`NETWORK_STACK.md` §12.12, D-003/D-004), and a reader
who takes the preset as the MVP's acceptance configuration will build the wrong test.
`PROTOCOL.md` §13 carries the same statement under its preset block.

The 3–6 player extension adds no new phase and no new transition. It changes only the guards that
count seats — plus T34, which starts existing once a hand has `|V| >= 2`. That is the concrete sense in which this machine
"does not make the multi-player extension impossible" (§32).

---

## 10. Invariants for the property tests

`SPEC_CS.md` §26 requires `proptest`/`quickcheck` invariants. Each is a predicate over
`TableState` (and, where noted, over a `(state, event, state')` triple), asserted after **every**
transition including rejected ones. **29 invariants.**

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
| **I9** | Board shape | `|board| ≤ 5` and `|board| == {PreFlop:0, Flop:3, Turn:4, River:5}[street]` whenever `phase` is a betting phase |
| **I10** | Card uniqueness | the multiset `board ∪ ⋃_s revealed_hole[s]` contains no duplicate, and every element is in `0..=51` |
| **I11** | Index discipline | `deck.opened.keys() ⊆ deal_map.all_indices()`, and each index appears at most once |
| **I12** | Street monotonicity | `street` never decreases, and advances only `PreFlop→Flop→Turn→River`; no street is skipped |
| **I13** | Phase discipline | every `(phase, phase')` pair appears in the §5.2 table; there is no other edge. **A row whose State column is a scope rather than one phase — T4, T48, T49, T50, T51, T57, T58, T59 — is read as instantiated at every phase in that scope**, so `(any live phase, HandAborted)` on `HandDeadlineAbort` (T57) — **`(Diverged, HandAborted)` included, which is P5's liveness exit** — and `(Seating \| AwaitingSeatRngCommit \| AwaitingSeatRngReveal \| Diverged, TableClosed)` on `FormationAbandoned` (T4) are *in* the table and are not rejections. I13 constrains edges, not the width of a row, and it is **not** a soundness check on the event that took an edge: it cannot tell a justly produced artefact from a manufactured one, and no invariant in §10 can. The two rows this clause used to name, T55 and T56, are deleted (G2) and the numbers are retired |
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
| **I29** | The voter-set floor (D-008) | two parts, both asserted after every transition. **(a)** `certified_subjects ⊆ deck.participants`, it is `∅` at hand init, and a seat enters it **only** on a certificate accepted under §8.4 rules 1–6 with `|V(subject)| ≥ 2` — so `|V|` is non-increasing within a hand and every decrement was paid for by a certificate that itself cleared the floor. **(b)** for every accepted `TimeoutCertificate`, `|V(subject)| ≥ 2` **and** `signers == V(subject)`. A certificate with `|V(subject)| < 2` is never accepted, of either `kind` (§8.4 rule 6, D-009 rule 2), so it produces no `AbortRecord`, no `FaultRecord`, no entry in `certified_subjects` and no stack change at all, and the post-state is bit-identical to the pre-state (I21). The previous form of this clause allowed a below-floor `kind == Crypto` certificate through with `attributed == []`; that carve-out is deleted, and the only `AbortRecord{kind: HandDeadline}` producer is now T57, whose `attributed` is empty by construction and which involves no certificate. **Under D-010 the stake in (b) is a false attribution record, not chips** — no certificate of any kind moves a chip any more — and the clause is kept for that reason and because `certified_subjects` still gates T34, which moves the action. Generate the adversarial case directly rather than by random play: a client that votes against `k` seats without completing a certificate against any of them must leave `certified_subjects` empty, so `|V|` must be unchanged — this is the N3 attack and a `proptest` over legal play never reaches it |

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

Four of these deserve dedicated adversarial tests rather than random generation, because a
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
* **I29** against a voter-set-collapse client at `seats >= 4` — the N3 construction: vote against
  every other seat but one, then present a single-signature certificate against the last. The
  assertion is that `certified_subjects` stayed empty, `|V|` stayed at `|dealt_in| − 1`, the
  certificate failed §8.4 rule 4, and **no seat's stack moved** (I21 as well as I29). This one is
  worth writing before the code it tests, because the defect it catches was live in four documents
  and looked correct in all of them;
* **I27** against every abort path, and specifically against a *named* one: at `seats >= 4`, a
  certificate-borne abort naming a seat must leave that seat's stack at `start_stack_this_hand`
  (D-010). A generator over legal play reaches aborts rarely and reaches attributed aborts almost
  never, so this is a directed test, not a `proptest`;
* **I7** and **I8** against the P7 construction, which random legal play does not produce either:
  three or more dealt-in seats, an `Absent` seat in the blinds, and a dealt-in seat all-in below
  that blind, asserting that the absent seat appears in no `eligible` set and receives no award.

---

## 11. Open questions carried forward

None of these is resolved here. Each is recorded so it cannot be lost, and each now carries a
**named default the engine may build against meanwhile** — an implementer should never have to
guess what the engine does while a question is open, only know that the behaviour is interim.

| # | Question | Owner document |
|---|---|---|
| Q1 | **Mucking at showdown.** Mandatory universal reveal, TDA-faithful muck with signed forfeiture, or delayed reveal at end of tournament. Each changes the cryptographic protocol, not just the engine. Carried unresolved from `POKER_RULES.md` A8. `config.showdown_policy` keeps both branches alive; the MVP's use of `MandatoryReveal` is implementation order, not a decision. **Named default:** the MVP implements `MandatoryReveal` and, per Q-01 below, **refuses** a table configured for a policy it has not implemented rather than playing it with T43 absent. | `PROTOCOL.md` / `DECISIONS.md` |
| Q3 | **Hand-deadline certificate signers when several seats are simultaneously unresponsive.** `V(subject)` is unachievable with two seats gone. A certificate naming a *set* of subjects is **not** adopted; neither is attributing every non-voting seat; and neither is **excluding** a seat from `V` for being the subject of an older unmet deadline, which D-008 deletes because it let `V` be shrunk by assertion. **Interim rule (§8.4), implemented and numbered: T57**, and since P3 it is a rule that can actually fire — the terminal stage is witness-independent (§4.1), so it does not wait for the seat whose silence caused it. The hand ends at `hand_deadline_ms` with `attributed = []` and no chip movement (I27). **D-010 shrinks what is left open:** attributing somebody would now put a name in the transcript rather than chips in a stack, so this no longer decides who pays, only what the record says. It is kept open because a later version may give attribution teeth again and the artefact has to be right before it does. Carried as **OQ-E** and blocking for Phase 4. | `PROTOCOL.md` Q-02, `THREAT_MODEL.md` §9.2 |
| Q5 | **Rebuys, add-ons and late registration.** Out of scope for the MVP and absent from `RATED_SNG_POKERTH_V1`, but they would change §5.3 and §9.3 and should be designed for rather than retrofitted. **Named default: not supported.** No config parameter selects them, §9.3's end conditions do not admit a re-entry, and §9.3's end conditions do not admit a re-entry, and since P8 hand init step 0 has no seat-entry clause at all (Q6). | `DECISIONS.md` |
| **Q6** | **May a seat be added after `Seating`?** Cash mode's whole difference from tournament mode is that `ledger_in` can move again, and nothing in the corpus can move it: there is no `PLAYER_SEAT` message in `PROTOCOL.md` §4.10 or §4.11, no hand-boundary stage for one, and `Event::PlayerSeated` is consumed only by T1 and T2 in `Seating` (P8). **Named default, implemented: not supported.** A cash table forms in `Seating` and thereafter only loses seats; §5.3 step 0 has no seat-entry clause and §9.4 no longer describes one; I1's right-hand side is non-increasing after the first hand in both modes. Adding it needs a message type, a stage kind under §3.2's principle, and a `HandComplete \| Paused × PlayerSeated` row — in that order, and not before a mode that needs it ships. | `PROTOCOL.md` §4.10/§4.11, `DECISIONS.md` |
| **Q7** | **What marks a seat `Absent` when the abort names nobody?** T46 sets `Absent` from `abort.attributed`, and T57's `attributed` is empty by construction, so a heads-up stall — the MVP's regime (§9.5) — marks nobody, deals the silent seat in again, and stalls the next hand too, for `hand_deadline_ms` each time. **Named default, implemented: nothing marks it**, and an implementer must not derive `Absent` from the abort's `owed` list: that list is the quantity peers disagree about, and deriving canonical state from it forks `TERMINAL(k)` (§4.1, P3). Closing this needs a basis every peer already agrees on, which does not exist yet; a slow table is a liveness cost, a forked chain is an integrity cost, and `SPEC_CS.md` §19 ranks them in that order. §12 records the cost. | `PROTOCOL.md` §4.10, `DECISIONS.md` |
| **Q-01** | **Is `showdown_policy = TDA_MUCK` offered at all?** This is `PROTOCOL.md` Q-01 seen from the engine, and it is listed here because §12 of that document names the engine's showdown path as what turns on it. It is *not* a second copy of Q1 above: Q1 asks which resolution the corpus should adopt, Q-01 asks whether the config value ships. **Named default, so no implementer has to guess what a build does with a value it does not implement:** §9.4's config validation is the gate, and a build validates `showdown_policy` against what it actually implements — a table whose value it does not implement **goes to `TableClosed` and is never played**, exactly as any other failing config parameter does, with no repair path. The MVP implements `MandatoryReveal` only, so an MVP client refuses a `TDA_MUCK` table rather than playing it with T43 silently missing. The value stays in the type (§7.7) and T43 stays specified. | `PROTOCOL.md` Q-01, `DECISIONS.md` |

**A note on the letters, because two of the rows below carry labels that have since moved (R-2).**
`DECISIONS.md` is authority for the corpus-wide `OQ-*` series and `PROTOCOL.md` §12 has adopted its
letters, so **OQ-A** now names the reference-engine question, **OQ-D** the dispute path that does
not need the accused's signature, and **OQ-F** whether the certificate and proof machinery is
produced at all. This document's own former uses of `OQ-A` and `OQ-D` named different questions;
both of those questions are closed, so the labels are annotated in place rather than reused, and
every live cross-reference in this document — §5.2, §8.4, §12 — is written against the adopted
scheme. `OQ-E` is `THREAT_MODEL.md`'s and is unchanged.

**Closed, recorded so the history is not lost.**

| # | Question as it stood | The answer |
|---|---|---|
| Q4 | Whether `STATE_HASH` is exchanged at every phase boundary or only at hand boundaries. `SPEC_CS.md` §15 says "after critical transitions" without enumerating them, and this document made every phase hashable so that either policy would be implementable. | **Closed** by `PROTOCOL.md` §6.2, which enumerates them normatively — seven checkpoints, "at these points and no others": after `TABLE_READY`, after `DECK_COMMIT`, after each of the four betting rounds closes, and immediately before `SHOWDOWN_REVEAL`. It is neither of the two candidate policies but a third, chosen for a reason this document must not undo: **no checkpoint is placed after a hole card has been opened to anyone but its owner**, because a checkpoint after the cards are known is an abort trigger the loser can pull with full information (§6.3 case (c), `THREAT_MODEL.md` X29). The engine's part is unchanged — every phase is hashable — but the *placement* is settled and is `PROTOCOL.md`'s, not an open choice. |
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
  It does not forfeit, does not unseat, does not block-list, and does not penalise. The one state
  change attribution still causes is `status := Absent` at T46, which is D-005's absent-seat rule
  — the seat keeps its stack, its position and its player, and returns on its own `PlayerSitsIn`
  — and is not a penalty (§8.6). Anything in another document that reads a consequence into an
  attribution is describing a version of this protocol that does not exist yet.
* **Wherever the required voter set has fewer than two members there is no enforceable deadline of
  any kind** (D-007 as generalised by D-008, D-009 rule 2, §8.4 rule 6). An opponent who stalls, or
  who goes silent mid-hand, cannot be punished inside the protocol there: **every** certificate is
  inert below the floor, of either kind, so the action deadline is a UI countdown that produces no
  signed transition and the crypto deadline produces nothing at all. The hand still ends — at
  `hand_deadline_ms`, ten minutes under the preset, with nobody attributed and stacks restored
  (T57) — but nothing about it is a punishment. Heads-up is the common case — `|V|` is 1 in every
  heads-up hand, and heads-up is the first shipped mode (§9.5), so this is the regime the MVP
  actually runs in — but the statement is scoped on `|V|` and **not** on the seat count, because
  scoping it on the seat count was itself the defect (D-008). Since D-010 the same is true *above*
  the floor as well: a certificate at `|V| >= 2` names a seat and ends a hand, and that is the
  whole of its effect on chips, which is none.
* **Liveness is not owed, and this document does not claim it.** There is no bound on how long a
  table can be made to make no progress: a stall costs `hand_deadline_ms` per hand and can be
  repeated every hand, and two colluding seats can do the same at any table size (Q3).
  `SPEC_CS.md` §19 ranks security above finishing a hand conveniently and D-009 rule 2 applies that
  ranking literally, in preference to an effect a single signature could manufacture.
  **What *is* claimed is narrower, and it is checked phase by phase in §12.1: every phase has a
  reachable exit under every adversarial behaviour, including a peer that simply stops.** Slow is
  not the same as frozen.
* **A heads-up stall repeats.** T57 names nobody, nothing marks the silent seat `Absent`, and the
  next hand deals it in and stalls again for `hand_deadline_ms`. This is the cost of not deriving
  canonical state from a set peers disagree about, it is recorded as **Q7** in §11, and an
  implementer must not close it locally — the local fix forks the chain.
* **Conversely, the certificate's protection at `|V| >= 2` is not a protection against collusion.**
  It requires *every* seat still in the voter set to sign, so `|V|` colluders defeat it — two of
  them at a three-seat table. This document claims only that a certificate cannot be produced
  unilaterally, and `SPEC_CS.md` §18 forbids reading more into it than that.
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
* **Every recorded deviation is listed once**, in the register at `THREAT_MODEL.md` §9.1.1. This
  document is a source for four of its rows and restates none of them: the once-per-table randomness
  beacon (§7.9, row 2), no burn cards (§7.8, row 3), the absent seat that pays and cannot win
  (row 4 — implemented in §7.5's `dealt_in` filter since P7, which is where it previously was not),
  and the advisory deadline where `|V| < 2` (§8.4, §9.5, row 5 — recorded as the
  "advisory heads-up deadline", which is that row's common case; D-008 widens its scope from the
  seat count to the voter set and the register wording should follow). **A fifth row is now owed:
  the neutral abort of D-010**, which deviates from D-005's forfeiture rule and from the live-poker
  analogue that a player who cannot act loses what they bet. A deviation that is not in that
  register has not been recorded, whatever any single document says about it.
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
| 16 | `HandComplete` | **T47**, derived `NextHand`. No external input | immediately | `AwaitingKeySetup` \| `Paused` \| `TableClosed`, per §9.3 |
| 17 | `HandAborted` | **T46**, derived `AbortSettle`, guard `—`. No external input | immediately | `HandComplete` |
| 18 | `Paused` | **idles, and that is correct**: no hand is live, no chips are committed, no deadline is armed, `Σ committed_hand == 0`. Left by T59 on a `PlayerSitsIn`. A `Paused` table every seat has left **stays** `Paused` — §9.3 is evaluated only at T47 and T47 does not run from here — which is idle, not frozen, and is corrected wording rather than a new exit | — | nothing is owed to anybody; a client closes the window |
| 19 | `TableClosed` | absorbing. Every event is a `Rejection` | — | — |
| 20 | `Diverged`, hand live | **T57** — the hand deadline is **not** disarmed on entry (T50) and phase 20 is in T57's scope | `hand_deadline_ms` from `TERMINAL(k−1)`, still running | `HandAborted`; `table_faulted` **unchanged**, so the table plays on |
| 20 | `Diverged`, no hand started | **T4**, guard `hand_id == 0 ∧ ledger_in == 0` — reachable because checkpoint 1 sits after `TABLE_READY` and before any `HAND_INIT` | `join_deadline_ms` | `TableClosed` |

**Four things the table asserts that a reader should check rather than take on trust.**

1. **Phases 4–14 and 20 no longer depend on an artefact that cannot be accepted.** That was G1: the
   terminal `HAND_ABORT` collided with its own emitter's contribution at the stalled stage, so every
   receiver rejected it and T57 could never fire — on the MVP's shipped regime, which is *every*
   heads-up stall, not a corner. It is closed in `PROTOCOL.md` §5.2.1 by putting `event_type` in the
   slot key, and this document reproduces no part of that key. No row above rests on a rule this
   document invented.
2. **No exit reads a certificate, a voter set, or `|V|`.** T4, T45, T46, T47 and T57 are the whole
   of the column and none of them touches `V`. That is what makes the table true heads-up, where
   `|V| = 1` in every hand and every certificate is inert (D-009 rule 2). It is also why the table
   does not change shape between two seats and nine.
3. **No exit requires the silent peer to do anything.** T4 is a local timer; T45, T46 and T47 are
   derived from state; T57's stage has no required emitter set (`PROTOCOL.md` §3.2), so it closes on
   the first copy from any peer. A peer that stops cannot hold any phase open, which is the property
   the whole table exists to establish.
4. **Every exit leads to a phase that has an exit, and the walk terminates.** Rows 1–3 and the
   second row of 20 reach the absorbing `TableClosed` directly. Rows 4–14 and the first row of 20
   reach `HandAborted`, which T46 leaves unconditionally, reaching `HandComplete`, which T47 leaves
   unconditionally for `AwaitingKeySetup` (back to row 4), `Paused` (row 18, which owes nobody
   anything) or `TableClosed`. The cycle 4 → 17 → 16 → 4 makes progress on the one counter that
   matters: `hand_id` strictly increases across it, and §9.3's conditions are re-evaluated every
   time round.

**Two exits that are new in this revision, so the table is not read as unchanged.** Row 3's exit
changed because T11 stopped unseating (G4): the previous revision left `AwaitingSeatRngReveal` on a
commitment mismatch by removing the seat, which was a peer removal D-010 point 3 forbids; it now
stalls to T4 like every other beacon failure. And the first row of 20 no longer faults the table:
`PROTOCOL.md` §6.4 reserves that for `cause = 4`, so a divergence that times out ends the hand and
the table deals the next one (§5.2, T57's `Diverged` note), whereas a divergence that reconciles
into case (c) closes the table through T54 and §9.3 condition 0.5.

**What the table does not claim, said plainly.** It is a *termination* argument, not a liveness
bound. Ten minutes a hand, every hand, forever is a passing row here and an unplayable table in
practice. The bound on how bad it gets is `hand_deadline_ms` per hand with no limit on repetition,
and closing that needs Q7's agreed basis for marking a seat absent, which does not exist. **Read
the table as: nothing is frozen, and slow is the worst case.**

**Where the previous revisions failed this check, recorded so the fix can be verified rather than
believed.** The Phase 2 gate found phases 4–14 and 20 with **no reachable exit** — the whole of the
regime in which chips are committed — because T57's artefact was unacceptable at every receiver
under G1. The revision before that left phase 20 with no exit at all (P5). The one before that
could not complete the `HAND_ABORT` stage, because its required emitter set contained the silent
seat (P3). Three phases, three revisions, one class of defect: **a terminus that depends on the
participation of the peer whose non-participation is the reason for it.** Every row above is
checked against exactly that question, which is why the *Fires after* column names a timer or the
word *immediately*, and never a peer.

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
    when T35 became a rejection; it is set at T46 from `abort.attributed`, which is where
    `PROTOCOL.md` §4.10 says it is set. **(e)** §9.3 gained end condition 0, the empty table, which
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

**Two objections addressed to `PROTOCOL.md`.** The three that stood here are closed: §3.2 and
§4.10 carry the witness-independent terminal stage (item 22), §6.3 step 3 and §4.9 carry the
reconciliation round (item 23), and §8.2 starts the hand deadline at `TERMINAL(k−1)` (item 19).
What remains is two, and the engine builds against a named default for each:

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
