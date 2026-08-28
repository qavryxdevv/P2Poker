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
| `DECISIONS.md` D-005 | absent seat keeps its stack, pays blinds, takes no cards; mid-hand abort and chip forfeiture |
| `DECISIONS.md` D-006 | action timeout is auto check/fold, never an abort; timeout certificate |
| `docs/research/POKER_RULES.md` | every NLHE rule, the TDA citations, the `RATED_SNG_POKERTH_V1` preset |
| `docs/research/MENTAL_POKER.md` | what the crypto layer can and cannot do, `n`-of-`n`, per-card reveal tokens |
| `docs/research/CRYPTO_LIBS.md` | canonical bytes (`minicbor` arrays, no maps, no floats), BLAKE3, the canonicality gate |
| `docs/research/GUI_STACK.md` | crypto must not run on the UI thread; the engine is called from both |

**What this document is not.** It contains no new API claims. Every crate-level fact it
relies on (`rs_poker =5.0.0` behind a façade, `minicbor` array encoding, `getrandom::SysRng`,
`ziffle` reveal tokens) is carried unchanged from the research documents together with the
verification recorded there. Nothing here was invented; where something is undecided it is
marked **OPEN QUESTION** rather than resolved.

**Counts.** 19 phases, 26 invariants, 47 numbered transitions.

---

## 1. Position in the architecture

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
    Busted,      // stack == 0, eliminated, position retained for the dead button
}
```

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

    // ---- time, without a clock (D-006) ------------------------------------
    pub deadline:            Option<Deadline>,

    // ---- results and evidence ---------------------------------------------
    pub settlement:          Option<Settlement>,  // last completed hand
    pub finish_order:        Vec<SeatIdx>,        // busted seats, first busted first
    pub faults:              Vec<FaultRecord>,    // SPEC_CS.md §19 evidence
    pub abort:               Option<AbortRecord>,
}

pub struct ActionRecord {
    pub sequence: u64, pub seat: SeatIdx, pub street: Street,
    pub action: Action, pub amount_to: Chips, pub was_auto: bool,
}

pub struct Deadline {                 // a DESCRIPTION, not a timestamp
    pub kind:        DeadlineKind,    // Action | Crypto | Join
    pub subject:     SeatIdx,
    pub sequence:    u64,             // the sequence this deadline is attached to
    pub parent_hash: Hash,            // the event the deadline chains from
    pub duration_ms: u32,             // from config; the engine never adds it to anything
}
```

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
| `pot` | **derived** `pots(&self) -> Vec<Pot>` | see below |
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
| `Seat::committed_hand` | `build_pots` input (A7), and the D-005 abort forfeiture base. |
| `Seat::acted_this_round` | Encodes the big blind's option; round termination and `can_reopen` both read it (A2, A5, appendix). |
| `sb_pos`, `bb_seat` separate from `button_pos` | The dead button (A1.3) rotates three independent positions; one field cannot express a dead button *and* a dead small blind. |
| `Seat::start_stack_this_hand` | Abort settlement (D-005) and the simultaneous-bust-out tie-break (A1.3) both need the value as of hand start. |
| `Seat::dealt_in` | D-005: an absent seat pays blinds and takes no cards. Without this field "pays but is not a party to the cryptography" is inexpressible. |
| `SeatStatus` | D-005 / D-006 §4: sitting-out and absent seats keep their stacks and drain. |
| `Seat::consecutive_auto_actions` | D-006 §4: after `auto_action_limit` consecutive auto-actions the seat is marked sitting out. |
| `Seat::revealed_hole`, `Seat::mucked` | Showdown results are public and identical everywhere; a verifier re-computes the award from them. |
| `deadline` | D-006 requires the deadline to be **explicit state**, not a wall-clock read inside the engine. |
| `sequence`, `previous_event_hash` | §12/§13/§14: monotonic sequence and parent hash are what make replay, reordering and equivocation detectable. |
| `level` | Tournament layer (§9); cached derivation of `hand_id`, asserted by I25. |
| `deck` | The crypto-waiting bookkeeping. Hashes only; see §2.5. |
| `rng_beacon` | §7 commit/reveal for seat order and initial button (`MENTAL_POKER.md` §6). |
| `aggressor` | TDA 17-A order of show (`POKER_RULES.md` A8). |
| `settlement` | Lets any peer re-check "the winner and the pot size were computed correctly" (§13) from the transcript alone. |
| `finish_order` | Tournament result; needs a total order with no floor person (A1.3). |
| `faults`, `abort` | §19 requires *evidence of which peer failed*, in the transcript, signed. |

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

`STATE_HASH` covers `TableState` in full, including `phase`, `deck` and `deadline`. It covers
nothing from `LocalView`. Two honest peers that have processed the same event prefix produce the
same hash; a mismatch triggers the §15 dispute path and the game does not continue silently.

### 3.4 Derived events

Some transitions have no author. `HAND_COMPLETE`, `HAND_INIT` for the next hand, and the
settlement step are *computed*, not decided (this is exactly `SPEC_CS.md` §4's "not driven by
whoever clicks first"). The engine emits them in `Step::derived`; every peer emits byte-identical
derived events independently and inserts them into the hash chain at the same sequence number.

They carry no signature, because there is no signer. Their correctness is guaranteed by
determinism, and a peer that derives something different is caught by the very next `STATE_HASH`
comparison — which is the §15 mechanism working as designed rather than a hole in §12.

**OPEN QUESTION (for `PROTOCOL.md`).** The exact envelope of an unsigned derived event: what
goes in `sender_public_key`, and whether each peer additionally counter-signs the derived event
so the transcript carries attributable agreement. This document requires only that the *bytes*
be a pure function of the previous state; the envelope is the protocol layer's to fix.

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

    // ---- time, as a signed artefact only (D-006) ----
    TimeoutCertificate { subject: SeatIdx, kind: DeadlineKind,
                         sequence: u64, parent_hash: Hash, signers: SeatSet },

    // ---- engine-derived (§3.4) ----
    Settle,
    AbortSettle,
    NextHand,
}
```

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

Effects never affect the state. Replaying a transcript with the effect handler disabled must
reproduce the identical final state — that is a property test (I22).

---

## 5. The phase state machine

### 5.1 The 19 phases

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
    17 HandAborted,            //  SPEC_CS.md §19 / D-005
    18 Paused,                 //  fewer than two seats willing to be dealt in
    19 TableClosed,            //  terminal, absorbing
}
```

**Nine** of the nineteen — 2, 3, 4, 5, 6, 8, 10, 12 and 14 — are cryptographic waits, and one
(15) is a pure computation checkpoint. They are first-class states because the engine has to be able to make no
progress at all, indefinitely, while remaining a valid hashable state that every peer agrees on,
and because the *only* legal exits from them are a crypto verdict, a timeout certificate, or a
fold-out that makes the pending cryptography unnecessary. A boolean "waiting" flag could not
express which artefact is owed by whom, which is exactly what §19 requires as evidence.

### 5.2 The transition table

Notation. `k` = number of dealt-in seats. `live(s)` = dealt-in ∧ ¬folded. `contenders(s)` =
live ∧ ¬all_in. `round_closed(s)` is the `POKER_RULES.md` A2 predicate: every contender has
`acted_this_round` **and** `committed_round == current_bet` (or is all-in for less). Effects are
abbreviated. A transition not listed does not exist; any event arriving in a state with no
matching row is a `Rejection` and leaves the state bit-identical (I21, I13).

#### Seating and start-of-table

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T1 | `Seating` | `PlayerSeated` | seat empty ∧ occupied+1 < `min_players_to_start` ∧ config valid | `Seating` | — |
| T2 | `Seating` | `PlayerSeated` | occupied+1 == `min_players_to_start` | `AwaitingSeatRngCommit` | `ArmDeadline{Join}`; request commits from all seated |
| T3 | `Seating` | `PlayerLeft` | seat occupied | `Seating` | seat → `Empty`, stack returned to nothing (no chips exist yet) |
| T4 | `Seating` | `TimeoutCertificate{Join}` | occupied < `min_players_to_start` | `TableClosed` | `Fault` none; table simply never started |
| T5 | `Seating` | any other | — | `Seating` | `Rejection` |

#### Seat-order randomness beacon (`SPEC_CS.md` §7, `MENTAL_POKER.md` §6)

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T6 | `AwaitingSeatRngCommit` | `RngCommit` | seat seated ∧ not yet committed | `AwaitingSeatRngCommit` | record commitment |
| T7 | `AwaitingSeatRngCommit` | `RngCommit` | all seated seats have committed | `AwaitingSeatRngReveal` | `ArmDeadline{Crypto}`; request reveals |
| T8 | `AwaitingSeatRngCommit` | `TimeoutCertificate{Crypto}` | subject has not committed | `Seating` | subject unseated; `Fault{NoRngCommit}` |
| T9 | `AwaitingSeatRngReveal` | `RngReveal` | `H(value‖salt) == commitment` ∧ not yet revealed | `AwaitingSeatRngReveal` | record |
| T10 | `AwaitingSeatRngReveal` | `RngReveal` | all revealed | `AwaitingKeySetup` | `seed = BLAKE3(v₁‖…‖vₙ)`; derive seat permutation and initial `button_pos`; `hand_id := 1`; **hand init** (§5.3); `RequestKeySetup`; `ArmDeadline{Crypto}` |
| T11 | `AwaitingSeatRngReveal` | `RngReveal` | commitment mismatch | `Seating` | subject unseated; `Fault{RngCommitmentMismatch}` — this is an equivocation-class fault with a self-contained proof (the commitment and the bad opening) |
| T12 | `AwaitingSeatRngReveal` | `TimeoutCertificate{Crypto}` | subject has not revealed | `Seating` | subject unseated; `Fault{NoRngReveal}` |

The beacon runs **once per table**, not per hand. Per `MENTAL_POKER.md` §6, the shuffle chain
*is* the per-hand randomness; a commit/reveal beacon is needed only for the non-deck randomness
(seat assignment and the initial button), which is precisely what T6–T12 cover.

#### Per-hand key setup

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T13 | `AwaitingKeySetup` | `KeyPublished` | seat ∈ `deck.participants` ∧ not yet published | `AwaitingKeySetup` | record |
| T14 | `AwaitingKeySetup` | `KeyPublished` | all participants published | `AwaitingShuffle` | compute `agg_key_hash`; fix `deck.shuffle_order` and `deck.deal_map` (§7.8) **before** any shuffle; `RequestShuffle{first}`; `ArmDeadline{Crypto}` |
| T15 | `AwaitingKeySetup` | `KeyRejected` | — | `HandAborted` | `Fault{BadKeyProof}`; `AbortRecord` names the seat |
| T16 | `AwaitingKeySetup` | `TimeoutCertificate{Crypto}` | subject owes a key | `HandAborted` | `Fault{NoKey}`; `AbortRecord` |
| T17 | `AwaitingKeySetup` | `PlayerLeft`/`PlayerSitsOut` | — | `HandAborted` | subject → `Absent`/`SittingOut`; abort this hand, next hand excludes them (D-005) |

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
| T22 | `AwaitingShuffle` | `TimeoutCertificate{Crypto}` | subject is the expected shuffler | `HandAborted` | `Fault{NoShuffle}`; `AbortRecord` |

#### The private deal

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T23 | `AwaitingDeal` | `RevealTokensPublished` | indices ⊆ hole indices of seats **other than** the publisher | `AwaitingDeal` | record in `deck.tokens` |
| T24 | `AwaitingDeal` | `RevealTokensPublished` | publisher included **its own** hole index | `AwaitingDeal` | `Rejection` + `Fault{SelfRevealTooEarly}` — publishing your own token pre-showdown would let everyone open your hand |
| T25 | `AwaitingDeal` | `RevealTokensPublished` | any index ∉ this hand's hole indices | `AwaitingDeal` | `Rejection` + `Fault{TokenForUnauthorisedIndex}` — this is the §10 "early board" attack |
| T26 | `AwaitingDeal` | derived `DealComplete` | ∀ participant `P`, ∀ hole index `i` of `P`: every participant `≠ P` has published a token for `i` | `BettingPreFlop` | `street := PreFlop`; open the betting round (§5.4); `player_to_act` per A2; `ArmDeadline{Action}` |
| T27 | `AwaitingDeal` | `TimeoutCertificate{Crypto}` | subject owes tokens | `HandAborted` | `Fault{NoDealTokens}`; `AbortRecord` lists exactly which `(seat, index)` pairs were owed |

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
| T30 | `S` | `Action{Fold}` | legal ∧ exactly one live seat remains | **`Settling`** | `DisarmDeadline`; **no reveal is requested at all** — see §8.5 and D-005 case 1 |
| T31 | `S` | `Action{Check\|Call\|Bet\|Raise}` | legal ∧ ¬`round_closed` after applying | `S` | move chips; update `current_bet`, `last_full_raise`, `aggressor`; `acted_this_round := true`; advance `player_to_act`; `ArmDeadline{Action}` |
| T32 | `S` | `Action{…}` | legal ∧ `round_closed` ∧ `S != BettingRiver` | `next_reveal(S)` | `DisarmDeadline`; `RequestOpen{street indices, Public}`; `ArmDeadline{Crypto}` |
| T33 | `S` | `Action{…}` | legal ∧ `round_closed` ∧ `S == BettingRiver` ∧ `|live| ≥ 2` | `AwaitingShowdownReveal` | compute `showdown_order` (§7.7); `RequestOpen{hole indices of each live seat, OwnerOf(seat)}`; `ArmDeadline{Crypto}` |
| T34 | `S` | `TimeoutCertificate{Action}` | `subject == player_to_act` ∧ signers == `deck.participants \ {subject}` ∧ `sequence`/`parent_hash` match `state.deadline` | as T29–T33 | apply `Check` if `to_call == 0`, else `Fold` (D-006 §1); `was_auto := true`; `consecutive_auto_actions += 1`; if it reaches `auto_action_limit`, mark the seat `SittingOut` **effective at the next hand boundary** |
| T35 | `S` | `PlayerLeft` | — | `S` | mark `Absent`; **the hand continues** — the seat is still a key holder, so nothing is unblocked; the effect is felt at the next crypto wait |
| T36 | `S` | `Show`/`Muck` | — | `S` | `Rejection` — showdown declarations are only legal in `AwaitingShowdownReveal` |

T32 has one further guard worth stating separately because it is the all-in run-out:

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T37 | `next_reveal(S)` | `CardsOpened` | street cards appended ∧ `|contenders| ≥ 2` | the betting phase for that street | reset the round (§5.4); `player_to_act` per A2; `ArmDeadline{Action}` |
| T38 | `next_reveal(S)` | `CardsOpened` | street cards appended ∧ `|contenders| < 2` ∧ street < River | the **next** reveal phase, betting skipped | `RequestOpen{next street, Public}`; `ArmDeadline{Crypto}` — `POKER_RULES.md` A2: remaining streets are still dealt because they decide the pots |
| T39 | `next_reveal(S)` | `CardsOpened` | river opened ∧ `|contenders| < 2` ∧ `|live| ≥ 2` | `AwaitingShowdownReveal` | `RequestOpen{hole indices, OwnerOf(seat)}`; `ArmDeadline{Crypto}` |
| T40 | `next_reveal(S)` | `CardsOpened` | opened indices ≠ exactly the indices this street owes | `next_reveal(S)` | `Rejection` + `Fault{WrongRevealSet}` |
| T41 | `next_reveal(S)` | `TimeoutCertificate{Crypto}` | subject owes tokens for this street | `HandAborted` | `Fault{NoBoardTokens}`; `AbortRecord` |

#### Showdown, settlement, hand end

| # | State | Trigger | Guard | Next | Side effects |
|---|---|---|---|---|---|
| T42 | `AwaitingShowdownReveal` | `CardsOpened` | all required hands opened (§7.7) | `Settling` | fill `Seat::revealed_hole`; `DisarmDeadline` |
| T43 | `AwaitingShowdownReveal` | `Muck` | `showdown_policy == TdaMuckWithForfeiture` ∧ seat is not the last unmucked | `AwaitingShowdownReveal` | `mucked := true` — an irrevocable forfeiture of every pot (§7.7) |
| T44 | `AwaitingShowdownReveal` | `TimeoutCertificate{Crypto}` | subject owes its own token | `HandAborted` | `Fault{NoShowdownToken}`; `AbortRecord` |
| T45 | `Settling` | derived `Settle` | — | `HandComplete` | `build_pots` (A7), evaluate, award, refund uncalled excess, split with odd chips (A8), mark busts, extend `finish_order`; `Settled(settlement)` |
| T46 | `HandAborted` | derived `AbortSettle` | — | `HandComplete` | D-005 forfeiture (§8.6); `settlement.aborted := true`; `Fault` records already present |
| T47 | `HandComplete` | derived `NextHand` | see §9.3 end conditions | `AwaitingKeySetup` \| `Paused` \| `TableClosed` | rotate the dead button (A1.3), advance the blind level (§9.2), reset the hand, run **hand init** (§5.3) |

`Paused` and `TableClosed`:

* `Paused` + `PlayerSitsIn` → `AwaitingKeySetup` when at least two seats are again willing to be
  dealt in and at least two have chips. `Paused` arms no deadline and moves no chips.
* `TableClosed` is absorbing. Every event is a `Rejection`. `SPEC_CS.md` §4 and D-006 §5 both
  forbid a timeout from producing this state during play; only a won tournament, an
  unstarted table (T4) or every seat leaving reaches it.

### 5.3 Hand init (the procedure entered on T10 and T47)

Pure, no events, no clock. In this exact order:

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
   `deck := DeckState::new(participants = dealt_in seats)`; `settlement := None`; `abort := None`.
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
fn build_pots(committed: &[Chips], folded: &[bool]) -> (Vec<Pot>, Vec<(SeatIdx, Chips)>) {
    let mut levels: Vec<Chips> = committed.iter().copied().filter(|&c| c > 0).collect();
    levels.sort_unstable();  levels.dedup();
    let (mut pots, mut refunds, mut prev) = (Vec::new(), Vec::new(), 0);
    for lvl in levels {
        let contributors: Vec<SeatIdx> = seats().filter(|&s| committed[s] > prev).collect();
        let size = (lvl - prev) * contributors.len() as Chips;
        if contributors.len() == 1 {
            refunds.push((contributors[0], size));       // uncalled excess, never a pot
        } else {
            let eligible = contributors.iter().copied().filter(|&s| !folded[s]).collect();
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
* The single-contributor test is simultaneously the uncalled-bet refund rule, the
  "everyone folds to a bet" rule, and the "A bets 300, B raises all-in to 900, A folds" rule.
* `eligible` is never empty in a legally reachable state. The engine must nevertheless **assert**
  non-emptiness, and if a byzantine peer ever produces such a state, split that pot pro rata
  among its contributors rather than destroying chips (A7).
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

### 7.8 The deal map — fixed before the shuffle

`MENTAL_POKER.md` §6 is explicit that the index→recipient mapping must be fixed **before** the
shuffle chain starts, or a malicious last shuffler could argue afterwards about which index is
whose card. It is therefore computed in T14, from public state only, with no randomness:

Let `D = [d₀ … d_{k-1}]` be the dealt-in seats in clockwise order starting from
`succ(button_pos)`. Then

```
hole card 1 of dⱼ = index j
hole card 2 of dⱼ = index k + j
flop              = indices 2k, 2k+1, 2k+2
turn              = index   2k+3
river             = index   2k+4
```

with `k ≤ 10`, so at most 25 of 52 indices are ever used.

**No burn cards** (**[OUR CHOICE]**). A burn exists to defeat marked-card reading of a physical
deck. Here it would consume an index that would then have to be provably never opened, adding a
rule with no security value.

Indices `2k+5 … 51` are **never** opened. A reveal token for any of them is a protocol violation
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

Per `MENTAL_POKER.md` §6: the shuffle chain already *is* the per-hand distributed randomness, so
`SPEC_CS.md` §7's substantive requirement is met structurally — the final deck order is the
composition of all `n` secret permutations and is uniform as long as **one** player is honest.
The last shuffler gains nothing, because it sees only ciphertexts under the aggregate key and has
no information to bias toward.

The commit/reveal beacon (T6–T12) exists for the *non-deck* randomness only: seat assignment and
the initial button position. `commit_i = BLAKE3(value_i ‖ salt_i)`,
`seed = BLAKE3("p2p-poker/seat-beacon/v1" ‖ value_1 ‖ … ‖ value_n)` in seat-index order.
Failure to reveal, or a mismatched opening, is an attributable fault with a self-contained proof.
`value_i` and `salt_i` come from `getrandom::SysRng` (`CRYPTO_LIBS.md` §1 — `OsRng` no longer
exists in the current release line).

---

## 8. Timeouts, disconnect and abort

### 8.1 The distinction that governs everything here

D-006 corrects a conflation in D-005 and this document adopts the correction verbatim:

| | Human absent, client running | Client gone or withholding |
|---|---|---|
| Betting decision | auto check/fold | auto check/fold |
| Decryption shares | published normally | **not published** |
| The hand | **continues to the end** | cannot open further cards |
| Consequence | next street, next hand | abort, attribute, next hand |

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
peer holds assertions from *every* required signer does it assemble a `TimeoutCertificate` and
feed that to `step` as an ordinary event.

### 8.3 `hand_delay_sec` is not engine state

`RATED_SNG_POKERTH_V1` carries `hand_delay_sec = 7` from PokerTH's config default. It is a
**display** delay so a human can see the result. The protocol does not wait for it: T47 fires as
soon as the terminal event of hand `h` is in the chain, and peers may publish hand `h+1`'s key
setup immediately. A GUI still animating hand `h` simply lags behind a state that has already
advanced; that is a rendering concern, and making it an engine state would reintroduce a clock.

### 8.4 The timeout certificate (D-006)

```rust
pub struct TimeoutCertificate {
    pub table_id: TableId, pub hand_id: u64,
    pub subject: SeatIdx, pub kind: DeadlineKind,
    pub sequence: u64, pub parent_hash: Hash,
    pub signers: SeatSet,            // Ed25519 signatures carried in the envelope
}
```

Validity, checked by `step` before any effect (T34, T41, T44…):

1. `table_id`, `hand_id` match the current state (§14 replay defence);
2. `sequence` and `parent_hash` equal `state.deadline`'s — so a certificate cannot be replayed
   into a different position in the hand;
3. `subject == state.deadline.subject`;
4. `signers == deck.participants \ {subject}` — **unanimity of every other dealt-in seat**. No
   quorum reduction is permitted;
5. every signature verifies (`verify_strict`) over the canonical bytes.

**Why unanimity, and what it buys.** One peer asserting a timeout would let anyone steal the
action from a player who was about to act. Requiring every other active player also settles the
race with no window: if the slow player's action reached any one of them first, that peer will not
sign, no certificate forms, and the action stands; if none accepted an action, the certificate
forms and the action is too late. There is never a state in which both a valid action and a valid
certificate exist for the same parent.

**Why the quorum is never reduced.** Two clients both genuinely gone cannot sign each other's
certificates, so no action certificate forms. That is correct behaviour, not a deadlock: the
escalation is the `hand_deadline_ms` crypto deadline, whose certificate has the same unanimity
requirement over the *remaining* signers and which leads to the D-005 abort. Reducing the quorum
to make progress would reintroduce exactly the thing D-006 was designed to prevent — a subset of
players asserting facts about another player.

**OPEN QUESTION (for `PROTOCOL.md`).** The hand-deadline certificate's signer set when several
seats are simultaneously unresponsive. `signers == participants \ {subject}` becomes unachievable
if two seats are gone. A defensible reading is `signers == participants \ {all seats named as
failing in this certificate}`, with the certificate naming a *set* of subjects; that keeps
unanimity among everyone still present without letting a minority act. This document does not fix
it, because the choice interacts with the abort attribution in §8.6 and belongs with the
certificate's exact field list.

### 8.5 Auto check/fold, and the fold-out escape hatch

On a valid action-timeout certificate (T34) the engine applies **`Check` if `to_call == 0`, else
`Fold`**. Never fold a hand that could check for free (D-006 §1). The auto-action is a real entry
in `history` with `was_auto = true`, derived identically by every peer from the same state, so it
stays deterministic and verifiable (§11, §13). `consecutive_auto_actions` increments; on reaching
`config.auto_action_limit` the seat is marked `SittingOut`, **effective at the next hand
boundary**, and thereafter behaves as a D-005 absent seat: keeps its stack, pays blinds and antes,
takes no cards, drains until it busts. It may sit back in at a hand boundary.

T30 is the escape hatch that D-005 case 1 names explicitly: if folding leaves exactly one live
seat, the engine goes **straight to `Settling`** with no reveal request at all, even when a peer
has vanished and is blocking every possible opening. A player who quits to escape a losing pot
does not escape if the others simply fold.

### 8.6 Abort, attribution and chip forfeiture (D-005)

Barnett–Smart is `n`-of-`n`: any dealt-in seat that stops publishing makes it impossible for
**anyone** to open any further card, board included (`MENTAL_POKER.md` §8). The hand cannot be
finished by the remaining players, and the standard robustness upgrade — `t`-of-`n` threshold
ElGamal — is **refused**, because with `t < n` any `t` colluding players could decrypt every hole
card at the table, violating `SPEC_CS.md` §35 and §19 directly.

So the abort path is the answer, and §19 prescribes its shape: timeout, abort, evidence, penalty.

```rust
pub struct AbortRecord {
    pub hand_id:            u64,
    pub phase_at_abort:     Phase,
    pub kind:               AbortKind,   // NoKey | InvalidShuffleProof | NoShuffle |
                                         // NoDealTokens | NoBoardTokens | NoShowdownToken |
                                         // HandDeadline
    pub attributed:         Vec<SeatIdx>,
    pub owed:               Vec<(SeatIdx, Requirement)>,  // exactly what each seat failed to produce
    pub observed_by:        SeatSet,                      // certificate signers
    pub sequence:           u64,
    pub parent_event_hash:  Hash,
}
```

This is a signed protocol event in the transcript, so every participant can independently verify
*who* failed, *what* they owed and *when* — `SPEC_CS.md` §19's "evidence of which peer failed",
made checkable rather than asserted. Attribution is by **missing artefact**, never by liveness
guesswork: the missing `(seat, card_index)` reveal token is publicly visible, and everyone else's
signed events prove they did their part.

**Chip settlement on abort (T46), D-005.** The absent player forfeits what they committed; it is
distributed to the remaining players in proportion to their own contributions. Restoring stacks to
their start-of-hand values was rejected because it hands every player a free in-protocol escape
from a losing pot. Exactly:

```
culprits = abort.attributed
forfeit  = Σ_{c ∈ culprits} committed_hand[c]
others   = { s ∉ culprits : committed_hand[s] > 0 }        // includes blinded-off absent seats
base     = Σ_{s ∈ others} committed_hand[s]

if others is empty:
    every culprit gets committed_hand back                  // nothing was ever contested
else:
    for s ∈ others:  share[s]  = forfeit * committed_hand[s] / base      // integer division
    remainder = forfeit - Σ share[s]
    distribute the remainder one chip each, clockwise from succ(button_pos), over `others`
    for s ∈ others:  stack[s] += committed_hand[s] + share[s]
    for c ∈ culprits: stack[c] += 0
```

Chips are conserved exactly: `Σ returns = base + forfeit = Σ committed_hand` (invariant I2, I28).
The odd-chip tie-break reuses the A8 rule so there is one convention in the codebase.

Its cost, recorded honestly rather than solved: an opponent able to knock a player offline right
after a big bet would collect that bet. D-005 classifies this as an out-of-protocol attack that
already sits outside the threat model, requiring real capability against the victim's connection,
and prefers it to the free in-protocol rage-quit escape. `THREAT_MODEL.md` must carry both.

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
silent.** It cannot steal cards or chips by doing so — D-005's forfeiture rule ensures quitting
costs at least what folding would have cost — but it is a griefing vector, and the only mitigations
are social (a visible count of attributable aborts per identity), not cryptographic. In play money
that visible count is the whole penalty, and `SPEC_CS.md` §18 forbids claiming more.

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

Evaluated at T47, in this order:

1. **Tournament won** — exactly one seat has `stack > 0`. Append the remaining seats to
   `finish_order`, set `settlement.tournament_winner`, → `TableClosed`.
2. **Nobody willing** — `|{s : status == Active ∧ stack > 0}| == 0` → `Paused`.
3. **One willing, others with chips** — the drain case of §5.3 step 9 applies and the hand is
   played as an uncontested blind steal.
4. Otherwise → `AwaitingKeySetup` for hand `hand_id + 1`.

No timeout, disconnect or abort reaches condition 1 (D-006 §5). A tournament ends only when one
player holds every chip, or when every seat has left (which empties condition 2 into
`TableClosed` after the last `PlayerLeft`).

### 9.4 Cash mode and custom tables

`Mode::CashPlayMoney` differs in exactly three ways, all confined to T47 and hand init:

* the blind level never advances — `small_blind`/`big_blind` are the config values for every hand;
* condition 1 of §9.3 does not exist; the table runs until every seat leaves;
* a seat may be added or removed at a **hand boundary** only. `PlayerSeated` and `PlayerLeft`
  arriving mid-hand are recorded and applied at the next T47, never mid-hand.

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
```

A config that fails goes to `TableClosed` and is never played. There is no repair path — repairing
a signed advertisement locally would mean two peers playing different games.

### 9.5 `SPEC_CS.md` §32 — heads-up first

All 19 phases and 47 transitions are reachable with `seats = 2` **except** the multi-pot form of
`build_pots` (§7.5) and the odd-chip distribution over more than two winners (§7.6), both of which
require three distinct commitment levels or three tied winners. The heads-up-specific rules —
button-is-small-blind, inverted post-flop order, TDA 34-B button adjustment — are exercised *only*
heads-up, so both branches need explicit tests from the start (A1.2).

The 3–6 player extension adds no new phase and no new transition. It changes only the guards that
count seats. That is the concrete sense in which this machine "does not make the multi-player
extension impossible" (§32).

---

## 10. Invariants for the property tests

`SPEC_CS.md` §26 requires `proptest`/`quickcheck` invariants. Each is a predicate over
`TableState` (and, where noted, over a `(state, event, state')` triple), asserted after **every**
transition including rejected ones. **26 invariants.**

The nine §26 named invariants map to I1, I10, I9, I15, I6, I8, I7, I12 and I21 respectively; the
eight of `POKER_RULES.md` A0 map to I1, I6, I10, I9, I4, I8, I7 and I14.

| # | Invariant | Predicate |
|---|---|---|
| **I1** | Global chip conservation | `Σ_s stack[s] + Σ_s committed_hand[s] == total_chips`, where `total_chips = players_at_start × start_stack`, constant for the table's whole life |
| **I2** | Step-local conservation | for every accepted event, total chips before == total chips after |
| **I3** | No underflow | every `Chips` arithmetic uses checked or saturating operations; `stack`, `committed_*` are never decremented below 0; a `u64` wrap is a test failure, not a wrap |
| **I4** | Commitment ordering | `∀s: committed_round[s] ≤ committed_hand[s] ≤ start_stack_this_hand[s]` |
| **I5** | Stack accounting | `∀s: stack[s] + committed_hand[s] == start_stack_this_hand[s]` throughout a hand, for every seat that posted anything |
| **I6** | Pot exactness | `Σ pots().size + Σ pots().refunds == Σ_s committed_hand[s]`, exactly, at every point |
| **I7** | Pot eligibility | `∀ pot: eligible ⊆ contributors ∧ eligible ∩ folded == ∅ ∧ eligible ≠ ∅` |
| **I8** | No folded winner | `settlement.award[s] > 0 ⇒ ¬folded[s]` |
| **I9** | Board shape | `|board| ≤ 5` and `|board| == {PreFlop:0, Flop:3, Turn:4, River:5}[street]` whenever `phase` is a betting phase |
| **I10** | Card uniqueness | the multiset `board ∪ ⋃_s revealed_hole[s]` contains no duplicate, and every element is in `0..=51` |
| **I11** | Index discipline | `deck.opened.keys() ⊆ deal_map.all_indices()`, and each index appears at most once |
| **I12** | Street monotonicity | `street` never decreases, and advances only `PreFlop→Flop→Turn→River`; no street is skipped |
| **I13** | Phase discipline | every `(phase, phase')` pair appears in the §5.2 table; there is no other edge |
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

Three of these deserve dedicated adversarial tests rather than random generation, because a
`proptest` generator that only produces legal actions will never reach them:

* **I21** against `CheaterIllegalRaise`, `CheaterReplayAction`, `CheaterFakeStack`,
  `CheaterEquivocation` (`SPEC_CS.md` §25);
* **I23** against `CheaterFutureBoard` — a reveal token for a turn index published during the flop
  betting round must be rejected by T25 and leave `deck.opened` untouched;
* **I20** against a `CheaterDisconnect` who leaves mid-hand and whose seat must be shown to receive
  no index and no token in the following hand.

---

## 11. Open questions carried forward

None of these is resolved here. Each is recorded so it cannot be lost.

| # | Question | Owner document |
|---|---|---|
| Q1 | **Mucking at showdown.** Mandatory universal reveal, TDA-faithful muck with signed forfeiture, or delayed reveal at end of tournament. Each changes the cryptographic protocol, not just the engine. Carried unresolved from `POKER_RULES.md` A8. `config.showdown_policy` keeps both branches alive; the MVP's use of `MandatoryReveal` is implementation order, not a decision. | `PROTOCOL.md` / `DECISIONS.md` |
| Q2 | **The envelope of an unsigned derived event** (`HAND_COMPLETE`, `HAND_INIT`, `Settle`). What occupies `sender_public_key`, and whether peers counter-sign so the transcript carries attributable agreement rather than relying on the next `STATE_HASH` comparison. | `PROTOCOL.md` |
| Q3 | **Hand-deadline certificate signers when several seats are simultaneously unresponsive.** `participants \ {subject}` is unachievable with two seats gone; a certificate naming a *set* of subjects is defensible but interacts with the §8.6 attribution and forfeiture split. | `PROTOCOL.md` |
| Q4 | **Whether `STATE_HASH` is exchanged at every phase boundary or only at hand boundaries.** §15 says "after critical transitions" without enumerating them. Every boundary is the strongest option and costs `n²` small messages per hand; hand boundaries are cheapest and detect divergence latest. This document makes every phase a hashable state so either policy is implementable. | `PROTOCOL.md` |
| Q5 | **Rebuys, add-ons and late registration.** Out of scope for the MVP and absent from `RATED_SNG_POKERTH_V1`, but they would change §5.3 and §9.3 and should be designed for rather than retrofitted. | `DECISIONS.md` |

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
  (`t`-of-`n`) is refused because it is strictly worse.
* An absent seat cannot win the blind it posts. This is a **deliberate, documented deviation from
  TDA rules** (D-005), forced by the fact that opening an absent player's cards at showdown would
  require exactly the mechanism `SPEC_CS.md` §19 forbids. The practical difference is small;
  the deviation is real and is stated rather than hidden.
* The five open questions in §11 are open. Anything downstream that assumes an answer to one of
  them is assuming something this document did not say.
