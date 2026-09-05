//! One hand, as a state machine with no network in it.
//!
//! Bytes in, bytes out, and no `async` anywhere — the same shape as
//! [`net::formation`](crate::net::formation), and for the same reason: a state
//! machine that owns a socket can only be tested by standing up a socket, and
//! the defect that matters here is *two peers disagreeing*, which needs two
//! copies of the state and no network at all.
//!
//! # What this does today
//!
//! Stage 0 of hand `k`: `HAND_INIT`, a **collective** stage. Every seat in `R`
//! derives the same twelve fields, signs them, and sends them; every seat
//! recomputes what the others sent and compares field by field; the stage
//! completes when every seat in `R` has been heard, and its hash becomes the
//! parent of stage 1.
//!
//! `HAND_INIT` announces nothing and decides nothing — every field is a pure
//! function of the state before it (`PROTOCOL.md` §4.3), which is exactly why
//! nobody has a veto over the hand starting.
//!
//! # What it does not do yet, said plainly
//!
//! * The **RNG beacon** (`RNG_COMMIT` / `RNG_REVEAL`, §7.9) that decides the
//!   seat permutation and the initial button. Until it exists,
//!   [`provisional_button`] stands in: deterministic, agreed by everybody,
//!   and **not** the normative rule. It is one function so the beacon replaces
//!   it in one place.
//! * Everything after stage 0: the deck, the shuffle chain, the betting. The
//!   slot for stage 1 is computed and handed back, which is where that work
//!   starts.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use ed25519_dalek::SigningKey;

use crate::net::chained::{self, Slot};
use crate::net::joinwire::WireError;
use crate::poker::state::{Hash, SeatIdx};
use crate::protocol::messages::EventType;

use crate::mental_poker::backend::{DeckParams, HandDeck, HandSecret, VerifiedKey,
    WireKey, WireKeyProof, WireToken, WireTokenProof, CIPHERTEXT, DECK};
use crate::mental_poker::deck::{CardIndex, DeckIndexMap};
use crate::mental_poker::protocol::{Ciphertext, CtxFields, DeckCtx, DeckWire, Final,
    ProofPosition, Verified};
use crate::mental_poker::shuffle::{ChainParams, ShuffleChain, StepError};
use crate::mental_poker::protocol::{DeckCrypto, VerifyOutcome};
use crate::poker::actions::{Action, BettingRound, Illegal, LegalActions};
use crate::poker::engine::{
    advance_positions, betting_is_closed, first_to_act, initial_positions, next_to_act,
    only_one_live, post_blinds, round_complete, Positions,
};
use crate::poker::evaluator::{evaluate_holdem, HandRank};
use crate::poker::pots::{award, build_pots};
use crate::poker::state::{Card, Chips, Street};
use crate::protocol::serialization::h;
use crate::protocol::signatures::Domain;
use crate::protocol::transcript::stage_hash_single;

use crate::mental_poker::reveal::RevealStage;

use super::dealing::{self, Dealing, Identity, Refused, Share};
use super::handwire::{street_code, street_from_code, ActionAmount, ActionHead, BoardReveal,
    DealPrivate, DeckCommit, DeckInit, HandAbort, HandComplete, HandInit, NotOurs, PotAward,
    Refund, RevealEntry, ShowdownMuck, ShowdownReveal, ShuffleProof, ShuffleStep, TimeoutCert,
    TimeoutVote};
use super::stage::{Collective, Heard};

/// What a step wants sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Send {
    /// To every seat of the table.
    Broadcast(Vec<u8>),
}

/// Why a step did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failed {
    /// The bytes are not a well-formed chained event for the slot expected.
    Wire(WireError),
    /// The sender is not on this table's roster.
    NotAtThisTable,
    /// The sender is on the roster but may not speak in this stage.
    NotInThisStage,
    /// The body disagrees with what this client derived, and which field.
    Disagrees { seat: SeatIdx, what: NotOurs },
    /// One seat, two different bodies, one stage.
    Equivocation { seat: SeatIdx },
    /// For a stage this client has not reached. Held, not refused.
    NotYet,
    /// A key or its proof did not survive the boundary, or the proof does not
    /// hold. Attributable: the seat that sent it is named.
    BadKey { seat: SeatIdx, why: &'static str },
    /// The hand has gone as far as this client can take it.
    NothingFurther,
    /// A seat shuffled when it was not its turn, or shuffled twice.
    OutOfTurn { seat: SeatIdx, expected: Option<SeatIdx> },
    /// A deck or a proof did not survive the boundary, or names a deck this
    /// client does not hold. Attributable: the seat that sent it is named.
    BadDeck { seat: SeatIdx, why: &'static str },
    /// A shuffle argument did not verify.
    ///
    /// Separate from [`BadDeck`](Failed::BadDeck) because the consequence is
    /// different: the chain can never complete, so the hand is abandoned rather
    /// than shortened (`ZIFFLE_VERDICT.md` C-6 rule 4). The chain is never
    /// re-formed without the refused shuffler; a chain that could be shortened
    /// is a chain an attacker can shorten to one honest shuffler.
    BadShuffle { seat: SeatIdx, why: &'static str },
    /// Two peers hold different decks. Caught by `DECK_COMMIT`, which is the
    /// last moment at which catching it is cheap.
    DeckDisagrees { seat: SeatIdx, what: &'static str },
    /// A reveal contribution was not what the stage required, or its proof did
    /// not verify against the committed deck.
    BadToken { seat: SeatIdx, why: &'static str },
    /// A reveal share's proof is **provably** wrong against the committed deck.
    ///
    /// Split out of [`BadToken`](Failed::BadToken) because it is the one reveal
    /// failure that is evidence rather than a statement about this receiver:
    /// `Refused::DidNotVerify(VerifyOutcome::Invalid(_))` and nothing else.
    /// `NotDue`, `UnknownSeat` and `CouldNotVerify` all stay `BadToken`,
    /// because a receiver that is behind, or that cannot run the check, has
    /// found nothing about the sender.
    ///
    /// It never leaves `on_event`: it is caught there and turned into a
    /// `HAND_ABORT cause = 3` carrying the offending frame. A variant rather
    /// than a string comparison on `BadToken`'s `why`, because control flow
    /// keyed on prose is control flow that a copy-edit breaks.
    RevealDisproved { seat: SeatIdx },
    /// A seat offered an action the rules do not allow.
    ///
    /// Attributable and never corrected: `PROTOCOL.md` §4.7 is explicit that an
    /// illegal action *"is not a state transition, and it never becomes one"*.
    /// The receiver does not clamp a raise to the minimum or turn a check into
    /// a call; it refuses the message and names who signed it.
    Illegal { seat: SeatIdx, what: Illegal },
    /// The action names a street or a position in the hand that is not the one
    /// this client's engine is at.
    ///
    /// Separate from an illegal action because it is a different accusation: the
    /// action might be perfectly legal somewhere else in the hand, and what is
    /// wrong is *where the sender thinks it is*.
    Elsewhere { seat: SeatIdx, what: &'static str },
    /// Every share verified and the product is still not a card.
    ///
    /// Not attributable to anybody: it means the argument's soundness failed,
    /// so there is nothing to retry and nobody to blame (`ZIFFLE_VERDICT.md`
    /// C-10). It is carried separately from every other failure for exactly
    /// that reason - a client that reported it as a peer's fault would be
    /// naming somebody who did nothing.
    Unsound { index: u8, what: &'static str },
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wire(e) => write!(f, "{e:?}"),
            Self::NotAtThisTable => f.write_str("that is not a seat at this table"),
            Self::NotInThisStage => f.write_str("that seat may not speak in this stage"),
            Self::Disagrees { seat, what } => write!(f, "seat {seat}: {what}"),
            Self::Equivocation { seat } => {
                write!(f, "seat {seat} sent two different copies of one stage")
            }
            Self::NotYet => f.write_str("that belongs to a stage this client has not reached"),
            Self::BadKey { seat, why } => write!(f, "seat {seat}'s deck key: {why}"),
            Self::NothingFurther => f.write_str("this hand has gone as far as it can"),
            Self::OutOfTurn { seat, expected } => match expected {
                Some(e) => write!(f, "seat {seat} shuffled out of turn; seat {e} is up"),
                None => write!(f, "seat {seat} shuffled after the chain closed"),
            },
            Self::BadDeck { seat, why } => write!(f, "seat {seat}'s deck: {why}"),
            Self::BadShuffle { seat, why } => write!(f, "seat {seat}'s shuffle: {why}"),
            Self::DeckDisagrees { seat, what } => {
                write!(f, "seat {seat} holds a different {what}")
            }
            Self::BadToken { seat, why } => write!(f, "seat {seat}'s reveal: {why}"),
            Self::RevealDisproved { seat } => write!(
                f,
                "seat {seat}'s reveal proof does not hold against the committed deck"
            ),
            Self::Illegal { seat, what } => write!(f, "seat {seat} cannot do that: {what:?}"),
            Self::Elsewhere { seat, what } => {
                write!(f, "seat {seat} is acting as though {what}")
            }
            Self::Unsound { index, what } => {
                write!(f, "card {index} could not be opened: {what}")
            }
        }
    }
}

/// Everything a hand needs that comes from outside it.
///
/// Passed as one struct rather than nine arguments, and every field is
/// something the table already agreed on before the hand began.
#[derive(Debug, Clone)]
pub struct Opening {
    pub table_id: Hash,
    pub hand_id: u64,
    pub session_id: Hash,
    pub roster_hash: Hash,
    /// `GENESIS(k)`, the parent of stage 0.
    pub genesis: Hash,
    /// The required emitter set: for hand 1, the signers of `TABLE_READY`.
    pub required: Vec<SeatIdx>,
    /// §4.9's readmission set `A`, and it widens the **accepted** emitter set
    /// of stage 0 alone.
    ///
    /// **This is the door back for a seat that missed a hand**, and until it
    /// existed there was none. A seat outside `P(k)` is not dealt into `k+1`
    /// (§4.4, `dealt_in ⊆ R(HAND_INIT, k+1) = P(k)`), and it cannot enter
    /// `P(k+1)` without signing a chained event of hand `k+1`, which it cannot
    /// do while it is not dealt in. Measured: a client that reconnected,
    /// rejoined the table's group and never played another hand — `S1-O`.
    ///
    /// A seat gets in here by **signing**: a checkpoint-8 `STATE_HASH` of a
    /// completed hand that **agrees** with this receiver's own value, or a
    /// `PLAYER_SIT_IN` in that hand's boundary window. Signing requires being
    /// alive, which is the entire test — no certificate, no vote, no quorum,
    /// no third party.
    ///
    /// **Accepted and not required, which is `P2`'s disposition.** The write is
    /// reachable by replay, so a read that enlarged a *required* set would let
    /// one stale agreeing copy stall stage 0 to the hand deadline once per hand
    /// for every hand `§5.3` retains. Widening the accepted set costs a map
    /// lookup and completes nothing on its own. A seat readmitted this way is
    /// **not** dealt in at `k+1`; it is counted into `P(k+1)` by its own
    /// `HAND_INIT`, and is required and dealable at `k+2` — one hand later,
    /// which is what D-013's promise costs when the news arrives late.
    pub readmitted: Vec<SeatIdx>,
    /// Seat, public key and starting stack, ascending by seat.
    pub seats: Vec<(SeatIdx, [u8; 32], u64)>,
    pub max_players: u8,
    pub small_blind: u64,
    pub big_blind: u64,
    pub level: u16,
    /// The blind schedule, carried so that hand `k+1` can **derive** its level
    /// and its blinds instead of copying hand `k`'s.
    ///
    /// `PROTOCOL.md` §7.2 `BlindSchedule`, and all three are parts of
    /// `table_params_hash` (§3.1) — so they are the table's, signed, and not a
    /// client's setting. Without them in the `Opening` nothing at the hand
    /// boundary could compute the next level, which is why the blinds stood
    /// still: `level` was set to 1 at formation and copied forward for ever,
    /// and `RATED_SNG_POKERTH_V1`'s doubling never happened in play.
    pub every_n_hands: u16,
    pub first_small_blind: u64,
    pub small_blind_cap: u64,
    pub my_seat: SeatIdx,
    /// How long a peer has to answer a cryptographic step, from the table's own
    /// parameters. Carried on the envelope of every stage this hand emits.
    pub crypto_step_timeout_ms: u32,
    /// Each seat's reconnection allowance, in **hands**, indexed by seat.
    ///
    /// See [`GRACE_HANDS`]. Carried from hand to hand rather than recomputed,
    /// because it is a fold over every hand played so far and a peer that
    /// joined late has no way to replay it — which is also why a seat that
    /// arrives after the table is seated cannot exist (`P8`).
    pub grace: Vec<u8>,
    /// How many consecutive hands each seat has been present for, towards
    /// [`REPLENISH_AFTER`].
    pub present_run: Vec<u8>,
    /// How long the whole hand may take before any peer may end it.
    ///
    /// `PROTOCOL.md` §8: the only terminus a stalled **cryptographic** stage
    /// has in version 1. Every seat has the same number, from the table's
    /// advertisement, and each measures it on its own clock.
    pub hand_deadline_ms: u32,
    /// The allowance beyond `action_timeout_ms` before a deadline has passed.
    ///
    /// `PROTOCOL.md` §8.2: it *"must absorb the P2P round trip, relay hops for
    /// CGNAT peers, and signature verification"*, and **every peer must use the
    /// identical constant, because peers disagreeing about whether a timeout
    /// fired is a consensus fault, not a UX detail.** So it is a table
    /// parameter and never a client's choice.
    ///
    /// The player sees `action_timeout_ms`; the deadline is the sum.
    pub action_grace_ms: u32,
    /// The pause between a hand ending and the next one being dealt, from the
    /// table's advertisement. `PROTOCOL.md` §8.2 puts it in the deadline a
    /// terminal event arms; D-020's five seconds is this client's own hold on
    /// top and is not the same number.
    pub hand_delay_ms: u32,
    /// The per-hand thinking reserve every seat may spend on top of
    /// `action_timeout_ms`. A table parameter inside `table_params_hash`, so
    /// every peer derives the same betting deadline from it.
    pub time_bank_ms: u32,
    /// How long a seat has to act before its own client acts for it.
    ///
    /// From the table's advertisement, so every seat has the same number. It is
    /// **not** enforced by anybody else: version 1 has no `TIMEOUT_VOTE` and no
    /// `TIMEOUT_CERT` (D-015), so a seat that goes quiet in a betting stage is
    /// answered by its own client folding for it, and by nothing else.
    pub action_timeout_ms: u32,
    /// Where the button sits, when a previous hand decided it.
    ///
    /// `None` only for the **first** hand of a table, where nothing has decided
    /// it yet and `provisional_button` stands in until the RNG beacon of §7.9
    /// exists. From hand two onwards it is the dead-button rotation's answer,
    /// and re-rolling it from `session_id` would move the button backwards.
    pub button: Option<SeatIdx>,
}

impl Opening {
    /// Everything hand `k` needs, read off a settled formation.
    ///
    /// `None` until the roster has ratified, because three of these fields do
    /// not exist before then — the session, the genesis that hangs off the
    /// `TABLE_READY` stage hash, and the required emitter set, which §3.2 says
    /// **is** the signers of `TABLE_READY` for the first hand.
    pub fn from_formation(f: &crate::net::formation::Formation, hand_id: u64) -> Option<Self> {
        let ad = f.advert();
        Some(Opening {
            table_id: f.table_id(),
            hand_id,
            session_id: f.session()?,
            roster_hash: f.roster().hash_at_zero(),
            genesis: f.genesis_one()?,
            required: f.ratifiers(),
            // Hand one has no readmission set: nobody has missed a hand yet,
            // and `A` is written only by a stale event of a **completed** hand.
            readmitted: Vec::new(),
            seats: f
                .roster()
                .seats()
                .iter()
                .map(|e| (e.seat, e.app_public_key, e.buyin))
                .collect(),
            max_players: ad.max_players,
            small_blind: ad.small_blind,
            big_blind: ad.big_blind,
            // Level 1: the blind schedule advances from `HAND_COMPLETE`, and no
            // hand has completed. §7.2 rule 2 also forces
            // `small_blind == blind_schedule.first_small_blind`, so hand one's
            // blinds and level one's are the same numbers by construction.
            level: 1,
            every_n_hands: ad.blind_schedule.every_n_hands,
            first_small_blind: ad.blind_schedule.first_small_blind,
            small_blind_cap: ad.blind_schedule.small_blind_cap,
            my_seat: f.my_seat()?,
            crypto_step_timeout_ms: ad.crypto_step_timeout_ms,
            action_timeout_ms: ad.action_timeout_ms,
            action_grace_ms: ad.action_grace_ms,
            hand_delay_ms: ad.hand_delay_ms,
            time_bank_ms: ad.time_bank_ms,
            hand_deadline_ms: ad.hand_deadline_ms,
            // Everybody starts whole. A table that has played no hands has
            // nobody who has missed one.
            grace: vec![GRACE_HANDS; usize::from(ad.max_players)],
            present_run: vec![0; usize::from(ad.max_players)],
            // The first hand of a table: nothing has decided the button yet.
            button: None,
        })
    }

    /// Why `from_formation` would decline, or `None` if it would not.
    ///
    /// `from_formation` has four `?`s and no voice. It is the **only** road into
    /// a hand — every peer derives its own opening rather than taking one from
    /// somebody's `HAND_INIT` — so a silent `None` is a client that sits at a
    /// table for ever, holding a roster, with no line in its log saying what it
    /// is missing. Measured: a rejoining client did exactly that for 275 s.
    ///
    /// Kept beside the constructor and not inside it, so that the constructor
    /// stays an expression and the two cannot drift: each arm names the field
    /// whose `?` it stands for.
    pub fn why_not_from_formation(f: &crate::net::formation::Formation) -> Option<&'static str> {
        if f.session().is_none() {
            return Some("the table has no session id: its roster has not ratified here");
        }
        if f.genesis_one().is_none() {
            return Some("the table has no genesis: its roster has not ratified here");
        }
        if f.my_seat().is_none() {
            return Some("this client holds no seat in the roster it has");
        }
        None
    }
}

/// Return the state hash, or a **wrong** one, if this build and this run were
/// both asked for a divergence.
///
/// `P2P_POKER_DIVERGE_AT_HAND=<k>` makes this peer's checkpoint-8 value for hand
/// `k` differ from everybody else's by one bit. That is the only fault §6.3
/// exists to answer and the only one an honest run never produces, so without a
/// way to cause it deliberately the freeze and the reconciliation rounds would
/// first execute in front of a player.
///
/// **Two locks, and the outer one is a compile-time absence.** A build without
/// `--features fault-harness` contains no path to a wrong hash — this
/// function is the identity and the environment variable is not read. That is
/// what makes it safe to have at all.
#[cfg(feature = "fault-harness")]
fn diverge_if_asked(state_hash: Hash, hand_id: u64) -> Hash {
    let Ok(at) = std::env::var("P2P_POKER_DIVERGE_AT_HAND") else {
        return state_hash;
    };
    if at.trim().parse::<u64>() != Ok(hand_id) {
        return state_hash;
    }
    let mut wrong = state_hash;
    wrong[0] ^= 1;
    wrong
}

/// The identity, in every build that did not ask for the harness.
#[cfg(not(feature = "fault-harness"))]
fn diverge_if_asked(state_hash: Hash, _hand_id: u64) -> Hash {
    state_hash
}

/// How much of an event body this client will decode.
///
/// `PROTOCOL.md` §9.3's cap for `HAND_INIT`. Nested: the frame holds an
/// envelope which holds the body, and each has its own bound, because a decoder
/// with no bound is a decoder whose working set an attacker chooses.
pub const HAND_INIT_CAP: usize = 512;
/// The largest chained frame this hand will open.
///
/// Sized by `SHUFFLE_PROOF`, which is the biggest thing a hand sends: 5547 B of
/// Bayer-Groth argument, two hashes and an envelope. Sixteen kilobytes leaves
/// room for the 8192 B the protocol allows a proof and is still small enough
/// that a peer cannot make this client hold much by sending nonsense.
pub const FRAME_CAP: usize = 16_384;

/// The frame cap for a `STATE_HASH`.
///
/// Three fields — a `u16` and two thirty-two-byte hashes — so this is generous
/// by an order of magnitude and is a bound rather than a size. Every cap in
/// this file is one: `SPEC_CS.md` §17 wants a limit on everything that arrives
/// from the network, and a body this small still arrives from the network.
const STATE_HASH_CAP: usize = 512;

/// The cap for [`chained::peek`], which runs **before** the type is known.
///
/// `peek` cannot use a per-type cap, because reading the type is what it is
/// for. So it takes the largest chained frame any type may have, which is
/// [`HAND_ABORT_CAP`] — an abort carrying two embedded events is several times
/// [`FRAME_CAP`], and peeking it under `FRAME_CAP` would report it as malformed
/// and reject the one message that ends a hand nobody else can end.
///
/// **What this gives an attacker, stated rather than left to be discovered:**
/// one CBOR decode of up to this many bytes instead of up to `FRAME_CAP`, for
/// any frame on the table topic. It is bounded above by `GOSSIP_MAX_TRANSMIT`,
/// which libp2p has already read and buffered before this code sees it, so the
/// marginal cost is a decode of bytes that are already in memory. Nothing is
/// retained: a frame that peeks as the wrong type never reaches an `open`, and
/// every `open` still uses its own type's cap.
const PEEK_CAP: usize = HAND_ABORT_CAP;
const _: () = assert!(PEEK_CAP >= FRAME_CAP);

/// Where the button sits, until the RNG beacon exists to decide it.
///
/// **Provisional and named as such.** `STATE_MACHINE.md` T10 gives this to the
/// beacon of §7.9 — the seat permutation and the initial button are derived from
/// a commit-and-reveal that no code in this project performs yet. Deriving it
/// from `session_id` instead is deterministic and agreed by every peer, which is
/// all stage 0 needs to complete, and it is **not** the rule: a beacon exists so
/// that no single seat's contribution decides the button, and `session_id` is a
/// hash of the ratifications, which is not the same guarantee.
///
/// # And it is biasable by whoever ratifies last, which is worse than provisional
///
/// `session_id` is a hash over the ratifications' `event_hash`es (§4.3), and an
/// event hash covers the whole signed envelope — including `emitted_at_unix_ms`,
/// a number its emitter picks. `TABLE_READY` also carries `capability_set`, a
/// list of byte strings the emitter controls outright. So the last seat to
/// ratify can re-sign its own with a different timestamp, recompute
/// `session_id`, and stop when the button lands where it wants.
///
/// `the_last_seat_to_ratify_can_choose_the_button` measures it: **under a
/// hundred hashes** to choose any seat at a six-handed table. The dead-button
/// rule makes that worth doing — the initial button fixes who posts which blind
/// in hand one and who acts last, and every later button is a rotation of it.
///
/// This is exactly what §4.4's commit-and-reveal beacon exists to stop, and the
/// reason this function is documented as *not the rule* rather than as a
/// simplification. **What blocks replacing it is not the beacon**: `RNG_COMMIT`,
/// `RNG_REVEAL` and `seed` are fully specified. It is that no document says how
/// to get a button out of the seed — `PROTOCOL.md` §4.4 says the rule is in
/// `STATE_MACHINE.md`, §7.9 says the constructions are `PROTOCOL.md`'s and that
/// the engine computes neither value, and T10 points at §7.9. A citation cycle
/// with nothing at the centre.
///
/// One function, so replacing it is one change.
pub fn provisional_button(session_id: &Hash, occupied: &[SeatIdx]) -> SeatIdx {
    debug_assert!(!occupied.is_empty());
    let pick = u64::from_be_bytes([
        session_id[0],
        session_id[1],
        session_id[2],
        session_id[3],
        session_id[4],
        session_id[5],
        session_id[6],
        session_id[7],
    ]);
    occupied[(pick % occupied.len() as u64) as usize]
}

/// The cap on a `DECK_INIT` body: a key and a proof, and nothing else.
pub const DECK_INIT_CAP: usize = 256;

/// The cap on a `SHUFFLE_STEP` body: a round byte and 3432 B of deck.
pub const SHUFFLE_STEP_CAP: usize = 3_500;

/// The cap on a `SHUFFLE_PROOF` body: a round byte, two hashes and the
/// argument, which `PROTOCOL.md` §4.5 caps at 8192 B.
pub const SHUFFLE_PROOF_CAP: usize = 8_320;

/// The cap on a `DECK_COMMIT` body: two hashes and a point.
pub const DECK_COMMIT_CAP: usize = 160;

/// The cap on a `DEAL_PRIVATE` body.
///
/// `PROTOCOL.md` §9 caps a reveal message at 25 entries, and an entry is an
/// index, a 33 B token and a 98 B proof. Four kilobytes is that with room for
/// the encoding, and it is the size a peer can make this client hold by
/// sending nonsense - which is why it is stated as a number here rather than
/// derived from the seat count of whatever table happens to be running.
pub const DEAL_PRIVATE_CAP: usize = 4_096;

/// The cap on any betting action: three small numbers, and at most one more.
pub const ACTION_CAP: usize = 64;

/// The cap on a `BOARD_REVEAL` body: a street code and at most three entries.
pub const BOARD_REVEAL_CAP: usize = 1_024;

/// The cap on a `SHOWDOWN_REVEAL` body: exactly two entries.
pub const SHOWDOWN_REVEAL_CAP: usize = 512;

/// The cap on a `SHOWDOWN_MUCK` body: one boolean.
pub const SHOWDOWN_MUCK_CAP: usize = 64;

/// The cap on a `HAND_COMPLETE` body: at most `MAX_SEATS` pots, each with three
/// seat lists, plus four vectors of that length.
pub const HAND_COMPLETE_CAP: usize = 4_096;

/// The group base of the betting actions, and what a vote names when the
/// stage it is about is a betting one.
///
/// Five types are legal at one betting `sequence` and which one the seat would
/// have chosen is exactly what nobody knows, because it never spoke. The group
/// base names the group and commits to no member. `PROTOCOL.md` §4.8 pins it,
/// because the value is inside `subject_digest`: two clients that each picked a
/// reasonable member would produce different digests, their votes would land in
/// different slots, and **no certificate would ever assemble**.
pub const ACTION_GROUP: u16 = 0x0500;

/// What a verified certificate is remembered as.
///
/// The hash alone was not enough: an abort names a subject, and a receiver
/// checking that name against a bare set of hashes could only ask *"is this a
/// certificate?"* and never *"is it a certificate about the seat you are
/// naming?"*.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CertFact {
    pub subject_seat: SeatIdx,
    pub kind: u16,
}

/// What became of an event offered for holding.
///
/// The three answers exist because they need three different things said to
/// the mesh: a kept event was worth forwarding, an event of another hand is
/// somebody else's business and costs its sender nothing, and a malformed one
/// is the sender's fault and must not be forwarded in this client's name.
/// Whether this client signs its own `HAND_INIT` when a hand opens.
///
/// `Speak` is every ordinary hand. `Quiet` opens the hand with this client's
/// own copy heard but **not sent**: the parent was contested before the open
/// — two seats had already signed this hand at another genesis — and a
/// signature at a parent the table may not hold is withheld until either a
/// seat is counted at this genesis (`speak`) or a certificate re-derives the
/// hand. `Muted` never sends: this client already signed this hand at a
/// genesis the table does not hold, and a second `HAND_INIT` from one seat at
/// one slot is the equivocation §5.2.3 forbids of an honest peer, so the
/// corrected hand is followed silently and the seat rejoins at the next one
/// (`S1-BS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    Speak,
    Quiet,
    Muted,
}

/// How many roster seats must name one foreign genesis at sequence 0 of this
/// hand before this client says so: the same floor `adrift_now` uses for
/// *a hand ahead*, and for the same reason — one seat's word is one seat's
/// word.
pub const FOREIGN_GENESIS_FLOOR: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holding {
    Kept,
    /// Verified, and of a hand this client is not playing.
    ///
    /// **The two values are carried because they are evidence.** A signed,
    /// well-formed chained event of *this table* naming a hand this client has
    /// not reached is another seat's signature on the proposition that the
    /// table has moved past it. One such seat could be mistaken; several
    /// agreeing cannot all be, and that is what tells a client that fell far
    /// enough behind that it is dealing a game of its own.
    ///
    /// `seat` is `None` when the sender holds no seat in this hand's roster,
    /// which is not evidence of anything and is counted as nothing.
    AnotherHand {
        hand_id: u64,
        seat: Option<SeatIdx>,
    },
    Malformed,
}

/// A settlement being collected while this client is in `Phase::Aborted`.
///
/// The aborted peer never computed a settlement of its own, so it cannot
/// compare each body against one it derived. It compares the emitters against
/// **each other** instead: the first body is the claim and every later one must
/// equal it, which is the same disagreement test from the other side.
#[derive(Clone, Debug)]
struct Late {
    stage: Collective,
    body: HandComplete,
    /// Set once every seat of the required set has published the same body.
    closed: Option<(Hash, Vec<Chips>)>,
}

/// A `TIMEOUT_CERT` checked from its own bytes, with nothing taken on trust.
#[derive(Clone, Debug)]
struct VerifiedCert {
    event_hash: Hash,
    emitter: SeatIdx,
    subject: TimeoutVote,
    voters: BTreeSet<SeatIdx>,
    raw: Vec<u8>,
}

/// The most subject digests one hand may bank. Four per seat is well past
/// `MAX_CONSECUTIVE_AUTO_ACTIONS` and bounds what a flood can cost.
const BANKED_CAP: usize = crate::protocol::constants::MAX_SEATS as usize * 4;

/// The cap on a `TIMEOUT_VOTE` body: six small numbers and a hash.
pub const TIMEOUT_VOTE_CAP: usize = 128;

/// The cap on a `TIMEOUT_CERT` body: a digest and up to `MAX_SEATS - 1`
/// embedded signed votes.
///
/// **It has to actually hold them, and at 4 096 it did not.** A sealed
/// `TIMEOUT_VOTE` is 266 B, `MAX_SEATS - 1` of them are carried whole, and
/// `TimeoutCert::votes` is a plain `Vec<Vec<u8>>` — no `minicbor::bytes`, so
/// each vote encodes as a CBOR array of integers rather than a byte string and
/// very nearly doubles. Nine votes plus the digest come to **4 699 B**.
///
/// Nothing caught it because nothing ever built a large one:
/// `tests/timeout_certificate.rs` is a thousand lines and every case in it
/// opens three seats, where the voter set is two. The largest certificate this
/// repo had ever encoded carried two votes against a documented maximum of
/// nine.
///
/// What it cost, measured on `split163641-10`: hand #4 stalled with nine seats
/// at *"your turn"* and the tenth still at *"the deck is shuffled and sealed"*
/// — it had never learned it was on the clock. The nine ran their clocks out on
/// it and the tally climbed to **9/9 agree at 376.0 s**. Unanimous. And every
/// seat then logged `TooLong("the payload is over its cap")` once per attempt
/// for the remaining four minutes. The hand never ended. A full table could not
/// certify a timeout at all, so one seat missing one message froze the table
/// permanently — the worst failure this protocol has, because to every honest
/// seat it is indistinguishable from the table being over.
///
/// 8 192 is the next power of two above the measured worst case and leaves 74 %
/// headroom. `tests/timeout_certificate_at_a_full_table.rs` pins the arithmetic
/// at `MAX_SEATS - 1` so it cannot drift under again.
///
/// The encoding itself is left alone deliberately. Annotating `votes` with
/// `minicbor::bytes` would halve the certificate — worth having, because this
/// message is sent precisely when the network is already struggling and a
/// smaller body is fewer fragments to lose — but it changes the bytes on the
/// wire, hence the event hash, hence everything chained from it. That is the
/// owner's call, not a bug fix, and it is filed as one.
pub const TIMEOUT_CERT_CAP: usize = 8_192;

pub use crate::protocol::constants::MAX_CONSECUTIVE_AUTO_ACTIONS;

/// How many hands a seat may miss and still be dealt back in.
///
/// **Denominated in hands, and that is the whole design.** A reconnection
/// allowance measured in seconds needs a clock two peers share, and there is
/// none — `PROTOCOL.md` §8.2 puts every deadline on the peer's own monotonic
/// clock. An allowance that decides `dealt_in` and is measured on a clock
/// nobody shares is an allowance two peers disagree about, and a disagreement
/// about `dealt_in` is a different `HAND_INIT` at every seat: the chain forks
/// with nobody lying.
///
/// Hands are agreed by construction. `signed_this_hand` is `P(k)` (§3.2) and
/// the bank below is a pure function of the sequence of those sets from hand
/// one, so two peers that agree on every `signed_this_hand` agree on every
/// seat's bank.
///
/// **What is no longer true is the second half of that sentence.** It used to
/// end *"and the checkpoint that compares one compares the other"*, because
/// §6.1 field 28 hashed `signed_this_hand` into the state hash. Field 28 is
/// deleted: it was an observation of the listener rather than a fact about the
/// hand, so two honest peers on a lossy link had to differ in it, and requiring
/// them to agree removed honest players for it (`S1-AZ` … `S1-BD`).
///
/// **And the bank is not guarded by the settlement comparison — that sentence
/// stood here for one commit and was wrong.** No wire body carries `grace` or
/// the bank; both are local derivations, so `on_hand_complete` cannot compare
/// them. What guards them is one step earlier and is enough: the bank decides
/// `dealt_in` through `grace > 0`, `dealt_in` is `HAND_INIT`'s `n(8)`, and
/// `on_hand_init` compares the whole `HAND_INIT` body at stage 0 exactly as
/// `on_hand_complete` compares the whole settlement. So a bank two peers
/// disagree about surfaces as a `HAND_INIT` they disagree about, at the first
/// stage of the hand rather than the last.
///
/// The difference from the settlement case, and it is the whole point: a
/// `HAND_INIT` body contains no observation of the listener, so that comparison
/// is satisfiable. Field 28's was not.
///
/// Two hands is about a minute at this table's pace, which is the interval the
/// owner asked for, expressed in the one unit that cannot drift.
pub const GRACE_HANDS: u8 = 2;

/// How many hands a seat must be present for to earn one unit back.
pub const REPLENISH_AFTER: u8 = 15;

/// The cap on a `HAND_ABORT` body **and on its frame**, which for this one type
/// are the same number.
///
/// Sized for causes 2 and 3, which embed two `SignedEvent`s — the step whose
/// deck is disputed and the proof that fails on it. See
/// [`ABORT_EVIDENCE_MAX`](crate::protocol::constants::ABORT_EVIDENCE_MAX) for
/// why that bound is this client's own and smaller than `PROTOCOL.md` §4.10's:
/// two of §4.10's would not fit in a GossipSub frame, so an abort built to it
/// could never be sent.
///
/// It is used at both levels because `on_hand_abort` opens the frame with it
/// and then decodes the payload with it. Passing `FRAME_CAP` for the frame
/// and this for the payload would refuse every abort that needs the room, at
/// the outer decode, before the payload cap was ever consulted.
pub const HAND_ABORT_CAP: usize = crate::protocol::constants::HAND_ABORT_MAX;

/// How far this hand has got.
///
/// One variant per stage, each carrying only what that stage needs, so a stage
/// cannot read state that belongs to a later one. The `Slot` lives outside,
/// because advancing it is the same operation whatever the stage was.
enum Phase {
    /// Stage 0: `HAND_INIT`, collective.
    Init(Collective),
    /// Stage 1: `DECK_INIT`, collective.
    Deck {
        stage: Collective,
        /// The keys so far, indexed by seat. See [`Deal::by_seat`].
        by_seat: Vec<Option<VerifiedKey>>,
        /// The verified keys, **in arrival order**.
        ///
        /// Not sorted, and this matters: `verify_key` refuses a key equal to
        /// one already held and refuses the identity, and it compares against
        /// exactly this list. Sorting it or building it twice in different
        /// orders would change which duplicate is caught first.
        keys: Vec<VerifiedKey>,
        /// This client's own secret for the hand. Never leaves the process.
        secret: HandSecret,
    },
    /// The hand ended without being played out.
    ///
    /// Terminal, and it holds nothing: an abort restores every stack to what it
    /// was at the genesis of the hand, and those are `HandInit::stacks`, which
    /// every seat compared byte for byte at stage 0. There is nothing else to
    /// remember and nothing to settle.
    Aborted(Abort),
    /// The hole left while one phase is being rebuilt into the next.
    ///
    /// A phase carries values that must not be copied - a `HandSecret` above
    /// all - so moving between phases means moving the contents out, and Rust
    /// wants something in the field while that happens. This is that something.
    /// It is never observable: every function that puts it there replaces it
    /// before returning, and the arms that match it exist only so that the
    /// compiler does not have to be told to trust that.
    Between,
    /// Stages `2 .. 2m+1`: the shuffle chain, single-writer, a pair of stages
    /// for each of the `m` dealt-in seats.
    Shuffling {
        deal: Deal,
        /// Boxed: a `ShuffleChain` carries every intermediate deck, and a phase
        /// that big would make every other variant that big too.
        chain: Box<ShuffleChain>,
        /// The `SHUFFLE_STEP` heard at `2+2j`, waiting for its proof at `3+2j`.
        heard: Option<StepHeard>,
    },
    /// Stage `2m+2`: `DECK_COMMIT`, collective. The chain has finished and the
    /// deck is final; reveal tokens can be issued against it and against
    /// nothing else, which is what the type says.
    Committing {
        deal: Deal,
        table: Table,
        stage: Collective,
        /// This client's own commitment. Everybody else's is compared to it.
        mine: DeckCommit,
    },
    /// Stage `2m+3`: `DEAL_PRIVATE`, collective.
    Dealing {
        deal: Deal,
        table: Table,
        stage: Collective,
        /// Every share of every card of the hand, not just this client's.
        ///
        /// Boxed: it carries `2m + 5` token sets. Every seat's shares for every
        /// seat's hole cards are kept, which is what makes a showdown one
        /// message — the other `m-1` shares are already here. The card stays
        /// private because its **owner** never publishes its own share, and
        /// `m-1` of `m` opens nothing (`PROTOCOL.md` §3.4).
        dealing: Box<Dealing>,
    },
    /// The cards are out and the hand is being played: betting, the board,
    /// the showdown, the settlement.
    ///
    /// **One variant, not four.** Every stage from here to the end of the hand
    /// needs the same things — the round to judge the next action, what each
    /// seat has already paid to build the pots, every seat's shares to open a
    /// hand at showdown — so four variants would either repeat all of it or box
    /// it into exactly the struct below. The [`Step`] inside says which stage is
    /// open, and nothing outside `Play` reads it.
    Playing {
        deal: Deal,
        table: Table,
        /// Boxed for the reason `ShuffleChain` is: `Play` carries `2m + 5` token
        /// sets and a `BettingRound`, and an unboxed variant would make
        /// `Phase::Init` that big too.
        play: Box<Play>,
    },
}

/// Everything the hand needs once the cards are out.
///
/// Nothing in here is a local quantity. Every field is a pure function of
/// `HandInit` — which every seat compared byte for byte at stage 0 — and of the
/// chained events accepted since. That is what makes two peers derive the same
/// betting state from the same transcript, and it is why [`Hand::act`] applies
/// this client's own action through the same `BettingRound::apply` that a
/// peer's goes through: one predicate, so the two cannot disagree.
pub struct Play {
    /// Every share of every card of the hand. Also owns the deck-index map's
    /// second copy and the board as it opens.
    dealing: Box<Dealing>,

    /// `max_players` long, `true` for a seat in `HandInit::dealt_in`. The engine
    /// takes `&DealtIn` on every call and this is it, built once.
    dealt: Vec<bool>,

    /// The betting round now open. Its `committed` is **this street only**; the
    /// hand total of seat `s` is `paid[s] + round.committed[s]`.
    round: BettingRound,

    /// What each seat committed on the streets already closed.
    ///
    /// The engine does not hold this — `BettingRound::committed` is per round —
    /// and `build_pots` needs the per-hand total, so somebody has to carry it.
    /// Written in exactly one place, where a round closes.
    paid: Vec<Chips>,

    street: Street,

    /// The last seat to bet or raise in the last betting round that actually
    /// ran. `poker::actions` deliberately does not track it
    /// (`src/poker/actions.rs:113`) — its legality predicate has no use for one.
    ///
    /// The showdown order is the one place its identity changes what happens
    /// (D-021): the last aggressor shows first. It is cleared when a round
    /// **opens**, never when a street is skipped, which is what makes it the
    /// right seat after an all-in run-out where the last streets had no betting.
    aggressor: Option<SeatIdx>,

    /// `PROTOCOL.md` §4.7's `action_index`: how many actions this hand has
    /// accepted, this client's own included.
    actions: u32,

    /// This client's own two, opened when `DEAL_PRIVATE` completed.
    ///
    /// **`None` for a seat that is not dealt in.** §4.4: it *"takes no cards,
    /// and is not a party to the cryptography"* — and it still plays out the
    /// hand as a follower, because it is a required emitter of `HAND_COMPLETE`
    /// and of the boundary checkpoint that §4.9 lets it back in with. An array
    /// here made that seat's `Play` unbuildable, so the hand stopped dead at
    /// `read_my_cards` and left the phase `Between` for ever.
    cards: Option<[Card; 2]>,

    /// What each seat showed, by seat.
    ///
    /// Kept after the hand ends rather than dropped with the stage, because
    /// D-020's five-second hold is what draws it: the screen holds the hands
    /// that were shown, and a driver that had thrown them away would have
    /// nothing to hold.
    shown: Vec<Option<[Card; 2]>>,

    /// Which seats forfeited, by seat. A mucked hand is never opened by
    /// anybody: its owner never publishes the share that would open it.
    mucked: Vec<bool>,

    step: Step,
}

/// Which stage of the played-out hand is open.
enum Step {
    /// A single-writer betting stage (`PROTOCOL.md` §4.7). `to_act` owes one of
    /// the five action events at the sequence now open.
    ///
    /// **The event type is read from the event here, not from this state**, and
    /// that is deliberately the opposite of the shuffle chain's rule. There two
    /// types alternate deterministically and reading the sender's claim would
    /// let a proof be taken for a step; here five types are all legal at one
    /// sequence and *which one arrives is the entire content of the message*.
    Acting { to_act: SeatIdx },

    /// A collective `BOARD_REVEAL` for `street` (`PROTOCOL.md` §4.6).
    ///
    /// Every dealt-in seat contributes, folded and all-in seats included. That
    /// is the price of `n`-of-`n` and it is §4.6's own sentence: a folded player
    /// holds a key share until the hand ends, and a folded player who goes
    /// silent stalls the hand exactly as an active one would.
    Opening { street: Street, stage: Collective },

    /// The showdown: **one collective stage**, and every live seat owes exactly
    /// one of `SHOWDOWN_REVEAL` and `SHOWDOWN_MUCK`.
    ///
    /// One stage and not one per seat, which is `PROTOCOL.md` §4.6's shape and
    /// D-021's correction to its own first draft: the two types are the only
    /// `event_class = 0` pair that shares a `sequence`, and that exclusivity is
    /// what keeps the slot at capacity one.
    ///
    /// TDA order lives in `order` and is an **emission discipline**: a client
    /// waits until every seat ahead of it has spoken before it speaks. The
    /// stage does not care in what order it is filled, so a seat that speaks
    /// early has committed a live-poker irregularity and nothing more — it
    /// cannot see a card it was not going to see, or claim a pot it did not win.
    Showdown { stage: Collective, order: Vec<SeatIdx> },

    /// Stage after the showdown: `HAND_COMPLETE`, collective and **derived**.
    ///
    /// There is no writer. Every seat computes the byte-identical body from its
    /// own engine and signs its own copy, and a receiver recomputes all of it —
    /// the pot layering, the eligible sets, the winners, the clockwise odd-chip
    /// distribution — rather than believing any of it.
    Settling { stage: Collective, mine: Box<HandComplete> },

    /// The hand is over and settled. Kept rather than dropped, because D-020's
    /// five-second hold is what draws the board and the hands that were shown.
    Ended,
}

/// Why a hand ended without being played out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Abort {
    /// Nobody produced a required cryptographic contribution before the hand's
    /// own deadline. Nobody is named: `PROTOCOL.md` §4.9's `attributed` is
    /// empty on this path, and D-010 forbids reading it to move a chip anyway.
    Deadline,
    /// A cause this client accepted from a peer.
    Told { cause: u16 },
    /// `cause = 2`: a `SHUFFLE_PROOF` that does not hold for the pair of decks
    /// it names.
    ///
    /// The one abort this client emits that names a seat **and needs no
    /// certificate**, because it needs no witnesses: the evidence is
    /// self-authenticating. Every receiver that holds the same input deck runs
    /// the same verification over the same two signed frames and reaches the
    /// same verdict, so `PROTOCOL.md` §4.10 says *accept at once* rather than
    /// buffering it behind a deadline.
    ///
    /// It replaces ninety seconds of silence with one message. Before it, a
    /// refused proof left the chain unable to complete — `accept_step` spends
    /// the seat's one attempt before verifying, so there is no retry (C-6 rule
    /// 4) — and every peer sat until `hand_deadline_ms` and aborted with
    /// nobody named.
    ///
    /// **The evidence is deliberately not in this variant.** The phase is
    /// queried by `aborted()`, which every status read calls, and a variant
    /// holding two frames would make `Abort` fifteen kilobytes and stop it
    /// being `Copy` — so a question about what happened would copy the proof
    /// of it. The frames go straight into the message in
    /// `Hand::abort_bad_shuffle` and are not kept afterwards: they were
    /// broadcast, and a receiver that needs them has them.
    BadShuffle {
        /// The seat whose proof failed. Named in `attributed`.
        seat: SeatIdx,
    },
    /// `cause = 3`: a reveal share whose proof does not hold against the
    /// committed deck.
    ///
    /// The same shape as [`BadShuffle`](Abort::BadShuffle) and for the same
    /// reason — self-authenticating evidence, so no certificate and no
    /// deadline — over one frame instead of two: a reveal share carries its own
    /// token and proof, and the deck they are checked against is the committed
    /// one every seat already holds.
    BadReveal {
        /// The seat whose share failed. Named in `attributed`.
        seat: SeatIdx,
    },
}

/// The deck every stage from `DECK_COMMIT` onwards is about.
///
/// The final deck and the index map travel together from here to showdown
/// because neither means anything without the other: the deck says what the
/// cards are and the map says whose they are, and a stage that had one without
/// the other could open a card without knowing whether it was allowed to.
struct Table {
    deck: Box<Final<Verified<Vec<Ciphertext>>>>,
    map: DeckIndexMap,
}

/// What the deck stages leave behind and every later stage needs.
///
/// One struct rather than three fields repeated in each variant, because the
/// deck, the secret and the keys are used together from here to showdown: the
/// secret and the deck issue this client's reveal tokens, the keys verify
/// everybody else's.
struct Deal {
    deck: HandDeck,
    /// This client's own secret for the hand. Never leaves the process.
    secret: HandSecret,
    /// The verified keys in **arrival order**, which is the order
    /// `HandDeck::verify_key` compared them in and the order `HandDeck::new`
    /// aggregated them in. Not sorted, and it must not be.
    keys: Vec<VerifiedKey>,
    /// The same keys indexed by seat, which is what every later stage wants:
    /// a share arrives from a seat and has to be verified under that seat's
    /// key. `VerifiedKey` is `Copy`, so this is not a second copy of anything
    /// that could drift — the two are filled from one value.
    by_seat: Vec<Option<VerifiedKey>>,
}

/// A `SHUFFLE_STEP` admitted and waiting for the argument that justifies it.
///
/// Held rather than applied, because a deck without a verified proof is a deck
/// somebody asserted. It occupies a stage of the chain the moment it arrives -
/// that is what a single-writer stage is - but it does not enter the shuffle
/// chain until `SHUFFLE_PROOF` verifies.
struct StepHeard {
    round: u8,
    seat: SeatIdx,
    deck: Vec<Ciphertext>,
    /// The frame this step arrived in, kept verbatim.
    ///
    /// A `cause = 2` abort must carry the step **and** the proof, because a
    /// Bayer-Groth argument is about a pair of decks and the proof event names
    /// its decks by hash only. A receiver holds the input deck from its own
    /// chain; the output deck exists nowhere but here.
    ///
    /// Kept rather than re-encoded, and that is the whole reason the field is
    /// bytes and not a decoded body: the evidence has to be the signed frame
    /// the accused seat produced, and re-encoding a decoded body is a frame
    /// this client signed nothing of and whose signature would not verify.
    /// One step is held at a time, so this is about nine kilobytes.
    bytes: Vec<u8>,
}

/// One hand in progress.
pub struct Hand {
    open: Opening,
    /// The last vote tally worth saying out loud: subject, held, needed.
    ///
    /// A vote that is not counted is the quietest failure in this machinery —
    /// the peers all say their clocks ran out and nothing ever happens — so the
    /// count is reported rather than inferred.
    tally: Option<(SeatIdx, usize, usize, Hash)>,
    /// `event_hash` of every `TIMEOUT_CERT` this client has verified, its own
    /// included. An abort that names a subject carries the hash of the
    /// certificate that justifies the naming (§4.10), and this is what lets a
    /// receiver check that claim against something it verified itself rather
    /// than take the emitter's word for it.
    certs: BTreeMap<Hash, CertFact>,
    /// Subject digests already applied to the roster, so a redelivery counts
    /// once and two genuine certifications of one seat count twice.
    banked: BTreeSet<Hash>,
    /// One fully verified certificate's bytes, for this client's own abort to
    /// carry as evidence. This client's own sealed copy wins where it is a
    /// voter — §4.10's *"it verified that certificate itself"* — and otherwise
    /// the first peer copy it checked.
    proof: Option<(Hash, Vec<u8>)>,
    /// R4: a betting stage certified while this client was elsewhere. The two
    /// chains cannot be reconciled, so this is reported and not repaired.
    forked: Option<String>,
    /// A settlement that completed after this client had already aborted.
    ///
    /// §4.10: *a hand is decided or aborted, never both*, and `HAND_COMPLETE`
    /// wins — it is collective, so a completing one proves no seat was silent,
    /// which is the premise every abort rests on. A receiver that aborted first
    /// therefore replaces its terminal, and its stacks, with the settlement's.
    late: Option<Late>,
    /// What is left of **this** client's own per-hand thinking reserve.
    ///
    /// Local, and deliberately so. Every peer budgets the *whole* reserve for
    /// every other seat, because what a seat has left is known only to that
    /// seat. This is what the owner of the seat spends, spending it can only
    /// ever cost its owner a fold, and nothing derived from it crosses the
    /// wire — so no peer has to agree about it and none can be misled by it.
    bank_left_ms: u32,
    /// Diagnostic: what a refused shuffle step looked like from here.
    shuffle_note: Option<String>,
    /// Diagnostic: what two engines disagreed about when a settlement did not
    /// match.
    settle_note: Option<String>,
    /// Diagnostic: what the certificate path last decided.
    cert_note: Vec<String>,
    /// Seat → the genesis its sequence-0 `HAND_INIT` of this hand named, when
    /// that is not this client's. A roster seat's signed word that it opened
    /// this hand elsewhere: the evidence a client on a private branch can
    /// get, read off the envelope `opened()` throws away as `NotYet`.
    foreign_genesis: BTreeMap<SeatIdx, Hash>,
    /// Said once per hand, when `FOREIGN_GENESIS_FLOOR` seats name one value.
    genesis_note: Option<String>,
    /// A certificate banked after this hand's abort terminal: the next hand's
    /// roster is to be re-derived (`S1-BS`). Taken by the node.
    late_roster: bool,
    /// Whether this client's own `HAND_INIT` went out, is withheld, or never
    /// goes out.
    voice: Voice,
    /// The withheld `HAND_INIT`, while the voice is `Quiet`.
    own_init: Option<Vec<u8>>,
    /// The stage at which the "abort held: a vote this client joined is
    /// open" note was said, so it is said once per stage.
    abort_hold_said: Option<u64>,
    /// The last seat a certificate acted for, and what it did.
    ///
    /// Read once by the node so it can say so: an action nobody took is the one
    /// event at a table that has no author to attribute it to, and a player who
    /// saw a seat fold without folding deserves to know why.
    acted_for: Option<(SeatIdx, Action)>,
    /// Votes heard about each subject, by the digest that identifies it.
    ///
    /// A vote alone is not evidence and does nothing; only a complete set —
    /// one from **every** seat in `V(subject)` — becomes a certificate. Kept as
    /// the signed bytes, because a certificate embeds them whole so that it
    /// carries its own proof and needs nothing from the receiver's store.
    votes: BTreeMap<Hash, BTreeMap<SeatIdx, Vec<u8>>>,
    /// Subjects this client has already voted about, so it votes once.
    voted: BTreeSet<Hash>,
    /// The certificate stage now open, if one is.
    certifying: Option<Certifying>,
    /// Seats a completed certificate has named.
    ///
    /// `V(subject)` shrinks by this and by nothing else, and only on acceptance
    /// of a certificate that itself cleared the floor — which makes the
    /// shrinkage inductive and unbuyable with assertions. That is the whole of
    /// D-008: an attacker that could shrink `V` by asserting would reach
    /// `|V| = 1` at any table size and certify alone.
    certified: Vec<SeatIdx>,
    /// Consecutive certificates against each seat, towards
    /// [`MAX_CONSECUTIVE_AUTO_ACTIONS`].
    strikes: Vec<u8>,
    /// When the stage now open was reached, on this peer's own clock.
    ///
    /// A **cryptographic** stage that stalls is what
    /// [`STAGE_DEADLINE_FACTOR`] bounds, and it is bounded per stage rather
    /// than per hand because the hand's own budget has to accommodate a whole
    /// legal hand of human thinking and is therefore tens of minutes.
    stage_at_ms: u64,
    /// The sequence `stage_at_ms` belongs to, so the clock restarts when — and
    /// only when — the hand actually moves.
    stage_seq: u64,
    /// When this hand's stage 0 was sealed, on this peer's own clock.
    ///
    /// The hand deadline is measured from here. Advisory, local, and shared
    /// with nobody: §8.2 puts every deadline on the peer's own monotonic clock,
    /// so two peers reaching it a second apart is ordinary rather than a
    /// divergence — and it is why an abort is buffered until the receiver's own
    /// timer agrees.
    opened_at_ms: u64,
    /// `P(k)` as a vector: the seats this client has accepted at least one
    /// chained event from this hand.
    ///
    /// It is in the end-of-hand state hash, and `PROTOCOL.md` D-013 makes it
    /// what decides who is dealt in next hand — a seat that signed nothing is
    /// outside every required set from the following hand, which is how one
    /// silent seat costs exactly one hand rather than the table.
    signed: Vec<bool>,
    /// Per seat, the stage sequence this client last accepted anything from
    /// it at. Diagnostic only: it enters no hash and no wire body.
    last_heard_at: Vec<Option<u64>>,
    /// The boundary checkpoint's value for this hand, once the hand has one.
    ///
    /// `(state_hash, TERMINAL(k))` — §6.1's hash of the settled state, and the
    /// stage hash the checkpoint chains from.
    ///
    /// **Kept because the hand is about to stop holding it.** The settlement
    /// computes `state_hash` for `HAND_COMPLETE`'s body and the `Step` that
    /// carries it is replaced by `Step::Ended` a moment later; §4.9 needs the
    /// value **after** that, and §4.9 needs it to outlive the hand entirely —
    /// a checkpoint-8 `STATE_HASH` of hand `k` is admissible until
    /// `TERMINAL(k+1)` is fixed. This is the hand's half of that; the record
    /// that outlives it is `protocol::checkpoint`'s.
    checkpoint8: Option<(Hash, Hash)>,
    /// The body this client derived. Everybody else's is compared against it.
    mine: HandInit,
    /// The slot of the stage now open.
    slot: Slot,
    phase: Phase,
    params: std::sync::Arc<DeckParams>,
    /// Events for a stage this client has not reached. Held rather than
    /// refused, because GossipSub does not order two messages and a peer that
    /// is one step ahead is not a peer that is wrong. The same reason
    /// `Formation` keeps one.
    early: VecDeque<Vec<u8>>,
}

impl Hand {
    /// Begin a hand: derive the body, sign it, and record this client's own copy.
    ///
    /// The own copy is recorded here and **not** sent back to itself, which is
    /// the convention every other collective stage in this project follows.
    pub fn open(
        o: Opening,
        key: &SigningKey,
        now_ms: u64,
        next_deadline_ms: u32,
    ) -> Result<(Hand, Vec<Send>), Failed> {
        Self::open_with(o, key, now_ms, next_deadline_ms, Voice::Speak)
    }

    /// [`open`](Hand::open) with a [`Voice`]: whether this client's own
    /// `HAND_INIT` is sent now, withheld, or never sent.
    pub fn open_with(
        o: Opening,
        key: &SigningKey,
        now_ms: u64,
        next_deadline_ms: u32,
        voice: Voice,
    ) -> Result<(Hand, Vec<Send>), Failed> {
        let occupied: Vec<SeatIdx> = o.seats.iter().map(|(s, _, _)| *s).collect();
        let button = o
            .button
            .unwrap_or_else(|| provisional_button(&o.session_id, &occupied));
        // Keyed on **chips**, not on occupancy. A busted seat is still an
        // occupied seat, so "two seats at the table" and "two players with
        // chips" part company the moment somebody busts — and heads-up is a
        // rule about the second of those. `initial_positions` is the engine's
        // own TDA 32 implementation and gets it right; deriving it a second
        // time here is how the two would come to disagree.
        let mut alive = vec![false; usize::from(o.max_players)];
        for (seat, _, stack) in &o.seats {
            let Some(slot) = alive.get_mut(usize::from(*seat)) else {
                return Err(Failed::NotAtThisTable);
            };
            *slot = *stack > 0;
        }
        let Positions {
            button,
            small_blind: sb_position,
            big_blind: bb_seat,
        } = initial_positions(button, &alive, o.max_players).ok_or(Failed::NotInThisStage)?;

        let mine = HandInit {
            hand_id: o.hand_id,
            button_position: button,
            sb_position,
            bb_seat,
            level: o.level,
            small_blind: o.small_blind,
            big_blind: o.big_blind,
            ante: 0,
            // **Who took part, not who is sitting there.**
            //
            // `STATE_MACHINE.md` §5.3 step 4: `dealt_in` is the seats that are
            // active *and* in `signed_this_hand`, with chips. The required set
            // this hand was opened with **is** that — for hand one it is the
            // signers of `TABLE_READY`, and for every hand after it is `P(k-1)`
            // (D-013) — so this filters it by chips and nothing more.
            //
            // Dealing in whoever happens to occupy a seat was the defect that
            // made one disconnection kill a whole table rather than cost one
            // hand: an absent seat stayed a **required** contributor of a deck
            // key and a shuffle, so every subsequent hand stalled to the hand
            // deadline, for ever. It still pays blinds from its position, which
            // is what a tournament's dead money is and is handled by
            // `post_blinds` reading the stack rather than the deal.
            // A seat that has burned its reconnection allowance is not
            // dealt back in, however present it becomes. Its stack stays and
            // the blinds keep taking it, which is the tournament's answer to a
            // seat nobody can play against — and it can earn its way back in
            // by being present for [`REPLENISH_AFTER`] hands.
            dealt_in: {
                let mut d: Vec<SeatIdx> = o
                    .required
                    .iter()
                    .copied()
                    .filter(|s| {
                        o.seats
                            .iter()
                            .any(|(seat, _, stack)| seat == s && *stack > 0)
                            && o.grace.get(usize::from(*s)).copied().unwrap_or(0) > 0
                    })
                    .collect();
                d.sort_unstable();
                d.dedup();
                d
            },
            stacks: o.seats.iter().map(|(_, _, stack)| *stack).collect(),
            roster_hash: o.roster_hash,
            // A buy-in enters the ledger here and nowhere earlier, which is
            // what makes an abandoned formation move no chips (§4.3).
            ledger_delta: o
                .seats
                .iter()
                .map(|(seat, _, stack)| (*seat, *stack as i64))
                .collect(),
        };
        mine.self_consistent(o.max_players)
            .map_err(|what| Failed::Disagrees {
                seat: o.my_seat,
                what,
            })?;

        let slot = Slot {
            table_id: o.table_id,
            hand_id: o.hand_id,
            sequence: 0,
            previous_event_hash: o.genesis,
        };
        let bytes = chained::seal(
            EventType::HandInit,
            &slot,
            &mine,
            key,
            now_ms,
            next_deadline_ms,
            HAND_INIT_CAP,
        )
        .map_err(Failed::Wire)?;

        // **Required is `P(k-1)`; accepted is `P(k-1) ∪ A`.** The distinction is
        // §4.9's and `Collective` has carried it from the start: `hear` counts a
        // required seat and admits an accepted one without counting it towards
        // completion. `closed` — the constructor every other stage uses — is
        // exactly `new(required, required)`, which is right everywhere the
        // readmission set is empty and wrong at this one stage.
        let mut accepted: Vec<SeatIdx> = o.required.clone();
        for seat in &o.readmitted {
            if !accepted.contains(seat) {
                accepted.push(*seat);
            }
        }
        accepted.sort_unstable();
        let mut stage = Collective::new(0, EventType::HandInit.code(), &o.required, &accepted)
            .ok_or(Failed::NotInThisStage)?;
        // **Only a member of `R(HAND_INIT, k) = P(k-1)` emits one, and only a
        // seat that emitted one counts itself into `P(k)`.**
        //
        // §4.10: a seat outside `P(k-1)` *"may legally emit exactly one chained
        // event"*, and `HAND_INIT` is not it — *"every other chained type from
        // a seat outside `P(k-1)` … is a divergence under §4.0 step 12a"*. Such
        // a seat still opens the hand and follows it, which is what §4.9's
        // readmission route needs of it, and says nothing while it does.
        //
        // **The `signed` bool used to be the half that bit.** It is
        // `signed_this_hand`, and while §6.1 field 28 hashed it into
        // `PublicTableState` the settlement carried that hash — so a seat that
        // marked itself into its
        // own `P(k)` for an event no receiver accepted derived a settlement
        // that differed from everybody's by one bit, and every arriving copy
        // came back `DeckDisagrees { what: "settlement" }`. Measured, and it
        // turned §4.9's readmission route into a §6.3 divergence.
        // **The ACCEPTED set, not the required one**, and the difference is
        // §4.9's whole readmission route. A seat in `A` is admitted at this
        // stage without counting towards its completion, and emitting here is
        // exactly how it earns its way into `P(k)` and is required again at
        // `k+2`. Gating on `required` instead shut that door on the seat it was
        // built for — caught by
        // `a_readmitted_seat_is_accepted_at_stage_zero_and_never_required`.
        let a_member = accepted.contains(&o.my_seat);
        let mut signed = vec![false; usize::from(o.max_players)];
        if a_member {
            let own_hash = chained::open(&bytes, FRAME_CAP, EventType::HandInit, &slot)
                .map_err(Failed::Wire)?
                .event_hash;
            stage.hear(o.my_seat, own_hash);
            if let Some(slot) = signed.get_mut(usize::from(o.my_seat)) {
                *slot = true;
            }
        }

        // **Heard here whatever the voice, sent only when speaking.** A quiet
        // or muted hand is a member like any other — its own copy is in its
        // own stage, so the stage completes the moment the table's copies
        // land — and only the wire is withheld.
        let (sends, own_init) = match (a_member, voice) {
            (true, Voice::Speak) => (vec![Send::Broadcast(bytes)], None),
            (true, Voice::Quiet) => (Vec::new(), Some(bytes)),
            (true, Voice::Muted) | (false, _) => (Vec::new(), None),
        };
        let opened_at_ms = now_ms;
        Ok((
            Hand {
                signed,
                last_heard_at: vec![None; usize::from(o.max_players)],
            checkpoint8: None,
                tally: None,
                certs: BTreeMap::new(),
                banked: BTreeSet::new(),
                proof: None,
                forked: None,
                late: None,
                bank_left_ms: o.time_bank_ms,
                shuffle_note: None,
                settle_note: None,
                cert_note: Vec::new(),
                foreign_genesis: BTreeMap::new(),
                genesis_note: None,
                late_roster: false,
                voice,
                own_init,
                abort_hold_said: None,
                acted_for: None,
                votes: BTreeMap::new(),
                voted: BTreeSet::new(),
                certifying: None,
                certified: Vec::new(),
                strikes: vec![0; usize::from(o.max_players)],
                opened_at_ms,
                stage_at_ms: opened_at_ms,
                stage_seq: 0,
                open: o,
                mine,
                slot,
                phase: Phase::Init(stage),
                params: DeckParams::new(),
                early: VecDeque::new(),
            },
            // Nothing goes out from a seat that is in no `R` of this hand,
            // and nothing from a quiet or muted one.
            sends,
        ))
    }

    /// Take one event off the wire.
    ///
    /// Returns what this client must now say. A stage completing is what
    /// produces the next stage's message, so the two are one call: there is
    /// no state in which the hand has advanced and nobody has been told.
    pub fn on_event(
        &mut self,
        bytes: &[u8],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        // What it claims to be, before anything about it is believed. One
        // channel carries the formation's messages and the hand's, and a
        // duplicate of a stage already left has to be told from a message of
        // another kind entirely — the first is ordinary GossipSub weather and
        // the second belongs to somebody else's handler.
        let (kind, hand_id, sequence) = chained::peek(bytes, PEEK_CAP).map_err(Failed::Wire)?;
        if !matches!(
            kind,
            EventType::HandInit
                | EventType::DeckInit
                | EventType::ShuffleStep
                | EventType::ShuffleProof
                | EventType::DeckCommit
                | EventType::DealPrivate
                | EventType::BoardReveal
                | EventType::ActionCheck
                | EventType::ActionCall
                | EventType::ActionBet
                | EventType::ActionRaise
                | EventType::ActionFold
                | EventType::ShowdownReveal
                | EventType::ShowdownMuck
                | EventType::HandComplete
                | EventType::HandAbort
                | EventType::TimeoutVote
                | EventType::TimeoutCert
        ) {
            return Err(Failed::Wire(WireError::WrongType));
        }
        if hand_id != self.open.hand_id {
            return Err(Failed::NotYet);
        }
        // **Above the sequence guards, deliberately.** §4.10 says an abort's
        // position in the chain is checked loosely and that this is on purpose;
        // §4.8 says a certificate *references* its stage rather than occupying
        // it. Both were being dropped at `sequence < slot.sequence` before
        // their handlers were ever reached — and the peer least able to be
        // standing at the stage a certificate is about is the subject of it,
        // which is exactly the peer that has to hear it.
        // A settlement arriving after this client gave the hand up. §4.10:
        // `HAND_COMPLETE` wins, because it is collective.
        if kind == EventType::HandComplete && matches!(self.phase, Phase::Aborted(_)) {
            return self.on_late_settlement(bytes);
        }
        // An abort is answered from **any** phase and from any position: it is
        // §3.2's witness-independent terminal, it has no required emitter set,
        // and two peers that saw different prefixes of a hand must still be
        // able to accept each other's. A strict parent check would have each
        // reject the other's artefact, which is the deadlock again by a third
        // route.
        if kind == EventType::HandAbort {
            return self.on_hand_abort(bytes, now_ms);
        }
        if kind == EventType::TimeoutCert {
            return self.on_timeout_cert(bytes, key, now_ms);
        }
        // A vote stays below the guards. `on_timeout_vote` rebuilds the subject
        // from this client's own position, which a passed-stage receiver cannot
        // do, so it would answer `NotYet` for ever — and `NotYet` is held in a
        // bounded FIFO that the five-second re-send would then fill with stale
        // votes until every genuinely early event had been evicted.
        if sequence < self.slot.sequence {
            // A stage this hand has left. Not a fault and not worth a word: the
            // mesh delivers a message more than once as a matter of course.
            return Ok(Vec::new());
        }
        if sequence > self.slot.sequence {
            return Err(Failed::NotYet);
        }

        // A vote and a certificate **reference** the stage they are about
        // rather than occupying it, so they are answered from whatever phase
        // this client is in and never routed through it.
        if kind == EventType::TimeoutVote {
            return self.on_timeout_vote(bytes, key, now_ms);
        }

        let out = self.dispatch(bytes, kind, key, now_ms);
        self.mark_stage(now_ms);
        // **Caught here, because here is where the frame still exists.** A
        // reveal share proved wrong is `PROTOCOL.md` §4.10's `cause = 3`, whose
        // evidence is the offending event itself — and the three reveal
        // handlers all run inside a borrow of `self.phase`, where neither the
        // abort nor the note could be built. One catch above them costs three
        // restructurings and a variant.
        match out {
            Err(Failed::RevealDisproved { seat }) => {
                self.abort_bad_reveal(seat, bytes.to_vec(), key, now_ms)
            }
            other => other,
        }
    }

    /// The phase's own handler for an event that passed the guards.
    fn dispatch(
        &mut self,
        bytes: &[u8],
        kind: EventType,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        match self.phase {
            Phase::Init(_) => self.on_hand_init(bytes, key, now_ms),
            Phase::Deck { .. } => self.on_deck_init(bytes, key, now_ms),
            // Which of the two the chain is expecting is not guessed from the
            // event's own type field: a step is expected exactly when no step
            // is held, and reading the state rather than the sender's claim is
            // what stops a proof being taken for a step.
            Phase::Shuffling { ref heard, .. } => {
                if heard.is_some() {
                    self.on_shuffle_proof(bytes, key, now_ms)
                } else {
                    self.on_shuffle_step(bytes)
                }
            }
            Phase::Committing { .. } => self.on_deck_commit(bytes, key, now_ms),
            Phase::Dealing { .. } => self.on_deal_private(bytes, key, now_ms),
            Phase::Playing { ref play, .. } => match play.step {
                Step::Acting { .. } => self.on_action(bytes, kind, key, now_ms),
                Step::Opening { .. } => self.on_board_reveal(bytes, key, now_ms),
                Step::Showdown { .. } => self.on_showdown(bytes, kind, key, now_ms),
                Step::Settling { .. } => self.on_hand_complete(bytes),
                Step::Ended => Err(Failed::NothingFurther),
            },
            Phase::Aborted(_) | Phase::Between => Err(Failed::NothingFurther),
        }
    }

    /// Note that a seat has signed something this hand.
    fn note_signed(&mut self, seat: SeatIdx) {
        // **And at which stage, because an accusation needs its evidence.**
        //
        // `S1-BB`: a seat was certified out for letting its clock run when it
        // had answered every prompt in the same millisecond it arrived. The two
        // candidate causes -- its action was lost, or the two engines disagree
        // about which stage the hand is on -- are told apart by exactly one
        // fact: the stage this client last heard that seat at, against the
        // stage this client is accusing it for. Nothing recorded the first, so
        // the run could not separate them.
        //
        // This is the one place a seat is marked heard, so it is the one place
        // that has to remember where.
        let at = self.slot.sequence;
        if let Some(slot) = self.last_heard_at.get_mut(usize::from(seat)) {
            *slot = Some(at);
        }
        if let Some(slot) = self.signed.get_mut(usize::from(seat)) {
            *slot = true;
        }
    }

    /// The stage sequence this client last accepted anything from `seat` at.
    ///
    /// `None` means nothing at all this hand. Compare it against
    /// [`stage_sequence`](Self::stage_sequence): equal means the seat spoke at
    /// the very stage it is being accused of ignoring, which is a divergence
    /// and not a silence.
    pub fn last_heard_at(&self, seat: SeatIdx) -> Option<u64> {
        self.last_heard_at
            .get(usize::from(seat))
            .copied()
            .flatten()
    }

    /// The stage this client is at, which is what it accuses a seat against.
    pub fn stage_sequence(&self) -> u64 {
        self.slot.sequence
    }

    /// Which seat a sender is, or nobody.
    fn seat_of(&self, sender: &[u8; 32]) -> Result<SeatIdx, Failed> {
        self.open
            .seats
            .iter()
            .find(|(_, k, _)| k == sender)
            .map(|(seat, _, _)| *seat)
            .ok_or(Failed::NotAtThisTable)
    }

    /// Open an event against the slot now expected.
    fn opened(&self, bytes: &[u8], kind: EventType) -> Result<chained::Opened, Failed> {
        chained::open(bytes, FRAME_CAP, kind, &self.slot).map_err(|e| match e {
            // A different stage or a different parent is not a fault: it is
            // a peer one step ahead, and GossipSub does not order two
            // messages. `chained::open` reports the four slot fields with a
            // distinct message each, deliberately, so the two that mean
            // "later" can be told from the two that mean "elsewhere".
            WireError::Envelope("an event at another stage")
            | WireError::Envelope("a different parent: the sender is on another chain") => {
                Failed::NotYet
            }
            other => Failed::Wire(other),
        })
    }

    fn on_hand_init(
        &mut self,
        bytes: &[u8],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, EventType::HandInit)?;
        let seat = self.seat_of(&opened.sender)?;
        self.note_signed(seat);

        let theirs: HandInit = chained::payload(&opened, HAND_INIT_CAP).map_err(Failed::Wire)?;
        theirs
            .self_consistent(self.open.max_players)
            .map_err(|what| Failed::Disagrees { seat, what })?;
        self.mine
            .disagreement(&theirs)
            .map_err(|what| Failed::Disagrees { seat, what })?;

        let Phase::Init(stage) = &mut self.phase else {
            return Err(Failed::NothingFurther);
        };
        if stage.heard(seat) == Some(opened.event_hash) {
            return Ok(Vec::new());
        }
        match stage.hear(seat, opened.event_hash) {
            Heard::Counted | Heard::Bystander | Heard::Again => {}
            Heard::Equivocation { .. } => return Err(Failed::Equivocation { seat }),
            // **Held, not refused.** By the time a contribution reaches a
            // collective stage its sender has already been resolved to a seat
            // at this table, so `Uninvited` does not mean "a stranger" — it
            // means this client derived an accepted set that does not hold
            // that seat, which after a certificate is a roster this client and
            // its neighbour disagree about by one entry. `run.rs` turns
            // anything but `NotYet` into a GossipSub `Reject`, and repeatedly
            // rejecting an honest peer is how it stops being forwarded.
            Heard::Uninvited => return Err(Failed::NotYet),
        }
        if !stage.complete() {
            return Ok(Vec::new());
        }
        let parent = stage.hash().expect("a complete stage has one");
        self.begin_deck(parent, key, now_ms)
    }

    /// Stage 0 completed: move to stage 1 and offer this client's deck key.
    fn begin_deck(
        &mut self,
        parent: Hash,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        self.slot = self.slot.then(parent);

        // **Whether this client is a party to the deck at all.**
        //
        // §4.4: *"A seat outside `dealt_in` keeps its stack, pays its blinds
        // and antes as dead money, takes no cards, and is not a party to the
        // cryptography."* And the aggregate key is derived the same way by
        // everybody — *"every peer derives `apk = Σ pk_i` over the **`dealt_in`**
        // seats in ascending seat order"* — so a seat that adds its own key to
        // its own sum computes an `apk` over `|dealt_in| + 1` keys that no other
        // peer will ever match.
        //
        // Unguarded, that is not a quiet disagreement: the first `SHUFFLE_PROOF`
        // fails to verify against the wrong `apk`, and the hand turns that into
        // `abort_bad_shuffle` — **a §4.10 cause-2 abort naming an honest
        // shuffler**. Reproduced by
        // `a_seat_that_is_not_dealt_in_still_follows_the_hand_and_accuses_nobody`.
        //
        // The seat still follows the hand: it is a required emitter of
        // `HAND_INIT`, of `HAND_COMPLETE` and of every checkpoint, and §4.9's
        // readmission set is written by a checkpoint copy it can only produce by
        // having followed. So this is a guard on **emitting and self-seeding**,
        // never on listening.
        let a_party = self.mine.dealt_in.contains(&self.open.my_seat);

        let me = self.open.seats[self.seat_index()].1;
        let ctx = self.deck_ctx(&me);
        // The key is generated either way: `Phase::Deck` needs a `HandSecret`,
        // and a secret nothing is encrypted to is inert.
        let (secret, wire_key, proof) = self.params.keygen(&ctx);
        let body = DeckInit {
            key: wire_key.encode(),
            proof: proof.encode(),
        };
        let bytes = chained::seal(
            EventType::DeckInit,
            &self.slot,
            &body,
            key,
            now_ms,
            self.open.crypto_step_timeout_ms,
            DECK_INIT_CAP,
        )
        .map_err(Failed::Wire)?;

        // The dealt-in seats are the parties to the deck: a seat that posts
        // dead money holds no key, so the required set here is `dealt_in`
        // and not `P(k-1)`.
        let mut stage = Collective::closed(
            self.slot.sequence,
            EventType::DeckInit.code(),
            &self.mine.dealt_in,
        )
        .ok_or(Failed::NotInThisStage)?;

        // This client's own key goes through the same verification
        // everybody else's does, against an empty set, which is what makes
        // it the first entry the others are compared against.
        let own = HandDeck::verify_key(wire_key, &proof, &[], &ctx).map_err(|_| Failed::BadKey {
            seat: self.open.my_seat,
            why: "this client's own proof did not verify",
        })?;
        let own_hash = self.opened(&bytes, EventType::DeckInit)?.event_hash;
        let mut by_seat = vec![None; usize::from(self.open.max_players)];
        if a_party {
            stage.hear(self.open.my_seat, own_hash);
            if let Some(slot) = by_seat.get_mut(usize::from(self.open.my_seat)) {
                *slot = Some(own);
            }
        }
        self.phase = Phase::Deck {
            stage,
            by_seat,
            // **Empty for a seat that is not dealt in**, so that `apk` is the
            // sum over `dealt_in` and nothing else — which is what every other
            // peer computes for this hand.
            keys: if a_party { vec![own] } else { Vec::new() },
            secret,
        };
        // **Nothing is emitted by a seat that is not a party.** §4.4 gives
        // `DECK_INIT` the emitter set `dealt_in`, and the stage every receiver
        // built is `Collective::closed` over exactly that — so an out-of-set
        // copy comes back `Heard::Uninvited`, which `on_event` turns into
        // `Failed::NotYet` and the node then holds in a bounded queue that the
        // five-second re-send churns. It is refused everywhere it lands, and
        // sending it costs the sender the only thing that could still go wrong.
        Ok(if a_party {
            vec![Send::Broadcast(bytes)]
        } else {
            Vec::new()
        })
    }

    fn on_deck_init(
        &mut self,
        bytes: &[u8],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, EventType::DeckInit)?;
        let seat = self.seat_of(&opened.sender)?;
        self.note_signed(seat);
        let body: DeckInit = chained::payload(&opened, DECK_INIT_CAP).map_err(Failed::Wire)?;

        // An exact repeat is weather, and it must be answered before the key
        // is looked at: `verify_key` refuses a key already in the set, so the
        // ordinary redelivery would otherwise come back as "already in use"
        // against a peer that sent its own key twice, which is what a mesh does.
        if let Phase::Deck { stage, .. } = &self.phase {
            if stage.heard(seat) == Some(opened.event_hash) {
                return Ok(Vec::new());
            }
        }

        let wire_key = WireKey::decode(&body.key).map_err(|_| Failed::BadKey {
            seat,
            why: "not a point on the curve",
        })?;
        let proof = WireKeyProof::decode(&body.proof).map_err(|_| Failed::BadKey {
            seat,
            why: "not a well-formed ownership proof",
        })?;
        let ctx = self.deck_ctx(&opened.sender);

        let Phase::Deck {
            stage,
            keys,
            by_seat,
            ..
        } = &mut self.phase
        else {
            return Err(Failed::NothingFurther);
        };
        // Against the keys already held, in arrival order: that is what
        // refuses the identity element and a key equal to one already
        // seated, which is condition C-4 of the deck review.
        let verified =
            HandDeck::verify_key(wire_key, &proof, keys, &ctx).map_err(|_| Failed::BadKey {
                seat,
                why: "the proof does not hold, or the key is already in use",
            })?;

        match stage.hear(seat, opened.event_hash) {
            Heard::Counted | Heard::Bystander => {
                keys.push(verified);
                if let Some(slot) = by_seat.get_mut(usize::from(seat)) {
                    *slot = Some(verified);
                }
            }
            // A repeat must not add the key twice, or the duplicate check
            // would refuse the honest sender's own key on its next copy.
            Heard::Again => {}
            Heard::Equivocation { .. } => return Err(Failed::Equivocation { seat }),
            // **Held, not refused.** By the time a contribution reaches a
            // collective stage its sender has already been resolved to a seat
            // at this table, so `Uninvited` does not mean "a stranger" — it
            // means this client derived an accepted set that does not hold
            // that seat, which after a certificate is a roster this client and
            // its neighbour disagree about by one entry. `run.rs` turns
            // anything but `NotYet` into a GossipSub `Reject`, and repeatedly
            // rejecting an honest peer is how it stops being forwarded.
            Heard::Uninvited => return Err(Failed::NotYet),
        }
        if !stage.complete() {
            return Ok(Vec::new());
        }
        let parent = stage.hash().expect("a complete stage has one");
        self.slot = self.slot.then(parent);
        self.begin_shuffle(key, now_ms)
    }

    /// Stage 1 completed: build the deck and open the shuffle chain.
    fn begin_shuffle(&mut self, key: &SigningKey, now_ms: u64) -> Result<Vec<Send>, Failed> {
        // Move the keys and the secret out of the phase rather than cloning
        // them: `HandSecret` is the one value in this program that must exist
        // in exactly one place, and a `clone` on it would be a second copy of
        // a hole-card key with no owner responsible for it.
        let taken = std::mem::replace(&mut self.phase, Phase::Between);
        let Phase::Deck {
            keys,
            by_seat,
            secret,
            ..
        } = taken
        else {
            return Err(Failed::NothingFurther);
        };
        let deal = Deal {
            deck: HandDeck::new(self.params.clone(), &keys),
            secret,
            keys,
            by_seat,
        };

        // The shufflers are the dealt-in seats in ascending order, and their
        // keys go in the same order: `ShuffleChain` pairs the two by index to
        // build each step's context, so a mismatch here would make every
        // honest peer unable to verify an honest shuffle.
        let mut order = self.mine.dealt_in.clone();
        order.sort_unstable();
        let mut chain_keys = Vec::with_capacity(order.len());
        for seat in &order {
            let k = self
                .open
                .seats
                .iter()
                .find(|(s, _, _)| s == seat)
                .ok_or(Failed::NotAtThisTable)?;
            chain_keys.push(k.1);
        }
        let chain = ShuffleChain::open(
            ChainParams {
                protocol_version: crate::protocol::messages::PROTOCOL_VERSION,
                table_id: self.open.table_id,
                session_id: self.open.session_id,
                hand_id: self.open.hand_id,
            },
            order,
            chain_keys,
        )
        .map_err(|_| Failed::NotInThisStage)?;

        self.phase = Phase::Shuffling {
            deal,
            chain: Box::new(chain),
            heard: None,
        };
        self.shuffle_if_mine(key, now_ms)
    }

    /// Take this client's turn in the chain, if it is this client's turn.
    ///
    /// Emits both stages at once. The shuffler can, because both stage hashes
    /// are single-writer and it holds every input to them: it does not have to
    /// wait to hear its own step back before it can seal the proof.
    fn shuffle_if_mine(&mut self, key: &SigningKey, now_ms: u64) -> Result<Vec<Send>, Failed> {
        let Phase::Shuffling { deal, chain, .. } = &mut self.phase else {
            return Ok(Vec::new());
        };
        if chain.whose_turn() != Some(self.open.my_seat) {
            return Ok(Vec::new());
        }
        let round = u8::try_from(chain.steps_taken()).map_err(|_| Failed::NotInThisStage)?;

        // The proof is proved under the *proof* stage's sequence, one past the
        // step's. `next_ctx` owns that derivation so that a shuffler and its
        // verifiers cannot disagree about it.
        let proof_seq = self.slot.sequence + 1;
        let ctx = chain.next_ctx(proof_seq).ok_or(Failed::NothingFurther)?;
        let input_hash = input_deck_hash(chain.last_verified());
        let (next, proof) = deal
            .deck
            .shuffle(chain.last_verified(), &ctx)
            .map_err(|_| Failed::BadShuffle {
                seat: self.open.my_seat,
                why: "this client could not shuffle the deck it holds",
            })?;
        let output_hash = deck_hash(&next);

        let step = chained::seal(
            EventType::ShuffleStep,
            &self.slot,
            &ShuffleStep {
                shuffle_round: round,
                deck: flatten(&next),
            },
            key,
            now_ms,
            self.open.crypto_step_timeout_ms,
            SHUFFLE_STEP_CAP,
        )
        .map_err(Failed::Wire)?;
        let step_hash = self.opened(&step, EventType::ShuffleStep)?.event_hash;

        // **Nothing is moved until the chain has taken the step.** The slot used
        // to advance here, before the proof was even sealed, so any failure
        // afterwards left the slot one stage ahead of the chain — and
        // `whose_turn()` therefore still said *this seat*. The next event that
        // reached `shuffle_if_mine` ran the whole thing again, and
        // `ShuffleChain::accept_step` reserves the position **before** it
        // verifies (C-9, so a bad prover cannot make everyone re-verify), so
        // the retry answered `AlreadySubmitted` and the seat could never take
        // its turn. That is the refusal that has been appearing in live runs
        // for a day: `seat N's shuffle: a second attempt at a position that
        // already has one`, always about this client's own contribution.
        let after_step = self.slot.then(stage_hash_single(
            self.slot.sequence,
            EventType::ShuffleStep.code(),
            self.open.my_seat,
            step_hash,
        ));

        let body = ShuffleProof {
            shuffle_round: round,
            input_deck_hash: input_hash,
            output_deck_hash: output_hash,
            proof,
        };
        let proof_event = chained::seal(
            EventType::ShuffleProof,
            &after_step,
            &body,
            key,
            now_ms,
            self.open.crypto_step_timeout_ms,
            SHUFFLE_PROOF_CAP,
        )
        .map_err(Failed::Wire)?;
        let proof_hash = chained::open(
            &proof_event,
            FRAME_CAP,
            EventType::ShuffleProof,
            &after_step,
        )
        .map_err(Failed::Wire)?
        .event_hash;

        // This client's own step goes through `accept_step` like anybody
        // else's, which costs it the 42 ms of verifying its own argument. That
        // is the price of having one path into the chain: a second, trusting
        // path would be a path an attacker only has to find once.
        let me = self.open.my_seat;
        let proof_seq = after_step.sequence;
        let Phase::Shuffling { deal, chain, .. } = &mut self.phase else {
            unreachable!("just matched")
        };
        let taken = chain.steps_taken();
        let turn = chain.whose_turn();
        let mine = chain.ctx_report(taken, proof_seq);
        chain
            .accept_step(&deal.deck, me, next, &body.proof, proof_seq)
            .map_err(|e| step_failure(me, e))
            .inspect_err(|_| {
                // The instrument that was missing. Every diagnostic for this
                // family was on the path that verifies a PEER's proof, and the
                // refusal that keeps appearing in live runs is about this
                // client's own — so a day of logs said which seat and never
                // which road.
                self.shuffle_note = Some(format!(
                    "own shuffle refused at round {round}: chain step {taken}, turn {turn:?}, slot {} | prover {mine}",
                    self.slot.sequence
                ));
            })?;
        // Said on success too, once per hand per shuffler. A verifier's
        // refusal names what IT checked against; without the prover's side
        // there is nothing to compare it with, and the disagreement is by
        // construction between two peers rather than inside one.
        self.shuffle_note = Some(format!("prover {mine}"));
        self.slot = after_step.then(stage_hash_single(
            proof_seq,
            EventType::ShuffleProof.code(),
            me,
            proof_hash,
        ));

        let mut out = vec![Send::Broadcast(step), Send::Broadcast(proof_event)];
        out.append(&mut self.finish_chain_if_done(key, now_ms)?);
        Ok(out)
    }

    /// Admit a `SHUFFLE_STEP`: the deck is held, the stage is chained.
    fn on_shuffle_step(&mut self, bytes: &[u8]) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, EventType::ShuffleStep)?;
        let seat = self.seat_of(&opened.sender)?;
        self.note_signed(seat);
        let body: ShuffleStep =
            chained::payload(&opened, SHUFFLE_STEP_CAP).map_err(Failed::Wire)?;

        let Phase::Shuffling { chain, .. } = &self.phase else {
            return Err(Failed::NothingFurther);
        };
        let expected = chain.whose_turn();
        if expected != Some(seat) {
            return Err(Failed::OutOfTurn { seat, expected });
        }
        if usize::from(body.shuffle_round) != chain.steps_taken() {
            return Err(Failed::BadDeck {
                seat,
                why: "the round does not match the chain's position",
            });
        }
        let deck = unflatten(&body.deck).ok_or(Failed::BadDeck {
            seat,
            why: "not fifty-two cards of sixty-six bytes",
        })?;

        let hash = stage_hash_single(
            self.slot.sequence,
            EventType::ShuffleStep.code(),
            seat,
            opened.event_hash,
        );
        self.slot = self.slot.then(hash);
        let Phase::Shuffling { heard, .. } = &mut self.phase else {
            unreachable!("just matched")
        };
        *heard = Some(StepHeard {
            round: body.shuffle_round,
            seat,
            deck,
            bytes: bytes.to_vec(),
        });
        Ok(Vec::new())
    }

    /// Verify a `SHUFFLE_PROOF` and let the held step into the chain.
    fn on_shuffle_proof(
        &mut self,
        bytes: &[u8],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, EventType::ShuffleProof)?;
        let seat = self.seat_of(&opened.sender)?;
        self.note_signed(seat);
        let body: ShuffleProof =
            chained::payload(&opened, SHUFFLE_PROOF_CAP).map_err(Failed::Wire)?;

        let Phase::Shuffling { deal, chain, heard } = &mut self.phase else {
            return Err(Failed::NothingFurther);
        };
        let held = heard.as_ref().ok_or(Failed::NotYet)?;
        // The proof must come from the seat whose step is held. A different
        // seat here is not a late message from elsewhere - the chain is at
        // this exact stage - so it is attributable.
        if held.seat != seat {
            return Err(Failed::OutOfTurn {
                seat,
                expected: Some(held.seat),
            });
        }
        if held.round != body.shuffle_round {
            return Err(Failed::BadDeck {
                seat,
                why: "the proof's round does not match the step's",
            });
        }
        // The two cheap checks before the expensive one, in that order: this
        // is what makes a lifted proof cost a hash rather than 42 ms.
        if body.input_deck_hash != input_deck_hash(chain.last_verified()) {
            return Err(Failed::BadDeck {
                seat,
                why: "the proof names an input deck this client does not hold",
            });
        }
        if body.output_deck_hash != deck_hash(&held.deck) {
            return Err(Failed::BadDeck {
                seat,
                why: "the proof names an output deck that is not the step's",
            });
        }

        let deck = held.deck.clone();
        let taken = chain.steps_taken();
        let report = chain.ctx_report(taken, self.slot.sequence);
        let outcome = chain.accept_step(&deal.deck, seat, deck, &body.proof, self.slot.sequence);

        // **The step's own frame, kept only when it is about to be evidence.**
        // Nine kilobytes cloned on every proof would be nine kilobytes cloned
        // for nothing on the path that is taken every time.
        //
        // `Invalid` and nothing else. `CouldNotVerify` is *never* evidence
        // against anybody - `VerifyOutcome` exists to carry exactly that
        // distinction - and the procedural refusals (out of turn, already
        // submitted, out of range) are facts about this receiver's chain rather
        // than about the proof, so a receiver re-running the argument would
        // find nothing wrong with it and refuse the accusation. Emitting a
        // cause 2 for any of them would be broadcasting an accusation that
        // every honest peer rejects, which ends no hand and names this client
        // as the one making things up.
        let evidence = match &outcome {
            Err(StepError::Rejected(VerifyOutcome::Invalid(_))) => {
                Some([held.bytes.clone(), bytes.to_vec()])
            }
            _ => None,
        };

        if let Err(e) = outcome {
            self.shuffle_note = Some(format!(
                "shuffle refusal from seat {seat}: chain at step {taken}, slot sequence {}, round {} | verifier {report}",
                self.slot.sequence, body.shuffle_round
            ));
            if let Some(evidence) = evidence {
                // §4.10 cause 2, and it is the difference between a hand that
                // ends now and a hand that ends in ninety seconds with nobody
                // named. `accept_step` has already spent this seat's one
                // attempt (C-6 rule 4), so the chain can never complete: there
                // is nothing left to wait for and the only question was how
                // long everybody waits to find out.
                return self.abort_bad_shuffle(seat, evidence, key, now_ms);
            }
            // Seen twice and unexplained, so the refusal carries what would
            // settle it: whether the chain had already taken this position
            // while the slot had not moved past it. Guessing at this cost two
            // wrong hypotheses already.
            if matches!(e, StepError::AlreadySubmitted) {
                return Err(Failed::Elsewhere {
                    seat,
                    what: "no step had been taken at this position — chain position and slot sequence are in the log",
                });
            }
            return Err(step_failure(seat, e));
        }

        let Phase::Shuffling { heard, .. } = &mut self.phase else {
            unreachable!("just matched")
        };
        *heard = None;

        let hash = stage_hash_single(
            self.slot.sequence,
            EventType::ShuffleProof.code(),
            seat,
            opened.event_hash,
        );
        self.slot = self.slot.then(hash);

        let mut out = self.finish_chain_if_done(key, now_ms)?;
        out.append(&mut self.shuffle_if_mine(key, now_ms)?);
        Ok(out)
    }

    /// If the last shuffler has been through, close the chain.
    fn finish_chain_if_done(
        &mut self,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let Phase::Shuffling { chain, .. } = &mut self.phase else {
            return Ok(Vec::new());
        };
        let Some(final_deck) = chain.finish() else {
            return Ok(Vec::new());
        };
        let taken = std::mem::replace(&mut self.phase, Phase::Between);
        let Phase::Shuffling { deal, .. } = taken else {
            unreachable!("just matched")
        };
        self.begin_commit(deal, final_deck, key, now_ms)
    }

    /// The chain has closed: commit to the deck everybody must now agree on.
    fn begin_commit(
        &mut self,
        deal: Deal,
        final_deck: Final<Verified<Vec<Ciphertext>>>,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        // The map is a pure function of the hand's own parameters and was
        // fixed before the chain started - which is the point of it. A last
        // shuffler that could argue afterwards about which index is the
        // button's first card would be choosing who gets which cards.
        let mut seated = vec![false; usize::from(self.open.max_players)];
        for seat in &self.mine.dealt_in {
            let Some(slot) = seated.get_mut(usize::from(*seat)) else {
                return Err(Failed::NotAtThisTable);
            };
            *slot = true;
        }
        let map = DeckIndexMap::build(&seated, self.mine.button_position, DECK)
            .map_err(|_| Failed::NotInThisStage)?;

        let mine = DeckCommit {
            final_deck_hash: deck_hash(final_deck.as_ref().as_ref()),
            index_map_hash: map.index_map_hash(),
            apk: deal.deck.apk().encode(),
        };
        let bytes = self.say(EventType::DeckCommit, &mine, DECK_COMMIT_CAP, key, now_ms)?;
        let mut stage = Collective::closed(
            self.slot.sequence,
            EventType::DeckCommit.code(),
            &self.mine.dealt_in,
        )
        .ok_or(Failed::NotInThisStage)?;
        stage.hear(
            self.open.my_seat,
            self.opened(&bytes, EventType::DeckCommit)?.event_hash,
        );

        self.phase = Phase::Committing {
            deal,
            table: Table {
                deck: Box::new(final_deck),
                map,
            },
            stage,
            mine,
        };
        Ok(vec![Send::Broadcast(bytes)])
    }

    fn on_deck_commit(
        &mut self,
        bytes: &[u8],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, EventType::DeckCommit)?;
        let seat = self.seat_of(&opened.sender)?;
        self.note_signed(seat);
        let theirs: DeckCommit =
            chained::payload(&opened, DECK_COMMIT_CAP).map_err(Failed::Wire)?;

        let Phase::Committing { stage, mine, .. } = &mut self.phase else {
            return Err(Failed::NothingFurther);
        };
        if stage.heard(seat) == Some(opened.event_hash) {
            return Ok(Vec::new());
        }
        mine.disagreement(&theirs)
            .map_err(|what| Failed::DeckDisagrees { seat, what })?;
        match stage.hear(seat, opened.event_hash) {
            Heard::Counted | Heard::Bystander | Heard::Again => {}
            Heard::Equivocation { .. } => return Err(Failed::Equivocation { seat }),
            // **Held, not refused.** By the time a contribution reaches a
            // collective stage its sender has already been resolved to a seat
            // at this table, so `Uninvited` does not mean "a stranger" — it
            // means this client derived an accepted set that does not hold
            // that seat, which after a certificate is a roster this client and
            // its neighbour disagree about by one entry. `run.rs` turns
            // anything but `NotYet` into a GossipSub `Reject`, and repeatedly
            // rejecting an honest peer is how it stops being forwarded.
            Heard::Uninvited => return Err(Failed::NotYet),
        }
        if !stage.complete() {
            return Ok(Vec::new());
        }
        let parent = stage.hash().expect("a complete stage has one");
        self.slot = self.slot.then(parent);
        self.begin_dealing(key, now_ms)
    }

    /// Everybody holds the same deck: issue this client's shares.
    ///
    /// One share for every hole index that is **not** this client's own, and
    /// its own two computed but never published. That single omission is what
    /// makes a hole card private, and it is worth being explicit that nothing
    /// else does: the message is broadcast, every peer keeps every share in it,
    /// and each card is one share short for everybody but its owner.
    fn begin_dealing(&mut self, key: &SigningKey, now_ms: u64) -> Result<Vec<Send>, Failed> {
        let taken = std::mem::replace(&mut self.phase, Phase::Between);
        let Phase::Committing { deal, table, .. } = taken else {
            return Err(Failed::NothingFurther);
        };

        let me = self.open.my_seat;

        // **A seat that is not dealt in follows this stage and contributes
        // nothing to it.** §4.4: it *"takes no cards, and is not a party to the
        // cryptography"*, so it holds no hole-card indices and no deck key —
        // and the two lookups below are exactly where that used to stop the
        // hand dead with `NotInThisStage`, at the `DEAL_PRIVATE` stage, while
        // every dealt-in seat walked on. Measured: the observer stuck at
        // sequence 7 while the others reached 21.
        //
        // It still needs the stage, because §4.6 gives it *"all `m` tokens for
        // those indices"* and it must reach the settlement to produce the
        // checkpoint §4.9 lets it back in with.
        if !self.mine.dealt_in.contains(&me) {
            let stage = Collective::closed(
                self.slot.sequence,
                EventType::DealPrivate.code(),
                &self.mine.dealt_in,
            )
            .ok_or(Failed::NotInThisStage)?;
            let dealing = Dealing::new(table.map.clone(), self.mine.dealt_in.clone());
            self.phase = Phase::Dealing {
                deal,
                table,
                stage,
                dealing: Box::new(dealing),
            };
            return Ok(Vec::new());
        }

        let mine_indices = table.map.hole_cards(me).ok_or(Failed::NotInThisStage)?;
        let my_key = deal.key_of(me).ok_or(Failed::NotInThisStage)?;
        let ctx = self.deck_ctx(&self.open.seats[self.seat_index()].1);

        // The map is cloned rather than moved because `DECK_COMMIT` committed
        // to its hash and `index_map()` still answers from `Table`. It is a
        // `Vec<u8>` and a byte; both copies are built from one value and
        // neither is ever mutated, so there is nothing that can drift.
        let mut dealing = Dealing::new(table.map.clone(), self.mine.dealt_in.clone());
        let identity = Identity {
            seat: me,
            key: my_key,
            secret: &deal.secret,
        };

        // Ascending by index, which is the canonical order, and built by
        // walking the map rather than by sorting afterwards - a sort would hide
        // a duplicate instead of making it impossible.
        let mut entries = Vec::new();
        for index in every_hole_index(&table.map) {
            let (token, proof) = dealing
                .own_share(&deal.as_ref(&table), &identity, index, &ctx)
                .map_err(|e| refused(me, e))?;
            // This client's own two are computed, recorded, and never sent.
            if !mine_indices.iter().any(|i| i.get() == index.get()) {
                entries.push(RevealEntry {
                    deck_index: index.get(),
                    token: token.encode(),
                    proof: proof.encode(),
                });
            }
        }

        let body = DealPrivate { entries };
        let bytes = self.say(EventType::DealPrivate, &body, DEAL_PRIVATE_CAP, key, now_ms)?;
        let mut stage = Collective::closed(
            self.slot.sequence,
            EventType::DealPrivate.code(),
            &self.mine.dealt_in,
        )
        .ok_or(Failed::NotInThisStage)?;
        stage.hear(
            me,
            self.opened(&bytes, EventType::DealPrivate)?.event_hash,
        );

        self.phase = Phase::Dealing {
            deal,
            table,
            stage,
            dealing: Box::new(dealing),
        };
        Ok(vec![Send::Broadcast(bytes)])
    }

    fn on_deal_private(
        &mut self,
        bytes: &[u8],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, EventType::DealPrivate)?;
        let seat = self.seat_of(&opened.sender)?;
        self.note_signed(seat);
        let body: DealPrivate =
            chained::payload(&opened, DEAL_PRIVATE_CAP).map_err(Failed::Wire)?;
        if !body.canonical() {
            return Err(Failed::BadToken {
                seat,
                why: "the entries are not ascending and unique",
            });
        }

        let ctx = self.deck_ctx(&opened.sender);
        let Phase::Dealing {
            deal,
            table,
            stage,
            dealing,
        } = &mut self.phase
        else {
            return Err(Failed::NothingFurther);
        };

        // An exact repeat is weather: a mesh redelivers as a matter of course,
        // and a token set takes one share per seat and never an update, so
        // replaying the shares would report an honest peer as at fault. Answered
        // before anything is spent on the message, and **before** the stage is
        // told — a seat is counted towards a stage only by a message that
        // verified, which is the other half of the same rule.
        if stage.heard(seat) == Some(opened.event_hash) {
            return Ok(Vec::new());
        }

        // Exactly the required set: fewer is a refusal to cooperate, more means
        // a share for a card this sender was never asked to help open - its own
        // (harmless but wrong) or a board index, which is an attempt to open
        // the board early.
        let their_own = table.map.hole_cards(seat).ok_or(Failed::NotInThisStage)?;
        let wanted: Vec<u8> = every_hole_index(&table.map)
            .into_iter()
            .map(|i| i.get())
            .filter(|i| !their_own.iter().any(|o| o.get() == *i))
            .collect();
        if body.indices() != wanted {
            return Err(Failed::BadToken {
                seat,
                why: "not exactly the indices this seat owes the table",
            });
        }

        for entry in &body.entries {
            let index = table
                .map
                .index_from_wire(entry.deck_index)
                .ok_or(Failed::BadToken {
                    seat,
                    why: "an index this hand gave no role to",
                })?;
            let token = WireToken::decode(&entry.token).map_err(|_| Failed::BadToken {
                seat,
                why: "the share is not a point on the curve",
            })?;
            let proof = WireTokenProof::decode(&entry.proof).map_err(|_| Failed::BadToken {
                seat,
                why: "the proof is not well formed",
            })?;
            // **Every** share is kept, not only the ones for this client's own
            // cards. That is what makes a showdown one message: seat X's hand
            // opens from the `m-1` shares already here plus the one X publishes.
            dealing
                .accept(
                    &deal.as_ref(table),
                    self.open.my_seat,
                    &Share {
                        from: seat,
                        index,
                        token,
                        proof: &proof,
                    },
                    &ctx,
                )
                .map_err(|e| refused(seat, e))?;
        }

        // Everything verified, so now the seat counts. A different body from
        // this seat at this stage is still the equivocation it always was.
        match stage.hear(seat, opened.event_hash) {
            Heard::Counted | Heard::Bystander | Heard::Again => {}
            Heard::Equivocation { .. } => return Err(Failed::Equivocation { seat }),
            // **Held, not refused.** By the time a contribution reaches a
            // collective stage its sender has already been resolved to a seat
            // at this table, so `Uninvited` does not mean "a stranger" — it
            // means this client derived an accepted set that does not hold
            // that seat, which after a certificate is a roster this client and
            // its neighbour disagree about by one entry. `run.rs` turns
            // anything but `NotYet` into a GossipSub `Reject`, and repeatedly
            // rejecting an honest peer is how it stops being forwarded.
            Heard::Uninvited => return Err(Failed::NotYet),
        }
        if !stage.complete() {
            return Ok(Vec::new());
        }
        let parent = stage.hash().expect("a complete stage has one");
        self.slot = self.slot.then(parent);
        self.read_my_cards(key, now_ms)
    }

    /// Every share is in: open this client's two cards.
    fn read_my_cards(
        &mut self,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let taken = std::mem::replace(&mut self.phase, Phase::Between);
        let Phase::Dealing {
            deal,
            table,
            mut dealing,
            ..
        } = taken
        else {
            return Err(Failed::NothingFurther);
        };

        let me = self.open.my_seat;
        // **A seat that is not dealt in reads no cards and plays on anyway.**
        // §4.4 gives it no hole-card indices at all, so this used to end the
        // hand for it with `NotInThisStage` — and because the phase is taken
        // out with `mem::replace` above, the failure left it in `Between` for
        // ever, one stage short of the settlement §4.9 needs it to reach.
        let cards: Option<[Card; 2]> = if self.mine.dealt_in.contains(&me) {
            let indices = table.map.hole_cards(me).ok_or(Failed::NotInThisStage)?;
            let mut cards = Vec::with_capacity(2);
            for index in indices {
                let card =
                    dealing
                        .open(&deal.as_ref(&table), index)
                        .map_err(|_| Failed::BadToken {
                            seat: me,
                            why: "the stage completed without every share for a card",
                        })?;
                cards.push(card);
            }
            Some(cards.try_into().map_err(|_| Failed::NotInThisStage)?)
        } else {
            None
        };

        let n = usize::from(self.open.max_players);
        let mut dealt = vec![false; n];
        let mut stack = vec![0 as Chips; n];
        for (seat, _, chips) in &self.open.seats {
            let Some(slot) = stack.get_mut(usize::from(*seat)) else {
                return Err(Failed::NotAtThisTable);
            };
            *slot = *chips;
        }
        for seat in &self.mine.dealt_in {
            let Some(slot) = dealt.get_mut(usize::from(*seat)) else {
                return Err(Failed::NotAtThisTable);
            };
            *slot = true;
        }

        let mut play = Play {
            dealing,
            round: BettingRound {
                big_blind: self.mine.big_blind,
                current_bet: 0,
                last_full_raise: self.mine.big_blind,
                committed: vec![0; n],
                stack,
                acted: vec![false; n],
                // A seat that is not dealt in is `folded` from the start. The
                // engine has one predicate for "cannot act", and this is how a
                // seat that was never in the hand enters it.
                folded: dealt.iter().map(|d| !d).collect(),
            },
            dealt,
            paid: vec![0; n],
            street: Street::PreFlop,
            aggressor: None,
            actions: 0,
            cards,
            shown: vec![None; n],
            mucked: vec![false; n],
            step: Step::Ended,
        };

        // Pre-flop, and only pre-flop, the blinds go in before anybody acts.
        // They are not actions: `post_blinds` leaves `acted` false for both,
        // which is what gives the big blind its option without a rule for it.
        post_blinds(
            &mut play.round,
            self.mine.sb_position,
            self.mine.bb_seat,
            self.mine.small_blind,
            self.mine.big_blind,
        );
        self.phase = Phase::Playing {
            deal,
            table,
            play: Box::new(play),
        };
        self.open_betting(Street::PreFlop, key, now_ms)
    }

    /// Open the betting for a street, or skip past it if nobody can act.
    ///
    /// The skip is not an optimisation. When every remaining seat is all in
    /// there is no decision left to make and no message anybody could send, so
    /// a street that waited for one would wait for ever. The board still opens;
    /// only the betting is skipped.
    fn open_betting(
        &mut self,
        street: Street,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let button = self.mine.button_position;
        let bb_seat = self.mine.bb_seat;
        let seat_count = self.open.max_players;
        let up = {
            let Phase::Playing { play, .. } = &mut self.phase else {
                return Err(Failed::NothingFurther);
            };
            play.street = street;
            // Cleared when a round **opens**. A street skipped in a run-out
            // leaves the previous street's aggressor standing, which is the
            // seat that must show first at the showdown (D-021).
            play.aggressor = None;
            if betting_is_closed(&play.round, &play.dealt) {
                None
            } else {
                first_to_act(street, &play.round, &play.dealt, button, bb_seat, seat_count)
            }
        };
        match up {
            Some(to_act) => {
                let Phase::Playing { play, .. } = &mut self.phase else {
                    return Err(Failed::NothingFurther);
                };
                play.step = Step::Acting { to_act };
                Ok(Vec::new())
            }
            // Nobody can act. Either every remaining seat is all in — the
            // run-out, where the board still opens and only the betting is
            // skipped — or all but one has folded, which `close_round_and_open`
            // recognises and ends the hand on. Ending it here instead would
            // freeze an all-in hand with the board unfinished.
            None => self.close_round_and_open(key, now_ms),
        }
    }

    /// This client's own action, from the player.
    ///
    /// The second entry point, and the only one: every other stage of the hand
    /// is driven by an arriving event, and this is the one that waits for a
    /// human. It applies the action through `BettingRound::apply` — the **same**
    /// predicate a receiver runs — so this client cannot send itself something
    /// a receiver would refuse.
    pub fn act(
        &mut self,
        action: Action,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let me = self.open.my_seat;
        let (kind, body) = {
            let Phase::Playing { play, .. } = &self.phase else {
                return Err(Failed::NothingFurther);
            };
            let Step::Acting { to_act } = play.step else {
                return Err(Failed::Elsewhere {
                    seat: me,
                    what: "a betting stage were open",
                });
            };
            if to_act != me {
                return Err(Failed::OutOfTurn {
                    seat: me,
                    expected: Some(to_act),
                });
            }
            let head = ActionHead {
                street: street_code(play.street),
                seat: me,
                action_index: play.actions,
            };
            let kind = match action {
                Action::Fold => EventType::ActionFold,
                Action::Check => EventType::ActionCheck,
                Action::Call => EventType::ActionCall,
                Action::Bet(_) => EventType::ActionBet,
                Action::Raise(_) => EventType::ActionRaise,
            };
            let body = match action {
                Action::Bet(total) | Action::Raise(total) => Body::Amount(ActionAmount {
                    street: head.street,
                    seat: head.seat,
                    action_index: head.action_index,
                    total,
                }),
                _ => Body::Head(head),
            };
            (kind, body)
        };

        let bytes = match &body {
            Body::Head(h) => self.say(kind, h, ACTION_CAP, key, now_ms)?,
            Body::Amount(a) => self.say(kind, a, ACTION_CAP, key, now_ms)?,
        };
        let hash = self.opened(&bytes, kind)?.event_hash;
        let mut out = vec![Send::Broadcast(bytes)];
        out.append(&mut self.apply_action(me, action, kind, hash, key, now_ms)?);
        // What this turn cost the reserve, charged once and only on the action
        // that ends the turn. `stage_at_ms` is when this client accepted the
        // event that gave it the turn, which is §8.2's own starting point, so
        // the reserve drains against the same clock the deadline is measured
        // on. An action refused as illegal charges nothing: the player has not
        // acted yet and will be asked again.
        let over = now_ms
            .saturating_sub(self.stage_at_ms)
            .saturating_sub(u64::from(self.open.action_timeout_ms));
        self.bank_left_ms = self
            .bank_left_ms
            .saturating_sub(u32::try_from(over).unwrap_or(u32::MAX));
        self.mark_stage(now_ms);
        Ok(out)
    }

    /// One betting action off the wire.
    fn on_action(
        &mut self,
        bytes: &[u8],
        kind: EventType,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, kind)?;
        let seat = self.seat_of(&opened.sender)?;
        self.note_signed(seat);

        let (head, action) = match kind {
            EventType::ActionBet | EventType::ActionRaise => {
                let a: ActionAmount =
                    chained::payload(&opened, ACTION_CAP).map_err(Failed::Wire)?;
                let act = if kind == EventType::ActionBet {
                    Action::Bet(a.total)
                } else {
                    Action::Raise(a.total)
                };
                (a.head(), act)
            }
            _ => {
                let h: ActionHead =
                    chained::payload(&opened, ACTION_CAP).map_err(Failed::Wire)?;
                let act = match kind {
                    EventType::ActionFold => Action::Fold,
                    EventType::ActionCheck => Action::Check,
                    _ => Action::Call,
                };
                (h, act)
            }
        };

        {
            let Phase::Playing { play, .. } = &self.phase else {
                return Err(Failed::NothingFurther);
            };
            let Step::Acting { to_act } = play.step else {
                return Err(Failed::Elsewhere {
                    seat,
                    what: "a betting stage were open",
                });
            };
            if to_act != seat {
                return Err(Failed::OutOfTurn {
                    seat,
                    expected: Some(to_act),
                });
            }
            // The envelope already proved who signed it. These three fields say
            // where the *sender* believes the hand is, and a disagreement means
            // two engines have diverged rather than that anybody lied — which
            // is why they are on the wire at all.
            if head.seat != seat {
                return Err(Failed::Elsewhere {
                    seat,
                    what: "it were another seat",
                });
            }
            if street_from_code(head.street) != Some(play.street) {
                return Err(Failed::Elsewhere {
                    seat,
                    what: "the hand were on another street",
                });
            }
            if head.action_index != play.actions {
                return Err(Failed::Elsewhere {
                    seat,
                    what: "a different number of actions had been taken",
                });
            }
        }
        self.apply_action(seat, action, kind, opened.event_hash, key, now_ms)
    }

    /// Apply an action that has been checked into place, and move the hand on.
    fn apply_action(
        &mut self,
        seat: SeatIdx,
        action: Action,
        kind: EventType,
        event_hash: Hash,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        {
            let Phase::Playing { play, .. } = &mut self.phase else {
                return Err(Failed::NothingFurther);
            };
            // The engine, and nothing else, decides whether this was legal. An
            // illegal action is refused and never corrected: `PROTOCOL.md` §4.7
            // says it "is not a state transition, and it never becomes one".
            play.round
                .apply(seat, action)
                .map_err(|what| Failed::Illegal { seat, what })?;
            if matches!(action, Action::Bet(_) | Action::Raise(_)) {
                play.aggressor = Some(seat);
            }
            play.actions = play.actions.saturating_add(1);
        }

        // A betting stage is single-writer, so its hash is fixed the moment its
        // one writer has been heard.
        self.slot = self.slot.then(stage_hash_single(
            self.slot.sequence,
            kind.code(),
            seat,
            event_hash,
        ));

        self.after_action(seat, key, now_ms)
    }

    /// Whose turn it is after a seat has acted — however it acted.
    ///
    /// Shared by the seat's own action and by a certificate acting for it, so
    /// the two cannot leave the round in different places.
    fn after_action(
        &mut self,
        seat: SeatIdx,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let seat_count = self.open.max_players;
        let next = {
            let Phase::Playing { play, .. } = &self.phase else {
                return Err(Failed::NothingFurther);
            };
            if only_one_live(&play.round, &play.dealt) || round_complete(&play.round, &play.dealt) {
                None
            } else {
                next_to_act(&play.round, &play.dealt, seat, seat_count)
            }
        };
        match next {
            Some(to_act) => {
                let Phase::Playing { play, .. } = &mut self.phase else {
                    return Err(Failed::NothingFurther);
                };
                play.step = Step::Acting { to_act };
                Ok(Vec::new())
            }
            None => self.close_round_and_open(key, now_ms),
        }
    }

    /// The betting round is over: bank what was committed, then open the board.
    fn close_round_and_open(
        &mut self,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let next = {
            let Phase::Playing { play, .. } = &mut self.phase else {
                return Err(Failed::NothingFurther);
            };
            // Everything committed this street joins the hand total, and the
            // round resets for the next one. This is the only place `paid` is
            // written, and `paid[s] + round.committed[s]` is what `build_pots`
            // will want — the engine holds only the per-round half.
            for seat in 0..play.paid.len() {
                play.paid[seat] += play.round.committed[seat];
                play.round.committed[seat] = 0;
                play.round.acted[seat] = false;
            }
            play.round.current_bet = 0;
            play.round.last_full_raise = play.round.big_blind;

            // Everybody but one has folded. No card needs opening, whatever
            // street it is, and there is no showdown: nobody has to show a hand
            // that nothing was called against, and nobody could open it anyway.
            if only_one_live(&play.round, &play.dealt) {
                After::Settle
            } else {
                match play.street.next() {
                    Some(street) => After::Board(street),
                    // The river's betting closed.
                    None => After::Showdown,
                }
            }
        };
        match next {
            After::Board(street) => self.open_board(street, key, now_ms),
            After::Showdown => self.begin_showdown(key, now_ms),
            After::Settle => self.begin_settlement(key, now_ms),
        }
    }

    /// The indices one street opens.
    fn street_indices(map: &DeckIndexMap, street: Street) -> Vec<CardIndex> {
        match street {
            Street::PreFlop => Vec::new(),
            Street::Flop => map.flop().to_vec(),
            Street::Turn => vec![map.turn()],
            Street::River => vec![map.river()],
        }
    }

    /// Open a `BOARD_REVEAL` stage for a street and publish this client's part.
    fn open_board(
        &mut self,
        street: Street,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let me = self.open.my_seat;

        // **A seat that is not dealt in opens the board with everybody else and
        // contributes no share to it.** §4.6: *"every peer has all `m` tokens
        // for those indices and opens the cards"* — the shares come from the
        // dealt-in seats and the board is public, which is what lets a follower
        // hold the same `PublicTableState` and therefore the same checkpoint.
        //
        // It holds no deck key, so `keys_by_seat(me)` is `None`, and this was
        // the last of the three `NotInThisStage` walls between such a seat and
        // its own settlement.
        if !self.mine.dealt_in.contains(&me) {
            let stage = Collective::closed(
                self.slot.sequence,
                EventType::BoardReveal.code(),
                &self.mine.dealt_in,
            )
            .ok_or(Failed::NotInThisStage)?;
            let Phase::Playing { play, .. } = &mut self.phase else {
                return Err(Failed::NothingFurther);
            };
            play.dealing.advance(RevealStage::Betting(street));
            play.step = Step::Opening { street, stage };
            return Ok(Vec::new());
        }

        let ctx = self.deck_ctx(&self.open.seats[self.seat_index()].1);
        let my_key = *self
            .keys_by_seat(me)
            .ok_or(Failed::NotInThisStage)?;

        let entries = {
            let Phase::Playing { deal, table, play } = &mut self.phase else {
                return Err(Failed::NothingFurther);
            };
            // The reveal stage moves first, or the shares for this street are
            // not yet due and `own_share` refuses them. `advance` never moves
            // backwards, which is what keeps a late message from reopening an
            // earlier street.
            play.dealing.advance(RevealStage::Betting(street));
            let identity = Identity {
                seat: me,
                key: &my_key,
                secret: &deal.secret,
            };
            let mut entries = Vec::new();
            for index in Self::street_indices(&table.map, street) {
                let (token, proof) = play
                    .dealing
                    .own_share(&deal.as_ref(table), &identity, index, &ctx)
                    .map_err(|e| refused(me, e))?;
                entries.push(RevealEntry {
                    deck_index: index.get(),
                    token: token.encode(),
                    proof: proof.encode(),
                });
            }
            entries
        };

        let body = BoardReveal {
            street: street_code(street),
            entries,
        };
        let bytes = self.say(EventType::BoardReveal, &body, BOARD_REVEAL_CAP, key, now_ms)?;
        let hash = self.opened(&bytes, EventType::BoardReveal)?.event_hash;
        let mut stage = Collective::closed(
            self.slot.sequence,
            EventType::BoardReveal.code(),
            &self.mine.dealt_in,
        )
        .ok_or(Failed::NotInThisStage)?;
        stage.hear(me, hash);

        let Phase::Playing { play, .. } = &mut self.phase else {
            return Err(Failed::NothingFurther);
        };
        play.step = Step::Opening { street, stage };
        Ok(vec![Send::Broadcast(bytes)])
    }

    /// One seat's contribution to a street's board cards.
    fn on_board_reveal(
        &mut self,
        bytes: &[u8],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, EventType::BoardReveal)?;
        let seat = self.seat_of(&opened.sender)?;
        self.note_signed(seat);
        let body: BoardReveal =
            chained::payload(&opened, BOARD_REVEAL_CAP).map_err(Failed::Wire)?;
        let ctx = self.deck_ctx(&opened.sender);
        let me = self.open.my_seat;

        let street = {
            let Phase::Playing { deal, table, play } = &mut self.phase else {
                return Err(Failed::NothingFurther);
            };
            let Step::Opening { street, stage } = &mut play.step else {
                return Err(Failed::Elsewhere {
                    seat,
                    what: "a board stage were open",
                });
            };
            let street = *street;
            if stage.heard(seat) == Some(opened.event_hash) {
                return Ok(Vec::new());
            }
            if street_from_code(body.street) != Some(street) {
                return Err(Failed::Elsewhere {
                    seat,
                    what: "another street were opening",
                });
            }
            let wanted: Vec<u8> = Self::street_indices(&table.map, street)
                .into_iter()
                .map(|i| i.get())
                .collect();
            if body.entries.iter().map(|e| e.deck_index).collect::<Vec<_>>() != wanted {
                return Err(Failed::BadToken {
                    seat,
                    why: "not exactly this street's indices",
                });
            }
            for entry in &body.entries {
                let index = table
                    .map
                    .index_from_wire(entry.deck_index)
                    .ok_or(Failed::BadToken {
                        seat,
                        why: "an index this hand gave no role to",
                    })?;
                let token = WireToken::decode(&entry.token).map_err(|_| Failed::BadToken {
                    seat,
                    why: "the share is not a point on the curve",
                })?;
                let proof =
                    WireTokenProof::decode(&entry.proof).map_err(|_| Failed::BadToken {
                        seat,
                        why: "the proof is not well formed",
                    })?;
                play.dealing
                    .accept(
                        &deal.as_ref(table),
                        me,
                        &Share {
                            from: seat,
                            index,
                            token,
                            proof: &proof,
                        },
                        &ctx,
                    )
                    .map_err(|e| refused(seat, e))?;
            }
            match stage.hear(seat, opened.event_hash) {
                Heard::Counted | Heard::Bystander | Heard::Again => {}
                Heard::Equivocation { .. } => return Err(Failed::Equivocation { seat }),
                // **Held, not refused.** By the time a contribution reaches a
            // collective stage its sender has already been resolved to a seat
            // at this table, so `Uninvited` does not mean "a stranger" — it
            // means this client derived an accepted set that does not hold
            // that seat, which after a certificate is a roster this client and
            // its neighbour disagree about by one entry. `run.rs` turns
            // anything but `NotYet` into a GossipSub `Reject`, and repeatedly
            // rejecting an honest peer is how it stops being forwarded.
            Heard::Uninvited => return Err(Failed::NotYet),
            }
            if !stage.complete() {
                return Ok(Vec::new());
            }
            street
        };

        // Every share is in: the cards come out, and only now.
        let parent = {
            let Phase::Playing { deal, table, play } = &mut self.phase else {
                return Err(Failed::NothingFurther);
            };
            for index in Self::street_indices(&table.map, street) {
                play.dealing
                    .open(&deal.as_ref(table), index)
                    .map_err(|_| Failed::BadToken {
                        seat: me,
                        why: "the board stage completed without every share",
                    })?;
            }
            let Step::Opening { stage, .. } = &play.step else {
                return Err(Failed::NothingFurther);
            };
            stage.hash().ok_or(Failed::NotInThisStage)?
        };
        self.slot = self.slot.then(parent);
        self.open_betting(street, key, now_ms)
    }

    /// The river's betting has closed: open the showdown.
    ///
    /// The order is TDA 17-A — the last aggressor on the river shows first, or
    /// the first seat to act on the river if it was checked through, and then
    /// clockwise. It decides **when** each client speaks and nothing else: the
    /// stage is collective and completes whenever every live seat has spoken.
    fn begin_showdown(&mut self, key: &SigningKey, now_ms: u64) -> Result<Vec<Send>, Failed> {
        let button = self.mine.button_position;
        let bb_seat = self.mine.bb_seat;
        let seat_count = self.open.max_players;

        let (live, order) = {
            let Phase::Playing { play, .. } = &self.phase else {
                return Err(Failed::NothingFurther);
            };
            let live: Vec<SeatIdx> = (0..seat_count)
                .filter(|s| {
                    let i = usize::from(*s);
                    play.dealt.get(i).copied().unwrap_or(false)
                        && !play.round.folded.get(i).copied().unwrap_or(true)
                })
                .collect();
            if live.len() < 2 {
                // One seat left. Nothing to compare and nothing to show, so the
                // hand goes straight to its settlement.
                return self.begin_settlement(key, now_ms);
            }
            // The aggressor, if there was one and it is still in the hand;
            // otherwise the seat that would have opened the river's betting.
            let first = play
                .aggressor
                .filter(|a| live.contains(a))
                .or_else(|| {
                    first_to_act(
                        Street::River,
                        &play.round,
                        &play.dealt,
                        button,
                        bb_seat,
                        seat_count,
                    )
                })
                .unwrap_or(live[0]);
            let at = live.iter().position(|s| *s == first).unwrap_or(0);
            let order: Vec<SeatIdx> = live
                .iter()
                .cycle()
                .skip(at)
                .take(live.len())
                .copied()
                .collect();
            (live, order)
        };

        let stage = Collective::closed(
            self.slot.sequence,
            EventType::ShowdownReveal.code(),
            &live,
        )
        .ok_or(Failed::NotInThisStage)?;

        {
            let Phase::Playing { play, .. } = &mut self.phase else {
                return Err(Failed::NothingFurther);
            };
            // The owner may publish its own share now, and only now.
            play.dealing
                .advance(RevealStage::Showdown { showing: live });
            play.step = Step::Showdown { stage, order };
        }
        self.speak_at_showdown(key, now_ms)
    }

    /// Speak at the showdown, if every seat ahead of this one has.
    ///
    /// This is the whole of the emission discipline. It is checked on every
    /// arrival rather than scheduled, because "the seats ahead of me have
    /// spoken" is a fact about the transcript and not about a timer.
    fn speak_at_showdown(
        &mut self,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let me = self.open.my_seat;
        let (my_place, first) = {
            let Phase::Playing { play, .. } = &self.phase else {
                return Ok(Vec::new());
            };
            let Step::Showdown { stage, order } = &play.step else {
                return Ok(Vec::new());
            };
            let Some(at) = order.iter().position(|s| *s == me) else {
                // Not in the showdown: folded, or not dealt in.
                return Ok(Vec::new());
            };
            if stage.heard(me).is_some() {
                return Ok(Vec::new());
            }
            // Everybody ahead has to have spoken. Not "the stage is nearly
            // complete" — a seat behind this one speaking early must not pull
            // this one forward, because the whole point is that this client
            // decides after seeing what it was entitled to see.
            if order[..at].iter().any(|s| stage.heard(*s).is_none()) {
                return Ok(Vec::new());
            }
            (at, order[0])
        };

        if self.shows_rather_than_mucks(my_place, first)? {
            self.show(key, now_ms)
        } else {
            self.muck(key, now_ms)
        }
    }

    /// Whether this client shows its hand or forfeits (D-021).
    ///
    /// The owner's words were *"if they find out they have lost, they muck"*, so
    /// a beaten hand mucks by default and nobody is asked. Three cases force a
    /// show, and all three are rules rather than preferences:
    ///
    /// * **First to show.** Somebody has to put a hand on the table.
    /// * **Anybody is all in.** TDA 16, and `PROTOCOL.md` §4.6 states it: with a
    ///   seat all in and the betting complete every live seat must show.
    /// * **Nobody has shown yet that this hand cannot beat.** A hand that is
    ///   winning or tied has no reason to muck and every reason not to.
    fn shows_rather_than_mucks(
        &self,
        my_place: usize,
        _first: SeatIdx,
    ) -> Result<bool, Failed> {
        if my_place == 0 {
            return Ok(true);
        }
        let Phase::Playing { play, .. } = &self.phase else {
            return Err(Failed::NothingFurther);
        };
        let Step::Showdown { order, .. } = &play.step else {
            return Err(Failed::NothingFurther);
        };
        // TDA 16: with anybody all in, nobody may muck.
        if order
            .iter()
            .any(|s| play.round.stack.get(usize::from(*s)).copied() == Some(0))
        {
            return Ok(true);
        }
        let Some(board) = five_card_board(&play.dealing.board()) else {
            // No complete board: this cannot be a river showdown, so there is
            // nothing to compare against and showing is the safe answer.
            return Ok(true);
        };
        // A seat with no cards has nothing to show and nothing to compare; it
        // is `folded` from the start of the hand and never reaches a showdown
        // decision about its own hand.
        let Some(my_cards) = play.cards else {
            return Ok(false);
        };
        let mine = evaluate_holdem(my_cards, &board);
        let best_shown = play
            .shown
            .iter()
            .flatten()
            .map(|hole| evaluate_holdem(*hole, &board))
            .max();
        Ok(match best_shown {
            // Ties show: a split pot is won by showing, not by mucking.
            Some(best) => mine >= best,
            None => true,
        })
    }

    /// Publish this client's own two shares, which opens its hand.
    fn show(&mut self, key: &SigningKey, now_ms: u64) -> Result<Vec<Send>, Failed> {
        let me = self.open.my_seat;
        let ctx = self.deck_ctx(&self.open.seats[self.seat_index()].1);
        let my_key = *self.keys_by_seat(me).ok_or(Failed::NotInThisStage)?;

        let (entries, cards) = {
            let Phase::Playing { deal, table, play } = &mut self.phase else {
                return Err(Failed::NothingFurther);
            };
            let indices = table.map.hole_cards(me).ok_or(Failed::NotInThisStage)?;
            let mut entries = Vec::new();
            for index in indices {
                // Computed straight from the deck rather than through
                // `own_share`, and this is the one place that is right.
                //
                // This client's own share of its own card has been in its own
                // token set since `DEAL_PRIVATE` — that is how it read the card
                // at all — and a set takes one share per seat and never an
                // update, so `own_share` would refuse it. What the wire needs is
                // not a second entry in the set; it is the same share, proved
                // again under the **showdown's** context, which is a different
                // sequence and therefore a different proof. The token is the
                // same value either way: it is a function of the secret and the
                // ciphertext, and nothing else.
                let (token, proof) = deal
                    .deck
                    .token(&deal.secret, &my_key, &table.deck, index, &ctx)
                    .map_err(|_| Failed::BadToken {
                        seat: me,
                        why: "this client could not compute its own share",
                    })?;
                entries.push(RevealEntry {
                    deck_index: index.get(),
                    token: token.encode(),
                    proof: proof.encode(),
                });
            }
            // Its own hand goes on the table here, because nothing else will
            // put it there: every other seat learns it from the message below,
            // and this client is not a receiver of its own messages.
            let Some(cards) = play.cards else {
                // Only a seat that was dealt cards reaches a showdown, and this
                // is the one place that would otherwise have to invent two.
                return Err(Failed::NotInThisStage);
            };
            if let Some(slot) = play.shown.get_mut(usize::from(me)) {
                *slot = Some(cards);
            }
            (entries, cards)
        };
        let _ = cards;

        let body = ShowdownReveal { entries };
        let bytes = self.say(
            EventType::ShowdownReveal,
            &body,
            SHOWDOWN_REVEAL_CAP,
            key,
            now_ms,
        )?;
        let hash = self.opened(&bytes, EventType::ShowdownReveal)?.event_hash;
        self.record_showdown(me, hash, true)?;
        let mut out = vec![Send::Broadcast(bytes)];
        out.append(&mut self.close_showdown_if_done(key, now_ms)?);
        Ok(out)
    }

    /// Forfeit every pot rather than show.
    ///
    /// A muck is the **absence** of a share, and this message only says so: the
    /// two shares that would open this hand are never published, so no peer —
    /// honest, modified, or all of them together — can open it. The forfeiture
    /// needs no enforcement for the same reason.
    fn muck(&mut self, key: &SigningKey, now_ms: u64) -> Result<Vec<Send>, Failed> {
        let me = self.open.my_seat;
        let body = ShowdownMuck { forfeit: true };
        let bytes = self.say(
            EventType::ShowdownMuck,
            &body,
            SHOWDOWN_MUCK_CAP,
            key,
            now_ms,
        )?;
        let hash = self.opened(&bytes, EventType::ShowdownMuck)?.event_hash;
        self.record_showdown(me, hash, false)?;
        let mut out = vec![Send::Broadcast(bytes)];
        out.append(&mut self.close_showdown_if_done(key, now_ms)?);
        Ok(out)
    }

    /// Note that a seat has spoken at the showdown.
    fn record_showdown(
        &mut self,
        seat: SeatIdx,
        hash: Hash,
        showed: bool,
    ) -> Result<(), Failed> {
        let Phase::Playing { play, .. } = &mut self.phase else {
            return Err(Failed::NothingFurther);
        };
        if !showed {
            if let Some(slot) = play.mucked.get_mut(usize::from(seat)) {
                *slot = true;
            }
        }
        let Step::Showdown { stage, .. } = &mut play.step else {
            return Err(Failed::NothingFurther);
        };
        match stage.hear(seat, hash) {
            Heard::Counted | Heard::Bystander | Heard::Again => Ok(()),
            Heard::Equivocation { .. } => Err(Failed::Equivocation { seat }),
            Heard::Uninvited => Err(Failed::NotInThisStage),
        }
    }

    /// One seat's showdown message.
    fn on_showdown(
        &mut self,
        bytes: &[u8],
        kind: EventType,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, kind)?;
        let seat = self.seat_of(&opened.sender)?;
        self.note_signed(seat);
        let ctx = self.deck_ctx(&opened.sender);
        let me = self.open.my_seat;

        {
            let Phase::Playing { play, .. } = &self.phase else {
                return Err(Failed::NothingFurther);
            };
            let Step::Showdown { stage, order } = &play.step else {
                return Err(Failed::Elsewhere {
                    seat,
                    what: "the showdown were open",
                });
            };
            if stage.heard(seat) == Some(opened.event_hash) {
                return Ok(Vec::new());
            }
            if !order.contains(&seat) {
                return Err(Failed::NotInThisStage);
            }
            // The first to show may not muck, and nobody may muck with a seat
            // all in (TDA 16). Both are refusals of the message, not of the
            // seat: a muck that is not allowed is not a muck.
            if kind == EventType::ShowdownMuck {
                if order.first() == Some(&seat) {
                    return Err(Failed::Elsewhere {
                        seat,
                        what: "it were not first to show",
                    });
                }
                if order
                    .iter()
                    .any(|s| play.round.stack.get(usize::from(*s)).copied() == Some(0))
                {
                    return Err(Failed::Elsewhere {
                        seat,
                        what: "nobody at this showdown were all in",
                    });
                }
            }
        }

        if kind == EventType::ShowdownMuck {
            let body: ShowdownMuck =
                chained::payload(&opened, SHOWDOWN_MUCK_CAP).map_err(Failed::Wire)?;
            if !body.forfeit {
                return Err(Failed::BadToken {
                    seat,
                    why: "a muck that does not forfeit is not a muck",
                });
            }
            self.record_showdown(seat, opened.event_hash, false)?;
        } else {
            let body: ShowdownReveal =
                chained::payload(&opened, SHOWDOWN_REVEAL_CAP).map_err(Failed::Wire)?;
            let cards = {
                let Phase::Playing { deal, table, play } = &mut self.phase else {
                    return Err(Failed::NothingFurther);
                };
                let indices = table.map.hole_cards(seat).ok_or(Failed::NotInThisStage)?;
                let wanted: Vec<u8> = indices.iter().map(|i| i.get()).collect();
                if body.entries.iter().map(|e| e.deck_index).collect::<Vec<_>>() != wanted {
                    return Err(Failed::BadToken {
                        seat,
                        why: "not exactly this seat's own two hole cards",
                    });
                }
                for entry in &body.entries {
                    let index =
                        table
                            .map
                            .index_from_wire(entry.deck_index)
                            .ok_or(Failed::BadToken {
                                seat,
                                why: "an index this hand gave no role to",
                            })?;
                    let token =
                        WireToken::decode(&entry.token).map_err(|_| Failed::BadToken {
                            seat,
                            why: "the share is not a point on the curve",
                        })?;
                    let proof = WireTokenProof::decode(&entry.proof).map_err(|_| {
                        Failed::BadToken {
                            seat,
                            why: "the proof is not well formed",
                        }
                    })?;
                    play.dealing
                        .accept(
                            &deal.as_ref(table),
                            me,
                            &Share {
                                from: seat,
                                index,
                                token,
                                proof: &proof,
                            },
                            &ctx,
                        )
                        .map_err(|e| refused(seat, e))?;
                }
                // Every share is in for those two indices — the `m-1` from
                // `DEAL_PRIVATE` and now the owner's — so the hand opens.
                let mut cards = Vec::with_capacity(2);
                for index in indices {
                    cards.push(play.dealing.open(&deal.as_ref(table), index).map_err(
                        |_| Failed::BadToken {
                            seat,
                            why: "the reveal did not complete this seat's cards",
                        },
                    )?);
                }
                let cards: [Card; 2] =
                    cards.try_into().map_err(|_| Failed::NotInThisStage)?;
                if let Some(slot) = play.shown.get_mut(usize::from(seat)) {
                    *slot = Some(cards);
                }
                cards
            };
            let _ = cards;
            self.record_showdown(seat, opened.event_hash, true)?;
        }

        // Somebody speaking may be what makes it this client's turn.
        let mut out = self.speak_at_showdown(key, now_ms)?;
        out.append(&mut self.close_showdown_if_done(key, now_ms)?);
        Ok(out)
    }

    /// If every live seat has spoken, the showdown is over.
    fn close_showdown_if_done(
        &mut self,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let parent = {
            let Phase::Playing { play, .. } = &self.phase else {
                return Ok(Vec::new());
            };
            let Step::Showdown { stage, .. } = &play.step else {
                return Ok(Vec::new());
            };
            if !stage.complete() {
                return Ok(Vec::new());
            }
            stage.hash().ok_or(Failed::NotInThisStage)?
        };
        self.slot = self.slot.then(parent);
        self.begin_settlement(key, now_ms)
    }

    /// The hand is decided: compute the settlement and publish this peer's copy.
    ///
    /// `STATE_MACHINE.md` splits this in two on purpose and so does this: the
    /// settlement is **computed and published** here, and **applied** only when
    /// the collective stage completes. A driver that awarded the pots on its own
    /// derivation would leave the chain with no terminal stage if a seat went
    /// quiet between the last reveal and this one.
    fn begin_settlement(&mut self, key: &SigningKey, now_ms: u64) -> Result<Vec<Send>, Failed> {
        // **Two different sets, and conflating them is what made this stage the
        // last wall.** `dealt_in` decides who is a party to the CRYPTOGRAPHY;
        // `required` decides who is a member of a STAGE. A dead-money seat is
        // in `required` and outside `dealt_in` — it emits `HAND_COMPLETE` like
        // everybody else, computed from public state, holding no cards. A seat
        // outside `required` altogether is in no `R` of this hand (§4.10: it
        // *"may legally emit exactly one chained event"*, and this is not it),
        // so it completes the stage by hearing the others and emits nothing.
        //
        // Unguarded it did both wrong: it sealed a `HAND_COMPLETE` no receiver
        // accepts, and `stage.hear` answered `Uninvited` for its own copy, so
        // the stage could never complete here and the settlement — and with it
        // the boundary checkpoint §4.9 readmits on — was unreachable.
        if !self.open.required.contains(&self.open.my_seat) {
            // It still DERIVES the settlement — from pots, stacks and a board
            // that are public by construction — because `on_hand_complete`
            // compares every arriving copy against its own and that comparison
            // is the whole of how a follower knows it is still in step. What it
            // does not do is seal one or count itself into the stage.
            let mine = Box::new(self.settlement()?);
            let stage = Collective::closed(
                self.slot.sequence,
                EventType::HandComplete.code(),
                &self.open.required,
            )
            .ok_or(Failed::NotInThisStage)?;
            let Phase::Playing { play, .. } = &mut self.phase else {
                return Err(Failed::NothingFurther);
            };
            play.step = Step::Settling { stage, mine };
            return Ok(Vec::new());
        }
        let mine = Box::new(self.settlement()?);
        let bytes = self.say(
            EventType::HandComplete,
            mine.as_ref(),
            HAND_COMPLETE_CAP,
            key,
            now_ms,
        )?;
        let hash = self.opened(&bytes, EventType::HandComplete)?.event_hash;
        // The required set is the set that emitted `HAND_INIT`, and it is
        // deliberately **not** narrowed within the hand: a seat that signed
        // stage 0 and then went quiet blocks this stage, and exactly one hand
        // pays for that — the one it went quiet in.
        let mut stage = Collective::closed(
            self.slot.sequence,
            EventType::HandComplete.code(),
            &self.open.required,
        )
        .ok_or(Failed::NotInThisStage)?;
        stage.hear(self.open.my_seat, hash);

        let Phase::Playing { play, .. } = &mut self.phase else {
            return Err(Failed::NothingFurther);
        };
        play.step = Step::Settling { stage, mine };
        let mut out = vec![Send::Broadcast(bytes)];
        out.append(&mut self.close_settlement_if_done()?);
        Ok(out)
    }

    /// Everything `HAND_COMPLETE` says, derived from this peer's own engine.
    fn settlement(&self) -> Result<HandComplete, Failed> {
        let button = self.mine.button_position;
        let seat_count = self.open.max_players;
        let n = usize::from(seat_count);
        let Phase::Playing { table, play, .. } = &self.phase else {
            return Err(Failed::NothingFurther);
        };

        // The per-hand commitments, which is what a pot is layered from. The
        // round is closed, so `paid` is the whole of it.
        let (pots, refunds) = build_pots(&play.paid, &play.round.folded);

        // Only the hands that were **shown** are ranked. A seat that folded or
        // mucked has no live hand here, and `award` treats `None` as exactly
        // that — which is how the forfeiture is enforced without a rule for it.
        let board = five_card_board(&play.dealing.board());
        let mut rank: Vec<Option<HandRank>> = vec![None; n];
        for (seat, hole) in play.shown.iter().enumerate() {
            if let (Some(hole), Some(board)) = (hole, board) {
                rank[seat] = Some(evaluate_holdem(*hole, &board));
            }
        }

        let mut won = vec![0 as Chips; n];
        let mut awards = Vec::with_capacity(pots.len());
        for pot in &pots {
            let payout = award(pot, &rank, button, seat_count);
            let mut winners: Vec<u8> = payout.iter().map(|(s, _)| *s).collect();
            winners.sort_unstable();
            // The seats that took a chip more than the even share, which is the
            // remainder handed out clockwise from the button. Named on the wire
            // because two peers that split a remainder differently disagree
            // about a stack for ever.
            let share = pot.size / payout.len().max(1) as Chips;
            let mut odd: Vec<u8> = payout
                .iter()
                .filter(|(_, c)| *c > share)
                .map(|(s, _)| *s)
                .collect();
            odd.sort_unstable();
            for (seat, chips) in payout {
                won[usize::from(seat)] += chips;
            }
            let mut eligible = pot.eligible.clone();
            eligible.sort_unstable();
            awards.push(PotAward {
                size: pot.size,
                eligible,
                winners,
                odd_chips: odd,
            });
        }

        let mut back = vec![0 as Chips; n];
        for r in &refunds {
            back[usize::from(r.seat)] += r.amount;
        }

        // What everybody started the hand with is `HAND_INIT`'s own `stacks`,
        // which every seat compared byte for byte at stage 0. Using the local
        // roster instead would make the deltas depend on a value nobody agreed.
        let mut final_stacks = Vec::with_capacity(n);
        let mut deltas = Vec::with_capacity(n);
        let mut busted = Vec::new();
        for seat in 0..n {
            let start = self.mine.stacks.get(seat).copied().unwrap_or(0);
            let end = play.round.stack.get(seat).copied().unwrap_or(0) + won[seat] + back[seat];
            if end == 0 && start > 0 {
                busted.push(seat as u8);
            }
            final_stacks.push(end);
            deltas.push(i64::try_from(end).unwrap_or(i64::MAX)
                - i64::try_from(start).unwrap_or(i64::MAX));
        }

        let state_hash = self.state_hash(table, play, &pots, &final_stacks)?;
        Ok(HandComplete {
            pots: awards,
            refunds: refunds
                .iter()
                .map(|r| Refund {
                    seat: r.seat,
                    amount: r.amount,
                })
                .collect(),
            deltas,
            final_stacks,
            busted,
            state_hash,
        })
    }

    /// The end-of-hand state hash, `PROTOCOL.md` §6.1's checkpoint 8.
    ///
    /// Three fields are the table's defaults rather than tracked quantities,
    /// and each is **correct** for what this client can form today rather than
    /// a placeholder: `ledger_out` is zero because no seat has left (there is no
    /// message that removes one yet), `sitting_out` is all false for the same
    /// reason, and `ante` comes from `HAND_INIT`, which is agreed. When a seat
    /// can leave, all three become real and this function is where they land.
    fn state_hash(
        &self,
        table: &Table,
        play: &Play,
        pots: &[crate::poker::pots::Pot],
        final_stacks: &[Chips],
    ) -> Result<Hash, Failed> {
        use crate::protocol::state_view::{PotView, PublicTableState, RosterEntry};

        let n = usize::from(self.open.max_players);
        let roster: Vec<RosterEntry> = self
            .open
            .seats
            .iter()
            .map(|(seat, k, _)| RosterEntry {
                seat: *seat,
                app_public_key: *k,
                stack: final_stacks.get(usize::from(*seat)).copied().unwrap_or(0),
            })
            .collect();

        let view = PublicTableState {
            protocol_version: crate::protocol::messages::PROTOCOL_VERSION,
            table_id: self.open.table_id,
            hand_id: self.open.hand_id,
            checkpoint: 8,
            roster,
            button_position: self.mine.button_position,
            sb_position: self.mine.sb_position,
            bb_seat: self.mine.bb_seat,
            level: u32::from(self.mine.level),
            small_blind: self.mine.small_blind,
            big_blind: self.mine.big_blind,
            ante: self.mine.ante,
            street: street_code(play.street),
            board: play.dealing.board().iter().map(|c| c.index()).collect(),
            committed_this_round: play.round.committed.clone(),
            committed_this_hand: play.paid.clone(),
            folded: play.round.folded.clone(),
            all_in: play.round.stack.iter().map(|c| *c == 0).collect(),
            acted_this_round: play.round.acted.clone(),
            sitting_out: vec![false; n],
            current_bet: play.round.current_bet,
            last_full_raise: play.round.last_full_raise,
            player_to_act: None,
            pots: pots
                .iter()
                .map(|p| {
                    let mut eligible = p.eligible.clone();
                    eligible.sort_unstable();
                    PotView {
                        size: p.size,
                        eligible,
                    }
                })
                .collect(),
            deck_commitment: deck_hash(table.deck.as_ref().as_ref().as_ref()),
            ledger_in: self.mine.stacks.iter().sum(),
            ledger_out: 0,
            transcript_head: self.slot.previous_event_hash,
        };
        view.state_hash()
            .map_err(|_| Failed::Wire(WireError::Unencodable("the end-of-hand state")))
    }

    /// One seat's copy of the settlement.
    fn on_hand_complete(&mut self, bytes: &[u8]) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, EventType::HandComplete)?;
        let seat = self.seat_of(&opened.sender)?;
        self.note_signed(seat);
        let theirs: HandComplete =
            chained::payload(&opened, HAND_COMPLETE_CAP).map_err(Failed::Wire)?;

        let Phase::Playing { play, .. } = &mut self.phase else {
            return Err(Failed::NothingFurther);
        };
        let Step::Settling { stage, mine } = &mut play.step else {
            return Err(Failed::Elsewhere {
                seat,
                what: "the settlement were open",
            });
        };
        if stage.heard(seat) == Some(opened.event_hash) {
            return Ok(Vec::new());
        }
        // Every field recomputed, and compared as a whole. There is no writer
        // here: a body that differs anywhere means two engines disagree about
        // the hand, which is a divergence and not a preference.
        if theirs != **mine {
            // **And say what differs — the field, not the body.** "Seat N holds
            // a different settlement" named the peer and nothing else, so the
            // one occurrence a day of running produced could not be told from
            // any other. The first repair printed two `final_stacks` vectors
            // and two pot counts, which covers two of the six fields: a hand
            // that disagreed only about `deltas`, `busted` or `state_hash`
            // printed two identical vectors under the word "disagreement",
            // which reads as a broken instrument rather than as a difference
            // somewhere the instrument does not look.
            let what = mine.disagreement(&theirs);
            self.settle_note = Some(if what.is_empty() {
                // Unreachable while `PartialEq` and `disagreement` enumerate the
                // same fields, and reachable the moment a field is added to one
                // and not the other. Said out loud, because a report that goes
                // quiet is how that survives to a second occurrence.
                format!(
                    "settlement disagreement with seat {seat} in a field this report does not enumerate                      - HandComplete::disagreement is missing a field the derive compares"
                )
            } else {
                // **And where this client's own view stands.** Two state
                // hashes name a disagreement and nothing else; the sequence
                // and the head this client hashed into `transcript_head` let
                // two nodes' notes be lined up offline. Measured before this:
                // one hand in two camps with every chip equal, and no way to
                // say from nine logs which field differed.
                format!(
                    "settlement disagreement with seat {seat}: {}; my view is at sequence {} with head {}",
                    what.join("; "),
                    self.slot.sequence,
                    self.slot.previous_event_hash[..4]
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                )
            });
            // **Noted, not refused — and that one word is the whole of `S1-BD`.**
            //
            // This returned here, before `stage.hear`. So a disagreeing
            // settlement never entered the stage; the stage never completed;
            // `close_settlement_if_done` never ran; `checkpoint8` stayed `None`;
            // no checkpoint-8 `STATE_HASH` was ever emitted — and the two
            // differing values never met at the checkpoint that exists to
            // compare them. §6.3's freeze is fully implemented and was
            // unreachable from above: measured, 124 settlement disagreements in
            // one run and **zero** freeze or reconciliation lines. The hand
            // ended by abort instead, the pot was restored rather than awarded,
            // and that is `D-026` violated by doing nothing — a hand in
            // progress cancelled with nobody paid.
            //
            // **Hearing it cannot corrupt anything, which is what makes this
            // safe.** `close_settlement_if_done` applies `mine.final_stacks` —
            // this client's OWN settlement — and publishes this client's own
            // `state_hash`. It never adopts the other seat's numbers. So a
            // differing copy does not move a chip here; it lets the stage
            // close, which is what publishes the checkpoint, which is what
            // surfaces the difference where §6.3 can act on it.
            //
            // The report above is kept and is now the only thing that refusal
            // was really achieving.
        }
        match stage.hear(seat, opened.event_hash) {
            Heard::Counted | Heard::Bystander | Heard::Again => {}
            Heard::Equivocation { .. } => return Err(Failed::Equivocation { seat }),
            // **Held, not refused.** By the time a contribution reaches a
            // collective stage its sender has already been resolved to a seat
            // at this table, so `Uninvited` does not mean "a stranger" — it
            // means this client derived an accepted set that does not hold
            // that seat, which after a certificate is a roster this client and
            // its neighbour disagree about by one entry. `run.rs` turns
            // anything but `NotYet` into a GossipSub `Reject`, and repeatedly
            // rejecting an honest peer is how it stops being forwarded.
            Heard::Uninvited => return Err(Failed::NotYet),
        }
        self.close_settlement_if_done()
    }

    /// If everybody has published the same settlement, apply it.
    fn close_settlement_if_done(&mut self) -> Result<Vec<Send>, Failed> {
        let parent = {
            let Phase::Playing { play, .. } = &self.phase else {
                return Ok(Vec::new());
            };
            let Step::Settling { stage, .. } = &play.step else {
                return Ok(Vec::new());
            };
            if !stage.complete() {
                return Ok(Vec::new());
            }
            stage.hash().ok_or(Failed::NotInThisStage)?
        };
        self.slot = self.slot.then(parent);

        let Phase::Playing { play, .. } = &mut self.phase else {
            return Err(Failed::NothingFurther);
        };
        let Step::Settling { mine, .. } = &play.step else {
            return Err(Failed::NothingFurther);
        };
        // Applied now, and not when it was computed. The stacks in `round` are
        // what every later question reads.
        for (seat, end) in mine.final_stacks.iter().enumerate() {
            if let Some(slot) = play.round.stack.get_mut(seat) {
                *slot = *end;
            }
        }
        // **Held before the step that computed it goes.** `mine.state_hash` is
        // §6.1's hash of the settled state and `parent` is `TERMINAL(k)`; the
        // next line drops the `Step` that carries the first, and the slot moves
        // past the second on the following hand. See the field's own note.
        self.checkpoint8 = Some((diverge_if_asked(mine.state_hash, self.open.hand_id), parent));
        play.step = Step::Ended;
        Ok(Vec::new())
    }

    /// The boundary checkpoint's value and the stage it chains from, once the
    /// hand has settled. `None` before that, and on the aborted path, which
    /// §6.2 row 8 also places a checkpoint on and which is not built yet.
    pub fn checkpoint8(&self) -> Option<(Hash, Hash)> {
        self.checkpoint8
    }

    /// This client's copy of the boundary checkpoint's `STATE_HASH` (§4.9).
    ///
    /// **Why the hand emits this at all, and it is not only about divergence.**
    /// §6.1 calls the state hash *"the only way a silent divergence is ever
    /// caught"*, which is what this stage is usually queued under. It is also
    /// the door back: §4.9's readmission set `A` is written by a seat signing
    /// *a checkpoint-8 `STATE_HASH` of chain `k` that agrees*, and without it a
    /// seat that misses one hand is outside `P(k)`, is therefore not dealt into
    /// `k+1` (§4.4), and cannot sign its way back in. Measured — a client that
    /// reconnected, rejoined the group and never played again. That is `S1-O`.
    ///
    /// `None` until the hand has settled: there is nothing to hash before then.
    ///
    /// The slot is **named, not walked** — `sequence = BOUNDARY_CHECKPOINT_BASE`
    /// with `previous_event_hash = TERMINAL(k)`, which is §4.9's rule and eight
    /// thousand stages above anything the hand itself reached.
    pub fn state_hash_event(
        &self,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Option<Vec<u8>>, Failed> {
        let Some((state_hash, terminal)) = self.checkpoint8 else {
            return Ok(None);
        };
        let Some(sequence) = crate::table::checkwire::hash_sequence(0) else {
            return Ok(None);
        };
        let body = crate::table::checkwire::StateHash {
            checkpoint: crate::table::checkwire::BOUNDARY_CHECKPOINT,
            state_hash,
            transcript_head: terminal,
        };
        let slot = self.slot.at(sequence, terminal);
        let bytes = chained::seal(
            EventType::StateHash,
            &slot,
            &body,
            key,
            now_ms,
            // **The checkpoint arms no deadline.** Every other stage's number
            // says how long the next one may take; this one has no next stage
            // yet — its `STATE_ACK` follows when the hash stage completes — and
            // §4.9 accepts a copy of it until `TERMINAL(k+1)` is fixed, which
            // is a window the hand cannot name because it belongs to the hand
            // after. A deadline here would be a promise about somebody else's
            // clock.
            0,
            STATE_HASH_CAP,
        )
        .map_err(Failed::Wire)?;
        Ok(Some(bytes))
    }

    /// The blind level this hand is played at, and the two blinds themselves.
    ///
    /// Said out loud when a hand opens, because a player has to be able to see
    /// the blinds go up — and because the escalation stood still for the whole
    /// life of this client without anything in a log being wrong.
    pub fn level(&self) -> u16 {
        self.open.level
    }
    pub fn small_blind(&self) -> Chips {
        self.open.small_blind
    }
    pub fn big_blind(&self) -> Chips {
        self.open.big_blind
    }

    /// The table this hand belongs to.
    pub fn table_id(&self) -> Hash {
        self.open.table_id
    }

    /// Which seat this client sits in.
    pub fn my_seat(&self) -> SeatIdx {
        self.open.my_seat
    }

    /// Which seat holds an application key, if any.
    ///
    /// `seat_of` answers the same question and refuses with `NotAtThisTable`,
    /// which is right inside a stage. This one is for a caller **outside** any
    /// stage — a checkpoint-8 `STATE_HASH` of a finished hand — where a key
    /// that is not on the roster is a message to ignore rather than a fault to
    /// report.
    pub fn seat_of_key(&self, key: &[u8; 32]) -> Option<SeatIdx> {
        self.open
            .seats
            .iter()
            .find(|(_, k, _)| k == key)
            .map(|(seat, _, _)| *seat)
    }

    /// `P(k)` as seat indices: the seats this client accepted a chained event
    /// from during this hand.
    ///
    /// The required emitter set of the boundary checkpoint's `STATE_HASH`
    /// stage (§4.9: *"at checkpoint 8 it is `P(k)`"*), and the set §4.4 draws
    /// the next hand's `dealt_in` from.
    /// Whether this seat took part in this hand, for the purpose of deriving
    /// the **next** one.
    ///
    /// Where a certificate is possible — three seats or more, D-023's floor —
    /// absence means **certified** absence and nothing else, because a
    /// certificate is an artefact every peer holds and agrees about by
    /// construction. Heads-up there can be no certificate at all and this falls
    /// back to observation, which is safe for the only reason it is ever safe:
    /// with one other peer there is nobody to disagree with.
    ///
    /// # This is not the same question `participants` answers, and that is `S1-V`
    ///
    /// [`participants`](Self::participants) returns the raw `signed` set — who
    /// was **heard from** — and it is what §4.9 makes the boundary checkpoint's
    /// required set. It was also §6.1 field 28 until that field was deleted for
    /// being an observation of the listener rather than a fact about the hand.
    /// This method is what `R(k+1)` and the whole `grace`/`present_run` fold
    /// read.
    ///
    /// So the quantity every peer **compares** at the checkpoint and the
    /// quantity every peer **acts on** at the next hand are different
    /// quantities, and they differ exactly at a seat that was certified absent
    /// while its events were nonetheless heard, or the reverse.
    ///
    /// # The reason given for leaving it open was false, and it was checked
    ///
    /// This comment used to end: *"changing `participants` would move
    /// `state_hash`, which is a §6.1 wire change"*. **It would not**, and since
    /// field 28 was deleted the question no longer arises at all:
    /// [`state_hash`](Self::state_hash) reads neither. It used to fill
    /// `signed_this_hand` from
    /// [`heard_from_flags`](Self::heard_from_flags); `participants()` had no
    /// caller inside it, and
    /// outside this file it is read only by `run.rs` to build §4.9's required
    /// emitter set. Changing it moves `P(k)` and leaves §6.1 field 28
    /// byte-identical. The `S1-V` row inherited the same false sentence and both
    /// are corrected.
    ///
    /// **And a §6.1 change would not be a cost even if it were one.**
    /// `PROTOCOL.md` §10.2: `PROTOCOL_MAJOR = 1` has not shipped and no peer is
    /// emitting the struct, so an edit today is a revision of version 1's
    /// definition rather than a break — free now, and forbidden after release.
    ///
    /// # What is actually open, which is narrower
    ///
    /// With the false blocker gone, the two predicates stop looking like two
    /// answers to one question. §4.9 defines `P(k)` as *the seats this client
    /// accepted a chained event from*, which is `signed` — so `participants()`
    /// is literally what the specification asks for and is not a candidate for
    /// change. `took_part` answers a different question: whether a seat was a
    /// party to hand `k` for the purpose of deriving hand `k+1`.
    ///
    /// So what remains is only whether `R(k+1)` should be derived from
    /// certification or from having been heard, and that is `Q-10` — *"nothing
    /// ratifies the stalled stage, and no construction can"*. This method's
    /// value is unchanged in kind and sharper in shape: **both definitions
    /// are named functions written next to each other**, and all four readers
    /// go through one of them, where before there was a closure in one
    /// derivation and three separate reads of the raw field in the others.
    /// That does not answer `Q-10` and is not meant to; it makes answering it
    /// an edit to one call site rather than a search.
    /// Was this seat **heard from** during hand `k`?
    ///
    /// The other half of `S1-V`'s pair, and the one §4.9 asks for: `P(k)` is
    /// *the seats this client accepted a chained event from*, which is exactly
    /// this flag. [`participants`](Self::participants) is this predicate as a
    /// seat list and [`heard_from_flags`](Self::heard_from_flags) is the same
    /// thing in the shape §6.1 field 28 wants; all three are one fact.
    ///
    /// **It is deliberately not the same question as
    /// [`took_part`](Self::took_part)**, which sits directly below so the
    /// difference is read rather than reconstructed. This one is an
    /// observation — what *this* client heard — and `took_part` is an
    /// agreement — what every peer holds a certificate about. They diverge at
    /// a seat certified absent whose events were nonetheless heard, or the
    /// reverse, and which of them `R(k+1)` should be derived from is `Q-10`.
    ///
    /// Routing all four readers through these two names does not answer
    /// `Q-10`, and is not meant to. It makes the divergence a visible choice
    /// between two documented functions instead of a structural accident
    /// between a closure and three copies of a field read — so that when
    /// `Q-10` is answered, the edit is one call site rather than a search.
    ///
    /// An out-of-range seat is not heard from. That is the same answer the
    /// three separate field reads gave, kept deliberately: a seat index past
    /// `max_players` cannot have signed anything.
    pub fn heard_from(&self, seat: SeatIdx) -> bool {
        self.signed.get(usize::from(seat)).copied().unwrap_or(false)
    }

    /// [`heard_from`](Self::heard_from) for every seat, in seat order.
    ///
    /// §6.1 hashed `signed_this_hand` as a flag per seat rather than as a list,
    /// which is where this shape came from; field 28 is now deleted and
    /// [`state_hash`](Self::state_hash) no longer reads it.
    ///
    /// **So this accessor has no caller, and that is the point of leaving it
    /// documented rather than deleting it quietly.** Its only purpose was to
    /// feed the field, and the field was removed because it is an observation
    /// of the listener. A future caller that reaches for it is reaching for a
    /// per-receiver quantity; if the answer wanted is *who took part*, that is
    /// [`participants`](Self::participants), and if it is *who is required
    /// next*, that is [`took_part`](Self::took_part). It is still the
    /// stored vector in the wire's own ordering, which is why this returns it
    /// rather than rebuilding it: a rebuild that disagreed with the field by
    /// one element would move field 28 and be found at a boundary checkpoint,
    /// not here.
    pub fn heard_from_flags(&self) -> &[bool] {
        &self.signed
    }

    pub fn took_part(&self, seat: SeatIdx) -> bool {
        if self.open.required.len() >= 3 {
            !self.certified.contains(&seat)
        } else {
            self.heard_from(seat)
        }
    }

    pub fn participants(&self) -> Vec<SeatIdx> {
        (0..self.open.max_players)
            .filter(|seat| self.heard_from(*seat))
            .collect()
    }

    /// Give up on this hand: nobody produced what the stage needed in time.
    ///
    /// The **only** answer version 1 has to a seat going quiet in a
    /// cryptographic stage. There is no `TIMEOUT_VOTE` and no `TIMEOUT_CERT`
    /// (D-015), so nobody is named — `attributed` is empty — and no chip moves.
    /// The silent seat pays for it in the next hand instead, by being outside
    /// `signed_this_hand` and therefore outside `P(k)` (D-013).
    ///
    /// The abort chains from the **stalled** stage's own index, because a
    /// stalled stage has no `stage_hash` to chain from and there is no third
    /// option. `PROTOCOL.md` §5.2.1's slot key contains `event_type`, which is
    /// what stops this being an equivocation against a peer that already spoke
    /// at that sequence.
    pub fn abort_now(
        &mut self,
        why: Abort,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        if matches!(self.phase, Phase::Aborted(_)) {
            return Ok(Vec::new());
        }
        let body = HandAbort::on_deadline(self.mine.stacks.clone());
        let bytes = self.say(EventType::HandAbort, &body, HAND_ABORT_CAP, key, now_ms)?;
        self.give_up(why);
        Ok(vec![Send::Broadcast(bytes)])
    }

    /// `cause = 2`: the shuffle proof from `seat` does not hold, and here are
    /// the two frames that say so.
    ///
    /// Separate from [`abort_now`](Hand::abort_now) because the evidence goes
    /// only into the message and never into the phase, and because this is the
    /// one abort with no waiting in it: §4.10's gate for causes 2 and 3 is
    /// *accept at once*, so the hand ends on this message rather than at
    /// everybody's deadline ninety seconds later.
    ///
    /// The caller has already had the proof refused by the chain. This does not
    /// re-verify it — it is the accuser, and an accuser that could be talked out
    /// of its own finding by running it twice would be a different bug — but
    /// every **receiver** does, which is what makes the accusation checkable
    /// rather than believed.
    fn abort_bad_shuffle(
        &mut self,
        seat: SeatIdx,
        evidence: [Vec<u8>; 2],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        if matches!(self.phase, Phase::Aborted(_)) {
            return Ok(Vec::new());
        }
        let accused = self
            .open
            .seats
            .iter()
            .find(|(s, _, _)| *s == seat)
            .map(|(_, k, _)| *k)
            .ok_or(Failed::NotInThisStage)?;
        let body =
            HandAbort::on_bad_shuffle(accused, evidence, self.mine.stacks.clone());
        // Checked against this client's own rules before it is signed. An
        // emitter that sends what every receiver must refuse has ended nobody's
        // hand and told nobody why, and the two bounds `consistent` now carries
        // are exactly the ones an emitter can breach by accident.
        body.consistent(&self.mine.stacks)
            .map_err(|what| Failed::Elsewhere { seat, what })?;
        let bytes = self.say(EventType::HandAbort, &body, HAND_ABORT_CAP, key, now_ms)?;
        self.give_up(Abort::BadShuffle { seat });
        Ok(vec![Send::Broadcast(bytes)])
    }

    /// `cause = 3`: the reveal share from `seat` does not hold against the
    /// committed deck, and here is the frame that carries it.
    ///
    /// The reveal twin of [`abort_bad_shuffle`](Hand::abort_bad_shuffle), and
    /// everything said there applies: accepted at once by §4.10, so no
    /// certificate and no deadline, and safe only because every receiver redoes
    /// the check itself.
    ///
    /// One frame, not two. A reveal share carries its own token and its own
    /// proof; the deck they are checked against is the committed one, which
    /// every seat of the hand already holds.
    fn abort_bad_reveal(
        &mut self,
        seat: SeatIdx,
        evidence: Vec<u8>,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        if matches!(self.phase, Phase::Aborted(_)) {
            return Ok(Vec::new());
        }
        let accused = self
            .open
            .seats
            .iter()
            .find(|(s, _, _)| *s == seat)
            .map(|(_, k, _)| *k)
            .ok_or(Failed::NotInThisStage)?;
        let body = HandAbort::on_bad_reveal(accused, evidence, self.mine.stacks.clone());
        body.consistent(&self.mine.stacks)
            .map_err(|what| Failed::Elsewhere { seat, what })?;
        let bytes = self.say(EventType::HandAbort, &body, HAND_ABORT_CAP, key, now_ms)?;
        self.give_up(Abort::BadReveal { seat });
        Ok(vec![Send::Broadcast(bytes)])
    }

    /// A peer's abort.
    ///
    /// **Buffered, not refused**, until this receiver's own trigger is present.
    /// `PROTOCOL.md` §4.9's acceptance gate says so in those words for the
    /// hand-deadline path, and it is what stops a witness-independent terminal
    /// from becoming a one-message hand void: a peer cannot end everybody's
    /// hand by claiming a deadline that has not passed here. `Failed::NotYet`
    /// is exactly the buffer — the caller holds it and replays it.
    fn on_hand_abort(&mut self, bytes: &[u8], now_ms: u64) -> Result<Vec<Send>, Failed> {
        let opened = chained::open_in_hand(
            bytes,
            HAND_ABORT_CAP,
            EventType::HandAbort,
            &self.open.table_id,
            self.open.hand_id,
        )
        .map_err(Failed::Wire)?;
        let seat = self.seat_of(&opened.sender)?;

        // **A hand is decided or aborted, never both** (§4.10). A receiver
        // holding a complete `HAND_COMPLETE` stage discards every abort for
        // that hand, and `Step::Ended` is exactly that: the settlement stage
        // completed, which means every seat of `P(k-1)` published the same
        // one. `HAND_COMPLETE` wins because it is collective — a completing
        // one proves no seat was silent, which is the premise every abort
        // rests on.
        //
        // Discarded rather than refused: the race is legitimate and narrow, it
        // needs a peer's own deadline to expire between its `HAND_COMPLETE`
        // and the arrival of the last other copy, and the sender did nothing
        // wrong. Without this the abort was **applied**: `consistent` compares
        // `final_stacks` against `self.mine.stacks`, which is the hand's
        // START-of-hand stacks and does not move at settlement, so a late
        // abort passed every check and reverted a hand that had already paid
        // out — on that receiver alone, forking it from the table.
        if matches!(
            &self.phase,
            Phase::Playing { play, .. } if matches!(play.step, Step::Ended)
        ) {
            return Ok(Vec::new());
        }

        let body: HandAbort =
            chained::payload(&opened, HAND_ABORT_CAP).map_err(Failed::Wire)?;
        body.consistent(&self.mine.stacks)
            .map_err(|what| Failed::Elsewhere { seat, what })?;

        match body.cause {
            // The uncertified path. The gate: this receiver's **own** deadline
            // must have passed. Until it has, hold the message.
            //
            // **And `past_deadline` alone was not the whole of this
            // receiver's own judgement, which stranded eight seats of a
            // ten-seat table for ever.** `past_deadline` returns `false` for a
            // **betting** stage by construction, deliberately, because a
            // player thinking is legitimate. But a client in a betting stage
            // still forms an opinion about that stage running late — it is
            // `past_stage_deadline` that fires the `TIMEOUT_VOTE`, and it
            // covers every stage. So a client could vote that this stage had
            // timed out and, in the same breath, refuse a peer's assertion of
            // exactly the same thing. Two expressions of one judgement,
            // disagreeing.
            //
            // Measured, `split173908-10`: hand #4 reached a betting stage,
            // seat 6 gave up on its own clock at 188.3 s and said so, the
            // other seats' clocks ran out on seat 5 and they voted — 8/9 at
            // 225.5 s, where it stayed for the remaining 375 seconds. The
            // certificate needed seat 6's vote and seat 6 was no longer in the
            // hand to cast it (`S1-AQ`); seat 6's abort would have ended the
            // hand instead, and every one of the eight refused it here. Seats
            // 5 and 6 ran on to hand #10 while the eight sat in hand #4. The
            // table forked and neither half could recover.
            //
            // **`long_past_stage` and not `past_stage_deadline`, and the
            // factor of two is the point.** `past_stage_deadline` is the
            // instant the vote fires; accepting on it would end the hand
            // anonymously at the very millisecond the mechanism that could
            // have *named* the stalling seat began. `may_abandon` already
            // refuses to do that on the origination side — *"the better
            // mechanism gets to go first"* — and does it with exactly this
            // predicate. Using the same one here makes the two sides of one
            // rule agree: a peer gives up at `2 x next_deadline_for(owed)` and
            // a peer accepts that giving-up at the same point.
            //
            // **A union, never a replacement.** `past_stage_deadline` is not a
            // superset of `past_deadline`: it is `false` whenever `owed_type`
            // is `None`, and it ignores the whole-hand budget, so replacing
            // the call would delete the 55-minute backstop and re-create this
            // same permanent `NotYet` somewhere else. `long_past_stage`
            // returns `true` for `owed_type() == None`, and the `||` keeps
            // every acceptance there is today.
            //
            // Nothing here reads a field of the sender's message: the receiver
            // still decides on its own monotonic clock and against `§8.2`'s
            // normative deadline, so no per-receiver quantity reaches a hash
            // and D-012 is untouched.
            1 if body.attributed.is_empty() => {
                // **Three admissions, and the middle one is `S1-BS` option 1.**
                // The whole-hand budget admits unconditionally: it is §4.10's
                // literal trigger and the fifty-five-minute backstop. Twice the
                // stage budget admits unconditionally: D-026 gives the
                // certificate round the air between one budget and two, and no
                // more. One stage budget admits only while no certificate this
                // client is party to is one copy away — a vote cast about a
                // seat the open stage still waits for, or a certificate stage
                // open here. Before this a received bare abort was applied at
                // one budget of a cryptographic stage while this client's own
                // `may_abandon` waited to two when a certificate was possible:
                // the two sides of one rule disagreed, and a table whose vote
                // was one copy short ended the hand on the abort with nothing
                // banked at the seats that had not sealed yet. At a betting
                // stage `past_deadline` is false by construction, so the gate
                // there was already twice the budget; this changes the
                // cryptographic stages only. Bounded by `long_past_stage`, which
                // runs on a clock only a sequence move resets.
                let hand_budget_spent = now_ms.saturating_sub(self.opened_at_ms)
                    >= u64::from(self.open.hand_deadline_ms);
                let admitted = hand_budget_spent
                    || self.long_past_stage(now_ms)
                    || (self.past_deadline(now_ms) && !self.vote_joined());
                if !admitted {
                    if self.past_deadline(now_ms) && self.abort_hold_said != Some(self.slot.sequence) {
                        self.abort_hold_said = Some(self.slot.sequence);
                        let allowed = self
                            .owed_type()
                            .map(|o| u64::from(self.next_deadline_for(o)).saturating_mul(2))
                            .unwrap_or(0);
                        let age = now_ms.saturating_sub(self.stage_at_ms);
                        self.cert_note.push(format!(
                            "abort from seat {seat} held: a vote this client joined about seat(s) {:?} is open at stage {}, {} s to twice the budget",
                            self.waiting_for(),
                            self.slot.sequence,
                            allowed.saturating_sub(age) / 1_000
                        ));
                    }
                    return Err(Failed::NotYet);
                }
            }
            // §4.10: *accept at once* - the evidence carries its own disproof.
            // At once, and not on this peer's word: this client redoes the
            // verification over the two signed frames and accepts only if the
            // proof really does fail. Anything else and a seat could void any
            // hand by shouting `cause = 2` over a proof that is perfectly good.
            2 => self.bad_shuffle_holds(&body, seat)?,
            // The reveal half of the same rule, and the same gate: accepted
            // at once, and only after this client's own check over the frame
            // the accused signed says the share really is wrong.
            3 => self.bad_reveal_holds(&body, seat)?,
            // The certified-subject path (§4.10, D-023). A named subject is
            // accepted only against a certificate this client verified itself:
            // `certs` holds nothing it did not open, check for unanimity and
            // count. An abort whose certificate has not arrived yet is
            // **held**, not refused — the mesh does not order two messages, and
            // refusing here would turn ordinary weather into a fault.
            1 => {
                let Some(h) = body.cert_hash else {
                    return Err(Failed::Elsewhere {
                        seat,
                        what: "a named subject came with the certificate naming it",
                    });
                };
                // Held, not refused: the mesh does not order two messages, and
                // an abort whose certificate has not arrived yet is ordinary
                // weather.
                let Some(fact) = self.certs.get(&h).copied() else {
                    return Err(Failed::NotYet);
                };
                // **A certificate about somebody else is not a licence to name
                // this one.** A bare set of hashes could only answer "is this a
                // certificate?"; the fact answers "is it a certificate about
                // the seat you are naming, for a deadline that ends a hand?".
                if fact.kind != 2 {
                    return Err(Failed::Elsewhere {
                        seat,
                        what: "the certificate ended a hand rather than taking an action",
                    });
                }
                let names = self
                    .open
                    .seats
                    .iter()
                    .find(|(s, _, _)| *s == fact.subject_seat)
                    .map(|(_, k, _)| *k);
                if body.attributed.first().copied() != names {
                    return Err(Failed::Elsewhere {
                        seat,
                        what: "the seat named were the one its certificate is about",
                    });
                }
            }
            _ => {
                return Err(Failed::Elsewhere {
                    seat,
                    what: "this build implemented that cause",
                })
            }
        }

        self.give_up(Abort::Told { cause: body.cause });
        Ok(Vec::new())
    }

    /// Does the `cause = 2` evidence really disprove the proof it names?
    ///
    /// §4.10 lets this abort end the hand **at once**, with no deadline and no
    /// certificate behind it, which makes it the one message a hostile seat
    /// could use to void any hand it disliked. What stops that is that the
    /// permission is conditional on arithmetic every receiver redoes: the abort
    /// is accepted only when this client's own run over the two frames *fails*.
    /// A `cause = 2` over a good proof is refused, and the hand goes on.
    ///
    /// # What is checked, in order, and why the order is that
    ///
    /// 1. Two entries, the step then the proof. §4.10 bounds the count at two
    ///    and `consistent` has already refused anything else.
    /// 2. Both frames open **in this hand** - table identity, hand, catalogue
    ///    envelope and signature - and both are signed by the seat the abort
    ///    names. An accusation carrying somebody else's frames proves nothing
    ///    about the seat it names. Position is not checked, and must not be:
    ///    the accused's frames sit at the stalled stage, which is exactly where
    ///    this receiver's cursor is not.
    /// 3. This client is still shuffling and its chain can name the context. If
    ///    it cannot - the hand has moved on, or it never got this far - it
    ///    **holds** the abort rather than refusing it. Refusing would punish a
    ///    peer for this client's own position, and the hand's deadline is the
    ///    backstop, which is where this message left everybody before.
    /// 4. The argument, against **this client's own input deck**, at the context
    ///    its own chain derives, from the *proof's own signed* `sequence`. The
    ///    emitter of the abort supplies none of those three: it supplies two
    ///    frames the accused signed, and nothing it chooses enters the
    ///    verification.
    fn bad_shuffle_holds(&self, body: &HandAbort, from: SeatIdx) -> Result<(), Failed> {
        let [step_bytes, proof_bytes] = match body.evidence.as_slice() {
            [a, b] => [a, b],
            _ => {
                return Err(Failed::Elsewhere {
                    seat: from,
                    what: "a cause-2 abort carried the step and the proof it disputes",
                })
            }
        };
        let accused_key = body.attributed.first().copied().ok_or(Failed::Elsewhere {
            seat: from,
            what: "a cause-2 abort named the seat it accuses",
        })?;
        let accused = self
            .open
            .seats
            .iter()
            .find(|(_, k, _)| *k == accused_key)
            .map(|(s, _, _)| *s)
            .ok_or(Failed::Elsewhere {
                seat: from,
                what: "the seat it accuses were at this table",
            })?;

        // Opened by chain identity, not by position: the accused's frames are
        // at the stage the chain stalled on, which is not where this receiver's
        // cursor sits, and a strict parent check would refuse every one of them.
        let open_one = |bytes: &[u8], kind: EventType| -> Result<chained::Opened, Failed> {
            let o = chained::open_in_hand(
                bytes,
                FRAME_CAP,
                kind,
                &self.open.table_id,
                self.open.hand_id,
            )
            .map_err(Failed::Wire)?;
            if o.sender != accused_key {
                return Err(Failed::Elsewhere {
                    seat: from,
                    what: "both frames were signed by the seat the abort accuses",
                });
            }
            Ok(o)
        };
        let step = open_one(step_bytes, EventType::ShuffleStep)?;
        let proof = open_one(proof_bytes, EventType::ShuffleProof)?;

        let step_body: ShuffleStep =
            chained::payload(&step, SHUFFLE_STEP_CAP).map_err(Failed::Wire)?;
        let proof_body: ShuffleProof =
            chained::payload(&proof, SHUFFLE_PROOF_CAP).map_err(Failed::Wire)?;

        let Phase::Shuffling { deal, chain, .. } = &self.phase else {
            // Held, not refused. See point 3 above.
            return Err(Failed::NotYet);
        };
        if chain.whose_turn() != Some(accused) {
            return Err(Failed::NotYet);
        }
        // The deck the proof is *about*, and the two hashes the proof itself
        // names. Checking them here is not redundant with the argument: it is
        // what makes "these two frames belong together" a fact rather than an
        // assumption, and it costs a hash where the argument costs 42 ms.
        let deck = unflatten(&step_body.deck).ok_or(Failed::Elsewhere {
            seat: from,
            what: "the step it carries held fifty-two cards",
        })?;
        if proof_body.output_deck_hash != deck_hash(&deck)
            || proof_body.input_deck_hash != input_deck_hash(chain.last_verified())
        {
            // The two frames are not about each other, or not about this
            // client's chain. Either way this receiver cannot be the judge of
            // it, and the deadline remains the backstop.
            return Err(Failed::NotYet);
        }

        // **The proof's own `sequence`, which the accused signed.** Not the
        // abort emitter's and not this client's cursor: the accused chose it
        // when it signed, so an emitter cannot shift the context to make a good
        // proof look bad, and an accused that signed a wrong one produced a
        // proof that fails at every honest peer anyway - which is the same
        // finding by the same route.
        let ctx = chain
            .next_ctx(proof.envelope.sequence)
            .ok_or(Failed::NotYet)?;
        let verdict = match chain.last_verified() {
            None => deal.deck.verify_initial_shuffle(&deck, &proof_body.proof, &ctx),
            Some(prev) => deal
                .deck
                .verify_shuffle(prev.as_ref(), &deck, &proof_body.proof, &ctx),
        };
        match verdict {
            // The accusation is false: the proof holds here. Refused, and the
            // hand carries on - which is the whole reason this gate exists.
            Ok(_) => Err(Failed::Elsewhere {
                seat: from,
                what: "the proof it calls invalid does not verify at this client either",
            }),
            // Confirmed, by this client's own arithmetic over frames the
            // accused signed. §4.10's *accept at once*.
            Err(VerifyOutcome::Invalid(_)) => Ok(()),
            // **And this is why the verdict is matched rather than tested with
            // `is_err`.** `CouldNotVerify` is not a finding about the proof: it
            // is this client saying it could not run the check, and
            // `VerifyOutcome` exists to carry exactly that distinction — *never
            // evidence against anybody*. Treating it as confirmation would let
            // an accuser end a hand at any receiver whose own verifier was
            // unavailable, which is the failure mode the whole gate is here to
            // prevent, reached through the one arm that looks like agreement.
            //
            // Held, not refused: nothing is known to be wrong with the abort,
            // this client simply cannot judge it, and the hand's deadline
            // remains the backstop.
            Err(VerifyOutcome::CouldNotVerify(_)) => Err(Failed::NotYet),
        }
    }

    /// Does the `cause = 3` evidence really disprove the share it names?
    ///
    /// The reveal twin of [`bad_shuffle_holds`](Hand::bad_shuffle_holds), and
    /// the same rule decides it: §4.10 accepts causes 2 and 3 **at once**, so
    /// the abort ends the hand only when this client's own check over the frame
    /// the accused signed says the share really is wrong.
    ///
    /// # The three-way answer, and why it is three and not two
    ///
    /// * **Any entry `Invalid`** — the share is provably wrong against the
    ///   committed deck. Accept; the hand is over.
    /// * **Every entry verified** — the accusation is false. Refuse, and the
    ///   hand goes on. This is the direction the gate exists for.
    /// * **Anything else** — `NotDue`, `UnknownSeat`, `CouldNotVerify`, an
    ///   index or a token this client cannot decode. None of those is a finding
    ///   about the sender; they are findings about this receiver's own position
    ///   and equipment. **Hold**, and let the hand's deadline be the backstop.
    ///   Refusing would score down a peer for carrying a message this client
    ///   merely could not judge.
    ///
    /// The evidence may be any of the three reveal events, because all three
    /// carry `RevealEntry` lists and any of them can be wrong. Which one it is
    /// comes from peeking the frame, and the frame's own signed `sequence`
    /// builds the context — never this client's cursor, which is somewhere else
    /// by construction.
    fn bad_reveal_holds(&self, body: &HandAbort, from: SeatIdx) -> Result<(), Failed> {
        let [frame] = match body.evidence.as_slice() {
            [a] => [a],
            _ => {
                return Err(Failed::Elsewhere {
                    seat: from,
                    what: "a cause-3 abort carried exactly the one frame it disputes",
                })
            }
        };
        let accused_key = body.attributed.first().copied().ok_or(Failed::Elsewhere {
            seat: from,
            what: "a cause-3 abort named the seat it accuses",
        })?;
        let accused = self
            .open
            .seats
            .iter()
            .find(|(_, k, _)| *k == accused_key)
            .map(|(s, _, _)| *s)
            .ok_or(Failed::Elsewhere {
                seat: from,
                what: "the seat it accuses were at this table",
            })?;

        let (kind, _, _) = chained::peek(frame, PEEK_CAP).map_err(Failed::Wire)?;
        let cap = match kind {
            EventType::DealPrivate => DEAL_PRIVATE_CAP,
            EventType::BoardReveal => BOARD_REVEAL_CAP,
            EventType::ShowdownReveal => SHOWDOWN_REVEAL_CAP,
            _ => {
                return Err(Failed::Elsewhere {
                    seat: from,
                    what: "the frame it carries were a reveal at all",
                })
            }
        };
        // By chain identity, not position: the frame sits at the stage the
        // accused was at, which is not where this receiver's cursor is.
        let opened = chained::open_in_hand(
            frame,
            FRAME_CAP,
            kind,
            &self.open.table_id,
            self.open.hand_id,
        )
        .map_err(Failed::Wire)?;
        if opened.sender != accused_key {
            return Err(Failed::Elsewhere {
                seat: from,
                what: "the frame were signed by the seat the abort accuses",
            });
        }
        let entries: Vec<RevealEntry> = match kind {
            EventType::DealPrivate => {
                chained::payload::<DealPrivate>(&opened, cap)
                    .map_err(Failed::Wire)?
                    .entries
            }
            EventType::BoardReveal => {
                chained::payload::<BoardReveal>(&opened, cap)
                    .map_err(Failed::Wire)?
                    .entries
            }
            _ => {
                chained::payload::<ShowdownReveal>(&opened, cap)
                    .map_err(Failed::Wire)?
                    .entries
            }
        };

        // Whichever phase still holds a deck and a share store. Outside both,
        // this client has nothing to check against and holds.
        let (deal, table, dealing) = match &self.phase {
            Phase::Dealing { deal, table, dealing, .. } => (deal, table, &**dealing),
            Phase::Playing { deal, table, play } => (deal, table, &*play.dealing),
            _ => return Err(Failed::NotYet),
        };

        let ctx = self.deck_ctx_at(&accused_key, opened.envelope.sequence);
        let mut unjudgeable = false;
        for entry in &entries {
            let Some(index) = table.map.index_from_wire(entry.deck_index) else {
                unjudgeable = true;
                continue;
            };
            let (Ok(token), Ok(proof)) = (
                WireToken::decode(&entry.token),
                WireTokenProof::decode(&entry.proof),
            ) else {
                // Bytes that are not a point or not a proof are attributable
                // under §4.0 — but as a **tier-1** finding under `cause = 6`,
                // not as this cause. `cause = 3` is *this share does not verify*
                // and nothing else, so an undecodable one is held rather than
                // quietly promoted to a different accusation.
                unjudgeable = true;
                continue;
            };
            let share = Share {
                from: accused,
                index,
                token,
                proof: &proof,
            };
            match dealing.would_verify(&deal.as_ref(table), &share, &ctx) {
                Err(Refused::DidNotVerify(VerifyOutcome::Invalid(_))) => return Ok(()),
                Ok(()) => {}
                Err(_) => unjudgeable = true,
            }
        }

        if unjudgeable {
            // Nothing here is known to be wrong; this client simply could not
            // finish the check.
            return Err(Failed::NotYet);
        }
        Err(Failed::Elsewhere {
            seat: from,
            what: "the share it calls invalid verifies at this client",
        })
    }

    /// Whether this hand's own deadline has passed.
    ///
    /// From the envelope this client sealed its own stage-0 event with, which
    /// is the table's `hand_deadline_ms` and is therefore the same number at
    /// every seat. It is a **local** timer against an advisory clock: §8.2 says
    /// a deadline is measured on the peer's own monotonic clock and never on
    /// anybody's wall time, which is why nothing in the chain depends on two
    /// peers agreeing about when it passed.
    /// **Which** of the two budgets expired, in words a reader can act on.
    ///
    /// `abort_now(Abort::Deadline)` carries no reason, so the report said *"the
    /// hand ran out of time"* for a thirty-second stage stall as readily as for
    /// a fifty-five-minute hand -- and the two send a reader looking in
    /// completely different places. Measured in `split163641-10`: `n0` opened
    /// hand 1 at 108.3 s and aborted at 169.3 s, sixty seconds later, against a
    /// `hand_deadline_ms` of `RATED_HAND_DEADLINE_MS` = 3 300 000. Nothing in a
    /// three-minute-old process can have consumed fifty-five minutes, so the
    /// clock that fired was the stage's; the sentence named the other one.
    ///
    /// `None` when neither has expired, which is also the answer for a betting
    /// stage: `past_deadline` deliberately does not bound one, because a player
    /// thinking is legitimate and it is `action_timeout_ms` that answers it.
    pub fn expired_budget(&self, now_ms: u64) -> Option<String> {
        let waiting = self.waiting_for();
        let who = if waiting.is_empty() {
            String::new()
        } else {
            format!(", still waiting for seat(s) {waiting:?}")
        };
        let hand = now_ms.saturating_sub(self.opened_at_ms);
        if hand >= u64::from(self.open.hand_deadline_ms) {
            return Some(format!(
                "the hand's own budget: {} s of {} s{who}",
                hand / 1_000,
                self.open.hand_deadline_ms / 1_000,
            ));
        }
        if !self.crypto_stage() {
            return None;
        }
        let stage = now_ms.saturating_sub(self.stage_at_ms);
        if stage >= u64::from(self.open.crypto_step_timeout_ms) {
            return Some(format!(
                "this stage's budget: {} s of {} s{who} (the hand itself has used                  {} s of {} s)",
                stage / 1_000,
                self.open.crypto_step_timeout_ms / 1_000,
                hand / 1_000,
                self.open.hand_deadline_ms / 1_000,
            ));
        }
        None
    }

    fn past_deadline(&self, now_ms: u64) -> bool {
        // The hand's own budget, which every stage shares and which has to be
        // long enough for a whole legal hand of everybody thinking.
        if now_ms.saturating_sub(self.opened_at_ms) >= u64::from(self.open.hand_deadline_ms) {
            return true;
        }
        // And the stage's own, which is `crypto_step_timeout_ms` and is
        // `PROTOCOL.md` §8.2's number rather than one this client picked. It is
        // measured from **accepting the event that completed the previous
        // stage**, which is what `stage_at_ms` records, and §8.2 says that is
        // what removes clock synchronisation from the problem: two peers'
        // timers then differ by the propagation delay between them and not by
        // their clock offset.
        //
        // A **betting** stage is not bounded here. A player thinking is
        // legitimate, and it is bounded instead by that seat's own client
        // acting for it at `action_timeout_ms + action_grace_ms`.
        if !self.crypto_stage() {
            return false;
        }
        now_ms.saturating_sub(self.stage_at_ms) >= u64::from(self.open.crypto_step_timeout_ms)
    }

    /// Whether this hand may be given up on now.
    ///
    /// Public because the deadline is the **node's** to act on: it is measured
    /// on this peer's own monotonic clock and shared with nobody, so nothing
    /// inside the hand can ask what time it is. Polled rather than scheduled,
    /// because the answer changes as stages open and close and a single armed
    /// instant would have to be re-armed at every one of them.
    pub fn may_abandon(&self, now_ms: u64) -> bool {
        if self.over() || !self.past_deadline(now_ms) {
            return false;
        }
        // **The better mechanism gets to go first.** The stage deadline and the
        // vote fall due at the same moment, so without this the local abort
        // pre-empts the certificate on the very tick that produced the vote —
        // and the hand would end anonymously where it could have ended naming
        // the seat that stalled it. Where a certificate is achievable the abort
        // waits one more stage's worth; where it is not — heads-up, and below
        // the floor generally — it fires at once, because nothing better is
        // coming.
        // **A follower ends no hand, and the corpus says so rather than this
        // comment.** `required` decides who is a member of a **stage** —
        // `begin_settlement` says exactly that and enforces the identical test
        // one function away — and `PROTOCOL.md` §4.10 gives a seat outside
        // `P(k)` **exactly one** legal chained event, `PLAYER_SIT_IN`, with
        // §4.9's checkpoint-8 `STATE_HASH` the only exemption. `HAND_ABORT` is
        // on neither list, so a non-member emitting one is a wire violation and
        // not merely impolite.
        //
        // **And it is the fork.** Its own deadline is about a hand it is not a
        // party to, and the terminal it produces — `abort_terminal` — is hashed
        // under a different domain from the settled branch's `HAND_COMPLETE`
        // stage hash, so `GENESIS(k+1)` diverges from the table's the moment it
        // fires.
        //
        // Measured, `split110500-10`, ten seats across two machines: seats 1, 4,
        // 5 and 6 were certified out one per hand between 306 s and 413 s, each
        // **derived its own removal** — the log prints the shrinking `required`
        // set — and each went on to open three more hands and then abort hand 9
        // on its own deadline while the other six settled it. The adrift latch
        // caught them 0.8 s later, which is `ADRIFT_MARGIN` working; what it
        // could not do was stop the abort that made the fork inevitable.
        //
        // A non-member now waits for `Abort::Told` or for a complete
        // `HAND_COMPLETE` stage from the seats that are parties. If neither
        // comes it stalls, which is what the adrift latch is for.
        if !self.open.required.contains(&self.open.my_seat) {
            return false;
        }
        if self.certificate_possible() && !self.long_past_stage(now_ms) {
            return false;
        }
        true
    }

    /// Whether a certificate could still end the stage this hand is waiting on.
    fn certificate_possible(&self) -> bool {
        self.waiting_for()
            .iter()
            .any(|s| *s != self.open.my_seat && self.voters(*s).len() >= 2)
    }

    /// Twice the stage's own deadline: the window a certificate is given.
    fn long_past_stage(&self, now_ms: u64) -> bool {
        let Some(owed) = self.owed_type() else {
            return true;
        };
        let allowed = u64::from(self.next_deadline_for(owed)).saturating_mul(2);
        now_ms.saturating_sub(self.stage_at_ms) >= allowed
    }

    /// Whether the stage now open is one the cryptography has to complete.
    fn crypto_stage(&self) -> bool {
        match &self.phase {
            Phase::Init(_)
            | Phase::Deck { .. }
            | Phase::Shuffling { .. }
            | Phase::Committing { .. }
            | Phase::Dealing { .. } => true,
            // A hand being played is bounded by the action clock, and one that
            // has ended or is between phases is bounded by nothing because
            // there is nothing to wait for.
            Phase::Playing { play, .. } => !matches!(play.step, Step::Acting { .. }),
            Phase::Aborted(_) | Phase::Between => false,
        }
    }

    /// Note the clock against the stage now open.
    ///
    /// Called wherever an event has been handled, and restarts the stage clock
    /// only when the sequence actually moved — so a peer that is being sent
    /// duplicates cannot hold the stage open by re-sending, which is precisely
    /// what the re-send loop does every five seconds.
    fn mark_stage(&mut self, now_ms: u64) {
        if self.slot.sequence == self.stage_seq {
            return;
        }
        self.stage_seq = self.slot.sequence;
        self.stage_at_ms = now_ms;

        // Everything about the stage just left goes with it. A vote and a
        // certificate are bound to one stage — a vote for a stage nobody is
        // waiting on any more can never be counted, because the subject it
        // names cannot be rebuilt — and keeping them had two costs, one of
        // them fatal: `certify_if_unanimous` refuses to start while a
        // certificate is in progress, so a stage that opened one and moved on
        // without completing it **blocked every certificate for the rest of
        // the hand**. The other is only that the map grows.
        self.certifying = None;
        self.votes.clear();
        self.voted.clear();
    }

    /// `GENESIS(k)`: what this hand's first stage hangs off.
    ///
    /// Public so a client can say it. Two peers that opened one hand from
    /// different views of the formation differ here and nowhere a log would
    /// otherwise show.
    pub fn genesis(&self) -> Hash {
        self.open.genesis
    }

    /// The required emitter set this hand was opened with.
    pub fn required(&self) -> &[SeatIdx] {
        &self.open.required
    }

    /// Whether this hand has ended, however it ended.
    ///
    /// The two ways are not interchangeable — one moves chips and one does not
    /// — but for "is there anything left to wait for" they are the same answer,
    /// and a caller that asked only about the settled one would wait for ever
    /// on an aborted hand.
    pub fn over(&self) -> bool {
        self.betting_over() || self.aborted().is_some()
    }

    /// How long this hand has left before any peer may end it.
    pub fn hand_deadline(&self) -> std::time::Duration {
        std::time::Duration::from_millis(
            u64::from(self.open.hand_deadline_ms).clamp(30_000, 3_600_000),
        )
    }

    /// Whether this hand ended without being played out.
    pub fn aborted(&self) -> Option<Abort> {
        match &self.phase {
            Phase::Aborted(why) => Some(*why),
            _ => None,
        }
    }

    // ---------------------------------------------------------------------
    // The decision clock that is not this client's own word for it
    // ---------------------------------------------------------------------

    /// The subject this client would vote about, if its own timer has expired.
    ///
    /// One subject at a time and only the seats the open stage is waiting for.
    /// Everything in it is a function of the stage rather than of an opinion:
    /// two honest voters build the identical body and therefore the identical
    /// `subject_digest`, which is what lets their votes be counted together.
    fn subject_now(&self, seat: SeatIdx) -> Option<TimeoutVote> {
        let acting = matches!(
            &self.phase,
            Phase::Playing { play, .. } if matches!(play.step, Step::Acting { .. })
        );
        Some(TimeoutVote {
            subject_sequence: self.slot.sequence,
            subject_seat: seat,
            // A betting stage has five legal types and no expected one, so the
            // group base names the group (`PROTOCOL.md` §4.8).
            subject_event_type: if acting {
                ACTION_GROUP
            } else {
                self.stage_type()?
            },
            parent_event_hash: self.slot.previous_event_hash,
            // The parent stage's own `next_deadline_ms`, which is normative
            // (§8.2) — so a voter cannot choose a shorter one and a receiver
            // can check the value rather than trust it.
            deadline_ms: self.next_deadline_for(self.owed_type()?),
            kind: if acting { 1 } else { 2 },
        })
    }

    /// The event type the open stage is collecting, if it collects one.
    fn stage_type(&self) -> Option<u16> {
        Some(match &self.phase {
            Phase::Init(_) => EventType::HandInit.code(),
            Phase::Deck { .. } => EventType::DeckInit.code(),
            Phase::Shuffling { heard, .. } => {
                if heard.is_some() {
                    EventType::ShuffleProof.code()
                } else {
                    EventType::ShuffleStep.code()
                }
            }
            Phase::Committing { .. } => EventType::DeckCommit.code(),
            Phase::Dealing { .. } => EventType::DealPrivate.code(),
            Phase::Playing { play, .. } => match play.step {
                Step::Opening { .. } => EventType::BoardReveal.code(),
                Step::Showdown { .. } => EventType::ShowdownReveal.code(),
                Step::Settling { .. } => EventType::HandComplete.code(),
                Step::Acting { .. } => ACTION_GROUP,
                Step::Ended => return None,
            },
            Phase::Aborted(_) | Phase::Between => return None,
        })
    }

    /// The type whose deadline the open stage runs under.
    fn owed_type(&self) -> Option<EventType> {
        match self.stage_type()? {
            ACTION_GROUP => Some(EventType::ActionFold),
            other => EventType::try_from(other).ok(),
        }
    }

    /// The voter set for a subject: everybody dealt in but the subject, less
    /// the seats a completed certificate has already named.
    fn voters(&self, subject: SeatIdx) -> Vec<SeatIdx> {
        self.mine
            .dealt_in
            .iter()
            .copied()
            .filter(|s| *s != subject && !self.certified.contains(s))
            .collect()
    }

    /// Why this client is not voting, if a vote is owed and has not been cast.
    ///
    /// `None` while nothing is owed — the hand is over, nobody is waited for,
    /// or the stage is inside its deadline — or while this client has already
    /// voted about everyone it waits for. Otherwise every gate of
    /// `vote_on_timeouts`, with its value: measured in `split084211-10`, eight
    /// seats waited on one for 370 s, four voted within half a second of each
    /// other, four never did, and nothing in nine logs said which gate held
    /// them. Read by the node's stall tick, said at most every 30 s.
    pub fn vote_state(&self, now_ms: u64, mid_delivery: u32) -> Option<String> {
        if self.over() {
            return None;
        }
        let waiting = self.waiting_for();
        if waiting.is_empty() {
            return None;
        }
        let owed = self.owed_type();
        let deadline = owed.map(|o| u64::from(self.next_deadline_for(o)));
        let age = now_ms.saturating_sub(self.stage_at_ms);
        if deadline.is_some_and(|d| age < d) {
            return None;
        }
        let mut owing = false;
        let seats: Vec<String> = waiting
            .iter()
            .map(|s| {
                let subject = self.subject_now(*s);
                let digest = subject.as_ref().map(|sub| sub.subject_digest());
                let voted = digest.as_ref().is_some_and(|d| self.voted.contains(d));
                let held = digest
                    .as_ref()
                    .and_then(|d| self.votes.get(d))
                    .map(|m| m.len())
                    .unwrap_or(0);
                if !voted && *s != self.open.my_seat {
                    owing = true;
                }
                format!(
                    "seat {s}: subject {}, voters {:?}, I voted {voted}, votes held {held}, mid-delivery {}",
                    if subject.is_some() { "yes" } else { "NONE" },
                    self.voters(*s),
                    (mid_delivery >> u32::from((*s).min(31))) & 1 == 1
                )
            })
            .collect();
        if !owing {
            return None;
        }
        Some(format!(
            "not voting: stage {} open {} s, deadline {:?} s for {:?}, long past {}, my seat {}, sequence {}; {}",
            self.stage_seq,
            age / 1_000,
            deadline.map(|d| d / 1_000),
            owed,
            self.long_past_stage(now_ms),
            self.open.my_seat,
            self.slot.sequence,
            seats.join("; ")
        ))
    }

    /// A certificate this client is party to is one copy away: this client
    /// has voted about a seat the open stage still waits for, or a
    /// certificate stage is open here. Stage-local by construction — the
    /// digest commits to the subject's sequence and parent, and
    /// `mark_stage` clears all of it when the sequence moves.
    fn vote_joined(&self) -> bool {
        if self.certifying.is_some() {
            return true;
        }
        self.waiting_for().iter().any(|s| {
            self.subject_now(*s)
                .is_some_and(|sub| self.voted.contains(&sub.subject_digest()))
        })
    }

    /// Vote about every seat this client's own timer has run out on.
    ///
    /// Called by the node, because the deadline is measured on the node's own
    /// monotonic clock and the hand cannot ask what time it is. A vote says
    /// only *"my timer expired and I have accepted nothing from that seat at
    /// this stage"* — it is not an accusation, it does nothing alone, and it is
    /// not evidence against anybody until a complete set exists.
    /// `mid_delivery` is a bit per seat, set where the carrier is still
    /// delivering that seat's traffic — see `Trouble::mid_delivery`. Pass 0
    /// where there is no carrier to ask.
    pub fn vote_on_timeouts(
        &mut self,
        key: &SigningKey,
        now_ms: u64,
        mid_delivery: u32,
    ) -> Result<Vec<Send>, Failed> {
        if self.over() || !self.past_stage_deadline(now_ms) {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for seat in self.waiting_for() {
            // Never about oneself, and never twice.
            if seat == self.open.my_seat {
                continue;
            }
            // **A seat the carrier is mid-delivery with is late, not silent.**
            //
            // `S1-BK`. A bit here means messages from this seat are sitting in
            // the receive array: they arrived out of order, so the seat IS
            // sending and something earlier has not landed yet. Accusing it of
            // not speaking is accusing it of the carrier's backlog, and the
            // stage clock alone cannot tell those apart — this is the only
            // fact in the system that can.
            //
            // **Bounded, on purpose.** A seat could otherwise buy silence for
            // ever by sending later messages while withholding the one a stage
            // needs. Past `long_past_stage` — twice the budget, which is just
            // beyond the point where the carrier itself gives a confirmed peer
            // up at 58 s — the suppression stops and the vote goes ahead
            // whatever the carrier says. `D-026` is what that bound is for.
            if !self.long_past_stage(now_ms)
                && mid_delivery & (1u32 << u32::from(seat.min(31))) != 0
            {
                continue;
            }
            let Some(subject) = self.subject_now(seat) else {
                continue;
            };
            let digest = subject.subject_digest();
            if self.voted.contains(&digest) {
                continue;
            }
            // Below the floor a certificate has no effect whatever, so a vote
            // towards one is noise on the wire and an invitation to an
            // implementer to reach for a quorum. It is not sent at all.
            if self.voters(seat).len() < 2 {
                continue;
            }
            // **And this client has to be one of the voters.**
            //
            // `on_timeout_vote` already refuses a vote from a seat outside
            // `voters(subject)` — *“if !self.voters(body.subject_seat)
            // .contains(&seat)”* — but the emit side had no such test, and
            // `take_vote` below inserts `my_seat` unconditionally. So a seat the
            // table had already certified out went on voting **for itself**,
            // `certify_if_unanimous` saw `held = |voters| + 1`, sealed, and
            // broadcast a certificate carrying a voter nobody else has in
            // `dealt_in`. Every receiver then refused it at `on_timeout_cert`'s
            // `c.voters.is_subset(&nominal)` and this client's own copy of the
            // hand was the only one that could accept it.
            //
            // Measured, `split215300-10`: **443** certificates refused
            // table-wide with *“acting as though every voter were dealt in”*,
            // and every one of them from a seat that had been certified out —
            // **394 from seat 2, 25 from seat 0, 24 from seat 9**, against a
            // roster whose narrowings were `certified [2]`, `[4]`, `[7]`, `[8]`
            // and `[9, 0]`. The fingerprint is a tally over its own
            // denominator: `4/3`, `7/6`, `8/7`, `9/8`, `5/4` — always exactly
            // one too many, always this client's own.
            //
            // The same predicate as the receive path, applied where the vote is
            // made rather than where it lands.
            if !self.voters(seat).contains(&self.open.my_seat) {
                continue;
            }
            let bytes = self.say_at(
                EventType::TimeoutVote,
                &subject,
                TIMEOUT_VOTE_CAP,
                key,
                now_ms,
            )?;
            self.voted.insert(digest);
            self.take_vote(digest, self.open.my_seat, bytes.clone(), subject);
            self.tally = Some((
                seat,
                self.votes.get(&digest).map(|m| m.len()).unwrap_or(0),
                self.voters(seat).len(),
                digest,
            ));
            out.push(Send::Broadcast(bytes));
            out.append(&mut self.certify_if_unanimous(key, now_ms)?);
        }
        Ok(out)
    }

    /// Record one vote, whoever it came from.
    fn take_vote(&mut self, digest: Hash, from: SeatIdx, bytes: Vec<u8>, _v: TimeoutVote) {
        self.votes.entry(digest).or_default().insert(from, bytes);
    }

    /// A vote from a peer.
    fn on_timeout_vote(
        &mut self,
        bytes: &[u8],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, EventType::TimeoutVote)?;
        let seat = self.seat_of(&opened.sender)?;
        let body: TimeoutVote =
            chained::payload(&opened, TIMEOUT_VOTE_CAP).map_err(Failed::Wire)?;

        // A voter may not vote about itself, and may not vote about a stage
        // this client is not at — the second is what stops a replay of a vote
        // into a different position in the hand.
        if body.subject_seat == seat {
            return Err(Failed::Elsewhere {
                seat,
                what: "it were not the subject of its own vote",
            });
        }
        let Some(mine) = self.subject_now(body.subject_seat) else {
            return Err(Failed::NotYet);
        };
        if !mine.same_subject(&body) {
            // Not a fault: a peer whose stage differs from this client's is a
            // peer one step away, and the mesh does not order two messages.
            return Err(Failed::NotYet);
        }
        if !self.voters(body.subject_seat).contains(&seat) {
            return Err(Failed::NotInThisStage);
        }
        let digest = mine.subject_digest();
        self.take_vote(digest, seat, bytes.to_vec(), body);
        let held = self.votes.get(&digest).map(|m| m.len()).unwrap_or(0);
        self.tally = Some((
            mine.subject_seat,
            held,
            self.voters(mine.subject_seat).len(),
            digest,
        ));
        // The vote that completes the set is what produces the certificate, so
        // the two are one call: there is no state in which unanimity has been
        // reached and nobody has said so.
        self.certify_if_unanimous(key, now_ms)
    }

    /// Emit a certificate once every voter has said the same thing.
    fn certify_if_unanimous(
        &mut self,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        // **A certificate already in progress is not a reason to stay silent.**
        // It is usually the opposite: a peer's copy arrived before this client
        // had a complete set of its own, which opened the stage here without
        // this client having contributed to it. The stage then waits for every
        // voter, this client owes one of them, and returning early left both
        // peers holding a stage neither could close — measured, twice, as
        // `cert: not started, one is already in progress` on one survivor while
        // the other had emitted and was waiting for it.
        if let Some(c) = &self.certifying {
            // **Never about itself.** The subject must ACCEPT a certificate
            // naming it — that is the whole point of the position-free path —
            // but it must not EMIT one: every voter answers a certificate from
            // outside the voter set with `NotInThisStage`, which the node turns
            // into a GossipSub `Reject` against a peer that did nothing wrong.
            if c.subject.subject_seat == self.open.my_seat {
                return Ok(Vec::new());
            }
            if c.stage.heard(self.open.my_seat).is_some() {
                return Ok(Vec::new());
            }
            let subject = c.subject;
            let digest = subject.subject_digest();
            let voters = self.voters(subject.subject_seat);
            let held = self.votes.get(&digest).map(|m| m.len()).unwrap_or(0);
            // Its own copy is its own word, so it waits for a complete set
            // exactly as it would to start one. A peer's certificate is not
            // evidence this client may sign against.
            if voters.len() < 2 || held < voters.len() {
                return Ok(Vec::new());
            }
            let bytes = self.seal_certificate(&digest, key, now_ms)?;
            let hash = self.opened(&bytes, EventType::TimeoutCert)?.event_hash;
            self.note_own_certificate(hash, &bytes, &subject);
            let complete = match self.certifying.as_mut() {
                Some(c) => {
                    c.stage.hear(self.open.my_seat, hash);
                    c.stage.complete()
                }
                None => unreachable!("checked just above"),
            };
            self.cert_note.push(format!(
                "the table has certified seat {}'s timeout, unanimously among {:?}",
                subject.subject_seat, voters
            ));
            let mut out = vec![Send::Broadcast(bytes)];
            if complete {
                out.append(&mut self.apply_certificate(key, now_ms)?);
            }
            return Ok(out);
        }
        // The first subject this client holds a complete set for. Complete
        // means **every** seat in the voter set, which is what unanimity is:
        // there is no quorum and no reduction.
        let mut found = None;
        for digest in self.votes.keys().copied().collect::<Vec<_>>() {
            let Some(subject) = self.subject_of(&digest) else {
                continue;
            };
            // Never about itself: see the note above.
            if subject.subject_seat == self.open.my_seat {
                continue;
            }
            let voters = self.voters(subject.subject_seat);
            if voters.len() < 2 {
                continue;
            }
            let held = self.votes.get(&digest).map(|m| m.len()).unwrap_or(0);
            if held >= voters.len() {
                found = Some((digest, subject, voters));
                break;
            }
        }
        let Some((digest, subject, voters)) = found else {
            // Only worth a word when a set that looks complete produced
            // nothing. A partial set is the ordinary state and says nothing.
            if let Some((d, m)) = self.votes.iter().max_by_key(|(_, m)| m.len()) {
                let subject = self.subject_of(d).map(|s| s.subject_seat);
                let need = subject.map(|s| self.voters(s).len()).unwrap_or(0);
                if m.len() >= need && need >= 2 {
                    self.cert_note.push(format!(
                        "cert: {} votes held and none certified; subject {:?} dealt_in {:?} certified {:?}",
                        m.len(),
                        subject,
                        self.mine.dealt_in,
                        self.certified
                    ));
                }
            }
            return Ok(Vec::new());
        };
        // Ascending by voter seat, which the wire requires and which is what
        // makes one set of votes one byte string.
        let bytes = self.seal_certificate(&digest, key, now_ms)?;
        let hash = self.opened(&bytes, EventType::TimeoutCert)?.event_hash;
        let mut stage = Collective::closed(
            self.slot.sequence,
            EventType::TimeoutCert.code(),
            &voters,
        )
        .ok_or(Failed::NotInThisStage)?;
        stage.hear(self.open.my_seat, hash);
        self.note_own_certificate(hash, &bytes, &subject);
        // The one line worth an operator's attention: from here the table has
        // said something about a seat with everybody's signature behind it.
        self.cert_note.push(format!(
            "the table has certified seat {}'s timeout, unanimously among {:?}",
            subject.subject_seat, voters
        ));
        self.certifying = Some(Certifying { subject, stage });
        Ok(vec![Send::Broadcast(bytes)])
    }

    /// One certificate, from the votes this client holds for `digest`.
    ///
    /// Ascending by voter seat, which the wire requires and which is what makes
    /// one set of votes one byte string.
    fn seal_certificate(
        &self,
        digest: &Hash,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<u8>, Failed> {
        let votes: Vec<Vec<u8>> = self
            .votes
            .get(digest)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();
        let body = TimeoutCert {
            subject_digest: *digest,
            votes,
        };
        self.say_at(EventType::TimeoutCert, &body, TIMEOUT_CERT_CAP, key, now_ms)
    }

    /// The subject a digest is about, from a vote this client holds.
    fn subject_of(&self, digest: &Hash) -> Option<TimeoutVote> {
        // **Skipped, not abandoned.** This was `self.subject_now(*seat)?`, so a
        // single seat yielding `None` returned `None` for the whole lookup and
        // silently turned off every certificate for the stage — including ones
        // about seats the function had not reached yet. It happens not to fire
        // today, because `subject_now` returns `None` on phase state and not on
        // anything about the seat, so it is all seats or none; that is a
        // property of the current `stage_type`, not a rule anybody stated, and
        // it is not worth resting a security check on.
        self.mine
            .dealt_in
            .iter()
            .filter_map(|seat| self.subject_now(*seat))
            .find(|s| s.subject_digest() == *digest)
    }

    /// Seal into the stage now open, without advancing it.
    ///
    /// A vote and a certificate **reference** their stage rather than occupying
    /// it — that is what `event_class` 1 and 2 are for — so they are sealed at
    /// the same slot as the stage they are about.
    fn say_at<T: minicbor::Encode<()>>(
        &self,
        kind: EventType,
        body: &T,
        cap: usize,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<u8>, Failed> {
        chained::seal(
            kind,
            &self.slot,
            body,
            key,
            now_ms,
            self.next_deadline_for(kind),
            cap,
        )
        .map_err(Failed::Wire)
    }

    /// Whether the stage now open has outlived its own deadline.
    fn past_stage_deadline(&self, now_ms: u64) -> bool {
        let Some(owed) = self.owed_type() else {
            return false;
        };
        now_ms.saturating_sub(self.stage_at_ms) >= u64::from(self.next_deadline_for(owed))
    }


    /// This client's own certificate, remembered as a fact and kept as proof.
    ///
    /// Its own copy wins over any peer's for `proof`, because §4.10 asks the
    /// emitter of an abort to have *verified that certificate itself* and a
    /// copy this client sealed is the strongest form of that.
    fn note_own_certificate(&mut self, hash: Hash, bytes: &[u8], subject: &TimeoutVote) {
        // The roster effect, on the same evidence a receiver banks on: this
        // client holds a complete unanimous set of votes, which is what a
        // certificate *is*.
        self.bank(subject, hash, bytes);
        // And its own copy wins as the proof an abort carries, because §4.10
        // asks the emitter to have verified that certificate itself.
        self.proof = Some((hash, bytes.to_vec()));
    }

    /// Give the hand up, keeping the settlement this client had already
    /// published.
    ///
    /// §4.10's race is exactly *"a peer's own deadline expiring between its
    /// `HAND_COMPLETE` emission and the arrival of the last other copy"*. The
    /// stage that peer had open already holds its own copy, and a stage
    /// requiring every seat of the required set can only ever complete if it is
    /// carried across. Dropped, the rule that `HAND_COMPLETE` wins would be
    /// unreachable in the one case it is written for.
    fn give_up(&mut self, why: Abort) {
        if self.late.is_none() {
            if let Phase::Playing { play, .. } = &self.phase {
                if let Step::Settling { stage, mine } = &play.step {
                    self.late = Some(Late {
                        stage: stage.clone(),
                        body: (**mine).clone(),
                        closed: None,
                    });
                }
            }
        }
        self.phase = Phase::Aborted(why);
    }

    /// A `HAND_COMPLETE` arriving after this client gave the hand up.
    ///
    /// §4.10's precedence rule has two halves. The first — a settled hand
    /// discards a late abort — is a guard in `on_hand_abort`. This is the
    /// second: *a receiver that applied an abort and later accepts a complete
    /// `HAND_COMPLETE` stage for the same hand replaces its terminal with that
    /// stage's `stage_hash`*. Without it the peer that gave up keeps
    /// `abort_terminal(k)` while everybody else keeps the settlement's hash,
    /// and `GENESIS(k+1)` depends on `TERMINAL(k)` — so the two could never
    /// speak again. The race is narrow and entirely legitimate: it needs one
    /// peer's own deadline to expire between its neighbours' settlement and the
    /// arrival of the last copy.
    fn on_late_settlement(&mut self, bytes: &[u8]) -> Result<Vec<Send>, Failed> {
        let opened = chained::open_in_hand(
            bytes,
            FRAME_CAP,
            EventType::HandComplete,
            &self.open.table_id,
            self.open.hand_id,
        )
        .map_err(Failed::Wire)?;
        let seat = self.seat_of(&opened.sender)?;
        let theirs: HandComplete =
            chained::payload(&opened, HAND_COMPLETE_CAP).map_err(Failed::Wire)?;

        let sequence = opened.envelope.sequence;
        if self.late.is_none() {
            let stage = Collective::closed(
                sequence,
                EventType::HandComplete.code(),
                &self.open.required,
            )
            .ok_or(Failed::NotInThisStage)?;
            self.late = Some(Late {
                stage,
                body: theirs.clone(),
                closed: None,
            });
        }
        let Some(late) = self.late.as_mut() else {
            unreachable!("just set")
        };
        if late.stage.sequence() != sequence {
            // Two settlements at two positions is not a settlement.
            return Err(Failed::Elsewhere {
                seat,
                what: "one settlement stood at one stage",
            });
        }
        if late.stage.heard(seat) == Some(opened.event_hash) {
            return Ok(Vec::new());
        }
        // **Noted, not refused — `S1-BD`'s one word, on its sibling (`S1-BP`).**
        //
        // This returned `DeckDisagrees` here, before `stage.hear`, so a
        // disagreeing copy never entered the late stage, the stage never
        // completed, `late.closed` stayed `None`, and `next_hand` fell through
        // to the abort terminal while the seats that settled kept the
        // settlement's — the fork the `HAND_COMPLETE`-wins rule exists to
        // prevent, produced by the check meant to guard it.
        //
        // **Hearing it cannot move a chip, on either road here.** When this
        // client gave up while settling, `give_up` carried its own stage and
        // its own body across, so `late.body` is this client's OWN settlement
        // and `late.closed` applies those stacks and no other seat's. When it
        // gave up before settling, `late.body` is whichever peer spoke first —
        // and that stage can never complete: `Collective::complete` needs every
        // seat of the required set, this client is in that set, and it never
        // published a `HAND_COMPLETE` for this stage to hear. So a peer-seeded
        // body reaches neither `next_hand` nor `GENESIS(k+1)`, and D-012 is
        // not in question.
        let note = if theirs == late.body {
            None
        } else {
            let what = late.body.disagreement(&theirs);
            Some(if what.is_empty() {
                format!(
                    "late settlement disagreement with seat {seat} in a field this report does not enumerate \
                     - HandComplete::disagreement is missing a field the derive compares"
                )
            } else {
                format!(
                    "late settlement disagreement with seat {seat}: {}; my view is at sequence {} with head {}",
                    what.join("; "),
                    self.slot.sequence,
                    self.slot.previous_event_hash[..4]
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                )
            })
        };
        match late.stage.hear(seat, opened.event_hash) {
            Heard::Counted | Heard::Bystander | Heard::Again => {}
            Heard::Equivocation { .. } => return Err(Failed::Equivocation { seat }),
            // **Held, not refused.** By the time a contribution reaches a
            // collective stage its sender has already been resolved to a seat
            // at this table, so `Uninvited` does not mean "a stranger" — it
            // means this client derived an accepted set that does not hold
            // that seat, which after a certificate is a roster this client and
            // its neighbour disagree about by one entry. `run.rs` turns
            // anything but `NotYet` into a GossipSub `Reject`, and repeatedly
            // rejecting an honest peer is how it stops being forwarded.
            Heard::Uninvited => return Err(Failed::NotYet),
        }
        if late.stage.complete() {
            let hash = late.stage.hash().ok_or(Failed::NotInThisStage)?;
            let stacks = late.body.final_stacks.clone();
            late.closed = Some((hash, stacks));
        }
        if note.is_some() {
            self.settle_note = note;
        }
        Ok(Vec::new())
    }

    /// A `TIMEOUT_CERT` checked from its own bytes, with nothing taken on
    /// trust and nothing read from this receiver's position in the hand.
    ///
    /// **This is what lets the subject of a certificate apply it.** The peer
    /// least able to be standing at the stage a certificate is about is the one
    /// it names — it moved on precisely because it did the thing the voters
    /// never saw — so a check that starts from the receiver's own slot can only
    /// ever refuse it. The artefact proves its own position instead: every
    /// carried vote's **signed** envelope must name the stage the subject names,
    /// which is a binding the voter put its own key behind and is strictly
    /// stronger than anything this receiver's cursor could offer.
    fn verify_certificate(&self, raw: &[u8]) -> Result<VerifiedCert, Failed> {
        // The frame cap on the envelope, the body cap on the payload: a
        // `SignedEvent` wrapping two whole votes is larger than the body it
        // carries, and opening it under the body's cap read as a malformed
        // event.
        let opened = chained::open_in_hand(
            raw,
            FRAME_CAP,
            EventType::TimeoutCert,
            &self.open.table_id,
            self.open.hand_id,
        )
        .map_err(Failed::Wire)?;
        let body: TimeoutCert =
            chained::payload(&opened, TIMEOUT_CERT_CAP).map_err(Failed::Wire)?;

        // Free, and before a single signature is checked: a certificate
        // claiming a hundred votes must cost a length comparison rather than a
        // hundred verifications.
        let max_voters = usize::from(crate::protocol::constants::MAX_SEATS) - 1;
        if body.votes.len() < 2 || body.votes.len() > max_voters {
            return Err(Failed::Elsewhere {
                seat: self.open.my_seat,
                what: "a voter set inside the protocol's bounds",
            });
        }
        let emitter = self.seat_of(&opened.sender)?;

        let mut voters: BTreeSet<SeatIdx> = BTreeSet::new();
        let mut subject: Option<TimeoutVote> = None;
        for vote in &body.votes {
            let v = chained::open_in_hand(
                vote,
                FRAME_CAP,
                EventType::TimeoutVote,
                &self.open.table_id,
                self.open.hand_id,
            )
            .map_err(Failed::Wire)?;
            let voter = self.seat_of(&v.sender)?;
            if !voters.insert(voter) {
                return Err(Failed::Elsewhere {
                    seat: emitter,
                    what: "no seat had voted twice",
                });
            }
            let named: TimeoutVote =
                chained::payload(&v, TIMEOUT_VOTE_CAP).map_err(Failed::Wire)?;
            match &subject {
                None => subject = Some(named),
                Some(first) => {
                    if !named.same_subject(first) {
                        return Err(Failed::Elsewhere {
                            seat: emitter,
                            what: "every carried vote were about one subject",
                        });
                    }
                }
            }
            if voter == named.subject_seat {
                return Err(Failed::Elsewhere {
                    seat: emitter,
                    what: "the subject were not a voter about itself",
                });
            }
            // **The anti-replay binding, and the voter signed it.** A vote
            // sealed at one stage cannot be counted towards a subject at
            // another, for any receiver, with or without a cursor of its own.
            if v.envelope.sequence != named.subject_sequence
                || v.envelope.previous_event_hash != named.parent_event_hash
            {
                return Err(Failed::Elsewhere {
                    seat: emitter,
                    what: "each vote were sealed at the stage it names",
                });
            }
        }
        let Some(subject) = subject else {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "a certificate carried the votes it is made of",
            });
        };
        if opened.envelope.sequence != subject.subject_sequence
            || opened.envelope.previous_event_hash != subject.parent_event_hash
        {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "the certificate were sealed at the stage its votes name",
            });
        }
        // Recomputed, never read from the body: a name the sender may choose is
        // not a name.
        if subject.subject_digest() != body.subject_digest {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "the digest were the one its own votes hash to",
            });
        }
        // **Held, not refused.** A subject this client does not have dealt in
        // is a disagreement about who is playing, which is what this whole path
        // exists to resolve — and `run.rs` turns anything but `NotYet` into a
        // GossipSub `Reject`, which is how an honest peer stops being
        // forwarded.
        if !self.mine.dealt_in.contains(&subject.subject_seat) {
            return Err(Failed::NotYet);
        }
        if subject.subject_sequence >= crate::protocol::constants::MAX_STAGES_PER_HAND {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "a stage index this hand can have",
            });
        }
        let betting = subject.subject_event_type == ACTION_GROUP;
        if betting != (subject.kind == 1) {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "the kind matched the stage type",
            });
        }
        // **The security floor of the whole position-free path.**
        // `next_deadline_for` reads nothing but this hand's `Opening`, whose
        // parameters are inside `table_params_hash` and thence every genesis, so
        // two colluding peers cannot certify a seat at a deadline shorter than
        // this table's own. On the in-position road this comes free from
        // rebuilding the subject; here it has to be restored explicitly, or the
        // carrier really would weaken D-023.
        let owed = if betting {
            Some(EventType::ActionFold)
        } else {
            EventType::try_from(subject.subject_event_type).ok()
        };
        let Some(owed) = owed else {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "a stage type this catalogue defines",
            });
        };
        if subject.deadline_ms != self.next_deadline_for(owed) {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "the deadline this table's parameters give",
            });
        }
        if !voters.contains(&emitter) {
            return Err(Failed::NotInThisStage);
        }

        Ok(VerifiedCert {
            event_hash: opened.event_hash,
            emitter,
            subject,
            voters,
            raw: raw.to_vec(),
        })
    }

    /// The roster half of a certificate: a set insert, and nothing positional.
    ///
    /// Keyed on the **subject digest** and not on the seat, because one hand may
    /// legitimately certify one seat at two stages — that is what
    /// [`MAX_CONSECUTIVE_AUTO_ACTIONS`] counts — and those two must count twice
    /// while a redelivery of either counts once.
    fn bank_certificate(&mut self, c: &VerifiedCert) -> bool {
        self.bank(&c.subject, c.event_hash, &c.raw)
    }

    /// The same, from the pieces, for a certificate this client sealed itself.
    ///
    /// **Both roads bank, and that is the whole point.** The roster effect
    /// rests on a complete unanimous vote set, and a peer that assembled one
    /// and sealed a certificate from it holds exactly the evidence a peer that
    /// received one holds. Banking only on receipt made the two disagree in the
    /// ordinary case: at three seats the voter set is two, so a certificate
    /// about the third completes its collective stage only if BOTH voters'
    /// copies arrive — and if one of them is the peer that went quiet, the
    /// emitter never completes the stage, never reaches `apply_certificate`,
    /// and never shrinks its own roster. Measured: the subject banked the
    /// certificate about itself and dropped to `[1, 2]` while its author stayed
    /// at `[0, 1, 2]`, and the two forked at the next genesis.
    /// A hand whose terminal is a **settlement**: `TERMINAL(k)` is the
    /// `HAND_COMPLETE` stage hash, which a client that did not chain the
    /// certificate stage cannot reach, so a late certificate there is D-024
    /// point 4's fork and the roster is frozen. After an **abort** terminal
    /// the roster still moves on a verified certificate about this hand
    /// until the next hand's stage 0 completes at this receiver (`S1-BS`):
    /// `ABORT_TERMINAL(k)` is a function of `GENESIS(k)` alone, so the
    /// terminal does not move — only R(k+1) does, and re-deriving it is what
    /// puts this client on the table's genesis instead of one of its own.
    fn settled(&self) -> bool {
        self.betting_over()
            || (self.aborted().is_some() && self.late.as_ref().is_some_and(|l| l.closed.is_some()))
    }

    fn bank(&mut self, subject: &TimeoutVote, event_hash: Hash, raw: &[u8]) -> bool {
        // The roster freezes with a settled terminal. After an abort terminal
        // a certificate still banks — and says so, so the node re-derives the
        // next hand (`late_roster`). `S1-BS`: eight seats banked a certificate
        // before the abort ended the hand, the ninth received no copy until
        // after, refused it here, and opened the next hand with the certified
        // seat still in R — a genesis nobody else held.
        if self.settled() || self.banked.len() >= BANKED_CAP {
            return false;
        }
        self.certs.insert(
            event_hash,
            CertFact {
                subject_seat: subject.subject_seat,
                kind: subject.kind,
            },
        );
        if self.proof.is_none() {
            self.proof = Some((event_hash, raw.to_vec()));
        }
        if !self.banked.insert(subject.subject_digest()) {
            return false;
        }
        if !self.certified.contains(&subject.subject_seat) {
            self.certified.push(subject.subject_seat);
        }
        if let Some(n) = self.strikes.get_mut(usize::from(subject.subject_seat)) {
            *n = n.saturating_add(1);
        }
        if self.aborted().is_some() {
            self.late_roster = true;
            self.cert_note.push(format!(
                "cert: about seat {} banked after the terminal; the roster of hand #{} is re-derived",
                subject.subject_seat,
                self.open.hand_id.saturating_add(1)
            ));
        }
        true
    }

    /// What accepting a certificate does to the roster, once it has had its
    /// effect and not before.
    ///
    /// The voter set shrinks here and **only** here — on acceptance of a
    /// certificate that itself cleared the floor. That is what makes the
    /// shrinkage inductive: an attacker cannot reach `|V| = 1` by asserting,
    /// because every step towards it had to clear the floor too.
    ///
    /// Idempotent, and the strike is inside the same guard as the voter set.
    /// They were separate, so a certificate applied twice about one seat
    /// counted once towards `R(k+1)` and twice towards
    /// [`MAX_CONSECUTIVE_AUTO_ACTIONS`] — which decides when a seat sits out.
    fn commit_certificate(&mut self, subject: SeatIdx) {
        self.certifying = None;
        self.note_signed(subject);
        // The same rule as `bank`: a settled hand's roster does not move.
        if self.settled() || self.certified.contains(&subject) {
            return;
        }
        self.certified.push(subject);
        if let Some(n) = self.strikes.get_mut(usize::from(subject)) {
            *n = n.saturating_add(1);
        }
    }

    /// A certificate from a peer, or this client's own coming back.
    fn on_timeout_cert(
        &mut self,
        bytes: &[u8],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let c = self.verify_certificate(bytes)?;
        let seat = c.emitter;

        // **The floor, before anything is spent on it.** Below two voters a
        // certificate is inert: not accepted, not chained, not evidence, no
        // effect at all. At `|V| = 1` "unanimity" is the signature of the one
        // party with an interest in the outcome, which is the whole reason
        // heads-up cannot have this and falls back to the hand deadline.
        if self.mine.dealt_in.len() < 3 || c.voters.len() < 2 {
            return Ok(Vec::new());
        }
        let nominal: BTreeSet<SeatIdx> = self
            .mine
            .dealt_in
            .iter()
            .copied()
            .filter(|s| *s != c.subject.subject_seat)
            .collect();
        if !c.voters.is_subset(&nominal) {
            return Err(Failed::Elsewhere {
                seat,
                what: "every voter were dealt in",
            });
        }
        // **Checked in the direction that can only tighten.** A receiver whose
        // own `certified` is shorter than the emitters' derives a *larger*
        // voter set — which is precisely the subject's own position — so a
        // shortfall is this client's incompleteness talking and is held, never
        // refused. Every other error becomes a GossipSub `Reject`, and
        // repeatedly rejecting an honest peer is how it stops being forwarded.
        let mine: BTreeSet<SeatIdx> =
            self.voters(c.subject.subject_seat).into_iter().collect();
        if !mine.is_subset(&c.voters) {
            self.cert_note.push(format!(
                "cert: from seat {seat} about seat {} with voters {:?}; this client \
                 derives {mine:?} and is missing a certificate the emitters hold — HELD",
                c.subject.subject_seat, c.voters
            ));
            return Err(Failed::NotYet);
        }

        // The roster half, wherever this client happens to stand — and it
        // comes FIRST, before the positional half below can hold. `bank` is
        // the only writer of `certified` and `strikes` on receipt, and
        // `next_hand` derives R(k+1) from `certified` through `took_part`;
        // a client that holds a certificate it never gets to replay — its
        // hand ended first, which in `split003741-10` was every one of the
        // five measured cases — must still open hand k+1 at the genesis the
        // table opens it at. Banking is keyed on the subject digest, so a
        // replayed copy banks once and `banked` is false on the replay.
        let banked = self.bank_certificate(&c);

        // **A hand that is over takes the roster half and nothing else.** No
        // certificate stage is built on a finished hand, no fork is reported
        // about a stage it will never reach, and nothing is sealed or sent;
        // the bank above is the whole of what a late copy can do (`S1-BS`).
        if self.aborted().is_some() {
            let _ = banked;
            return Ok(Vec::new());
        }

        // **Behind is not forked (`S1-BQ`).** A certificate about a betting
        // stage this client has not reached yet is held and replayed when
        // it gets there, like every other early event (`PROTOCOL.md` §5.2.5,
        // normative: *an event of a stage not yet reached is held*). It used
        // to be consumed here with the fork report below — `Ok`, so never
        // replayed — and every voter's copy went the same way; on catching
        // up the client had no certificate stage. The fork the report
        // describes (D-024 point 4: two parents at one sequence) needs this
        // client to have chained something AT that sequence, which a client
        // behind it has not; equal-with-a-different-parent or ahead is the
        // fork, and stays below. Said once, on the first copy: the replay
        // pass re-delivers every held copy every 2 s, and a note per pass
        // would be a line every 2 s until the hand ends.
        //
        // Two things this does not repair, measured in the same run: a
        // client that is behind because an EARLIER event was lost never
        // reaches the sequence at all, and holding changes nothing for it;
        // and the held FIFO is 64 deep, so a client 64 events behind loses
        // the copies before it could replay them. Both are the carrier's
        // loss, not the certificate's disposition.
        if c.subject.kind == 1 && self.slot.sequence < c.subject.subject_sequence {
            if banked {
                self.cert_note.push(format!(
                    "cert: from seat {seat} about seat {} at sequence {} reached this client at \
                     sequence {} — roster banked, positional half HELD until it is in position",
                    c.subject.subject_seat, c.subject.subject_sequence, self.slot.sequence
                ));
            }
            return Err(Failed::NotYet);
        }

        let in_position = self.slot.sequence == c.subject.subject_sequence
            && self.slot.previous_event_hash == c.subject.parent_event_hash;

        if !in_position {
            if banked && c.subject.kind == 1 {
                // A betting stage is single-writer. The subject advanced with
                // its own action's `stage_hash_single`; the voters advanced
                // with the certificate stage's hash. Two parents at one
                // sequence, and there is no reconciliation: `BettingRound`
                // validates legality but never turn order, so replaying would
                // take a decision for a seat on a street it is not acting in
                // and succeed in silence — and §3.2 forbids the alternative,
                // because the hash has already been chained from. Reported,
                // not repaired.
                self.forked = Some(format!(
                    "the table acted for seat {} at sequence {} and played on; this \
                     client is at sequence {} on a branch of its own, and the two \
                     cannot be reconciled",
                    c.subject.subject_seat, c.subject.subject_sequence, self.slot.sequence
                ));
            }
            if banked && c.subject.kind == 2 {
                // Convergent: `abort_terminal(k)` is a function of `GENESIS(k)`
                // and of nothing in the middle of the hand, so a peer ending the
                // hand from anywhere ends it where everyone else does.
                let named = self
                    .open
                    .seats
                    .iter()
                    .find(|(s, _, _)| *s == c.subject.subject_seat)
                    .map(|(_, k, _)| *k);
                let proof = self.proof.as_ref().map(|(h, _)| *h);
                return self.abort_named(named, proof, key, now_ms);
            }
            return Ok(Vec::new());
        }

        // In position: the collective stage, so that every voter's copy is
        // counted and the stage closes the way every other stage does.
        let voters: Vec<SeatIdx> = c.voters.iter().copied().collect();
        let subject = c.subject;
        let stage = match &mut self.certifying {
            Some(cur) if cur.subject.same_subject(&subject) => &mut cur.stage,
            // A stale certificate must not destroy a live certification about
            // something else.
            Some(_) => return Err(Failed::NotYet),
            None => {
                // **The subject's sequence, not this client's slot.**
                // `stage_hash_collective` hashes the sequence, so a peer that
                // built the stage at its own cursor would compute a parent no
                // other peer computed. A no-op while every peer is in position,
                // and a fork the moment one is not.
                let stage = Collective::closed(
                    subject.subject_sequence,
                    EventType::TimeoutCert.code(),
                    &voters,
                )
                .ok_or(Failed::NotInThisStage)?;
                self.certifying = Some(Certifying { subject, stage });
                let Some(cur) = self.certifying.as_mut() else {
                    unreachable!("just set")
                };
                &mut cur.stage
            }
        };
        if stage.heard(seat) == Some(c.event_hash) {
            return Ok(Vec::new());
        }
        match stage.hear(seat, c.event_hash) {
            Heard::Counted | Heard::Bystander | Heard::Again => {}
            Heard::Equivocation { .. } => return Err(Failed::Equivocation { seat }),
            // The stage here was built from the first certificate's voter set,
            // and an emitter outside it means the two peers derived different
            // sets. That is this client being behind, not the sender being
            // wrong, so it is held rather than rejected.
            Heard::Uninvited => return Err(Failed::NotYet),
        }
        if !stage.complete() {
            // This client may still owe its own copy: every voter emits one.
            return self.certify_if_unanimous(key, now_ms);
        }
        self.apply_certificate(key, now_ms)
    }

    /// Every voter has certified: do what the certificate says.
    fn apply_certificate(
        &mut self,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        // **Read, do not take, and change nothing yet.** Everything below can
        // fail — the phase may not be a betting one, and the engine may refuse
        // the action — and the first version had already shrunk the voter set,
        // added a strike and consumed the certificate before it found out. The
        // state that left behind was the worst of both: a seat removed from
        // `R(k+1)` with nothing done about the stage it was late for, and no
        // certificate left to try again with.
        let (subject, parent, my_copy) = {
            let c = self.certifying.as_ref().ok_or(Failed::NothingFurther)?;
            (
                c.subject,
                c.stage.hash().ok_or(Failed::NotInThisStage)?,
                c.stage.heard(self.open.my_seat),
            )
        };

        match subject.kind {
            // An action deadline. The engine acts for the seat: **check** when
            // nothing is owed and **fold** when facing a bet — never fold a
            // hand that could check for free. The round then continues among
            // the remaining players and the hand plays out normally. Nothing
            // aborts and nothing waits.
            1 => {
                let seat = subject.subject_seat;
                let action = {
                    let Phase::Playing { play, .. } = &self.phase else {
                        return Err(Failed::NothingFurther);
                    };
                    if play.round.to_call(seat) == 0 {
                        Action::Check
                    } else {
                        Action::Fold
                    }
                };
                {
                    let Phase::Playing { play, .. } = &mut self.phase else {
                        return Err(Failed::NothingFurther);
                    };
                    play.round
                        .apply(seat, action)
                        .map_err(|what| Failed::Illegal { seat, what })?;
                    play.actions = play.actions.saturating_add(1);
                }
                // Nothing has failed, so the certificate has had its effect and
                // the roster effects go with it.
                self.commit_certificate(subject.subject_seat);
                // The stage is closed by the **certificate** stage's own hash,
                // because the betting stage it was about has none: nobody
                // wrote it. The two are at one sequence in two classes, which
                // is what `event_class` is for.
                self.slot = self.slot.then(parent);
                self.mark_stage(now_ms);
                self.acted_for = Some((seat, action));
                self.after_action(seat, key, now_ms)
            }
            // A cryptographic deadline. The hand ends and the seat is named —
            // **as evidence only**. No chips move: an abort restores every
            // stack, and D-010 forbids reading `attributed` to move one.
            _ => {
                self.commit_certificate(subject.subject_seat);
                self.slot = self.slot.then(parent);
                self.mark_stage(now_ms);
                let named = self
                    .open
                    .seats
                    .iter()
                    .find(|(s, _, _)| *s == subject.subject_seat)
                    .map(|(_, k, _)| *k);
                // The **certificate**, not the stage: §4.10 says `cert_hash`
                // is the `event_hash` of a `TIMEOUT_CERT`. This client's own
                // copy is the one it can prove it holds.
                self.abort_named(named, my_copy, key, now_ms)
            }
        }
    }

    /// Give up on the hand, naming a seat as the evidence says.
    fn abort_named(
        &mut self,
        named: Option<[u8; 32]>,
        cert_hash: Option<Hash>,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        if matches!(self.phase, Phase::Aborted(_)) {
            return Ok(Vec::new());
        }
        let mut body = HandAbort::on_deadline(self.mine.stacks.clone());
        // The two travel together or not at all: a named subject without the
        // certificate that named it is an accusation on a peer's word, and
        // `HandAbort::consistent` refuses that shape in both directions.
        if let (Some(k), Some(h)) = (named, cert_hash) {
            body.attributed = vec![k];
            body.cert_hash = Some(h);
        }
        let bytes = self.say(EventType::HandAbort, &body, HAND_ABORT_CAP, key, now_ms)?;
        self.give_up(Abort::Told { cause: 1 });
        Ok(vec![Send::Broadcast(bytes)])
    }

    /// How the vote count stands, taken rather than read.
    /// How many `TIMEOUT_CERT`s this client has verified for itself.
    ///
    /// The invariant behind `on_hand_abort`'s gate, exposed so a test can hold
    /// it: this counts certificates **opened, checked vote by vote against the
    /// voter set and found unanimous** by this client, and nothing else. A
    /// misplaced insert once filled the same set with every `HAND_INIT` hash of
    /// the hand — a value every peer holds — which opened the gate to anybody
    /// and shut it to the mechanism it exists for. A count is what tells those
    /// two apart from outside.
    pub fn verified_certificates(&self) -> usize {
        self.certs.len()
    }

    pub fn take_settle_note(&mut self) -> Option<String> {
        self.settle_note.take()
    }

    pub fn take_shuffle_note(&mut self) -> Option<String> {
        self.shuffle_note.take()
    }

    pub fn take_cert_note(&mut self) -> Option<String> {
        if self.cert_note.is_empty() {
            return None;
        }
        Some(std::mem::take(&mut self.cert_note).join(" | "))
    }

    /// The required emitter set this hand is being played under.
    ///
    /// So the node can say *the table changes* only when it does, rather than
    /// printing a derivation after every hand of an unchanged table.
    pub fn required_now(&self) -> Vec<SeatIdx> {
        self.open.required.clone()
    }

    /// Every input the next hand's roster is derived from, in one line.
    ///
    /// Written because two readings of the derivation gave answers a live log
    /// contradicted: a seat dropped with no certificate accepted, and a seat
    /// that had been dropped came back. The inputs are all private and the
    /// output is a set, so from outside there was no way to tell which moved.
    pub fn roster_derivation(&self) -> String {
        let stacked: Vec<SeatIdx> = (0..self.open.max_players)
            .filter(|s| {
                self.open
                    .seats
                    .iter()
                    .any(|(seat, _, stack)| seat == s && *stack > 0)
            })
            .collect();
        let signed: Vec<SeatIdx> = (0..self.open.max_players)
            .filter(|s| self.heard_from(*s))
            .collect();
        format!(
            "roster from: required {:?} certified {:?} strikes {:?} grace {:?} signed {signed:?} stacked {stacked:?} by_certificate={}",
            self.open.required,
            self.certified,
            self.strikes,
            self.open.grace,
            self.open.required.len() >= 3,
        )
    }

    /// The one condition this client cannot repair and must not hide: two
    /// parents at one sequence, taken rather than read so the node reports it
    /// once.
    pub fn take_fork(&mut self) -> Option<String> {
        self.forked.take()
    }

    pub fn take_tally(&mut self) -> Option<(SeatIdx, usize, usize, Hash)> {
        self.tally.take()
    }

    /// What a certificate last did, taken rather than read: the node reports
    /// it once and it is not a standing fact about the hand.
    pub fn take_certified_action(&mut self) -> Option<(SeatIdx, Action)> {
        self.acted_for.take()
    }

    /// How many certificates have been accepted against a seat this hand.
    ///
    /// At [`MAX_CONSECUTIVE_AUTO_ACTIONS`] the seat is marked sitting out at
    /// the next hand boundary — it keeps its stack, posts dead money, takes no
    /// cards and drains, which is a tournament's dead seat. It exists only
    /// where `|V| >= 2`: below the floor no certificate has an effect and this
    /// never increments.
    pub fn strikes_against(&self, seat: SeatIdx) -> u8 {
        self.strikes.get(usize::from(seat)).copied().unwrap_or(0)
    }

    /// Seal one body into the stage now open.
    fn say<T: minicbor::Encode<()>>(
        &self,
        kind: EventType,
        body: &T,
        cap: usize,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<u8>, Failed> {
        chained::seal(
            kind,
            &self.slot,
            body,
            key,
            now_ms,
            self.next_deadline_for(kind),
            cap,
        )
        .map_err(Failed::Wire)
    }

    /// The proof context for one emitter at the stage now open.
    ///
    /// Per **sender**, which the wire requires: `sender_public_key` is inside
    /// the context, so one context shared by three senders would let a proof
    /// made for one of them verify for another. A test that shares one passes
    /// and the wire does not.
    fn deck_ctx(&self, sender: &[u8; 32]) -> DeckCtx {
        self.deck_ctx_at(sender, self.slot.sequence)
    }

    /// The same context at a **named** stage.
    ///
    /// Live traffic is judged at this client's own cursor, which is what
    /// [`deck_ctx`](Hand::deck_ctx) passes. Evidence is not: a `cause = 3`
    /// abort carries a frame from the stage the accused was at, and re-deriving
    /// its context from the receiver's cursor would make a perfectly good share
    /// fail wherever the two differ — turning a false accusation into one that
    /// succeeds, at exactly the receivers that had moved on. The gate passes
    /// the frame's own signed `sequence` instead, which the accused chose and
    /// no accuser can alter.
    fn deck_ctx_at(&self, sender: &[u8; 32], sequence: u64) -> DeckCtx {
        DeckCtx::build(&CtxFields {
            protocol_version: crate::protocol::messages::PROTOCOL_VERSION,
            table_id: self.open.table_id,
            session_id: self.open.session_id,
            hand_id: self.open.hand_id,
            sequence,
            position: ProofPosition::NotAShuffleStep,
            sender_public_key: *sender,
        })
    }

    /// This client's verified key for the seat, wherever the phase keeps it.
    fn keys_by_seat(&self, seat: SeatIdx) -> Option<&VerifiedKey> {
        match &self.phase {
            Phase::Shuffling { deal, .. }
            | Phase::Committing { deal, .. }
            | Phase::Dealing { deal, .. }
            | Phase::Playing { deal, .. } => deal.key_of(seat),
            _ => None,
        }
    }

    fn seat_index(&self) -> usize {
        self.open
            .seats
            .iter()
            .position(|(s, _, _)| *s == self.open.my_seat)
            .unwrap_or(0)
    }

    /// Hold an event that belongs to a stage this client has not reached.
    pub fn hold(&mut self, bytes: Vec<u8>) -> Holding {
        // **Verified before it is kept, and this is the cheapest attack in the
        // set without it.** `on_event` routes on `peek`, which checks no
        // signature and no key, so `Failed::NotYet` is reachable by unsigned
        // junk — a matching type byte and a sequence one ahead is the whole
        // recipe. The node answered that by storing the bytes and telling
        // GossipSub to **Accept**, which forwards a forgery in this client's
        // own name and evicts a genuine early event from a queue of sixty-four.
        // Measured by the review that found it: seventy-one frames, seventy-one
        // `NotYet`, zero signature checks, the queue full.
        //
        // Opening the event proves the signature and that it belongs to this
        // table and this hand, and relaxes only the position — which is the
        // one thing that was ever in question about a held event.
        let Ok((kind, hand_id, _)) = chained::peek(&bytes, PEEK_CAP) else {
            return Holding::Malformed;
        };
        // **Verified first, and classified second.** The order was the other
        // way round, so an event of another hand was answered before its
        // signature was looked at — and the node turns that answer into
        // `Ignore`, which does not forward. A peer one hand behind is exactly
        // the intermediary a peer one hand ahead needs, so that quietly undid
        // the forwarding property for every next-hand event crossing a table
        // wider than a full mesh.
        //
        // The event is opened against **the hand it names**, which leaves the
        // table identity and the signature doing all the work — that is what a
        // relay decision may rest on, and a `hand_id` this client is not
        // playing is not a fault in the sender.
        let Ok(opened) =
            chained::open_in_hand(&bytes, FRAME_CAP, kind, &self.open.table_id, hand_id)
        else {
            return Holding::Malformed;
        };
        if hand_id != self.open.hand_id {
            // A hand this client is not playing. Worth relaying and not worth
            // holding: `replay_early` re-runs the same guard, `early` does not
            // survive into the next hand, and the bytes would sit here until
            // they pushed something useful out.
            //
            // **Who signed it is carried out with it.** The signature and the
            // table have just been checked, so this is a roster seat's own
            // word that the table is somewhere this client is not — which is
            // the only evidence a client on a private branch can ever get.
            return Holding::AnotherHand {
                hand_id,
                seat: self.seat_of_key(&opened.sender),
            };
        }
        // **The same bytes twice are one held event.** The power-of-two re-send
        // puts every recent stage on the wire again at ticks 2, 4, 8, 16, 32,
        // and each copy used to take a slot of its own: `split092359-10`,
        // `far-n1`, thirteen held for seven seats. A re-open's replay rests
        // on the copies surviving a queue of sixty-four.
        if self.early.iter().any(|b| *b == bytes) {
            return Holding::Kept;
        }
        // **A `HAND_INIT` of this hand at a genesis this client does not
        // hold is a roster seat's signed word that the table opened the hand
        // elsewhere** — `chained::open_inner` computes exactly that
        // (*a different parent: the sender is on another chain*) and
        // `opened()` collapses it into `NotYet`, so until now no client on a
        // private branch could tell a slow table from a lost one. Recorded
        // per seat, and said once when `FOREIGN_GENESIS_FLOOR` seats name one
        // value. Read off a signed envelope field, never a body, and it
        // decides nothing about R (D-012): it says when to look.
        if kind == EventType::HandInit
            && opened.envelope.sequence == 0
            && self.slot.sequence == 0
            && opened.envelope.previous_event_hash != self.open.genesis
        {
            // Roster seats of this hand only: a seat certified out of it
            // opens the hand it thinks it is in, and its word does not
            // contest this one.
            if let Some(seat) = self
                .seat_of_key(&opened.sender)
                .filter(|s| self.open.required.contains(s))
            {
                self.foreign_genesis
                    .insert(seat, opened.envelope.previous_event_hash);
                if self.genesis_note.is_none() {
                    if let Some((g, seats)) = self.foreign_genesis_named() {
                        let short = |h: &Hash| -> String {
                            h[..4].iter().map(|b| format!("{b:02x}")).collect()
                        };
                        self.genesis_note = Some(format!(
                            "hand #{}: seat(s) {:?} opened it at genesis {}; this client opened it at {} \
                             and nobody has been heard at that genesis. A certificate about hand #{} \
                             would repair it",
                            self.open.hand_id,
                            seats,
                            short(&g),
                            short(&self.open.genesis),
                            self.open.hand_id.saturating_sub(1)
                        ));
                    }
                }
            }
        }
        // Bounded: this is fed from the network, and everything fed from the
        // network is bounded where it is consumed.
        if self.early.len() >= 64 {
            self.early.pop_front();
        }
        self.early.push_back(bytes);
        Holding::Kept
    }

    /// The foreign genesis named by at least `FOREIGN_GENESIS_FLOOR` roster
    /// seats' sequence-0 `HAND_INIT`s of this hand, with those seats — the
    /// most-named value if there are several. `None` below the floor.
    pub fn foreign_genesis_named(&self) -> Option<(Hash, Vec<SeatIdx>)> {
        let mut by_value: BTreeMap<Hash, Vec<SeatIdx>> = BTreeMap::new();
        for (seat, g) in &self.foreign_genesis {
            by_value.entry(*g).or_default().push(*seat);
        }
        by_value
            .into_iter()
            .filter(|(_, seats)| seats.len() >= FOREIGN_GENESIS_FLOOR)
            .max_by_key(|(_, seats)| seats.len())
    }

    /// The once-per-hand foreign-genesis line, taken rather than read.
    pub fn take_genesis_note(&mut self) -> Option<String> {
        self.genesis_note.take()
    }

    /// Whether a certificate banked after this hand's abort terminal since
    /// the last time this was asked: the next hand's roster is to be
    /// re-derived through [`next_hand`](Hand::next_hand).
    pub fn take_late_roster(&mut self) -> bool {
        std::mem::take(&mut self.late_roster)
    }

    /// The seats other than this client's whose sequence-0 `HAND_INIT` of
    /// this hand was counted at this client's genesis. Meaningful while the
    /// hand is at sequence 0, and readable after it ended there.
    pub fn counted_at_stage_zero(&self) -> Vec<SeatIdx> {
        self.signed
            .iter()
            .enumerate()
            .filter(|(s, on)| **on && *s != usize::from(self.open.my_seat))
            .map(|(s, _)| s as SeatIdx)
            .collect()
    }

    /// The held events, taken: a re-opened hand inherits them.
    pub fn take_early(&mut self) -> VecDeque<Vec<u8>> {
        std::mem::take(&mut self.early)
    }

    /// §4.9's readmission set this hand was opened with.
    pub fn readmitted(&self) -> &[SeatIdx] {
        &self.open.readmitted
    }

    pub fn voice(&self) -> Voice {
        self.voice
    }

    /// Whether this client's own `HAND_INIT` of this hand went out.
    pub fn spoke(&self) -> bool {
        self.voice == Voice::Speak
            && self.signed.get(usize::from(self.open.my_seat)).copied().unwrap_or(false)
    }

    /// Send the withheld `HAND_INIT` of a quiet hand — once. `None` for a
    /// hand that is speaking already, muted, or not a member.
    pub fn speak(&mut self) -> Option<Send> {
        if self.voice != Voice::Quiet {
            return None;
        }
        let bytes = self.own_init.take()?;
        self.voice = Voice::Speak;
        Some(Send::Broadcast(bytes))
    }

    /// Judge everything that was held, now that the stage may have moved.
    ///
    /// Whatever the replay produces is returned with the failures, because a
    /// held event can be the one that completes a stage — and a stage that
    /// completed without its message going out is a table that stops.
    ///
    /// Two things here are not obvious and both were wrong the first time.
    ///
    /// **An event that is still early goes back in the queue.** The first
    /// version dropped it, which turned "this peer is one stage ahead" into
    /// "this peer's message is gone", and the only thing that would have
    /// recovered it was a re-send nothing performs.
    ///
    /// **The replay loops.** Applying the held stage `n+1` can complete it and
    /// move the hand to `n+2`, which may also be in the queue — from one pass
    /// it would go back into the queue and stay there until some *other* event
    /// arrived to trigger another replay. Over a mesh that delivers a burst and
    /// then goes quiet, "some other event" is not guaranteed to exist, so the
    /// pass repeats while it is making progress and stops when it is not.
    pub fn replay_early(
        &mut self,
        key: &SigningKey,
        now_ms: u64,
    ) -> (Vec<Send>, Vec<Failed>) {
        let mut sends = Vec::new();
        let mut failures = Vec::new();
        loop {
            let waiting: Vec<Vec<u8>> = self.early.drain(..).collect();
            let mut applied = false;
            for bytes in waiting {
                match self.on_event(&bytes, key, now_ms) {
                    Ok(mut out) => {
                        applied = true;
                        sends.append(&mut out);
                    }
                    // Still ahead of this client. Held again, and the hold is
                    // still bounded - `hold` drops the oldest at 64.
                    // It was verified on the way in, so re-holding it cannot
                    // fail for any reason worth acting on.
                    Err(Failed::NotYet) => {
                        let _ = self.hold(bytes);
                    }
                    Err(e) => failures.push(e),
                }
            }
            if !applied {
                return (sends, failures);
            }
        }
    }

    /// How many events are being held for a stage this client has not reached.
    pub fn held(&self) -> usize {
        self.early.len()
    }

    /// Whether stage 0 is complete: every required seat heard and agreed.
    ///
    /// The name is the player's word for it. What it means underneath is that
    /// the hand has left `HAND_INIT` behind.
    pub fn dealt(&self) -> bool {
        !matches!(self.phase, Phase::Init(_))
    }

    /// Whether every seat's deck key is verified and the deck exists.
    pub fn deck_ready(&self) -> bool {
        !matches!(
            self.phase,
            Phase::Init(_) | Phase::Deck { .. } | Phase::Between
        )
    }

    /// Whether the chain has closed and the deck is final.
    pub fn shuffled(&self) -> bool {
        matches!(
            self.phase,
            Phase::Committing { .. } | Phase::Dealing { .. } | Phase::Playing { .. }
        )
    }

    /// This client's two cards, once every share for them has arrived.
    ///
    /// `None` at every other moment, and that is the only claim this type makes
    /// about them: a card exists here when it has been opened from a complete
    /// set of verified shares, and at no earlier point is there anything to
    /// show. §22 is kept by there being no card rather than by a check.
    pub fn cards(&self) -> Option<[Card; 2]> {
        match &self.phase {
            Phase::Playing { play, .. } => play.cards,
            _ => None,
        }
    }

    /// Whether this client is holding its hole cards.
    pub fn dealt_cards(&self) -> bool {
        matches!(self.phase, Phase::Playing { .. })
    }

    /// Whose turn it is, and everything a table window needs to draw it.
    ///
    /// Handed out whole rather than as six accessors, because a window that
    /// read `legal` on one frame and `to_call` on the next could draw a button
    /// for an action that is no longer offered. One call, one consistent
    /// answer.
    pub fn turn(&self) -> Option<Turn> {
        let Phase::Playing { play, .. } = &self.phase else {
            return None;
        };
        let Step::Acting { to_act } = play.step else {
            return None;
        };
        Some(Turn {
            seat: to_act,
            mine: to_act == self.open.my_seat,
            street: play.street,
            to_call: play.round.to_call(to_act),
            legal: play.round.legal(to_act)?,
            pot: play.pot(),
        })
    }

    /// The street the hand is on, once it is being played.
    pub fn street(&self) -> Option<Street> {
        match &self.phase {
            Phase::Playing { play, .. } => Some(play.street),
            _ => None,
        }
    }

    /// Everything committed to the pot so far this hand, every street.
    pub fn pot(&self) -> Chips {
        match &self.phase {
            Phase::Playing { play, .. } => play.pot(),
            _ => 0,
        }
    }

    /// What each seat has behind, by seat.
    pub fn stacks(&self) -> Vec<Chips> {
        match &self.phase {
            Phase::Playing { play, .. } => play.round.stack.clone(),
            _ => Vec::new(),
        }
    }

    /// Which seats have folded, by seat.
    pub fn folded(&self) -> Vec<bool> {
        match &self.phase {
            Phase::Playing { play, .. } => play.round.folded.clone(),
            _ => Vec::new(),
        }
    }

    /// The board, as far as it has been opened.
    ///
    /// Empty before the flop, and it stays empty until a `BOARD_REVEAL` stage
    /// completes: a board card comes out of a complete set of verified shares
    /// or it does not exist. Nobody is ever shown one this client did not open
    /// itself.
    pub fn board(&self) -> Vec<Card> {
        match &self.phase {
            Phase::Playing { play, .. } => play.dealing.board(),
            _ => Vec::new(),
        }
    }

    /// The order the showdown runs in, TDA 17-A. Empty unless one is open.
    pub fn showdown_order(&self) -> Vec<SeatIdx> {
        match &self.phase {
            Phase::Playing { play, .. } => match &play.step {
                Step::Showdown { order, .. } => order.clone(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// What a seat showed, if it showed.
    ///
    /// `None` for a seat that folded, mucked, or has not spoken yet — and the
    /// three are not distinguished here on purpose, because to a reader of the
    /// table they are the same thing: a hand nobody will ever see.
    pub fn shown(&self, seat: SeatIdx) -> Option<[Card; 2]> {
        match &self.phase {
            Phase::Playing { play, .. } => *play.shown.get(usize::from(seat))?,
            _ => None,
        }
    }

    /// Whether a seat forfeited at the showdown.
    pub fn mucked(&self, seat: SeatIdx) -> bool {
        match &self.phase {
            Phase::Playing { play, .. } => {
                play.mucked.get(usize::from(seat)).copied().unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Whether a showdown is open and this client has yet to speak in it.
    pub fn showing(&self) -> bool {
        matches!(
            &self.phase,
            Phase::Playing { play, .. } if matches!(play.step, Step::Showdown { .. })
        )
    }

    /// Everything hand `k+1` needs, read off this hand's settled outcome.
    ///
    /// `None` until this hand has a terminal stage, and `None` when the table
    /// is over — fewer than two seats with chips is the tournament's end
    /// condition (`STATE_MACHINE.md` §9.3), not an error.
    ///
    /// Every input is derived from the chain rather than from anything local:
    /// the stacks are the settlement every seat published byte-identically, the
    /// parent is `TERMINAL(k)`, and the required emitter set is `P(k)` — the
    /// seats this client accepted an event from this hand.
    pub fn next_hand(&self) -> Option<Opening> {
        // Two roads to a terminal, and they differ in both quantities that go
        // into hand `k+1`: what everybody holds, and what `TERMINAL(k)` is.
        let (stacks, terminal): (Vec<Chips>, Hash) = match &self.phase {
            Phase::Playing { play, .. } if matches!(play.step, Step::Ended) => (
                // The settlement has been applied, so this is what everybody
                // holds; and `TERMINAL(k)` is the `HAND_COMPLETE` stage hash,
                // which is the slot's parent now that the stage closed.
                play.round.stack.clone(),
                self.slot.previous_event_hash,
            ),
            // A settlement that arrived after this client gave up. §4.10:
            // `HAND_COMPLETE` wins over an abort, so the terminal and the
            // stacks are the settlement's and not the abort's.
            Phase::Aborted(_) if self.late.as_ref().is_some_and(|l| l.closed.is_some()) => {
                let (hash, stacks) = self
                    .late
                    .as_ref()
                    .and_then(|l| l.closed.clone())
                    .expect("checked in the guard");
                (stacks, hash)
            }
            Phase::Aborted(_) => (
                // An abort moves no chips: every seat ends the hand with what
                // it started it with (D-010, I27), and those are the values
                // already bound into `roster_hash(k)`.
                self.mine.stacks.clone(),
                // A function of `GENESIS(k)` and nothing else, which is exactly
                // why an abort's terminal cannot fork: the peers could not
                // agree about the middle of the hand, so the terminal must not
                // depend on the middle of the hand.
                crate::protocol::transcript::abort_terminal(
                    &self.open.table_id,
                    self.open.hand_id,
                    &self.open.genesis,
                ),
            ),
            _ => return None,
        };
        let stacks = &stacks;
        let alive: Vec<bool> = (0..usize::from(self.open.max_players))
            .map(|i| stacks.get(i).copied().unwrap_or(0) > 0)
            .collect();
        // Fewer than two seats with chips: the tournament is decided. There is
        // no hand `k+1` and saying so is the whole answer.
        let positions = advance_positions(
            Positions {
                button: self.mine.button_position,
                small_blind: self.mine.sb_position,
                big_blind: self.mine.bb_seat,
            },
            &alive,
            self.open.max_players,
        )?;

        // The roster at the start of hand `k+1`, which is this hand's final
        // stacks — `PROTOCOL.md` §3.1's `stack_at_hand_start`.
        let mut seats: Vec<(SeatIdx, [u8; 32], u64)> = Vec::new();
        let mut roster = Vec::new();
        for (seat, keyb, _) in &self.open.seats {
            let stack = stacks.get(usize::from(*seat)).copied().unwrap_or(0);
            seats.push((*seat, *keyb, stack));
            roster.push(crate::protocol::transcript::RosterSeat {
                seat: *seat,
                app_public_key: *keyb,
                stack_at_hand_start: stack,
            });
        }
        let roster_hash = crate::protocol::transcript::roster_hash(&roster);
        let hand_id = self.open.hand_id + 1;

        // The bank, folded forward one hand. Every input is agreed: `P(k)` is
        // `signed`, which is inside the end-of-hand state hash, and the two
        // constants are this version's. A seat present this hand adds to its
        // run and earns a unit back at [`REPLENISH_AFTER`]; a seat absent this
        // hand spends one and its run resets.
        //
        // A seat that has burned its allowance can still accrue: it is not
        // dealt in, but it may sign the hand's terminal as a bystander, which
        // puts it in `P(k)` and is how it plays its way back to the table.
        let n = usize::from(self.open.max_players);
        let mut grace = self.open.grace.clone();
        let mut present_run = self.open.present_run.clone();
        grace.resize(n, GRACE_HANDS);
        present_run.resize(n, 0);
        // **What counts as having taken part must be agreed, not observed.**
        // `signed` is this client's own record of whose events it accepted, and
        // two honest peers legitimately differ in it: a hand ended by a
        // witness-independent terminal closes at whatever each peer had reached,
        // so the tail of a hand is exactly where their records diverge. Both
        // `grace` and `R(k+1)` go into the next hand's genesis, so a difference
        // there is not a cosmetic one — measured, as two survivors opening hand
        // four at the same genesis with `dealt_in` `[0, 2]` and `[0, 1, 2]` and
        // each refusing the other's.
        //
        // A certificate is the shared record: it is accepted only when every
        // voter has signed the same thing, so two peers that applied one agree
        // about it by construction. Where a certificate is possible — three
        // seats or more, D-023's floor — absence means *certified* absence and
        // nothing else. Heads-up there can be no certificate at all, and this
        // falls back to observation, which is safe for the only reason it is
        // ever safe: with one other peer there is nobody to disagree with.
        // The same test `took_part` applies, kept here because the two
        // branches below differ in more than the predicate.
        let by_certificate = self.open.required.len() >= 3;
        let took_part = |me: &Self, seat: usize| -> bool {
            me.took_part(seat as SeatIdx)
        };
        for seat in 0..n {
            if !alive.get(seat).copied().unwrap_or(false) {
                continue;
            }
            // Three certificates against one seat and it sits out at this
            // boundary: it keeps its stack, posts dead money, takes no cards
            // and drains. `PROTOCOL.md` §8.3, and it is the tournament's dead
            // seat. Expressed by emptying the allowance rather than by a
            // second flag, so there is one gate on `dealt_in` and not two that
            // can disagree — and so the seat can still earn its way back the
            // way any other absent one does.
            if self.strikes.get(seat).copied().unwrap_or(0) >= MAX_CONSECUTIVE_AUTO_ACTIONS {
                grace[seat] = 0;
                present_run[seat] = 0;
                continue;
            }
            if took_part(self, seat) {
                present_run[seat] = present_run[seat].saturating_add(1);
                if present_run[seat] >= REPLENISH_AFTER && grace[seat] < GRACE_HANDS {
                    grace[seat] += 1;
                    present_run[seat] = 0;
                }
            } else {
                grace[seat] = grace[seat].saturating_sub(1);
                present_run[seat] = 0;
            }
        }

        // `R(k+1) = P(k)`: the seats that demonstrably took part in hand `k`.
        // A seat that signed nothing is outside the required set from here on,
        // which is D-013 and is how one silent seat costs exactly one hand
        // rather than the table.
        // Where certificates are possible this is `R(k)` less the seats the
        // table certified absent, which is derived from the previous genesis
        // and from evidence every peer holds. A seat cannot join `R` by being
        // heard from: it joins by being in the roster the table ratified.
        let required: Vec<SeatIdx> = if by_certificate {
            self.open
                .required
                .iter()
                .copied()
                .filter(|s| {
                    took_part(self, usize::from(*s))
                        && alive.get(usize::from(*s)).copied().unwrap_or(false)
                })
                .collect()
        } else {
            // **`R(k)`, never every seat.** This branch used `0..max_players`,
            // so it could ADD a seat — and it is reached exactly when the
            // certified branch has just shrunk the roster to two, because that
            // branch is gated on `required.len() >= 3`. Measured, four hands:
            //
            //   hand 1 [0, 1, 2] -> seat 2 certified -> required [0, 1]
            //   hand 2 [0, 1]    -> observation, `signed` holds 2 -> [0, 1, 2]
            //   hand 3 [0, 1, 2] -> seat 2 certified again -> [0, 1]
            //   hand 4 [0, 1]    -> ...
            //
            // A seat certified absent came back every other hand and was
            // certified out again, for ever. `R(k+1) ⊆ R(k)` closes it: the
            // roster is monotone, whichever branch derives it.
            //
            // The cost is stated rather than hidden: D-013 says one silent seat
            // costs one hand and not the table, and under this a seat dropped
            // heads-up does not return. That is a rules question and it is in
            // NEXT.md; what is not a question is that the two branches must not
            // have opposite policies.
            self.open
                .required
                .iter()
                .copied()
                .filter(|s| {
                    took_part(self, usize::from(*s))
                        && alive.get(usize::from(*s)).copied().unwrap_or(false)
                })
                .collect()
        };
        if required.len() < 2 {
            return None;
        }

        // **After `required`, because the genesis commits to it.** Two peers
        // that derive different participation must derive different hands, or
        // they play different tables under one hash and nothing refuses
        // anything.
        let genesis = crate::protocol::transcript::genesis_hand(
            &self.open.table_id,
            hand_id,
            &self.open.session_id,
            &roster_hash,
            &terminal,
            &required,
        );

        let next_hand_id = self.open.hand_id + 1;
        let next_small_blind = crate::poker::tournament::small_blind_at(
            u32::try_from(next_hand_id).unwrap_or(u32::MAX),
            u32::from(self.open.every_n_hands),
            self.open.first_small_blind,
            self.open.small_blind_cap,
        );
        Some(Opening {
            table_id: self.open.table_id,
            hand_id,
            session_id: self.open.session_id,
            roster_hash,
            genesis,
            required,
            // **Filled by the caller, because the set outlives this hand.**
            // `A` is written by a stale checkpoint-8 `STATE_HASH` or
            // `PLAYER_SIT_IN` of a **completed** hand, which arrives at the
            // node loop after this hand has stopped holding anything, and is
            // read and cleared at exactly one place: the next hand init. A
            // `Hand` cannot hold it, because the hand it belongs to is over.
            readmitted: Vec::new(),
            seats,
            max_players: self.open.max_players,
            // **Derived, not copied**, which is the whole of the fix. §7.2:
            // `small_blind(h) = min(first_small_blind · 2^(⌊(h-1)/every_n⌋),
            // small_blind_cap)`, and §7.2 rule 2 forces
            // `big_blind == 2 × small_blind` on every advert, so the big blind
            // is that and nothing else. Every peer computes it from the same
            // signed parameters and the same hand number, so it is agreement by
            // construction rather than by message.
            small_blind: next_small_blind,
            big_blind: next_small_blind.saturating_mul(2),
            level: u16::try_from(crate::poker::tournament::blind_level(
                u32::try_from(next_hand_id).unwrap_or(u32::MAX),
                u32::from(self.open.every_n_hands),
            ))
            .unwrap_or(u16::MAX),
            every_n_hands: self.open.every_n_hands,
            first_small_blind: self.open.first_small_blind,
            small_blind_cap: self.open.small_blind_cap,
            my_seat: self.open.my_seat,
            crypto_step_timeout_ms: self.open.crypto_step_timeout_ms,
            action_timeout_ms: self.open.action_timeout_ms,
            action_grace_ms: self.open.action_grace_ms,
            hand_delay_ms: self.open.hand_delay_ms,
            time_bank_ms: self.open.time_bank_ms,
            hand_deadline_ms: self.open.hand_deadline_ms,
            grace,
            present_run,
            button: Some(positions.button),
        })
    }

    /// How long this client may take on its own turn before it acts for its
    /// owner.
    ///
    /// **`action_timeout_ms + action_grace_ms`**, which is `PROTOCOL.md` §8.2's
    /// deadline for a betting action and not a number of this client's
    /// choosing. The player's own clock is the shorter one — the timeout — and
    /// the grace is what absorbs the round trip, so a client that folded at the
    /// timeout would be folding hands that had in fact been played in time.
    pub fn action_deadline(&self) -> std::time::Duration {
        std::time::Duration::from_millis(
            u64::from(self.open.action_timeout_ms)
                .saturating_add(u64::from(self.open.action_grace_ms))
                .saturating_add(u64::from(self.bank_left_ms)),
        )
    }

    /// What is left of this client's own thinking reserve, for the table to
    /// show its owner. Nobody else's is knowable, and none is shown.
    pub fn bank_left(&self) -> std::time::Duration {
        std::time::Duration::from_millis(u64::from(self.bank_left_ms))
    }

    /// What the player sees: their own clock, without the transport's share.
    pub fn action_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(u64::from(self.open.action_timeout_ms))
    }

    /// `next_deadline_ms` for the stage this event opens.
    ///
    /// **Normative, not the emitter's choice** (`PROTOCOL.md` §8.2): a
    /// deterministic function of the table parameters and the kind of the next
    /// stage, and *"a peer that writes a different value emits an invalid
    /// event"*. It is a function of the event being sealed because the event
    /// that completes stage `s` is what arms stage `s+1`.
    fn next_deadline_for(&self, kind: EventType) -> u32 {
        match kind {
            // A betting action opens another betting stage — or a reveal, and
            // the reveal's own arming is the same number either way only by
            // coincidence, so it is written per row rather than shared.
            EventType::ActionCheck
            | EventType::ActionCall
            | EventType::ActionBet
            | EventType::ActionRaise
            | EventType::ActionFold => self
                .open
                .action_timeout_ms
                .saturating_add(self.open.action_grace_ms)
                // **The whole reserve, not what the seat has left of it.**
                // What a seat has left is known only to that seat, and §8.2
                // requires every peer to derive the same number. Budgeting the
                // maximum costs a certificate against a genuinely absent seat
                // one reserve's delay; budgeting anything less would let the
                // table certify a player who was still legitimately thinking,
                // which is the one thing D-023's machinery must never do.
                .saturating_add(self.open.time_bank_ms),
            // A hand boundary: the pause before the next deal, and then the
            // first cryptographic stage of it.
            EventType::HandComplete | EventType::HandAbort => self
                .open
                .hand_delay_ms
                .saturating_add(self.open.crypto_step_timeout_ms),
            // Everything else this driver emits opens a cryptographic stage.
            _ => self.open.crypto_step_timeout_ms,
        }
    }

    /// Whether the betting is over and the hand is waiting to be settled.
    pub fn betting_over(&self) -> bool {
        matches!(
            &self.phase,
            Phase::Playing { play, .. } if matches!(play.step, Step::Ended)
        )
    }

    /// Which seats are still owed for the stage now open, by seat.
    ///
    /// For a reveal stage this is the seats whose shares have not arrived, and
    /// it is what a table window puts next to "waiting for".
    pub fn outstanding_shares(&self, index: crate::mental_poker::deck::CardIndex)
        -> Option<Vec<SeatIdx>>
    {
        match &self.phase {
            Phase::Dealing { dealing, .. } => dealing.outstanding(index),
            Phase::Playing { play, .. } => play.dealing.outstanding(index),
            _ => None,
        }
    }

    /// Whose turn it is to shuffle, or `None` once the chain has closed.
    pub fn shuffler(&self) -> Option<SeatIdx> {
        match &self.phase {
            Phase::Shuffling { chain, .. } => chain.whose_turn(),
            _ => None,
        }
    }

    /// Which seats the stage now open is still waiting for.
    /// The application key seated at `seat` in this hand.
    ///
    /// Used to name a seat to the carrier: `waiting_for` says which seats a
    /// stage is missing, and the carrier knows peers by key.
    pub fn key_of(&self, seat: SeatIdx) -> Option<[u8; 32]> {
        self.open
            .seats
            .iter()
            .find(|(s, _, _)| *s == seat)
            .map(|(_, key, _)| *key)
    }

    pub fn waiting_for(&self) -> Vec<SeatIdx> {
        match &self.phase {
            Phase::Init(stage) => stage.waiting_for(),
            Phase::Deck { stage, .. } => stage.waiting_for(),
            // One seat at a time: a single-writer stage is waiting for exactly
            // the shuffler whose turn it is, and naming the others would put
            // seats on screen that owe the table nothing.
            Phase::Shuffling { chain, .. } => chain.whose_turn().into_iter().collect(),
            Phase::Committing { stage, .. } | Phase::Dealing { stage, .. } => stage.waiting_for(),
            // Once the cards are out the answer is still "who owes the open
            // stage", and it was `Vec::new()` — which made the whole deadline
            // path blind to a betting stall, because that path asks exactly
            // this question and would have found nobody to vote about.
            Phase::Playing { play, .. } => match &play.step {
                Step::Acting { to_act } => vec![*to_act],
                Step::Opening { stage, .. }
                | Step::Showdown { stage, .. }
                | Step::Settling { stage, .. } => stage.waiting_for(),
                Step::Ended => Vec::new(),
            },
            Phase::Aborted(_) | Phase::Between => Vec::new(),
        }
    }

    /// The slot of the stage now open. After stage 0 completes this is stage
    /// 1's, which is what makes it the thing to compare between two peers.
    pub fn slot(&self) -> Slot {
        self.slot
    }

    /// This client's own secret for the hand, once it has one.
    ///
    /// Held across the stage transition and never sent: it is what will decrypt
    /// this seat's own cards, and a hand that lost it would be a player who
    /// cannot read the hand they are in. Borrowed rather than copied, and
    /// `HandSecret`'s `Debug` never prints it.
    pub fn secret(&self) -> Option<&HandSecret> {
        match &self.phase {
            Phase::Deck { secret, .. } => Some(secret),
            Phase::Shuffling { deal, .. }
            | Phase::Committing { deal, .. }
            | Phase::Dealing { deal, .. }
            | Phase::Playing { deal, .. } => Some(&deal.secret),
            Phase::Init(_) | Phase::Aborted(_) | Phase::Between => None,
        }
    }

    /// The deck, once every seat's key has been verified.
    pub fn deck(&self) -> Option<&HandDeck> {
        match &self.phase {
            Phase::Shuffling { deal, .. }
            | Phase::Committing { deal, .. }
            | Phase::Dealing { deal, .. }
            | Phase::Playing { deal, .. } => Some(&deal.deck),
            _ => None,
        }
    }

    /// The verified keys of the seats holding the deck, in arrival order.
    pub fn keys(&self) -> &[VerifiedKey] {
        match &self.phase {
            Phase::Deck { keys, .. } => keys,
            Phase::Shuffling { deal, .. }
            | Phase::Committing { deal, .. }
            | Phase::Dealing { deal, .. }
            | Phase::Playing { deal, .. } => &deal.keys,
            Phase::Init(_) | Phase::Aborted(_) | Phase::Between => &[],
        }
    }

    /// The final deck, once the chain has closed.
    pub fn final_deck(&self) -> Option<&Final<Verified<Vec<Ciphertext>>>> {
        match &self.phase {
            Phase::Committing { table, .. }
            | Phase::Dealing { table, .. }
            | Phase::Playing { table, .. } => Some(&table.deck),
            _ => None,
        }
    }

    /// The deck-index map, once it has been fixed.
    pub fn index_map(&self) -> Option<&DeckIndexMap> {
        match &self.phase {
            Phase::Committing { table, .. }
            | Phase::Dealing { table, .. }
            | Phase::Playing { table, .. } => Some(&table.map),
            _ => None,
        }
    }

    pub fn init(&self) -> &HandInit {
        &self.mine
    }

    pub fn hand_id(&self) -> u64 {
        self.open.hand_id
    }
}

/// The canonical bytes of a deck: fifty-two ciphertexts, end to end.
fn flatten(deck: &[Ciphertext]) -> Vec<u8> {
    let mut out = Vec::with_capacity(deck.len() * CIPHERTEXT);
    for card in deck {
        out.extend_from_slice(card);
    }
    out
}

/// The reverse, refusing anything that is not exactly a deck.
///
/// A short deck, a long one and a deck whose length is not a multiple of a
/// ciphertext are one answer - `None` - because none of them is a deck and
/// telling a sender which way it was wrong tells it nothing worth knowing.
fn unflatten(bytes: &[u8]) -> Option<Vec<Ciphertext>> {
    if bytes.len() != DECK * CIPHERTEXT {
        return None;
    }
    Some(
        bytes
            .chunks_exact(CIPHERTEXT)
            .map(|c| {
                let mut card = [0u8; CIPHERTEXT];
                card.copy_from_slice(c);
                card
            })
            .collect(),
    )
}

/// `h("p2p-poker v1 deck-commit", [deck bytes])`, as `PROTOCOL.md` §4.5 defines
/// the two hashes a `SHUFFLE_PROOF` carries.
fn deck_hash(deck: &[Ciphertext]) -> Hash {
    h(Domain::DeckCommit.context(), &[&flatten(deck)])
}

/// The input hash for a step, including the first one.
///
/// The first link's input is the **open deck**, which the crypto library owns
/// and never surfaces as ciphertexts - `verify_initial_shuffle` is a separate
/// entry point precisely because the open deck is not a deck anyone shuffled.
/// So round 0 names a constant instead. It loses nothing: the open deck is the
/// same in every hand at every table, so its hash binds a proof to nothing, and
/// all of round 0's binding comes from the context, which carries the table,
/// the session, the hand, the sequence and the shuffler's own key.
fn input_deck_hash(prev: Option<&Verified<Vec<Ciphertext>>>) -> Hash {
    match prev {
        Some(v) => deck_hash(v.as_ref()),
        None => h(Domain::DeckCommit.context(), &[b"the open deck"]),
    }
}

impl Deal {
    /// The verified key a seat committed to in `DECK_INIT`.
    fn key_of(&self, seat: SeatIdx) -> Option<&VerifiedKey> {
        self.by_seat.get(usize::from(seat))?.as_ref()
    }

    /// The borrowed view `table::dealing` takes.
    ///
    /// A function rather than a stored struct because it borrows three things
    /// that live in two different places, and a stored one would pin them all
    /// for the life of the phase.
    fn as_ref<'a>(&'a self, table: &'a Table) -> dealing::Deal<'a> {
        dealing::Deal {
            hand: &self.deck,
            keys: &self.by_seat,
            deck: &table.deck,
        }
    }
}

/// Turn a refused share into this hand's failure, keeping who is answerable.
///
/// `Refused::NotDue` is the interesting one: it means the sender published a
/// share the rules do not allow at this point — its own hole card before
/// showdown, or a board index whose street has not been reached. Both are
/// attributable and both are named as such.
fn refused(seat: SeatIdx, e: Refused) -> Failed {
    match e {
        Refused::NotDue(_) => Failed::BadToken {
            seat,
            why: "a share the rules do not allow at this point in the hand",
        },
        // **The one arm that is evidence, and it is separated here rather than
        // at the four call sites.** `VerifyOutcome` already carries the
        // distinction the whole of `cause = 3` rests on: `Invalid` is a finding
        // about the signer, `CouldNotVerify` is a finding about this peer's own
        // ability to check and is *never* evidence against anybody.
        Refused::DidNotVerify(VerifyOutcome::Invalid(_)) => Failed::RevealDisproved { seat },
        Refused::DidNotVerify(_) => Failed::BadToken {
            seat,
            why: "the proof could not be checked against the committed deck here",
        },
        Refused::NotWanted(_) => Failed::BadToken {
            seat,
            why: "a second share for one card from one seat",
        },
        Refused::UnknownSeat { .. } => Failed::BadToken {
            seat,
            why: "no verified deck key for this seat",
        },
        Refused::NoSuchCard { .. } => Failed::BadToken {
            seat,
            why: "an index this hand gave no role to",
        },
    }
}

impl Play {
    /// Everything committed this hand, across every street.
    ///
    /// `paid` holds the closed streets and `round.committed` the open one; the
    /// engine keeps only the second, which is why the first exists here.
    fn pot(&self) -> Chips {
        self.paid.iter().chain(self.round.committed.iter()).sum()
    }
}

/// Whose turn it is and what they may do.
///
/// Every field is derived from the same `BettingRound` the receiver of the
/// action will run, so a window cannot offer something a peer would refuse.
pub struct Turn {
    pub seat: SeatIdx,
    /// Whether that seat is this client's.
    pub mine: bool,
    pub street: Street,
    /// What this seat owes to match the current bet.
    pub to_call: Chips,
    pub legal: LegalActions,
    /// Everything committed this hand so far.
    pub pot: Chips,
}

/// A board of exactly five cards, or nothing.
///
/// `evaluate_holdem` wants `&[Card; 5]` and the board is a `Vec` that grows
/// three, four, five. The conversion is here rather than at the call sites so
/// that "the river is out" is asked once.
fn five_card_board(board: &[Card]) -> Option<[Card; 5]> {
    <[Card; 5]>::try_from(board).ok()
}

/// A certificate stage that is being filled.
///
/// Collective in its own right, at the subject stage's own `sequence` and in
/// `event_class = 2`, so it references the stalled stage rather than occupying
/// it. Its `stage_hash` is taken over the **whole** set of certificates rather
/// than over one chosen copy — which is what stops two honest emitters who
/// embedded different valid signatures for one vote from forking the stage.
struct Certifying {
    subject: TimeoutVote,
    stage: Collective,
}

/// What follows a closed betting round.
///
/// Three roads and no fourth: the board opens, the showdown opens, or the hand
/// is over and is settled. Named rather than left as nested `Option`s, because
/// the three are not degrees of one thing.
enum After {
    Board(Street),
    Showdown,
    Settle,
}

/// Which of the two action bodies is being sealed.
///
/// `say` is generic over the body, so the two cannot be one variable without a
/// trait object. This is the alternative, named.
enum Body {
    Head(ActionHead),
    Amount(ActionAmount),
}

/// Every hole-card index of the hand, ascending.
///
/// `0 .. 2m` by construction of the map: the first hole cards then the second
/// ones, in deal order. Built from the map rather than from `2 * m` written out
/// here, so that a change to the map is a change in one place.
fn every_hole_index(map: &DeckIndexMap) -> Vec<CardIndex> {
    let mut out = Vec::with_capacity(2 * map.m());
    for seat in map.deal_order() {
        if let Some([first, _]) = map.hole_cards(*seat) {
            out.push(first);
        }
    }
    for seat in map.deal_order() {
        if let Some([_, second]) = map.hole_cards(*seat) {
            out.push(second);
        }
    }
    out
}

/// Turn a chain refusal into this hand's failure, keeping who is answerable.
fn step_failure(seat: SeatIdx, e: StepError) -> Failed {
    match e {
        StepError::NotYourTurn { expected, .. } => Failed::OutOfTurn {
            seat,
            expected: Some(expected),
        },
        StepError::Closed => Failed::OutOfTurn {
            seat,
            expected: None,
        },
        StepError::AlreadySubmitted => Failed::BadShuffle {
            seat,
            why: "a second attempt at a position that already has one",
        },
        StepError::OutOfRange { .. } => Failed::BadShuffle {
            seat,
            why: "a seat and a position this chain admits",
        },
        StepError::Rejected(_) => Failed::BadShuffle {
            seat,
            why: "the argument does not hold for this pair of decks",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    const NOW: u64 = 1_700_000_000_000;

    fn opening(my_seat: SeatIdx) -> Opening {
        Opening {
            table_id: [1; 32],
            hand_id: 1,
            session_id: [2; 32],
            roster_hash: [3; 32],
            genesis: [4; 32],
            required: vec![0, 1],
            readmitted: Vec::new(),
            // The rated preset's schedule, so a test hand doubles
            // where a real one does.
            every_n_hands: 11,
            first_small_blind: 50,
            small_blind_cap: 50_000,
            seats: vec![
                (0, key(10).verifying_key().to_bytes(), 10_000),
                (1, key(11).verifying_key().to_bytes(), 10_000),
            ],
            max_players: 2,
            small_blind: 50,
            big_blind: 100,
            level: 1,
            my_seat,
            crypto_step_timeout_ms: 30_000,
            action_timeout_ms: 20_000,
            action_grace_ms: 5_000,
            hand_delay_ms: 7_000,
            time_bank_ms: 0,
            hand_deadline_ms: 600_000,
            grace: vec![GRACE_HANDS; 3],
            present_run: vec![0; 3],
            button: None,
        }
    }

    /// Five seats, which is the smallest table at which a certified-out seat's
    /// own vote is not masked by the two-voter floor.
    ///
    /// At three seats, certifying one out leaves one voter, and
    /// `voters(seat).len() < 2` refuses the vote before anything else is
    /// checked. At five it leaves three, the floor passes, and the question of
    /// whether this client belongs in its own voter set is finally asked. That
    /// is why nothing in this repo caught it: every table-opening test here
    /// used three seats or two.
    fn opening5(my_seat: SeatIdx) -> Opening {
        Opening {
            table_id: [1; 32],
            hand_id: 1,
            session_id: [2; 32],
            roster_hash: [3; 32],
            genesis: [4; 32],
            required: vec![0, 1, 2, 3, 4],
            readmitted: Vec::new(),
            every_n_hands: 11,
            first_small_blind: 50,
            small_blind_cap: 50_000,
            seats: (0..5u8)
                .map(|s| (s, key(10 + s).verifying_key().to_bytes(), 10_000u64))
                .collect(),
            max_players: 5,
            small_blind: 50,
            big_blind: 100,
            level: 1,
            my_seat,
            crypto_step_timeout_ms: 30_000,
            action_timeout_ms: 20_000,
            action_grace_ms: 5_000,
            hand_delay_ms: 7_000,
            time_bank_ms: 0,
            hand_deadline_ms: 600_000,
            grace: vec![GRACE_HANDS; 5],
            present_run: vec![0; 5],
            button: None,
        }
    }

    /// **A seat the table has certified out must stop voting**, and it had not.
    ///
    /// `on_timeout_vote` refuses a vote from a seat outside `voters(subject)`.
    /// The emit side had no such test and `take_vote` inserted `my_seat`
    /// unconditionally, so a certified-out seat went on voting for itself,
    /// `certify_if_unanimous` saw one more vote than there were voters, sealed,
    /// and broadcast a certificate carrying a voter nobody else had in
    /// `dealt_in` — which every receiver then refused.
    ///
    /// Measured in `split215300-10`: **443** certificates refused table-wide,
    /// **394 from seat 2, 25 from seat 0, 24 from seat 9**, against roster
    /// narrowings of `certified [2]`, `[4]`, `[7]`, `[8]` and `[9, 0]`. Every
    /// refusal came from a seat that had been certified out. The fingerprint is
    /// a tally over its own denominator — `4/3`, `7/6`, `8/7`, `9/8` — always
    /// exactly one too many, always its own.
    #[test]
    fn a_certified_out_seat_does_not_vote_for_itself() {
        let (mut hand, _) =
            Hand::open(opening5(4), &key(14), NOW, 30_000).expect("the hand opens");

        // Before: seat 4 is a voter about seat 0 like anyone else.
        assert!(
            hand.voters(0).contains(&4),
            "the fixture must start with seat 4 inside the voter set",
        );

        // The table certifies seat 4 out. That is the state the run was in.
        hand.commit_certificate(4);
        assert!(
            !hand.voters(0).contains(&4),
            "a certified seat must leave the voter set",
        );
        assert!(
            hand.voters(0).len() >= 2,
            "and the two-voter floor must still pass, or this test proves \
             nothing that three seats would not have",
        );

        // It must now say nothing, whatever its clock thinks.
        let out = hand
            .vote_on_timeouts(&key(14), NOW + 600_000, 0)
            .expect("voting is not an error");
        assert!(
            out.is_empty(),
            "a seat outside its own voter set voted anyway, and every receiver \
             will refuse the certificate that vote is sealed into",
        );
    }

    /// Three seats, which is the smallest table at which a collective stage
    /// can still be open after one peer has been heard.
    ///
    /// Heads-up hides a whole class of defect: with two seats every collective
    /// stage completes on the first message that arrives, so a second copy of
    /// it lands at a sequence the hand has already left and is dropped before
    /// any handler sees it. At three it lands *in* the open stage, which is
    /// where duplicate handling is actually exercised.
    fn opening3(my_seat: SeatIdx) -> Opening {
        Opening {
            table_id: [1; 32],
            hand_id: 1,
            session_id: [2; 32],
            roster_hash: [3; 32],
            genesis: [4; 32],
            required: vec![0, 1, 2],
            readmitted: Vec::new(),
            // The rated preset's schedule, so a test hand doubles
            // where a real one does.
            every_n_hands: 11,
            first_small_blind: 50,
            small_blind_cap: 50_000,
            seats: vec![
                (0, key(10).verifying_key().to_bytes(), 10_000),
                (1, key(11).verifying_key().to_bytes(), 10_000),
                (2, key(12).verifying_key().to_bytes(), 10_000),
            ],
            max_players: 3,
            small_blind: 50,
            big_blind: 100,
            level: 1,
            my_seat,
            crypto_step_timeout_ms: 30_000,
            action_timeout_ms: 20_000,
            action_grace_ms: 5_000,
            hand_delay_ms: 7_000,
            time_bank_ms: 0,
            hand_deadline_ms: 600_000,
            grace: vec![GRACE_HANDS; 3],
            present_run: vec![0; 3],
            button: None,
        }
    }

    /// Deliver one client's output to the other, and hand back whatever that
    /// produced. A stage completing is what emits the next stage's message, so
    /// a test that dropped the return value would stall one step in.
    fn deliver(to: &mut Hand, sends: &[Send], key: &SigningKey) -> Vec<Send> {
        let mut out = Vec::new();
        for (i, Send::Broadcast(bytes)) in sends.iter().enumerate() {
            let at = to.slot().sequence;
            match to.on_event(bytes, key, NOW) {
                Ok(mut more) => out.append(&mut more),
                Err(e) => panic!("event {i} of {} at stage {at}: {e}", sends.len()),
            }
        }
        out
    }

    /// The whole milestone, with no network: two independent states, each
    /// hearing the other, both reaching the same parent for stage 1.
    #[test]
    fn two_clients_open_hand_one() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();

        assert!(!a.dealt() && !b.dealt(), "neither has heard the other yet");
        assert_eq!(a.waiting_for(), vec![1]);

        // Stage 0 completes, and completing it is what produces each side's
        // `DECK_INIT` — so the deal and the deck are one exchange, not two.
        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));

        assert!(a.dealt() && b.dealt(), "stage 0 is behind them");
        assert_eq!(
            a.slot(),
            b.slot(),
            "two peers must hang stage 1 off one parent"
        );
        assert_eq!(a.slot().sequence, 1);
        assert_eq!(b_deck.len(), 1, "completing stage 0 offers a deck key");
        assert_eq!(a_deck.len(), 1);

        // And stage 1: each verifies the other's key, the deck exists, and
        // the first shuffler takes its turn without being asked to.
        let b_more = deliver(&mut b, &a_deck, &key(11));
        let a_more = deliver(&mut a, &b_deck, &key(10));
        assert!(a.deck_ready() && b.deck_ready(), "every key verified");
        assert_eq!(b.slot().sequence, 2, "stage 2 is the first shuffle");
        assert_eq!(b.shuffler(), Some(0), "and seat 0 is up");
        assert!(b_more.is_empty(), "seat 1 is not the first shuffler");
        assert_eq!(a_more.len(), 2, "seat 0 sends a step and its proof");
    }

    /// The whole chain, between two processes' worth of state: two seats, two
    /// links, and a deck that is final at the end of it.
    ///
    /// Slow on purpose. It generates two Bayer-Groth arguments and verifies
    /// four - each client checks its own as well as the other's, because there
    /// is one path into the chain and no trusted shortcut along it.
    #[test]
    fn two_clients_shuffle_the_deck() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();

        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        deliver(&mut b, &a_deck, &key(11));
        // Seat 0 completes stage 1 and shuffles in the same call.
        let a_shuffle = deliver(&mut a, &b_deck, &key(10));
        assert_eq!(a_shuffle.len(), 2);
        assert_eq!(a.shuffler(), Some(1), "seat 0 is through; seat 1 is up");
        assert_eq!(a.waiting_for(), vec![1], "one seat at a time");

        // Seat 1 hears the pair, accepts it, and takes its own turn - again in
        // one call, because accepting the last step is what makes it your turn.
        // Its own step closes the chain, so the commitment follows in the same
        // breath: three messages out of one delivery.
        let b_shuffle = deliver(&mut b, &a_shuffle, &key(11));
        assert_eq!(b_shuffle.len(), 3, "seat 1's step, its proof, and the commit");
        assert!(b.shuffled(), "seat 1 saw its own step close the chain");
        assert_eq!(
            b.slot().sequence,
            6,
            "two seats: stages 2,3 and 4,5, so the commit is stage 6"
        );

        let a_more = deliver(&mut a, &b_shuffle, &key(10));
        assert!(a.shuffled(), "and so did seat 0");
        assert_eq!(
            a.final_deck().unwrap().as_ref().as_ref(),
            b.final_deck().unwrap().as_ref().as_ref(),
            "and on one deck"
        );
        assert_eq!(
            a.index_map().unwrap().index_map_hash(),
            b.index_map().unwrap().index_map_hash(),
            "and one map of whose card is whose"
        );
        // A's chain closed on B's proof, so A commits; then B's commit
        // completes the barrier and A deals.
        assert_eq!(a_more.len(), 2, "A's commit and its shares");
    }

    /// Two events held in the wrong order still both land.
    ///
    /// The mesh does not order anything, so a peer one stage ahead arrives as a
    /// burst in whatever order the network chose. The first version of the
    /// replay dropped anything still early and ran one pass, which turned that
    /// ordinary case into a table that stops with no error anywhere.
    #[test]
    fn events_held_out_of_order_are_replayed_until_they_fit() {
        // This client is seat 1, so seat 0 shuffles first and can run ahead.
        let (mut a, from_a) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();

        let a_deck = deliver(&mut a, &from_b, &key(11));
        let b_deck = deliver(&mut b, &from_a, &key(10));
        let b_shuffle = deliver(&mut b, &a_deck, &key(10));
        assert_eq!(b_shuffle.len(), 2, "seat 0's step and its proof");

        // Seat 1 has not heard seat 0's deck key yet, and the shuffle arrives
        // first — proof before step, which is the harder order.
        let Send::Broadcast(step) = &b_shuffle[0];
        let Send::Broadcast(proof) = &b_shuffle[1];
        assert_eq!(a.on_event(proof, &key(11), NOW), Err(Failed::NotYet));
        a.hold(proof.clone());
        assert_eq!(a.on_event(step, &key(11), NOW), Err(Failed::NotYet));
        a.hold(step.clone());
        assert_eq!(a.held(), 2);

        // The key that unblocks them.
        let Send::Broadcast(bd) = &b_deck[0];
        assert!(a.on_event(bd, &key(11), NOW).unwrap().is_empty());
        assert_eq!(a.held(), 2, "still both, and both still early");

        let (more, failures) = a.replay_early(&key(11), NOW);
        assert!(failures.is_empty(), "{failures:?}");
        assert_eq!(a.held(), 0, "nothing left parked");
        assert!(a.shuffled(), "seat 1 took its turn and closed the chain");
        assert_eq!(
            more.len(),
            3,
            "seat 1's step, its proof, and the commit its own step unlocks"
        );
    }

    /// Three peers play the whole deal out with **every message delivered
    /// twice**, which is what a mesh does.
    ///
    /// This is the defect the order of two lines used to cause in
    /// `on_deal_private`: the shares were recorded before `stage.hear` ran, and
    /// a token set takes one share per seat and never an update — so an
    /// ordinary redelivery came back as `BadToken` against a peer that had done
    /// nothing but send its message twice.
    ///
    /// It also happens to be the first three-handed hand in the tree, which
    /// exercises a shuffle chain of three links and a `DEAL_PRIVATE` of four
    /// entries rather than two.
    #[test]
    fn every_message_delivered_twice_still_deals() {
        let (mut a, from_a) = Hand::open(opening3(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening3(1), &key(11), NOW, 30_000).unwrap();
        let (mut c, from_c) = Hand::open(opening3(2), &key(12), NOW, 30_000).unwrap();

        let keys = [key(10), key(11), key(12)];
        let mut pending: Vec<(SeatIdx, Vec<Send>)> = vec![(0, from_a), (1, from_b), (2, from_c)];

        // Bounded, so a hand that fails to converge fails the test rather than
        // running for ever.
        for _ in 0..64 {
            if pending.is_empty() {
                break;
            }
            let mut next: Vec<(SeatIdx, Vec<Send>)> = Vec::new();
            for (from, sends) in std::mem::take(&mut pending) {
                for seat in 0..3u8 {
                    if seat == from {
                        continue;
                    }
                    let hand: &mut Hand = match seat {
                        0 => &mut a,
                        1 => &mut b,
                        _ => &mut c,
                    };
                    let out = deliver(hand, &sends, &keys[usize::from(seat)]);
                    // And again. A second copy must be weather: no fault, and
                    // nothing new to say.
                    let again = deliver(hand, &sends, &keys[usize::from(seat)]);
                    assert!(
                        again.is_empty(),
                        "a duplicate produced {} message(s) of its own",
                        again.len()
                    );
                    if !out.is_empty() {
                        next.push((seat, out));
                    }
                }
            }
            pending = next;
        }

        for (seat, hand) in [(0u8, &a), (1, &b), (2, &c)] {
            assert!(hand.dealt_cards(), "seat {seat} did not reach its own cards");
            assert_eq!(hand.slot(), a.slot(), "all three left the deal together");
        }
        let all: Vec<[Card; 2]> = [&a, &b, &c].iter().map(|h| h.cards().unwrap()).collect();
        for i in 0..3 {
            for j in (i + 1)..3 {
                for card in all[i] {
                    assert!(
                        !all[j].contains(&card),
                        "one deck: seats {i} and {j} share a card"
                    );
                }
            }
        }
    }

    /// Two clients, heads-up, play the pre-flop round and the flop appears.
    ///
    /// The whole point of the loop is that it never says whose turn it is: it
    /// asks. `provisional_button` picks the button from `session_id`, so which
    /// seat is the small blind is not this test's business, and a test that
    /// hard-coded it would pass for the wrong reason.
    #[test]
    fn two_clients_bet_and_the_flop_opens() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        let mut queue: Vec<(SeatIdx, Vec<Send>)> = Vec::new();
        queue.push((1, deliver(&mut b, &a_deck, &key(11))));
        queue.push((0, deliver(&mut a, &b_deck, &key(10))));

        let keys = [key(10), key(11)];
        for _ in 0..64 {
            // Anything in flight is delivered first.
            if let Some((from, sends)) = queue.pop() {
                if sends.is_empty() {
                    continue;
                }
                let to = 1 - from;
                let hand: &mut Hand = if to == 0 { &mut a } else { &mut b };
                let out = deliver(hand, &sends, &keys[usize::from(to)]);
                queue.push((to, out));
                continue;
            }
            // Nothing in flight. Somebody owes an action, or the flop is out.
            if a.board().len() == 3 {
                break;
            }
            let seat = match a.turn() {
                Some(t) => t.seat,
                None => panic!("nothing in flight and nobody to act"),
            };
            let hand: &mut Hand = if seat == 0 { &mut a } else { &mut b };
            let turn = hand.turn().expect("that hand agrees it is to act");
            assert!(turn.mine, "each client acts only for itself");
            // Call if anything is owed, check otherwise — the cheapest way to
            // the flop, and it exercises both.
            let action = if turn.legal.can_check {
                Action::Check
            } else {
                Action::Call
            };
            let out = hand.act(action, &keys[usize::from(seat)], NOW).unwrap();
            queue.push((seat, out));
        }

        assert_eq!(a.board().len(), 3, "the flop is on the board");
        assert_eq!(a.board(), b.board(), "and it is the same flop");
        assert_eq!(a.street(), Some(Street::Flop));
        assert_eq!(b.street(), Some(Street::Flop));
        assert_eq!(
            a.pot(),
            2 * 100,
            "heads-up, the small blind called and the big blind checked"
        );
        assert_eq!(a.slot(), b.slot(), "one chain, one stage");
        for card in a.board() {
            assert!(
                !a.cards().unwrap().contains(&card) && !b.cards().unwrap().contains(&card),
                "one deck: a board card is nobody's hole card"
            );
        }
    }

    /// A whole hand, checked down to a showdown.
    ///
    /// Four streets, five board cards, and one of the two seats putting its
    /// hand on the table. Which one is not this test's business — the TDA order
    /// falls out of the button, and the second seat shows or mucks by comparing
    /// against what it was shown, which is D-021.
    #[test]
    fn two_clients_play_a_hand_to_showdown() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        let mut queue: Vec<(SeatIdx, Vec<Send>)> = Vec::new();
        queue.push((1, deliver(&mut b, &a_deck, &key(11))));
        queue.push((0, deliver(&mut a, &b_deck, &key(10))));

        let keys = [key(10), key(11)];
        for _ in 0..256 {
            if let Some((from, sends)) = queue.pop() {
                if sends.is_empty() {
                    continue;
                }
                let to = 1 - from;
                let hand: &mut Hand = if to == 0 { &mut a } else { &mut b };
                let out = deliver(hand, &sends, &keys[usize::from(to)]);
                queue.push((to, out));
                continue;
            }
            if a.betting_over() && b.betting_over() {
                break;
            }
            // Nothing in flight. Somebody owes an action.
            let Some(turn) = a.turn().or_else(|| b.turn()) else {
                panic!("nothing in flight, nobody to act, and the hand is not over");
            };
            let seat = turn.seat;
            let hand: &mut Hand = if seat == 0 { &mut a } else { &mut b };
            let turn = hand.turn().expect("that hand agrees it is to act");
            let action = if turn.legal.can_check {
                Action::Check
            } else {
                Action::Call
            };
            let out = hand.act(action, &keys[usize::from(seat)], NOW).unwrap();
            queue.push((seat, out));
        }

        assert!(a.betting_over() && b.betting_over(), "the hand played out");
        assert_eq!(a.board().len(), 5, "every street opened");
        assert_eq!(a.board(), b.board(), "and both peers hold one board");
        assert_eq!(a.street(), Some(Street::River));
        assert_eq!(
            a.pot(),
            2 * 100,
            "checked down: two big blinds and nothing more"
        );
        assert_eq!(a.slot(), b.slot(), "one chain, one stage");

        // Exactly one of the two possibilities for each seat, and both peers
        // agree about which.
        let mut showed = 0;
        for seat in 0..2u8 {
            assert_eq!(a.shown(seat), b.shown(seat), "seat {seat}");
            assert_eq!(a.mucked(seat), b.mucked(seat), "seat {seat}");
            assert!(
                a.shown(seat).is_some() != a.mucked(seat),
                "seat {seat} either showed or mucked, never both and never neither"
            );
            if let Some(cards) = a.shown(seat) {
                showed += 1;
                let holder: &Hand = if seat == 0 { &a } else { &b };
                assert_eq!(
                    cards,
                    holder.cards().unwrap(),
                    "the hand on the table is the hand that seat held"
                );
            }
        }
        assert!(showed >= 1, "somebody has to show");

        // And the decision was the right one. A seat mucks only when it cannot
        // beat what is already on the table, which is D-021's rule and the
        // whole reason the client is allowed to decide without asking.
        let board = <[Card; 5]>::try_from(a.board().as_slice()).unwrap();
        for seat in 0..2u8 {
            if a.mucked(seat) {
                let holder: &Hand = if seat == 0 { &a } else { &b };
                let folded_hand = evaluate_holdem(holder.cards().unwrap(), &board);
                let best_shown = (0..2u8)
                    .filter_map(|s| a.shown(s))
                    .map(|h| evaluate_holdem(h, &board))
                    .max()
                    .expect("something was shown");
                assert!(
                    folded_hand < best_shown,
                    "seat {seat} mucked a hand that was not beaten"
                );
            }
        }

        // And the chips moved, identically on both peers.
        let stacks = a.stacks();
        assert_eq!(stacks, b.stacks(), "one settlement, not two");
        assert_eq!(
            stacks.iter().sum::<u64>(),
            2 * 10_000,
            "a hand may not create or destroy a chip"
        );

        // The pot went to the best hand shown. A seat that mucked forfeits
        // whatever it held, which is the point of forfeiting.
        let best = (0..2u8)
            .filter_map(|seat| a.shown(seat).map(|h| (seat, evaluate_holdem(h, &board))))
            .max_by_key(|(_, r)| *r)
            .expect("something was shown");
        let split = (0..2u8)
            .filter_map(|seat| a.shown(seat).map(|h| (seat, evaluate_holdem(h, &board))))
            .filter(|(_, r)| *r == best.1)
            .count();
        if split == 1 {
            assert_eq!(
                stacks[usize::from(best.0)],
                10_000 + 100,
                "the best hand shown took the other seat's blind"
            );
        } else {
            assert_eq!(stacks, vec![10_000, 10_000], "a tie splits it back");
        }
    }

    /// A readmitted seat is **accepted** at stage 0 and never **required**.
    ///
    /// **This is the door `S1-O` found missing**, and both halves of it matter.
    ///
    /// *Accepted*: without it a seat outside `P(k)` cannot send a `HAND_INIT`
    /// this receiver will look at, so it cannot be counted into `P(k+1)`, so it
    /// is never dealt in again. Measured: a client that reconnected, rejoined
    /// the table's group, and played nothing for the rest of a run.
    ///
    /// *Never required*: the write to `A` is reachable by replay — §4.0 step
    /// 10a is skipped for the stale events that write it — so if `A` enlarged
    /// the **required** set, one replayed and perfectly agreeing checkpoint-8
    /// `STATE_HASH` would stall stage 0 to the full hand deadline, once per
    /// hand, for every hand §5.3 retains a record of. No key and no forgery
    /// needed. That is `P2`, and it is why the widening is of the accepted set
    /// **The oscillation, re-measured against today's `grace`** — the rules
    /// question `NEXT.md` files as *"it is consistent and it may be wrong"*.
    ///
    /// `next_hand` derives `R(k+1)` by filtering `R(k)`, so the roster is
    /// monotone and a seat that misses a hand never returns. That rule was
    /// adopted because the alternative — deriving `R(k+1)` from `P(k)` — was
    /// measured oscillating: a seat certified absent *"came back every other
    /// hand and was certified out again, for ever"*.
    ///
    /// The whole alternative reduces to one question, which is what this
    /// measures: **does an accepted bystander's `HAND_INIT` put its seat into
    /// `P(k)`?** If it does, a seat that is present again re-enters `R` and
    /// D-013 is kept; if a seat that stays away also re-enters, the oscillation
    /// is real and the monotone rule is right.
    ///
    /// Both halves are measured here, and they differ — which is the answer.
    #[test]
    fn a_returning_seat_enters_p_of_k_and_an_absent_one_does_not() {
        // Hand two: seat 2 missed hand one, so it is outside the required set
        // and inside the readmission set §4.9 writes.
        let mut o = opening3(0);
        o.hand_id = 2;
        o.required = vec![0, 1];
        o.readmitted = vec![2];
        let (mut present, _) = Hand::open(o.clone(), &key(10), NOW, 30_000).unwrap();
        let (absent, _) = Hand::open(o.clone(), &key(10), NOW, 30_000).unwrap();

        // The returning seat opens the same hand and says its one piece.
        let mut two = opening3(2);
        two.hand_id = 2;
        two.required = vec![0, 1];
        two.readmitted = vec![2];
        let (_, from_two) = Hand::open(two, &key(12), NOW, 30_000).unwrap();
        let Send::Broadcast(bytes) = &from_two[0];
        present
            .on_event(bytes, &key(10), NOW)
            .expect("§4.9 admits a readmitted seat's HAND_INIT");

        // **The returning seat is in `P(k)`.** So under the alternative rule —
        // `R(k+1) = P(k)` — it is required again at hand three, which is
        // exactly D-013's "one silent seat costs exactly one hand".
        assert!(
            present.participants().contains(&2),
            "a readmitted seat that speaks joins P(k): {:?}",
            present.participants()
        );

        // **And a seat that stays away does not.** Nothing was delivered to
        // `absent` from seat 2, and `P(k)` does not hold it — so the
        // alternative rule would NOT make an absent seat required again, and
        // the oscillation the monotone rule was adopted against needs a seat
        // that is heard from to occur at all.
        assert!(
            !absent.participants().contains(&2),
            "an absent seat is not in P(k) however long it stays away: {:?}",
            absent.participants()
        );

        // The two differ, and that difference is the whole of the answer: `P(k)`
        // separates "came back" from "still gone", which is what the monotone
        // rule gives up in order to be safe.
        assert_ne!(present.participants(), absent.participants());
    }

    /// alone.
    #[test]
    fn a_readmitted_seat_is_accepted_at_stage_zero_and_never_required() {
        let mut o = opening3(0);
        // Seat 2 missed the last hand: it is outside `P(k)` and so outside the
        // required set, and the readmission set is what lets it be heard.
        o.required = vec![0, 1];
        o.readmitted = vec![2];
        let (mut h, _) = Hand::open(o, &key(10), NOW, 30_000).unwrap();

        // A stage that needs seats 0 and 1 is complete on those two alone, and
        // seat 2 arriving does not change that — it was never counted towards
        // completion.
        let mut with_two = opening3(2);
        with_two.required = vec![0, 1];
        with_two.readmitted = vec![2];
        let (_, from_two) = Hand::open(with_two, &key(12), NOW, 30_000).unwrap();
        let Send::Broadcast(bytes) = &from_two[0];
        let accepted = h.on_event(bytes, &key(10), NOW);
        assert!(
            accepted.is_ok(),
            "a readmitted seat's HAND_INIT is admitted: {accepted:?}"
        );
        assert!(
            h.participants().contains(&2),
            "and it is counted into P(k+1), which is what makes it dealable at k+2"
        );
    }

    /// `S1-BP`: a disagreeing settlement that arrives after this client gave
    /// up is heard, not refused — the sibling of `S1-BD`.
    ///
    /// Seat 1 publishes its own settlement, then its deadline expires before
    /// seat 0's copy lands (§4.10's race, exactly). Seat 0's copy differs in
    /// one field. Before: `on_late_settlement` returned `DeckDisagrees` before
    /// `stage.hear`, the late stage never closed, and `next_hand` fell through
    /// to the abort terminal while seat 0 kept the settlement's — a fork. Now
    /// the disagreement is reported, the body is heard, the stage closes on
    /// seat 1's OWN stacks, and `GENESIS(k+1)` is built from the settlement.
    #[test]
    fn a_disagreeing_late_settlement_is_heard_and_the_settlement_wins_over_the_abort() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        let mut queue: Vec<(SeatIdx, Vec<Send>)> = Vec::new();
        queue.push((1, deliver(&mut b, &a_deck, &key(11))));
        queue.push((0, deliver(&mut a, &b_deck, &key(10))));

        let keys = [key(10), key(11)];
        let late = NOW + 10 * 60 * 1000;
        let is_settlement = |bytes: &[u8], at: &Slot| {
            chained::open(bytes, FRAME_CAP, EventType::HandComplete, at).is_ok()
        };
        // Seat 0's settlement, held back and re-sealed with a changed field.
        let mut held: Option<Vec<u8>> = None;
        let mut b_settled = false;
        let mut done = false;

        for _ in 0..256 {
            if held.is_some() && b_settled && !done {
                done = true;
                // Seat 1's own deadline goes first, with its settlement
                // already published: `give_up` carries the stage across.
                let _abort = b.abort_now(Abort::Deadline, &keys[1], late).unwrap();
                assert!(b.aborted().is_some());
                let abort_next = b.next_hand().expect("an abort has a successor");
                // And then seat 0's differing copy arrives.
                let bytes = held.take().unwrap();
                assert_eq!(
                    b.on_event(&bytes, &keys[1], late),
                    Ok(Vec::new()),
                    "a disagreeing late settlement must be heard, not refused"
                );
                let note = b.take_settle_note().expect("the disagreement is reported");
                assert!(note.starts_with("late settlement disagreement with seat 0"), "{note}");
                let settled_next = b.next_hand().expect("a settled hand has a successor");
                assert_ne!(
                    settled_next.genesis, abort_next.genesis,
                    "the late stage never closed, so the abort terminal still stands"
                );
                continue;
            }
            if let Some((from, sends)) = queue.pop() {
                if sends.is_empty() {
                    continue;
                }
                let to = 1 - from;
                let sends = if from == 0 && held.is_none() {
                    let at = a.slot();
                    let mut out = Vec::new();
                    for s in &sends {
                        let Send::Broadcast(bytes) = s;
                        if is_settlement(bytes, &at) && held.is_none() {
                            held = Some(tamper::<HandComplete>(
                                bytes,
                                EventType::HandComplete,
                                &at,
                                HAND_COMPLETE_CAP,
                                |c| c.state_hash[0] ^= 1,
                            ));
                        } else {
                            out.push(s.clone());
                        }
                    }
                    out
                } else {
                    sends
                };
                if sends.is_empty() {
                    continue;
                }
                let hand: &mut Hand = if to == 0 { &mut a } else { &mut b };
                let out = deliver(hand, &sends, &keys[usize::from(to)]);
                if to == 1 && !b_settled {
                    let at = b.slot();
                    b_settled = out.iter().any(|s| {
                        let Send::Broadcast(bytes) = s;
                        is_settlement(bytes, &at)
                    });
                }
                queue.push((to, out));
                continue;
            }
            if done {
                break;
            }
            let Some(turn) = a.turn().or_else(|| b.turn()) else {
                break;
            };
            let seat = turn.seat;
            let hand: &mut Hand = if seat == 0 { &mut a } else { &mut b };
            let action = if hand.turn().expect("that hand agrees").legal.can_check {
                Action::Check
            } else {
                Action::Call
            };
            let out = hand.act(action, &keys[usize::from(seat)], NOW).unwrap();
            if seat == 1 && !b_settled {
                let at = b.slot();
                b_settled = out.iter().any(|s| {
                    let Send::Broadcast(bytes) = s;
                    is_settlement(bytes, &at)
                });
            }
            queue.push((seat, out));
        }

        assert!(done, "the race was never staged: seat 1 never settled before seat 0's copy was held");
        // Seat 0 settled normally on seat 1's genuine copy, and the two agree
        // on every chip: only the state hash was changed.
        let next_a = a.next_hand().expect("seat 0 settled");
        let next_b = b.next_hand().expect("seat 1 took the settlement");
        assert_eq!(
            next_a.seats.iter().map(|(_, _, c)| *c).collect::<Vec<_>>(),
            next_b.seats.iter().map(|(_, _, c)| *c).collect::<Vec<_>>(),
            "the settlement's stacks on both, not the abort's on one"
        );
    }

    /// `S1-BS`'s detector: a `HAND_INIT` of this hand at a genesis this client
    /// does not hold is recorded per seat and said once when two seats name one
    /// value — and it is still held, still not counted, and held only once.
    #[test]
    fn a_hand_init_at_a_foreign_genesis_is_recorded_and_not_counted() {
        let (mut a, _) = Hand::open(opening3(0), &key(10), NOW, 30_000).unwrap();
        let mut elsewhere_b = opening3(1);
        elsewhere_b.genesis = [9; 32];
        let mut elsewhere_c = opening3(2);
        elsewhere_c.genesis = [9; 32];
        let (_b, from_b) = Hand::open(elsewhere_b, &key(11), NOW, 30_000).unwrap();
        let (_c, from_c) = Hand::open(elsewhere_c, &key(12), NOW, 30_000).unwrap();
        let Send::Broadcast(init_b) = &from_b[0];
        let Send::Broadcast(init_c) = &from_c[0];

        // Seat 1's copy: a different parent, so held — and recorded.
        assert_eq!(a.on_event(init_b, &key(10), NOW), Err(Failed::NotYet));
        assert!(matches!(a.hold(init_b.clone()), Holding::Kept));
        assert!(a.foreign_genesis_named().is_none(), "one seat's word is one seat's word");
        assert!(a.take_genesis_note().is_none());
        // Seat 2's copy reaches the floor.
        assert_eq!(a.on_event(init_c, &key(10), NOW), Err(Failed::NotYet));
        assert!(matches!(a.hold(init_c.clone()), Holding::Kept));
        assert_eq!(a.foreign_genesis_named(), Some(([9; 32], vec![1, 2])));
        let note = a.take_genesis_note().expect("said once");
        assert!(note.contains("seat(s) [1, 2] opened it at genesis 09090909"), "{note}");
        assert!(a.take_genesis_note().is_none(), "and only once");
        // Nothing was counted: seat 0 is still waiting for everybody.
        assert_eq!(a.waiting_for(), vec![1, 2]);
        // Held once each, however often the re-send repeats them.
        assert_eq!(a.held(), 2);
        assert!(matches!(a.hold(init_b.clone()), Holding::Kept));
        assert_eq!(a.held(), 2, "the same bytes twice are one held event");

        // And a copy at this client's own genesis records nothing.
        let (mut a2, _) = Hand::open(opening3(0), &key(10), NOW, 30_000).unwrap();
        let (_b2, from_b2) = Hand::open(opening3(1), &key(11), NOW, 30_000).unwrap();
        let Send::Broadcast(init_b2) = &from_b2[0];
        assert!(a2.on_event(init_b2, &key(10), NOW).is_ok());
        assert!(a2.foreign_genesis_named().is_none());
        assert_eq!(a2.waiting_for(), vec![2]);
    }

    /// `S1-BS`: a certificate that reaches this client after its hand ended by
    /// an abort still banks its roster half, says so, and narrows the next
    /// hand — and a second copy banks nothing twice.
    #[test]
    fn a_certificate_about_an_aborted_hand_still_banks() {
        let (mut a, _b, _c, keys, certs, _to_a, late) = one_action_behind_with_a_certificate();
        // The hand ends first, on this client's own deadline.
        let _ = a.abort_now(Abort::Deadline, &keys[0], late).unwrap();
        assert!(a.aborted().is_some());
        for cert in &certs {
            let r = a.on_event(cert, &keys[0], late);
            assert_eq!(r, Ok(Vec::new()), "a late copy banks and stops: {r:?}");
        }
        assert!(a.take_fork().is_none(), "no fork is reported about a stage a finished hand never reaches");
        assert!(a.take_late_roster(), "the first copy banked after the terminal");
        assert!(!a.take_late_roster(), "and the second did not bank it twice");
        let note = a.take_cert_note().expect("the late bank is said");
        assert!(note.contains("banked after the terminal"), "{note}");
        let next = a.next_hand().expect("an abort has a successor");
        assert_eq!(next.required, vec![1, 2], "the certified seat left R(k+1)");
    }

    /// `S1-BS`, the whole shape at hand level, converging at k+2 without a
    /// second signature.
    ///
    /// Five seats. Seat 4 goes quiet at the deck stage; seats 0 to 3 vote.
    /// Seat 0 holds every vote, seals, and banks `certified [4]`; seat 1
    /// holds two votes and seals nothing. Seat 4's own unattributed abort ends
    /// the hand on both: same terminal, and R(2) differs by seat 4 — seat 1
    /// opens hand 2 at a genesis nobody else holds and signs it. Then seat 0's
    /// certificate copy reaches seat 1 during hand 2: hand 1, retained, banks
    /// it and re-derives hand 2 at seat 0's genesis; seat 1 re-opens hand 2
    /// there **muted**, inherits the held copies, and never signs hand 2
    /// twice. Hand 2 dies on both — it cannot complete without seat 1's
    /// signature — and hand 3 opens at one genesis on both.
    #[test]
    fn a_late_certificate_re_derives_the_next_hand_and_the_table_converges_at_the_hand_after() {
        let keys: Vec<SigningKey> = (10..15).map(key).collect();
        let mut hands: Vec<Hand> = Vec::new();
        let mut inits: Vec<Vec<u8>> = Vec::new();
        for seat in 0..5u8 {
            let (h, sends) = Hand::open(opening5(seat), &keys[usize::from(seat)], NOW, 30_000).unwrap();
            let Send::Broadcast(b) = &sends[0];
            inits.push(b.clone());
            hands.push(h);
        }
        // Stage 0 completes everywhere; every seat emits its DECK_INIT.
        let mut decks: Vec<Vec<Send>> = vec![Vec::new(); 5];
        for to in 0..5usize {
            for from in 0..5usize {
                if from == to {
                    continue;
                }
                let mut out = deliver(&mut hands[to], &[Send::Broadcast(inits[from].clone())], &keys[to]);
                decks[to].append(&mut out);
            }
            assert!(!decks[to].is_empty(), "seat {to} emits its deck contribution");
        }
        // Seats 0..3 hear each other's decks; seat 4's never arrives.
        for to in 0..4usize {
            for from in 0..4usize {
                if from != to {
                    let _ = deliver(&mut hands[to], &decks[from], &keys[to]);
                }
            }
            assert_eq!(hands[to].waiting_for(), vec![4], "seat {to} waits for seat 4");
        }
        // Votes at the stage budget. Seat 0 hears every vote; seat 1 hears seat 0's only.
        let t1 = NOW + 30_000;
        let mut votes: Vec<Vec<u8>> = Vec::new();
        for s in 0..4usize {
            let v = hands[s].vote_on_timeouts(&keys[s], t1, 0).unwrap();
            let Send::Broadcast(b) = &v[0];
            votes.push(b.clone());
        }
        let table_id = hands[0].slot().table_id;
        let mut cert_copy: Option<Vec<u8>> = None;
        for s in 1..4usize {
            let out = hands[0].on_event(&votes[s], &keys[0], t1).unwrap();
            for o in out {
                let Send::Broadcast(b) = o;
                if chained::open_in_hand(&b, FRAME_CAP, EventType::TimeoutCert, &table_id, 1).is_ok() {
                    cert_copy = Some(b);
                }
            }
        }
        let cert_copy = cert_copy.expect("seat 0 sealed its copy on the fourth vote");
        assert!(hands[1].on_event(&votes[0], &keys[1], t1).unwrap().is_empty(), "two of four votes seal nothing");
        // Seat 4's own unattributed abort ends the hand on both, at twice the budget.
        let t2 = NOW + 60_000;
        let ab = hands[4].abort_now(Abort::Deadline, &keys[4], t2).unwrap();
        let Send::Broadcast(abort) = &ab[0];
        assert!(hands[0].on_event(abort, &keys[0], t2).is_ok());
        assert!(hands[1].on_event(abort, &keys[1], t2).is_ok());
        let oa = hands[0].next_hand().expect("seat 0 has a successor");
        let ob = hands[1].next_hand().expect("seat 1 has a successor");
        assert_eq!(oa.required, vec![0, 1, 2, 3], "seat 0 banked the certificate when it sealed");
        assert_eq!(ob.required, vec![0, 1, 2, 3, 4], "seat 1 never had a copy");
        assert_ne!(oa.genesis, ob.genesis, "the fork S1-BS measured");

        // Hand 2: both speak at their own genesis; seat 0's copy is foreign at seat 1.
        let (mut a2, from_a2) = Hand::open(oa.clone(), &keys[0], NOW, 30_000).unwrap();
        let (mut b2, _from_b2) = Hand::open(ob, &keys[1], NOW, 30_000).unwrap();
        assert!(b2.spoke(), "seat 1 signed hand 2 at its own genesis");
        let Send::Broadcast(init_a2) = &from_a2[0];
        assert_eq!(b2.on_event(init_a2, &keys[1], NOW), Err(Failed::NotYet));
        assert!(matches!(b2.hold(init_a2.clone()), Holding::Kept));

        // The late copy: hand 2 cannot keep it, hand 1 banks it and re-derives.
        assert_eq!(b2.on_event(&cert_copy, &keys[1], NOW), Err(Failed::NotYet));
        assert!(matches!(b2.hold(cert_copy.clone()), Holding::AnotherHand { hand_id: 1, seat: Some(0) }));
        assert_eq!(hands[1].on_event(&cert_copy, &keys[1], t2), Ok(Vec::new()));
        assert!(hands[1].take_late_roster());
        let mut ob2 = hands[1].next_hand().expect("re-derived");
        assert_eq!(ob2.required, vec![0, 1, 2, 3]);
        assert_eq!(ob2.genesis, oa.genesis, "the table's genesis, from the certificate alone");

        // Re-open muted: no second signature, the held copies carried over.
        ob2.readmitted = b2.readmitted().to_vec();
        let early = b2.take_early();
        let (mut b2m, sends) = Hand::open_with(ob2, &keys[1], NOW, 30_000, Voice::Muted).unwrap();
        assert!(sends.is_empty(), "a muted hand sends nothing");
        assert!(!b2m.spoke() && b2m.speak().is_none());
        for e in early {
            let _ = b2m.hold(e);
        }
        let (more, failures) = b2m.replay_early(&keys[1], NOW);
        assert!(failures.is_empty(), "{failures:?}");
        assert!(more.is_empty(), "nothing goes out from a muted hand's replay either");
        assert_eq!(b2m.genesis(), a2.genesis());
        assert_eq!(b2m.waiting_for(), vec![2, 3], "seat 0's copy counted at the corrected genesis");

        // Hand 2 dies on both; hand 3 opens at one genesis on both.
        let _ = a2.abort_now(Abort::Deadline, &keys[0], t2).unwrap();
        let _ = b2m.abort_now(Abort::Deadline, &keys[1], t2).unwrap();
        let na = a2.next_hand().expect("hand 3 on seat 0");
        let nb = b2m.next_hand().expect("hand 3 on seat 1");
        assert_eq!(na.required, vec![0, 1, 2, 3]);
        assert_eq!(na.genesis, nb.genesis, "converged at k+2 without a second signature");
    }

    /// A quiet hand hears itself, counts the table's copies, and speaks once
    /// when asked; a muted one never does.
    #[test]
    fn a_quiet_hand_speaks_once_and_a_muted_one_never() {
        let (mut a, sends) = Hand::open_with(opening3(0), &key(10), NOW, 30_000, Voice::Quiet).unwrap();
        assert!(sends.is_empty() && !a.spoke() && a.voice() == Voice::Quiet);
        let (mut b, from_b) = Hand::open(opening3(1), &key(11), NOW, 30_000).unwrap();
        let (_c, from_c) = Hand::open(opening3(2), &key(12), NOW, 30_000).unwrap();
        let _ = deliver(&mut a, &from_b, &key(10));
        let _ = deliver(&mut a, &from_c, &key(10));
        assert_eq!(a.counted_at_stage_zero(), vec![1, 2]);
        assert!(a.dealt(), "its own copy was heard, so stage 0 completed here");
        let spoken = a.speak().expect("the withheld HAND_INIT");
        assert!(a.spoke() && a.speak().is_none(), "once");
        let _ = deliver(&mut b, &[spoken], &key(11));
        assert_eq!(b.waiting_for(), vec![2]);
        let (mut m, sends) = Hand::open_with(opening3(0), &key(10), NOW, 30_000, Voice::Muted).unwrap();
        assert!(sends.is_empty() && m.speak().is_none() && !m.spoke());
    }

    /// Three seats at the deck stage, seat 2 silent: seats 0 and 1 wait for
    /// it, vote at the stage budget, and seat 2 gives the hand up unattributed.
    fn two_voters_and_a_silent_seat() -> (Hand, Hand, [SigningKey; 3], Vec<u8>, Vec<u8>, Vec<u8>) {
        let (mut a, from_a) = Hand::open(opening3(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening3(1), &key(11), NOW, 30_000).unwrap();
        let (mut c, from_c) = Hand::open(opening3(2), &key(12), NOW, 30_000).unwrap();
        let keys = [key(10), key(11), key(12)];
        let _ = deliver(&mut b, &from_a, &keys[1]);
        let _ = deliver(&mut c, &from_a, &keys[2]);
        let _ = deliver(&mut a, &from_b, &keys[0]);
        let _ = deliver(&mut c, &from_b, &keys[2]);
        let deck_a = deliver(&mut a, &from_c, &keys[0]);
        let deck_b = deliver(&mut b, &from_c, &keys[1]);
        let _ = deliver(&mut b, &deck_a, &keys[1]);
        let _ = deliver(&mut a, &deck_b, &keys[0]);
        assert_eq!(a.waiting_for(), vec![2], "a cryptographic stage waiting on seat 2");
        let t1 = NOW + 30_000;
        let va = a.vote_on_timeouts(&keys[0], t1, 0).unwrap();
        let vb = b.vote_on_timeouts(&keys[1], t1, 0).unwrap();
        assert!(!va.is_empty() && !vb.is_empty(), "both voters vote at the budget");
        let Send::Broadcast(va) = &va[0];
        let Send::Broadcast(vb) = &vb[0];
        let sends = c.abort_now(Abort::Deadline, &keys[2], t1).unwrap();
        let Send::Broadcast(abort) = &sends[0];
        (a, b, keys, va.clone(), vb.clone(), abort.clone())
    }

    /// `S1-BS` option 1: an unattributed abort arriving between one and two
    /// stage budgets of a cryptographic stage, while this client has voted,
    /// is held; the certificate that completes meanwhile banks the roster and
    /// ends the hand with its subject named; the replayed abort is neither
    /// applied over it nor refused. With the old gate the first assertion
    /// fails: `past_deadline` is true at 31 s of a deck stage.
    #[test]
    fn an_unattributed_abort_waits_for_a_vote_this_client_joined() {
        let (mut a, mut b, keys, va, vb, abort) = two_voters_and_a_silent_seat();
        let t2 = NOW + 31_000;
        assert_eq!(a.on_event(&abort, &keys[0], t2), Err(Failed::NotYet), "held: a vote is open");
        assert!(a.aborted().is_none(), "the hand did not end on a peer's word");
        assert!(matches!(a.hold(abort.clone()), Holding::Kept));
        let note = a.take_cert_note().expect("the hold is said");
        assert!(note.contains("abort from seat 2 held"), "{note}");
        // The vote completes: each voter seals on the other's vote and hears the other's copy.
        let ca = a.on_event(&vb, &keys[0], t2).unwrap();
        let cb = b.on_event(&va, &keys[1], t2).unwrap();
        assert!(!ca.is_empty() && !cb.is_empty(), "each voter seals a copy");
        let Send::Broadcast(ca) = &ca[0];
        let Send::Broadcast(cb) = &cb[0];
        assert!(a.on_event(cb, &keys[0], t2).is_ok());
        assert!(b.on_event(ca, &keys[1], t2).is_ok());
        assert_eq!(a.aborted(), Some(Abort::Told { cause: 1 }), "ended by the certificate, subject named");
        assert!(!a.took_part(2), "the roster half is banked");
        // The held abort is replayed as the node replays it: nothing undone, nothing refused.
        let (_out, failures) = a.replay_early(&keys[0], t2);
        assert!(failures.is_empty(), "the replayed abort must not become a refusal: {failures:?}");
        assert_eq!(a.aborted(), Some(Abort::Told { cause: 1 }));
        let na = a.next_hand().expect("an aborted hand has a successor");
        let nb = b.next_hand().expect("on both");
        assert!(!na.required.contains(&2), "{:?}", na.required);
        assert_eq!(na.genesis, nb.genesis, "one GENESIS(k+1)");
    }

    /// The bound: with no certificate completing, the held abort is admitted
    /// at twice the stage budget regardless — `S1-AQ`'s permanent `NotYet`
    /// cannot come back through this gate.
    #[test]
    fn a_held_abort_is_admitted_at_twice_the_budget_without_a_certificate() {
        let (mut a, _b, keys, _va, _vb, abort) = two_voters_and_a_silent_seat();
        assert_eq!(a.on_event(&abort, &keys[0], NOW + 31_000), Err(Failed::NotYet));
        let t3 = NOW + 60_000;
        assert!(a.on_event(&abort, &keys[0], t3).is_ok(), "admitted at twice the budget");
        assert_eq!(a.aborted(), Some(Abort::Told { cause: 1 }));
    }

    /// `S1-BQ`'s fixture. Three seats; seat 0 is one action behind — the last
    /// action before its own turn has not reached it — and seats 1 and 2
    /// time seat 0 out, vote, seal a `TIMEOUT_CERT` at the sequence of seat
    /// 0's turn, and close their own certificate stage on each other's copy.
    ///
    /// Returns the three hands, the keys, every certificate copy, and the
    /// action seat 0 has not heard yet.
    fn one_action_behind_with_a_certificate() -> (
        Hand,
        Hand,
        Hand,
        [SigningKey; 3],
        Vec<Vec<u8>>,
        Vec<Send>,
        u64,
    ) {
        let (mut a, from_a) = Hand::open(opening3(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening3(1), &key(11), NOW, 30_000).unwrap();
        let (mut c, from_c) = Hand::open(opening3(2), &key(12), NOW, 30_000).unwrap();
        let keys = [key(10), key(11), key(12)];
        let mut queue: Vec<(SeatIdx, Vec<Send>)> = Vec::new();
        queue.push((1, deliver(&mut b, &from_a, &keys[1])));
        queue.push((2, deliver(&mut c, &from_a, &keys[2])));
        queue.push((0, deliver(&mut a, &from_b, &keys[0])));
        queue.push((2, deliver(&mut c, &from_b, &keys[2])));
        queue.push((0, deliver(&mut a, &from_c, &keys[0])));
        queue.push((1, deliver(&mut b, &from_c, &keys[1])));

        // Once the cards are out, seat 0 hears every action one step late.
        let mut lagging = false;
        let mut to_a: Vec<Send> = Vec::new();
        let mut staged = false;

        for _ in 0..512 {
            if !queue.is_empty() {
                let (from, sends) = queue.remove(0);
                if sends.is_empty() {
                    continue;
                }
                for to in 0..3u8 {
                    if to == from {
                        continue;
                    }
                    if to == 0 && lagging {
                        let earlier = std::mem::take(&mut to_a);
                        if !earlier.is_empty() {
                            let out = deliver(&mut a, &earlier, &keys[0]);
                            queue.push((0, out));
                        }
                        to_a.extend(sends.iter().cloned());
                        continue;
                    }
                    let hand: &mut Hand = match to {
                        0 => &mut a,
                        1 => &mut b,
                        _ => &mut c,
                    };
                    let out = deliver(hand, &sends, &keys[usize::from(to)]);
                    queue.push((to, out));
                }
                continue;
            }
            if !lagging && a.dealt() && b.dealt() && c.dealt() {
                lagging = true;
            }
            let Some(turn) = b.turn() else {
                break;
            };
            if turn.seat == 0 && lagging && !to_a.is_empty() {
                staged = true;
                break;
            }
            let seat = turn.seat;
            let hand: &mut Hand = match seat {
                0 => &mut a,
                1 => &mut b,
                _ => &mut c,
            };
            let action = if hand.turn().expect("that hand agrees").legal.can_check {
                Action::Check
            } else {
                Action::Call
            };
            let out = hand.act(action, &keys[usize::from(seat)], NOW).unwrap();
            queue.push((seat, out));
        }
        assert!(staged, "the fixture never reached seat 0's turn with seat 0 one action behind");
        assert_eq!(
            a.slot().sequence + 1,
            b.slot().sequence,
            "seat 0 must be exactly one sequence behind the voters"
        );

        // Seats 1 and 2 time seat 0 out and seal the certificate between them.
        let late = NOW + 600_000;
        let vb = b.vote_on_timeouts(&keys[1], late, 0).unwrap();
        let vc = c.vote_on_timeouts(&keys[2], late, 0).unwrap();
        assert!(!vb.is_empty() && !vc.is_empty(), "both voters must vote");
        let mut certs: Vec<Vec<u8>> = Vec::new();
        let mut from_b = deliver(&mut b, &vc, &keys[1]);
        let mut from_c = deliver(&mut c, &vb, &keys[2]);
        from_b.append(&mut from_c);
        for s in &from_b {
            let Send::Broadcast(bytes) = s;
            if chained::open_in_hand(bytes, FRAME_CAP, EventType::TimeoutCert, &[1; 32], 1).is_ok() {
                certs.push(bytes.clone());
            }
        }
        assert!(!certs.is_empty(), "no certificate was sealed, so nothing is tested");
        // Each voter hears the other's copy, so their certificate stage closes
        // the way it does at a table: on every voter's copy, not on its own.
        for cert in &certs {
            let rb = b.on_event(cert, &keys[1], late);
            assert!(rb.is_ok(), "seat 1 hears a voter's copy: {rb:?}");
            let rc = c.on_event(cert, &keys[2], late);
            assert!(rc.is_ok(), "seat 2 hears a voter's copy: {rc:?}");
        }
        assert_eq!(b.slot(), c.slot(), "the voters stand together after the certificate");
        (a, b, c, keys, certs, to_a, late)
    }

    /// `S1-BQ`: a certificate about a stage this client has not reached yet is
    /// held, and applied when it gets there — not consumed with a fork report.
    ///
    /// Before: banked, reported as *this hand has forked*, and returned `Ok`
    /// — every copy, never replayed — so on catching up the client had no
    /// certificate stage. Now: `NotYet`, held the way `run.rs` holds it, and
    /// once the missing action lands, `replay_early` re-delivers the copies,
    /// seat 0 applies the certificate and stands where the voters stand. The
    /// roster half is banked at once: seat 0 is out of the voter sets while
    /// the positional half still waits.
    #[test]
    fn a_certificate_for_a_stage_not_yet_reached_is_held_and_then_applied() {
        let (mut a, b, _c, keys, certs, to_a, late) = one_action_behind_with_a_certificate();

        // The certificate reaches seat 0 one sequence early: held, not forked.
        for cert in &certs {
            assert_eq!(
                a.on_event(cert, &keys[0], late),
                Err(Failed::NotYet),
                "a certificate for a stage not yet reached must be held"
            );
            assert!(
                matches!(a.hold(cert.clone()), Holding::Kept),
                "and the node keeps it for replay"
            );
        }
        assert!(a.take_fork().is_none(), "and it is not a fork: nothing else was chained there");
        assert!(
            !a.voters(1).contains(&0),
            "the roster half is banked at once: the certified seat has left the voter sets"
        );
        let note = a.take_cert_note().expect("the hold is said");
        assert!(note.contains("HELD until it is in position"), "{note}");

        // The missing action lands, seat 0 is in position, and the replay
        // pass — the one `run.rs` runs after every accepted event — delivers
        // the held copies.
        let _ = deliver(&mut a, &to_a, &keys[0]);
        let (_more, failures) = a.replay_early(&keys[0], late);
        assert!(failures.is_empty(), "replayed copies were refused: {failures:?}");
        assert_eq!(
            a.slot(),
            b.slot(),
            "seat 0 applied the certificate and stands where the voters stand"
        );
        assert!(a.take_fork().is_none(), "no fork was ever declared");
    }

    /// `S1-BQ`'s refuted first shape, kept as a test. Holding BEFORE banking
    /// lost the roster half for a client whose hand ends before it reaches the
    /// sequence — every one of the five measured cases — and such a client
    /// then kept the certified seat in R(k+1) and opened hand k+1 at a genesis
    /// the table did not hold. The roster half is banked first, so a client
    /// that never replays the certificate still derives the table's roster.
    #[test]
    fn a_certificate_held_behind_still_narrows_the_next_hand() {
        let (mut a, _b, _c, keys, certs, _to_a, late) = one_action_behind_with_a_certificate();
        for cert in &certs {
            assert_eq!(a.on_event(cert, &keys[0], late), Err(Failed::NotYet));
        }
        // Seat 0 never gets there: its own deadline ends the hand first.
        let _ = a.abort_now(Abort::Deadline, &keys[0], late).unwrap();
        let next = a.next_hand().expect("an abort has a successor");
        assert!(
            !next.required.contains(&0),
            "the certified seat must be outside R(k+1) on the client that held the certificate, \
             or that client opens hand k+1 at a genesis nobody else holds: {:?}",
            next.required
        );
    }

    /// Both peers reach the same boundary checkpoint, and it sits where §4.9
    /// puts it.
    ///
    /// **This is the property everything downstream of the checkpoint rests
    /// on.** §6.1 calls the state hash *"the only way a silent divergence is
    /// ever caught"*, and a comparison is worth nothing unless two honest peers
    /// that played the same hand produce the same value. So the value is
    /// compared, and so is the slot: `sequence = BOUNDARY_CHECKPOINT_BASE` with
    /// A settlement this client disagrees with is **heard**, so the checkpoint
    /// that exists to judge it is reached.
    ///
    /// `S1-BD`. `on_hand_complete` used to return before `stage.hear`, and
    /// everything followed from that: the stage never completed, so
    /// `close_settlement_if_done` never ran, so `checkpoint8` stayed `None`, so
    /// no checkpoint-8 `STATE_HASH` was published — and the two differing
    /// values never met at §6.3's comparison. Measured, one 900-second run: 124
    /// settlement disagreements and **zero** freeze lines. The hand ended by
    /// abort instead and the pot was restored rather than awarded, which is the
    /// outcome `D-026` forbids.
    ///
    /// Hearing it moves no chips: `close_settlement_if_done` applies this
    /// client's OWN `final_stacks` and publishes its OWN `state_hash`.
    #[test]
    fn a_disagreeing_settlement_is_heard_so_the_checkpoint_is_reached() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        let mut queue: Vec<(SeatIdx, Vec<Send>)> = Vec::new();
        queue.push((1, deliver(&mut b, &a_deck, &key(11))));
        queue.push((0, deliver(&mut a, &b_deck, &key(10))));

        let keys = [key(10), key(11)];
        let mut swapped = false;

        for _ in 0..256 {
            if let Some((from, sends)) = queue.pop() {
                if sends.is_empty() {
                    continue;
                }
                let to = 1 - from;

                // The one interception: seat 0's settlement, re-sealed with a
                // changed field, so seat 1 hears a body it disagrees with.
                let sends = if from == 0 && !swapped {
                    let at = a.slot();
                    let mut out = Vec::new();
                    for s in &sends {
                        let Send::Broadcast(bytes) = s;
                        let mine: Result<_, _> =
                            chained::open(bytes, FRAME_CAP, EventType::HandComplete, &at);
                        if mine.is_ok() && !swapped {
                            swapped = true;
                            out.push(Send::Broadcast(tamper::<HandComplete>(
                                bytes,
                                EventType::HandComplete,
                                &at,
                                HAND_COMPLETE_CAP,
                                |c| c.state_hash[0] ^= 1,
                            )));
                        } else {
                            out.push(s.clone());
                        }
                    }
                    out
                } else {
                    sends
                };

                let hand: &mut Hand = if to == 0 { &mut a } else { &mut b };
                let out = deliver(hand, &sends, &keys[usize::from(to)]);
                queue.push((to, out));
                continue;
            }
            if a.over() && b.over() {
                break;
            }
            let Some(turn) = a.turn().or_else(|| b.turn()) else {
                break;
            };
            let seat = turn.seat;
            let hand: &mut Hand = if seat == 0 { &mut a } else { &mut b };
            let action = if hand.turn().expect("that hand agrees").legal.can_check {
                Action::Check
            } else {
                Action::Call
            };
            let out = hand.act(action, &keys[usize::from(seat)], NOW).unwrap();
            queue.push((seat, out));
        }

        assert!(swapped, "no settlement was ever intercepted, so nothing was tested");
        assert!(
            b.take_settle_note().is_some(),
            "the disagreement was not reported, and the report is what the refusal was really for"
        );
        assert!(
            b.checkpoint8().is_some(),
            "the settlement stage never closed, so §6.3's checkpoint is still unreachable"
        );
    }

    /// `previous_event_hash = TERMINAL(k)`, which is §4.9's rule and not the
    /// next stage after the hand.
    ///
    /// The stage does not run yet — nothing collects these or completes it.
    /// What is pinned here is the value and its address, which is what the
    /// stage will carry.
    #[test]
    fn both_peers_reach_the_same_boundary_checkpoint() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        let mut queue: Vec<(SeatIdx, Vec<Send>)> = Vec::new();
        queue.push((1, deliver(&mut b, &a_deck, &key(11))));
        queue.push((0, deliver(&mut a, &b_deck, &key(10))));

        let keys = [key(10), key(11)];
        for _ in 0..256 {
            if let Some((from, sends)) = queue.pop() {
                if sends.is_empty() {
                    continue;
                }
                let to = 1 - from;
                let hand: &mut Hand = if to == 0 { &mut a } else { &mut b };
                let out = deliver(hand, &sends, &keys[usize::from(to)]);
                queue.push((to, out));
                continue;
            }
            if a.over() && b.over() {
                break;
            }
            let Some(turn) = a.turn().or_else(|| b.turn()) else {
                break;
            };
            let seat = turn.seat;
            let hand: &mut Hand = if seat == 0 { &mut a } else { &mut b };
            let action = if hand.turn().expect("that hand agrees").legal.can_check {
                Action::Check
            } else {
                Action::Call
            };
            let out = hand.act(action, &keys[usize::from(seat)], NOW).unwrap();
            queue.push((seat, out));
        }

        let (a_state, a_terminal) = a.checkpoint8().expect("a settled hand has a checkpoint");
        let (b_state, b_terminal) = b.checkpoint8().expect("and so does the other peer");
        assert_eq!(
            a_state, b_state,
            "two peers that played the same hand hash the same state, or the              checkpoint compares nothing"
        );
        assert_eq!(
            a_terminal, b_terminal,
            "and chain it from the same TERMINAL(k)"
        );

        // `P(k)`: both peers heard both seats.
        assert_eq!(a.participants(), vec![0, 1]);
        assert_eq!(b.participants(), vec![0, 1]);

        // The address, which is §4.9's and not the hand's own next stage.
        let bytes = a
            .state_hash_event(&key(10), NOW)
            .expect("the event seals")
            .expect("and there is one to seal");
        let (_, _, sequence) =
            crate::net::chained::peek(&bytes, STATE_HASH_CAP).expect("it is a chained event");
        assert_eq!(
            sequence,
            crate::protocol::constants::BOUNDARY_CHECKPOINT_BASE,
            "the checkpoint is named, not walked to"
        );
    }

    /// Hand two follows hand one, and both peers derive the same opening.
    ///
    /// This is the property the whole chain rests on at a hand boundary: two
    /// peers that saw the same hand must agree on `GENESIS(k+1)` down to the
    /// byte, or hand two forks with nobody lying. Every input is chained —
    /// the settled stacks, `TERMINAL(k)`, and `P(k)`.
    #[test]
    fn hand_two_follows_hand_one() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        let mut queue: Vec<(SeatIdx, Vec<Send>)> = Vec::new();
        queue.push((1, deliver(&mut b, &a_deck, &key(11))));
        queue.push((0, deliver(&mut a, &b_deck, &key(10))));
        let keys = [key(10), key(11)];
        for _ in 0..256 {
            if let Some((from, sends)) = queue.pop() {
                if sends.is_empty() {
                    continue;
                }
                let to = 1 - from;
                let hand: &mut Hand = if to == 0 { &mut a } else { &mut b };
                let out = deliver(hand, &sends, &keys[usize::from(to)]);
                queue.push((to, out));
                continue;
            }
            if a.betting_over() && b.betting_over() {
                break;
            }
            let Some(turn) = a.turn().or_else(|| b.turn()) else {
                break;
            };
            let seat = turn.seat;
            let hand: &mut Hand = if seat == 0 { &mut a } else { &mut b };
            let action = if hand.turn().unwrap().legal.can_check {
                Action::Check
            } else {
                Action::Call
            };
            let out = hand.act(action, &keys[usize::from(seat)], NOW).unwrap();
            queue.push((seat, out));
        }

        let next_a = a.next_hand().expect("hand two exists");
        let next_b = b.next_hand().expect("and both peers say so");

        assert_eq!(next_a.genesis, next_b.genesis, "one GENESIS(2), or a fork");
        assert_eq!(next_a.roster_hash, next_b.roster_hash);
        assert_eq!(next_a.required, next_b.required);
        assert_eq!(next_a.seats, next_b.seats);
        assert_eq!(next_a.button, next_b.button);
        assert_eq!(next_a.hand_id, 2);
        assert_eq!(
            next_a.session_id, next_b.session_id,
            "the session is the table's and does not change between hands"
        );
        assert_eq!(next_a.table_id, next_b.table_id);

        // The button moved. Heads-up it alternates, which is TDA 34-B and is
        // the one rotation a two-handed table has.
        assert_ne!(
            next_a.button,
            Some(a.init().button_position),
            "the button rotates"
        );

        // The stacks carried over, and they are what the settlement produced.
        assert_eq!(
            next_a.seats.iter().map(|(_, _, c)| *c).collect::<Vec<_>>(),
            a.stacks(),
            "hand two starts from hand one's chips"
        );
        assert_eq!(
            next_a.seats.iter().map(|(_, _, c)| c).sum::<u64>(),
            2 * 10_000,
            "and no chip was created between hands"
        );

        // And hand two opens.
        let (mut a2, from_a2) = Hand::open(next_a, &key(10), NOW, 30_000).unwrap();
        let (mut b2, from_b2) = Hand::open(next_b, &key(11), NOW, 30_000).unwrap();
        deliver(&mut b2, &from_a2, &key(11));
        deliver(&mut a2, &from_b2, &key(10));
        assert!(a2.dealt() && b2.dealt(), "stage 0 of hand two completed");
        assert_eq!(a2.slot(), b2.slot(), "off one parent");
    }

    /// A hand that stalls is given up on, and the next one still starts.
    ///
    /// The property that matters is not that the hand ends — it is that two
    /// peers which gave up at slightly different moments derive the **same**
    /// `GENESIS(k+1)`. `ABORT_TERMINAL(k)` is a function of `GENESIS(k)` and
    /// nothing else precisely so that the middle of the hand, which is what
    /// they could not agree about, cannot enter it.
    #[test]
    fn an_abandoned_hand_still_leads_to_the_next_one() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        // They get as far as stage 1 and then one of them goes quiet.
        deliver(&mut b, &from_a, &key(11));
        deliver(&mut a, &from_b, &key(10));
        assert!(a.next_hand().is_none(), "a hand in progress has no successor");

        // A's deadline passes first, and it says so.
        let late = NOW + 10 * 60 * 1000;
        let sends = a.abort_now(Abort::Deadline, &key(10), late).unwrap();
        assert_eq!(sends.len(), 1);
        assert!(a.aborted().is_some() && a.over());

        // B is not there yet: the message is **held**, not refused. Otherwise
        // one peer could end everybody's hand by claiming a deadline.
        let Send::Broadcast(bytes) = &sends[0];
        assert_eq!(
            b.on_event(bytes, &key(11), NOW),
            Err(Failed::NotYet),
            "an abort before this receiver's own deadline is buffered"
        );
        assert!(b.aborted().is_none(), "and it did not end B's hand");

        // Once B's own timer agrees, the same message lands.
        assert!(b.on_event(bytes, &key(11), late).unwrap().is_empty());
        assert!(b.aborted().is_some());

        // And both derive one successor.
        let next_a = a.next_hand().expect("hand two exists after an abort");
        let next_b = b.next_hand().expect("on both peers");
        assert_eq!(next_a.genesis, next_b.genesis, "one GENESIS(2), or a fork");
        assert_eq!(next_a.roster_hash, next_b.roster_hash);
        assert_eq!(next_a.hand_id, 2);

        // No chip moved. Every seat starts hand two with what it started hand
        // one with, which is D-010's accepted cost and I27's invariant.
        assert_eq!(
            next_a.seats.iter().map(|(_, _, c)| *c).collect::<Vec<_>>(),
            a.init().stacks,
            "an abort restores every stack"
        );
    }

    /// An abort that moves a chip is refused, whoever signed it.
    ///
    /// This is what makes D-010 a receiver's rule rather than a request to
    /// emitters: not that nobody writes a forfeiture, but that nobody can make
    /// one stick.
    #[test]
    fn an_abort_that_moves_a_chip_is_refused() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        deliver(&mut b, &from_a, &key(11));
        deliver(&mut a, &from_b, &key(10));
        let late = NOW + 10 * 60 * 1000;
        let sends = a.abort_now(Abort::Deadline, &key(10), late).unwrap();
        let Send::Broadcast(good) = &sends[0];

        let greedy = tamper::<HandAbort>(
            good,
            EventType::HandAbort,
            &b.slot(),
            HAND_ABORT_CAP,
            |x| {
                x.final_stacks[0] += 100;
                x.final_stacks[1] -= 100;
            },
        );
        let e = b.on_event(&greedy, &key(11), late).unwrap_err();
        assert!(matches!(e, Failed::Elsewhere { seat: 0, .. }), "{e}");
        assert!(b.aborted().is_none(), "and the hand did not end");
    }

    /// The reconnection bank: a seat that drops is held, and a seat that keeps
    /// dropping is not.
    ///
    /// Every quantity here is a fold over `P(k)`, which is agreed and already
    /// in the state hash, so two peers cannot disagree about how much anybody
    /// has left. That is the whole reason it is counted in hands: an allowance
    /// in seconds is an allowance measured on a clock nobody shares, and one
    /// **The seat that is seated and not dealt in must be able to follow, and
    /// today it cannot.** This is `S1-S`, and D-013's promise rests on it.
    ///
    /// §3.2's box: *"A seat outside `P(k)` keeps everything except its vote."*
    /// §4.4: *"A seat outside `dealt_in` keeps its stack, pays its blinds and
    /// antes as dead money, takes no cards, and is not a party to the
    /// cryptography."* It is still a **required emitter** of `HAND_INIT`, of
    /// `HAND_COMPLETE` and of every checkpoint — so it has to follow the whole
    /// hand it holds no cards in, and §4.6 makes that possible by construction:
    /// *"every peer has all `m` tokens for those indices and opens the cards."*
    ///
    /// And it has to, or §4.9's readmission set is unreachable by the only seat
    /// it exists for: `A` is written by *a checkpoint-8 `STATE_HASH` that
    /// agrees*, and a seat that cannot reach its own settlement has no such
    /// value to sign.
    ///
    /// **What this test pins, and what it costs to leave open:** seat 2 is on
    /// the roster with chips and is outside `dealt_in`. Seats 0 and 1 must
    /// finish the hand; seat 2 must accuse nobody; and seat 2 must reach its own
    /// boundary checkpoint. Today `begin_deck` seeds this client's own deck key
    /// unconditionally (`keys: vec![own]`, with no `dealt_in` test), so the
    /// observer's aggregate key is a sum over `|dealt_in| + 1` keys while every
    /// dealt-in seat's is a sum over `|dealt_in|`, and the first shuffle proof
    /// it checks cannot verify.
    ///
    /// **It was written `#[ignore]`d, as the specification the fix had to
    /// satisfy, and it failed on the accusation.** Four guards later it passes,
    /// and it is the thing that told each of them apart: the observer stopped at
    /// sequence 7, then 8, then 10, then 20, each number naming the next wall.
    #[test]
    fn a_seat_that_is_not_dealt_in_still_follows_the_hand_and_accuses_nobody() {
        // Seat 2 is on the roster and outside `P(k)`, so it is not dealt in.
        let observer: SeatIdx = 2;
        let mut hands: Vec<Hand> = Vec::new();
        let keys = [key(10), key(11), key(12)];
        let mut pending: Vec<Vec<u8>> = Vec::new();
        for seat in 0..3u8 {
            let mut o = opening3(seat);
            o.required = vec![0, 1];
            let (h, sends) = Hand::open(o, &keys[usize::from(seat)], NOW, 30_000)
                .expect("every seat on the roster opens the hand, dealt in or not");
            assert_eq!(
                h.init().dealt_in,
                vec![0, 1],
                "seat {seat} agrees who is dealt in"
            );
            for Send::Broadcast(b) in sends {
                pending.push(b);
            }
            hands.push(h);
        }

        // A broadcast bus: everything anybody says reaches everybody else.
        let mut accusations = 0usize;
        let mut aborts = 0usize;
        let mut last_refusal: Option<(u64, String)> = None;
        for _ in 0..512 {
            if pending.is_empty() {
                let Some(turn) = (0..2u8).find_map(|s| hands[usize::from(s)].turn()) else {
                    break;
                };
                let seat = turn.seat;
                let h = &mut hands[usize::from(seat)];
                let turn = h.turn().expect("that hand agrees it is to act");
                let action = if turn.legal.can_check {
                    Action::Check
                } else {
                    Action::Call
                };
                let out = h
                    .act(action, &keys[usize::from(seat)], NOW)
                    .expect("a legal action");
                for Send::Broadcast(b) in out {
                    pending.push(b);
                }
                continue;
            }
            let batch: Vec<Vec<u8>> = std::mem::take(&mut pending);
            for bytes in batch {
                for seat in 0..3u8 {
                    let h = &mut hands[usize::from(seat)];
                    let at = h.slot().sequence;
                    let answer = h.on_event(&bytes, &keys[usize::from(seat)], NOW);
                    if seat == observer {
                        if let Err(e) = &answer {
                            last_refusal = Some((at, format!("{e:?}")));
                        }
                    }
                    if let Ok(out) = answer {
                        for Send::Broadcast(b) in out {
                            // An abort with an empty `attributed` is the
                            // anonymous deadline abort and names nobody; one
                            // that carries a key is the accusation. Counting
                            // both as one would repeat the mistake `S1-S` was
                            // corrected for once already.
                            if let Ok((kind, _, _)) =
                                crate::net::chained::peek(&b, HAND_ABORT_CAP)
                            {
                                if kind == EventType::HandAbort {
                                    aborts += 1;
                                    if let Ok(o) = crate::net::chained::open_in_hand(
                                        &b,
                                        HAND_ABORT_CAP,
                                        EventType::HandAbort,
                                        &[1u8; 32],
                                        1,
                                    ) {
                                        if let Ok(body) = crate::net::chained::payload::<
                                            crate::table::handwire::HandAbort,
                                        >(&o, HAND_ABORT_CAP)
                                        {
                                            if !body.attributed.is_empty() {
                                                accusations += 1;
                                            }
                                        }
                                    }
                                }
                            }
                            pending.push(b);
                        }
                    }
                }
            }
        }

        assert_eq!(
            accusations, 0,
            "nobody was named: an observer that cannot verify a shuffle must not \
             conclude that the shuffler cheated ({aborts} abort(s) in all)"
        );
        for seat in [0u8, 1] {
            assert!(
                hands[usize::from(seat)].checkpoint8().is_some(),
                "seat {seat} played the hand and reached its settlement"
            );
        }
        let stopped = (
            hands[usize::from(observer)].slot().sequence,
            hands[usize::from(observer)].dealt(),
            hands[usize::from(observer)].deck_ready(),
            hands[usize::from(observer)].betting_over(),
            hands[0].slot().sequence,
        );
        assert!(
            hands[usize::from(observer)].checkpoint8().is_some(),
            "the observer stopped at {stopped:?} (sequence, dealt, deck_ready, betting_over, seat 0's sequence), last refusal {last_refusal:?} \
             back in that section 4.9 gives it"
        );
    }

    /// that decides `dealt_in` would fork the chain.
    #[test]
    fn the_bank_is_spent_by_absence_and_earned_by_presence() {
        let mut o = opening3(0);
        assert_eq!(o.grace, vec![GRACE_HANDS; 3]);

        // Seat 2 goes quiet. It is still on the roster and still has chips, so
        // it still posts blinds from its position — but it is not dealt in.
        o.required = vec![0, 1];
        let (h, _) = Hand::open(o.clone(), &key(10), NOW, 30_000).unwrap();
        assert_eq!(
            h.init().dealt_in,
            vec![0, 1],
            "an absent seat is not a required contributor of a deck key"
        );
        assert_eq!(
            h.init().stacks.len(),
            3,
            "and it is still on the roster, with its chips"
        );

        // A seat with no allowance left is not dealt in even when it is in the
        // required set — which is what stops a player dropping every hand.
        let mut spent = opening3(0);
        spent.grace = vec![GRACE_HANDS, GRACE_HANDS, 0];
        let (h, _) = Hand::open(spent, &key(10), NOW, 30_000).unwrap();
        assert_eq!(
            h.init().dealt_in,
            vec![0, 1],
            "a seat that burned its allowance stays out until it earns one back"
        );
    }

    /// A busted seat is not dealt in, whatever its allowance says.
    #[test]
    fn no_chips_means_no_hand() {
        let mut o = opening3(0);
        o.seats[2].2 = 0;
        let (h, _) = Hand::open(o, &key(10), NOW, 30_000).unwrap();
        assert_eq!(h.init().dealt_in, vec![0, 1]);
    }

    /// Three seats to the first betting decision, so the deadline path has
    /// something to be about.
    fn three_to_the_bet() -> ([Hand; 3], [SigningKey; 3]) {
        let keys = [key(10), key(11), key(12)];
        let (mut a, from_a) = Hand::open(opening3(0), &keys[0], NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening3(1), &keys[1], NOW, 30_000).unwrap();
        let (mut c, from_c) = Hand::open(opening3(2), &keys[2], NOW, 30_000).unwrap();

        let mut pending: Vec<(SeatIdx, Vec<Send>)> = vec![(0, from_a), (1, from_b), (2, from_c)];
        for _ in 0..64 {
            if pending.is_empty() {
                break;
            }
            let mut next: Vec<(SeatIdx, Vec<Send>)> = Vec::new();
            for (from, sends) in std::mem::take(&mut pending) {
                for seat in 0..3u8 {
                    if seat == from {
                        continue;
                    }
                    let hand: &mut Hand = match seat {
                        0 => &mut a,
                        1 => &mut b,
                        _ => &mut c,
                    };
                    let out = deliver(hand, &sends, &keys[usize::from(seat)]);
                    if !out.is_empty() {
                        next.push((seat, out));
                    }
                }
            }
            pending = next;
        }
        ([a, b, c], keys)
    }

    /// **One peer cannot take the action from a player who was about to act.**
    ///
    /// This is the property the whole mechanism exists for. A single vote is
    /// not evidence and does nothing: without every other seat's signature
    /// there is no certificate, and without a certificate nothing moves.
    #[test]
    fn one_vote_forces_nothing() {
        let (mut hands, keys) = three_to_the_bet();
        let up = hands[0].turn().expect("somebody is to act").seat;
        let rogue = (0..3u8).find(|s| *s != up).expect("a third seat");
        let late = NOW + 60_000;

        let votes = hands[usize::from(rogue)]
            .vote_on_timeouts(&keys[usize::from(rogue)], late, 0)
            .unwrap();
        assert_eq!(votes.len(), 1, "a rogue may say its own timer expired");

        // Nobody else votes. Deliver the rogue's vote everywhere and check that
        // the seat it names is still the one to act, on every peer.
        for seat in 0..3u8 {
            if seat == rogue {
                continue;
            }
            let out = deliver(&mut hands[usize::from(seat)], &votes, &keys[usize::from(seat)]);
            assert!(out.is_empty(), "a lone vote produces no certificate");
        }
        for (seat, hand) in hands.iter().enumerate() {
            assert_eq!(
                hand.turn().map(|t| t.seat),
                Some(up),
                "seat {seat} still has the same player to act"
            );
        }
    }

    /// A seat the carrier is still delivering from is late, not silent — and
    /// the reprieve runs out.
    ///
    /// `S1-BK`. The stage clock cannot tell a seat that said nothing from a
    /// seat whose message the wire refused and whose carrier is still trying;
    /// the receive array can, because a message sitting in it arrived out of
    /// order, which means that seat IS sending. Voting anyway accuses a player
    /// of the carrier's backlog.
    ///
    /// The bound is the other half and it is what keeps `D-026` intact: a seat
    /// could otherwise buy silence for ever by sending later messages while
    /// withholding the one a stage needs, so past twice the stage budget the
    /// vote goes ahead whatever the carrier says.
    #[test]
    fn a_seat_the_carrier_is_still_delivering_is_not_accused_yet() {
        // Past the betting stage's own deadline — `action_timeout` 20 s plus
        // `grace` 5 s — but well inside twice it, which is where the reprieve
        // lives. Outside that window there is nothing to test: the vote goes
        // ahead by design.
        let late = NOW + 30_000;

        let (mut hands, keys) = three_to_the_bet();
        let up = hands[0].turn().expect("somebody is to act").seat;
        let voter = (0..3u8).find(|s| *s != up).expect("a third seat");

        // With the carrier saying nothing, the vote is made, as it always was.
        let plain = hands[usize::from(voter)]
            .vote_on_timeouts(&keys[usize::from(voter)], late, 0)
            .unwrap();
        assert_eq!(plain.len(), 1, "a silent seat is voted on");

        // A fresh table, because the vote above is remembered.
        let (mut hands, keys) = three_to_the_bet();
        let up = hands[0].turn().expect("somebody is to act").seat;
        let voter = (0..3u8).find(|s| *s != up).expect("a third seat");
        let bit = 1u32 << u32::from(up);

        let held = hands[usize::from(voter)]
            .vote_on_timeouts(&keys[usize::from(voter)], late, bit)
            .unwrap();
        assert!(
            held.is_empty(),
            "a seat whose traffic is still arriving was accused of silence"
        );

        // And the reprieve ends. Twice the stage budget is past the point where
        // the carrier gives a confirmed peer up, so nothing is owed to it.
        let much_later = NOW + 600_000;
        let forced = hands[usize::from(voter)]
            .vote_on_timeouts(&keys[usize::from(voter)], much_later, bit)
            .unwrap();
        assert_eq!(
            forced.len(),
            1,
            "the carrier's word suppressed the vote for ever, which is D-026's exploit"
        );
    }

    /// Unanimity of the voter set does force it, and the effect is the one the
    /// rules give: **check when nothing is owed, fold when facing a bet**.
    #[test]
    fn unanimity_acts_for_a_seat_that_did_not() {
        let (mut hands, keys) = three_to_the_bet();
        let up = hands[0].turn().expect("somebody is to act").seat;
        let owed = hands[0].turn().unwrap().to_call;
        let voters: Vec<u8> = (0..3u8).filter(|s| *s != up).collect();
        let late = NOW + 60_000;

        // Both voters' own timers expire. Each says so.
        let mut said: Vec<(SeatIdx, Vec<Send>)> = Vec::new();
        for v in &voters {
            let out = hands[usize::from(*v)]
                .vote_on_timeouts(&keys[usize::from(*v)], late, 0)
                .unwrap();
            assert_eq!(out.len(), 1, "seat {v} votes once");
            said.push((*v, out));
        }

        // Deliver everything to everybody until it settles: the second vote to
        // arrive completes the set and produces a certificate, and the
        // certificates then complete their own stage.
        for _ in 0..8 {
            let mut next: Vec<(SeatIdx, Vec<Send>)> = Vec::new();
            for (from, sends) in std::mem::take(&mut said) {
                for seat in 0..3u8 {
                    if seat == from {
                        continue;
                    }
                    let out =
                        deliver(&mut hands[usize::from(seat)], &sends, &keys[usize::from(seat)]);
                    if !out.is_empty() {
                        next.push((seat, out));
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            said = next;
        }

        // The seat that said nothing has been acted for, on both voters.
        for v in &voters {
            let hand = &hands[usize::from(*v)];
            assert_ne!(
                hand.turn().map(|t| t.seat),
                Some(up),
                "seat {v} still waits for the seat the certificate acted for"
            );
            if owed > 0 {
                assert!(
                    hand.folded()[usize::from(up)],
                    "facing a bet, the certificate folds"
                );
            } else {
                assert!(
                    !hand.folded()[usize::from(up)],
                    "with nothing owed it checks, and never folds for free"
                );
            }
            assert_eq!(hand.strikes_against(up), 1, "and it counts as one strike");
        }
    }

    /// Heads-up the mechanism is inert, which is the point rather than a gap.
    ///
    /// The voter set is the one opponent, so "unanimity" would be the signature
    /// of the single party with an interest in the outcome. Below the floor a
    /// vote is not even sent.
    #[test]
    fn heads_up_nobody_votes() {
        let keys = [key(10), key(11)];
        let (mut a, from_a) = Hand::open(opening(0), &keys[0], NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &keys[1], NOW, 30_000).unwrap();
        deliver(&mut b, &from_a, &keys[1]);
        deliver(&mut a, &from_b, &keys[0]);

        let late = NOW + 60_000;
        assert!(
            a.vote_on_timeouts(&keys[0], late, 0).unwrap().is_empty(),
            "with one voter there is nothing a vote could become"
        );
        assert!(b.vote_on_timeouts(&keys[1], late, 0).unwrap().is_empty());
    }

    /// Before its own timer expires a peer says nothing, whatever anybody else
    /// claims. The deadline is measured on the peer's own clock and on no
    /// other, so an early vote is one this client never joins.
    #[test]
    fn nobody_votes_early() {
        let (mut hands, keys) = three_to_the_bet();
        let up = hands[0].turn().expect("somebody is to act").seat;
        let other = (0..3u8).find(|s| *s != up).unwrap();
        assert!(
            hands[usize::from(other)]
                .vote_on_timeouts(&keys[usize::from(other)], NOW + 1_000, 0)
                .unwrap()
                .is_empty(),
            "one second in, nobody's timer has expired"
        );
    }

    /// Three certificates and the seat sits out: it keeps its stack, posts
    /// dead money, takes no cards and drains. The tournament's dead seat.
    #[test]
    fn three_strikes_and_the_seat_takes_no_more_cards() {
        let mut o = opening3(0);
        // The state a seat reaches after three certificates against it, which
        // `apply_certificate` produces one at a time.
        o.grace = vec![GRACE_HANDS, GRACE_HANDS, 0];
        let (h, _) = Hand::open(o, &key(10), NOW, 30_000).unwrap();
        assert_eq!(
            h.init().dealt_in,
            vec![0, 1],
            "a seat sitting out takes no cards"
        );
        assert_eq!(
            h.init().stacks.len(),
            3,
            "and keeps its stack, which the blinds go on taking"
        );
    }

    /// A mucked hand is never opened, by anybody, ever.
    ///
    /// Not because the interface declines to draw it: because the share that
    /// would open it was never published and no peer holds it.
    #[test]
    fn a_mucked_hand_stays_shut() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        let mut queue: Vec<(SeatIdx, Vec<Send>)> = Vec::new();
        queue.push((1, deliver(&mut b, &a_deck, &key(11))));
        queue.push((0, deliver(&mut a, &b_deck, &key(10))));
        let keys = [key(10), key(11)];
        for _ in 0..256 {
            if let Some((from, sends)) = queue.pop() {
                if sends.is_empty() {
                    continue;
                }
                let to = 1 - from;
                let hand: &mut Hand = if to == 0 { &mut a } else { &mut b };
                let out = deliver(hand, &sends, &keys[usize::from(to)]);
                queue.push((to, out));
                continue;
            }
            if a.betting_over() && b.betting_over() {
                break;
            }
            let Some(turn) = a.turn().or_else(|| b.turn()) else {
                break;
            };
            let seat = turn.seat;
            let hand: &mut Hand = if seat == 0 { &mut a } else { &mut b };
            let action = if hand.turn().unwrap().legal.can_check {
                Action::Check
            } else {
                Action::Call
            };
            let out = hand.act(action, &keys[usize::from(seat)], NOW).unwrap();
            queue.push((seat, out));
        }

        for seat in 0..2u8 {
            if a.mucked(seat) {
                assert!(a.shown(seat).is_none(), "a mucked hand is not on the table");
                assert!(b.shown(seat).is_none(), "and not on the other peer's");
                // And the other peer is still one share short of it: the owner
                // never published its own, which is the whole of the guarantee.
                let map = b.index_map().expect("the map exists");
                for index in map.hole_cards(seat).unwrap() {
                    assert_eq!(
                        b.outstanding_shares(index),
                        Some(vec![seat]),
                        "exactly the owner's share is missing"
                    );
                }
            }
        }
    }

    /// Drive two peers until seat 0 holds seat 1's `SHUFFLE_STEP` and has not
    /// yet seen its `SHUFFLE_PROOF`, and hand back both frames.
    ///
    /// That is the only state in which seat 0 can judge an accusation about
    /// them: its chain is at seat 1's position, so it holds the input deck the
    /// proof is against and derives the same context. One event later it has
    /// accepted the proof and moved on, and the gate holds the abort instead of
    /// ruling on it.
    fn shuffling_with_a_step_in_hand() -> (Hand, Hand, Vec<u8>, Vec<u8>) {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let keys = [key(10), key(11)];
        // The opening exchange, in the order every other two-peer test here
        // uses: each peer hears the other's `HAND_INIT`, then each hears the
        // other's `DECK_INIT`. A queue seeded before that runs the frames out
        // of order and every one of them is `NotYet`.
        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        let mut queue: Vec<(SeatIdx, Vec<Send>)> = vec![
            (1, deliver(&mut b, &a_deck, &key(11))),
            (0, deliver(&mut a, &b_deck, &key(10))),
        ];
        let mut step: Option<Vec<u8>> = None;
        let mut proof: Option<Vec<u8>> = None;

        'outer: for _ in 0..64 {
            let Some((from, sends)) = queue.pop() else { break };
            let to = 1 - from;
            let mut produced = Vec::new();
            for Send::Broadcast(frame) in &sends {
                let (kind, _, _) = chained::peek(frame, PEEK_CAP).unwrap();
                if from == 1 && kind == EventType::ShuffleStep {
                    step = Some(frame.clone());
                }
                if from == 1 && kind == EventType::ShuffleProof {
                    proof = Some(frame.clone());
                    break 'outer;
                }
                let hand: &mut Hand = if to == 0 { &mut a } else { &mut b };
                let mut more = hand.on_event(frame, &keys[usize::from(to)], NOW).unwrap();
                produced.append(&mut more);
            }
            queue.push((to, produced));
        }

        let step = step.expect("seat 1 took its step");
        let proof = proof.expect("and proved it");
        assert!(
            matches!(&a.phase, Phase::Shuffling { chain, heard, .. }
                     if chain.whose_turn() == Some(1) && heard.is_some()),
            "seat 0 holds the step and is waiting for the proof"
        );
        (a, b, step, proof)
    }

    /// **A `cause = 2` abort over a proof that is perfectly good ends nobody's
    /// hand.**
    ///
    /// This is the whole reason the acceptance gate exists. §4.10 lets causes 2
    /// and 3 be accepted *at once* — no deadline, no certificate, no other
    /// seat's agreement — which would otherwise make one message enough for any
    /// seat to void any hand it did not like the look of. The permission is
    /// conditional on arithmetic every receiver redoes, and here the arithmetic
    /// says the proof holds.
    #[test]
    fn a_cause_two_abort_over_a_valid_proof_is_refused() {
        let (mut a, mut b, step, proof) = shuffling_with_a_step_in_hand();

        // Seat 1's own frames, genuine and verifying, dressed up as evidence
        // against seat 1.
        let sends = b
            .abort_bad_shuffle(1, [step, proof], &key(11), NOW)
            .expect("the accusation is well formed - it is simply false");
        let Send::Broadcast(abort) = &sends[0];

        let err = a
            .on_event(abort, &key(10), NOW)
            .expect_err("a false accusation is refused");
        assert!(
            matches!(&err, Failed::Elsewhere { what, .. }
                     if what.contains("does not verify at this client either")),
            "and it says why: {err}"
        );
        assert!(
            a.aborted().is_none(),
            "the hand goes on - which is the point"
        );
    }

    /// **A proof that really does not hold ends the hand on one message.**
    ///
    /// The other half of the gate, and the reason `cause = 2` exists at all.
    /// Before it, a refused proof left the chain unable to complete —
    /// `accept_step` spends the seat's one attempt before verifying, so there
    /// is no retry — and every peer sat until `hand_deadline_ms` and then
    /// aborted with nobody named. Here the hand is over at once and the seat
    /// whose proof failed is named, with the two frames that say so attached.
    ///
    /// The bad proof is **correctly signed by the accused** and wrong only in
    /// its argument, which is the only shape that tests anything: a frame with
    /// a broken signature never reaches the argument at all.
    #[test]
    fn a_cause_two_abort_over_a_proof_that_fails_ends_the_hand() {
        let (a, b, step, proof) = shuffling_with_a_step_in_hand();

        // Re-seal seat 1's proof at its own slot with the argument replaced by
        // zeroes. Same seat, same position, same parent, same deck hashes —
        // everything the gate checks before the argument still holds, and the
        // argument does not.
        let opened = chained::open_in_hand(
            &proof,
            FRAME_CAP,
            EventType::ShuffleProof,
            &a.open.table_id,
            a.open.hand_id,
        )
        .unwrap();
        let mut body: ShuffleProof =
            chained::payload(&opened, SHUFFLE_PROOF_CAP).unwrap();
        body.proof = vec![0u8; body.proof.len()];
        let slot = chained::Slot {
            table_id: opened.envelope.table_id,
            hand_id: opened.envelope.hand_id,
            sequence: opened.envelope.sequence,
            previous_event_hash: opened.envelope.previous_event_hash,
        };
        let bad = chained::seal(
            EventType::ShuffleProof,
            &slot,
            &body,
            &key(11),
            NOW,
            30_000,
            SHUFFLE_PROOF_CAP,
        )
        .unwrap();

        let accused = a.open.seats[1].1;
        let abort = HandAbort::on_bad_shuffle(accused, [step, bad], b.mine.stacks.clone());
        a.bad_shuffle_holds(&abort, 0)
            .expect("the argument does not hold, and this client says so itself");
    }

    /// Evidence signed by somebody other than the seat the abort names proves
    /// nothing about that seat, and is refused before any argument is verified.
    #[test]
    fn cause_two_evidence_must_be_signed_by_the_seat_it_accuses() {
        let (a, b, step, proof) = shuffling_with_a_step_in_hand();

        // Seat 1's frames, but the abort names seat 0. Seat 0 signed neither.
        let accused = a.open.seats[0].1;
        let body = HandAbort::on_bad_shuffle(
            accused,
            [step, proof],
            b.mine.stacks.clone(),
        );
        let err = a
            .bad_shuffle_holds(&body, 1)
            .expect_err("frames the accused never signed prove nothing");
        assert!(
            matches!(&err, Failed::Elsewhere { what, .. }
                     if what.contains("signed by the seat the abort accuses")),
            "{err}"
        );
    }

    /// An abort this client cannot judge is **held**, never refused.
    ///
    /// Refusing would punish a peer for this receiver's own position in the
    /// hand, and `run.rs` turns anything but `NotYet` into a GossipSub
    /// `Reject` — so a receiver one stage behind would score down the peer
    /// carrying the one message that ends the hand.
    #[test]
    fn a_cause_two_abort_is_held_when_this_client_cannot_judge_it() {
        let (mut a, mut b, step, proof) = shuffling_with_a_step_in_hand();

        // Move seat 0 past the position the evidence is about by giving it the
        // proof. Now its chain is somewhere else and it can no longer derive
        // the context the argument was made in.
        let _ = a.on_event(&proof, &key(10), NOW).unwrap();

        let sends = b.abort_bad_shuffle(1, [step, proof], &key(11), NOW).unwrap();
        let Send::Broadcast(abort) = &sends[0];
        assert!(
            matches!(a.on_event(abort, &key(10), NOW), Err(Failed::NotYet)),
            "held, not refused"
        );
    }

    /// An action out of turn is refused and names who was up.
    #[test]
    fn acting_out_of_turn_is_refused() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        let mut queue: Vec<(SeatIdx, Vec<Send>)> = Vec::new();
        queue.push((1, deliver(&mut b, &a_deck, &key(11))));
        queue.push((0, deliver(&mut a, &b_deck, &key(10))));
        let keys = [key(10), key(11)];
        for _ in 0..64 {
            let Some((from, sends)) = queue.pop() else { break };
            if sends.is_empty() {
                continue;
            }
            let to = 1 - from;
            let hand: &mut Hand = if to == 0 { &mut a } else { &mut b };
            let out = deliver(hand, &sends, &keys[usize::from(to)]);
            queue.push((to, out));
        }

        let up = a.turn().expect("somebody is to act").seat;
        let waiting = 1 - up;
        let hand: &mut Hand = if waiting == 0 { &mut a } else { &mut b };
        let e = hand
            .act(Action::Check, &keys[usize::from(waiting)], NOW)
            .unwrap_err();
        assert!(
            matches!(e, Failed::OutOfTurn { seat, expected: Some(x) } if seat == waiting && x == up),
            "{e}"
        );
    }

    /// The milestone: two clients, no network, each holding two cards it can
    /// read and the other cannot.
    #[test]
    fn two_clients_reach_their_own_hole_cards() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();

        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        deliver(&mut b, &a_deck, &key(11));
        let a_shuffle = deliver(&mut a, &b_deck, &key(10));
        let b_shuffle = deliver(&mut b, &a_shuffle, &key(11));
        let a_more = deliver(&mut a, &b_shuffle, &key(10));

        assert!(a.cards().is_none(), "A has committed and dealt, not read");
        let b_more = deliver(&mut b, &a_more, &key(11));
        assert!(b.dealt_cards(), "B held every share for its own two");
        assert!(deliver(&mut a, &b_more, &key(10)).is_empty());
        assert!(a.dealt_cards());

        let mine = a.cards().unwrap();
        let theirs = b.cards().unwrap();
        assert_ne!(mine[0], mine[1], "two cards, not one twice");
        assert_ne!(theirs[0], theirs[1]);
        for card in &mine {
            assert!(
                !theirs.contains(card),
                "one deck: no card is dealt to two seats"
            );
        }
        assert_eq!(
            a.slot(),
            b.slot(),
            "and both left the deal at one stage, off one parent"
        );
    }

    /// A share for a card its sender was not asked to help open is refused.
    ///
    /// The dangerous version of this is a board index — a seat publishing a
    /// board share early — and the check is the same one: the index set must be
    /// exactly what the sender owes, not a superset of it.
    #[test]
    fn a_share_for_an_index_not_owed_is_refused() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        deliver(&mut b, &a_deck, &key(11));
        let a_shuffle = deliver(&mut a, &b_deck, &key(10));
        let b_shuffle = deliver(&mut b, &a_shuffle, &key(11));
        let a_more = deliver(&mut a, &b_shuffle, &key(10));

        // A's commit reaches B first, so B is dealing when the shares arrive.
        let Send::Broadcast(a_commit) = &a_more[0];
        let Send::Broadcast(a_deal) = &a_more[1];
        deliver(&mut b, &[Send::Broadcast(a_commit.clone())], &key(11));

        let padded = tamper::<DealPrivate>(
            a_deal,
            EventType::DealPrivate,
            &b.slot(),
            DEAL_PRIVATE_CAP,
            |d| {
                let mut extra = d.entries[0].clone();
                extra.deck_index = 51;
                d.entries.push(extra);
            },
        );
        let e = b.on_event(&padded, &key(11), NOW).unwrap_err();
        assert!(matches!(e, Failed::BadToken { seat: 0, .. }), "{e}");
        assert!(!b.dealt_cards(), "and no card was opened");
    }

    /// Two peers that derived different decks stop at the barrier, before any
    /// card exists — which is the only point at which stopping is cheap.
    #[test]
    fn a_commitment_to_another_deck_is_refused() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        deliver(&mut b, &a_deck, &key(11));
        let a_shuffle = deliver(&mut a, &b_deck, &key(10));
        let b_shuffle = deliver(&mut b, &a_shuffle, &key(11));
        let a_more = deliver(&mut a, &b_shuffle, &key(10));

        let Send::Broadcast(a_commit) = &a_more[0];
        let wrong = tamper::<DeckCommit>(
            a_commit,
            EventType::DeckCommit,
            &b.slot(),
            DECK_COMMIT_CAP,
            |c| c.final_deck_hash[0] ^= 1,
        );
        let e = b.on_event(&wrong, &key(11), NOW).unwrap_err();
        assert!(
            matches!(
                e,
                Failed::DeckDisagrees {
                    seat: 0,
                    what: "the final deck"
                }
            ),
            "{e}"
        );
    }

    /// Re-seal one of A's own events with a changed body, as A, so the
    /// envelope is honest and only the contents are not.
    fn tamper<T: minicbor::Encode<()> + for<'b> minicbor::Decode<'b, ()>>(
        bytes: &[u8],
        kind: EventType,
        at: &Slot,
        cap: usize,
        change: impl FnOnce(&mut T),
    ) -> Vec<u8> {
        let opened = chained::open(bytes, FRAME_CAP, kind, at).unwrap();
        let mut body: T = chained::payload(&opened, cap).unwrap();
        change(&mut body);
        chained::seal(kind, at, &body, &key(10), NOW, 30_000, cap).unwrap()
    }

    /// Two clients up to the point where seat 0 has sent a step and a proof,
    /// and seat 1 has heard neither.
    fn ready_to_shuffle() -> (Hand, Hand, Vec<Send>) {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        deliver(&mut b, &a_deck, &key(11));
        let shuffle = deliver(&mut a, &b_deck, &key(10));
        (a, b, shuffle)
    }

    /// Two clients up to the point where seat 0 has published its
    /// `DEAL_PRIVATE` and seat 1 is in `Dealing`, waiting for it.
    ///
    /// Seat 1 must be at that stage and not past it: the reveal context is
    /// built from a `sequence`, and a receiver that has moved on derives a
    /// different one. Everything up to the deal is delivered; the deal itself
    /// is handed back untouched.
    fn ready_to_deal() -> (Hand, Hand, Vec<u8>, Vec<u8>) {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        deliver(&mut b, &a_deck, &key(11));
        let a_shuffle = deliver(&mut a, &b_deck, &key(10));
        let b_shuffle = deliver(&mut b, &a_shuffle, &key(11));
        let a_more = deliver(&mut a, &b_shuffle, &key(10));

        let mut deal: Option<Vec<u8>> = None;
        let mut other: Option<Vec<u8>> = None;
        for Send::Broadcast(frame) in &a_more {
            let (kind, _, _) = chained::peek(frame, PEEK_CAP).unwrap();
            if kind == EventType::DealPrivate {
                deal = Some(frame.clone());
                break;
            }
            // Something seat 0 signed this hand that is not a reveal, for the
            // test that the gate refuses evidence of the wrong kind.
            other = Some(frame.clone());
            b.on_event(frame, &key(11), NOW).unwrap();
        }
        let deal = deal.expect("seat 0 published its shares for seat 1's cards");
        let other = other.expect("and something before it that is not a reveal");
        assert!(
            matches!(b.phase, Phase::Dealing { .. }),
            "seat 1 is at the deal and has not heard it"
        );
        (a, b, deal, other)
    }

    /// **A reveal share that does not hold ends the hand at once**, with the
    /// seat named and the frame that proves it attached.
    ///
    /// The reveal half of `cause = 2`. The tampering swaps the two entries'
    /// tokens: both still decode — they are each other's, and each is a
    /// perfectly good curve point — so the message survives every structural
    /// check and dies at the DLEQ, which is the only failure that is evidence.
    #[test]
    fn a_reveal_share_that_does_not_hold_ends_the_hand_with_cause_three() {
        let (_a, mut b, deal, _other) = ready_to_deal();
        let broken = tamper::<DealPrivate>(
            &deal,
            EventType::DealPrivate,
            &b.slot(),
            DEAL_PRIVATE_CAP,
            |d| {
                let first = d.entries[0].token.clone();
                d.entries[0].token = d.entries[1].token.clone();
                d.entries[1].token = first;
            },
        );

        let sends = b
            .on_event(&broken, &key(11), NOW)
            .expect("the hand ends on this message rather than at its deadline");
        assert_eq!(sends.len(), 1, "one abort, broadcast");
        assert_eq!(
            b.aborted(),
            Some(Abort::BadReveal { seat: 0 }),
            "the seat whose share failed is named"
        );

        let Send::Broadcast(bytes) = &sends[0];
        let opened = chained::open_in_hand(
            bytes,
            HAND_ABORT_CAP,
            EventType::HandAbort,
            &b.open.table_id,
            b.open.hand_id,
        )
        .unwrap();
        let body: HandAbort = chained::payload(&opened, HAND_ABORT_CAP).unwrap();
        assert_eq!(body.cause, 3);
        assert_eq!(body.evidence.len(), 1, "one frame: the share carries its own");
        assert_eq!(body.attributed, vec![b.open.seats[0].1]);
        assert!(body.cert_hash.is_none(), "no certificate: none is needed");
        assert!(body.consistent(&b.mine.stacks).is_ok());
    }

    /// **A `cause = 3` abort over a share that is perfectly good ends nobody's
    /// hand**, which is the direction the gate exists for.
    #[test]
    fn a_cause_three_abort_over_a_valid_share_is_refused() {
        let (_a, b, deal, _other) = ready_to_deal();
        let accused = b.open.seats[0].1;
        let body = HandAbort::on_bad_reveal(accused, deal, b.mine.stacks.clone());
        let err = b
            .bad_reveal_holds(&body, 0)
            .expect_err("a false accusation is refused");
        assert!(
            matches!(&err, Failed::Elsewhere { what, .. }
                     if what.contains("verifies at this client")),
            "and it says why: {err}"
        );
    }

    /// Evidence that is not a reveal at all is refused before any share is
    /// checked, and evidence the accused never signed with it.
    #[test]
    fn cause_three_evidence_must_be_a_reveal_the_accused_signed() {
        let (_a, b, deal, other) = ready_to_deal();

        // Seat 0's frame, but the abort names seat 1.
        let wrong_seat = HandAbort::on_bad_reveal(
            b.open.seats[1].1,
            deal.clone(),
            b.mine.stacks.clone(),
        );
        let err = b.bad_reveal_holds(&wrong_seat, 0).unwrap_err();
        assert!(
            matches!(&err, Failed::Elsewhere { what, .. }
                     if what.contains("signed by the seat the abort accuses")),
            "{err}"
        );

        // And a frame that is not a reveal event at all.
        let init = HandAbort::on_bad_reveal(
            b.open.seats[0].1,
            other,
            b.mine.stacks.clone(),
        );
        let err = b.bad_reveal_holds(&init, 0).unwrap_err();
        assert!(
            matches!(&err, Failed::Elsewhere { what, .. }
                     if what.contains("were a reveal at all")),
            "{err}"
        );
    }

    /// A deck that is not fifty-two cards never reaches the crypto at all.
    #[test]
    fn a_deck_of_the_wrong_size_is_refused() {
        let (_a, mut b, shuffle) = ready_to_shuffle();
        let Send::Broadcast(step) = &shuffle[0];
        let short = tamper::<ShuffleStep>(
            step,
            EventType::ShuffleStep,
            &b.slot(),
            SHUFFLE_STEP_CAP,
            |s| s.deck.truncate(66 * 51),
        );
        let e = b.on_event(&short, &key(11), NOW).unwrap_err();
        assert!(
            matches!(e, Failed::BadDeck { seat: 0, .. }),
            "{e}"
        );
    }

    /// A round byte that does not match the chain's position is refused before
    /// the deck is even parsed.
    #[test]
    fn a_step_at_the_wrong_round_is_refused() {
        let (_a, mut b, shuffle) = ready_to_shuffle();
        let Send::Broadcast(step) = &shuffle[0];
        let wrong = tamper::<ShuffleStep>(
            step,
            EventType::ShuffleStep,
            &b.slot(),
            SHUFFLE_STEP_CAP,
            |s| s.shuffle_round = 1,
        );
        let e = b.on_event(&wrong, &key(11), NOW).unwrap_err();
        assert!(matches!(e, Failed::BadDeck { seat: 0, .. }), "{e}");
    }

    /// A proof that names a deck other than the step it follows is refused,
    /// and refused by the hash rather than by forty-two milliseconds of
    /// verification.
    #[test]
    fn a_proof_that_names_another_deck_is_refused() {
        let (_a, mut b, shuffle) = ready_to_shuffle();
        let Send::Broadcast(step) = &shuffle[0];
        let Send::Broadcast(proof) = &shuffle[1];
        b.on_event(step, &key(11), NOW).unwrap();

        let lifted = tamper::<ShuffleProof>(
            proof,
            EventType::ShuffleProof,
            &b.slot(),
            SHUFFLE_PROOF_CAP,
            |p| p.output_deck_hash[0] ^= 1,
        );
        let e = b.on_event(&lifted, &key(11), NOW).unwrap_err();
        assert!(matches!(e, Failed::BadDeck { seat: 0, .. }), "{e}");
        assert!(!b.shuffled());
    }

    /// An argument that does not hold **ends the hand at once**, with the seat
    /// named and the two frames that prove it attached.
    ///
    /// This used to assert `Failed::BadShuffle`, and the refusal was the whole
    /// of the answer: the chain could never complete — `accept_step` spends the
    /// seat's one attempt before verifying, so there is no retry — and every
    /// peer waited out `hand_deadline_ms` to reach an abort that named nobody.
    /// §4.10's `cause = 2` is what that ninety seconds was standing in for, and
    /// the outcome is now an `Ok` carrying it.
    ///
    /// The distinction the old name drew is still real and is still drawn: a
    /// mismatched deck hash is a different failure from a failed argument, and
    /// [`a_proof_that_names_another_deck_is_refused`] above still asserts
    /// `Failed::BadDeck` for it. Only an argument that verifiably does not hold
    /// is evidence, because only that is decidable from the frames themselves.
    #[test]
    fn an_argument_that_does_not_hold_ends_the_hand_with_cause_two() {
        let (_a, mut b, shuffle) = ready_to_shuffle();
        let Send::Broadcast(step) = &shuffle[0];
        let Send::Broadcast(proof) = &shuffle[1];
        b.on_event(step, &key(11), NOW).unwrap();

        let broken = tamper::<ShuffleProof>(
            proof,
            EventType::ShuffleProof,
            &b.slot(),
            SHUFFLE_PROOF_CAP,
            |p| {
                let n = p.proof.len();
                p.proof[n / 2] ^= 0xff;
            },
        );
        let sends = b
            .on_event(&broken, &key(11), NOW)
            .expect("the hand ends on this message rather than at its deadline");
        assert_eq!(sends.len(), 1, "one abort, broadcast");
        assert!(!b.shuffled(), "and the chain did not advance");
        assert_eq!(
            b.aborted(),
            Some(Abort::BadShuffle { seat: 0 }),
            "the seat whose proof failed is named"
        );

        // The message really is a `cause = 2` carrying both frames, and it says
        // so on the wire rather than only in this client's own phase.
        let Send::Broadcast(bytes) = &sends[0];
        let opened = chained::open_in_hand(
            bytes,
            HAND_ABORT_CAP,
            EventType::HandAbort,
            &b.open.table_id,
            b.open.hand_id,
        )
        .unwrap();
        let body: HandAbort = chained::payload(&opened, HAND_ABORT_CAP).unwrap();
        assert_eq!(body.cause, 2);
        assert_eq!(body.evidence.len(), 2, "the step and the proof");
        assert_eq!(
            body.attributed,
            vec![b.open.seats[0].1],
            "and it names seat 0 by key"
        );
        assert!(body.cert_hash.is_none(), "no certificate: none is needed");
        assert!(
            body.consistent(&b.mine.stacks).is_ok(),
            "an abort every receiver's own rules admit"
        );
    }

    /// The round-0 input hash is a constant, and it is the same constant for
    /// every client - which is the whole of what it has to be.
    #[test]
    fn the_open_deck_has_one_hash() {
        assert_eq!(input_deck_hash(None), input_deck_hash(None));
        assert_ne!(
            input_deck_hash(None),
            deck_hash(&[[0u8; CIPHERTEXT]; DECK]),
            "and it is not the hash of a deck of zeroes"
        );
    }

    /// Fifty-two cards out and fifty-two back, and nothing else accepted.
    #[test]
    fn a_deck_survives_the_wire_and_nothing_else_does() {
        let deck: Vec<Ciphertext> = (0..DECK as u8).map(|i| [i; CIPHERTEXT]).collect();
        let flat = flatten(&deck);
        assert_eq!(flat.len(), DECK * CIPHERTEXT);
        assert_eq!(unflatten(&flat).unwrap(), deck);
        assert!(unflatten(&flat[..flat.len() - 1]).is_none(), "short");
        assert!(unflatten(&[flat.clone(), vec![0]].concat()).is_none(), "long");
        assert!(unflatten(&[]).is_none(), "empty");
    }

    /// The secret is kept across the stage transition. Losing it would be a
    /// player who cannot read the hand they are sitting in, and it would not
    /// show until the cards were dealt.
    #[test]
    fn the_secret_survives_the_stage_it_was_made_in() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        assert!(a.secret().is_none(), "no secret before there is a deck");

        let b_deck = deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        assert!(a.secret().is_some(), "stage 1 opened, so there is one");
        assert!(a.deck().is_none(), "and no deck until every key is in");

        deliver(&mut a, &b_deck, &key(10));
        deliver(&mut b, &a_deck, &key(11));
        assert!(a.deck().is_some() && b.deck().is_some());
    }

    /// A key that is not a point on the curve is refused, and the seat that
    /// sent it is named — the deck review's whole point is that the library
    /// proves what it claims and the claim is not what a poker game needs.
    #[test]
    fn a_deck_key_that_is_not_a_point_is_refused() {
        let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        deliver(&mut b, &from_a, &key(11));
        let a_deck = deliver(&mut a, &from_b, &key(10));
        assert_eq!(a_deck.len(), 1);

        // Take A's own `DECK_INIT`, wreck the key inside it, and re-sign it as
        // A: the envelope is honest and the point is not.
        let Send::Broadcast(good) = &a_deck[0];
        let opened = chained::open(good, FRAME_CAP, EventType::DeckInit, &a.slot()).unwrap();
        let mut body: DeckInit =
            chained::payload(&opened, DECK_INIT_CAP).unwrap();
        body.key = vec![0xff; body.key.len()];
        let forged = chained::seal(
            EventType::DeckInit,
            &a.slot(),
            &body,
            &key(10),
            NOW,
            30_000,
            DECK_INIT_CAP,
        )
        .unwrap();

        let e = b.on_event(&forged, &key(11), NOW).unwrap_err();
        assert!(matches!(e, Failed::BadKey { seat: 0, .. }), "{e}");
        assert!(!b.deck_ready(), "and the deck does not exist");
    }

    /// A peer's own copy is recorded locally and never travels back to itself.
    /// Echoing it reads as a duplicate and looks like an anti-replay defect.
    #[test]
    fn a_client_does_not_wait_for_its_own_copy() {
        let (a, _) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        assert_eq!(a.waiting_for(), vec![1], "it has already heard itself");
    }

    /// The same message twice is the ordinary weather of GossipSub.
    #[test]
    fn the_same_copy_twice_is_not_a_fault() {
        let (mut a, _) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (_, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let Send::Broadcast(b_bytes) = &from_b[0];
        a.on_event(b_bytes, &key(10), NOW).unwrap();
        // The second copy arrives at a stage this client has already left. It
        // is not a fault and not worth a word.
        assert_eq!(a.on_event(b_bytes, &key(10), NOW), Ok(Vec::new()));
        assert!(a.dealt());
    }

    /// A copy that differs does not complete the stage for its emitter, and the
    /// receiver is told which field — because "the table is not moving" is not
    /// something anybody can act on.
    #[test]
    fn a_wrong_copy_names_its_field_and_does_not_complete_the_stage() {
        let (mut a, _) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();

        // B derives the same hand at a table it thinks has different blinds.
        let mut wrong = opening(1);
        wrong.small_blind = 25;
        wrong.big_blind = 50;
        let (_, from_b) = Hand::open(wrong, &key(11), NOW, 30_000).unwrap();
        let Send::Broadcast(b_bytes) = &from_b[0];

        let e = a.on_event(b_bytes, &key(10), NOW).unwrap_err();
        assert!(
            matches!(
                e,
                Failed::Disagrees {
                    seat: 1,
                    what: NotOurs::Differs(super::super::handwire::Field::SmallBlind)
                }
            ),
            "{e}"
        );
        assert!(!a.dealt(), "a wrong copy does not complete the stage");
        assert_eq!(a.waiting_for(), vec![1]);
    }

    /// A seat that is not on the roster is refused before its body is believed.
    #[test]
    fn a_stranger_is_not_a_seat() {
        let (mut a, _) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let mut theirs = opening(1);
        theirs.seats[1].1 = key(99).verifying_key().to_bytes();
        let (_, from_c) = Hand::open(theirs, &key(99), NOW, 30_000).unwrap();
        let Send::Broadcast(c_bytes) = &from_c[0];
        assert_eq!(
            a.on_event(c_bytes, &key(10), NOW),
            Err(Failed::NotAtThisTable)
        );
    }

    /// Heads-up, the button is the small blind. Getting this wrong is the
    /// classic mistake, and it would show as two clients disagreeing about
    /// `sb_position` and the hand never starting.
    #[test]
    fn heads_up_the_button_posts_the_small_blind() {
        let two = |n: usize, alive: Vec<bool>, button: u8| {
            let p = initial_positions(button, &alive, n as u8).unwrap();
            (p.button, p.small_blind, p.big_blind)
        };
        assert_eq!(two(2, vec![true, true], 0), (0, 0, 1));
        assert_eq!(two(2, vec![true, true], 1), (1, 1, 0));

        // The case the old derivation got wrong: three seats at the table, one
        // of them busted. Heads-up is a rule about players with chips, and
        // keying it on how many seats are occupied gives seat 2 a small blind
        // it should not post and leaves the button posting nothing.
        assert_eq!(
            two(3, vec![true, false, true], 0),
            (0, 0, 2),
            "two players with chips is heads-up whatever the third seat is"
        );
    }

    /// Three-handed and more, the blinds are the next two occupied seats.
    #[test]
    fn at_three_the_blinds_follow_the_button() {
        let at = |n: usize, alive: Vec<bool>, button: u8| {
            let p = initial_positions(button, &alive, n as u8).unwrap();
            (p.small_blind, p.big_blind)
        };
        assert_eq!(at(3, vec![true, true, true], 0), (1, 2));
        assert_eq!(at(3, vec![true, true, true], 2), (0, 1));

        let mut wide = vec![false; 8];
        for s in [0usize, 3, 7] {
            wide[s] = true;
        }
        assert_eq!(at(8, wide, 3), (7, 0), "the ring wraps past the empty seats");
    }

    /// **The last seat to ratify can choose the button.**
    ///
    /// `provisional_button` reads `session_id`, and `session_id` is a hash over
    /// the ratifications' `event_hash`es. An event hash covers the whole signed
    /// envelope, and the envelope carries `emitted_at_unix_ms` — a number its
    /// emitter picks. `TABLE_READY` also carries `capability_set`, a list of
    /// byte strings the emitter controls outright.
    ///
    /// So a seat that ratifies last can re-sign its own `TABLE_READY` with a
    /// different timestamp, recompute `session_id`, and stop when the button
    /// lands where it wants. At a table of `m` seats it needs about `m`
    /// attempts for a chosen seat, and a few hundred for a chosen seat with
    /// confidence: milliseconds of work.
    ///
    /// This test does not attack the client — it computes the same function the
    /// client does, over hashes an attacker can produce, and counts how few
    /// tries it takes. What it demonstrates is that **the initial button is not
    /// unbiased**, which is exactly what `PROTOCOL.md` §4.4's beacon exists to
    /// fix and why `provisional_button` is documented as *not the rule*.
    ///
    /// The dead-button rule makes the initial button worth choosing: it fixes
    /// who posts which blind in hand one and who acts last, and every later
    /// button is a rotation of it.
    #[test]
    fn the_last_seat_to_ratify_can_choose_the_button() {
        use crate::protocol::transcript::{session_id, Ratification};

        let table_id = [1u8; 32];
        let params = [2u8; 32];
        let roster_zero = [3u8; 32];
        let occupied: Vec<SeatIdx> = vec![0, 1, 2, 3, 4, 5];

        // Two seats have ratified and their hashes are fixed. The third is the
        // attacker's, and it varies only its own event hash - which is what
        // re-signing with a different timestamp gives it.
        let fixed = |seat: u8, b: u8| Ratification {
            seat,
            event_hash: [b; 32],
        };

        let tries_for = |want: SeatIdx| -> u32 {
            for n in 0u32..10_000 {
                let mut mine = [0u8; 32];
                mine[..4].copy_from_slice(&n.to_be_bytes());
                let rats = vec![fixed(0, 0xAA), fixed(1, 0xBB), Ratification { seat: 2, event_hash: mine }];
                let sid = session_id(&table_id, &params, &roster_zero, &rats);
                if provisional_button(&sid, &occupied) == want {
                    return n + 1;
                }
            }
            u32::MAX
        };

        // Every seat at the table is reachable, and cheaply.
        for want in &occupied {
            let tries = tries_for(*want);
            assert!(
                tries < 1_000,
                "seat {want} took {tries} tries, which is still trivial but means \
                 this test is measuring something other than what it thinks"
            );
        }

        // And the cost of picking a specific one is what an attacker would
        // actually pay: a handful of hashes.
        let worst = occupied.iter().map(|w| tries_for(*w)).max().unwrap();
        assert!(
            worst < 100,
            "choosing the button cost {worst} hashes, which is still nothing"
        );
    }

    /// Every peer derives one button from one session, or stage 0 cannot
    /// complete for anybody.
    #[test]
    fn the_provisional_button_is_the_same_for_everybody() {
        let occupied = [0u8, 1, 2, 5];
        let session = [7u8; 32];
        assert_eq!(
            provisional_button(&session, &occupied),
            provisional_button(&session, &occupied)
        );
        assert!(occupied.contains(&provisional_button(&session, &occupied)));
    }

    /// An event for a stage this client has not reached is held, not refused —
    /// GossipSub does not order two messages, and a peer one step ahead is not
    /// a peer that is wrong.
    #[test]
    fn an_event_from_a_later_stage_is_held_and_replayed() {
        let (mut a, _) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let (_, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let Send::Broadcast(b_bytes) = &from_b[0];

        a.hold(b_bytes.clone());
        assert!(!a.dealt(), "held, not applied");
        let (sends, failures) = a.replay_early(&key(10), NOW);
        assert!(failures.is_empty(), "it was good all along");
        assert_eq!(sends.len(), 1, "and completing stage 0 offers a deck key");
        assert!(a.dealt());
    }
}
