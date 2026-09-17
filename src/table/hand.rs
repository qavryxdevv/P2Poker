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
    DealPrivate, DeckCommit, DeckInit, Field, HandAbort, HandComplete, HandInit, NotOurs, PotAward,
    Refund, RevealEntry, ShowdownMuck, ShowdownReveal, ShuffleProof, ShuffleStep, TimeoutCert,
    TimeoutVote, CertSubject, CAUSE_FLOOD};
use super::stage::{Collective, Heard};
use crate::table::returnwire::{ReturnCert, ReturnVote, RETURN_CERT_CAP, RETURN_VOTE_CAP};

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
    /// lookup and completes nothing on its own.
    ///
    /// **And that is all it is.** This comment used to end by saying the seat
    /// is counted into `P(k+1)` by its own `HAND_INIT` and *"is required and
    /// dealable at `k+2` — one hand later"*. It is not: `next_hand` builds
    /// `required` by filtering `self.open.required` in **both** branches, so
    /// a seat outside `R` cannot re-enter by being heard from — measured over
    /// a whole run in `S1-BV`, where seven clients announced a readmission
    /// six times each and dealt nobody in. Widening the accepted set lets the
    /// seat *sign* stage 0. The door back into the roster is `S1-BM`'s return
    /// certificate (D-028): a seat outside `R(k)` with chips asks at a settled
    /// boundary, and `R(k+1)` gains it on a unanimous `RETURN_CERT` and by
    /// nothing else.
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
    /// `D-032`: how many times each seat has come back to the table by a
    /// return certificate (`S1-BM`), indexed by seat. Carried like `grace`,
    /// this receiver's own; at `MAX_RETURNS` this client votes for no further
    /// return of that seat, and a certificate needs every voter.
    pub returns: Vec<u8>,
    /// `D-047`: the seats out of the table for good -- certified absent with
    /// `MAX_RETURNS` returns behind them. Their chips left the table at the
    /// boundary that put them here, so every rule treats them as busted:
    /// not dealt in, no blinds, no return. Carried like `returns`.
    pub out: Vec<SeatIdx>,
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
            returns: vec![0; usize::from(ad.max_players)],
            out: Vec::new(),
            // The first hand of a table: nothing has decided the button yet.
            button: None,
        })
    }

    /// `S1-CR`: the opening of a hand this client did not derive, adopted from
    /// the members' own signed `HAND_INIT` copies -- a client that restarted,
    /// rejoined the group and holds no hand, or one that sat down at a table
    /// already playing.
    ///
    /// **What is trusted and what is checked.** Every copy is opened as a
    /// chain-`hand_id` stage-0 event of this table, signed by a key the roster
    /// holds; copies are grouped by `(parent, body bytes)` -- the parent of a
    /// stage-0 event is `GENESIS(k)` and the body is the derivation's whole
    /// output -- and the group with the most distinct seats is taken only when
    /// it holds a **strict majority of the occupied seats**, so a minority fork
    /// is never followed. The body's stacks must hash to the roster hash it
    /// names (§4.4 `n(10)`), and its blinds must be the schedule's for this
    /// hand, so a majority cannot name stacks or blinds the table does not
    /// have. What the group signed becomes the opening: the genesis, the
    /// stacks, the button, the level and blinds, and `required` -- the seats
    /// that signed, which is `R(k)` exactly when every required seat's copy is
    /// in the group, and the checkpoint at the end of the hand says whether it
    /// was (a different set is a different stage-0 hash, and a different
    /// checkpoint earns no return; the client adopts again at the next
    /// boundary). `readmitted`, `grace` and `present_run` are this receiver's
    /// own and start fresh.
    ///
    /// `base` carries what the copies do not: the table, the session, the
    /// roster's keys, this client's seat, and the advertised parameters.
    pub fn adopt(base: Opening, copies: &[Vec<u8>]) -> Result<Opening, Failed> {
        Self::adopt_with_signers(base, copies).map(|(o, _)| o)
    }

    /// [`adopt`](Self::adopt), and the seats whose copies carried it. The node
    /// tells a seat whose previous life signed the hand (D-033: taken up where
    /// it stood) from one the table deals in without a copy of its own (D-039:
    /// opened anew and said) by this, since both are in `required`.
    ///
    /// **`D-039`: the required set is the body's `dealt_in` joined with the
    /// signers.** `dealt_in` is the table's own word on who plays this hand,
    /// signed by a strict majority of the other seats, and `dealt_in ⊆ R(k)`
    /// (§4.4); the signers are in `R(k)` by having signed. So the union is a
    /// subset of `R(k)` that is exact whenever every required seat is dealt in,
    /// which is every hand this client can open (a required seat outside
    /// `dealt_in` is one with no chips or no grace, and nothing here emits a
    /// sit-out). Before this the set was the signers alone, so a seat the table
    /// was waiting for at stage 0 -- back from a restart, still required, its
    /// copy the one thing missing -- adopted as a bystander that could not sign,
    /// and at a table where half the seats had restarted together nobody could
    /// certify anybody, no stage 0 ever closed, and the hands aborted on their
    /// budget for the rest of the run (`run175510-4`).
    pub fn adopt_with_signers(base: Opening, copies: &[Vec<u8>]) -> Result<(Opening, Vec<SeatIdx>), Failed> {
        use std::collections::BTreeMap;
        let occupied: Vec<SeatIdx> = base.seats.iter().map(|(s, _, _)| *s).collect();
        // (parent, body bytes) -> (seats that signed exactly this, the body)
        let mut groups: BTreeMap<(Hash, Vec<u8>), (BTreeSet<SeatIdx>, HandInit)> = BTreeMap::new();
        for raw in copies {
            let Ok(opened) = chained::open_in_hand(
                raw,
                FRAME_CAP,
                EventType::HandInit,
                &base.table_id,
                base.hand_id,
            ) else {
                continue;
            };
            if opened.envelope.sequence != 0 {
                continue;
            }
            let Some(seat) = base
                .seats
                .iter()
                .find(|(_, k, _)| *k == opened.sender)
                .map(|(s, _, _)| *s)
            else {
                continue;
            };
            let Ok(body) = chained::payload::<HandInit>(&opened, HAND_INIT_CAP) else {
                continue;
            };
            if body.hand_id != base.hand_id || body.self_consistent(base.max_players).is_err() {
                continue;
            }
            let entry = groups
                .entry((opened.envelope.previous_event_hash, opened.envelope.payload.clone()))
                .or_insert_with(|| (BTreeSet::new(), body));
            entry.0.insert(seat);
        }
        let Some(((genesis, _), (signers, body))) = groups.into_iter().max_by_key(|(_, (s, _))| s.len())
        else {
            return Err(Failed::NotYet);
        };
        // A strict majority of the occupied seats **other than the adopter's**:
        // the adopter has signed nothing, so it is not a seat that could have
        // agreed. Heads-up that is the one other seat, which is what lets a
        // restarted client come back to a two-seat table at all (`S1-CX`).
        let others = occupied.iter().filter(|s| **s != base.my_seat).count();
        if signers.len() * 2 <= others {
            return Err(Failed::NotYet);
        }
        if body.stacks.len() != base.seats.len() {
            return Err(Failed::Elsewhere {
                seat: base.my_seat,
                what: "the copies carried one stack per occupied seat",
            });
        }
        let seats: Vec<(SeatIdx, [u8; 32], u64)> = base
            .seats
            .iter()
            .zip(body.stacks.iter())
            .map(|((s, k, _), stack)| (*s, *k, *stack))
            .collect();
        let roster: Vec<crate::protocol::transcript::RosterSeat> = seats
            .iter()
            .map(|(seat, key, stack)| crate::protocol::transcript::RosterSeat {
                seat: *seat,
                app_public_key: *key,
                stack_at_hand_start: *stack,
            })
            .collect();
        if crate::protocol::transcript::roster_hash(&roster) != body.roster_hash {
            return Err(Failed::Elsewhere {
                seat: base.my_seat,
                what: "the copies' stacks hashed to the roster hash they name",
            });
        }
        let small_blind = crate::poker::tournament::small_blind_at(
            u32::try_from(base.hand_id).unwrap_or(u32::MAX),
            u32::from(base.every_n_hands),
            base.first_small_blind,
            base.small_blind_cap,
        );
        if body.small_blind != small_blind || body.big_blind != small_blind.saturating_mul(2) {
            return Err(Failed::Elsewhere {
                seat: base.my_seat,
                what: "the copies' blinds were the schedule's for this hand",
            });
        }
        let signers: Vec<SeatIdx> = signers.into_iter().collect();
        let mut required: Vec<SeatIdx> = body.dealt_in.iter().copied().chain(signers.iter().copied()).collect();
        required.sort_unstable();
        required.dedup();
        let opening = Opening {
            genesis,
            required,
            readmitted: Vec::new(),
            seats,
            small_blind: body.small_blind,
            big_blind: body.big_blind,
            level: body.level,
            button: Some(body.button_position),
            roster_hash: body.roster_hash,
            grace: vec![GRACE_HANDS; usize::from(base.max_players)],
            present_run: vec![0; usize::from(base.max_players)],
            returns: vec![0; usize::from(base.max_players)],
            out: Vec::new(),
            ..base
        };
        Ok((opening, signers))
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

/// What a signed envelope costs around a payload of nothing.
///
/// `PROTOCOL.md` §9.2 accounts for it as *"frame − 128 B"* for the body and
/// *"body − 256 B"* for the payload, so 384 is the specification's own
/// allowance. Counting the wire gives **214**: `EventBody` is a twelve-element
/// array of 1 + 1 + 34 + 9 + 9 + 34 + 3 + 3 + 34 + 9 + 5 + 1 + 1 = 144 B, and
/// `SignedEvent` adds 1 + 3 + 2 + 64 = 70. The specification's number is used
/// rather than the counted one, with its 170 bytes of slack, because a gate
/// that refuses a **legal** frame is indistinguishable in the log from the loss
/// this whole row exists to end — and `src/net/relay.rs` already budgets 220
/// for the same quantity, so 214 is not comfortably above every encoder.
pub const ENVELOPE_MAX: usize = 384;

/// The largest a frame of this type may legitimately be, payload cap plus
/// envelope — **the ceiling a buffer should charge it, rather than
/// `FRAME_CAP`.**
///
/// **Why this exists.** Nothing between the wire and `Hand::early` or
/// `next_early` bounds a frame by its own type: `open_in_hand` is given
/// `FRAME_CAP`, `check_envelope` reads the version, the chain scope, the event
/// class and the unchained sentinels and never the payload's length, and the
/// per-type caps below are applied at **replay**, by `chained::payload`. So a
/// `DECK_COMMIT` — cap 160 — could occupy 16 384 bytes of a hold queue for a
/// whole boundary and be discarded minutes later. Thirty-two such slots are
/// half a megabyte, and a client holds four such queues plus a retained hand.
///
/// **The match is exhaustive with no wildcard, deliberately.** A type added to
/// the catalogue later must be classified here rather than admitted at
/// `FRAME_CAP` by an arm nobody revisited.
///
/// **Four types fall back to `FRAME_CAP` and it is stated rather than hidden:**
/// `RngCommit`, `RngReveal`, `Dispute` and `StateAck` publish no payload cap
/// anywhere in this tree. They are charged what they are charged today, which is
/// no worse, and the day one of them gets a cap this function is where it lands.
///
/// **The three `PLAYER_*` boundary types were the fifth and are not any more.**
/// They stood here because `S1-BZ` recorded that the state machine answered them
/// with `WrongType` and the window that would own them was not built. It is
/// built, so they are charged [`seatwire::BOUNDARY_EVENT_CAP`] — the largest of
/// the three bodies is a one-element array holding a `u16`.
pub fn frame_ceiling(kind: EventType) -> usize {
    let payload = match kind {
        EventType::HandInit => HAND_INIT_CAP,
        EventType::DeckInit => DECK_INIT_CAP,
        EventType::ShuffleStep => SHUFFLE_STEP_CAP,
        EventType::ShuffleProof => SHUFFLE_PROOF_CAP,
        EventType::DeckCommit => DECK_COMMIT_CAP,
        EventType::DealPrivate => DEAL_PRIVATE_CAP,
        EventType::BoardReveal => BOARD_REVEAL_CAP,
        EventType::ShowdownReveal => SHOWDOWN_REVEAL_CAP,
        EventType::ShowdownMuck => SHOWDOWN_MUCK_CAP,
        EventType::ActionCheck
        | EventType::ActionCall
        | EventType::ActionBet
        | EventType::ActionRaise
        | EventType::ActionFold => ACTION_CAP,
        EventType::TimeoutVote => TIMEOUT_VOTE_CAP,
        EventType::TimeoutCert => TIMEOUT_CERT_CAP,
        // `S1-BM`: the return pair, sealed in the boundary band.
        EventType::ReturnVote => crate::table::returnwire::RETURN_VOTE_CAP,
        EventType::ReturnCert => crate::table::returnwire::RETURN_CERT_CAP,
        EventType::HandComplete => HAND_COMPLETE_CAP,
        EventType::HandAbort => HAND_ABORT_CAP,
        EventType::StateHash => STATE_HASH_CAP,
        // §4.10's hand boundary window, whose bodies are one `u16` or nothing.
        EventType::PlayerSitOut | EventType::PlayerSitIn | EventType::PlayerLeave => {
            crate::table::seatwire::BOUNDARY_EVENT_CAP
        }
        // The four with no published cap, and the reason is in the doc above.
        EventType::RngCommit
        | EventType::RngReveal
        | EventType::Dispute
        | EventType::StateAck => return FRAME_CAP,
        // Not chained, so they never reach a hold queue; and an unknown type
        // is refused by `check_envelope` long before this.
        EventType::Hello
        | EventType::Capabilities
        | EventType::LobbyTableAd
        | EventType::LobbyTableRemove
        | EventType::LobbyPlayerPresence
        | EventType::LobbySnapshotRequest
        | EventType::LobbySnapshotResponse
        | EventType::LobbyChat
        | EventType::TableChat
        | EventType::TableLeave
        | EventType::TableHearing
        | EventType::TableContinues
        | EventType::SearchPresence
        | EventType::JoinRequest
        | EventType::JoinAccept
        | EventType::JoinReject
        | EventType::PlayerList
        | EventType::TableReady => return FRAME_CAP,
    };
    // Never above what the frame decoder itself would accept: this may only
    // ever refuse relative to today, never admit.
    payload.saturating_add(ENVELOPE_MAX).min(FRAME_CAP)
}

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

/// `D-050`: how long a hand that may muck waits for its player (the owner:
/// seven seconds is too long, three are enough).
pub const SHOW_WINDOW_MS: u64 = 3_000;

/// `D-050`: what a waiting hand leaves of the showdown stage's budget for the
/// seats behind it and the network.
pub const SHOW_MARGIN_MS: u64 = 8_000;

/// `D-050`: below this, a hand does not wait for its player: it mucks.
pub const SHOW_WINDOW_MIN_MS: u64 = 2_000;

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertFact {
    /// D-036: every seat the certificate names, ascending.
    pub subject_seats: Vec<SeatIdx>,
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

/// What one seat's whole run from the deal to the first bet costs, at its own
/// caps.
///
/// The stage layout is `DECK_INIT`, then the shuffle chain as a pair of stages
/// per dealt-in seat, then `DECK_COMMIT`, then `DEAL_PRIVATE` — so **five
/// frames from each seat** stand between a hand opening and its first bet, and
/// `PROTOCOL.md` §4.6's *"exactly `2(m−1)` entries in one body"* is what
/// forbids a sixth.
pub const PRE_BET_BYTES_PER_SEAT: usize = DECK_INIT_CAP
    + SHUFFLE_STEP_CAP
    + SHUFFLE_PROOF_CAP
    + DECK_COMMIT_CAP
    + DEAL_PRIVATE_CAP
    + 5 * ENVELOPE_MAX;

/// What the pre-open buffer guarantees **each seat**, in bytes.
///
/// One whole run to the first bet, plus one more frame of the largest type this
/// buffer can carry. The spare is what buys the betting frames that arrive
/// while a bystander is still opening, and it absorbs exactly one legally-sized
/// but junk arrival before that seat starts costing itself its own
/// `DEAL_PRIVATE`.
pub const NEXT_EARLY_BYTES_PER_SEAT: usize =
    PRE_BET_BYTES_PER_SEAT + SHUFFLE_PROOF_CAP + ENVELOPE_MAX;

/// The whole pre-open buffer's budget: the per-seat share times `MAX_SEATS`.
///
/// **The part and the whole are one rule counted twice**, which is the point:
/// no seat can ever be evicted on another seat's behalf, because the aggregate
/// is exactly the sum of the shares.
pub const NEXT_EARLY_BYTES: usize =
    (crate::protocol::constants::MAX_SEATS as usize) * NEXT_EARLY_BYTES_PER_SEAT;

/// How many events of a hand ahead of this one are held for the replay.
///
/// Sixty-four was written inline and is kept: it is four full collective stages
/// at nine seats, which is what a client one boundary behind has to swallow.
/// What changed with `S1-CD` is **which** entry goes when it is full — the
/// highest sequence, never the oldest arrival.
pub const EARLY_CAP: usize = 100;

/// And the bytes, because a slot without a ceiling is a `FRAME_CAP` slot.
///
/// Twice the pre-open buffer's budget: `begin_hand_with` pours both pre-open
/// queues through `hold` into this one, and a retained previous hand keeps a
/// second `early` alive until the running hand leaves stage 0 — which is
/// precisely the boundary, so the client pays for two of these at once. The
/// factor of two is the drain headroom `NEXT_EARLY_CAP`'s own comment intended,
/// written in the unit that binds.
pub const EARLY_BYTES: usize = 2 * NEXT_EARLY_BYTES;

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
    /// Whether `body` is this client's OWN settlement, carried across by
    /// `give_up` from a `Step::Settling` it was already in — as opposed to the
    /// first peer's, adopted because this client never settled.
    ///
    /// The two are closed differently and the difference is `S1-CL`: an own
    /// body is applied whatever the peers say, exactly as a peer that settled
    /// normally applies `mine.final_stacks`; a borrowed one is applied only if
    /// every peer said the same thing, because otherwise this client would be
    /// choosing a side by arrival order.
    own: bool,
    /// Whether any later body differed from `body`. Read only when `own` is
    /// false, where it is the one thing that stops the stage closing.
    disagreed: bool,
    /// Set once every seat of the required set has published, and — for a
    /// borrowed body — published the same thing. The second clause was in
    /// this sentence before it was in the code (`S1-CL`).
    closed: Option<(Hash, Vec<Chips>)>,
}

/// A `TIMEOUT_CERT` checked from its own bytes, with nothing taken on trust.
#[derive(Clone, Debug)]
struct VerifiedCert {
    event_hash: Hash,
    emitter: SeatIdx,
    subject: CertSubject,
    voters: BTreeSet<SeatIdx>,
    /// D-036: the votes it carries -- voter, what the vote says, its bytes --
    /// so a receiver that lacks one holds it from here.
    votes: Vec<(SeatIdx, TimeoutVote, Vec<u8>)>,
    /// `D-063`: the words of the players that left, by seat, checked.
    resignations: Vec<(SeatIdx, Vec<u8>)>,
    raw: Vec<u8>,
}

/// The most subject digests one hand may bank. Four per seat is well past
/// `S1-FM`: how much longer than its own budget the table's **first** hand's
/// opening waits for a seat before anybody votes about it (the owner,
/// 2026-09-15: *before the first hand, give a peer the time it takes to join
/// the table's Tox group -- measured, and not so long that a peer with a bad
/// line holds the table*).
///
/// Measured in the owner's own games across two networks, eight tables, the
/// far seat's opening sent this long after its table was set: 15.6, 19.4,
/// 22.6, 24.3, 30.1, 32.0, 42.0 and 73.7 s. Its client entered the group 0.5
/// to 12 s after the table was set; the rest is its view of the other seats
/// in the group completing, which is what `hand_one_may_open` waits for before
/// it opens a hand at all. The seats that were there certified it out at
/// 31.3 to 32 s -- the thirty-second budget -- four times in eight. A minute
/// more is ninety seconds: the slowest measured with a quarter-minute to spare,
/// and a table held a minute once, before its first hand, by a seat that never
/// comes. Every later stage has its seats already talking.
pub const FIRST_HAND_JOIN_ALLOWANCE_MS: u64 = 60_000;

/// `D-059`: how long a cryptographic step waits for a seat that has made the
/// table wait `waits` times: the step's budget less `PATIENCE_CUT_MS` a time, at
/// most `MAX_PATIENCE_CUTS` times, and never less than `WAIT_FROM_MS` -- nor
/// more than the budget itself, where that is shorter. Thirty seconds at the
/// table's step, then twenty, and ten from then on.
pub fn patience_ms(budget_ms: u64, waits: u8) -> u64 {
    use crate::protocol::constants::{MAX_PATIENCE_CUTS, PATIENCE_CUT_MS, WAIT_FROM_MS};
    let cut = u64::from(waits.min(MAX_PATIENCE_CUTS)).saturating_mul(PATIENCE_CUT_MS);
    budget_ms.saturating_sub(cut).max(WAIT_FROM_MS.min(budget_ms))
}

/// `D-059`: a stage standing on seats, as [`Hand::stall_now`] reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stall {
    /// The hand and the stage: a stall is one stage of one hand.
    pub key: (u64, u64),
    /// The seats it counts as the table waiting for already -- a
    /// cryptographic stage standing on them for `WAIT_FROM_MS`, a turn run past
    /// its deadline.
    pub waits: Vec<SeatIdx>,
    /// Whether a wait may count here at all: not while the table's first
    /// hand's opening holds for its seats (`S1-FM`), not below `D-036`'s floor.
    pub countable: bool,
}

/// `D-059`: this client's count of the times each seat made the table wait,
/// kept by the node for the table. A stall is watched while it stands and
/// counted once it is over -- the stage moved, the hand ended -- and not at all
/// if at any moment of it this client heard none of the other seats, or the
/// stall could not count: a client whose own line went is looking at its own
/// stall, and the library says so a minute late (the bed, `run181556-3`: the
/// seat whose line was cut counted the others while it heard nobody).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Waits {
    watching: Option<(u64, u64)>,
    seats: Vec<SeatIdx>,
    spoilt: bool,
    counts: Vec<u8>,
}

impl Waits {
    /// How many times each seat has made the table wait, indexed by seat.
    pub fn counts(&self) -> &[u8] {
        &self.counts
    }

    /// One tick: the stall now, if any, and whether this client hears none of
    /// the other seats. Returns the waits counted on this tick -- the stall
    /// just over -- each with its seat's new count.
    pub fn tick(&mut self, stall: Option<Stall>, deaf: bool) -> Vec<(SeatIdx, u8)> {
        let key = stall.as_ref().map(|s| s.key);
        let mut counted = Vec::new();
        if self.watching.is_some() && self.watching != key {
            let seats = std::mem::take(&mut self.seats);
            if !self.spoilt {
                for seat in seats {
                    let i = usize::from(seat);
                    if self.counts.len() <= i {
                        self.counts.resize(i + 1, 0);
                    }
                    self.counts[i] = self.counts[i].saturating_add(1);
                    counted.push((seat, self.counts[i]));
                }
            }
            self.watching = None;
            self.spoilt = false;
        }
        if let Some(st) = stall {
            self.watching = Some(st.key);
            self.spoilt |= deaf || !st.countable;
            for seat in st.waits {
                if !self.seats.contains(&seat) {
                    self.seats.push(seat);
                }
            }
        }
        counted
    }
}

/// `S1-FL`: a seat's place in the tournament, decided at a hand's boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatFinish {
    pub seat: SeatIdx,
    /// 1 is the winner.
    pub place: usize,
    /// Another seat shares the place: busted in the same hand with the same
    /// chips at its start.
    pub tied: bool,
    /// The seats still in the tournament after this boundary; 1 once it is
    /// won.
    pub players_left: usize,
}

/// `S1-FL`: the places a boundary decides (the owner, 2026-09-15: *the client
/// must know reliably where it finished, also when several seats bust in one
/// hand -- by the international tournament rules*).
///
/// The tournament rule: of the seats out of chips in one hand, the one that
/// began the hand with more chips finishes higher, and the same chips tie.
/// Ahead of them are the seats that play on (`R(k+1)`) and, while the
/// tournament goes on, the seats holding chips outside it -- absent, and free
/// to return. When the boundary ends the tournament (fewer than two play on),
/// the seat that plays on has won, the seats busted in that hand follow it,
/// and the absent seats holding chips come last, by their chips -- the seat
/// that stayed at the table to the end finishes ahead of one that left it
/// (`S1-FA`, the owner: busting heads-up beside an absent seat's chips is
/// second, not third). A seat out of the table for good busted at this
/// boundary, its chips leaving with it.
/// `S1-FL`: whether [`place_seats`] can still change at this boundary. A
/// return adds a seat holding chips to the seats that play on and nothing else,
/// so every place stands unless the tournament's end hangs on one: fewer than
/// two seats play on and a seat holding chips is outside them.
fn places_are_final(occupied: &[SeatIdx], end: &[Chips], playing_on: &[SeatIdx]) -> bool {
    let absent_with_chips = occupied
        .iter()
        .any(|s| end.get(usize::from(*s)).copied().unwrap_or(0) > 0 && !playing_on.contains(s));
    playing_on.len() >= 2 || !absent_with_chips
}

fn place_seats(occupied: &[SeatIdx], start: &[Chips], end: &[Chips], playing_on: &[SeatIdx]) -> (bool, Vec<SeatFinish>) {
    let at = |v: &[Chips], s: SeatIdx| v.get(usize::from(s)).copied().unwrap_or(0);
    let over = playing_on.len() < 2;
    let absent: Vec<SeatIdx> = occupied.iter().copied().filter(|s| at(end, *s) > 0 && !playing_on.contains(s)).collect();
    let busted: Vec<SeatIdx> = occupied.iter().copied().filter(|s| at(start, *s) > 0 && at(end, *s) == 0).collect();
    let players_left = if over { playing_on.len().min(1) } else { playing_on.len() + absent.len() };
    let ranked = |seats: &[SeatIdx], by: &[Chips], base: usize, out: &mut Vec<SeatFinish>| {
        for s in seats {
            let more = seats.iter().filter(|o| at(by, **o) > at(by, *s)).count();
            let tied = seats.iter().any(|o| *o != *s && at(by, *o) == at(by, *s));
            out.push(SeatFinish { seat: *s, place: base + more + 1, tied, players_left });
        }
    };
    let mut out = Vec::new();
    if over {
        for s in playing_on {
            out.push(SeatFinish { seat: *s, place: 1, tied: false, players_left });
        }
        if playing_on.is_empty() {
            // Nobody plays on: the seats holding chips rank by them.
            ranked(&absent, end, 0, &mut out);
            ranked(&busted, start, absent.len(), &mut out);
        } else {
            ranked(&busted, start, playing_on.len(), &mut out);
            ranked(&absent, end, playing_on.len() + busted.len(), &mut out);
        }
    } else {
        ranked(&busted, start, playing_on.len() + absent.len(), &mut out);
    }
    out.sort_by_key(|f| (f.place, f.seat));
    (over, out)
}

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
///
/// D-036: a certificate names every seat quiet at one stage and carries a
/// vote from every seat outside the set about each of them -- at its widest
/// four named by six, twenty-four whole signed votes, 12 459 B as measured
/// by `tests/timeout_certificate_at_a_full_table.rs`. `D-063`: with the seats
/// that resigned needing no majority, five named by five is twenty-five votes
/// and five signed words beside them. Under `FRAME_CAP` with the envelope
/// beside it.
pub const TIMEOUT_CERT_CAP: usize = 15_360;

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

/// One betting action as the hand applied it, for the window: who acted, what
/// they did, what went in with it, the seat's total for the round afterwards,
/// and whether it put the seat all in. `by_table`: a certificate acted for the
/// seat (`D-034`'s check or fold).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Acted {
    pub seat: SeatIdx,
    pub action: Action,
    pub put_in: Chips,
    pub total: Chips,
    pub all_in: bool,
    pub by_table: bool,
}

impl Acted {
    /// Read off the round right after the action was applied, with what the
    /// seat had committed before it.
    fn after(round: &BettingRound, seat: SeatIdx, action: Action, before: Chips, by_table: bool) -> Acted {
        let i = usize::from(seat);
        let total = round.committed.get(i).copied().unwrap_or(0);
        let behind = round.stack.get(i).copied().unwrap_or(0);
        Acted {
            seat,
            action,
            put_in: total.saturating_sub(before),
            total,
            all_in: behind == 0 && !matches!(action, Action::Fold | Action::Check),
            by_table,
        }
    }
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
    /// `D-058`: the same count for a seat's return -- the subject, votes held,
    /// votes needed -- taken by the node for the window's panel.
    return_tally: Option<(SeatIdx, usize, usize)>,
    /// What the carrier was holding **at the moment this client cast its own
    /// vote**: subject seat, the seat's `mid-delivery` bit, and whether the
    /// stage was already past twice its budget.
    ///
    /// **`S1-BB`'s discriminator, and it exists because the other sample is
    /// conditioned.** `vote_state` prints the same bit only for a client that
    /// has **not** voted — and one reason not to have voted is the lever that
    /// bit itself sets, so of 79 certifications with a reading nearby, 79 read
    /// `mid-delivery true` and the sample cannot say otherwise. Sampled here the
    /// condition is gone: every vote this client casts leaves a reading,
    /// whatever the bit says.
    ///
    /// **And the two values it can take are the two hypotheses.** The lever
    /// skips a seat whose bit is set, so a vote is cast either with the bit
    /// **clear** — the carrier had nothing of that seat's and the silence was
    /// real, which is the divergence reading — or with it **set and
    /// `long_past` true**, the bound releasing an accusation the carrier still
    /// contradicts, which is the tail-of-burst reading. Nothing else can appear
    /// here, and a third value would mean the lever is not doing what
    /// `vote_on_timeouts` says it does.
    ///
    /// A `Vec`, because one call votes about every seat the stage waits on.
    /// Drained by the node rather than read, like `tally` beside it.
    own_vote_carrier: Vec<(SeatIdx, bool, bool)>,
    /// `event_hash` of every `TIMEOUT_CERT` this client has verified, its own
    /// included. An abort that names a subject carries the hash of the
    /// certificate that justifies the naming (§4.10), and this is what lets a
    /// receiver check that claim against something it verified itself rather
    /// than take the emitter's word for it.
    certs: BTreeMap<Hash, CertFact>,
    /// Subject digests already applied to the roster, so a redelivery counts
    /// once and two genuine certifications of one seat count twice.
    banked: BTreeSet<Hash>,
    /// D-036: one strike per seat per stage, however many sets name it --
    /// the quiet set can grow and be sealed again about the same seat.
    struck: BTreeSet<(SeatIdx, u64)>,
    /// One fully verified certificate's bytes, for this client's own abort to
    /// carry as evidence. This client's own sealed copy wins where it is a
    /// voter — §4.10's *"it verified that certificate itself"* — and otherwise
    /// the first peer copy it checked.
    proof: Option<(Hash, Vec<u8>)>,
    /// `D-047`: the bytes of the certificate that named each seat this hand,
    /// by seat -- the table's word for the seat's own client. Kept at the
    /// bank, because a certificate never enters the transcript.
    words: BTreeMap<SeatIdx, Vec<u8>>,
    /// `D-051`: the seats this client cut off for flooding the table's carrier
    /// group, as the node says them. Its votes about them carry
    /// `CAUSE_FLOOD`. It only grows within a hand.
    flooders: BTreeSet<SeatIdx>,
    /// `S1-GK`: seats whose player left the table by its own signed word and
    /// whose client has left the table's group -- set by the node each tick
    /// ([`Hand::note_gone_by_their_word`]). Voted about at once: nobody waits
    /// out the clock of a player who said it is gone.
    gone_by_word: BTreeSet<SeatIdx>,
    /// `D-063`: the signed words of the players that left this table, by
    /// seat -- the node's, checked here ([`Hand::note_leave_words`]), and
    /// those a peer's certificate carried. A seat named with its word counts
    /// for nothing against the floor, and a certificate this client seals
    /// carries the words it holds for the seats it names.
    leave_words: BTreeMap<SeatIdx, Vec<u8>>,
    /// `D-063`: the seats a certificate named with their own word -- out of
    /// the table for good, not absent.
    resigned: BTreeSet<SeatIdx>,
    /// `S1-HA`: the seats the node reads as gone from the table's group -- out
    /// of it, or silent there for `QUIET_LIMIT_S` -- as of the last tick. A
    /// voter among them cannot complete the vote round this client is party
    /// to, so no certificate is reachable in this hand and the local abort
    /// waits for none.
    gone_from_group: BTreeSet<SeatIdx>,
    /// `D-051`: the seats this client has voted about at the stage now open,
    /// whatever the cause -- one vote about one seat at one stage, so a
    /// cause is fixed with the first.
    voted_about: BTreeSet<SeatIdx>,
    /// `D-051`: the seats a banked certificate named with `CAUSE_FLOOD`: every
    /// voter's client cut them off for flooding the group.
    flood_named: BTreeSet<SeatIdx>,
    /// `D-052`: the pots this hand settled into, in the settlement's own
    /// order -- the main pot first, then each side pot -- with what each held
    /// and which seats took it. Kept when the settlement is applied, because
    /// the step that carries the body goes with it (`Step::Ended`), and the
    /// window needs to say which pot a winner won.
    settled_pots: Vec<(Chips, Vec<SeatIdx>)>,
    /// `S1-ER`: what the settlement moved to each seat, by seat.
    ///
    /// Taken here, where the stacks before it and the stacks after it are both
    /// in hand, because this is the only place either is certain. The window
    /// used to subtract one reported state from another to learn it, and a
    /// state reported after the settlement -- which is what the node did in the
    /// call that ends the hand -- made every difference zero and left every
    /// winner unmarked.
    settled_gain: Vec<Chips>,
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
    /// `S1-CF`: a `HAND_INIT` refused on `dealt_in` **at a genesis this client
    /// agrees with**, said once per hand.
    ///
    /// `GENESIS(k+1)` commits `participants` and not `grace`, and `dealt_in` is
    /// derived from `grace` — so two peers can agree on the genesis and refuse
    /// each other's opening. Neither the fork detector nor §6.3's freeze can see
    /// it: the first compares genesis values, which match, and the second wants
    /// a checkpoint-8 `state_hash`, which a hand stalled at stage 0 never
    /// reaches. The stage simply never completes and the hand dies at
    /// `hand_deadline_ms` with nothing said.
    ///
    /// This does not repair it — the repair is a corpus decision, `S1-CF` — and
    /// it costs one comparison on a path that is already refusing the event.
    dealt_note: Option<String>,
    /// Whether `dealt_note` has ever been written this hand.
    ///
    /// **Separate from the note, because `take_dealt_note` empties the note.**
    /// Guarding on `dealt_note.is_none()` means *at most one PENDING*, not *at
    /// most one per hand* — and the difference is the whole point here: a
    /// stage 0 that will never complete is re-broadcast every five seconds for
    /// the life of the hand, and the disagreement check sits above the phase
    /// test, so every copy reaches it. The test caught this the moment it
    /// delivered a second copy.
    dealt_said: bool,
    /// Diagnostic: what the certificate path last decided.
    cert_note: Vec<String>,
    /// Whether the player has been told, this hand, that D-036's floor is
    /// what holds it: the seats that stopped are half the table or more.
    floor_said: bool,
    /// `D-034`: when the last event that could give somebody the turn was
    /// EMITTED, on its emitter's clock -- an action, a deal, a board -- and
    /// this client's own action's `now_ms`.
    last_stamp_ms: u64,
    /// `D-034`: when the seat now to act was given the turn, on the clock
    /// of the seat that gave it. The own clock and every window's countdown
    /// start here rather than at delivery, so a late delivery does not add
    /// to the thirty seconds.
    turn_began_unix_ms: u64,
    /// `S1-FS`: when this client applied the event that gave the seat now to
    /// act its turn, on this client's own clock.
    turn_heard_ms: u64,
    /// `S1-FS`: when this client took the hand up again after a restart
    /// (`D-033`), on its own clock; zero for a hand it did not restore.
    taken_up_ms: u64,
    /// Seat → the genesis its sequence-0 `HAND_INIT` of this hand named, when
    /// that is not this client's. A roster seat's signed word that it opened
    /// this hand elsewhere: the evidence a client on a private branch can
    /// get, read off the envelope `opened()` throws away as `NotYet`.
    foreign_genesis: BTreeMap<SeatIdx, Hash>,
    /// Said once per hand, when `FOREIGN_GENESIS_FLOOR` seats name one value.
    genesis_note: Option<String>,
    /// Whether it has been said: the note is taken by the node, and without
    /// this every re-sent copy re-armed it — one line every two seconds in
    /// `split124308-9`.
    genesis_said: bool,
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
    /// The subjects whose voter-set-shortfall HELD note has been said: the
    /// replay pass re-judges a held copy every two seconds, and the note
    /// used to come with it every time.
    shortfall_said: BTreeSet<Hash>,
    /// The last seat a certificate acted for, and what it did.
    ///
    /// Read once by the node so it can say so: an action nobody took is the one
    /// event at a table that has no author to attribute it to, and a player who
    /// saw a seat fold without folding deserves to know why.
    acted_for: Option<(SeatIdx, Action)>,
    /// Every betting action this hand applied, in order, whoever took it --
    /// the seat or a certificate for it. The node says each one to the window
    /// once: the badge beside the seat, the line in the table's log, the
    /// sound. A reading only: nothing the hand decides looks at it, and it is
    /// neither hashed nor sent.
    acted: Vec<Acted>,
    /// Votes heard about each subject, by the digest that identifies it.
    ///
    /// A vote alone is not evidence and does nothing; only a complete set —
    /// one from **every** seat in `V(subject)` — becomes a certificate. Kept as
    /// the signed bytes, because a certificate embeds them whole so that it
    /// carries its own proof and needs nothing from the receiver's store.
    votes: BTreeMap<Hash, BTreeMap<SeatIdx, Vec<u8>>>,
    /// Subjects this client has already voted about, so it votes once.
    voted: BTreeSet<Hash>,
    /// When this client cast its own vote at the stage now open (`S1-BT`).
    ///
    /// The round is given a stage budget of air measured from here rather than
    /// from the stage, because what has to fit in it is the other seats'
    /// copies of a vote this client has just sent, and they start when it is
    /// sent. Cleared with the rest of the stage's vote state in `mark_stage`.
    own_vote_at: Option<u64>,
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
    /// `D-059`: how many times each seat has made this table wait, by this
    /// client's count, indexed by seat -- set by the node, which keeps the
    /// counts from hand to hand ([`Hand::set_patience`]).
    patience: Vec<u8>,
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
    /// Whether stage 0 **completed** here, as opposed to merely ending.
    ///
    /// **`Phase` cannot answer this and that is not an oversight.** A hand that
    /// deals and then aborts is `Phase::Aborted` exactly like one that aborted
    /// at stage 0, so by the time anyone asks, `Init` is gone either way and
    /// the two are indistinguishable from the phase alone. The fact has to be
    /// remembered at the moment it becomes true, and there is exactly one such
    /// moment: the `Phase::Deck` assignment in `on_hand_init`, which is the
    /// only exit from `Init` that is not an abort.
    ///
    /// It is a latch and never clears: a hand is constructed once per hand id
    /// (`open_with` is the only site that writes `Phase::Init`), so nothing
    /// re-enters stage 0 in place.
    stage_zero_done: bool,
    /// `S1-BM`: `IN(k)`, the subjects of complete return certificates banked
    /// at this boundary, ascending.
    returned: Vec<SeatIdx>,
    /// Return votes by subject digest, by voter.
    return_votes: BTreeMap<Hash, BTreeMap<SeatIdx, Vec<u8>>>,
    /// The subject each return digest is about.
    return_subjects: BTreeMap<Hash, ReturnVote>,
    /// Return subjects this client has voted about.
    return_voted: BTreeSet<Hash>,
    /// Return subjects this client has banked.
    return_banked: BTreeSet<Hash>,
    /// Return subjects this client has sealed its own certificate for.
    return_sealed: BTreeSet<Hash>,
    /// The evidence this client voted on, kept to seal with.
    return_evidence: BTreeMap<Hash, (Vec<u8>, Vec<u8>)>,
    /// The certificate stages open at this boundary, by subject digest.
    returning: BTreeMap<Hash, Collective>,
    /// Whether this client has asked to sit in at this boundary.
    sit_in_asked: bool,
    /// `D-032`: the seats whose return this client refused this hand, so the
    /// refusal is said once.
    return_refused: BTreeSet<SeatIdx>,
    /// `D-033`: this client re-entered a hand its previous process was a member
    /// of. While set, this seat's own contributions come from the wire -- the
    /// frames its previous process signed, said again by the table -- rather
    /// than being made here; [`Hand::restore_done`] ends it and makes whatever
    /// the current stage still wants from this seat.
    restoring: bool,
    /// `D-033`: the previous process's deck secret, from the session record,
    /// until the deck stage takes it.
    restored_secret: Option<HandSecret>,
    /// `D-033`: no secret for this hand (none kept, or not this hand's): the
    /// hand can be followed and folded, not played out -- no share of this
    /// seat's is ever made, its cards are not read, and at a showdown it mucks.
    fold_only: bool,
    /// `D-050`: whether this client's own hand, where the rules let it muck at
    /// the showdown, waits for its player -- who may show it instead -- rather
    /// than mucking at once. Set by the node for a client with a player.
    hold_muck: bool,
    /// `D-050`: when this client saw the showdown stage open, on its clock.
    showdown_opened_ms: Option<u64>,
    /// `D-050`: the hand is waiting for its player until then, on the same clock.
    muck_held_until_ms: Option<u64>,
    /// `D-033`: every frame this hand accepted from the wire, in the order
    /// accepted, for a seat back from a restart to take the hand up from --
    /// said again by the node beside its own frames.
    transcript: Vec<Vec<u8>>,
    transcript_seen: BTreeSet<Hash>,
    params: std::sync::Arc<DeckParams>,
    /// Events for a stage this client has not reached. Held rather than
    /// refused, because GossipSub does not order two messages and a peer that
    /// is one step ahead is not a peer that is wrong. The same reason
    /// `Formation` keeps one.
    early: VecDeque<Vec<u8>>,
    /// Seats **this client has removed** that signed this hand at a genesis
    /// this client does not hold.
    ///
    /// **The one case a certificate cannot repair, and it was being discarded.**
    /// A certificate only removes, so a divergence is mendable only while this
    /// client's roster is a superset of the table's. A seat outside this
    /// client's `required`, signing this hand at the genesis the table is
    /// using, is proof of the opposite: this client shrank further than the
    /// table did, and `D-024`'s monotone roster forbids the way back. `S1-CE`.
    foreign_outside: std::collections::BTreeSet<SeatIdx>,
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
        Self::open_inner(o, key, now_ms, next_deadline_ms, voice, None)
    }

    /// `D-033`: open a hand whose stage 0 this seat signed in its previous
    /// life. No new opening is made or heard: the old one, said again by the
    /// table, arrives like anybody's; so does every later frame of this
    /// seat's. `kept` is the deck secret from the session record, if the record
    /// names this hand -- without it the hand can be followed and folded, not
    /// played out. The caller replays the table's frames, then calls
    /// [`Hand::restore_done`].
    pub fn open_restoring(
        o: Opening,
        key: &SigningKey,
        now_ms: u64,
        next_deadline_ms: u32,
        kept: Option<&[u8; 32]>,
    ) -> Result<Hand, Failed> {
        let secret = kept.and_then(HandSecret::kept);
        Self::open_inner(o, key, now_ms, next_deadline_ms, Voice::Speak, Some(secret)).map(|(h, _)| h)
    }

    fn open_inner(
        o: Opening,
        key: &SigningKey,
        now_ms: u64,
        next_deadline_ms: u32,
        voice: Voice,
        restore: Option<Option<HandSecret>>,
    ) -> Result<(Hand, Vec<Send>), Failed> {
        let restoring = restore.is_some();
        let fold_only = matches!(restore, Some(None));
        let restored_secret = restore.flatten();
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
        // stage without counting towards its completion. Gating on `required`
        // instead shut that door on the seat it was built for — caught by
        // `a_readmitted_seat_is_accepted_at_stage_zero_and_never_required`.
        //
        // **What emitting here does NOT do is earn the seat its place back.**
        // This comment used to say it "is required again at `k+2`". `next_hand`
        // filters `self.open.required` in both branches, so the roster only
        // ever shrinks (`S1-BV`, `D-024`); signing stage 0 puts the seat in
        // `P(k)` and no further. `S1-BM`'s return certificate is the door, and
        // it is not built.
        let a_member = accepted.contains(&o.my_seat);
        let mut signed = vec![false; usize::from(o.max_players)];
        // `D-033`: a restored member's opening is the one its previous life
        // signed, and it comes from the wire.
        if a_member && !restoring {
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
        let (sends, own_init) = match (a_member && !restoring, voice) {
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
                return_tally: None,
                own_vote_carrier: Vec::new(),
                certs: BTreeMap::new(),
                struck: BTreeSet::new(),
                banked: BTreeSet::new(),
                proof: None,
                words: BTreeMap::new(),
                flooders: BTreeSet::new(),
                gone_by_word: BTreeSet::new(),
                leave_words: BTreeMap::new(),
                resigned: BTreeSet::new(),
                gone_from_group: BTreeSet::new(),
                voted_about: BTreeSet::new(),
                flood_named: BTreeSet::new(),
                settled_pots: Vec::new(),
                settled_gain: Vec::new(),
                forked: None,
                late: None,
                bank_left_ms: o.time_bank_ms,
                shuffle_note: None,
                settle_note: None,
                dealt_note: None,
                dealt_said: false,
                cert_note: Vec::new(),
                floor_said: false,
                last_stamp_ms: opened_at_ms,
                turn_began_unix_ms: opened_at_ms,
                turn_heard_ms: opened_at_ms,
                taken_up_ms: 0,
                foreign_genesis: BTreeMap::new(),
                genesis_note: None,
                genesis_said: false,
                late_roster: false,
            returned: Vec::new(),
            return_votes: BTreeMap::new(),
            return_subjects: BTreeMap::new(),
            return_voted: BTreeSet::new(),
            return_banked: BTreeSet::new(),
            return_sealed: BTreeSet::new(),
            return_evidence: BTreeMap::new(),
            returning: BTreeMap::new(),
            sit_in_asked: false,
            return_refused: BTreeSet::new(),
                restoring,
                restored_secret,
                fold_only,
                hold_muck: false,
                showdown_opened_ms: None,
                muck_held_until_ms: None,
                transcript: Vec::new(),
                transcript_seen: BTreeSet::new(),
                voice,
                own_init,
                abort_hold_said: None,
                shortfall_said: BTreeSet::new(),
                acted_for: None,
                acted: Vec::new(),
                votes: BTreeMap::new(),
                voted: BTreeSet::new(),
                own_vote_at: None,
                certifying: None,
                certified: Vec::new(),
                strikes: vec![0; usize::from(o.max_players)],
                patience: Vec::new(),
                opened_at_ms,
                stage_at_ms: opened_at_ms,
                stage_seq: 0,
                open: o,
                mine,
                slot,
                phase: Phase::Init(stage),
                stage_zero_done: false,
                params: DeckParams::new(),
                early: VecDeque::new(),
                foreign_outside: std::collections::BTreeSet::new(),
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
                | EventType::ReturnVote
                | EventType::ReturnCert
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
        // `S1-BM`: the return pair lives in the boundary band, above every stage
        // of the hand, so it is answered before the sequence gate below.
        if kind == EventType::ReturnCert {
            return self.on_return_cert(bytes, key, now_ms);
        }
        if kind == EventType::ReturnVote {
            return self.on_return_vote(bytes, key, now_ms);
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
        // `D-033`: kept once, for a seat back from a restart -- keyed by the
        // frame's own bytes: the slot has moved on by now, and a frame opened
        // against it reads as a stage this hand has left (`run105747-2`, where
        // the survivor said only its own frames again).
        if out.is_ok() {
            let digest: Hash = *blake3::hash(bytes).as_bytes();
            if self.transcript_seen.insert(digest) {
                self.transcript.push(bytes.to_vec());
            }
        }
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
        if let Err(what) = self.mine.disagreement(&theirs) {
            // **`S1-CF`, and the guard below is a tautology today — which is
            // the finding rather than a flaw in the guard.** `opened()` runs
            // `chained::open` against `self.slot`, so a `HAND_INIT` chained
            // from any other parent comes back `Failed::NotYet` long before
            // this line. **Every `dealt_in` disagreement a client can ever
            // reach here is therefore one at its own genesis** — two peers who
            // agree about the hand and disagree about its players — and that is
            // exactly the shape no genesis-based detector can see. The
            // comparison is kept because it states the precondition the note
            // asserts, costs one equality on a path that is already refusing
            // the event, and stops being a tautology the day `opened()`'s
            // contract changes.
            if what == NotOurs::Differs(Field::DealtIn)
                && opened.envelope.previous_event_hash == self.open.genesis
                && !self.dealt_said
            {
                self.dealt_said = true;
                let short = |h: &Hash| -> String {
                    h.iter().take(4).map(|b| format!("{b:02x}")).collect()
                };
                self.dealt_note = Some(format!(
                    "seat {seat} opened hand #{} at THIS client's own genesis {} and named a \
                     different dealt_in: {:?} against this client's {:?}. The genesis commits \
                     the roster and not `grace`, so nothing downstream can see this: stage 0 \
                     will not complete and the hand will die at its deadline (S1-CF)",
                    self.open.hand_id,
                    short(&self.open.genesis),
                    theirs.dealt_in,
                    self.mine.dealt_in
                ));
            }
            return Err(Failed::Disagrees { seat, what });
        }

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
        // `D-033`: restoring, this seat's key comes from the wire and the secret
        // from the session record; the stage opens with this seat's slot open.
        // A fresh secret nothing is encrypted to stands in when none was kept.
        if self.restoring {
            let (fresh, _, _) = self.params.keygen(&ctx);
            let secret = match self.restored_secret.take() {
                Some(s) => s,
                None => {
                    self.fold_only = true;
                    fresh
                }
            };
            let stage = Collective::closed(
                self.slot.sequence,
                EventType::DeckInit.code(),
                &self.mine.dealt_in,
            )
            .ok_or(Failed::NotInThisStage)?;
            self.stage_zero_done = true;
            self.phase = Phase::Deck {
                stage,
                by_seat: vec![None; usize::from(self.open.max_players)],
                keys: Vec::new(),
                secret,
            };
            return Ok(Vec::new());
        }
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
        // **The one exit from stage 0 that is not an abort.** `S1-CG`.
        self.stage_zero_done = true;
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
        // `D-033`: this seat's own key, said again by the table, must be the
        // one the kept secret answers for; otherwise the secret is another
        // hand's and this one can only be followed and folded.
        if seat == self.open.my_seat && self.restoring && !self.fold_only {
            if let Phase::Deck { secret, .. } = &self.phase {
                if secret.wire_key().encode() != wire_key.encode() {
                    self.fold_only = true;
                }
            }
        }
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
        if self.restoring || chain.whose_turn() != Some(self.open.my_seat) {
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
        let stage = Collective::closed(
            self.slot.sequence,
            EventType::DeckCommit.code(),
            &self.mine.dealt_in,
        )
        .ok_or(Failed::NotInThisStage)?;
        self.phase = Phase::Committing {
            deal,
            table: Table {
                deck: Box::new(final_deck),
                map,
            },
            stage,
            mine,
        };
        // `D-033`: restoring, this seat's commitment comes from the wire.
        if self.restoring {
            return Ok(Vec::new());
        }
        self.commit_mine(key, now_ms)
    }

    /// This seat's commitment to the final deck, said and heard; the stage
    /// closes here if this was the last one owed.
    fn commit_mine(&mut self, key: &SigningKey, now_ms: u64) -> Result<Vec<Send>, Failed> {
        let me = self.open.my_seat;
        let mine = match &self.phase {
            Phase::Committing { stage, mine, .. } if stage.heard(me).is_none() => mine.clone(),
            _ => return Ok(Vec::new()),
        };
        let bytes = self.say(EventType::DeckCommit, &mine, DECK_COMMIT_CAP, key, now_ms)?;
        let hash = self.opened(&bytes, EventType::DeckCommit)?.event_hash;
        let complete = {
            let Phase::Committing { stage, .. } = &mut self.phase else {
                return Err(Failed::NothingFurther);
            };
            stage.hear(me, hash);
            stage.complete()
        };
        let mut out = vec![Send::Broadcast(bytes)];
        if complete {
            let parent = {
                let Phase::Committing { stage, .. } = &self.phase else {
                    return Err(Failed::NothingFurther);
                };
                stage.hash().expect("a complete stage has one")
            };
            self.slot = self.slot.then(parent);
            out.append(&mut self.begin_dealing(key, now_ms)?);
        }
        Ok(out)
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

        let stage = Collective::closed(
            self.slot.sequence,
            EventType::DealPrivate.code(),
            &self.mine.dealt_in,
        )
        .ok_or(Failed::NotInThisStage)?;
        let mut dealing = Dealing::new(table.map.clone(), self.mine.dealt_in.clone());
        if self.restoring {
            // `D-033`: this seat's shares for the others' cards come from the
            // wire; the shares for its own two cards it makes here, with the kept
            // secret, so the cards can be read when the stage closes.
            if !self.fold_only {
                let mine_indices = table.map.hole_cards(me).ok_or(Failed::NotInThisStage)?;
                let my_key = deal.key_of(me).ok_or(Failed::NotInThisStage)?;
                let ctx = self.deck_ctx(&self.open.seats[self.seat_index()].1);
                let identity = Identity {
                    seat: me,
                    key: my_key,
                    secret: &deal.secret,
                };
                for index in mine_indices.iter().copied() {
                    dealing
                        .own_share(&deal.as_ref(&table), &identity, index, &ctx)
                        .map_err(|e| refused(me, e))?;
                }
            }
            self.phase = Phase::Dealing {
                deal,
                table,
                stage,
                dealing: Box::new(dealing),
            };
            return Ok(Vec::new());
        }
        self.phase = Phase::Dealing {
            deal,
            table,
            stage,
            dealing: Box::new(dealing),
        };
        self.deal_mine(key, now_ms)
    }

    /// This seat's shares for every hole card -- the others' sent, its own
    /// kept -- said and heard; the stage closes here if this was the last one
    /// owed. A share already recorded (the own cards, after a restore) is not
    /// made twice; a hand with no secret makes none.
    fn deal_mine(&mut self, key: &SigningKey, now_ms: u64) -> Result<Vec<Send>, Failed> {
        let me = self.open.my_seat;
        if self.fold_only {
            return Ok(Vec::new());
        }
        let ctx = self.deck_ctx(&self.open.seats[self.seat_index()].1);
        let entries = {
            let Phase::Dealing {
                deal,
                table,
                stage,
                dealing,
            } = &mut self.phase
            else {
                return Err(Failed::NothingFurther);
            };
            if stage.heard(me).is_some() {
                return Ok(Vec::new());
            }
            let mine_indices = table.map.hole_cards(me).ok_or(Failed::NotInThisStage)?;
            let my_key = deal.key_of(me).ok_or(Failed::NotInThisStage)?;
            let identity = Identity {
                seat: me,
                key: my_key,
                secret: &deal.secret,
            };
            let mut entries = Vec::new();
            for index in every_hole_index(&table.map) {
                if dealing.outstanding(index).is_some_and(|o| !o.contains(&me)) {
                    continue;
                }
                let (token, proof) = dealing
                    .own_share(&deal.as_ref(table), &identity, index, &ctx)
                    .map_err(|e| refused(me, e))?;
                if !mine_indices.iter().any(|i| i.get() == index.get()) {
                    entries.push(RevealEntry {
                        deck_index: index.get(),
                        token: token.encode(),
                        proof: proof.encode(),
                    });
                }
            }
            entries
        };
        let body = DealPrivate { entries };
        let bytes = self.say(EventType::DealPrivate, &body, DEAL_PRIVATE_CAP, key, now_ms)?;
        let hash = self.opened(&bytes, EventType::DealPrivate)?.event_hash;
        let complete = {
            let Phase::Dealing { stage, .. } = &mut self.phase else {
                return Err(Failed::NothingFurther);
            };
            stage.hear(me, hash);
            stage.complete()
        };
        let mut out = vec![Send::Broadcast(bytes)];
        if complete {
            let parent = {
                let Phase::Dealing { stage, .. } = &self.phase else {
                    return Err(Failed::NothingFurther);
                };
                stage.hash().expect("a complete stage has one")
            };
            self.slot = self.slot.then(parent);
            out.append(&mut self.read_my_cards(key, now_ms)?);
        }
        Ok(out)
    }

    fn on_deal_private(
        &mut self,
        bytes: &[u8],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, EventType::DealPrivate)?;
        // `D-034`: the deal that completes gives pre-flop's first turn.
        self.last_stamp_ms = opened.envelope.emitted_at_unix_ms;
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
        // `D-033`: a hand with no secret reads no cards; it can only fold.
        let cards: Option<[Card; 2]> = if self.mine.dealt_in.contains(&me) && !self.fold_only {
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
                let began = self.last_stamp_ms;
                let Phase::Playing { play, .. } = &mut self.phase else {
                    return Err(Failed::NothingFurther);
                };
                play.step = Step::Acting { to_act };
                self.turn_began_unix_ms = began;
                self.turn_heard_ms = now_ms;
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
        // `D-034`: this client's own action gives the next seat its turn now.
        self.last_stamp_ms = now_ms;
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
        // `D-034`: the turn this action gives begins when the action was made.
        self.last_stamp_ms = opened.envelope.emitted_at_unix_ms;
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
            let before = play.round.committed.get(usize::from(seat)).copied().unwrap_or(0);
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
            self.acted.push(Acted::after(&play.round, seat, action, before, false));
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
                let began = self.last_stamp_ms;
                let Phase::Playing { play, .. } = &mut self.phase else {
                    return Err(Failed::NothingFurther);
                };
                play.step = Step::Acting { to_act };
                self.turn_began_unix_ms = began;
                self.turn_heard_ms = now_ms;
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

        {
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
        }
        // `D-033`: restoring, this seat's shares for the board come from the wire.
        if self.restoring {
            return Ok(Vec::new());
        }
        self.board_mine(key, now_ms)
    }

    /// This seat's shares for the street being opened, said and heard; the
    /// street opens here if this was the last share owed. A hand with no secret
    /// makes none, and says so once.
    fn board_mine(&mut self, key: &SigningKey, now_ms: u64) -> Result<Vec<Send>, Failed> {
        let me = self.open.my_seat;
        let street = match &self.phase {
            Phase::Playing { play, .. } => match &play.step {
                Step::Opening { street, stage } if stage.heard(me).is_none() => *street,
                _ => return Ok(Vec::new()),
            },
            _ => return Ok(Vec::new()),
        };
        if self.fold_only {
            let line = "this seat has no secret for this hand (D-033), so it cannot help open the board; the table waits".to_string();
            if !self.cert_note.contains(&line) {
                self.cert_note.push(line);
            }
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
        let complete = {
            let Phase::Playing { play, .. } = &mut self.phase else {
                return Err(Failed::NothingFurther);
            };
            let Step::Opening { stage, .. } = &mut play.step else {
                return Err(Failed::NothingFurther);
            };
            stage.hear(me, hash);
            stage.complete()
        };
        let mut out = vec![Send::Broadcast(bytes)];
        if complete {
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
            out.append(&mut self.open_betting(street, key, now_ms)?);
        }
        Ok(out)
    }

    fn on_board_reveal(
        &mut self,
        bytes: &[u8],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, EventType::BoardReveal)?;
        // `D-034`: a street that opens on this board gives its first turn now.
        self.last_stamp_ms = opened.envelope.emitted_at_unix_ms;
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
        self.showdown_opened_ms.get_or_insert(now_ms);
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
        // `D-033`: restoring, what this seat said at the showdown comes from the wire.
        if self.restoring {
            return Ok(Vec::new());
        }
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

        // `D-033`: a hand with no secret can show nothing, so it mucks.
        if !self.fold_only && self.shows_rather_than_mucks(my_place, first)? {
            self.show(key, now_ms)
        } else if self.muck_held_until_ms.is_some() {
            // `D-050`: already waiting for its player.
            Ok(Vec::new())
        } else if let Some(until) = self.hold_until(now_ms) {
            // `D-050`, the owner: a hand that may muck waits for its player,
            // who may show it instead (`show_held`); the node mucks it when the
            // window is up (`muck_held_now`).
            self.muck_held_until_ms = Some(until);
            Ok(Vec::new())
        } else {
            self.muck(key, now_ms)
        }
    }

    /// `D-050`: until when this client's hand may wait at the showdown for
    /// its player, or `None` when it mucks at once: nobody holds it, it has no
    /// cards to show, or too little of the stage's budget is left. The window
    /// is `SHOW_WINDOW_MS`, cut to what the stage leaves after
    /// `SHOW_MARGIN_MS`, since the seats behind this one in the order wait on
    /// it inside the same budget.
    fn hold_until(&self, now_ms: u64) -> Option<u64> {
        if !self.hold_muck || self.fold_only || self.restoring || self.cards().is_none() {
            return None;
        }
        let opened = self.showdown_opened_ms.unwrap_or(now_ms);
        let spent = now_ms.saturating_sub(opened);
        let left = u64::from(self.open.crypto_step_timeout_ms).saturating_sub(spent).saturating_sub(SHOW_MARGIN_MS);
        let window = SHOW_WINDOW_MS.min(left);
        (window >= SHOW_WINDOW_MIN_MS).then(|| now_ms.saturating_add(window))
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
        //
        // `S1-ER`: and what each seat gained is read off here, where both sides
        // of it are in hand -- the stack about to be overwritten and the one
        // going in.
        let mut gain: Vec<Chips> = Vec::with_capacity(mine.final_stacks.len());
        for (seat, end) in mine.final_stacks.iter().enumerate() {
            if let Some(slot) = play.round.stack.get_mut(seat) {
                gain.push(end.saturating_sub(*slot));
                *slot = *end;
            } else {
                gain.push(0);
            }
        }
        // **Held before the step that computed it goes.** `mine.state_hash` is
        // §6.1's hash of the settled state and `parent` is `TERMINAL(k)`; the
        // next line drops the `Step` that carries the first, and the slot moves
        // past the second on the following hand. See the field's own note.
        self.checkpoint8 = Some((diverge_if_asked(mine.state_hash, self.open.hand_id), parent));
        // `D-052`: and the pots, for the same reason -- the body goes with the
        // step, and the window says which pot each winner won.
        let pots: Vec<(Chips, Vec<SeatIdx>)> =
            mine.pots.iter().map(|a| (a.size, a.winners.clone())).collect();
        play.step = Step::Ended;
        self.settled_pots = pots;
        self.settled_gain = gain;
        Ok(Vec::new())
    }

    /// The boundary checkpoint's value and the stage it chains from, once the
    /// hand has settled. `None` before that, and on the aborted path, which
    /// §6.2 row 8 also places a checkpoint on and which is not built yet.
    pub fn checkpoint8(&self) -> Option<(Hash, Hash)> {
        self.checkpoint8
    }

    /// `TERMINAL(k)` — **on both terminal paths**, which is the whole reason
    /// this exists beside [`Hand::checkpoint8`].
    ///
    /// §3.1: the terminal is the `HAND_COMPLETE` stage hash on the decided path
    /// and `ABORT_TERMINAL(k)` on the abort path, *"and both are agreed by every
    /// peer by construction"*. `checkpoint8` answers only the first, because
    /// §6.2 row 8's aborted-path checkpoint is `S1-R` and is not built — and
    /// **a caller that needed the terminal and reached for `checkpoint8` got
    /// nothing after an abort.** §4.10's hand boundary window was opened
    /// through exactly that call, so it existed only on the settled path
    /// (`S1-BZ`): the window whose one purpose is a seat's route back was
    /// missing on the path a seat goes quiet on.
    ///
    /// This asks the question §3.1 asks and nothing more. It does not build the
    /// aborted checkpoint and does not touch `Q-10`: `ABORT_TERMINAL(k)` is a
    /// function of `GENESIS(k)` alone, so it needs no stage and no agreement
    /// about the middle of the hand.
    ///
    /// **Three arms, not two, and the third is the one that bites.**
    /// `checkpoint8` is written only inside `close_settlement_if_done`, which
    /// `give_up` makes unreachable by replacing the phase — so a peer that gave
    /// up and *then* closed a **late** settlement holds `TERMINAL(k)` = the
    /// `HAND_COMPLETE` stage hash with `checkpoint8` still `None`. A two-armed
    /// version fell through to the aborted arm and answered
    /// `ABORT_TERMINAL(k)`, which is not that peer's terminal at all: §4.10 says
    /// `HAND_COMPLETE` wins over an abort. `S1-BM` banked the same correction in
    /// words — *"the settled-path condition must be the chain fact `TERMINAL(k)`
    /// is the `HAND_COMPLETE` stage hash and not `checkpoint8.is_some()`, which
    /// comes apart on §4.10's abort-settle race"* — and this is that sentence in
    /// code.
    ///
    /// **`next_hand` reads this rather than repeating it**, so the terminal the
    /// window opens at and the terminal the next hand chains from cannot come
    /// apart. They did for twenty minutes, which is how this comment came to be
    /// written.
    ///
    /// `None` while the hand is still live, on every path.
    pub fn terminal(&self) -> Option<Hash> {
        match &self.phase {
            // The settlement closed here: `TERMINAL(k)` is the `HAND_COMPLETE`
            // stage hash, which is the slot's parent now that the stage closed.
            Phase::Playing { play, .. } if matches!(play.step, Step::Ended) => {
                Some(self.slot.previous_event_hash)
            }
            // A settlement that arrived after this client gave up. §4.10:
            // `HAND_COMPLETE` wins over an abort.
            Phase::Aborted(_) if self.late.as_ref().is_some_and(|l| l.closed.is_some()) => {
                self.late.as_ref().and_then(|l| l.closed.as_ref()).map(|(h, _)| *h)
            }
            // A function of `GENESIS(k)` and nothing else, which is exactly why
            // an abort's terminal cannot fork.
            Phase::Aborted(_) => Some(crate::protocol::transcript::abort_terminal(
                &self.open.table_id,
                self.open.hand_id,
                &self.open.genesis,
            )),
            _ => None,
        }
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
                // **Both sides of the rule move together (`S1-BT`).** The
                // emitter's gate is `may_abandon` and this is the receiver's;
                // if only one of them waited, the first seat to reach its own
                // bound would still end the hand at every other seat and the
                // round would die exactly as before. `party_to_vote` rather
                // than `vote_joined` for the same reason: a seat that owes a
                // vote it has not cast is still a party to the round.
                let admitted = hand_budget_spent
                    || (self.long_past_stage(now_ms)
                        && (!self.crypto_stage() || self.round_has_had_air(now_ms)))
                    || (self.past_deadline(now_ms) && !self.party_to_vote());
                if !admitted {
                    if self.past_deadline(now_ms) && self.abort_hold_said != Some(self.slot.sequence) {
                        self.abort_hold_said = Some(self.slot.sequence);
                        // The bound the note counts down to is the one the
                        // gate above actually uses: three budgets, or one
                        // budget after this client's own vote, whichever comes
                        // first (`S1-BT`).
                        let allowed = self
                            .owed_type()
                            .map(|o| {
                                let b = u64::from(self.next_deadline_for(o));
                                let ceiling = b.saturating_mul(3);
                                match self.own_vote_at {
                                    Some(v) => ceiling.min(
                                        v.saturating_sub(self.stage_at_ms).saturating_add(b),
                                    ),
                                    None => ceiling,
                                }
                            })
                            .unwrap_or(0);
                        let age = now_ms.saturating_sub(self.stage_at_ms);
                        self.cert_note.push(format!(
                            "abort from seat {seat} held: a vote round this client is party to about seat(s) {:?} is open at stage {}, {} s to the end of the round",
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
                let Some(fact) = self.certs.get(&h).cloned() else {
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
                // D-036: every seat the certificate names, in seat order.
                let names = self.keys_of(&fact.subject_seats);
                if body.attributed != names {
                    return Err(Failed::Elsewhere {
                        seat,
                        what: "the seats named were the ones its certificate is about",
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
        let budget = self.stage_budget_ms();
        if stage >= u64::from(budget) {
            return Some(format!(
                "this stage's budget: {} s of {} s{who} (the hand itself has used                  {} s of {} s)",
                stage / 1_000,
                budget / 1_000,
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
        // `S1-CX`: heads-up, a hand both seats have signed keeps the longer
        // budget -- nobody can vote at two seats, so this budget's only effect
        // is a unilateral give-up, and the carrier repairs a brief outage by
        // itself within `CARRIER_GIVES_UP_MS`. A hand the other seat never
        // signed (stage 0) keeps the ordinary one: that is the hand a returning
        // seat is waited for in, and the two-seat rule's whole road.
        now_ms.saturating_sub(self.stage_at_ms) >= u64::from(self.stage_budget_ms())
    }

    /// The budget of the current cryptographic stage: `crypto_step_timeout_ms`,
    /// or heads-up for a hand both seats have signed, `HEADS_UP_STAGE_BUDGET_MS`
    /// (`S1-CX`). One place, so the clock and its report agree.
    fn stage_budget_ms(&self) -> u32 {
        if self.open.required.len() == 2 && self.stage_zero_done {
            self.open
                .crypto_step_timeout_ms
                .max(crate::protocol::constants::HEADS_UP_STAGE_BUDGET_MS)
        } else {
            self.open.crypto_step_timeout_ms
        }
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
        // `HAND_COMPLETE` stage from the seats that are parties.
        //
        // **And if neither comes, it stalls — which the adrift latch does NOT
        // answer, whatever this comment used to say (`S1-BW`).** The latch is
        // read by the next-hand timer arm, and that arm is reached only once
        // the running hand is over, so a follower stuck inside a hand sets the
        // latch and nothing acts on it: measured at 283 seconds in one hand in
        // `split125944-9`, ended only by the harness. What answers it is
        // upstream of here — the boundary now keeps every event of the next
        // hand rather than only its `HAND_INIT`, so the stage a late-opening
        // follower needs is replayed into it instead of being dropped. A
        // follower's own local terminus remains available if that is not
        // enough, and is `S1-BW`'s option (2).
        //
        // **And membership is asked of `required` alone, deliberately
        // (`S1-BY`).** A certificate about this very seat can bank in the
        // middle of the hand, and it is tempting to read `certified` here too:
        // §4.10 gives a seat outside `P(k)` no terminal to emit, and the seat
        // goes on believing it is a party. That was built and reverted, and
        // the reason is worth keeping. `required` is **this hand's** party set
        // and `certified` is **the next hand's** roster; §4.10 does not narrow
        // the first within the hand, on purpose, so `HAND_COMPLETE` is a
        // collective stage over `required` that cannot complete anywhere
        // without this seat's own copy — which a seat stages behind cannot
        // produce. Silencing it therefore leaves it with no terminus at all,
        // which is `S1-BW`'s stall moved onto a different seat. And the
        // measurement says the emission is not the fault it looks like: in
        // `split092359-10` this seat's abort **was** the terminal the table
        // adopted, four nodes taking it as `Abort::Told` within 230 ms, with
        // every later hand at one genesis, because `abort_terminal` is a
        // function of `GENESIS(k)` alone. `certified` also grows for a kind-1
        // certificate, which removes nobody, so reading it here would let one
        // auto-fold disarm this backstop for the rest of the hand.
        if !self.open.required.contains(&self.open.my_seat) {
            return false;
        }
        // **`S1-BT`, and the owner's ruling of 2026-09-05.** Twice the budget
        // is when the vote round *starts* — the mid-delivery lever releases at
        // exactly that instant — so aborting there killed every round before it
        // could gather, and the seat that stalled the stage was never named.
        // The round now gets one stage budget of air after this client's own
        // vote, with a ceiling of three budgets so a round that cannot complete
        // still ends the hand.
        //
        // **The crypto-stage escape is here for the same reason it is on the
        // receiving side.** `past_deadline` returns true on the whole-hand
        // budget before it ever looks at the stage, so without this a betting
        // stage that has spent the hand's entire budget would have its backstop
        // deferred by a vote round — and that budget is the one deadline the
        // protocol makes unconditional.
        // `S1-HA`: no air for a round a voter gone from the group cannot close.
        if self.certificate_possible()
            && !self.a_voter_is_gone()
            && !(self.long_past_stage(now_ms)
                && (!self.crypto_stage() || self.round_has_had_air(now_ms)))
        {
            return false;
        }
        true
    }

    /// Whether a certificate could still end the stage this hand is waiting on.
    fn certificate_possible(&self) -> bool {
        // D-036: the quiet set is certified together, so the question is
        // whether the seats outside it clear the floor -- two at least, and
        // more of them than of the quiet.
        let quiet: Vec<SeatIdx> = self
            .waiting_for()
            .into_iter()
            .filter(|s| *s != self.open.my_seat && self.mine.dealt_in.contains(s))
            .collect();
        !quiet.is_empty() && self.admissible_for(self.voters_of(&quiet).len(), &quiet)
    }

    /// D-036: how many votes a subject needs, for the tally's denominator --
    /// the voters of the whole quiet set this client waits on, which is the
    /// set the certificate will carry. The single-subject count read *3/4
    /// agree* on a certificate that had just completed among three.
    fn tally_need(&self) -> usize {
        let quiet: Vec<SeatIdx> = self
            .waiting_for()
            .into_iter()
            .filter(|s| *s != self.open.my_seat)
            .collect();
        self.voters_of(&quiet).len()
    }

    /// D-036's floor, both halves: at least two voters, and more voters
    /// than seats named. Below two, "unanimity" is one interested party
    /// (D-008); at or below the named count, a group short of a majority of
    /// the live seats could certify the rest out.
    fn admissible(voters: usize, named: usize) -> bool {
        voters >= 2 && voters > named
    }

    /// `D-063`: the floor with the resigned excepted. A seat named with its
    /// player's own signed leave counts for nothing against it -- its word is
    /// its consent, and no fork can hold a seat that said it left -- so the
    /// floor is asked of the quiet alone, and a certificate naming only seats
    /// that resigned needs one voter (the owner, 2026-09-17: a table whose
    /// players leave must not stand for ever).
    fn floor_holds(voters: usize, named: usize, resigned: usize) -> bool {
        let quiet = named.saturating_sub(resigned.min(named));
        voters >= 1 && (quiet == 0 || Self::admissible(voters, quiet))
    }

    /// `D-063`: the floor for a set this client would seal, with the words it
    /// holds.
    fn admissible_for(&self, voters: usize, named: &[SeatIdx]) -> bool {
        let resigned = named.iter().filter(|s| self.leave_words.contains_key(s)).count();
        Self::floor_holds(voters, named.len(), resigned)
    }

    /// The floor, told to the player once per hand when it is what holds
    /// the hand: the seats that stopped are half the table or more, so no
    /// certificate can remove them: a betting stage waits until the hand's
    /// own time runs out, a crypto stage ends on its budget with every stack
    /// restored and the next hand waits again. Three seats dealt in or more -- heads-up
    /// the window says its own words (D-007, D-034). A note and nothing
    /// else: the rule is D-036's and the player's way out is the door.
    fn say_floor_once(&mut self, now_ms: u64) {
        if self.floor_said || self.mine.dealt_in.len() < 3 {
            return;
        }
        let quiet: Vec<SeatIdx> = self
            .waiting_for()
            .into_iter()
            .filter(|s| *s != self.open.my_seat && self.mine.dealt_in.contains(s))
            .collect();
        if quiet.is_empty() {
            return;
        }
        let voters = self.voters_of(&quiet).len();
        // `D-063`: the seats that resigned are removed whatever the floor says;
        // the note is about the quiet, and only while the floor holds.
        if self.admissible_for(voters, &quiet) {
            return;
        }
        let quiet: Vec<SeatIdx> = quiet.into_iter().filter(|s| !self.leave_words.contains_key(s)).collect();
        if quiet.is_empty() {
            return;
        }
        let waited_s = now_ms.saturating_sub(self.stage_at_ms) / 1_000;
        self.floor_said = true;
        self.cert_note.push(format!(
            "seat(s) {} have not acted at this stage for {waited_s} s and no certificate can remove them: {voters} seat(s) are present to vote about the {} that stopped, and the rule needs two voters at least and more voters than seats named (D-036); the table waits for them -- wait, or leave the table",
            Self::seats_words(&quiet),
            quiet.len()
        ));
    }

    /// D-036: the quiet set this client holds a complete case for, with its
    /// voters -- the greatest set `S` of dealt-in seats voted about at this
    /// stage such that every seat of `V(S)` (dealt in, outside `S`, not
    /// certified) has voted about every member of `S`. Greatest, because
    /// sealable sets are closed under union, so every honest seat converges
    /// on one `S` as the votes reach it. A seat already in `certified` from a
    /// peer's copy about this very stage is still a candidate: the roster
    /// half of that copy banks before the positional half is judged, and
    /// this client's own copy must still name it. `None` below the floor,
    /// when nothing is voted about, and when this client is itself in the
    /// set: the subject must accept a certificate naming it and must not
    /// emit one.
    fn sealable_set(&self) -> Option<(CertSubject, Vec<SeatIdx>)> {
        let mut about: BTreeMap<SeatIdx, (TimeoutVote, BTreeSet<SeatIdx>)> = BTreeMap::new();
        for (digest, held) in &self.votes {
            let Some(subject) = self.subject_of(digest) else {
                continue;
            };
            if !self.mine.dealt_in.contains(&subject.subject_seat) {
                continue;
            }
            about.insert(subject.subject_seat, (subject, held.keys().copied().collect()));
        }
        let mut set: BTreeSet<SeatIdx> = about.keys().copied().collect();
        loop {
            let named: Vec<SeatIdx> = set.iter().copied().collect();
            let outside: BTreeSet<SeatIdx> = self.voters_of(&named).into_iter().collect();
            let kept: BTreeSet<SeatIdx> = set
                .iter()
                .copied()
                .filter(|s| outside.is_subset(&about[s].1))
                .collect();
            if kept.len() == set.len() {
                break;
            }
            set = kept;
        }
        if set.is_empty() || set.contains(&self.open.my_seat) {
            return None;
        }
        let named: Vec<SeatIdx> = set.iter().copied().collect();
        let voters = self.voters_of(&named);
        if !self.admissible_for(voters.len(), &named) {
            return None;
        }
        let first = &about[set.iter().next()?].0;
        let mut subject = CertSubject::of(first);
        subject.subject_seats = set.into_iter().collect();
        // `D-051`: each seat with the cause its votes carry.
        subject.causes = subject.subject_seats.iter().map(|s| about[s].0.cause).collect();
        Some((subject, voters))
    }

    /// The keys of these seats, in the seats' order.
    fn keys_of(&self, seats: &[SeatIdx]) -> Vec<[u8; 32]> {
        seats
            .iter()
            .filter_map(|s| self.open.seats.iter().find(|(x, _, _)| x == s).map(|(_, k, _)| *k))
            .collect()
    }

    /// "seat 3" or "seats [3, 4]", for the notes.
    fn seats_words(seats: &[SeatIdx]) -> String {
        match seats {
            [one] => format!("seat {one}"),
            many => format!("seats {many:?}"),
        }
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
        self.voted_about.clear();
        self.own_vote_at = None;
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

    /// Whether a settlement that arrived after this client's own abort closed
    /// the late stage (`S1-BP`, `S1-CL`), so that `next_hand` derives hand k+1
    /// from the settlement's stacks rather than the abort's. `false` for a hand
    /// that was not aborted at all.
    pub fn late_settled(&self) -> bool {
        self.late.as_ref().is_some_and(|l| l.closed.is_some())
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
            // `D-051`: a seat this client cut off for flooding is voted about
            // with the cause, and only votes with the same cause count with
            // this client's.
            cause: self.flooders.contains(&seat).then_some(CAUSE_FLOOD),
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
        self.voters_of(&[subject])
    }

    /// D-036: the voter set for a certificate naming these seats: everybody
    /// dealt in but them, less the seats a completed certificate has already
    /// named. Ascending.
    fn voters_of(&self, named: &[SeatIdx]) -> Vec<SeatIdx> {
        let mut v: Vec<SeatIdx> = self
            .mine
            .dealt_in
            .iter()
            .copied()
            .filter(|s| !named.contains(s) && !self.certified.contains(s))
            .collect();
        v.sort_unstable();
        v
    }

    /// `D-058`: the seats the open cryptographic stage -- the hand's opening
    /// before the deal, the deck, a reveal, the showdown -- has waited on for at
    /// least `after_ms`; empty inside that moment, at a betting stage (whose
    /// turn says who) and for a hand that is over. For the window: a stage held
    /// by a seat that answers nothing is said long before its budget runs out.
    pub fn stage_stands_on(&self, now_ms: u64, after_ms: u64) -> Vec<SeatIdx> {
        if self.over() || !self.crypto_stage() || now_ms.saturating_sub(self.stage_at_ms) < after_ms {
            return Vec::new();
        }
        self.waiting_for()
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

    /// Whether this client is a party to a vote round that is open at this
    /// stage: a certificate stage is open here, or this client is one of the
    /// voters a subject the stage still waits on needs.
    ///
    /// It does **not** ask whether this client has voted. That was
    /// `vote_joined`'s question and it is the wrong one.
    ///
    /// **Owed or cast, and the difference is the whole of `S1-BT`.**
    /// `vote_joined` asks only whether this client has already voted. A seat
    /// whose own timer has not yet fired is not "joined" by that test and will
    /// admit a peer's abort at one budget — killing the round for everybody
    /// including the seats that did vote, because an abort ends the hand at
    /// every receiver. The round is a collective object and every seat whose
    /// vote it needs is a party to it, whether or not that seat has got round
    /// to voting.
    ///
    /// **Stage-local by construction**, which is what makes it safe to read at
    /// a gate: `waiting_for` is the open stage's own set, `voters` is derived
    /// from the roster at this stage, and `mark_stage` clears every piece of
    /// vote state when the sequence moves. Nothing here can be true about a
    /// stage the hand has left. This replaced `vote_joined`, whose test was
    /// *has this client voted* rather than *is this client needed*.
    fn party_to_vote(&self) -> bool {
        if self.certifying.is_some() {
            return true;
        }
        self.certificate_possible()
            && self.mine.dealt_in.contains(&self.open.my_seat)
            && !self.certified.contains(&self.open.my_seat)
    }

    /// Whether the vote round open at this stage has had its budget of air.
    ///
    /// **The bound, stated in one place because two gates read it.** True when
    /// there is nothing owed at all; when the stage is three budgets old, which
    /// is the ceiling and the thing that makes this terminate; when this client
    /// is not a party to any round, so there is nothing to wait for; or when a
    /// full stage budget has passed since this client cast its own vote, which
    /// is the time the other seats' copies need to arrive.
    ///
    /// **Why the ceiling is three and not two.** Two is where the round starts:
    /// the mid-delivery lever releases the vote at twice the budget
    /// (`S1-BK`), so a round that begins at all begins there, and a bound of
    /// two would give it no air whatever. Three is that plus one budget, which
    /// is the same budget every other stage of this hand gets.
    ///
    /// **`certificate_possible` and not `party_to_vote`, and the difference
    /// splits a table.** `may_abandon`'s own guard is `certificate_possible`,
    /// so if this predicate asked a narrower question the two would disagree
    /// exactly where it matters: a client whose own seat has been certified out
    /// mid-hand is no longer in `voters(s)`, so `party_to_vote` is false for it
    /// while `certificate_possible` is still true — and it would abort at two
    /// budgets while every seat that can vote holds to three. One seat ending
    /// hand k with a bare abort while the rest end it with a certificate is two
    /// different `R(k+1)` from one chain. `party_to_vote` is the right question
    /// at the receive gate's third clause, where it really is *is this client
    /// needed*, and it stays there.
    fn round_has_had_air(&self, now_ms: u64) -> bool {
        let Some(owed) = self.owed_type() else {
            return true;
        };
        let budget = u64::from(self.next_deadline_for(owed));
        now_ms.saturating_sub(self.stage_at_ms) >= budget.saturating_mul(3)
            || !self.certificate_possible()
            || self
                .own_vote_at
                .is_some_and(|t| now_ms.saturating_sub(t) >= budget)
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
        if self.over() {
            return Ok(Vec::new());
        }
        let Some(owed) = self.owed_type() else {
            return Ok(Vec::new());
        };
        let age = now_ms.saturating_sub(self.stage_at_ms);
        let mut out = Vec::new();
        for seat in self.waiting_for() {
            // Never about oneself, and never twice.
            if seat == self.open.my_seat {
                continue;
            }
            // `D-059`: each seat on its own clock -- the stage's budget, less the
            // table's patience the seat has used up, at a cryptographic step. The
            // vote still names the stage's own `deadline_ms`, which is all a
            // receiver checks, and a certificate needs every voter: the table
            // votes a seat out when the most patient voter's clock says so.
            let after = self.vote_after_ms(seat, owed);
            if age < after {
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
            // `D-059`: twice the seat's own time, which is twice the budget for a
            // seat that has not made the table wait.
            if age < after.saturating_mul(2)
                && mid_delivery & (1u32 << u32::from(seat.min(31))) != 0
            {
                continue;
            }
            let Some(subject) = self.subject_now(seat) else {
                continue;
            };
            let digest = subject.subject_digest();
            // `D-051`: once about one seat at one stage, whatever the cause --
            // a seat cut off after this client voted about it without one is
            // voted about with the cause at its next stage, never twice here.
            if self.voted.contains(&digest) || self.voted_about.contains(&seat) {
                continue;
            }
            // Below the floor a certificate has no effect whatever, so a vote
            // towards one is noise on the wire and an invitation to an
            // implementer to reach for a quorum. It is not sent at all.
            // D-036: the floor is the joint one -- the seats outside the
            // quiet set are two at least, and more than the quiet.
            if !self.certificate_possible() {
                self.say_floor_once(now_ms);
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
            self.voted_about.insert(seat);
            // **`S1-BB`: the carrier, sampled where the accusation is made.**
            // Read from the same `mid_delivery` word the lever above consulted,
            // so the two cannot drift; recorded after `say_at` has succeeded,
            // because a vote that failed to seal is not an accusation.
            self.own_vote_carrier.push((
                seat,
                mid_delivery & (1u32 << u32::from(seat.min(31))) != 0,
                self.long_past_stage(now_ms),
            ));
            // **`S1-BT`: the LAST vote, not the first.** This loop walks every
            // seat the stage waits on and the mid-delivery lever is applied per
            // seat, so a stage waiting on two seats with different bits votes
            // about the unheld one a whole budget before the held one. Keeping
            // the first vote's time spent the round's air on a round that had
            // not started: at twice the budget the lever released the second
            // vote and the abort fired in the same tick, which is the collision
            // this row exists to remove. The three-budget ceiling still bounds
            // it, because the lever cannot withhold a vote past
            // `long_past_stage`.
            self.own_vote_at = Some(now_ms);
            self.take_vote(digest, self.open.my_seat, bytes.clone(), subject);
            self.tally = Some((
                seat,
                self.votes.get(&digest).map(|m| m.len()).unwrap_or(0),
                self.tally_need(),
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
        if !body.cause_is_known() {
            return Err(Failed::Elsewhere {
                seat,
                what: "its vote named a cause this catalogue defines",
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
        self.tally = Some((mine.subject_seat, held, self.tally_need(), digest));
        // The vote that completes the set is what produces the certificate, so
        // the two are one call: there is no state in which unanimity has been
        // reached and nobody has said so.
        self.certify_if_unanimous(key, now_ms)
    }

    /// Emit a certificate once every voter has said the same thing.
    ///
    /// D-036: *the same thing* is the whole quiet set. `sealable_set` finds
    /// the greatest set `S` of seats voted about at this stage such that every
    /// seat outside `S` has voted about every member of `S`; this client seals
    /// a certificate naming `S` with all those votes, and the certificate
    /// stage is collective over `V(S)`. Sealable sets are closed under union,
    /// so every honest seat converges on one `S` as the votes reach it: a
    /// copy about a smaller set is answered by this client's own about the
    /// larger, which carries the votes that prove it. Never about itself --
    /// the subject must accept a certificate naming it and must not emit one
    /// -- and its own copy is its own word, so this client waits for a
    /// complete set exactly as it would to start one.
    fn certify_if_unanimous(
        &mut self,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let Some((subject, voters)) = self.sealable_set() else {
            // Only worth a word when a set that looks complete produced
            // nothing. A partial set is the ordinary state and says nothing.
            if let Some((d, m)) = self.votes.iter().max_by_key(|(_, m)| m.len()) {
                let seat = self.subject_of(d).map(|s| s.subject_seat);
                let need = seat.map(|s| self.voters(s).len()).unwrap_or(0);
                if m.len() >= need && need >= 2 {
                    self.cert_note.push(format!(
                        "cert: {} votes held and none certified; subject {:?} dealt_in {:?} certified {:?}",
                        m.len(),
                        seat,
                        self.mine.dealt_in,
                        self.certified
                    ));
                }
            }
            return Ok(Vec::new());
        };
        // A stage already open about exactly this set takes this client's
        // copy. One about another set is replaced: the sealable set only
        // grows, and the peers that opened the smaller one move to this one
        // when this client's copy brings them the votes (D-036).
        let same = self.certifying.as_ref().is_some_and(|c| c.subject == subject);
        if !same {
            if let Some(c) = &self.certifying {
                self.cert_note.push(format!(
                    "cert: the quiet set grew from {:?} to {:?}; the certificate stage moves with it (D-036)",
                    c.subject.subject_seats, subject.subject_seats
                ));
            }
            let stage = Collective::closed(
                subject.subject_sequence,
                EventType::TimeoutCert.code(),
                &voters,
            )
            .ok_or(Failed::NotInThisStage)?;
            self.certifying = Some(Certifying { subject: subject.clone(), stage });
        }
        if self
            .certifying
            .as_ref()
            .is_some_and(|c| c.stage.heard(self.open.my_seat).is_some())
        {
            return Ok(Vec::new());
        }
        let bytes = self.seal_certificate(&subject, &voters, key, now_ms)?;
        let hash = self.opened(&bytes, EventType::TimeoutCert)?.event_hash;
        self.note_own_certificate(hash, &bytes, &subject);
        let complete = match self.certifying.as_mut() {
            Some(c) => {
                c.stage.hear(self.open.my_seat, hash);
                c.stage.complete()
            }
            None => unreachable!("set just above"),
        };
        // The one line worth an operator's attention: from here the table has
        // said something about a seat with everybody's signature behind it.
        self.cert_note.push(format!(
            "the table has certified {}'s timeout, unanimously among {:?}",
            Self::seats_words(&subject.subject_seats),
            voters
        ));
        let mut out = vec![Send::Broadcast(bytes)];
        if complete {
            out.append(&mut self.apply_certificate(key, now_ms)?);
        }
        Ok(out)
    }

    /// One certificate, from the votes this client holds about every seat
    /// it names: for every named seat ascending, the votes of `V(S)`
    /// ascending by voter -- one byte string for one set of votes, at every
    /// sealer.
    fn seal_certificate(
        &self,
        subject: &CertSubject,
        voters: &[SeatIdx],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<u8>, Failed> {
        let mut votes = Vec::new();
        for seat in &subject.subject_seats {
            let digest = subject.vote_about(*seat).subject_digest();
            let held = self.votes.get(&digest).ok_or(Failed::NothingFurther)?;
            for voter in voters {
                votes.push(held.get(voter).cloned().ok_or(Failed::NothingFurther)?);
            }
        }
        // `D-063`: and the words of the players that left, for the seats named.
        let resignations: Vec<Vec<u8>> = subject
            .subject_seats
            .iter()
            .filter_map(|s| self.leave_words.get(s).cloned())
            .collect();
        let body = TimeoutCert {
            subject_digest: subject.digest(),
            votes,
            resignations,
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
    fn note_own_certificate(&mut self, hash: Hash, bytes: &[u8], subject: &CertSubject) {
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
                        own: true,
                        disagreed: false,
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
        // `S1-ER`: read before `self.late` is borrowed, because what each seat
        // gained is this against the body's own final stacks, and by the time
        // the body closes below there is no way back to it. Empty where this
        // client is no longer playing the hand, and then nothing is claimed.
        let stacks_before = self.stacks();
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
                own: false,
                disagreed: false,
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
        // **Whose body this is, and the second half of this used to be wrong**
        // (`S1-CL`). When this client gave up **while settling**, `give_up`
        // carried its own stage and its own body across, so `late.body` is this
        // client's OWN settlement and `late.closed` applies those stacks and no
        // other seat's. That half is sound.
        //
        // When it gave up **before** settling, `late.body` is whichever peer
        // spoke first. This comment used to argue that such a body reaches
        // neither `next_hand` nor `GENESIS(k+1)`, because the stage needs every
        // seat of the required set and *"this client is in that set, and it
        // never published a `HAND_COMPLETE` for this stage to hear"*.
        //
        // **That clause is false for a readmitted seat, and `open_with` makes
        // it so deliberately**: the membership test there is
        // `accepted.contains(&o.my_seat)` over `required ∪ readmitted`, which
        // is §4.9's readmission route. Such a seat holds a hand whose `my_seat`
        // is outside `required`, the stage closes on the required peers alone,
        // and `next_hand`'s `Phase::Aborted` arm does read `late.closed`'s
        // stacks — into `roster_hash(k+1)` and so into `GENESIS(k+1)`. Pinned
        // by `a_readmitted_seat_is_outside_the_set_the_late_stage_requires`.
        //
        // **What is actually true is narrower and is why this is not changed
        // here.** When the required peers agree — the ordinary case — every
        // body is identical, arrival order decides nothing, and following the
        // settlement is exactly what keeps a readmitted seat with the table.
        // The exposure is confined to a settlement the required peers
        // **disagree** about, which `S1-BD` made audible on purpose and which
        // `S1-CI` shows §6.3 cannot then heal. Gating this arm on
        // `required.contains(my_seat)` would send a readmitted seat to the
        // abort's stacks while everyone else took the settlement's, which forks
        // it away from a table it currently follows — a worse answer than the
        // one being defended.
        let note = if theirs == late.body {
            None
        } else {
            late.disagreed = true;
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
            if late.own || !late.disagreed {
                let hash = late.stage.hash().ok_or(Failed::NotInThisStage)?;
                let stacks = late.body.final_stacks.clone();
                // `D-052`: the settlement this client ends on is the one the
                // window says the pots from, the late road included.
                self.settled_pots =
                    late.body.pots.iter().map(|a| (a.size, a.winners.clone())).collect();
                // `S1-ER`: and what it moved to each seat, where the stacks
                // before it are known.
                self.settled_gain = if stacks_before.is_empty() {
                    Vec::new()
                } else {
                    late.body
                        .final_stacks
                        .iter()
                        .enumerate()
                        .map(|(s, end)| {
                            end.saturating_sub(stacks_before.get(s).copied().unwrap_or(*end))
                        })
                        .collect()
                };
                late.closed = Some((hash, stacks));
                // The agreed branch used to close in silence, so a run could
                // show the refusal below and never the adoption, and the
                // corpus could not say how often this path fires at all.
                // Same channel as the refusal, so one grep finds both.
                //
                // **Folded behind a pending disagreement, never over it.** The
                // channel is one slot, and an own body closing in the same call
                // that a dissenting copy arrived produced two facts for it. The
                // first draft kept the second and lost the first, and the
                // `S1-BP` test -- which asserts the disagreement is REPORTED --
                // is what caught it.
                let closed = format!(
                    "the late settlement of hand #{} closed: {} body, every required seat heard{}",
                    self.open.hand_id,
                    if late.own { "this client's own" } else { "a peer's" },
                    if late.disagreed { ", not all agreeing" } else { "" }
                );
                self.settle_note = Some(match note {
                    Some(n) => format!("{n}; and {closed}"),
                    None => closed,
                });
                return Ok(Vec::new());
            } else {
                // **A borrowed body under disagreement is not adopted** — that
                // would be choosing a side by arrival order, which is a
                // per-receiver quantity, into `GENESIS(k+1)` (D-012, `S1-CL`).
                // This client has no settlement of its own to prefer, so it
                // keeps the abort's terminal and stacks and says why. The
                // required peers, who are the ones disagreeing, are meanwhile
                // freezing the table at checkpoint 8, so no hand `k+1` is
                // being dealt for this client to have missed.
                let refused = format!(
                    "the late settlement of hand #{} is complete but its emitters disagree, and \
                     this client never settled the hand itself, so it adopts none of them: the \
                     abort's terminal stands here",
                    self.open.hand_id
                );
                // Same rule as the closed branch: the disagreement that got us
                // here is kept in front of the refusal it caused.
                self.settle_note = Some(match note {
                    Some(n) => format!("{n}; and {refused}"),
                    None => refused,
                });
                return Ok(Vec::new());
            }
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
        // hundred verifications. D-036: a joint certificate carries |S| x |V|
        // votes, and with |V| > |S| and |S| + |V| <= MAX_SEATS that is at most
        // MAX_SEATS squared over four.
        let max_votes = usize::from(crate::protocol::constants::MAX_SEATS).pow(2) / 4;
        if body.votes.len() < 2 || body.votes.len() > max_votes {
            return Err(Failed::Elsewhere {
                seat: self.open.my_seat,
                what: "a vote count inside the protocol's bounds",
            });
        }
        let emitter = self.seat_of(&opened.sender)?;

        // `D-063`: the words of the players that left, each the seat's own
        // signed leave for this table, one per seat, every one a seat named.
        if body.resignations.len() > usize::from(crate::protocol::constants::MAX_SEATS) {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "no more resignations than seats",
            });
        }
        let mut resignations: Vec<(SeatIdx, Vec<u8>)> = Vec::new();
        for word in &body.resignations {
            let (who, _) = crate::net::tabletalk::verify_leave_word(word, &self.open.table_id).map_err(|_| {
                Failed::Elsewhere {
                    seat: emitter,
                    what: "every resignation were the seat's own signed leave for this table",
                }
            })?;
            let seat = self.seat_of(&who)?;
            if resignations.iter().any(|(s, _)| *s == seat) {
                return Err(Failed::Elsewhere {
                    seat: emitter,
                    what: "one resignation per seat",
                });
            }
            resignations.push((seat, word.clone()));
        }

        let mut stage: Option<CertSubject> = None;
        let mut per_subject: BTreeMap<SeatIdx, BTreeSet<SeatIdx>> = BTreeMap::new();
        // `D-051`: the cause every vote about a seat carries, one per seat.
        let mut causes: BTreeMap<SeatIdx, Option<u16>> = BTreeMap::new();
        let mut votes: Vec<(SeatIdx, TimeoutVote, Vec<u8>)> = Vec::new();
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
            let named: TimeoutVote =
                chained::payload(&v, TIMEOUT_VOTE_CAP).map_err(Failed::Wire)?;
            if !named.cause_is_known() {
                return Err(Failed::Elsewhere {
                    seat: emitter,
                    what: "every carried vote named a cause this catalogue defines",
                });
            }
            if *causes.entry(named.subject_seat).or_insert(named.cause) != named.cause {
                return Err(Failed::Elsewhere {
                    seat: emitter,
                    what: "every vote about one seat named one cause",
                });
            }
            match &stage {
                None => stage = Some(CertSubject::of(&named)),
                Some(first) => {
                    if !first.same_stage(&named) {
                        return Err(Failed::Elsewhere {
                            seat: emitter,
                            what: "every carried vote were about one stage under one deadline",
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
            if !per_subject.entry(named.subject_seat).or_default().insert(voter) {
                return Err(Failed::Elsewhere {
                    seat: emitter,
                    what: "no seat had voted twice about one seat",
                });
            }
            votes.push((voter, named, vote.clone()));
        }
        let Some(mut subject) = stage else {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "a certificate carried the votes it is made of",
            });
        };
        subject.subject_seats = per_subject.keys().copied().collect();
        subject.causes = subject
            .subject_seats
            .iter()
            .map(|s| causes.get(s).copied().flatten())
            .collect();
        // D-036: one voter set for every seat named, and no named seat in it.
        let voters: BTreeSet<SeatIdx> = per_subject.values().next().cloned().unwrap_or_default();
        if per_subject.values().any(|v| *v != voters) {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "every seat named had the votes of one and the same voter set",
            });
        }
        if voters.iter().any(|v| subject.subject_seats.contains(v)) {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "no seat named were a voter",
            });
        }
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
        if subject.digest() != body.subject_digest {
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
        if subject.subject_seats.iter().any(|s| !self.mine.dealt_in.contains(s)) {
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
        if betting && subject.subject_seats.len() != 1 {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "a betting stage named the one seat to act",
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
        // D-036's floor: two voters at least, and more voters than seats
        // named -- a group short of a majority of the live seats can
        // complete nothing about the rest. `D-063`: the seats that resigned,
        // named with their word, count for nothing against it.
        if resignations.iter().any(|(s, _)| !subject.subject_seats.contains(s)) {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "every resignation named a seat the certificate names",
            });
        }
        if !Self::floor_holds(voters.len(), subject.subject_seats.len(), resignations.len()) {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "at least two voters, and more voters than seats named that did not resign",
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
            votes,
            resignations,
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

    fn bank(&mut self, subject: &CertSubject, event_hash: Hash, raw: &[u8]) -> bool {
        // The roster freezes with a settled terminal. After an abort terminal
        // a certificate still banks — and says so, so the node re-derives the
        // next hand (`late_roster`). `S1-BS`: eight seats banked a certificate
        // before the abort ended the hand, the ninth received no copy until
        // after, refused it here, and opened the next hand with the certified
        // seat still in R — a genesis nobody else held.
        if self.settled() || self.banked.len() >= BANKED_CAP {
            return false;
        }
        // `D-047`: the word, kept by the seats it names.
        for seat in &subject.subject_seats {
            self.words.insert(*seat, raw.to_vec());
            // `D-063`: named with its own word: out for good, not absent.
            if self.leave_words.contains_key(seat) {
                self.resigned.insert(*seat);
            }
        }
        self.certs.insert(
            event_hash,
            CertFact {
                subject_seats: subject.subject_seats.clone(),
                kind: subject.kind,
            },
        );
        if self.proof.is_none() {
            self.proof = Some((event_hash, raw.to_vec()));
        }
        if !self.banked.insert(subject.digest()) {
            return false;
        }
        // D-036: every seat named leaves R(k+1); one strike per seat per
        // stage, however many sets at this stage name it.
        for seat in &subject.subject_seats {
            // `D-051`: named with the flood cause by every voter -- the digest
            // commits to it -- the seat is out of the table for good.
            if subject.cause_of(*seat) == Some(CAUSE_FLOOD) {
                self.flood_named.insert(*seat);
            }
            if !self.certified.contains(seat) {
                self.certified.push(*seat);
            }
            if self.struck.insert((*seat, subject.subject_sequence)) {
                if let Some(n) = self.strikes.get_mut(usize::from(*seat)) {
                    *n = n.saturating_add(1);
                }
            }
        }
        if self.aborted().is_some() {
            self.late_roster = true;
            self.cert_note.push(format!(
                "cert: about {} banked after the terminal; the roster of hand #{} is re-derived",
                Self::seats_words(&subject.subject_seats),
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
    fn commit_certificate(&mut self, subject: &CertSubject) {
        self.certifying = None;
        for seat in &subject.subject_seats {
            self.note_signed(*seat);
            // The same rule as `bank`: a settled hand's roster does not move.
            if self.settled() || self.certified.contains(seat) {
                continue;
            }
            self.certified.push(*seat);
            if self.struck.insert((*seat, subject.subject_sequence)) {
                if let Some(n) = self.strikes.get_mut(usize::from(*seat)) {
                    *n = n.saturating_add(1);
                }
            }
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
        // D-036 adds the other half: the voters outnumber the seats named, so
        // no group short of a majority of the live seats completes a
        // certificate about the rest.
        if self.mine.dealt_in.len() < 3
            || !Self::floor_holds(c.voters.len(), c.subject.subject_seats.len(), c.resignations.len())
        {
            return Ok(Vec::new());
        }
        let nominal: BTreeSet<SeatIdx> = self
            .mine
            .dealt_in
            .iter()
            .copied()
            .filter(|s| !c.subject.subject_seats.contains(s))
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
        let mine: BTreeSet<SeatIdx> = self.voters_of(&c.subject.subject_seats).into_iter().collect();
        if !mine.is_subset(&c.voters) {
            // **Said once per subject.** The replay pass re-judges every held
            // copy on every two-second tick, and the note came with it every
            // time: a retained hand in the boundary wait printed one line per
            // held copy for as long as it waited.
            if self.shortfall_said.insert(c.subject.digest()) {
                self.cert_note.push(format!(
                    "cert: from seat {seat} about {} with voters {:?}; this client \
                     derives {mine:?} and is missing a certificate the emitters hold — HELD",
                    Self::seats_words(&c.subject.subject_seats),
                    c.voters
                ));
            }
            return Err(Failed::NotYet);
        }

        // D-036: the votes a certificate carries are this client's now. A
        // seat that never received one of them holds it from here, so its own
        // case can complete and its copy can join the stage.
        for (voter, v, raw) in &c.votes {
            self.take_vote(v.subject_digest(), *voter, raw.clone(), *v);
        }
        // `D-063`: and the words it carries, so this client's own copy carries
        // them too and its floor reads the same.
        for (seat, word) in &c.resignations {
            self.leave_words.entry(*seat).or_insert_with(|| word.clone());
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
                    "cert: from seat {seat} about {} at sequence {} reached this client at \
                     sequence {} — roster banked, positional half HELD until it is in position",
                    Self::seats_words(&c.subject.subject_seats),
                    c.subject.subject_sequence,
                    self.slot.sequence
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
                    "the table acted for {} at sequence {} and played on; this \
                     client is at sequence {} on a branch of its own, and the two \
                     cannot be reconciled",
                    Self::seats_words(&c.subject.subject_seats),
                    c.subject.subject_sequence,
                    self.slot.sequence
                ));
            }
            if banked && c.subject.kind == 2 {
                // Convergent: `abort_terminal(k)` is a function of `GENESIS(k)`
                // and of nothing in the middle of the hand, so a peer ending the
                // hand from anywhere ends it where everyone else does.
                let named = self.keys_of(&c.subject.subject_seats);
                let proof = self.proof.as_ref().map(|(h, _)| *h);
                return self.abort_named(named, proof, key, now_ms);
            }
            return Ok(Vec::new());
        }

        // In position: the collective stage, so that every voter's copy is
        // counted and the stage closes the way every other stage does.
        //
        // D-036: the stage is about the greatest quiet set this client can
        // seal -- this copy's set, or a larger one now that the copy's votes
        // are here. A copy about a smaller set is not counted; its emitter
        // moves to the larger set when this client's copy reaches it with the
        // votes that prove it.
        let target = match self.sealable_set() {
            Some((s, _)) if s != c.subject => s,
            _ => c.subject.clone(),
        };
        let open_here = self.certifying.as_ref().is_some_and(|cur| cur.subject == target);
        if !open_here {
            if let Some(cur) = &self.certifying {
                let grew = cur
                    .subject
                    .subject_seats
                    .iter()
                    .all(|s| target.subject_seats.contains(s));
                if !grew {
                    // A stale certificate must not destroy a live
                    // certification about something else.
                    return Err(Failed::NotYet);
                }
                self.cert_note.push(format!(
                    "cert: the quiet set grew from {:?} to {:?}; the certificate stage moves with it (D-036)",
                    cur.subject.subject_seats, target.subject_seats
                ));
            }
            // **The subject's sequence, not this client's slot.**
            // `stage_hash_collective` hashes the sequence, so a peer that
            // built the stage at its own cursor would compute a parent no
            // other peer computed. A no-op while every peer is in position,
            // and a fork the moment one is not.
            let voters = self.voters_of(&target.subject_seats);
            let stage = Collective::closed(
                target.subject_sequence,
                EventType::TimeoutCert.code(),
                &voters,
            )
            .ok_or(Failed::NotInThisStage)?;
            self.certifying = Some(Certifying {
                subject: target.clone(),
                stage,
            });
        }
        if c.subject == target {
            let Some(cur) = self.certifying.as_mut() else {
                unreachable!("just set")
            };
            if cur.stage.heard(seat) != Some(c.event_hash) {
                match cur.stage.hear(seat, c.event_hash) {
                    Heard::Counted | Heard::Bystander | Heard::Again => {}
                    Heard::Equivocation { .. } => return Err(Failed::Equivocation { seat }),
                    // The stage here was built from this client's own voter
                    // set, and an emitter outside it means the two peers
                    // derived different sets. That is this client being
                    // behind, not the sender being wrong, so it is held
                    // rather than rejected.
                    Heard::Uninvited => return Err(Failed::NotYet),
                }
            }
        }
        let complete = self
            .certifying
            .as_ref()
            .is_some_and(|cur| cur.stage.complete());
        if !complete {
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
                c.subject.clone(),
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
                // One seat: a betting stage has one seat to act, and
                // `verify_certificate` refuses a kind-1 certificate naming more.
                let seat = subject.subject_seats[0];
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
                    let before = play.round.committed.get(usize::from(seat)).copied().unwrap_or(0);
                    play.round
                        .apply(seat, action)
                        .map_err(|what| Failed::Illegal { seat, what })?;
                    play.actions = play.actions.saturating_add(1);
                    self.acted.push(Acted::after(&play.round, seat, action, before, true));
                }
                // Nothing has failed, so the certificate has had its effect and
                // the roster effects go with it.
                self.commit_certificate(&subject);
                // The stage is closed by the **certificate** stage's own hash,
                // because the betting stage it was about has none: nobody
                // wrote it. The two are at one sequence in two classes, which
                // is what `event_class` is for.
                self.slot = self.slot.then(parent);
                self.mark_stage(now_ms);
                self.acted_for = Some((seat, action));
                // `S1-FP`: the next turn is given now, by the table's word. The
                // stamp it began at was the last signed action's -- the silent
                // seat's own turn given, a whole clock and the vote ago -- and
                // `D-034`'s clock gave the next seat its half-second floor: its
                // own client folded for it and sat it out (the owner's game,
                // 2026-09-15, facing an all-in).
                self.last_stamp_ms = now_ms;
                self.after_action(seat, key, now_ms)
            }
            // A cryptographic deadline. The hand ends and the seat is named —
            // **as evidence only**. No chips move: an abort restores every
            // stack, and D-010 forbids reading `attributed` to move one.
            _ => {
                self.commit_certificate(&subject);
                self.slot = self.slot.then(parent);
                self.mark_stage(now_ms);
                let named = self.keys_of(&subject.subject_seats);
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
        named: Vec<[u8; 32]>,
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
        // D-036: every seat the certificate names, in seat order.
        if let Some(h) = cert_hash.filter(|_| !named.is_empty()) {
            body.attributed = named;
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

    /// `S1-CF`'s note, taken rather than read: the node says it once.
    pub fn take_dealt_note(&mut self) -> Option<String> {
        self.dealt_note.take()
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

    /// `D-045`: the seats the table has certified out of this hand so far,
    /// in the order the certificates completed. Both `required` sets are
    /// fixed for a hand; this is what a certificate moves.
    pub fn certified_seats(&self) -> &[SeatIdx] {
        &self.certified
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

    /// `D-058`: how a seat's return vote stands -- subject, held, needed --
    /// taken rather than read, as `take_tally`.
    pub fn take_return_tally(&mut self) -> Option<(SeatIdx, usize, usize)> {
        self.return_tally.take()
    }

    /// `S1-BB`'s reading: what the carrier held about each seat this client has
    /// just voted about. Taken rather than read; see the field.
    pub fn take_vote_carrier(&mut self) -> Vec<(SeatIdx, bool, bool)> {
        std::mem::take(&mut self.own_vote_carrier)
    }

    /// What a certificate last did, taken rather than read: the node reports
    /// it once and it is not a standing fact about the hand.
    pub fn take_certified_action(&mut self) -> Option<(SeatIdx, Action)> {
        self.acted_for.take()
    }

    /// Every betting action this hand has applied so far, in order. The node
    /// remembers how many it has said and says the rest.
    pub fn acted(&self) -> &[Acted] {
        &self.acted
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
            // **A seat this client removed and the table did not**, signing
            // this hand at the genesis the table is using. Recorded rather than
            // dropped: it is the proof that this client's roster is a strict
            // subset of the table's, which is the one divergence no certificate
            // can mend (`S1-CE`).
            if let Some(seat) = self
                .seat_of_key(&opened.sender)
                .filter(|s| !self.open.required.contains(s))
            {
                if opened.envelope.previous_event_hash != self.open.genesis {
                    self.foreign_outside.insert(seat);
                }
            }
            if let Some(seat) = self
                .seat_of_key(&opened.sender)
                .filter(|s| self.open.required.contains(s))
            {
                self.foreign_genesis
                    .insert(seat, opened.envelope.previous_event_hash);
                if !self.genesis_said {
                    if let Some((g, seats)) = self.foreign_genesis_named() {
                        self.genesis_said = true;
                        let short = |h: &Hash| -> String {
                            h[..4].iter().map(|b| format!("{b:02x}")).collect()
                        };
                        // **What can and cannot repair it, said honestly.** A
                        // certificate only removes a seat, so it mends this only
                        // while this client's roster is a superset of the
                        // table's. If a seat this client has removed is signing
                        // at the other genesis, the opposite is true and nothing
                        // in this protocol can add it back (`D-024`, `S1-CE`).
                        let outside: Vec<SeatIdx> =
                            self.foreign_outside.iter().copied().collect();
                        let road = if outside.is_empty() {
                            format!(
                                "A certificate about hand #{} would repair it",
                                self.open.hand_id.saturating_sub(1)
                            )
                        } else {
                            format!(
                                "seat(s) {outside:?} are at that genesis and this client has \
                                 removed them, so its roster is a strict subset of the table's \
                                 and NO certificate can repair it: a certificate only removes"
                            )
                        };
                        self.genesis_note = Some(format!(
                            "hand #{}: seat(s) {:?} opened it at genesis {}; this client opened it at {} \
                             and nobody has been heard at that genesis. {road}",
                            self.open.hand_id,
                            seats,
                            short(&g),
                            short(&self.open.genesis),
                        ));
                    }
                }
            }
        }
        // Bounded: this is fed from the network, and everything fed from the
        // network is bounded where it is consumed.
        //
        // **The highest sequence goes, not the oldest arrival, and that is the
        // whole of `S1-CD`'s receiver half.** This was `pop_front`, which for a
        // client walking stages upward evicts the run immediately above its
        // cursor — precisely the frames it is about to need — and guarantees the
        // replay stalls again at the next hole. Measured in `split135059-9`: a
        // bystander stuck at stage 18 held this queue **saturated at 64** from
        // 510 s to the end of the run while the frame it needed sat one
        // sequence above its cursor.
        //
        // An arrival that is itself the highest is dropped rather than making
        // room, for the same reason. `Holding::Kept` is still the answer, as it
        // already is for a byte-duplicate above: it means the event was
        // verified and answered — and it is what the relay decision reads — not
        // that a slot was spent on it.
        // **Refused at the door for being larger than its own type allows.**
        // Nothing between the wire and this queue bounds a frame by its type:
        // `open_in_hand` above was given `FRAME_CAP`, so without this a
        // `DECK_COMMIT` whose cap is 160 bytes could sit here occupying 16 384.
        if bytes.len() > frame_ceiling(kind) {
            return Holding::Malformed;
        }
        let held_bytes: usize = self.early.iter().map(Vec::len).sum();
        if self.early.len() >= EARLY_CAP || held_bytes + bytes.len() > EARLY_BYTES {
            let highest = self
                .early
                .iter()
                .enumerate()
                .filter_map(|(i, b)| {
                    chained::peek(b, PEEK_CAP).ok().map(|(_, _, q)| (i, q))
                })
                .max_by_key(|(_, q)| *q);
            match highest {
                Some((i, q)) if q > opened.envelope.sequence => {
                    self.early.remove(i);
                }
                _ => return Holding::Kept,
            }
            // **A loop and not an `if`.** Giving up one 544-byte `DECK_COMMIT`
            // does not make room for an 8 704-byte `SHUFFLE_PROOF`, and the
            // bound that binds is now the byte one. It terminates: every pass
            // removes an entry and `max_by_key` answers `None` on an empty
            // queue.
            while self.early.len() >= EARLY_CAP
                || self.early.iter().map(Vec::len).sum::<usize>() + bytes.len() > EARLY_BYTES
            {
                let next = self
                    .early
                    .iter()
                    .enumerate()
                    .filter_map(|(i, b)| chained::peek(b, PEEK_CAP).ok().map(|(_, _, q)| (i, q)))
                    .max_by_key(|(_, q)| *q);
                match next {
                    Some((i, q)) if q > opened.envelope.sequence => {
                        self.early.remove(i);
                    }
                    _ => return Holding::Kept,
                }
            }
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

    /// The oldest frame held, for a node that wants to say what it is
    /// waiting on (`S1-CR`: an adopted hand that held everything).
    pub fn first_held(&self) -> Option<&[u8]> {
        self.early.front().map(|b| b.as_slice())
    }

    /// Whether stage 0 is complete: every required seat heard and agreed.
    ///
    /// The name is the player's word for it, and it is now true.
    ///
    /// **It used to read `!matches!(self.phase, Phase::Init(_))`, which is a
    /// different question** -- *has the hand left `Init` by any route* -- and
    /// one of those routes is `Phase::Aborted`. So a hand that died at stage 0
    /// answered yes, and both readers of this function believe the sentence
    /// above rather than the code: the player was told *hand #8 has begun*
    /// three milliseconds before *hand #8 is over*
    /// (`split092359-10/far-n1.log`), and `cross_boundary_at_t47` quotes this
    /// comment as T47's condition before shutting every 4.10 window, so hand
    /// `k`'s window closed when hand `k+1` **aborted**.
    ///
    /// It was not cosmetic: a corpus fold that reads *has begun* as *this
    /// branch left stage 0* finds seven forks in which two branches both
    /// advanced -- the exact state `S1-CE`'s theorem says cannot exist -- out
    /// of hands that never dealt a card. `tools/fold-forks.py` keeps
    /// `--count-aborts-as-advanced` so that stays demonstrable.
    pub fn dealt(&self) -> bool {
        self.stage_zero_done
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
            began_unix_ms: self.turn_began_unix_ms,
            shown_unix_ms: self.turn_heard_ms.max(self.taken_up_ms),
            taken_up: self.taken_up_ms != 0 && self.turn_heard_ms <= self.taken_up_ms,
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

    /// What each seat has in front of it this street, by seat (`S1-CS`).
    pub fn bets(&self) -> Vec<Chips> {
        match &self.phase {
            Phase::Playing { play, .. } => play.round.committed.clone(),
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

    /// `D-050`: hold this client's muck at the showdown for its player.
    pub fn hold_muck_for_the_player(&mut self, on: bool) {
        self.hold_muck = on;
    }

    /// `D-050`: whether this client holds its muck for a player at all.
    pub fn holds_muck(&self) -> bool {
        self.hold_muck
    }

    /// `D-050`: until when, on this client's clock, its hand waits at the
    /// showdown for its player's word -- it may muck, and may be shown instead.
    pub fn muck_held_until(&self) -> Option<u64> {
        self.muck_held_until_ms.filter(|_| self.showing())
    }

    /// `D-050`: show the waiting hand instead of mucking it.
    pub fn show_held(&mut self, key: &SigningKey, now_ms: u64) -> Result<Vec<Send>, Failed> {
        if self.muck_held_until().is_none() {
            return Ok(Vec::new());
        }
        self.muck_held_until_ms = None;
        self.show(key, now_ms)
    }

    /// `D-050`: muck the waiting hand -- its player's window is up.
    pub fn muck_held_now(&mut self, key: &SigningKey, now_ms: u64) -> Result<Vec<Send>, Failed> {
        if self.muck_held_until().is_none() {
            return Ok(Vec::new());
        }
        self.muck_held_until_ms = None;
        self.muck(key, now_ms)
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
        // **`TERMINAL(k)` comes from one place**, [`Hand::terminal`], which
        // §4.10's boundary window also opens at. The two used to derive it
        // separately and came apart on the abort-settle race; the match below
        // now answers only *what everybody holds*.
        let terminal = self.terminal()?;
        let stacks: Vec<Chips> = match &self.phase {
            // The settlement has been applied, so this is what everybody holds.
            Phase::Playing { play, .. } if matches!(play.step, Step::Ended) => {
                play.round.stack.clone()
            }
            // A settlement that arrived after this client gave up. §4.10:
            // `HAND_COMPLETE` wins over an abort, so the stacks are the
            // settlement's and not the abort's.
            Phase::Aborted(_) if self.late.as_ref().is_some_and(|l| l.closed.is_some()) => self
                .late
                .as_ref()
                .and_then(|l| l.closed.clone())
                .expect("checked in the guard")
                .1,
            // An abort moves no chips: every seat ends the hand with what it
            // started it with (D-010, I27), and those are the values already
            // bound into `roster_hash(k)`.
            Phase::Aborted(_) => self.mine.stacks.clone(),
            // Unreachable: `terminal()` above returns `None` on every other
            // phase and this function has already left. Kept so the match is
            // exhaustive without a wildcard that could swallow a new phase.
            _ => return None,
        };
        self.next_hand_with(terminal, stacks)
    }

    /// `S1-FL`: `R(k+1)` as [`Hand::next_hand_with`] derives it, read alone --
    /// the seats that play on from this boundary -- for the places below. A
    /// mirror, and tested to agree with the derivation whenever there is a next
    /// hand: the derivation also folds the bank forward and is left as it is.
    fn required_next(&self, end: &[Chips]) -> Vec<SeatIdx> {
        let alive = |s: SeatIdx| end.get(usize::from(s)).copied().unwrap_or(0) > 0;
        let mut required: Vec<SeatIdx> = self
            .open
            .required
            .iter()
            .copied()
            .filter(|s| self.took_part(*s) && alive(*s))
            .collect();
        if self.open.required.len() == 2 {
            required = self.open.required.iter().copied().filter(|s| alive(*s)).collect();
        }
        for seat in &self.returned {
            if alive(*seat) && !required.contains(seat) {
                required.push(*seat);
            }
        }
        required.sort_unstable();
        required.dedup();
        required
    }

    /// The stacks at this boundary as the next hand reads them: settled or
    /// restored, and a seat out of the table for good holds nothing (`D-047`).
    fn end_stacks_at_boundary(&self) -> Vec<Chips> {
        let mut end = self.boundary_stacks();
        for seat in self.out_for_good() {
            if let Some(e) = end.get_mut(usize::from(seat)) {
                *e = 0;
            }
        }
        end
    }

    /// `S1-FL`: what this boundary decided about the tournament -- whether it
    /// is over, and the place of every seat that finished at it: out of chips,
    /// out of the table for good, or the winner. From figures every peer holds
    /// alike: the stacks the hand began with (bound into its roster hash), the
    /// stacks it ended with, and `R(k+1)`. See [`place_seats`] for the rule.
    pub fn finishes_at_boundary(&self) -> (bool, Vec<SeatFinish>) {
        let end = self.end_stacks_at_boundary();
        let occupied: Vec<SeatIdx> = self.open.seats.iter().map(|(s, _, _)| *s).collect();
        let playing_on = self.required_next(&end);
        place_seats(&occupied, &self.mine.stacks, &end, &playing_on)
    }

    /// `S1-FM`: whether the table's first hand is still opening inside the
    /// time a seat is given to join the table's group: the stage's own budget
    /// plus [`FIRST_HAND_JOIN_ALLOWANCE_MS`]. While it is, nobody votes about a
    /// seat the opening waits on; `and_then_ms` extends the same window for
    /// the local abort, which gives a certificate its chance first.
    pub fn first_hand_opening_held(&self, now_ms: u64, and_then_ms: u64) -> bool {
        // `S1-GN`: the allowance is for a seat still joining the group, never for
        // one whose player said it left: an opening that waits on such seats alone
        // is not held (`churn191947-10`: a player gone a second after the set held
        // the table's first hand for two minutes).
        let waiting = self.waiting_for();
        let only_the_gone = !waiting.is_empty() && waiting.iter().all(|s| self.gone_by_word.contains(s));
        self.open.hand_id == 1
            && matches!(self.phase, Phase::Init(_))
            && !only_the_gone
            && now_ms.saturating_sub(self.stage_at_ms)
                < u64::from(self.open.crypto_step_timeout_ms)
                    .saturating_add(FIRST_HAND_JOIN_ALLOWANCE_MS)
                    .saturating_add(and_then_ms)
    }

    /// `D-059`: how many times each seat has made this table wait, as the node
    /// keeps them for the table, indexed by seat.
    pub fn set_patience(&mut self, waits: &[u8]) {
        self.patience = waits.to_vec();
    }

    /// `D-059`: the table's cryptographic step, in milliseconds.
    pub fn crypto_step_ms(&self) -> u64 {
        u64::from(self.open.crypto_step_timeout_ms)
    }

    /// `D-059`: the stall at the open stage, if the stage waits on any seat but
    /// this client's own: its key, the seats it already counts as a wait for,
    /// and whether a wait may count here -- see [`Stall`].
    pub fn stall_now(&self, now_ms: u64) -> Option<Stall> {
        if self.over() {
            return None;
        }
        self.owed_type()?;
        let seats: Vec<SeatIdx> = self
            .waiting_for()
            .into_iter()
            .filter(|s| *s != self.open.my_seat && !self.certified.contains(s))
            .collect();
        if seats.is_empty() {
            return None;
        }
        let stood = if self.crypto_stage() {
            now_ms.saturating_sub(self.stage_at_ms) >= crate::protocol::constants::WAIT_FROM_MS
        } else {
            self.past_stage_deadline(now_ms)
        };
        Some(Stall {
            key: (self.open.hand_id, self.stage_seq),
            waits: if stood { seats } else { Vec::new() },
            countable: !self.first_hand_opening_held(now_ms, 0) && self.certificate_possible(),
        })
    }

    /// `D-059`: how long this client waits at the open stage before voting about
    /// `seat`: at a cryptographic step, `patience_ms` of the stage's budget for
    /// the waits counted before -- a stall is counted once it is over, so never
    /// this one; at a turn, the budget -- the owner's choice, a player's time to
    /// decide is never cut.
    fn vote_after_ms(&self, seat: SeatIdx, owed: EventType) -> u64 {
        // `S1-GK`: a seat whose player said it left, and whose client left the
        // table's group, has nothing more to say at any stage or turn.
        if self.gone_by_word.contains(&seat) {
            return 0;
        }
        let budget = u64::from(self.next_deadline_for(owed));
        if !self.crypto_stage() {
            return budget;
        }
        patience_ms(budget, self.patience.get(usize::from(seat)).copied().unwrap_or(0))
    }

    /// `S1-FL`: the places of [`Hand::finishes_at_boundary`], if nothing still
    /// to come at this boundary can change them -- which is every boundary but
    /// one: fewer than two seats play on while a seat holding chips sits
    /// outside the roster, whose return, banked in the boundary's window, would
    /// keep the tournament going. So the places are said the moment the hand is
    /// over and not after the showdown's pause and the return hold (the owner,
    /// 2026-09-15: *the window must come as soon as I am out*).
    pub fn finishes_final_at_the_end(&self) -> Option<(bool, Vec<SeatFinish>)> {
        let end = self.end_stacks_at_boundary();
        let occupied: Vec<SeatIdx> = self.open.seats.iter().map(|(s, _, _)| *s).collect();
        let playing_on = self.required_next(&end);
        places_are_final(&occupied, &end, &playing_on).then(|| self.finishes_at_boundary())
    }

    /// `S1-CX`: the genesis the next hand would open at if this hand were
    /// given up now -- an abort's terminal is a function of this hand's
    /// genesis and moves no chips, so the answer is known before the fact.
    /// `None` once the hand is over: `next_hand` answers then.
    pub fn genesis_if_given_up(&self) -> Option<Hash> {
        if self.over() {
            return None;
        }
        let terminal = crate::protocol::transcript::abort_terminal(
            &self.open.table_id,
            self.open.hand_id,
            &self.open.genesis,
        );
        self.next_hand_with(terminal, self.mine.stacks.clone()).map(|o| o.genesis)
    }

    /// Hand `k+1` from a terminal and the stacks everybody holds at it.
    fn next_hand_with(&self, terminal: Hash, stacks: Vec<Chips>) -> Option<Opening> {
        // `D-047`: a seat certified absent with `MAX_RETURNS` returns behind it
        // is out of the table for good. Its chips leave the table here, so it
        // is busted in every rule below -- not dealt in, no blinds, no return
        // (a busted seat cannot return) -- and every seat computes the same
        // from the same certificates: nothing new is said on the wire.
        let out = self.out_for_good();
        let mut stacks = stacks;
        for seat in &out {
            if let Some(s) = stacks.get_mut(usize::from(*seat)) {
                *s = 0;
            }
        }
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
        let mut returns = self.open.returns.clone();
        returns.resize(n, 0);
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
            // certified out again, for ever. Filtering `R(k)` in both branches
            // closes it: the roster is monotone between certificates, whichever
            // branch derives it, and grows only by `S1-BM`'s return certificate
            // below (D-028), which is unanimous and carries its own evidence.
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
        // **`S1-BM`: `∪ IN(k)`.** The subjects of complete return certificates
        // banked at this boundary join the roster here and nowhere else, each
        // with its allowance refilled: `open_with` deals in `required` filtered
        // by `grace > 0`, and a seat back in `R(k+1)` with a spent allowance
        // would be required and not dealt in -- correction 4 of READMISSION.md.
        // `∩ ALIVE` applies to them as to everybody.
        let mut required = required;
        // `S1-CX`: at two seats there is no table without both, and no
        // certificate to say a seat is absent (D-007). Both seats with chips
        // stay required, whatever this hand heard from them: a seat that
        // stopped is waited for at stage 0, the hand is given up on the
        // stage's budget and opened again, and a client that comes back
        // finds a hand to sign. Measured before this: one missed hand and
        // the survivor had *no next hand to deal* (`run080025-2`).
        if self.open.required.len() == 2 {
            required = self
                .open
                .required
                .iter()
                .copied()
                .filter(|s| alive.get(usize::from(*s)).copied().unwrap_or(false))
                .collect();
            for s in &required {
                let i = usize::from(*s);
                if let Some(g) = grace.get_mut(i) {
                    *g = GRACE_HANDS;
                }
                if let Some(p) = present_run.get_mut(i) {
                    *p = 0;
                }
            }
        }
        for seat in &self.returned {
            let s = usize::from(*seat);
            // `D-032`: a return is counted whether or not it changes the roster.
            if let Some(r) = returns.get_mut(s) {
                *r = r.saturating_add(1);
            }
            if alive.get(s).copied().unwrap_or(false) && !required.contains(seat) {
                required.push(*seat);
                if let Some(g) = grace.get_mut(s) {
                    *g = GRACE_HANDS;
                }
                if let Some(p) = present_run.get_mut(s) {
                    *p = 0;
                }
            }
        }
        required.sort_unstable();
        required.dedup();
        if required.len() < 2 {
            return None;
        }

        // **`S1-CF`'s invariant, written down where it is produced.**
        //
        // `open_with` filters `dealt_in` by `grace[s] > 0` **on the indices of
        // `required`**, and `GENESIS(k+1)` commits `required` and the stacks and
        // **not** `grace` — so if `grace` could vary for a required seat, two
        // honest peers could agree on the genesis and disagree about who is
        // playing, which is a hard refusal at stage 0 that no genesis-based
        // detector can see.
        //
        // It cannot vary, and the reason is four steps: the two limbs that
        // LOWER `grace` are `saturating_sub(1)`, which is the `else` of the very
        // `took_part` this function filters `required` by, and the
        // `MAX_CONSECUTIVE_AUTO_ACTIONS` gate, which needs `strikes[s] >= 1`,
        // which is only ever written beside a `certified.push` of the same seat
        // — and `certified` makes `took_part` false in the branch where a
        // certificate can exist at all. So every seat this loop can lower
        // `grace` for is a seat the filter below removes.
        //
        // **Asserted rather than trusted**, because the invariant is what keeps
        // the unhashed term inert, nothing else states it, and the day
        // `S1-BM`'s return certificate puts a seat back INTO `required` it
        // arrives carrying whatever `grace` it accrued while it was out.
        debug_assert!(
            required
                .iter()
                .all(|s| grace.get(usize::from(*s)).copied().unwrap_or(0) == GRACE_HANDS),
            "S1-CF: a required seat with grace below GRACE_HANDS makes dealt_in \
             diverge under one genesis. required {required:?} grace {grace:?}"
        );

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
            returns,
            out,
            button: Some(positions.button),
        })
    }

    /// How long this client may take on its own turn before it acts for its
    /// owner: **`action_timeout_ms` plus what is left of the reserve**, the
    /// player's own clock and the one the window shows.
    ///
    /// The grace is the network's share on top of it (D-034) and belongs to
    /// the OTHER seats' clocks, where `next_deadline_for` adds it so that a
    /// round trip does not read as a seat that did not act. It used to be
    /// added here as well, so this client's own check or fold went out three
    /// seconds after the clock its owner was watching had run out -- and,
    /// since the others' clocks started a delivery earlier, it raced their
    /// vote instead of landing before it. The owner's question at the window,
    /// 2026-09-11 evening: *every seat folds for itself at its own thirty
    /// seconds; the others force it only if that does not come.*
    pub fn action_deadline(&self) -> std::time::Duration {
        std::time::Duration::from_millis(
            u64::from(self.open.action_timeout_ms).saturating_add(u64::from(self.bank_left_ms)),
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

    /// `D-033`: the table's frames are replayed; whatever the current stage
    /// still wants from this seat is made now, and from here on the hand is
    /// played like any other. Nothing to do for a hand that was not restoring.
    pub fn restore_done(&mut self, key: &SigningKey, now_ms: u64) -> Result<Vec<Send>, Failed> {
        if !self.restoring {
            return Ok(Vec::new());
        }
        self.restoring = false;
        // `S1-FS`: from here this client can show the hand's turns to its player.
        self.taken_up_ms = now_ms;
        let which = match &self.phase {
            Phase::Deck { .. } => 1,
            Phase::Shuffling { .. } => 2,
            Phase::Committing { .. } => 3,
            Phase::Dealing { .. } => 4,
            Phase::Playing { play, .. } => match &play.step {
                Step::Opening { .. } => 5,
                Step::Showdown { .. } => 6,
                _ => 0,
            },
            _ => 0,
        };
        match which {
            1 => self.deck_mine(key, now_ms),
            2 => self.shuffle_if_mine(key, now_ms),
            3 => self.commit_mine(key, now_ms),
            4 => self.deal_mine(key, now_ms),
            5 => self.board_mine(key, now_ms),
            6 => self.speak_at_showdown(key, now_ms),
            _ => Ok(Vec::new()),
        }
    }

    /// `D-033`: every frame this hand accepted from the wire, in the order
    /// accepted. This client's own frames are the node's (`said`).
    pub fn transcript(&self) -> &[Vec<u8>] {
        &self.transcript
    }

    /// `D-033`: whether this hand is still taking this seat's frames from the wire.
    pub fn is_restoring(&self) -> bool {
        self.restoring
    }

    /// `D-033`: whether this seat can play the hand out -- it holds the hand's
    /// secret -- or can only follow it and fold.
    pub fn can_play_on(&self) -> bool {
        !self.fold_only
    }

    /// This seat's own deck key, made and said now: a restored hand whose deck
    /// stage still waits for this seat never had its previous life's key, so a
    /// fresh one is as good as any. The stage closes here if this was the last
    /// key owed.
    fn deck_mine(&mut self, key: &SigningKey, now_ms: u64) -> Result<Vec<Send>, Failed> {
        let me = self.open.my_seat;
        let heard = match &self.phase {
            Phase::Deck { stage, .. } => stage.heard(me).is_some(),
            _ => return Ok(Vec::new()),
        };
        if heard || !self.mine.dealt_in.contains(&me) {
            return Ok(Vec::new());
        }
        let me_key = self.open.seats[self.seat_index()].1;
        let ctx = self.deck_ctx(&me_key);
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
        let hash = self.opened(&bytes, EventType::DeckInit)?.event_hash;
        let complete = {
            let Phase::Deck {
                stage,
                keys,
                by_seat,
                secret: held,
            } = &mut self.phase
            else {
                return Err(Failed::NothingFurther);
            };
            let own = HandDeck::verify_key(wire_key, &proof, keys, &ctx).map_err(|_| Failed::BadKey {
                seat: me,
                why: "this client's own proof did not verify",
            })?;
            *held = secret;
            keys.push(own);
            if let Some(slot) = by_seat.get_mut(usize::from(me)) {
                *slot = Some(own);
            }
            stage.hear(me, hash);
            stage.complete()
        };
        // A key of this seat's own making: the hand can be played out.
        self.fold_only = false;
        let mut out = vec![Send::Broadcast(bytes)];
        if complete {
            let parent = {
                let Phase::Deck { stage, .. } = &self.phase else {
                    return Err(Failed::NothingFurther);
                };
                stage.hash().expect("a complete stage has one")
            };
            self.slot = self.slot.then(parent);
            out.append(&mut self.begin_shuffle(key, now_ms)?);
        }
        Ok(out)
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
    /// `D-034`: when this seat was given the turn, on the giver's clock
    /// (unix ms). Zero when unknown.
    pub began_unix_ms: u64,
    /// `S1-FS`: when this client could first show the turn to its player --
    /// when it applied the event that gave it, or took the hand up again after
    /// a restart, whichever is later -- on this client's own clock (unix ms).
    pub shown_unix_ms: u64,
    /// `S1-FS`: the turn already stood when this client took the hand up again
    /// after a restart (`D-033`).
    pub taken_up: bool,
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
    subject: CertSubject,
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


// ---------------------------------------------------------------------------
// `S1-BM`: the return certificate -- the grow side of the roster
// ---------------------------------------------------------------------------

/// What the node hands the hand about a seat that asked to sit in at this
/// boundary: the subject's own two signed events. The hand opens both itself
/// and believes neither until it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnEvidence {
    pub seat: SeatIdx,
    /// The subject's signed `PLAYER_SIT_IN`, sealed in §4.10's window at the
    /// subject's own slot, parented on `TERMINAL(k)`.
    pub request: Vec<u8>,
    /// The subject's signed checkpoint-8 `STATE_HASH` of hand `k`.
    pub checkpoint: Vec<u8>,
}

/// A return certificate this client verified, vote by vote and evidence by
/// evidence.
struct VerifiedReturn {
    event_hash: Hash,
    emitter: SeatIdx,
    subject: ReturnVote,
    voters: BTreeSet<SeatIdx>,
}

impl Hand {
    /// `R(k) \ OUT(k)`: the seats that would have to wait for a returning one,
    /// which is who a return certificate needs.
    ///
    /// **Written fresh, and it reads neither `dealt_in` nor `grace`**
    /// (`READMISSION.md` §5 correction 1 and §6's first trap). `voters()`
    /// reads `dealt_in`, which reads `grace`, which is a private per-receiver
    /// accumulator; imported here it would decide `participants` and so the
    /// genesis, pass every unit test because `grace` is uniform on a healthy
    /// table, and fork the first run where one seat spent a unit the others
    /// did not see it spend. `return_voters_is_written_without_dealt_in_or_grace`
    /// reads this function's own text.
    ///
    /// And it is taken **before** any return is added, so two seats returning
    /// at one boundary have one voter set and neither depends on the other.
    pub fn return_voters(&self) -> Vec<SeatIdx> {
        let mut v: Vec<SeatIdx> = self
            .open
            .required
            .iter()
            .copied()
            .filter(|s| !self.certified.contains(s))
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// `IN(k)`: the subjects of complete return certificates banked at this
    /// boundary, ascending.
    /// `D-032`: how many times each seat has come back, indexed by seat.
    pub fn returns(&self) -> &[u8] {
        &self.open.returns
    }

    /// `D-047`: the seats out of the table for good -- certified absent with
    /// `MAX_RETURNS` returns behind them, this hand or before. Their chips
    /// leave the table at the next boundary; the node removes them from the
    /// table's group for good and a client that finds itself here leaves.
    /// `D-047`: the certificate that took this seat out this hand -- the
    /// latest one naming it -- as bytes, for this client's lobby answers.
    /// From the bank, not the transcript: a certificate never enters the
    /// transcript (run193937-3 kept no word and told nobody).
    pub fn word_about(&self, seat: SeatIdx) -> Option<Vec<u8>> {
        self.words.get(&seat).cloned()
    }

    pub fn out_for_good(&self) -> Vec<SeatIdx> {
        let mut out = self.open.out.clone();
        for seat in &self.certified {
            let came_back = self.open.returns.get(usize::from(*seat)).copied().unwrap_or(0);
            // `D-051`: or certified with the flood cause by every voter.
            let flooded = self.flood_named.contains(seat);
            // `D-063`: or named with its player's own word that it left.
            let resigned = self.resigned.contains(seat);
            if (came_back >= crate::protocol::constants::MAX_RETURNS || flooded || resigned) && !out.contains(seat) {
                out.push(*seat);
            }
        }
        out.sort_unstable();
        out
    }

    /// `D-052`: the pots this hand settled into -- the main pot first, then
    /// each side pot -- with what each held and which seats took it. Empty
    /// until the settlement is applied, and on the aborted road, which moves
    /// no chip.
    pub fn settled_pots(&self) -> &[(Chips, Vec<SeatIdx>)] {
        &self.settled_pots
    }

    /// `S1-ER`: what the settlement moved to each seat, by seat -- the chips
    /// that fly to it, the figure in the table's log and the mark of a winner.
    ///
    /// Empty until a settlement is applied, and on the aborted road, which
    /// restores every stack and moves nothing.
    pub fn settled_gain(&self) -> &[Chips] {
        &self.settled_gain
    }

    /// `D-051`: whether this hand's certificate named the seat with the flood
    /// cause -- out of the table for good for flooding its carrier group, as
    /// against `D-047`'s fourth absence.
    pub fn named_for_flooding(&self, seat: SeatIdx) -> bool {
        self.flood_named.contains(&seat) && self.certified.contains(&seat)
    }

    /// `D-051`: the seats this client cut off for flooding the table's carrier
    /// group. Its votes about them carry the flood cause from the next vote
    /// on; a seat once said stays said for the hand.
    /// `S1-GK`: the seats whose player left the table by its own signed word
    /// and whose client is out of the table's group, as the node reads them
    /// now -- the whole set each time, so a seat back in the group is waited
    /// for again.
    pub fn note_gone_by_their_word(&mut self, seats: &[SeatIdx]) {
        self.gone_by_word = seats.iter().copied().filter(|s| *s != self.open.my_seat).collect();
    }

    /// `D-063`: the signed words of the players that left, as the node holds
    /// them -- the whole set each time; kept only where the word is the seat's
    /// own for this table, checked here as a certificate's reader checks it.
    /// A word a peer's certificate carried stays.
    pub fn note_leave_words(&mut self, words: &[(SeatIdx, Vec<u8>)]) {
        for (seat, word) in words {
            if *seat == self.open.my_seat || self.leave_words.contains_key(seat) {
                continue;
            }
            let Ok((who, _)) = crate::net::tabletalk::verify_leave_word(word, &self.open.table_id) else {
                continue;
            };
            if self.open.seats.iter().any(|(s, k, _)| s == seat && *k == who) {
                self.leave_words.insert(*seat, word.clone());
            }
        }
    }

    /// `S1-HA`: the seats out of the table's group, or silent there for
    /// `QUIET_LIMIT_S`, as the node reads them now -- the whole set each time.
    pub fn note_gone_from_group(&mut self, seats: &[SeatIdx]) {
        self.gone_from_group = seats.iter().copied().filter(|s| *s != self.open.my_seat).collect();
    }

    /// `S1-HA`: whether a voter of the certificate this hand waits on is gone
    /// from the table's group. A vote round needs every voter's vote; a voter
    /// that acted at the stage and then lost its line is quiet at no stage and
    /// so is named by nobody, and the round stood until the hand's budget ran
    /// out -- one hand of six seats lasted 62 s and the next 92 s after the
    /// founder and a far seat were cut together (`churn134826-6`). The hand
    /// ends at the deadline instead, and the next hand names both.
    fn a_voter_is_gone(&self) -> bool {
        let quiet: Vec<SeatIdx> = self
            .waiting_for()
            .into_iter()
            .filter(|s| *s != self.open.my_seat && self.mine.dealt_in.contains(s))
            .collect();
        !quiet.is_empty() && self.voters_of(&quiet).iter().any(|v| self.gone_from_group.contains(v))
    }

    pub fn note_flooders(&mut self, seats: &[SeatIdx]) {
        for seat in seats {
            if *seat != self.open.my_seat {
                self.flooders.insert(*seat);
            }
        }
    }

    pub fn returned(&self) -> &[SeatIdx] {
        &self.returned
    }

    /// Whether this client has asked to sit in at this boundary.
    pub fn asked_to_sit_in(&self) -> bool {
        self.sit_in_asked
    }

    /// The seats this hand deals cards to. Exposed for the tests that pin
    /// what a returned seat is at hand `k+1`: required AND dealt in.
    pub fn dealt_in(&self) -> &[SeatIdx] {
        &self.mine.dealt_in
    }

    /// The chain fact a return rests on: `TERMINAL(k)` is the `HAND_COMPLETE`
    /// stage hash. **Not** `checkpoint8.is_some()` -- correction 2: a peer that
    /// reached the settlement by the late road holds the settlement's terminal
    /// and no checkpoint of its own, and refusing it would be the permanent
    /// fork with no dissent and no attacker.
    fn settled_terminal(&self) -> Option<Hash> {
        if self.settled() {
            self.terminal()
        } else {
            None
        }
    }

    /// The stacks at this boundary, which are what `next_hand` opens `k+1` on.
    fn boundary_stacks(&self) -> Vec<Chips> {
        match &self.phase {
            Phase::Playing { play, .. } if matches!(play.step, Step::Ended) => {
                play.round.stack.clone()
            }
            Phase::Aborted(_) => self
                .late
                .as_ref()
                .and_then(|l| l.closed.as_ref())
                .map(|(_, s)| s.clone())
                .unwrap_or_else(|| self.mine.stacks.clone()),
            _ => self.mine.stacks.clone(),
        }
    }

    /// A seat's stack at this boundary, on both terminal paths: the settled
    /// stacks after a settlement, the start-of-hand stacks after an abort
    /// (every stack restored, D-010). `stacks()` answers only while a hand is
    /// being played, and reading it at an aborted boundary read zero -- which
    /// `S1-CR`'s record took for a bust and forgot the session on
    /// (`run193358-3`).
    pub fn stack_at_boundary(&self, seat: SeatIdx) -> Chips {
        self.boundary_stack_of(seat)
    }

    fn boundary_stack_of(&self, seat: SeatIdx) -> Chips {
        self.boundary_stacks().get(usize::from(seat)).copied().unwrap_or(0)
    }

    fn occupies_a_seat(&self, seat: SeatIdx) -> bool {
        self.open.seats.iter().any(|(s, _, _)| *s == seat)
    }

    /// Whether this client should ask to be dealt back in at this boundary:
    /// it sits at the table with chips, is outside `R(k)`, and the table
    /// settled -- there is no return at a boundary the table aborted
    /// (correction 5). The automation belongs here, in the client
    /// (`READMISSION.md` §3): the request is still the player's own signed
    /// event, sent at every boundary where this holds, so the player clicks
    /// nothing and sees *sitting in at the next hand*. A client told to stay
    /// out simply does not call `sit_in_request`.
    pub fn may_ask_to_sit_in(&self) -> bool {
        self.settled_terminal().is_some()
            && !self.open.required.contains(&self.open.my_seat)
            && self.occupies_a_seat(self.open.my_seat)
            && self.boundary_stack_of(self.open.my_seat) > 0
            && !self.returned.contains(&self.open.my_seat)
    }

    /// The player's own signed request, once per boundary: `PLAYER_SIT_IN` at
    /// this seat's window slot, parented on `TERMINAL(k)`, carrying the
    /// canonical empty payload. `None` when there is nothing to ask.
    pub fn sit_in_request(
        &mut self,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Option<Vec<u8>>, Failed> {
        if self.sit_in_asked || !self.may_ask_to_sit_in() {
            return Ok(None);
        }
        let terminal = self.settled_terminal().ok_or(Failed::NothingFurther)?;
        let sequence = crate::table::seatwire::window_sequence(self.open.my_seat)
            .ok_or(Failed::NotAtThisTable)?;
        let slot = self.slot().at(sequence, terminal);
        let bytes = chained::seal(
            EventType::PlayerSitIn,
            &slot,
            &Vec::<u16>::new(),
            key,
            now_ms,
            self.next_deadline_for(EventType::PlayerSitIn),
            crate::table::seatwire::BOUNDARY_EVENT_CAP,
        )
        .map_err(Failed::Wire)?;
        self.sit_in_asked = true;
        Ok(Some(bytes))
    }

    /// Open the subject's two signed events against this boundary, as this
    /// client itself: the request at the subject's window slot on `terminal`
    /// with the canonical payload, the checkpoint at round 0's hash slot on
    /// the same terminal, both signed by the key the roster holds for the
    /// seat. `None` when either is not what it claims. Returns the request's
    /// event hash and the checkpoint's value.
    fn open_return_evidence(
        &self,
        seat: SeatIdx,
        request: &[u8],
        checkpoint: &[u8],
        terminal: &Hash,
    ) -> Option<(Hash, Hash)> {
        let key = self
            .open
            .seats
            .iter()
            .find(|(s, _, _)| *s == seat)
            .map(|(_, k, _)| *k)?;
        let sequence = crate::table::seatwire::window_sequence(seat)?;
        let slot = self.slot().at(sequence, *terminal);
        // The frame cap on the signed event, the body cap on the payload.
        let req = chained::open(request, FRAME_CAP, EventType::PlayerSitIn, &slot).ok()?;
        if req.sender != key
            || req.envelope.payload != crate::table::seatwire::SIT_IN_PAYLOAD.to_vec()
        {
            return None;
        }
        let cslot = self
            .slot()
            .at(crate::table::checkwire::hash_sequence(0)?, *terminal);
        let chk = chained::open(checkpoint, FRAME_CAP, EventType::StateHash, &cslot).ok()?;
        if chk.sender != key {
            return None;
        }
        let body: crate::table::checkwire::StateHash =
            chained::payload(&chk, STATE_HASH_CAP).ok()?;
        if body.checkpoint != crate::table::checkwire::BOUNDARY_CHECKPOINT
            || body.transcript_head != *terminal
        {
            return None;
        }
        Some((req.event_hash, body.state_hash))
    }

    /// Vote for every seat whose evidence the node handed over and holds up:
    /// the subject is at the table with chips and outside `R(k)`, its request
    /// and checkpoint open as the subject's own at this boundary, and its
    /// checkpoint value **is this client's own**. Then seal whatever set that
    /// completes. A vote does nothing alone.
    pub fn vote_on_returns(
        &mut self,
        evidence: &[ReturnEvidence],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let Some(terminal) = self.settled_terminal() else {
            return Ok(Vec::new());
        };
        // A voter vouches for agreement; without a value of its own it has
        // nothing to vouch with. That is a liveness cost -- the return waits
        // for the next boundary -- never a safety one.
        let Some((own_value, _)) = self.checkpoint8 else {
            return Ok(Vec::new());
        };
        let voters = self.return_voters();
        if voters.len() < 2 || !voters.contains(&self.open.my_seat) {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for ev in evidence {
            let seat = ev.seat;
            if seat == self.open.my_seat
                || self.open.required.contains(&seat)
                || !self.occupies_a_seat(seat)
                || self.boundary_stack_of(seat) == 0
            {
                continue;
            }
            // `D-051`: a seat this client cut off for flooding the table's
            // group gets no vote for a return from it, ever.
            if self.flooders.contains(&seat) {
                if self.return_refused.insert(seat) {
                    self.cert_note.push(format!(
                        "seat {seat} asks to come back, and this client cut it off for flooding the table's group, so it votes for no return of that seat (D-051)"
                    ));
                }
                continue;
            }
            // `D-032`: three returns and no more. A certificate needs every
            // voter, so this one refusal keeps the seat out; said once per hand.
            let came_back = self.open.returns.get(usize::from(seat)).copied().unwrap_or(0);
            if came_back >= crate::protocol::constants::MAX_RETURNS {
                if self.return_refused.insert(seat) {
                    self.cert_note.push(format!(
                        "seat {seat} asks to come back, having come back {came_back} times already; {} returns are the limit (D-032), so this client votes for no further return of that seat",
                        crate::protocol::constants::MAX_RETURNS
                    ));
                }
                continue;
            }
            let Some((request_hash, state_hash)) =
                self.open_return_evidence(seat, &ev.request, &ev.checkpoint, &terminal)
            else {
                continue;
            };
            if state_hash != own_value {
                continue;
            }
            let subject = ReturnVote {
                subject_seat: seat,
                terminal,
                request_hash,
                state_hash,
            };
            let digest = subject.subject_digest();
            if self.return_voted.contains(&digest) {
                continue;
            }
            let Some(sequence) = crate::table::returnwire::return_sequence(seat) else {
                continue;
            };
            let slot = self.slot().at(sequence, terminal);
            let bytes = chained::seal(
                EventType::ReturnVote,
                &slot,
                &subject,
                key,
                now_ms,
                self.next_deadline_for(EventType::ReturnVote),
                RETURN_VOTE_CAP,
            )
            .map_err(Failed::Wire)?;
            self.return_voted.insert(digest);
            self.return_subjects.insert(digest, subject);
            self.return_evidence
                .insert(digest, (ev.request.clone(), ev.checkpoint.clone()));
            self.return_votes
                .entry(digest)
                .or_default()
                .insert(self.open.my_seat, bytes.clone());
            out.push(Send::Broadcast(bytes));
            // The tally, so a run can show who voted and who is owed.
            let held = self.return_votes.get(&digest).map_or(0, |m| m.len());
            self.cert_note.push(format!(
                "return: vote {held}/{} about seat {seat}'s return (mine)",
                self.return_voters().len()
            ));
            self.return_tally = Some((seat, held, self.return_voters().len()));
            out.append(&mut self.certify_returns_if_unanimous(key, now_ms)?);
        }
        Ok(out)
    }

    /// A return vote from a peer.
    fn on_return_vote(
        &mut self,
        bytes: &[u8],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let opened = chained::open_in_hand(
            bytes,
            FRAME_CAP,
            EventType::ReturnVote,
            &self.open.table_id,
            self.open.hand_id,
        )
        .map_err(Failed::Wire)?;
        let voter = self.seat_of(&opened.sender)?;
        let body: ReturnVote = chained::payload(&opened, RETURN_VOTE_CAP).map_err(Failed::Wire)?;
        if body.subject_seat == voter {
            return Err(Failed::Elsewhere {
                seat: voter,
                what: "it were not the subject of its own return vote",
            });
        }
        let Some(sequence) = crate::table::returnwire::return_sequence(body.subject_seat) else {
            return Err(Failed::Elsewhere {
                seat: voter,
                what: "a return were about a seat of this table",
            });
        };
        // **The anti-replay binding, and the voter signed it.** A vote sealed
        // anywhere but the subject's slot on the terminal it names is not a
        // vote about this boundary, for any receiver.
        if opened.envelope.sequence != sequence
            || opened.envelope.previous_event_hash != body.terminal
        {
            return Err(Failed::Elsewhere {
                seat: voter,
                what: "each return vote were sealed at its subject's slot on the terminal it names",
            });
        }
        // Held until this client has a terminal to compare with; refused when
        // that terminal is an abort's -- there is no return at a boundary the
        // table aborted.
        let Some(terminal) = self.terminal() else {
            return Err(Failed::NotYet);
        };
        if !self.settled() {
            return Err(Failed::Elsewhere {
                seat: voter,
                what: "no return at a boundary this client's table aborted",
            });
        }
        if body.terminal != terminal {
            // One step away, not a fault: a peer that settled differently is
            // the boundary checkpoint's business, not this vote's.
            return Err(Failed::NotYet);
        }
        if self.open.required.contains(&body.subject_seat) {
            return Err(Failed::Elsewhere {
                seat: voter,
                what: "the subject were outside the roster",
            });
        }
        if !self.return_voters().contains(&voter) {
            return Err(Failed::NotInThisStage);
        }
        let digest = body.subject_digest();
        self.return_subjects.entry(digest).or_insert(body);
        self.return_votes
            .entry(digest)
            .or_default()
            .insert(voter, bytes.to_vec());
        let held = self.return_votes.get(&digest).map_or(0, |m| m.len());
        self.cert_note.push(format!(
            "return: vote {held}/{} about seat {}'s return (from seat {voter})",
            self.return_voters().len(),
            body.subject_seat
        ));
        self.return_tally = Some((body.subject_seat, held, self.return_voters().len()));
        self.certify_returns_if_unanimous(key, now_ms)
    }

    /// Seal a certificate for every subject this client holds a complete set
    /// for -- every voter, no quorum, no reduction -- and bank it. Its own
    /// copy is its own word: it seals only from votes it holds, never from a
    /// peer's certificate.
    fn certify_returns_if_unanimous(
        &mut self,
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let voters = self.return_voters();
        if voters.len() < 2 {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        let digests: Vec<Hash> = self.return_votes.keys().copied().collect();
        for digest in digests {
            if self.return_sealed.contains(&digest) {
                continue;
            }
            let held: BTreeSet<SeatIdx> = self
                .return_votes
                .get(&digest)
                .map(|m| m.keys().copied().collect())
                .unwrap_or_default();
            if !voters.iter().all(|v| held.contains(v)) {
                continue;
            }
            // Sealing carries the subject's evidence, which this client holds
            // only if it voted on it. A voter that did not vote cannot seal.
            let Some((request, checkpoint)) = self.return_evidence.get(&digest).cloned() else {
                continue;
            };
            let Some(subject) = self.return_subjects.get(&digest).copied() else {
                continue;
            };
            // Ascending by voter seat, which the wire requires and which is
            // what makes one set of votes one byte string.
            let votes: Vec<Vec<u8>> = self
                .return_votes
                .get(&digest)
                .map(|m| m.values().cloned().collect())
                .unwrap_or_default();
            let body = ReturnCert {
                subject_digest: digest,
                votes,
                request,
                checkpoint,
            };
            let sequence = crate::table::returnwire::return_sequence(subject.subject_seat)
                .ok_or(Failed::NotInThisStage)?;
            let slot = self.slot().at(sequence, subject.terminal);
            let bytes = chained::seal(
                EventType::ReturnCert,
                &slot,
                &body,
                key,
                now_ms,
                self.next_deadline_for(EventType::ReturnCert),
                RETURN_CERT_CAP,
            )
            .map_err(Failed::Wire)?;
            let hash = chained::open(&bytes, FRAME_CAP, EventType::ReturnCert, &slot)
                .map_err(Failed::Wire)?
                .event_hash;
            self.return_sealed.insert(digest);
            // Both roads bank: a complete unanimous set IS the certificate.
            self.bank_return(&subject, digest);
            if !self.returning.contains_key(&digest) {
                let stage = Collective::closed(sequence, EventType::ReturnCert.code(), &voters)
                    .ok_or(Failed::NotInThisStage)?;
                self.returning.insert(digest, stage);
            }
            if let Some(stage) = self.returning.get_mut(&digest) {
                stage.hear(self.open.my_seat, hash);
            }
            self.cert_note.push(format!(
                "the table has certified seat {}'s return, unanimously among {:?}",
                subject.subject_seat, voters
            ));
            out.push(Send::Broadcast(bytes));
        }
        Ok(out)
    }

    /// The roster half of a return: a set insert, keyed on the subject digest
    /// so a redelivery counts once. The hand is over by definition at a
    /// boundary, so a bank always says the roster moved: where the node has
    /// already derived hand `k+1` from this hand, that is the settled-path
    /// late-roster repair (`READMISSION.md` §4), the same road `S1-BS` built
    /// for the abort path.
    fn bank_return(&mut self, subject: &ReturnVote, digest: Hash) -> bool {
        if !self.return_banked.insert(digest) {
            return false;
        }
        if !self.returned.contains(&subject.subject_seat) {
            self.returned.push(subject.subject_seat);
            self.returned.sort_unstable();
        }
        self.late_roster = true;
        self.cert_note.push(format!(
            "return: seat {} banked at the boundary; the roster of hand #{} is re-derived",
            subject.subject_seat,
            self.open.hand_id.saturating_add(1)
        ));
        true
    }

    /// Open a return certificate vote by vote, then the subject's own two
    /// events inside it. Nothing here reads what this client may already
    /// believe about the subject: a receiver that never heard the subject
    /// must be able to decide, which is what makes the artefact admissible.
    fn verify_return_certificate(&self, raw: &[u8]) -> Result<VerifiedReturn, Failed> {
        let opened = chained::open_in_hand(
            raw,
            FRAME_CAP,
            EventType::ReturnCert,
            &self.open.table_id,
            self.open.hand_id,
        )
        .map_err(Failed::Wire)?;
        let body: ReturnCert = chained::payload(&opened, RETURN_CERT_CAP).map_err(Failed::Wire)?;
        let max_voters = usize::from(crate::protocol::constants::MAX_SEATS) - 1;
        if body.votes.len() < 2 || body.votes.len() > max_voters {
            return Err(Failed::Elsewhere {
                seat: self.open.my_seat,
                what: "a voter set inside the protocol's bounds",
            });
        }
        let emitter = self.seat_of(&opened.sender)?;
        let mut voters: BTreeSet<SeatIdx> = BTreeSet::new();
        let mut subject: Option<ReturnVote> = None;
        for vote in &body.votes {
            let v = chained::open_in_hand(
                vote,
                FRAME_CAP,
                EventType::ReturnVote,
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
            let named: ReturnVote = chained::payload(&v, RETURN_VOTE_CAP).map_err(Failed::Wire)?;
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
            let Some(sequence) = crate::table::returnwire::return_sequence(named.subject_seat) else {
                return Err(Failed::Elsewhere {
                    seat: emitter,
                    what: "a return were about a seat of this table",
                });
            };
            if v.envelope.sequence != sequence || v.envelope.previous_event_hash != named.terminal {
                return Err(Failed::Elsewhere {
                    seat: emitter,
                    what: "each vote were sealed at the subject's slot on the terminal it names",
                });
            }
        }
        let Some(subject) = subject else {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "a certificate carried the votes it is made of",
            });
        };
        let sequence = crate::table::returnwire::return_sequence(subject.subject_seat)
            .ok_or(Failed::NotInThisStage)?;
        if opened.envelope.sequence != sequence
            || opened.envelope.previous_event_hash != subject.terminal
        {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "the certificate were sealed at the slot its votes name",
            });
        }
        if subject.subject_digest() != body.subject_digest {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "the digest were the one its own votes hash to",
            });
        }
        // The subject's own evidence, opened by this receiver itself.
        let Some((request_hash, state_hash)) = self.open_return_evidence(
            subject.subject_seat,
            &body.request,
            &body.checkpoint,
            &subject.terminal,
        ) else {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "the subject's own signed request and checkpoint were carried",
            });
        };
        if request_hash != subject.request_hash || state_hash != subject.state_hash {
            return Err(Failed::Elsewhere {
                seat: emitter,
                what: "the votes named the evidence the certificate carries",
            });
        }
        if !voters.contains(&emitter) {
            return Err(Failed::NotInThisStage);
        }
        Ok(VerifiedReturn {
            event_hash: opened.event_hash,
            emitter,
            subject,
            voters,
        })
    }

    /// A return certificate from a peer, or this client's own coming back.
    fn on_return_cert(
        &mut self,
        bytes: &[u8],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let c = self.verify_return_certificate(bytes)?;
        let seat = c.emitter;
        // The chain fact, at THIS receiver: held while it has no terminal at
        // all, refused when its terminal is an abort's (correction 5), and
        // held when its settlement is another (the checkpoint's business).
        let Some(terminal) = self.terminal() else {
            return Err(Failed::NotYet);
        };
        if !self.settled() {
            return Err(Failed::Elsewhere {
                seat,
                what: "no return at a boundary this client's table aborted",
            });
        }
        if c.subject.terminal != terminal {
            return Err(Failed::NotYet);
        }
        if self.open.required.contains(&c.subject.subject_seat) {
            return Err(Failed::Elsewhere {
                seat,
                what: "the subject were outside the roster",
            });
        }
        if !self.occupies_a_seat(c.subject.subject_seat)
            || self.boundary_stack_of(c.subject.subject_seat) == 0
        {
            return Err(Failed::Elsewhere {
                seat,
                what: "the subject sat at this table with chips",
            });
        }
        // Correction 2: a value of its own is compared when this client has
        // one, and the voters' unanimous word is enough when it has none.
        if let Some((own, _)) = self.checkpoint8 {
            if own != c.subject.state_hash {
                return Err(Failed::Elsewhere {
                    seat,
                    what: "the subject's checkpoint agreed with this client's",
                });
            }
        }
        let nominal: BTreeSet<SeatIdx> = self.return_voters().into_iter().collect();
        if nominal.len() < 2 || c.voters.len() < 2 {
            return Ok(Vec::new());
        }
        if !c.voters.is_subset(&nominal) {
            return Err(Failed::Elsewhere {
                seat,
                what: "every voter were in the roster less the certified",
            });
        }
        // Checked in the direction that can only tighten: a receiver whose own
        // `certified` is shorter derives a larger voter set, which is its own
        // incompleteness talking, and is held rather than refused.
        if !nominal.is_subset(&c.voters) {
            return Err(Failed::NotYet);
        }
        let digest = c.subject.subject_digest();
        let banked = self.bank_return(&c.subject, digest);
        self.return_subjects.entry(digest).or_insert(c.subject);
        let voters: Vec<SeatIdx> = c.voters.iter().copied().collect();
        let sequence = crate::table::returnwire::return_sequence(c.subject.subject_seat)
            .ok_or(Failed::NotInThisStage)?;
        if !self.returning.contains_key(&digest) {
            let stage = Collective::closed(sequence, EventType::ReturnCert.code(), &voters)
                .ok_or(Failed::NotInThisStage)?;
            self.returning.insert(digest, stage);
        }
        let Some(stage) = self.returning.get_mut(&digest) else {
            unreachable!("just inserted")
        };
        if stage.heard(seat) == Some(c.event_hash) {
            return Ok(Vec::new());
        }
        match stage.hear(seat, c.event_hash) {
            Heard::Counted | Heard::Bystander | Heard::Again => {}
            Heard::Equivocation { .. } => return Err(Failed::Equivocation { seat }),
            Heard::Uninvited => return Err(Failed::NotYet),
        }
        if banked {
            self.cert_note.push(format!(
                "return: seat {} certified back in, unanimously among {:?} (copy from seat {seat})",
                c.subject.subject_seat, voters
            ));
        }
        // A voter that has not sealed its own copy may owe one; it seals only
        // from a complete set of its own.
        self.certify_returns_if_unanimous(key, now_ms)
    }
}


/// `D-047`: what a timeout certificate says, verified from its bytes alone --
/// the table, the hand, the roster's keys -- so a seat's own client can read
/// the word about itself from a hand it never held (its line was down when
/// the table certified it the fourth time). The subjects it names and the
/// seats that voted; `None` for anything that does not verify. The checks
/// are `verify_certificate`'s, less the ones that need the hand.
pub fn certificate_names(
    raw: &[u8],
    table_id: &[u8; 32],
    hand_id: u64,
    seat_of: &dyn Fn(&[u8; 32]) -> Option<SeatIdx>,
) -> Option<(Vec<SeatIdx>, BTreeSet<SeatIdx>)> {
    let opened = chained::open_in_hand(raw, FRAME_CAP, EventType::TimeoutCert, table_id, hand_id).ok()?;
    let body: TimeoutCert = chained::payload(&opened, TIMEOUT_CERT_CAP).ok()?;
    let max_votes = usize::from(crate::protocol::constants::MAX_SEATS).pow(2) / 4;
    if body.votes.len() < 2 || body.votes.len() > max_votes {
        return None;
    }
    seat_of(&opened.sender)?;
    let mut stage: Option<CertSubject> = None;
    let mut subjects: Vec<SeatIdx> = Vec::new();
    let mut voters: BTreeSet<SeatIdx> = BTreeSet::new();
    for vote in &body.votes {
        let v = chained::open_in_hand(vote, FRAME_CAP, EventType::TimeoutVote, table_id, hand_id).ok()?;
        let voter = seat_of(&v.sender)?;
        let named: TimeoutVote = chained::payload(&v, TIMEOUT_VOTE_CAP).ok()?;
        match &stage {
            None => stage = Some(CertSubject::of(&named)),
            Some(first) => {
                if !first.same_stage(&named) {
                    return None;
                }
            }
        }
        if voter == named.subject_seat {
            return None;
        }
        if !subjects.contains(&named.subject_seat) {
            subjects.push(named.subject_seat);
        }
        voters.insert(voter);
    }
    Some((subjects, voters))
}

/// `D-051`: the cause every vote about `seat` in a certificate's bytes
/// carries -- `Some(CAUSE_FLOOD)` for a seat put out for flooding the table's
/// carrier group -- or `None` when the votes about it disagree or there are
/// none. Read after `certificate_names` has accepted the bytes.
pub fn certificate_cause(raw: &[u8], table_id: &[u8; 32], hand_id: u64, seat: SeatIdx) -> Option<u16> {
    let opened = chained::open_in_hand(raw, FRAME_CAP, EventType::TimeoutCert, table_id, hand_id).ok()?;
    let body: TimeoutCert = chained::payload(&opened, TIMEOUT_CERT_CAP).ok()?;
    let mut cause: Option<Option<u16>> = None;
    for vote in &body.votes {
        let v = chained::open_in_hand(vote, FRAME_CAP, EventType::TimeoutVote, table_id, hand_id).ok()?;
        let named: TimeoutVote = chained::payload(&v, TIMEOUT_VOTE_CAP).ok()?;
        if named.subject_seat != seat {
            continue;
        }
        match cause {
            None => cause = Some(named.cause),
            Some(c) if c != named.cause => return None,
            Some(_) => {}
        }
    }
    cause.flatten()
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
            returns: vec![0; 3],
            out: Vec::new(),
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
            returns: vec![0; 5],
            out: Vec::new(),
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
        let about_4 = CertSubject::of(&hand.subject_now(4).expect("a stage to vote about"));
        hand.commit_certificate(&about_4);
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
            returns: vec![0; 3],
            out: Vec::new(),
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

    /// `S1-CG`: a hand that ABORTED at stage 0 has not been dealt, and a hand
    /// that dealt and then aborted has.
    ///
    /// Both halves matter and the second is why the phase alone cannot answer
    /// it: `Phase::Aborted` is the same variant either way, so the old
    /// `!matches!(self.phase, Phase::Init(_))` could tell them apart in
    /// neither direction -- it said *dealt* to both.
    ///
    /// **To make this fail**: delete `self.stage_zero_done = true;` above the
    /// `Phase::Deck` assignment in `on_hand_init` and the second half goes red;
    /// restore `dealt` to `!matches!(self.phase, Phase::Init(_))` and the first
    /// half goes red. Both were run.
    #[test]
    fn an_abort_at_stage_zero_is_not_a_dealt_hand() {
        let (mut a, _from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        assert!(!a.dealt(), "nobody has been heard yet");
        let _ = a
            .abort_now(Abort::Deadline, &key(10), NOW + 600_000)
            .unwrap();
        assert!(
            !a.dealt(),
            "the hand died at stage 0 and no card was dealt: `HAND_BEGAN` must \
             not be reported for it, and 4.10's T47 must not be crossed on it"
        );

        let (_b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
        let (mut c, _from_c) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
        let _ = deliver(&mut c, &from_b, &key(10));
        assert!(c.dealt(), "stage 0 completed here");
        let _ = c
            .abort_now(Abort::Deadline, &key(10), NOW + 600_000)
            .unwrap();
        assert!(
            c.dealt(),
            "an abort AFTER the deal does not un-deal the hand, and this is the \
             half a phase test cannot get right"
        );
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

        // `S1-ER`: what the settlement moved to each seat, which is what the
        // window marks a winner by. Read off here, where both sides of it are
        // known -- and **the hand still says its street while it is over**,
        // which is exactly the trap the node fell into: it reported the table's
        // state on `street().is_some()`, so after the settlement it reported
        // the settled stacks and the window's own subtraction came out zero at
        // every seat.
        assert_eq!(a.street(), Some(Street::River), "a settled hand still says its street");
        assert!(a.over(), "and is over: the two questions are not the same one");
        let gain = a.settled_gain();
        assert_eq!(gain, b.settled_gain(), "one settlement, one figure");
        assert_eq!(
            gain.iter().sum::<u64>(),
            2 * 100,
            "what moved is the pot, no more and no less"
        );
        for (seat, took) in gain.iter().enumerate() {
            let named = a
                .settled_pots()
                .iter()
                .any(|(_, winners)| winners.contains(&(seat as u8)));
            assert_eq!(*took > 0, named, "seat {seat}: chips moved to the seats the pots name");
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

    /// **`on_late_settlement`'s justification does not hold for a readmitted
    /// seat, and this pins the half that fails** (`S1-CL`).
    ///
    /// That function builds its stage over `self.open.required` and defends
    /// adopting a peer's settlement body with: *"this client is in that set,
    /// and it never published a `HAND_COMPLETE` for this stage to hear. So a
    /// peer-seeded body reaches neither `next_hand` nor `GENESIS(k+1)`, and
    /// D-012 is not in question."*
    ///
    /// **The first clause is false and `Hand::open_with` makes it so on
    /// purpose.** The membership test there is
    /// `accepted.contains(&o.my_seat)`, where `accepted = required ∪
    /// readmitted` — §4.9's readmission route, and the sibling test above
    /// pins that a readmitted seat is *accepted at stage zero and never
    /// required*. So a readmitted seat holds a hand whose `my_seat` is
    /// **outside** the very set the late stage requires, the stage can close
    /// on the required peers alone, and `late.closed` is set from whichever
    /// body arrived first.
    ///
    /// **And `next_hand` does read those stacks**: its `Phase::Aborted` arm
    /// takes `late.closed`'s second element when it is `Some`, so they become
    /// hand `k+1`'s `stack_at_hand_start`, enter `roster_hash` and enter
    /// `GENESIS(k+1)`. The route the comment says does not exist, exists.
    ///
    /// **What this test does NOT claim.** It does not claim a divergence. When
    /// the required peers agree — the ordinary case — every body is identical
    /// and arrival order decides nothing, and following the settlement is what
    /// keeps a readmitted seat with the table. The exposure is confined to a
    /// settlement the required peers **disagree** about, which `S1-BD` made
    /// audible on purpose and which `S1-CI` shows §6.3 cannot then heal. It is
    /// the justification that is wrong, not necessarily the behaviour, and
    /// changing the behaviour here would fork a readmitted seat away from a
    /// table it currently follows.
    ///
    /// **To make this fail**: make `open_with` gate on `required` instead of
    /// `accepted`, which is the change
    /// `a_readmitted_seat_is_accepted_at_stage_zero_and_never_required` exists
    /// to prevent. It was run.
    #[test]
    fn a_readmitted_seat_is_outside_the_set_the_late_stage_requires() {
        let mut o = opening3(2);
        o.required = vec![0, 1];
        o.readmitted = vec![2];
        let (h, _) = Hand::open(o, &key(12), NOW, 30_000).unwrap();

        assert_eq!(h.open.my_seat, 2);
        assert!(
            !h.open.required.contains(&h.open.my_seat),
            "the hand exists and its own seat is NOT in the required set: this is \
             the clause on_late_settlement's comment relies on"
        );

        // And a stage over that required set closes without this seat, which is
        // the other half: nothing this client withholds can hold it open.
        let mut stage = Collective::closed(
            9,
            EventType::HandComplete.code(),
            &h.open.required,
        )
        .expect("a stage over R(k)");
        assert!(!stage.complete());
        stage.hear(0, [1u8; 32]);
        assert!(!stage.complete(), "one of the two required seats is not the stage");
        stage.hear(1, [2u8; 32]);
        assert!(
            stage.complete(),
            "and with both required seats heard it is complete, with seat 2 \
             having published nothing"
        );
    }

    /// **A readmitted seat adopts a peer's settlement only if the peers agree**
    /// (`S1-CL`, the fix).
    ///
    /// The sibling test above pins that such a seat is outside the set its
    /// late stage requires, so the stage closes on the required peers alone
    /// and `late.body` is whichever of them spoke first. Adopting that body
    /// when the peers **disagree** would be choosing a side by arrival order —
    /// a per-receiver quantity — into `GENESIS(k+1)`, which is D-012's shape.
    ///
    /// So a borrowed body closes the stage only when every later body equals
    /// the first. In the agreed case nothing changes and the seat follows the
    /// settlement, which is what keeps it with the table. In the disagreed
    /// case it adopts none of them, says so, and keeps the abort's terminal —
    /// while the peers, who are the ones disagreeing, are freezing the table
    /// at checkpoint 8 anyway.
    ///
    /// An OWN body — `give_up` from `Step::Settling` — is not gated: a peer
    /// that settled normally applies `mine.final_stacks` whatever the others
    /// say, and `a_disagreeing_late_settlement_is_heard_and_the_settlement_wins`
    /// below pins that side.
    ///
    /// **To make this fail**: close on `late.stage.complete()` alone, which is
    /// what the code did before this test existed. It was run.
    #[test]
    fn a_borrowed_late_settlement_closes_only_when_the_peers_agree() {
        use crate::table::handwire::HandComplete;

        // The two settlement bodies the required peers might publish.
        let body = |stacks: Vec<u64>| HandComplete {
            pots: Vec::new(),
            refunds: Vec::new(),
            deltas: vec![0, 0, 0],
            final_stacks: stacks,
            busted: Vec::new(),
            state_hash: [0u8; 32],
        };
        let sealed = |seat_key: &SigningKey, b: &HandComplete| -> Vec<u8> {
            let slot = Slot {
                table_id: [1; 32],
                hand_id: 1,
                sequence: 40,
                previous_event_hash: [5u8; 32],
            };
            chained::seal(
                EventType::HandComplete,
                &slot,
                b,
                seat_key,
                NOW,
                0,
                HAND_COMPLETE_CAP,
            )
            .expect("a settlement seals")
        };

        // A readmitted seat: accepted, never required, and it never settled.
        let fresh = || {
            let mut o = opening3(2);
            o.required = vec![0, 1];
            o.readmitted = vec![2];
            let (mut h, _) = Hand::open(o, &key(12), NOW, 30_000).unwrap();
            let _ = h.abort_now(Abort::Deadline, &key(12), NOW + 600_000).unwrap();
            assert!(h.aborted().is_some());
            h
        };

        // (i) The peers DISAGREE: seat 0 says one thing, seat 1 another.
        let mut h = fresh();
        let a = body(vec![12_000, 8_000, 10_000]);
        let b = body(vec![8_000, 12_000, 10_000]);
        assert_eq!(h.on_event(&sealed(&key(10), &a), &key(12), NOW), Ok(Vec::new()));
        assert_eq!(h.on_event(&sealed(&key(11), &b), &key(12), NOW), Ok(Vec::new()));
        let late = h.late.as_ref().expect("a late stage was built from seat 0's body");
        assert!(!late.own, "the body is borrowed, this seat never settled");
        assert!(late.disagreed, "and seat 1 disagreed with it");
        assert!(late.stage.complete(), "the required set is heard in full");
        assert!(
            late.closed.is_none(),
            "so the stage is complete and NOT closed: adopting either body would be \
             choosing a side by arrival order"
        );
        let note = h.take_settle_note().expect("and it says so");
        assert!(note.contains("emitters disagree"), "{note}");
        assert_eq!(
            h.terminal(),
            Some(crate::protocol::transcript::abort_terminal(
                &h.table_id(),
                h.hand_id(),
                &h.genesis()
            )),
            "the abort's terminal stands"
        );

        // (ii) The peers AGREE: the same body twice, and the seat follows it.
        let mut h = fresh();
        assert_eq!(h.on_event(&sealed(&key(10), &a), &key(12), NOW), Ok(Vec::new()));
        assert_eq!(h.on_event(&sealed(&key(11), &a), &key(12), NOW), Ok(Vec::new()));
        let late = h.late.as_ref().expect("held");
        assert!(!late.disagreed);
        let (_, stacks) = late.closed.as_ref().expect("closed on agreement");
        assert_eq!(stacks, &vec![12_000, 8_000, 10_000], "with the settlement's stacks");
        // The agreed branch says so, on the same channel as the refusal, so a
        // field run can show the fix adopting and not only refusing. Asserted,
        // because an unasserted log line can vanish with every test green.
        let note = h.take_settle_note().expect("the close is reported");
        assert!(
            note.contains("closed: a peer's body") && !note.contains("not all agreeing"),
            "{note}"
        );
        assert_ne!(
            h.terminal(),
            Some(crate::protocol::transcript::abort_terminal(
                &h.table_id(),
                h.hand_id(),
                &h.genesis()
            )),
            "and the settlement wins over the abort, exactly as before"
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
                // **`S1-BZ`: `TERMINAL(k)` on this path is the settlement's, and
                // §4.10's boundary window is opened at it.** `checkpoint8` is
                // `None` here — its only write is inside
                // `close_settlement_if_done`, which `give_up` made unreachable
                // by replacing the phase — so a `terminal()` that asked
                // `checkpoint8` and then fell through to the aborted arm would
                // answer `ABORT_TERMINAL(k)` and open the window at a parent no
                // boundary event of this hand carries. It did, for twenty
                // minutes on 2026-09-07.
                //
                // The break that must make this fail: delete `terminal()`'s
                // late-settlement arm.
                assert!(b.checkpoint8().is_none(), "give_up left no checkpoint here");
                assert_ne!(
                    b.terminal(),
                    Some(crate::protocol::transcript::abort_terminal(
                        &b.table_id(),
                        b.hand_id(),
                        &b.genesis()
                    )),
                    "§4.10: HAND_COMPLETE wins over an abort, so the terminal is                      the settlement's"
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

    /// Three seats brought to the deck stage with seat 2 silent, and nobody
    /// has voted yet. `S1-BT`'s tests start here, because what design A
    /// changes is what happens between the stage budget and the vote.
    fn three_at_the_deck_stage() -> (Hand, Hand, Hand, [SigningKey; 3]) {
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
        (a, b, c, keys)
    }

    /// Three seats at the deck stage, seat 2 silent: seats 0 and 1 wait for
    /// it, vote at the stage budget, and seat 2 gives the hand up unattributed.
    fn two_voters_and_a_silent_seat() -> (Hand, Hand, [SigningKey; 3], Vec<u8>, Vec<u8>, Vec<u8>) {
        let (mut a, mut b, mut c, keys) = three_at_the_deck_stage();
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

    /// `D-059`, the owner's rule (2026-09-15): a seat that has made the table
    /// wait is voted about sooner at a cryptographic step -- ten seconds a wait,
    /// three times and enough, never under `WAIT_FROM_MS` (ten) -- and the carrier's
    /// reprieve is twice the seat's own time.
    #[test]
    fn the_table_waits_less_at_a_step_for_a_seat_that_made_it_wait() {
        use crate::protocol::constants::WAIT_FROM_MS;
        assert_eq!(patience_ms(30_000, 0), 30_000);
        assert_eq!(patience_ms(30_000, 1), 20_000);
        assert_eq!(patience_ms(30_000, 2), 10_000);
        assert_eq!(patience_ms(30_000, 3), WAIT_FROM_MS, "ten seconds, the least");
        assert_eq!(patience_ms(30_000, 9), WAIT_FROM_MS, "three times and enough");
        assert_eq!(patience_ms(4_000, 3), 4_000, "never longer than the step's own budget");

        let (mut a, _, _, keys) = three_at_the_deck_stage();
        assert!(a.vote_on_timeouts(&keys[0], NOW + 29_999, 0).unwrap().is_empty());
        assert!(!a.vote_on_timeouts(&keys[0], NOW + 30_000, 0).unwrap().is_empty(), "no wait before: the whole budget");

        let (mut a, _, _, keys) = three_at_the_deck_stage();
        a.set_patience(&[0, 0, 1]);
        assert!(a.vote_on_timeouts(&keys[0], NOW + 19_999, 0).unwrap().is_empty());
        assert!(!a.vote_on_timeouts(&keys[0], NOW + 20_000, 0).unwrap().is_empty(), "one wait before: twenty seconds");

        for waits in [3u8, 7] {
            let (mut a, _, _, keys) = three_at_the_deck_stage();
            a.set_patience(&[0, 0, waits]);
            assert!(a.vote_on_timeouts(&keys[0], NOW + WAIT_FROM_MS - 1, 0).unwrap().is_empty());
            assert!(!a.vote_on_timeouts(&keys[0], NOW + WAIT_FROM_MS, 0).unwrap().is_empty(), "{waits} waits: ten seconds");
        }

        let (mut a, _, _, keys) = three_at_the_deck_stage();
        a.set_patience(&[0, 0, 1]);
        let held = 1u32 << 2;
        assert!(a.vote_on_timeouts(&keys[0], NOW + 39_999, held).unwrap().is_empty(), "still delivered: twice its time");
        assert!(!a.vote_on_timeouts(&keys[0], NOW + 40_000, held).unwrap().is_empty());
    }

    /// `D-059`: a stall is watched while it stands and counted once when it is
    /// over, however long it lasted; the step it stood at keeps the time it
    /// began with, and the next stall is counted on its own.
    #[test]
    fn a_wait_is_counted_once_when_the_stall_is_over() {
        use crate::protocol::constants::WAIT_FROM_MS;
        let (a, _, _, _) = three_at_the_deck_stage();
        let early = a.stall_now(NOW + WAIT_FROM_MS - 1).expect("the deck stage waits on seat 2");
        assert!(early.waits.is_empty() && early.countable, "a moment is no wait: {early:?}");
        let stood = a.stall_now(NOW + WAIT_FROM_MS).expect("still");
        assert_eq!((stood.waits.clone(), stood.key), (vec![2], early.key));

        let mut w = Waits::default();
        assert!(w.tick(Some(early), false).is_empty());
        assert!(w.tick(Some(stood.clone()), false).is_empty(), "counted when it is over, not while it stands");
        assert!(w.tick(Some(stood.clone()), false).is_empty());
        assert_eq!(w.tick(None, false), vec![(2, 1)], "over: one wait");
        assert_eq!(w.counts(), &[0, 0, 1]);
        assert!(w.tick(None, false).is_empty(), "and only once");

        // A stall that runs into the next stage's stall: the first is counted as
        // the second begins.
        let next = Stall { key: (stood.key.0, stood.key.1 + 1), waits: vec![2], countable: true };
        assert!(w.tick(Some(stood), false).is_empty());
        assert_eq!(w.tick(Some(next), false), vec![(2, 2)]);
        assert_eq!(w.tick(None, false), vec![(2, 3)]);
    }

    /// `D-059`, the bed's lesson (`run181556-3`): a client that heard none of the
    /// other seats at any moment of a stall counts nothing for it -- its own
    /// line went -- and neither does a stall that could not count.
    #[test]
    fn a_stall_this_client_was_deaf_through_counts_nothing() {
        let stall = |stage: u64, waits: Vec<SeatIdx>| Stall { key: (4, stage), waits, countable: true };
        let mut w = Waits::default();
        assert!(w.tick(Some(stall(3, Vec::new())), true).is_empty(), "deaf before the stall counts");
        assert!(w.tick(Some(stall(3, vec![1])), false).is_empty());
        assert!(w.tick(None, false).is_empty(), "the seat it waited on is not blamed");
        assert!(w.counts().iter().all(|c| *c == 0));

        assert!(w.tick(Some(Stall { key: (4, 5), waits: vec![1], countable: false }), false).is_empty());
        assert!(w.tick(None, false).is_empty(), "a stall that could not count counts nothing");

        assert!(w.tick(Some(stall(7, vec![1])), false).is_empty());
        assert_eq!(w.tick(None, false), vec![(1, 1)], "a stall heard through counts");
    }

    /// `D-059`, the owner's choice: a turn that runs out is a wait, and a turn's
    /// time is never cut, however many times the seat made the table wait.
    #[test]
    fn a_turn_run_out_is_a_wait_and_its_time_is_never_cut() {
        let (mut hands, keys) = three_to_the_bet();
        let up = hands[0].turn().expect("somebody is to act").seat;
        let voter = (0..3u8).find(|s| *s != up).expect("a third seat");
        let v = &mut hands[usize::from(voter)];
        let mut waits = vec![0u8; 3];
        waits[usize::from(up)] = 3;
        v.set_patience(&waits);
        // `opening3`: twenty seconds to decide, five for the network, no reserve.
        assert!(v.stall_now(NOW + 24_999).is_some_and(|s| s.waits.is_empty()), "inside its time a turn is no wait");
        assert!(v.vote_on_timeouts(&keys[usize::from(voter)], NOW + 24_999, 0).unwrap().is_empty(), "the turn's time is not cut");
        assert_eq!(v.stall_now(NOW + 25_000).map(|s| s.waits), Some(vec![up]), "run out, the table waits");
        assert!(!v.vote_on_timeouts(&keys[usize::from(voter)], NOW + 25_000, 0).unwrap().is_empty());
    }

    /// `D-059`: no wait counts while the table's first hand gives its seats
    /// `S1-FM`'s minute to join the table's group, and none where no certificate
    /// could remove the seat -- a client that hears nobody is looking at its own
    /// stall.
    #[test]
    fn no_wait_counts_while_the_first_hand_waits_for_its_seats_or_where_nobody_could_be_voted_out() {
        use crate::protocol::constants::WAIT_FROM_MS;
        let (mut a, from_a) = Hand::open(opening3(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening3(1), &key(11), NOW, 30_000).unwrap();
        let _ = deliver(&mut a, &from_b, &key(10));
        let _ = deliver(&mut b, &from_a, &key(11));
        assert_eq!(a.waiting_for(), vec![2], "the opening waits on seat 2's copy");
        assert!(a.stall_now(NOW + WAIT_FROM_MS).is_some_and(|s| !s.countable), "joining the table's group is no wait");
        assert!(a.stall_now(NOW + 90_000).is_some_and(|s| s.countable && s.waits == vec![2]), "past the minute it is");

        let (mut lone, _) = Hand::open(opening3(0), &key(10), NOW, 30_000).unwrap();
        let (_, from_b) = Hand::open(opening3(1), &key(11), NOW, 30_000).unwrap();
        let (_, from_c) = Hand::open(opening3(2), &key(12), NOW, 30_000).unwrap();
        let _ = deliver(&mut lone, &from_b, &key(10));
        let _ = deliver(&mut lone, &from_c, &key(10));
        assert_eq!(lone.waiting_for(), vec![1, 2], "the deck stage, nobody's deck heard");
        assert!(lone.stall_now(NOW + WAIT_FROM_MS).is_some_and(|s| !s.countable), "two of three silent: nothing counts");
    }

    /// `S1-FP`, the owner's game (2026-09-15): the table acted for a silent seat,
    /// and the next seat's own client folded for it half a second after the turn
    /// came -- the turn began where the silent seat's had, at the last signed
    /// action, a clock and a vote before. The turn after the table acts begins
    /// when the table acts.
    #[test]
    fn the_turn_after_the_table_acts_for_a_seat_begins_when_the_table_acts() {
        fn feed(h: &mut Hand, sends: &[Send], key: &SigningKey, at: u64) -> Vec<Send> {
            let mut out = Vec::new();
            for Send::Broadcast(bytes) in sends {
                out.append(&mut h.on_event(bytes, key, at).expect("accepted"));
            }
            out
        }
        let (mut hands, keys) = three_to_the_bet();
        let up = hands[0].turn().expect("somebody is to act").seat;
        let voters: Vec<usize> = (0..3usize).filter(|s| *s != usize::from(up)).collect();
        let (a, b) = (voters[0], voters[1]);
        let before = hands[a].turn().expect("the silent seat's turn").began_unix_ms;
        let late = NOW + 600_000;
        let va = hands[a].vote_on_timeouts(&keys[a], late, 0).unwrap();
        let vb = hands[b].vote_on_timeouts(&keys[b], late, 0).unwrap();
        let ca = feed(&mut hands[a], &vb, &keys[a], late);
        let cb = feed(&mut hands[b], &va, &keys[b], late);
        assert!(!ca.is_empty() && !cb.is_empty(), "each voter seals its copy");
        let _ = feed(&mut hands[a], &cb, &keys[a], late);
        let _ = feed(&mut hands[b], &ca, &keys[b], late);
        for v in [a, b] {
            let turn = hands[v].turn().expect("the betting goes on");
            assert_ne!(turn.seat, up, "the table acted for the silent seat");
            assert_eq!(turn.began_unix_ms, late, "seat {v}: the next turn begins when the table acts, not at {before}");
        }
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

    /// `S1-BT` design A, the shape the owner approved on 2026-09-05.
    ///
    /// **The collision.** The mid-delivery lever holds the vote on a seat the
    /// carrier is still delivering until twice the stage budget (`S1-BK`), and
    /// `may_abandon` waited for exactly the same instant. The stall tick votes
    /// and then aborts, so every voter cast its vote and ended the hand in one
    /// tick, the abort ended it at every receiver before the copies could
    /// gather, and the seat that stalled the stage was never named — measured
    /// in `split092359-10`, one vote of seven on every hand.
    ///
    /// The round now gets one stage budget of air after this client's own
    /// vote. This test is the whole of it: the vote is withheld to 60 s by the
    /// lever, the hand may not be abandoned at 62 s, and the certificate that
    /// completes at 70 s ends the hand with its subject named.
    #[test]
    fn a_lever_held_round_gets_its_air_and_certifies_instead_of_aborting() {
        let (mut a, mut b, _c, keys) = three_at_the_deck_stage();
        let held = 1u32 << 2;
        let t1 = NOW + 30_000;
        assert!(
            a.vote_on_timeouts(&keys[0], t1, held).unwrap().is_empty(),
            "the lever holds the vote at one budget"
        );
        assert!(!a.may_abandon(t1), "and the hand is not given up at one budget either");

        let t2 = NOW + 60_000;
        let va = a.vote_on_timeouts(&keys[0], t2, held).unwrap();
        let vb = b.vote_on_timeouts(&keys[1], t2, held).unwrap();
        assert!(!va.is_empty() && !vb.is_empty(), "the lever releases at twice the budget");
        assert!(
            !a.may_abandon(NOW + 62_000),
            "the round has just started and must not be aborted in the same tick"
        );

        let Send::Broadcast(va) = &va[0];
        let Send::Broadcast(vb) = &vb[0];
        let t3 = NOW + 70_000;
        let ca = a.on_event(vb, &keys[0], t3).unwrap();
        let cb = b.on_event(va, &keys[1], t3).unwrap();
        assert!(!ca.is_empty() && !cb.is_empty(), "each voter seals a copy");
        let Send::Broadcast(ca) = &ca[0];
        let Send::Broadcast(cb) = &cb[0];
        assert!(a.on_event(cb, &keys[0], t3).is_ok());
        assert!(b.on_event(ca, &keys[1], t3).is_ok());
        assert_eq!(
            a.aborted(),
            Some(Abort::Told { cause: 1 }),
            "ended by the certificate, with the seat named"
        );
        assert!(!a.took_part(2), "and it leaves the roster");
        let na = a.next_hand().expect("an aborted hand has a successor");
        let nb = b.next_hand().expect("on both");
        assert!(!na.required.contains(&2), "{:?}", na.required);
        assert_eq!(na.genesis, nb.genesis, "one GENESIS(k+1)");
    }

    /// Design A costs nothing where the carrier says nothing is in flight.
    ///
    /// With every mid-delivery bit clear the vote is cast at one budget, so
    /// the round has had its air by two — which is where `long_past_stage`
    /// opens and where this client gave the hand up before the change. The
    /// extra wait is paid only by a round the lever actually held.
    #[test]
    fn a_round_that_was_never_held_ends_the_hand_where_it_always_did() {
        let (mut a, _b, _c, keys) = three_at_the_deck_stage();
        let t1 = NOW + 30_000;
        assert!(
            !a.vote_on_timeouts(&keys[0], t1, 0).unwrap().is_empty(),
            "nothing is in flight, so the vote is cast at one budget"
        );
        assert!(!a.may_abandon(NOW + 59_999), "still inside the certificate's window");
        assert!(
            a.may_abandon(NOW + 60_000),
            "and the hand is given up at twice the budget, exactly as before design A"
        );
    }

    /// `S1-BY`, the negative: a certificate about this client's own seat does
    /// **not** take away its right to end the hand, and the guard that made it
    /// do so was reverted. What ends the hand for the other seats in the
    /// measured run is this seat's own abort.
    #[test]
    fn a_seat_certified_out_mid_hand_still_ends_the_hand_it_is_inside() {
        let (mut a, _b, _c, keys) = three_at_the_deck_stage();
        let t2 = NOW + 60_000;
        assert!(
            !a.vote_on_timeouts(&keys[0], t2, 0).unwrap().is_empty(),
            "this client votes about the silent seat"
        );
        assert!(a.may_abandon(NOW + 90_000), "and the ceiling lets it give the hand up");
        a.certified.push(0);
        assert!(
            a.may_abandon(NOW + 90_000),
            "a certificate about its own seat changes nothing here: `certified` is the \
             next hand's roster and this is this hand's party set"
        );
    }

    /// The ceiling, which is what makes design A terminate: a round that
    /// cannot complete — the other voter's copy never arrives — still ends the
    /// hand, at three stage budgets and not one tick later.
    #[test]
    fn a_round_that_cannot_complete_still_ends_the_hand_at_three_budgets() {
        let (mut a, _b, _c, keys) = three_at_the_deck_stage();
        let held = 1u32 << 2;
        let t2 = NOW + 60_000;
        assert!(
            !a.vote_on_timeouts(&keys[0], t2, held).unwrap().is_empty(),
            "this client votes at twice the budget"
        );
        assert!(!a.may_abandon(NOW + 89_999), "the round still has air");
        assert!(a.may_abandon(NOW + 90_000), "and the ceiling ends it");
    }

    /// **`S1-CF`, the half that decides whether it is a fault: the state is
    /// UNREACHABLE, and this drives the real derivation to say so.**
    ///
    /// The test below hand-builds an `Opening`. This one certifies a seat out
    /// through the actual machinery and asks `next_hand` what it produces:
    /// every seat in the derived `required` carries `GRACE_HANDS`, and the seat
    /// whose `grace` moved is exactly the one the filter removed. That is the
    /// invariant that keeps `dealt_in`'s unhashed `grace` term inert, and it is
    /// what `S1-BM`'s return certificate would break in one hand.
    ///
    /// The break that must make this fail: delete the `certified` test from
    /// `took_part`, so a certified seat stays in `required` carrying the grace
    /// the gate took off it.
    #[test]
    fn every_required_seat_carries_a_full_grace() {
        let (mut a, mut b, _c, keys) = three_at_the_deck_stage();
        let t2 = NOW + 60_000;
        let va = a.vote_on_timeouts(&keys[0], t2, 0).unwrap();
        let vb = b.vote_on_timeouts(&keys[1], t2, 0).unwrap();
        let Send::Broadcast(va) = &va[0];
        let Send::Broadcast(vb) = &vb[0];
        let t3 = NOW + 70_000;
        let ca = a.on_event(vb, &keys[0], t3).unwrap();
        let cb = b.on_event(va, &keys[1], t3).unwrap();
        assert!(!ca.is_empty() && !cb.is_empty(), "each voter seals a copy");
        let Send::Broadcast(ca) = &ca[0];
        let Send::Broadcast(cb) = &cb[0];
        assert!(a.on_event(cb, &keys[0], t3).is_ok());
        assert!(b.on_event(ca, &keys[1], t3).is_ok());
        assert!(!a.took_part(2), "seat 2 is certified out");

        let next = a.next_hand().expect("an aborted hand has a successor");
        assert!(!next.required.contains(&2), "{:?}", next.required);
        for s in &next.required {
            assert_eq!(
                next.grace.get(usize::from(*s)).copied(),
                Some(GRACE_HANDS),
                "seat {s} is required and its grace is not full: {:?}",
                next.grace
            );
        }
        // And the seat whose grace DID move is the one that left, which is the
        // whole of why the divergence cannot reach `dealt_in`.
        assert!(
            next.grace[2] < GRACE_HANDS,
            "the gate is real, it just fires where dealt_in never looks: {:?}",
            next.grace
        );
    }

    /// **`S1-CF`: two peers at one genesis with two `dealt_in` sets, and the
    /// client can say so.**
    ///
    /// `genesis_hand` commits `participants` and **not** `grace`, and
    /// `open_with` derives `dealt_in` from `grace` — so two peers whose `grace`
    /// differs for any reason derive the **same** `GENESIS(k+1)` and refuse each
    /// other's opening with `Differs(DealtIn)`, which is a hard refusal. Nothing
    /// downstream can see it: the fork detector compares genesis values, which
    /// match, and §6.3's freeze wants a checkpoint-8 `state_hash` that a hand
    /// stalled at stage 0 never reaches.
    ///
    /// This test is the mechanism and not only the note: it sets one seat's
    /// `grace` to zero on **one side only**, which is what a `strikes`
    /// disagreement produces in the field, and shows the opening refused.
    ///
    /// **What it pins and what it does not.** The fixture *assigns* the genesis
    /// rather than deriving it, so this pins the consequence — one `grace`
    /// difference at one genesis is a hard refusal — and not the premise. The
    /// premise needs no test: `genesis_hand`'s signature takes `table_id`,
    /// `hand_id`, `session_id`, `roster_hash`, `previous_terminal` and
    /// `required`, so *`grace` is not an input to the genesis* is enforced by
    /// the compiler.
    ///
    /// The break that must make this fail: drop the note, or widen its guard.
    #[test]
    fn one_genesis_and_two_dealt_in_sets_is_named_rather_than_silent() {
        let mut mine = opening(0);
        let mut theirs = opening(1);
        mine.required = vec![0, 1, 2];
        theirs.required = vec![0, 1, 2];
        mine.seats.push((2, key(12).verifying_key().to_bytes(), 10_000));
        theirs.seats.push((2, key(12).verifying_key().to_bytes(), 10_000));
        mine.max_players = 3;
        theirs.max_players = 3;
        // The one difference, and it is under no hash: seat 2 has spent its
        // allowance here and not there.
        mine.grace[2] = 0;
        assert_eq!(
            mine.genesis, theirs.genesis,
            "the fixture must differ ONLY in grace, or this proves nothing"
        );

        let (mut a, _from_a) = Hand::open(mine, &key(10), NOW, 30_000).unwrap();
        let (_b, from_b) = Hand::open(theirs, &key(11), NOW, 30_000).unwrap();
        let Send::Broadcast(bytes) = &from_b[0];
        let refused = a.on_event(bytes, &key(10), NOW);
        assert!(
            matches!(
                refused,
                Err(Failed::Disagrees {
                    seat: 1,
                    what: NotOurs::Differs(Field::DealtIn)
                })
            ),
            "one grace difference must refuse the opening: {refused:?}"
        );
        let note = a.take_dealt_note().expect("and the client must say so");
        assert!(
            note.contains("THIS client's own genesis") && note.contains("S1-CF"),
            "{note}"
        );
        // **Said once per hand, and the re-send is what makes that matter.** A
        // stage 0 that will never complete is re-broadcast every five seconds
        // for the life of the hand, and the disagreement check sits above the
        // phase test, so every copy reaches it. Without the guard the log
        // fills with one line per re-send.
        assert!(a.on_event(bytes, &key(10), NOW).is_err(), "the second copy is refused too");
        assert!(
            a.take_dealt_note().is_none(),
            "said once per hand, or a stalled stage 0 repeats it every re-send"
        );
    }

    /// `TERMINAL(k)` on **both** paths, which is what `checkpoint8` could not
    /// answer and what §4.10's window is opened from (`S1-BZ`).
    ///
    /// The hole this closes: `Boundaries::open` is reached only through
    /// `checkpoint8()`, which is `None` after an abort, so a window opened
    /// there existed on the settled path alone — and the abort path is the one
    /// a seat goes quiet on.
    ///
    /// The break that must make this fail: return `None` from `terminal()` on
    /// the aborted arm, or chain it from anything but `GENESIS(k)`.
    #[test]
    fn the_terminal_is_answered_on_the_aborted_path_too() {
        let (mut a, _b, _c, keys) = three_at_the_deck_stage();
        assert_eq!(a.terminal(), None, "a live hand has no terminal");
        let _ = a.abort_now(Abort::Deadline, &keys[0], NOW + 600_000).unwrap();
        assert!(a.aborted().is_some());
        assert_eq!(
            a.checkpoint8(),
            None,
            "§6.2 row 8's aborted checkpoint is S1-R and is not built"
        );
        assert_eq!(
            a.terminal(),
            Some(crate::protocol::transcript::abort_terminal(
                &a.table_id(),
                a.hand_id(),
                &a.genesis()
            )),
            "and ABORT_TERMINAL(k) is a function of GENESIS(k) alone, so it              needs neither a stage nor agreement about the middle of the hand"
        );
    }

    /// `S1-BB`'s discriminator, and this test is the whole of what makes it one.
    ///
    /// The row's open question is whether a certified-out seat had really gone
    /// silent or whether its last message was lost in flight, and the reading it
    /// has — `vote_state`'s `mid-delivery` bit — is sampled only where a vote is
    /// **held back**, which is the one place the lever guarantees the bit is
    /// set. 79 of 79 read `true` and the sample could not have said otherwise.
    ///
    /// Sampled at the vote instead, only two readings are reachable, and they
    /// are the two hypotheses: **bit clear**, so the carrier had nothing of that
    /// seat's and the silence was real; or **bit set with `long past` true**, the
    /// bound releasing an accusation the carrier still contradicts. A third —
    /// set with `long past` false — would mean the lever let a vote through it
    /// was supposed to hold, and this asserts it cannot appear.
    #[test]
    fn the_carrier_is_sampled_where_the_accusation_is_made() {
        // The carrier holding the subject's traffic. The lever withholds the
        // vote for one budget and releases it at two.
        let (mut a, _b, _c, keys) = three_at_the_deck_stage();
        let held = 1u32 << 2;
        assert!(
            a.vote_on_timeouts(&keys[0], NOW + 30_000, held).unwrap().is_empty(),
            "the lever holds the vote at one budget"
        );
        assert!(
            a.take_vote_carrier().is_empty(),
            "a vote that was never cast is not an accusation and must leave no reading"
        );
        let cast = a.vote_on_timeouts(&keys[0], NOW + 60_000, held).unwrap();
        assert!(!cast.is_empty(), "the lever releases at twice the budget");
        assert_eq!(
            a.take_vote_carrier(),
            vec![(2, true, true)],
            "released by the bound, with the carrier still holding: the tail-of-burst reading"
        );
        assert!(
            a.take_vote_carrier().is_empty(),
            "taken rather than read, so the node reports it once"
        );

        // And the other reading, on a fresh client: nothing of the subject's in
        // the carrier, so the vote goes at one budget and the silence is real.
        let (mut d, _e, _f, keys) = three_at_the_deck_stage();
        assert!(
            !d.vote_on_timeouts(&keys[0], NOW + 30_000, 0).unwrap().is_empty(),
            "with the bit clear there is nothing to hold the vote"
        );
        assert_eq!(
            d.take_vote_carrier(),
            vec![(2, false, false)],
            "cast at one budget with an empty carrier: the divergence reading"
        );
    }

    /// Two seats silent at one stage with different mid-delivery bits, which
    /// is where the first shape of design A still had the collision: the vote
    /// about the seat whose bit is clear is cast a budget before the lever
    /// releases the other, and if the round's air were measured from that first
    /// vote it would already be spent when the second one is cast.
    #[test]
    fn a_second_round_at_one_stage_gets_its_own_air() {
        let keys: Vec<SigningKey> = (10..15).map(key).collect();
        let mut hands: Vec<Hand> = Vec::new();
        let mut inits: Vec<Vec<u8>> = Vec::new();
        for seat in 0..5u8 {
            let (h, sends) = Hand::open(opening5(seat), &keys[usize::from(seat)], NOW, 30_000).unwrap();
            let Send::Broadcast(b) = &sends[0];
            inits.push(b.clone());
            hands.push(h);
        }
        let mut decks: Vec<Vec<Send>> = vec![Vec::new(); 5];
        for to in 0..5usize {
            for from in 0..5usize {
                if from != to {
                    let mut out =
                        deliver(&mut hands[to], &[Send::Broadcast(inits[from].clone())], &keys[to]);
                    decks[to].append(&mut out);
                }
            }
        }
        // Seats 0, 1 and 2 hear only each other: the deck stage waits on two.
        for to in 0..3usize {
            for from in 0..3usize {
                if from != to {
                    let _ = deliver(&mut hands[to], &decks[from], &keys[to]);
                }
            }
            assert_eq!(hands[to].waiting_for(), vec![3, 4], "seat {to} waits on two seats");
        }
        // Seat 4's traffic is still arriving; seat 3's is not.
        let held = 1u32 << 4;
        let t1 = NOW + 30_000;
        let first = hands[0].vote_on_timeouts(&keys[0], t1, held).unwrap();
        assert!(!first.is_empty(), "the seat whose bit is clear is voted on at one budget");
        assert!(!hands[0].may_abandon(t1), "and the hand is not given up there");

        let t2 = NOW + 60_000;
        let second = hands[0].vote_on_timeouts(&keys[0], t2, held).unwrap();
        assert!(!second.is_empty(), "the lever releases the second vote at twice the budget");
        assert!(
            !hands[0].may_abandon(NOW + 62_000),
            "the second round has just started: its air is its own, not the first round's"
        );
        assert!(hands[0].may_abandon(NOW + 90_000), "and the ceiling still ends the hand");
    }

    /// The difference between *has this client voted* and *is this client
    /// needed*, which is why design A reads `party_to_vote` and not
    /// `vote_joined`.
    ///
    /// A seat whose own timer has not fired yet is still one of the voters the
    /// round needs. Under the old test it was not "joined", so it admitted a
    /// peer's abort at one budget — and an abort ends the hand at every
    /// receiver, so that one seat killed the round for the seats that had
    /// voted.
    #[test]
    fn a_voter_that_owes_a_vote_it_has_not_cast_still_holds_the_abort() {
        let (mut a, mut b, mut c, keys) = three_at_the_deck_stage();
        let t1 = NOW + 30_000;
        let sends = c.abort_now(Abort::Deadline, &keys[2], t1).unwrap();
        let Send::Broadcast(abort) = &sends[0];
        let t2 = NOW + 31_000;
        assert_eq!(
            b.on_event(abort, &keys[1], t2),
            Err(Failed::NotYet),
            "seat 1 has not voted, but the round needs its vote"
        );
        assert!(b.aborted().is_none(), "so the hand did not end on a peer's word");
        // Seat 0 is a voter for the same subject, so it holds it too. The seat
        // the round does not need is seat 2 itself, which is the subject and
        // is not a party to a round about itself; this fixture has no fourth
        // seat to show that with, and `party_to_vote`'s own `*s !=
        // self.open.my_seat` is what implements it.
        assert_eq!(
            a.on_event(abort, &keys[0], t2),
            Err(Failed::NotYet),
            "seat 0 is a voter too, so it holds it as well"
        );
        assert!(a.aborted().is_none(), "held, not refused: only NotYet is re-judged later");
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
    ///
    /// **The state below is hand-built, and `next_hand` cannot produce it —
    /// `S1-CF`.** This comment used to read *"the state a seat reaches after
    /// three certificates against it"* and that is false: three certificates
    /// put the seat in `certified`, `took_part` is `!certified.contains`
    /// wherever a certificate can exist, and the seat is therefore filtered out
    /// of `required` before `grace` is ever consulted. So `PROTOCOL.md` §8.3's
    /// *sits out at three strikes* is enforced today by the certified exclusion
    /// and **not** by the allowance, and this test pins `open_with`'s reading of
    /// a `grace` of zero rather than any state the table reaches.
    ///
    /// It is kept because the reading is still the one §8.3 wants, and it is
    /// what `S1-BM`'s return certificate would make reachable in one hand.
    #[test]
    fn three_strikes_and_the_seat_takes_no_more_cards() {
        let mut o = opening3(0);
        // Hand-built; see the note above.
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

    /// `D-050`: a hand the rules let muck waits for its player and may be shown
    /// instead; left alone, it mucks. Both peers agree either way. The deal is
    /// random, so hands are played until each outcome has come up.
    #[test]
    fn a_hand_that_may_muck_waits_for_its_player() {
        let keys = [key(10), key(11)];
        let (mut shown_once, mut mucked_once) = (false, false);
        for _ in 0..60 {
            if shown_once && mucked_once {
                break;
            }
            let (mut a, from_a) = Hand::open(opening(0), &key(10), NOW, 30_000).unwrap();
            let (mut b, from_b) = Hand::open(opening(1), &key(11), NOW, 30_000).unwrap();
            a.hold_muck_for_the_player(true);
            b.hold_muck_for_the_player(true);
            let b_deck = deliver(&mut b, &from_a, &key(11));
            let a_deck = deliver(&mut a, &from_b, &key(10));
            let mut queue: Vec<(SeatIdx, Vec<Send>)> = vec![(1, deliver(&mut b, &a_deck, &key(11))), (0, deliver(&mut a, &b_deck, &key(10)))];
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
                let action = if hand.turn().unwrap().legal.can_check { Action::Check } else { Action::Call };
                let out = hand.act(action, &keys[usize::from(seat)], NOW).unwrap();
                queue.push((seat, out));
            }
            let Some(seat) = (0..2u8).find(|s| if *s == 0 { a.muck_held_until().is_some() } else { b.muck_held_until().is_some() }) else {
                // The second hand was not beaten, or tied: it showed, as the rules say.
                assert!(a.shown(0).is_some() && a.shown(1).is_some(), "nobody waited, so both showed");
                continue;
            };
            let until = if seat == 0 { a.muck_held_until() } else { b.muck_held_until() }.unwrap();
            assert!(until > NOW && until <= NOW + SHOW_WINDOW_MS, "three seconds at most: {}", until - NOW);
            assert!(!a.mucked(seat) && a.shown(seat).is_none() && !b.mucked(seat) && b.shown(seat).is_none(), "nothing said yet");
            let show = !shown_once;
            let out = {
                let hand: &mut Hand = if seat == 0 { &mut a } else { &mut b };
                if show {
                    hand.show_held(&keys[usize::from(seat)], NOW).unwrap()
                } else {
                    hand.muck_held_now(&keys[usize::from(seat)], NOW).unwrap()
                }
            };
            assert!(!out.is_empty(), "the word goes out");
            let mut queue = vec![(seat, out)];
            while let Some((from, sends)) = queue.pop() {
                if sends.is_empty() {
                    continue;
                }
                let to = 1 - from;
                let hand: &mut Hand = if to == 0 { &mut a } else { &mut b };
                let more = deliver(hand, &sends, &keys[usize::from(to)]);
                queue.push((to, more));
            }
            assert!(a.muck_held_until().is_none() && b.muck_held_until().is_none(), "nobody waits any more");
            if show {
                let holder: &Hand = if seat == 0 { &a } else { &b };
                assert_eq!(a.shown(seat), holder.cards(), "shown on this peer's table");
                assert_eq!(b.shown(seat), holder.cards(), "and on the other's");
                assert!(!a.mucked(seat) && !b.mucked(seat));
                shown_once = true;
            } else {
                assert!(a.mucked(seat) && b.mucked(seat), "mucked on both");
                assert!(a.shown(seat).is_none() && b.shown(seat).is_none());
                mucked_once = true;
            }
        }
        assert!(shown_once && mucked_once, "both outcomes came up (shown {shown_once}, mucked {mucked_once})");
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

    // -----------------------------------------------------------------------
    // `S1-BM`: the return certificate. Tests first, per `docs/research/READMISSION.md`.
    // -----------------------------------------------------------------------

    /// Seats 0 and 1 are the roster; seat 2 sits at the table with chips,
    /// **outside `R(k)`** -- the shape a certified-out seat has one boundary
    /// later -- and follows the hand as a bystander: it says nothing, is dealt
    /// nothing, and computes the same settlement everybody else does. Returns
    /// the three hands settled.
    fn a_settled_hand_with_a_bystander() -> (Vec<Hand>, [SigningKey; 3]) {
        let (hands, keys, _, _) = a_settled_hand_with_a_bystander_and_its_settlement();
        (hands, keys)
    }

    /// The same, also returning the two required seats' signed `HAND_COMPLETE`
    /// frames, for a client that has to reach the settlement by the late road.
    fn a_settled_hand_with_a_bystander_and_its_settlement() -> (Vec<Hand>, [SigningKey; 3], Vec<Vec<u8>>, Vec<Vec<u8>>) {
        a_settled_hand_with_a_bystander_holding(10_000)
    }

    /// The same with the bystander's stack chosen: zero is a busted seat.
    fn a_settled_hand_with_a_bystander_holding(chips: Chips) -> (Vec<Hand>, [SigningKey; 3], Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let keys = [key(10), key(11), key(12)];
        let mut hands: Vec<Hand> = Vec::new();
        let mut inits: Vec<Vec<Send>> = Vec::new();
        for seat in 0..3u8 {
            let mut o = opening3(seat);
            o.required = vec![0, 1];
            o.seats[2].2 = chips;
            let (h, sends) = Hand::open(o, &keys[usize::from(seat)], NOW, 30_000).unwrap();
            hands.push(h);
            inits.push(sends);
        }
        assert!(inits[2].is_empty(), "a bystander says nothing at stage 0");
        let table_id = hands[0].table_id();
        let mut settlements: Vec<Vec<u8>> = Vec::new();
        let mut queue: std::collections::VecDeque<(usize, Vec<Send>)> = std::collections::VecDeque::from(vec![(0, inits[0].clone()), (1, inits[1].clone())]);
        for _ in 0..1024 {
            if let Some((from, sends)) = queue.pop_front() {
                if sends.is_empty() {
                    continue;
                }
                for b in bytes_of(&sends) {
                    if chained::open_in_hand(&b, FRAME_CAP, EventType::HandComplete, &table_id, 1).is_ok() {
                        settlements.push(b);
                    }
                }
                for to in 0..3usize {
                    if to == from {
                        continue;
                    }
                    let out = deliver(&mut hands[to], &sends, &keys[to]);
                    if !out.is_empty() {
                        queue.push_back((to, out));
                    }
                }
                continue;
            }
            if hands[0].betting_over() && hands[1].betting_over() {
                break;
            }
            let Some(turn) = hands[0].turn().or_else(|| hands[1].turn()) else {
                panic!("nothing in flight, nobody to act, and the hand is not over");
            };
            let seat = usize::from(turn.seat);
            let turn = hands[seat].turn().expect("that hand agrees it is to act");
            let action = if turn.legal.can_check { Action::Check } else { Action::Call };
            let out = hands[seat].act(action, &keys[seat], NOW).unwrap();
            queue.push_back((seat, out));
        }
        for (i, h) in hands.iter().enumerate() {
            assert!(h.betting_over(), "seat {i} settled");
            assert!(h.checkpoint8().is_some(), "seat {i} holds the boundary checkpoint");
            assert!(h.terminal().is_some(), "seat {i} holds TERMINAL(k)");
        }
        assert_eq!(hands[0].checkpoint8(), hands[2].checkpoint8(), "the bystander agrees");
        assert_eq!(settlements.len(), 2, "one settlement frame from each required seat");
        let init_frames: Vec<Vec<u8>> = bytes_of(&inits[0]).into_iter().chain(bytes_of(&inits[1])).collect();
        (hands, keys, settlements, init_frames)
    }

    /// `S1-FH`: a busted seat watching the table opens exactly the hands shown
    /// at the showdown -- from the shares every seat broadcasts at the deal and
    /// the owner's own, published only when it shows -- and no other: the cards
    /// every seat at the table sees, at the same moment, and nothing more.
    #[test]
    fn a_busted_seat_watching_opens_the_hands_shown_at_the_showdown_and_no_other() {
        let (hands, _, _, _) = a_settled_hand_with_a_bystander_holding(0);
        let watcher = &hands[2];
        assert!(watcher.cards().is_none(), "dealt nothing, opens nothing of its own");
        let mut shown = 0;
        for seat in 0..2u8 {
            let owner = hands[usize::from(seat)].shown(seat);
            let opponent = hands[usize::from(1 - seat)].shown(seat);
            assert_eq!(opponent, owner, "seat {seat}: the table agrees");
            assert_eq!(watcher.shown(seat), owner, "seat {seat}: the watcher sees what the table sees");
            shown += usize::from(owner.is_some());
        }
        assert!(shown >= 1, "the first to show may not muck, so a hand was shown");
        assert_eq!(watcher.shown(2), None, "and no card of a seat dealt nothing");
    }

    /// `S1-FL`: the places a boundary decides, by the tournament rule.
    #[test]
    fn the_places_a_boundary_decides() {
        let places = |r: &(bool, Vec<SeatFinish>)| -> Vec<(SeatIdx, usize, bool)> { r.1.iter().map(|f| (f.seat, f.place, f.tied)).collect() };
        // Two seats out of chips in one hand: the one that began it with more
        // finishes higher; the last seat playing on has won.
        let r = place_seats(&[0, 1, 2], &[600, 1_400, 1_000], &[0, 3_000, 0], &[1]);
        assert!(r.0, "won");
        assert_eq!(places(&r), vec![(1, 1, false), (2, 2, false), (0, 3, false)]);
        assert!(r.1.iter().all(|f| f.players_left == 1));
        // The same chips at the start: the place is shared.
        let r = place_seats(&[0, 1, 2], &[500, 2_000, 500], &[0, 3_000, 0], &[1]);
        assert_eq!(places(&r), vec![(1, 1, false), (0, 2, true), (2, 2, true)]);
        // The tournament goes on: a seat holding chips outside it -- absent,
        // free to return -- is ahead of the seat busted now.
        let r = place_seats(&[0, 1, 2, 3], &[1_000; 4], &[1_500, 1_500, 1_000, 0], &[0, 1]);
        assert!(!r.0);
        assert_eq!(places(&r), vec![(3, 4, false)]);
        assert_eq!(r.1[0].players_left, 3, "three seats still hold chips");
        // `S1-FA`: the tournament ends heads-up beside an absent seat's chips --
        // the loser of that hand is second, the absent seat last.
        let r = place_seats(&[0, 1, 2], &[10_600, 17_200, 2_200], &[10_600, 19_400, 0], &[1]);
        assert!(r.0);
        assert_eq!(places(&r), vec![(1, 1, false), (2, 2, false), (0, 3, false)]);
        // A seat out of the table for good: its chips left at this boundary.
        let r = place_seats(&[0, 1, 2], &[1_000; 3], &[1_000, 1_000, 0], &[0, 1]);
        assert_eq!(places(&r), vec![(2, 3, false)]);
        assert_eq!(r.1[0].players_left, 2);
        // A boundary that busts nobody decides nothing.
        let r = place_seats(&[0, 1, 2], &[1_000; 3], &[900, 1_100, 1_000], &[0, 1, 2]);
        assert!(!r.0 && r.1.is_empty());
    }

    /// `S1-FM`: the first hand's opening waits a minute longer than its budget
    /// for a seat to join the table's group; no other stage and no other hand
    /// does.
    #[test]
    fn the_first_hands_opening_waits_a_minute_longer_for_its_seats() {
        let o = opening3(0);
        let budget = u64::from(o.crypto_step_timeout_ms);
        let (mut h, _) = Hand::open(o, &key(10), NOW, 30_000).unwrap();
        // `S1-GN`: not for seats gone by their own word -- unless another seat is
        // still waited for too.
        let waiting = h.waiting_for();
        assert_eq!(waiting.len(), 2, "the opening waits on both other seats");
        h.note_gone_by_their_word(&waiting[..1]);
        assert!(h.first_hand_opening_held(NOW + 1, 0), "one seat still joining holds it");
        h.note_gone_by_their_word(&waiting);
        assert!(!h.first_hand_opening_held(NOW + 1, 0), "the gone alone do not");
        h.note_gone_by_their_word(&[]);
        assert!(h.first_hand_opening_held(NOW + budget + 1, 0), "past the budget, still held");
        assert!(h.first_hand_opening_held(NOW + budget + FIRST_HAND_JOIN_ALLOWANCE_MS - 1, 0));
        assert!(!h.first_hand_opening_held(NOW + budget + FIRST_HAND_JOIN_ALLOWANCE_MS, 0), "and then not");
        assert!(h.first_hand_opening_held(NOW + budget + FIRST_HAND_JOIN_ALLOWANCE_MS, budget), "the abort waits a budget more");
        let mut o2 = opening3(0);
        o2.hand_id = 2;
        let (h2, _) = Hand::open(o2, &key(10), NOW, 30_000).unwrap();
        assert!(!h2.first_hand_opening_held(NOW + budget + 1, 0), "a later hand waits its budget only");
    }

    /// `S1-FL`: the places are final the moment the hand ends -- said then --
    /// unless the tournament's end hangs on a return still in the window.
    #[test]
    fn the_places_are_final_at_the_end_unless_a_return_can_keep_the_table_going() {
        // Two seats play on: a seat busted now has its place whatever returns.
        assert!(places_are_final(&[0, 1, 2, 3], &[1_000, 0, 2_000, 500], &[0, 2]));
        // Nobody holds chips outside the roster: the end is the end.
        assert!(places_are_final(&[0, 1, 2], &[3_000, 0, 0], &[0]));
        // One seat plays on beside an absent seat holding chips: its return
        // would keep the tournament going, so the places wait for the window.
        assert!(!places_are_final(&[0, 1, 2], &[2_000, 0, 1_000], &[0]));
    }

    /// `S1-FL`: the read-only `R(k+1)` agrees with the derivation, and a settled
    /// hand that busts nobody decides no place -- at the seats playing it and
    /// at a busted bystander alike.
    #[test]
    fn the_places_read_the_roster_the_next_hand_is_derived_from() {
        let (hands, _, _, _) = a_settled_hand_with_a_bystander_holding(0);
        for (i, h) in hands.iter().enumerate() {
            let next = h.next_hand().map(|o| o.required).expect("a next hand");
            assert_eq!(next, h.required_next(&h.end_stacks_at_boundary()), "seat {i}");
            let (over, finishes) = h.finishes_at_boundary();
            assert!(!over && finishes.is_empty(), "seat {i}: {finishes:?}");
            assert_eq!(h.finishes_final_at_the_end(), Some((over, finishes)), "seat {i}: final at the end");
        }
    }

    /// Four seats: 0 and 1 the roster, 2 and 3 bystanders with chips.
    fn opening4(my_seat: SeatIdx) -> Opening {
        let mut o = opening3(my_seat);
        o.seats.push((3, key(13).verifying_key().to_bytes(), 10_000));
        o.max_players = 4;
        o.grace = vec![GRACE_HANDS; 4];
        o.present_run = vec![0; 4];
        o
    }

    /// The subject's two signed events, which the node would hand the voters.
    fn evidence_of(subject: &mut Hand, key: &SigningKey, seat: SeatIdx) -> ReturnEvidence {
        let checkpoint = subject.state_hash_event(key, NOW).unwrap().expect("a settled bystander has a checkpoint");
        let request = subject.sit_in_request(key, NOW).unwrap().expect("a bystander with chips asks");
        ReturnEvidence { seat, request, checkpoint }
    }

    fn bytes_of(sends: &[Send]) -> Vec<Vec<u8>> {
        sends.iter().map(|Send::Broadcast(b)| b.clone()).collect()
    }

    fn certs_in(sends: &[Send], table_id: &Hash, hand_id: u64) -> Vec<Vec<u8>> {
        bytes_of(sends)
            .into_iter()
            .filter(|b| chained::open_in_hand(b, FRAME_CAP, EventType::ReturnCert, table_id, hand_id).is_ok())
            .collect()
    }

    /// Correction 1 of `READMISSION.md` §5, and §6's first trap: the voter set
    /// is `R(k) \ OUT(k)`, written fresh, and it reads neither `dealt_in` nor
    /// `grace`. Seat 1's grace is spent, so it is required and NOT dealt in --
    /// `voters()` would drop it, `return_voters()` must not.
    #[test]
    fn the_return_voter_set_is_the_roster_less_the_certified_and_reads_no_dealt_in() {
        let mut o = opening3(0);
        o.required = vec![0, 1];
        o.grace = vec![GRACE_HANDS, 0, GRACE_HANDS];
        let (h, _) = Hand::open(o, &key(10), NOW, 30_000).unwrap();
        assert_eq!(h.dealt_in(), &[0], "seat 1 is required and not dealt in");
        assert_eq!(h.return_voters(), vec![0, 1], "and it still votes on a return");
    }

    /// The structural half of the same rule: the function's own text names
    /// neither field. A faithful generalisation of `voters()` would.
    #[test]
    fn return_voters_is_written_without_dealt_in_or_grace() {
        let src = include_str!("hand.rs");
        let start = src.find("pub fn return_voters(").expect("the function exists");
        let body = &src[start..];
        let end = body[1..].find("\n    pub fn ").or_else(|| body[1..].find("\n    fn ")).expect("a next function");
        let body = &body[..end];
        assert!(!body.contains("dealt_in"), "return_voters reads dealt_in:\n{body}");
        assert!(!body.contains("grace"), "return_voters reads grace:\n{body}");
    }

    /// The whole road on three hands: the bystander asks and proves, the two
    /// voters vote, the set is unanimous, both seal, both bank, the bystander
    /// receives a copy and banks it too -- and every one of the three derives
    /// hand k+1 with seat 2 REQUIRED, at one genesis, with its grace full.
    #[test]
    fn a_return_certificate_puts_the_seat_back_into_the_roster_at_one_genesis() {
        let (mut hands, keys) = a_settled_hand_with_a_bystander();
        let ev = evidence_of(&mut hands[2], &keys[2], 2);
        let table_id = hands[0].table_id();
        let va = hands[0].vote_on_returns(std::slice::from_ref(&ev), &keys[0], NOW).unwrap();
        let vb = hands[1].vote_on_returns(std::slice::from_ref(&ev), &keys[1], NOW).unwrap();
        assert_eq!(va.len(), 1, "one vote, no certificate yet");
        assert_eq!(vb.len(), 1);
        assert!(hands[0].returned().is_empty(), "one vote moves nothing");
        let from_a = hands[1].on_event(&bytes_of(&va)[0], &keys[1], NOW).unwrap();
        let from_b = hands[0].on_event(&bytes_of(&vb)[0], &keys[0], NOW).unwrap();
        let cert_a = certs_in(&from_b, &table_id, 1);
        let cert_b = certs_in(&from_a, &table_id, 1);
        assert_eq!(cert_a.len(), 1, "the second vote seals seat 0's copy");
        assert_eq!(cert_b.len(), 1, "and seat 1's");
        assert_eq!(hands[0].returned(), &[2], "sealing banks");
        assert_eq!(hands[1].returned(), &[2]);
        assert!(hands[0].on_event(&cert_b[0], &keys[0], NOW).is_ok());
        assert!(hands[1].on_event(&cert_a[0], &keys[1], NOW).is_ok());
        // The subject never voted and holds no vote; a copy is all it needs.
        assert!(hands[2].on_event(&cert_a[0], &keys[2], NOW).is_ok());
        assert_eq!(hands[2].returned(), &[2]);
        let next: Vec<Opening> = hands.iter().map(|h| h.next_hand().expect("a successor")).collect();
        for (i, n) in next.iter().enumerate() {
            assert_eq!(n.required, vec![0, 1, 2], "seat {i}: R(k+1) = R(k) ∪ IN(k)");
            assert_eq!(n.grace[2], GRACE_HANDS, "seat {i}: a returned seat carries a full grace");
            assert_eq!(n.genesis, next[0].genesis, "seat {i}: one genesis");
        }
        // And the returned seat is dealt in at k+1, not merely required.
        let (h2, sends) = Hand::open(next[2].clone(), &keys[2], NOW, 30_000).unwrap();
        assert!(h2.dealt_in().contains(&2), "dealt in at the next hand");
        assert_eq!(sends.len(), 1, "and it signs stage 0 as a member");
    }

    /// Correction 2: the condition is the chain fact -- `TERMINAL(k)` is the
    /// `HAND_COMPLETE` stage hash -- and correction 5: there is no return at a
    /// boundary the table aborted. A hand that ABORTED refuses the certificate
    /// its settled neighbours sealed, and votes about nothing.
    #[test]
    fn a_return_needs_a_settled_terminal_and_an_abort_has_none() {
        let (mut hands, keys) = a_settled_hand_with_a_bystander();
        let ev = evidence_of(&mut hands[2], &keys[2], 2);
        let table_id = hands[0].table_id();
        let va = hands[0].vote_on_returns(std::slice::from_ref(&ev), &keys[0], NOW).unwrap();
        let vb = hands[1].vote_on_returns(std::slice::from_ref(&ev), &keys[1], NOW).unwrap();
        let from_b = hands[0].on_event(&bytes_of(&vb)[0], &keys[0], NOW).unwrap();
        let _ = hands[1].on_event(&bytes_of(&va)[0], &keys[1], NOW).unwrap();
        let cert = certs_in(&from_b, &table_id, 1).remove(0);
        // A fourth client, seat 1's twin, that gave the hand up at stage 0.
        let mut o = opening3(1);
        o.required = vec![0, 1];
        let (mut aborted, _) = Hand::open(o, &keys[1], NOW, 30_000).unwrap();
        let aborted_sends = aborted.abort_now(Abort::Deadline, &keys[1], NOW).unwrap();
        assert!(aborted.terminal().is_some(), "an abort has a terminal");
        assert_eq!(aborted.stack_at_boundary(1), 10_000, "an abort restores every stack, and the boundary stack says so");
        assert!(aborted.stacks().is_empty(), "while stacks() has nothing to say outside play -- the trap S1-CR fell into");
        assert!(!aborted.may_ask_to_sit_in(), "nobody asks at an aborted boundary");
        assert!(aborted.vote_on_returns(std::slice::from_ref(&ev), &keys[1], NOW).unwrap().is_empty());
        let refused = aborted.on_event(&cert, &keys[1], NOW);
        assert!(
            matches!(refused, Err(Failed::Elsewhere { .. })),
            "a certificate at an aborted boundary is refused, not held: {refused:?}"
        );
        assert!(aborted.returned().is_empty(), "nothing banked at an aborted boundary");
        // And the BYSTANDER's twin at an aborted boundary of its own: outside
        // R(k), with chips, holding a terminal -- and it asks nothing, for the
        // one reason that the terminal is an abort's.
        let mut o = opening3(2);
        o.required = vec![0, 1];
        let (mut bys, _) = Hand::open(o, &keys[2], NOW, 30_000).unwrap();
        let late_now = NOW + 120_000;
        let _ = bys.on_event(&bytes_of(&aborted_sends)[0], &keys[2], late_now);
        let _ = bys.replay_early(&keys[2], late_now);
        if bys.aborted().is_none() {
            let _ = bys.abort_now(Abort::Deadline, &keys[2], late_now);
        }
        assert!(bys.aborted().is_some(), "the bystander's twin ended on an abort");
        assert!(bys.terminal().is_some(), "and holds a terminal");
        assert!(!bys.may_ask_to_sit_in(), "a bystander asks nothing at an aborted boundary");
        assert!(bys.sit_in_request(&keys[2], late_now).unwrap().is_none());
        // A hand aborted at stage 0 has no successor of its own to derive; that
        // is `next_hand`'s existing answer and not the return's business.
    }

    /// Correction 2, the other half: a receiver that reached the settlement by
    /// the LATE road holds no checkpoint value of its own. It must not refuse
    /// the certificate -- that is the permanent fork with no dissent -- but
    /// accept it on the voters' unanimous word.
    #[test]
    fn a_receiver_without_a_checkpoint_of_its_own_accepts_on_the_voters_word() {
        let (mut hands, keys, settlements, inits) = a_settled_hand_with_a_bystander_and_its_settlement();
        let ev = evidence_of(&mut hands[2], &keys[2], 2);
        let table_id = hands[0].table_id();
        let va = hands[0].vote_on_returns(std::slice::from_ref(&ev), &keys[0], NOW).unwrap();
        let vb = hands[1].vote_on_returns(std::slice::from_ref(&ev), &keys[1], NOW).unwrap();
        let from_b = hands[0].on_event(&bytes_of(&vb)[0], &keys[0], NOW).unwrap();
        let _ = hands[1].on_event(&bytes_of(&va)[0], &keys[1], NOW).unwrap();
        let cert = certs_in(&from_b, &table_id, 1).remove(0);
        let terminal = hands[0].terminal().unwrap();
        // A fourth client, seat 1's twin, that gave the hand up at stage 0 and
        // then heard both settlement frames: §4.10's late road. Its terminal is
        // the settlement's and it computed no checkpoint of its own.
        let mut o = opening3(1);
        o.required = vec![0, 1];
        let (mut late, _) = Hand::open(o, &keys[1], NOW, 30_000).unwrap();
        // It heard stage 0 -- both seats are in its P(k) -- and then gave up.
        for i in &inits {
            let _ = late.on_event(i, &keys[1], NOW);
        }
        let _ = late.abort_now(Abort::Deadline, &keys[1], NOW).unwrap();
        for s in &settlements {
            let _ = late.on_event(s, &keys[1], NOW);
        }
        assert_eq!(late.terminal(), Some(terminal), "the settlement won over the abort");
        assert!(late.checkpoint8().is_none(), "and it holds no checkpoint of its own");
        assert!(late.on_event(&cert, &keys[1], NOW).is_ok(), "accepted on the voters' word");
        assert_eq!(late.returned(), &[2]);
        assert_eq!(late.next_hand().unwrap().required, vec![0, 1, 2]);
    }

    /// Correction 3: the two-voter floor does not exclude heads-up, because the
    /// subject is outside `R(k)` and removing it removes nothing. The fixture
    /// IS heads-up -- `R(k) = [0, 1]` -- and the certificate above sealed.
    #[test]
    fn heads_up_can_certify_a_return() {
        let (mut hands, keys) = a_settled_hand_with_a_bystander();
        assert_eq!(hands[0].return_voters(), vec![0, 1]);
        let ev = evidence_of(&mut hands[2], &keys[2], 2);
        assert_eq!(hands[0].vote_on_returns(std::slice::from_ref(&ev), &keys[0], NOW).unwrap().len(), 1);
    }

    /// Only on the subject's own signed request, never automatically: a
    /// certificate whose carried request is somebody else's signature, or is
    /// sealed at another seat's slot, is refused.
    #[test]
    fn a_return_needs_the_subjects_own_signed_request() {
        let (mut hands, keys) = a_settled_hand_with_a_bystander();
        let ev = evidence_of(&mut hands[2], &keys[2], 2);
        // Seat 1 forges a request "from" seat 2 with its own key.
        let terminal = hands[0].terminal().unwrap();
        let slot = hands[0].slot().at(crate::table::seatwire::window_sequence(2).unwrap(), terminal);
        let forged = chained::seal(EventType::PlayerSitIn, &slot, &Vec::<u16>::new(), &keys[1], NOW, 30_000, crate::table::seatwire::BOUNDARY_EVENT_CAP).unwrap();
        let mut bad = ev.clone();
        bad.request = forged;
        assert!(hands[0].vote_on_returns(std::slice::from_ref(&bad), &keys[0], NOW).unwrap().is_empty(), "a voter does not vote on a forged request");
        assert!(hands[0].vote_on_returns(std::slice::from_ref(&ev), &keys[0], NOW).unwrap().len() == 1, "and votes on the real one");
    }

    /// The subject's checkpoint must agree with the voter's own; a voter that
    /// holds a different value does not vote, and a receiver whose own value
    /// differs refuses the certificate.
    #[test]
    fn a_return_needs_the_subjects_agreeing_checkpoint() {
        let (mut hands, keys) = a_settled_hand_with_a_bystander();
        let mut ev = evidence_of(&mut hands[2], &keys[2], 2);
        // The subject signs a checkpoint that names a value nobody holds.
        let terminal = hands[2].terminal().unwrap();
        let body = crate::table::checkwire::StateHash {
            checkpoint: crate::table::checkwire::BOUNDARY_CHECKPOINT,
            state_hash: [0xAB; 32],
            transcript_head: terminal,
        };
        let slot = hands[2].slot().at(crate::table::checkwire::hash_sequence(0).unwrap(), terminal);
        ev.checkpoint = chained::seal(EventType::StateHash, &slot, &body, &keys[2], NOW, 30_000, STATE_HASH_CAP).unwrap();
        assert!(hands[0].vote_on_returns(std::slice::from_ref(&ev), &keys[0], NOW).unwrap().is_empty(), "a disagreeing value earns no vote");
    }

    /// A seat inside `R(k)` cannot be a return subject: it decides nothing.
    #[test]
    fn a_seat_inside_the_roster_cannot_be_a_return_subject() {
        let (mut hands, keys) = a_settled_hand_with_a_bystander();
        assert!(!hands[1].may_ask_to_sit_in(), "a required seat has nothing to ask");
        assert!(hands[1].sit_in_request(&keys[1], NOW).unwrap().is_none());
        // Seat 1's own evidence -- a genuine request at its own slot and its
        // genuine checkpoint, which opens and agrees -- earns no vote, for the
        // one reason that seat 1 is in R(k).
        let terminal = hands[0].terminal().unwrap();
        let checkpoint = hands[1].state_hash_event(&keys[1], NOW).unwrap().expect("a required seat has a checkpoint");
        let slot = hands[1].slot().at(crate::table::seatwire::window_sequence(1).unwrap(), terminal);
        let request = chained::seal(EventType::PlayerSitIn, &slot, &Vec::<u16>::new(), &keys[1], NOW, 30_000, crate::table::seatwire::BOUNDARY_EVENT_CAP).unwrap();
        let inside = ReturnEvidence { seat: 1, request, checkpoint };
        assert!(hands[0].vote_on_returns(std::slice::from_ref(&inside), &keys[0], NOW).unwrap().is_empty(), "a seat in R(k) earns no vote on its own evidence");
        // Seat 2's evidence relabelled as seat 1's is refused earlier, on the
        // signature, before the roster is consulted.
        let ev = evidence_of(&mut hands[2], &keys[2], 2);
        let mut relabelled = ev.clone();
        relabelled.seat = 1;
        assert!(hands[0].vote_on_returns(std::slice::from_ref(&relabelled), &keys[0], NOW).unwrap().is_empty());
        // And a well-formed vote from seat 1 about seat 0 -- a required seat --
        // is refused by the receiver, whatever the voter claims.
        let terminal = hands[0].terminal().unwrap();
        let state_hash = hands[0].checkpoint8().unwrap().0;
        let body = crate::table::returnwire::ReturnVote { subject_seat: 0, terminal, request_hash: [5; 32], state_hash };
        let slot = hands[1].slot().at(crate::table::returnwire::return_sequence(0).unwrap(), terminal);
        let bytes = chained::seal(EventType::ReturnVote, &slot, &body, &keys[1], NOW, 30_000, crate::table::returnwire::RETURN_VOTE_CAP).unwrap();
        let out = hands[0].on_event(&bytes, &keys[0], NOW);
        assert!(matches!(out, Err(Failed::Elsewhere { seat: 1, .. })), "{out:?}");
    }

    /// A busted seat cannot return: the roster is `∩ ALIVE(k+1)`.
    #[test]
    fn a_busted_seat_cannot_return() {
        let (mut hands, keys, _, _) = a_settled_hand_with_a_bystander_holding(0);
        assert!(!hands[2].may_ask_to_sit_in(), "no chips, no request");
        assert!(hands[2].sit_in_request(&keys[2], NOW).unwrap().is_none());
        // Its client would not ask, so the evidence is forged by hand: a
        // well-formed request at its own slot, and its genuine checkpoint.
        let terminal = hands[0].terminal().unwrap();
        let checkpoint = hands[2].state_hash_event(&keys[2], NOW).unwrap().expect("a settled bystander has a checkpoint, busted or not");
        let slot = hands[2].slot().at(crate::table::seatwire::window_sequence(2).unwrap(), terminal);
        let request = chained::seal(EventType::PlayerSitIn, &slot, &Vec::<u16>::new(), &keys[2], NOW, 30_000, crate::table::seatwire::BOUNDARY_EVENT_CAP).unwrap();
        let ev = ReturnEvidence { seat: 2, request: request.clone(), checkpoint: checkpoint.clone() };
        assert!(hands[0].vote_on_returns(std::slice::from_ref(&ev), &keys[0], NOW).unwrap().is_empty(), "no vote for a busted seat");
        // And a certificate about it, assembled by hand from two votes that
        // are well-formed in every other respect, is refused by a receiver.
        let table_id = hands[0].table_id();
        let request_hash = chained::open_in_hand(&request, FRAME_CAP, EventType::PlayerSitIn, &table_id, 1).unwrap().event_hash;
        let state_hash = hands[0].checkpoint8().unwrap().0;
        let subject = crate::table::returnwire::ReturnVote { subject_seat: 2, terminal, request_hash, state_hash };
        let seq = crate::table::returnwire::return_sequence(2).unwrap();
        let votes: Vec<Vec<u8>> = [0usize, 1]
            .iter()
            .map(|v| {
                let slot = hands[*v].slot().at(seq, terminal);
                chained::seal(EventType::ReturnVote, &slot, &subject, &keys[*v], NOW, 30_000, crate::table::returnwire::RETURN_VOTE_CAP).unwrap()
            })
            .collect();
        let cert = crate::table::returnwire::ReturnCert { subject_digest: subject.subject_digest(), votes, request, checkpoint };
        let slot = hands[1].slot().at(seq, terminal);
        let bytes = chained::seal(EventType::ReturnCert, &slot, &cert, &keys[1], NOW, 30_000, crate::table::returnwire::RETURN_CERT_CAP).unwrap();
        let out = hands[0].on_event(&bytes, &keys[0], NOW);
        assert!(matches!(out, Err(Failed::Elsewhere { seat: 1, .. })), "{out:?}");
        assert!(hands[0].returned().is_empty());
    }

    /// Correction 1's consequence: two seats returning at one boundary have
    /// the same voter set -- neither is a voter about the other -- and the two
    /// certificates applied in either order derive one `R(k+1)`.
    #[test]
    fn two_returns_at_one_boundary_do_not_depend_on_each_other() {
        let keys: Vec<SigningKey> = (10..14).map(key).collect();
        let mut hands: Vec<Hand> = Vec::new();
        let mut inits: Vec<Vec<Send>> = Vec::new();
        for seat in 0..4u8 {
            let mut o = opening4(seat);
            o.required = vec![0, 1];
            let (h, sends) = Hand::open(o, &keys[usize::from(seat)], NOW, 30_000).unwrap();
            hands.push(h);
            inits.push(sends);
        }
        let mut queue: std::collections::VecDeque<(usize, Vec<Send>)> = std::collections::VecDeque::from(vec![(0, inits[0].clone()), (1, inits[1].clone())]);
        for _ in 0..1024 {
            if let Some((from, sends)) = queue.pop_front() {
                if sends.is_empty() {
                    continue;
                }
                for to in 0..4usize {
                    if to != from {
                        let out = deliver(&mut hands[to], &sends, &keys[to]);
                        if !out.is_empty() {
                            queue.push_back((to, out));
                        }
                    }
                }
                continue;
            }
            if hands[0].betting_over() && hands[1].betting_over() {
                break;
            }
            let Some(turn) = hands[0].turn().or_else(|| hands[1].turn()) else { panic!("stuck") };
            let seat = usize::from(turn.seat);
            let turn = hands[seat].turn().expect("agrees");
            let action = if turn.legal.can_check { Action::Check } else { Action::Call };
            let out = hands[seat].act(action, &keys[seat], NOW).unwrap();
            queue.push_back((seat, out));
        }
        assert!(hands[2].betting_over() && hands[3].betting_over(), "both bystanders settled");
        assert_eq!(hands[0].return_voters(), vec![0, 1]);
        let ev2 = evidence_of(&mut hands[2], &keys[2], 2);
        let ev3 = evidence_of(&mut hands[3], &keys[3], 3);
        let table_id = hands[0].table_id();
        // Seat 0 votes on both, seat 1 on both, in opposite orders.
        let va = hands[0].vote_on_returns(&[ev2.clone(), ev3.clone()], &keys[0], NOW).unwrap();
        let vb = hands[1].vote_on_returns(&[ev3.clone(), ev2.clone()], &keys[1], NOW).unwrap();
        assert_eq!(va.len(), 2);
        assert_eq!(vb.len(), 2);
        let mut certs_a = Vec::new();
        for b in bytes_of(&vb) {
            certs_a.extend(certs_in(&hands[0].on_event(&b, &keys[0], NOW).unwrap(), &table_id, 1));
        }
        let mut certs_b = Vec::new();
        for b in bytes_of(&va) {
            certs_b.extend(certs_in(&hands[1].on_event(&b, &keys[1], NOW).unwrap(), &table_id, 1));
        }
        assert_eq!(certs_a.len(), 2, "two subjects, two certificates at seat 0");
        assert_eq!(certs_b.len(), 2);
        assert_eq!(hands[0].returned(), &[2, 3]);
        assert_eq!(hands[1].returned(), &[2, 3], "the same set, derived in the opposite order");
        let na = hands[0].next_hand().unwrap();
        let nb = hands[1].next_hand().unwrap();
        assert_eq!(na.required, vec![0, 1, 2, 3]);
        assert_eq!(na.genesis, nb.genesis, "one genesis from two orders");
    }

    /// The anti-replay binding: a vote sealed anywhere but the subject's own
    /// return slot, parented on `TERMINAL(k)`, is refused.
    #[test]
    fn a_return_vote_must_be_sealed_at_the_subjects_slot_on_the_terminal() {
        let (mut hands, keys) = a_settled_hand_with_a_bystander();
        let ev = evidence_of(&mut hands[2], &keys[2], 2);
        let terminal = hands[0].terminal().unwrap();
        let req_hash = chained::open_in_hand(&ev.request, FRAME_CAP, EventType::PlayerSitIn, &hands[0].table_id(), 1).unwrap().event_hash;
        let state_hash = hands[2].checkpoint8().unwrap().0;
        let body = crate::table::returnwire::ReturnVote { subject_seat: 2, terminal, request_hash: req_hash, state_hash };
        // Seat 1 seals a vote at the WRONG sequence (seat 3's slot).
        let wrong = hands[1].slot().at(crate::table::returnwire::return_sequence(3).unwrap(), terminal);
        let bytes = chained::seal(EventType::ReturnVote, &wrong, &body, &keys[1], NOW, 30_000, crate::table::returnwire::RETURN_VOTE_CAP).unwrap();
        let out = hands[0].on_event(&bytes, &keys[0], NOW);
        assert!(matches!(out, Err(Failed::Elsewhere { seat: 1, .. })), "{out:?}");
        // And at the right slot but parented on something else.
        let elsewhere = hands[1].slot().at(crate::table::returnwire::return_sequence(2).unwrap(), [7; 32]);
        let bytes = chained::seal(EventType::ReturnVote, &elsewhere, &body, &keys[1], NOW, 30_000, crate::table::returnwire::RETURN_VOTE_CAP).unwrap();
        let out = hands[0].on_event(&bytes, &keys[0], NOW);
        assert!(matches!(out, Err(Failed::Elsewhere { seat: 1, .. })), "{out:?}");
    }

    /// The settled-path late-roster road (`READMISSION.md` §4): a certificate
    /// banked on a hand that is over says so, and `next_hand` re-derived from
    /// it carries the seat -- the same road `S1-BS` built for the abort path.
    #[test]
    fn a_return_certificate_banked_after_the_hand_re_derives_the_next_hand() {
        let (mut hands, keys) = a_settled_hand_with_a_bystander();
        let ev = evidence_of(&mut hands[2], &keys[2], 2);
        let table_id = hands[0].table_id();
        let before = hands[1].next_hand().unwrap();
        assert_eq!(before.required, vec![0, 1], "before the certificate, the roster stands");
        let va = hands[0].vote_on_returns(std::slice::from_ref(&ev), &keys[0], NOW).unwrap();
        let vb = hands[1].vote_on_returns(std::slice::from_ref(&ev), &keys[1], NOW).unwrap();
        let from_b = hands[0].on_event(&bytes_of(&vb)[0], &keys[0], NOW).unwrap();
        let cert = certs_in(&from_b, &table_id, 1).remove(0);
        assert!(hands[0].take_late_roster(), "sealing on an ended hand says the roster moved");
        // A receiver that never voted (the subject) banks and says so too.
        assert!(hands[2].on_event(&cert, &keys[2], NOW).is_ok());
        assert!(hands[2].take_late_roster());
        assert_eq!(hands[2].next_hand().unwrap().required, vec![0, 1, 2]);
        let _ = va;
    }

    /// The emitter's predicate: a bystander with chips asks exactly once per
    /// boundary, at its own window slot, parented on `TERMINAL(k)`, with the
    /// canonical empty payload.
    #[test]
    fn a_bystander_with_chips_asks_once_at_its_own_slot() {
        let (mut hands, keys) = a_settled_hand_with_a_bystander();
        assert!(hands[2].may_ask_to_sit_in());
        let first = hands[2].sit_in_request(&keys[2], NOW).unwrap().expect("asks");
        assert!(hands[2].sit_in_request(&keys[2], NOW).unwrap().is_none(), "once");
        let terminal = hands[2].terminal().unwrap();
        let slot = hands[2].slot().at(crate::table::seatwire::window_sequence(2).unwrap(), terminal);
        let opened = chained::open(&first, FRAME_CAP, EventType::PlayerSitIn, &slot).expect("at seat 2's slot on the terminal");
        assert_eq!(opened.sender, keys[2].verifying_key().to_bytes());
        assert_eq!(opened.envelope.payload, crate::table::seatwire::SIT_IN_PAYLOAD.to_vec());
    }


    // ------------------------------------------------------------------ S1-CR

    /// Play one hand among `members` (indices into `hands`, which may also hold
    /// bystanders that follow), delivering every send to every other hand in
    /// FIFO order and acting for whoever is to act, until every member has
    /// settled. Returns every frame that went out, in order.
    fn play_out(
        hands: &mut Vec<Hand>,
        keys: &[SigningKey],
        members: &[usize],
        first: Vec<(usize, Vec<Send>)>,
    ) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        let mut queue: std::collections::VecDeque<(usize, Vec<Send>)> = std::collections::VecDeque::from(first);
        for _ in 0..4096 {
            if let Some((from, sends)) = queue.pop_front() {
                if sends.is_empty() {
                    continue;
                }
                frames.extend(bytes_of(&sends));
                for to in 0..hands.len() {
                    if to == from {
                        continue;
                    }
                    let out = deliver(&mut hands[to], &sends, &keys[to]);
                    if !out.is_empty() {
                        queue.push_back((to, out));
                    }
                }
                continue;
            }
            if members.iter().all(|m| hands[*m].betting_over()) {
                break;
            }
            let Some(seat) = members.iter().find_map(|m| hands[*m].turn().map(|t| usize::from(t.seat))) else {
                panic!("nothing in flight, nobody to act, and the hand is not over");
            };
            let turn = hands[seat].turn().expect("that hand agrees it is to act");
            let action = if turn.legal.can_check { Action::Check } else { Action::Call };
            let out = hands[seat].act(action, &keys[seat], NOW).unwrap();
            queue.push_back((seat, out));
        }
        frames
    }

    /// Four seats, three of them the roster, hand 1 played to settlement at
    /// all four (seat 3 follows as a bystander and is the control: its own
    /// `next_hand` is what an adopter must arrive at without having followed).
    fn a_table_after_hand_one() -> (Vec<Hand>, Vec<SigningKey>) {
        let keys: Vec<SigningKey> = (10..14).map(key).collect();
        let mut hands = Vec::new();
        let mut first = Vec::new();
        for seat in 0..4u8 {
            let mut o = opening4(seat);
            o.required = vec![0, 1, 2];
            let (h, sends) = Hand::open(o, &keys[usize::from(seat)], NOW, 30_000).unwrap();
            hands.push(h);
            first.push((usize::from(seat), sends));
        }
        play_out(&mut hands, &keys, &[0, 1, 2], first);
        for h in &hands {
            assert!(h.betting_over() && h.checkpoint8().is_some(), "hand 1 settled everywhere");
        }
        (hands, keys)
    }

    /// The members' signed `HAND_INIT(2)` copies, and their hands of hand 2.
    fn hand_two_copies(hands: &[Hand], keys: &[SigningKey]) -> (Vec<Vec<u8>>, Vec<Hand>) {
        let mut copies = Vec::new();
        let mut opened = Vec::new();
        for m in 0..3usize {
            let o = hands[m].next_hand().expect("a member derives hand 2");
            let (h, sends) = Hand::open(o, &keys[m], NOW, 30_000).unwrap();
            let mut b = bytes_of(&sends);
            assert_eq!(b.len(), 1, "one HAND_INIT from a member");
            copies.push(b.remove(0));
            opened.push(h);
        }
        (copies, opened)
    }

    /// The fresh client's `base`: what it knows without having followed --
    /// the table, the session, the roster's keys, its seat, the advertised
    /// parameters -- with placeholders where the copies decide.
    fn adopter_base() -> Opening {
        let mut o = opening4(3);
        o.hand_id = 2;
        o.required = Vec::new();
        o.genesis = [0; 32];
        o.roster_hash = [0; 32];
        o
    }

    /// The fields a genesis commits to, or that every member derived alike.
    fn committed(o: &Opening) -> (Hash, u64, Hash, Hash, Hash, Vec<SeatIdx>, Vec<(SeatIdx, [u8; 32], u64)>, u8, u64, u64, u16, Option<SeatIdx>) {
        (o.table_id, o.hand_id, o.session_id, o.roster_hash, o.genesis, o.required.clone(), o.seats.clone(), o.max_players, o.small_blind, o.big_blind, o.level, o.button)
    }

    /// The adopted opening is the members' own -- and the control's, seat 3's
    /// derivation after following hand 1 -- in every committed field, though
    /// the adopter followed nothing.
    #[test]
    fn an_adopted_opening_is_the_members_own_field_by_field() {
        let (hands, keys) = a_table_after_hand_one();
        let control = hands[3].next_hand().expect("the control derives hand 2");
        let (copies, _) = hand_two_copies(&hands, &keys);
        let adopted = Opening::adopt(adopter_base(), &copies).expect("three of four occupied seats agree");
        assert_eq!(committed(&adopted), committed(&control));
        assert_eq!(adopted.my_seat, 3);
        assert!(adopted.readmitted.is_empty());
        assert_eq!(adopted.required, vec![0, 1, 2]);
        assert_eq!(adopted.grace, vec![GRACE_HANDS; 4], "per-receiver accumulators start fresh");
    }

    /// `D-039`: the table's own word on who is dealt in decides membership.
    /// Four seats all in hand one; hand two's copies from three of them name
    /// seat 3 in `dealt_in`, so seat 3 -- back from a restart with no copy of
    /// its own -- adopts as a member, its opening is the table's body byte for
    /// byte under its own signature, and the members' stage 0 closes on it.
    /// Broken deliberately: with `required = signers` alone the first
    /// assertion on `required` fails.
    #[test]
    fn a_seat_the_tables_copies_deal_in_adopts_as_a_member_and_signs_the_same_body() {
        let keys: Vec<SigningKey> = (10..14).map(key).collect();
        let mut hands = Vec::new();
        let mut first = Vec::new();
        for seat in 0..4u8 {
            let mut o = opening4(seat);
            o.required = vec![0, 1, 2, 3];
            let (h, sends) = Hand::open(o, &keys[usize::from(seat)], NOW, 30_000).unwrap();
            hands.push(h);
            first.push((usize::from(seat), sends));
        }
        play_out(&mut hands, &keys, &[0, 1, 2, 3], first);
        for h in &hands {
            assert!(h.betting_over() && h.checkpoint8().is_some(), "hand 1 settled everywhere");
        }
        let (copies, mut opened) = hand_two_copies(&hands, &keys);
        let (adopted, signers) = Opening::adopt_with_signers(adopter_base(), &copies).expect("three of four");
        assert_eq!(signers, vec![0, 1, 2], "the copies' signers");
        assert_eq!(adopted.required, vec![0, 1, 2, 3], "the copies deal seat 3 in, so it is required");
        let (h3, sends) = Hand::open(adopted, &keys[3], NOW, 30_000).unwrap();
        let mine = bytes_of(&sends);
        assert_eq!(mine.len(), 1, "a member says its opening");
        let table_id = hands[0].table_id();
        let theirs = chained::open_in_hand(&copies[0], FRAME_CAP, EventType::HandInit, &table_id, 2).unwrap();
        let ours = chained::open_in_hand(&mine[0], FRAME_CAP, EventType::HandInit, &table_id, 2).unwrap();
        assert_eq!(ours.envelope.payload, theirs.envelope.payload, "the table's body, under seat 3's signature");
        assert_eq!(ours.envelope.previous_event_hash, theirs.envelope.previous_event_hash, "at the table's genesis");
        assert_eq!(h3.genesis(), opened[0].genesis(), "one hand");
        // Once the last copy is in, stage 0 closes here too and the hand owes
        // the deck stage its part: the node must say what `on_event` returns
        // for these copies (seat 3 of `run182312-4` never did, and was
        // certified out of the hand it had just been dealt into).
        let mut h3 = h3;
        let mut owed = Vec::new();
        for c in &copies {
            owed = h3.on_event(c, &keys[3], NOW + 1_000).expect("a member's copy");
        }
        assert!(h3.dealt(), "stage 0 closed on the copies: {:?}", h3.waiting_for());
        assert!(!owed.is_empty(), "the deck stage's contribution comes out of the last copy");
        // The members' stage 0 was waiting for exactly this copy.
        for c in &copies[1..] {
            opened[0].on_event(c, &keys[0], NOW + 1_000).expect("a member's copy");
        }
        assert!(!opened[0].dealt(), "stage 0 is open without seat 3: {:?}", opened[0].waiting_for());
        opened[0].on_event(&mine[0], &keys[0], NOW + 2_000).expect("seat 3's copy");
        assert!(opened[0].dealt(), "and closes on it");
    }

    /// A strict majority of the occupied seats, at one genesis: two of four
    /// is not one, a fork among the copies leaves no majority, and a copy
    /// signed by a key the roster does not hold counts for nothing.
    #[test]
    fn adoption_needs_a_strict_majority_of_the_roster_at_one_genesis() {
        let (hands, keys) = a_table_after_hand_one();
        let (copies, _) = hand_two_copies(&hands, &keys);
        // Two of the three OTHER seats is a majority; one of three is not.
        assert!(Opening::adopt(adopter_base(), &copies[..2]).is_ok(), "two of the three others carry it");
        assert!(matches!(Opening::adopt(adopter_base(), &copies[..1]), Err(Failed::NotYet)), "one of three is not a majority");
        assert!(matches!(Opening::adopt(adopter_base(), &[]), Err(Failed::NotYet)));
        // A member that derived another genesis signs a copy nobody else holds.
        let mut astray = hands[1].next_hand().unwrap();
        astray.genesis = [9; 32];
        let (_, sends) = Hand::open(astray, &keys[1], NOW, 30_000).unwrap();
        let forked = vec![copies[0].clone(), bytes_of(&sends).remove(0), copies[2].clone()];
        assert!(Opening::adopt(adopter_base(), &forked).is_ok(), "two at one genesis, one at another: the two carry it");
        let split = vec![copies[0].clone(), bytes_of(&sends).remove(0)];
        assert!(matches!(Opening::adopt(adopter_base(), &split), Err(Failed::NotYet)), "one at each genesis: no majority of three");
        // A stranger's copy, well-formed and signed, is not a roster seat's.
        let table_id = hands[0].table_id();
        let opened = chained::open_in_hand(&copies[0], FRAME_CAP, EventType::HandInit, &table_id, 2).unwrap();
        let body: HandInit = chained::payload(&opened, HAND_INIT_CAP).unwrap();
        let slot = Slot { table_id, hand_id: 2, sequence: 0, previous_event_hash: opened.envelope.previous_event_hash };
        let stranger = chained::seal(EventType::HandInit, &slot, &body, &key(99), NOW, 30_000, HAND_INIT_CAP).unwrap();
        assert!(matches!(Opening::adopt(adopter_base(), &[copies[0].clone(), stranger.clone()]), Err(Failed::NotYet)), "a stranger's copy does not count");
        let with_stranger = vec![copies[0].clone(), copies[1].clone(), copies[2].clone(), stranger];
        assert!(Opening::adopt(adopter_base(), &with_stranger).is_ok(), "the three roster copies still carry it");
    }

    /// A majority that names stacks its own roster hash does not cover, or
    /// blinds that are not the schedule's for this hand, is refused rather
    /// than followed.
    #[test]
    fn adoption_refuses_a_majority_whose_stacks_or_blinds_do_not_add_up() {
        let (hands, keys) = a_table_after_hand_one();
        let (copies, _) = hand_two_copies(&hands, &keys);
        let table_id = hands[0].table_id();
        let opened = chained::open_in_hand(&copies[0], FRAME_CAP, EventType::HandInit, &table_id, 2).unwrap();
        let body: HandInit = chained::payload(&opened, HAND_INIT_CAP).unwrap();
        let slot = Slot { table_id, hand_id: 2, sequence: 0, previous_event_hash: opened.envelope.previous_event_hash };
        let mut fat = body.clone();
        fat.stacks[0] += 1;
        let cooked: Vec<Vec<u8>> = (0..3usize)
            .map(|m| chained::seal(EventType::HandInit, &slot, &fat, &keys[m], NOW, 30_000, HAND_INIT_CAP).unwrap())
            .collect();
        assert!(matches!(Opening::adopt(adopter_base(), &cooked), Err(Failed::Elsewhere { .. })), "stacks that do not hash to the roster hash");
        let mut rich = body.clone();
        rich.small_blind *= 2;
        rich.big_blind *= 2;
        let cooked: Vec<Vec<u8>> = (0..3usize)
            .map(|m| chained::seal(EventType::HandInit, &slot, &rich, &keys[m], NOW, 30_000, HAND_INIT_CAP).unwrap())
            .collect();
        assert!(matches!(Opening::adopt(adopter_base(), &cooked), Err(Failed::Elsewhere { .. })), "blinds off the schedule");
    }

    /// The decisive test: the adopter opens hand 2 from the copies, follows
    /// it as a bystander to the settlement, and holds the members' checkpoint
    /// and terminal -- so `S1-BM`'s return can take it back at this boundary.
    #[test]
    fn an_adopted_hand_is_followed_to_the_members_own_checkpoint() {
        let (hands, keys) = a_table_after_hand_one();
        let (copies, members) = hand_two_copies(&hands, &keys);
        let adopted = Opening::adopt(adopter_base(), &copies).unwrap();
        let (adopter, sends) = Hand::open(adopted, &keys[3], NOW, 30_000).unwrap();
        assert!(sends.is_empty(), "a bystander says nothing at stage 0");
        let mut hands2: Vec<Hand> = members;
        hands2.push(adopter);
        let first: Vec<(usize, Vec<Send>)> = copies
            .iter()
            .enumerate()
            .map(|(m, c)| (m, vec![Send::Broadcast(c.clone())]))
            .collect();
        play_out(&mut hands2, &keys, &[0, 1, 2], first);
        assert!(hands2[3].betting_over(), "the adopter settled with the table");
        assert!(hands2[0].checkpoint8().is_some() && hands2[0].terminal().is_some(), "the members hold a checkpoint and a terminal");
        assert!(hands2[3].checkpoint8().is_some(), "and so does the adopter, of its own computing");
        assert_eq!(hands2[3].terminal(), hands2[0].terminal(), "one TERMINAL(2)");
        assert_eq!(hands2[3].checkpoint8(), hands2[0].checkpoint8(), "one end-of-hand state");
        assert!(hands2[3].may_ask_to_sit_in(), "outside the roster with chips at a settled boundary: it asks");
        // And its own derivation of hand 3 is the members' -- the required set
        // it adopted was exact, which the agreeing checkpoint already said.
        assert_eq!(committed(&hands2[3].next_hand().unwrap()), committed(&hands2[0].next_hand().unwrap()));
    }


    /// The node's road, exactly: the adopter opens from the copies, and then
    /// receives the whole hand -- every frame the members sent, in order -- as
    /// one batch through `hold` and `replay_early`, the way frames stashed
    /// before the adoption are replayed. `run191001-3`: the adopter held every
    /// frame of hand 6 as `NotYet` and never left stage 1.
    #[test]
    fn an_adopted_hand_replays_a_batch_of_held_frames_to_the_settlement() {
        let (hands, keys) = a_table_after_hand_one();
        let (copies, members) = hand_two_copies(&hands, &keys);
        // The members play hand 2 among themselves; every frame is kept.
        let mut hands2: Vec<Hand> = members;
        let first: Vec<(usize, Vec<Send>)> = copies
            .iter()
            .enumerate()
            .map(|(m, c)| (m, vec![Send::Broadcast(c.clone())]))
            .collect();
        let frames = play_out(&mut hands2, &keys, &[0, 1, 2], first);
        assert!(hands2[0].betting_over());
        // A late adopter: the copies first, then the batch.
        let adopted = Opening::adopt(adopter_base(), &copies).unwrap();
        let (mut late, _) = Hand::open(adopted, &keys[3], NOW, 30_000).unwrap();
        for c in &copies {
            late.on_event(c, &keys[3], NOW).expect("a copy of the stage this hand opened at");
        }
        assert_eq!(late.slot().sequence, 1, "stage 0 closed on the copies");
        // The members' stage-0 hash is the parent every stage-1 frame carries.
        let table_id = hands2[0].table_id();
        let members_stage0 = frames
            .iter()
            .find_map(|b| {
                let (kind, _, seq) = chained::peek(b, FRAME_CAP).ok()?;
                if seq != 1 {
                    return None;
                }
                chained::open_in_hand(b, FRAME_CAP, kind, &table_id, 2).ok().map(|o| o.envelope.previous_event_hash)
            })
            .expect("a stage-1 frame among the members' own");
        assert_eq!(late.slot().previous_event_hash, members_stage0, "the adopter's stage-0 hash is the members' own");
        let mut kept = 0usize;
        for b in &frames {
            if !copies.contains(b) && matches!(late.hold(b.clone()), Holding::Kept) {
                kept += 1;
            }
        }
        assert!(kept > 10, "the hand's frames are held: {kept}");
        let (_, failures) = late.replay_early(&keys[3], NOW);
        assert!(failures.is_empty(), "no frame refused on replay: {failures:?}");
        assert_eq!(late.held(), 0, "nothing left held after the replay; waiting for {:?} at sequence {}", late.waiting_for(), late.slot().sequence);
        assert!(late.betting_over(), "the adopter settled from the batch");
        assert_eq!(late.checkpoint8(), hands2[0].checkpoint8(), "and holds the members' checkpoint");
    }


    /// `S1-CW`: three seats open hand k and seat 2 never says its
    /// `HAND_INIT`. Seats 0 and 1 wait at stage 0, vote at the budget, seal,
    /// exchange the copies -- and the hand ends with seat 2 named at both,
    /// with one `GENESIS(k+1)` that leaves seat 2 out. Measured twice on the
    /// `S1-CR` bed (`run195623-3`, `run190228-3`): both survivors sealed
    /// (*the table has certified seat 1's timeout, unanimously among [0, 2]*)
    /// and then printed *waiting for seats* with an empty list, and nothing
    /// ended the hand for the rest of the run.
    #[test]
    fn a_seat_silent_at_stage_zero_is_certified_out_and_the_hand_ends() {
        let (mut a, from_a) = Hand::open(opening3(0), &key(10), NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening3(1), &key(11), NOW, 30_000).unwrap();
        let keys = [key(10), key(11), key(12)];
        let _ = deliver(&mut b, &from_a, &keys[1]);
        let _ = deliver(&mut a, &from_b, &keys[0]);
        assert_eq!(a.waiting_for(), vec![2], "stage 0 waiting on seat 2");
        assert_eq!(a.slot().sequence, 0, "still at stage 0");

        let t1 = NOW + 30_000;
        let va = a.vote_on_timeouts(&keys[0], t1, 0).unwrap();
        let vb = b.vote_on_timeouts(&keys[1], t1, 0).unwrap();
        assert!(!va.is_empty() && !vb.is_empty(), "both voters vote at the budget");
        let Send::Broadcast(va) = &va[0];
        let Send::Broadcast(vb) = &vb[0];
        let t2 = NOW + 31_000;
        let ca = a.on_event(vb, &keys[0], t2).unwrap();
        let cb = b.on_event(va, &keys[1], t2).unwrap();
        assert!(!ca.is_empty() && !cb.is_empty(), "each voter seals a copy");
        let Send::Broadcast(ca) = &ca[0];
        let Send::Broadcast(cb) = &cb[0];
        a.on_event(cb, &keys[0], t2).expect("the peer's certificate holds at seat 0");
        b.on_event(ca, &keys[1], t2).expect("the peer's certificate holds at seat 1");
        assert_eq!(
            a.aborted(),
            Some(Abort::Told { cause: 1 }),
            "ended by the certificate at seat 0, with the seat named; waiting for {:?}",
            a.waiting_for()
        );
        assert_eq!(b.aborted(), Some(Abort::Told { cause: 1 }), "and at seat 1");
        assert!(!a.took_part(2), "seat 2 leaves the roster");
        let na = a.next_hand().expect("an aborted hand has a successor");
        let nb = b.next_hand().expect("on both");
        assert!(!na.required.contains(&2), "{:?}", na.required);
        assert_eq!(na.genesis, nb.genesis, "one GENESIS(k+1)");
    }

    /// The three-seat fixture widened to `n` seats: keys `key(10 + seat)`,
    /// every seat with 10 000.
    fn opening_n(n: u8, my_seat: SeatIdx) -> Opening {
        let mut o = opening3(my_seat);
        o.required = (0..n).collect();
        o.seats = (0..n)
            .map(|s| (s, key(10 + s).verifying_key().to_bytes(), 10_000))
            .collect();
        o.max_players = n;
        o.grace = vec![GRACE_HANDS; usize::from(n)];
        o.present_run = vec![0; usize::from(n)];
        o.returns = vec![0; usize::from(n)];
        o
    }

    /// D-036: seats 0, 1 and 2 of a five-seat table open the hand and hear
    /// each other; seats 3 and 4 never say anything. The three hands and the
    /// five keys.
    fn three_present_two_quiet() -> (Vec<Hand>, Vec<SigningKey>) {
        let keys: Vec<SigningKey> = (0..5u8).map(|s| key(10 + s)).collect();
        let mut hands = Vec::new();
        let mut inits = Vec::new();
        for seat in 0..3u8 {
            let (h, from) =
                Hand::open(opening_n(5, seat), &keys[usize::from(seat)], NOW, 30_000).unwrap();
            hands.push(h);
            inits.push(from);
        }
        for i in 0..3 {
            for j in 0..3 {
                if i != j {
                    let _ = deliver(&mut hands[i], &inits[j], &keys[i]);
                }
            }
        }
        for h in &hands {
            assert_eq!(h.waiting_for(), vec![3, 4], "stage 0 waits on the two quiet seats");
        }
        (hands, keys)
    }

    fn bytes_of_sends(out: Vec<Send>) -> Vec<Vec<u8>> {
        out.into_iter().map(|Send::Broadcast(b)| b).collect()
    }

    /// D-036: two seats quiet at one stage are certified **together**. Every
    /// seat outside the pair votes about each of them, the certificate names
    /// the pair with every outside seat's vote about each, the hand ends with
    /// both named, and the next hand opens without them at one genesis.
    /// Before D-036 no certificate could form: each quiet seat was a voter
    /// about the other (`S1-CT`).
    #[test]
    fn seats_quiet_together_at_one_stage_are_certified_together_and_the_hand_ends() {
        let (mut hands, keys) = three_present_two_quiet();
        let t1 = NOW + 30_000;
        let mut votes: Vec<Vec<Vec<u8>>> = Vec::new();
        for i in 0..3 {
            let out = bytes_of_sends(hands[i].vote_on_timeouts(&keys[i], t1, 0).unwrap());
            assert_eq!(out.len(), 2, "seat {i} votes about both quiet seats");
            votes.push(out);
        }
        // The votes cross; the last one each seat hears completes its case,
        // and it seals a certificate about the pair.
        let mut copies: Vec<Vec<u8>> = vec![Vec::new(); 3];
        for i in 0..3 {
            for j in 0..3 {
                if i == j {
                    continue;
                }
                for v in &votes[j] {
                    for b in bytes_of_sends(hands[i].on_event(v, &keys[i], t1 + 1_000).unwrap()) {
                        copies[i] = b;
                    }
                }
            }
            assert!(!copies[i].is_empty(), "seat {i} seals once it holds everybody's votes about both");
            let note = hands[i].take_cert_note().expect("said");
            assert!(note.contains("[3, 4]"), "{note}");
        }
        for i in 0..3 {
            for j in 0..3 {
                if i != j {
                    hands[i].on_event(&copies[j], &keys[i], t1 + 2_000).expect("a peer's copy holds");
                }
            }
            assert_eq!(hands[i].aborted(), Some(Abort::Told { cause: 1 }), "seat {i}: ended by the certificate");
            assert!(!hands[i].took_part(3) && !hands[i].took_part(4), "seat {i}: both quiet seats leave the roster");
            assert!(hands[i].took_part(0) && hands[i].took_part(1) && hands[i].took_part(2));
        }
        let nexts: Vec<Opening> = hands.iter().map(|h| h.next_hand().expect("a successor")).collect();
        for n in &nexts {
            assert_eq!(n.required, vec![0, 1, 2], "{:?}", n.required);
            assert_eq!(n.genesis, nexts[0].genesis, "one GENESIS(k+1)");
        }
    }

    /// `S1-GK`: seats whose players left by their own signed word, out of the
    /// table's group, are voted about at once -- not at the stage's deadline --
    /// and certified out together as any quiet seats; a seat not so noted, or
    /// noted and back in the group, keeps its clock.
    #[test]
    fn seats_gone_by_their_own_word_are_voted_about_at_once() {
        let (mut hands, keys) = three_present_two_quiet();
        let early = NOW + 1_000;
        for i in 0..3 {
            let out = bytes_of_sends(hands[i].vote_on_timeouts(&keys[i], early, 0).unwrap());
            assert!(out.is_empty(), "seat {i}: inside the deadline nobody is voted about");
        }
        for h in hands.iter_mut() {
            h.note_gone_by_their_word(&[3, 4]);
        }
        // Mid-delivery bits do not hold a vote about a seat that said it left.
        let mut votes: Vec<Vec<Vec<u8>>> = Vec::new();
        for i in 0..3 {
            let out = bytes_of_sends(hands[i].vote_on_timeouts(&keys[i], early, 0b11000).unwrap());
            assert_eq!(out.len(), 2, "seat {i} votes about both at once");
            votes.push(out);
        }
        let mut copies: Vec<Vec<u8>> = vec![Vec::new(); 3];
        for i in 0..3 {
            for j in 0..3 {
                if i == j {
                    continue;
                }
                for v in &votes[j] {
                    for b in bytes_of_sends(hands[i].on_event(v, &keys[i], early + 100).unwrap()) {
                        copies[i] = b;
                    }
                }
            }
            assert!(!copies[i].is_empty(), "seat {i} seals a certificate about the pair");
        }
        for i in 0..3 {
            for j in 0..3 {
                if i != j {
                    hands[i].on_event(&copies[j], &keys[i], early + 200).expect("a peer's copy holds");
                }
            }
            assert!(!hands[i].took_part(3) && !hands[i].took_part(4), "seat {i}: both leave the roster");
        }
        let nexts: Vec<Opening> = hands.iter().map(|h| h.next_hand().expect("a successor")).collect();
        for n in &nexts {
            assert_eq!(n.required, vec![0, 1, 2]);
            assert_eq!(n.genesis, nexts[0].genesis, "one GENESIS(k+1)");
        }

        // Noted, then back in the group: the clock again.
        let (mut again, keys) = three_present_two_quiet();
        again[0].note_gone_by_their_word(&[3, 4]);
        again[0].note_gone_by_their_word(&[]);
        assert!(bytes_of_sends(again[0].vote_on_timeouts(&keys[0], early, 0).unwrap()).is_empty());
    }

    /// `D-063`: two of three players leave by their own signed word: the seat
    /// left certifies them out alone -- their words are their consent, and the
    /// floor asks nothing of it -- the hand ends, both are out for good, and the
    /// next hand opens with the one seat. Without the words the floor holds.
    #[test]
    fn the_seats_that_left_by_their_own_word_are_certified_out_without_the_floor() {
        let keys: Vec<SigningKey> = (0..3u8).map(|s| key(10 + s)).collect();
        let (mut a, _) = Hand::open(opening_n(3, 0), &keys[0], NOW, 30_000).unwrap();
        assert_eq!(a.waiting_for(), vec![1, 2]);
        let table = a.open.table_id;
        let w1 = crate::net::tabletalk::leave_word(&keys[1], &table, NOW).unwrap();
        let w2 = crate::net::tabletalk::leave_word(&keys[2], &table, NOW).unwrap();
        a.note_gone_by_their_word(&[1, 2]);
        assert!(
            a.vote_on_timeouts(&keys[0], NOW + 1_000, 0).unwrap().is_empty(),
            "one voter cannot name two quiet seats: the floor"
        );
        a.note_leave_words(&[(1, w1), (2, w2)]);
        let out = bytes_of_sends(a.vote_on_timeouts(&keys[0], NOW + 1_000, 0).unwrap());
        assert!(out.len() >= 3, "two votes and the certificate, at least: {}", out.len());
        assert!(a.aborted().is_some(), "the hand ended by the certificate");
        assert!(!a.took_part(1) && !a.took_part(2), "both leave the roster");
        assert_eq!(a.out_for_good(), vec![1, 2], "resigned: out for good, not absent");
        assert!(a.next_hand().is_none(), "one seat left: the tournament is over, and there is no next hand");
    }

    /// `D-063`: at five seats with two present, one quiet seat and two that
    /// resigned are certified together by the two -- the floor counts the
    /// quiet one alone -- and the copy each seals carries the words, which the
    /// other checks as it checks the votes; one genesis without the three.
    #[test]
    fn a_resignation_lets_two_voters_certify_the_rest() {
        let keys: Vec<SigningKey> = (0..5u8).map(|s| key(10 + s)).collect();
        let (mut a, from_a) = Hand::open(opening_n(5, 0), &keys[0], NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening_n(5, 1), &keys[1], NOW, 30_000).unwrap();
        let _ = deliver(&mut b, &from_a, &keys[1]);
        let _ = deliver(&mut a, &from_b, &keys[0]);
        let table = a.open.table_id;
        let w3 = crate::net::tabletalk::leave_word(&keys[3], &table, NOW).unwrap();
        let w4 = crate::net::tabletalk::leave_word(&keys[4], &table, NOW).unwrap();
        for h in [&mut a, &mut b] {
            h.note_gone_by_their_word(&[3, 4]);
            h.note_leave_words(&[(3, w3.clone()), (4, w4.clone())]);
        }
        let t1 = NOW + 30_000;
        let va = bytes_of_sends(a.vote_on_timeouts(&keys[0], t1, 0).unwrap());
        assert_eq!(va.len(), 3, "seat 0 votes about the quiet seat and both that left");
        let vb = bytes_of_sends(b.vote_on_timeouts(&keys[1], t1, 0).unwrap());
        assert_eq!(vb.len(), 3);
        let mut cert_a = Vec::new();
        for v in &vb {
            for x in bytes_of_sends(a.on_event(v, &keys[0], t1 + 100).unwrap()) {
                cert_a = x;
            }
        }
        assert!(!cert_a.is_empty(), "seat 0 seals: two voters, one quiet seat and two resigned");
        let mut cert_b = Vec::new();
        for v in &va {
            for x in bytes_of_sends(b.on_event(v, &keys[1], t1 + 100).unwrap()) {
                cert_b = x;
            }
        }
        assert!(!cert_b.is_empty());
        b.on_event(&cert_a, &keys[1], t1 + 200).expect("seat 0's copy, its words checked, holds at seat 1");
        a.on_event(&cert_b, &keys[0], t1 + 200).expect("seat 1's copy holds at seat 0");
        for h in [&a, &b] {
            assert!(!h.took_part(2) && !h.took_part(3) && !h.took_part(4));
            assert_eq!(h.out_for_good(), vec![3, 4], "the two that resigned are out for good; the quiet one is absent");
        }
        let na = a.next_hand().expect("a successor");
        let nb = b.next_hand().expect("a successor");
        assert_eq!(na.required, vec![0, 1]);
        assert_eq!(na.genesis, nb.genesis, "one GENESIS(k+1)");
    }

    /// `D-063`: a word that is not the seat's own -- another key's, or for
    /// another table -- is no resignation, and the floor holds.
    #[test]
    fn a_word_that_is_not_the_seats_own_is_no_resignation() {
        let keys: Vec<SigningKey> = (0..3u8).map(|s| key(10 + s)).collect();
        let (mut a, _) = Hand::open(opening_n(3, 0), &keys[0], NOW, 30_000).unwrap();
        let table = a.open.table_id;
        let forged = crate::net::tabletalk::leave_word(&key(99), &table, NOW).unwrap();
        let elsewhere = crate::net::tabletalk::leave_word(&keys[2], &[8u8; 32], NOW).unwrap();
        a.note_gone_by_their_word(&[1, 2]);
        a.note_leave_words(&[(1, forged), (2, elsewhere)]);
        assert!(
            a.vote_on_timeouts(&keys[0], NOW + 1_000, 0).unwrap().is_empty(),
            "neither word is the seat's own for this table: the floor holds"
        );
    }

    /// `S1-HA`: with a voter of the open round gone from the table's group the
    /// hand may be given up at the stage's deadline; with the voters all in
    /// the group it waits for the round's air, as before; and a quiet seat gone
    /// from the group is no voter and changes nothing.
    #[test]
    fn a_voter_gone_from_the_group_ends_the_hand_at_the_deadline() {
        let (mut hands, keys) = three_present_two_quiet();
        let t1 = NOW + 30_000;
        let out = hands[0].vote_on_timeouts(&keys[0], t1, 0).unwrap();
        assert_eq!(out.len(), 2, "seat 0 votes about both quiet seats");
        assert!(!hands[0].may_abandon(t1 + 1_000), "every voter in the group: the round gets its air");
        hands[0].note_gone_from_group(&[3]);
        assert!(!hands[0].may_abandon(t1 + 1_000), "a quiet seat gone is no voter");
        hands[0].note_gone_from_group(&[1]);
        assert!(hands[0].may_abandon(t1 + 1_000), "voter 1 gone from the group: no certificate can close, the deadline ends it");
        hands[0].note_gone_from_group(&[]);
        assert!(!hands[0].may_abandon(t1 + 1_000), "back in the group: the air again");
    }

    /// `D-051`: seats every voter's client cut off for flooding the table's
    /// group are certified with the flood cause -- together, as D-036 -- and
    /// are out of the table for good at the boundary: chips gone, not
    /// required, carried in `out`, one genesis. The flooders' own votes are
    /// never needed: they are the seats named.
    #[test]
    fn seats_every_voter_cut_off_for_flooding_are_out_of_the_table_for_good() {
        let (mut hands, keys) = three_present_two_quiet();
        for h in hands.iter_mut() {
            h.note_flooders(&[3, 4]);
        }
        let t1 = NOW + 30_000;
        let mut votes: Vec<Vec<Vec<u8>>> = Vec::new();
        for i in 0..3 {
            let out = bytes_of_sends(hands[i].vote_on_timeouts(&keys[i], t1, 0).unwrap());
            assert_eq!(out.len(), 2, "seat {i} votes about both flooders");
            votes.push(out);
        }
        let mut copies: Vec<Vec<u8>> = vec![Vec::new(); 3];
        for i in 0..3 {
            for j in 0..3 {
                if i == j {
                    continue;
                }
                for v in &votes[j] {
                    for b in bytes_of_sends(hands[i].on_event(v, &keys[i], t1 + 1_000).unwrap()) {
                        copies[i] = b;
                    }
                }
            }
            assert!(!copies[i].is_empty(), "seat {i} seals a certificate about the pair");
        }
        for i in 0..3 {
            for j in 0..3 {
                if i != j {
                    hands[i].on_event(&copies[j], &keys[i], t1 + 2_000).expect("a peer's copy holds");
                }
            }
            assert!(hands[i].named_for_flooding(3) && hands[i].named_for_flooding(4), "seat {i}: both named with the cause");
            assert_eq!(hands[i].out_for_good(), vec![3, 4], "seat {i}: out for good");
            assert_eq!(
                certificate_cause(&copies[i], &hands[i].open.table_id, hands[i].open.hand_id, 3),
                Some(CAUSE_FLOOD),
                "seat {i}: the bytes say why, for the seat's own client"
            );
        }
        let nexts: Vec<Opening> = hands.iter().map(|h| h.next_hand().expect("a successor")).collect();
        for n in &nexts {
            assert_eq!(n.required, vec![0, 1, 2]);
            assert_eq!(n.out, vec![3, 4], "carried");
            for s in [3u8, 4] {
                assert_eq!(n.seats.iter().find(|x| x.0 == s).map(|x| x.2), Some(0), "seat {s}'s chips left the table");
            }
            assert_eq!(n.genesis, nexts[0].genesis, "one GENESIS(k+1)");
        }
    }

    /// `D-051`: the cause is part of the subject. A voter that cut the seats
    /// off alone votes about another subject than the voters that did not, so
    /// nobody's set completes -- one client's measurement puts nobody out, and
    /// the table is back to waiting on its deadline, as with any vote that is
    /// not everybody's.
    #[test]
    fn a_flood_cause_one_voter_alone_names_completes_no_certificate() {
        let (mut hands, keys) = three_present_two_quiet();
        hands[0].note_flooders(&[3, 4]);
        let t1 = NOW + 30_000;
        let votes: Vec<Vec<Vec<u8>>> = (0..3)
            .map(|i| bytes_of_sends(hands[i].vote_on_timeouts(&keys[i], t1, 0).unwrap()))
            .collect();
        for i in 0..3 {
            for j in 0..3 {
                if i == j {
                    continue;
                }
                for v in &votes[j] {
                    let out = hands[i].on_event(v, &keys[i], t1 + 1_000);
                    assert!(out.map(|o| o.is_empty()).unwrap_or(true), "seat {i} seals nothing");
                }
            }
            assert!(hands[i].out_for_good().is_empty(), "seat {i}: nobody out");
            assert!(hands[i].aborted().is_none(), "seat {i}: no certificate ended the hand");
        }
        // And the voter that cut them off votes once about each seat at this
        // stage, whatever it learns later.
        hands[1].note_flooders(&[3, 4]);
        assert!(
            hands[1].vote_on_timeouts(&keys[1], t1 + 1_500, 0).unwrap().is_empty(),
            "a seat already voted about at this stage is not voted about again with a cause"
        );
    }

    /// D-034: a seat's own clock is the timeout the window shows plus what is
    /// left of its reserve, and not the grace -- the grace is the others'
    /// allowance for the round trip. Broken deliberately: adding the grace
    /// back reddens the first assertion.
    #[test]
    fn a_seats_own_clock_is_its_timeout_and_reserve_and_not_the_grace() {
        let mut o = opening_n(3, 0);
        o.action_timeout_ms = 30_000;
        o.action_grace_ms = 3_000;
        o.time_bank_ms = 0;
        let (h, _) = Hand::open(o, &key(10), NOW, 30_000).unwrap();
        assert_eq!(h.action_deadline(), std::time::Duration::from_secs(30), "the thirty seconds on the window");
        let mut o = opening_n(3, 0);
        o.action_timeout_ms = 30_000;
        o.action_grace_ms = 3_000;
        o.time_bank_ms = 5_000;
        let (h, _) = Hand::open(o, &key(10), NOW, 30_000).unwrap();
        assert_eq!(h.action_deadline(), std::time::Duration::from_secs(35), "plus the reserve, when the table has one");
    }

    /// D-036: the voters must outnumber the quiet. Two present seats cannot
    /// certify three quiet ones, so they do not vote at all, and the hand
    /// ends on the deadline naming nobody, as before.
    #[test]
    fn a_joint_certificate_needs_the_voters_to_outnumber_the_quiet() {
        let keys: Vec<SigningKey> = (0..5u8).map(|s| key(10 + s)).collect();
        let (mut a, from_a) = Hand::open(opening_n(5, 0), &keys[0], NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening_n(5, 1), &keys[1], NOW, 30_000).unwrap();
        let _ = deliver(&mut b, &from_a, &keys[1]);
        let _ = deliver(&mut a, &from_b, &keys[0]);
        assert_eq!(a.waiting_for(), vec![2, 3, 4]);
        let t1 = NOW + 30_000;
        assert!(a.vote_on_timeouts(&keys[0], t1, 0).unwrap().is_empty(), "no vote where no certificate can follow");
        assert!(b.vote_on_timeouts(&keys[1], t1, 0).unwrap().is_empty());
        // And the player is told why the hand waits, once: the seats that
        // stopped are more than half the table, so the floor holds.
        let note = a.take_cert_note().expect("the player is told");
        assert!(note.contains("no certificate can remove them") && note.contains("[2, 3, 4]"), "{note}");
        assert!(a.vote_on_timeouts(&keys[0], t1 + 500, 0).unwrap().is_empty());
        assert!(a.take_cert_note().is_none(), "said once per hand");
        // No certificate to wait for, so the stage budget alone ends it.
        let t2 = t1 + 1_000;
        assert!(a.may_abandon(t2), "no certificate possible: the deadline ends it at one budget");
        let out = a.abort_now(Abort::Deadline, &keys[0], t2).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(a.aborted(), Some(Abort::Deadline));
        let next = a.next_hand().expect("a successor");
        assert_eq!(next.required, vec![0, 1, 2, 3, 4], "nobody named, nobody shed");
    }

    /// D-036: a peer's certificate carries the votes it is made of, so a
    /// seat that never received one of them takes them from the copy, seals
    /// the same set itself, and the stage completes when the third copy comes.
    #[test]
    fn a_peer_copy_carries_the_votes_this_client_lacks_and_it_seals_the_same_set() {
        let (mut hands, keys) = three_present_two_quiet();
        let t1 = NOW + 30_000;
        let votes: Vec<Vec<Vec<u8>>> = (0..3)
            .map(|i| bytes_of_sends(hands[i].vote_on_timeouts(&keys[i], t1, 0).unwrap()))
            .collect();
        // Seat 0 hears everybody and seals.
        let mut copy0 = Vec::new();
        for j in 1..3 {
            for v in &votes[j] {
                for b in bytes_of_sends(hands[0].on_event(v, &keys[0], t1 + 1_000).unwrap()) {
                    copy0 = b;
                }
            }
        }
        assert!(!copy0.is_empty());
        // Seat 2 hears seat 0's votes only, and seals nothing.
        for v in &votes[0] {
            assert!(hands[2].on_event(v, &keys[2], t1 + 1_000).unwrap().is_empty());
        }
        // Seat 0's copy arrives: it carries seat 1's votes, and seat 2 seals.
        let sealed = bytes_of_sends(hands[2].on_event(&copy0, &keys[2], t1 + 2_000).unwrap());
        assert_eq!(sealed.len(), 1, "seat 2 seals its own copy from the carried votes");
        // Seat 1 hears everybody and seals; its copy completes the stage at seat 2.
        let mut copy1 = Vec::new();
        for j in [0usize, 2] {
            for v in &votes[j] {
                for b in bytes_of_sends(hands[1].on_event(v, &keys[1], t1 + 1_000).unwrap()) {
                    copy1 = b;
                }
            }
        }
        hands[2].on_event(&copy1, &keys[2], t1 + 3_000).expect("seat 1's copy holds");
        assert_eq!(hands[2].aborted(), Some(Abort::Told { cause: 1 }), "complete: 0, 1 and 2 about [3, 4]");
    }

    /// `S1-CX`, heads-up: a seat that stopped is still required in the next
    /// hand -- there is no table without both -- and the genesis that hand
    /// opens at is known before this one is given up.
    #[test]
    fn heads_up_a_seat_that_stopped_is_still_required_next_hand() {
        let keys = [key(10), key(11)];
        let (mut a, _from_a) = Hand::open(opening(0), &keys[0], NOW, 30_000).unwrap();
        // Seat 1 never says anything in this hand.
        let foreseen = a.genesis_if_given_up().expect("a hand not yet over can say");
        assert!(a.abort_now(Abort::Deadline, &keys[0], NOW + 30_000).is_ok());
        let next = a.next_hand().expect("two seats with chips is a table");
        assert_eq!(next.required, vec![0, 1], "the seat that stopped is still waited for");
        assert_eq!(next.genesis, foreseen, "and the give-up opened exactly where it was foreseen");
        assert!(a.genesis_if_given_up().is_none(), "nothing to foresee once the hand is over");
    }

    /// The two-seat fixture with the roster hash its seats and stacks really
    /// hash to: `Opening::adopt` checks a copy's stacks against it.
    fn heads_up_opening(my_seat: SeatIdx) -> Opening {
        let mut o = opening(my_seat);
        let roster: Vec<crate::protocol::transcript::RosterSeat> = o
            .seats
            .iter()
            .map(|(seat, key, stack)| crate::protocol::transcript::RosterSeat {
                seat: *seat,
                app_public_key: *key,
                stack_at_hand_start: *stack,
            })
            .collect();
        o.roster_hash = crate::protocol::transcript::roster_hash(&roster);
        o
    }

    /// `S1-CX`, heads-up: the returning seat adopts the running hand from the
    /// one other seat's copy as a member -- it says its own opening (which
    /// the node must send), the survivor's copy completes stage 0 there, and
    /// the stage budget holds for it like for any member, so a hand the
    /// survivor no longer answers in ends here too and the next opens with
    /// both seats.
    #[test]
    fn heads_up_a_returning_seat_adopts_the_running_hand_as_a_member() {
        let keys = [key(10), key(11)];
        let (mut a, from_a) = Hand::open(heads_up_opening(0), &keys[0], NOW, 30_000).unwrap();
        let copy = bytes_of(&from_a);
        assert_eq!(copy.len(), 1, "one opening from the survivor");
        let o = Opening::adopt(heads_up_opening(1), &copy).expect("the one other seat's copy carries it");
        // `D-039`: the survivor's body deals this seat in, so it is required.
        assert_eq!(o.required, vec![0, 1], "the table's own word on who is dealt in");
        let (mut b, from_b) = Hand::open(o, &keys[1], NOW + 5_000, 30_000).unwrap();
        assert!(!from_b.is_empty(), "a member says its opening, and the node must send it");
        assert_eq!(a.genesis(), b.genesis(), "one hand");
        deliver(&mut b, &from_a, &keys[1]);
        deliver(&mut a, &from_b, &keys[0]);
        assert!(a.dealt(), "the survivor's stage 0 is complete (it waits for {:?}, the shuffle)", a.waiting_for());
        // Nothing more comes from the survivor: the budget holds for the adopted
        // member -- the heads-up one, since both seats have signed this hand.
        assert!(!b.may_abandon(NOW + 5_000 + 31_000), "not on the ordinary budget: the other seat is in this hand");
        assert!(b.may_abandon(NOW + 5_000 + 101_000), "the heads-up budget holds for the adopted member");
        assert!(b.abort_now(Abort::Deadline, &keys[1], NOW + 106_000).is_ok());
        assert!(b.over());
        assert_eq!(b.next_hand().expect("a table").required, vec![0, 1]);
    }

    /// The shape `run085603-2` showed: the survivor had already given the hand
    /// up when the returning seat adopted it, so the copy comes with the
    /// give-up. A give-up from the other seat waits for this client's own
    /// deadline (§4.10), and at that deadline the hand ends here too -- by the
    /// held give-up or by this client's own -- and both open the same next hand.
    #[test]
    fn heads_up_a_survivors_give_up_ends_the_adopted_hand_at_this_clients_deadline() {
        let keys = [key(10), key(11)];
        let (mut a, from_a) = Hand::open(heads_up_opening(0), &keys[0], NOW, 30_000).unwrap();
        let gave_up = a.abort_now(Abort::Deadline, &keys[0], NOW + 30_000).unwrap();
        assert!(a.over());
        let o = Opening::adopt(heads_up_opening(1), &bytes_of(&from_a)).unwrap();
        assert_eq!(o.required, vec![0, 1], "the survivor's body deals this seat in (D-039)");
        let adopted_at = NOW + 40_000;
        let (mut b, _) = Hand::open(o, &keys[1], adopted_at, 30_000).unwrap();
        deliver(&mut b, &from_a, &keys[1]);
        for Send::Broadcast(bytes) in &gave_up {
            let _ = b.hold(bytes.clone());
        }
        let (_, failures) = b.replay_early(&keys[1], adopted_at);
        assert!(failures.is_empty(), "{failures:?}");
        assert!(!b.over(), "a give-up from the other seat waits for this client's own deadline");
        let late = adopted_at + 31_000;
        let _ = b.replay_early(&keys[1], late);
        if !b.over() {
            assert!(b.may_abandon(late), "at this client's deadline the hand ends, one way or the other");
            assert!(b.abort_now(Abort::Deadline, &keys[1], late).is_ok());
        }
        assert!(b.over());
        let (na, nb) = (a.next_hand().expect("a table"), b.next_hand().expect("a table"));
        assert_eq!(nb.required, vec![0, 1]);
        assert_eq!(na.genesis, nb.genesis, "and both open the same next hand");
    }

    /// `S1-CX`, heads-up: a hand both seats have signed is not given up on the
    /// ordinary stage budget -- the carrier repairs a brief outage by itself
    /// within `CARRIER_GIVES_UP_MS` -- but on `HEADS_UP_STAGE_BUDGET_MS`; a
    /// hand the other seat never signed keeps the ordinary budget.
    #[test]
    fn heads_up_a_signed_hand_keeps_the_longer_stage_budget() {
        use crate::protocol::constants::HEADS_UP_STAGE_BUDGET_MS;
        let keys = [key(10), key(11)];
        let (mut a, from_a) = Hand::open(opening(0), &keys[0], NOW, 30_000).unwrap();
        let (mut b, from_b) = Hand::open(opening(1), &keys[1], NOW, 30_000).unwrap();
        // The other seat never signs: the ordinary budget ends this hand.
        assert!(a.may_abandon(NOW + 31_000), "stage 0, the other seat never signed: the ordinary budget");
        // Both signed: the longer one.
        deliver(&mut b, &from_a, &keys[1]);
        deliver(&mut a, &from_b, &keys[0]);
        assert!(!a.may_abandon(NOW + 31_000), "signed by both: not on the ordinary budget");
        assert!(!b.may_abandon(NOW + 31_000));
        assert!(!a.may_abandon(NOW + u64::from(HEADS_UP_STAGE_BUDGET_MS) - 1_000));
        assert!(a.may_abandon(NOW + u64::from(HEADS_UP_STAGE_BUDGET_MS)), "and on the heads-up budget it is");
        assert!(b.may_abandon(NOW + u64::from(HEADS_UP_STAGE_BUDGET_MS)));
    }

    /// `D-032`: a return is counted in every seat's next hand, and at
    /// `MAX_RETURNS` this client votes for no further return of that seat --
    /// a certificate needs every voter, so one refusal keeps the seat out.
    #[test]
    fn three_returns_are_the_limit() {
        use crate::protocol::constants::MAX_RETURNS;
        let (mut hands, keys) = a_settled_hand_with_a_bystander();
        let ev = evidence_of(&mut hands[2], &keys[2], 2);
        let table_id = hands[0].table_id();
        let va = hands[0].vote_on_returns(std::slice::from_ref(&ev), &keys[0], NOW).unwrap();
        let vb = hands[1].vote_on_returns(std::slice::from_ref(&ev), &keys[1], NOW).unwrap();
        let from_a = hands[1].on_event(&bytes_of(&va)[0], &keys[1], NOW).unwrap();
        let from_b = hands[0].on_event(&bytes_of(&vb)[0], &keys[0], NOW).unwrap();
        let cert_a = certs_in(&from_b, &table_id, 1);
        let cert_b = certs_in(&from_a, &table_id, 1);
        assert!(hands[0].on_event(&cert_b[0], &keys[0], NOW).is_ok());
        assert!(hands[1].on_event(&cert_a[0], &keys[1], NOW).is_ok());
        assert!(hands[2].on_event(&cert_a[0], &keys[2], NOW).is_ok());
        for (i, h) in hands.iter().enumerate() {
            let next = h.next_hand().expect("a successor");
            assert_eq!(next.returns[2], 1, "seat {i}: one return counted");
            assert_eq!(next.returns[0], 0, "seat {i}: and nobody else's");
        }

        // Three returns already: this client votes for no more, and says so once.
        let (mut hands, keys) = a_settled_hand_with_a_bystander();
        hands[0].open.returns[2] = MAX_RETURNS;
        let ev = evidence_of(&mut hands[2], &keys[2], 2);
        assert!(
            hands[0].vote_on_returns(std::slice::from_ref(&ev), &keys[0], NOW).unwrap().is_empty(),
            "no vote at the limit"
        );
        assert!(hands[0].take_cert_note().is_some_and(|n| n.contains("D-032")), "and it says so");
        assert!(hands[0].vote_on_returns(std::slice::from_ref(&ev), &keys[0], NOW).unwrap().is_empty());
        assert!(hands[0].take_cert_note().is_none(), "once");
        // The other voter, whose count is short, votes -- and one vote is no certificate.
        let vb = hands[1].vote_on_returns(std::slice::from_ref(&ev), &keys[1], NOW).unwrap();
        assert_eq!(vb.len(), 1);
        let from_b = hands[0].on_event(&bytes_of(&vb)[0], &keys[0], NOW).unwrap();
        assert!(certs_in(&from_b, &table_id, 1).is_empty(), "one vote seals nothing");
        assert!(hands[0].returned().is_empty(), "the seat stays out");
        assert!(hands[1].returned().is_empty());
    }

    /// `D-047`: the fourth absence is the last. A seat certified absent with
    /// `MAX_RETURNS` returns behind it is out of the table: its chips leave
    /// the table at the boundary, so it is dealt in no more and cannot return.
    #[test]
    fn the_fourth_absence_takes_the_seat_out() {
        use crate::protocol::constants::MAX_RETURNS;
        let (mut hands, _keys) = a_settled_hand_with_a_bystander();
        for h in hands.iter_mut() {
            h.open.returns[2] = MAX_RETURNS;
            h.certified.push(2);
        }
        for (i, h) in hands.iter().enumerate() {
            assert_eq!(h.out_for_good(), vec![2], "seat {i}: the fourth absence puts seat 2 out");
            let next = h.next_hand().expect("a successor: two seats with chips remain");
            assert_eq!(next.out, vec![2], "seat {i}: carried");
            assert_eq!(next.seats.iter().find(|s| s.0 == 2).map(|s| s.2), Some(0), "seat {i}: its chips left the table");
            assert!(!next.required.contains(&2), "seat {i}: and it is required no more");
        }
        // Short of the limit, a certified seat keeps its chips: D-032's dead seat.
        let (mut hands, _keys) = a_settled_hand_with_a_bystander();
        hands[0].open.returns[2] = MAX_RETURNS - 1;
        hands[0].certified.push(2);
        assert!(hands[0].out_for_good().is_empty());
        // What it holds at the boundary: the bystander posted its blind as dead money.
        let kept = hands[0].stack_at_boundary(2);
        assert!(kept > 0);
        let next = hands[0].next_hand().expect("a successor");
        assert_eq!(next.seats.iter().find(|s| s.0 == 2).map(|s| s.2), Some(kept));
    }

    /// `D-033`: a table of two, dealt to the first bet, seen from a third
    /// hand -- the second seat's next life -- that replays the table's frames
    /// with the kept secret. Every frame either side said, in the order it
    /// was said.
    fn heads_up_to_the_first_bet() -> ([Hand; 2], [SigningKey; 2], Vec<Vec<u8>>, Vec<Vec<u8>>, [u8; 32]) {
        let keys = [key(10), key(11)];
        let (a, from_a) = Hand::open(heads_up_opening(0), &keys[0], NOW, 30_000).unwrap();
        let (b, from_b) = Hand::open(heads_up_opening(1), &keys[1], NOW, 30_000).unwrap();
        let mut hands = [a, b];
        let mut transcript: Vec<Vec<u8>> = Vec::new();
        let mut a_said: Vec<Vec<u8>> = Vec::new();
        let mut queue: Vec<(usize, Vec<Send>)> = vec![(1, from_b), (0, from_a)];
        while !queue.is_empty() {
            let (from, sends) = queue.remove(0);
            if sends.is_empty() {
                continue;
            }
            transcript.extend(bytes_of(&sends));
            if from == 0 {
                a_said.extend(bytes_of(&sends));
            }
            let to = 1 - from;
            let more = deliver(&mut hands[to], &sends, &keys[to]);
            queue.push((to, more));
        }
        assert!(hands[0].street().is_some() && hands[1].street().is_some(), "dealt");
        // Every frame said was accepted by exactly the other seat, and kept there.
        assert_eq!(
            hands[0].transcript().len() + hands[1].transcript().len(),
            transcript.len(),
            "each side keeps what it accepted from the other"
        );
        assert!(hands[0].turn().is_some(), "somebody is to act");
        let kept = hands[1].secret().expect("a member holds its secret").keep();
        (hands, keys, transcript, a_said, kept)
    }

    /// Carry what one seat said to the other, and the replies back, until
    /// nothing more is said -- in the order said.
    fn pump(hands: &mut [Hand; 2], keys: &[SigningKey; 2], from: usize, sends: Vec<Send>) {
        let mut queue: Vec<(usize, Vec<Send>)> = vec![(from, sends)];
        while !queue.is_empty() {
            let (from, sends) = queue.remove(0);
            if sends.is_empty() {
                continue;
            }
            let to = 1 - from;
            let more = deliver(&mut hands[to], &sends, &keys[to]);
            queue.push((to, more));
        }
    }

    /// What the survivor says again to a seat back from a restart: the frames
    /// it accepted (its transcript) and its own -- replayed into the restored
    /// hand, which holds what is early.
    fn replay_from_the_table(into: &mut Hand, survivor: &Hand, survivor_said: &[Vec<u8>], key: &SigningKey) {
        let mut frames: Vec<Vec<u8>> = survivor.transcript().to_vec();
        frames.extend(survivor_said.iter().cloned());
        replay(into, &frames, key);
    }

    /// Replay the table's frames into a restored hand, holding what is early.
    fn replay(into: &mut Hand, transcript: &[Vec<u8>], key: &SigningKey) {
        for bytes in transcript {
            match into.on_event(bytes, key, NOW) {
                Ok(_) => {}
                Err(Failed::NotYet) => {
                    let _ = into.hold(bytes.clone());
                }
                Err(e) => panic!("replaying a frame the table accepted: {e}"),
            }
        }
        let (_, failures) = into.replay_early(key, NOW);
        assert!(failures.is_empty(), "{failures:?}");
    }

    /// `D-033`: the second seat's client restarts inside a dealt hand. Its next
    /// life adopts the hand from both openings, takes its own frames from the
    /// wire and its secret from the record, reads the same two cards, and plays
    /// the hand out to one settlement on both sides.
    #[test]
    fn a_member_comes_back_into_a_dealt_hand_with_its_secret_and_plays_it_out() {
        let ([a, b], keys, transcript, a_said, kept) = heads_up_to_the_first_bet();
        let copies: Vec<Vec<u8>> = transcript[..2].to_vec();
        let o = Opening::adopt(heads_up_opening(1), &copies).expect("both openings");
        assert_eq!(o.required, vec![0, 1]);
        let mut b2 = Hand::open_restoring(o, &keys[1], NOW + 60_000, 30_000, Some(&kept)).unwrap();
        assert!(b2.is_restoring());
        replay_from_the_table(&mut b2, &a, &a_said, &keys[1]);
        let late = b2.restore_done(&keys[1], NOW + 60_000).unwrap();
        assert!(late.is_empty(), "nothing was owed at the first bet: {}", late.len());
        assert!(!b2.is_restoring() && b2.can_play_on());
        assert_eq!(b2.street(), b.street(), "the same street");
        assert_eq!(b2.turn().map(|t| t.seat), b.turn().map(|t| t.seat), "the same seat to act");
        assert_eq!(b2.cards(), b.cards(), "the same two cards, read with the kept secret");
        assert!(b2.cards().is_some());
        drop(b);
        // Played out: whoever is to act checks or calls, until the hand is over.
        let mut hands = [a, b2];
        for _ in 0..40 {
            if hands[0].over() && hands[1].over() {
                break;
            }
            let Some(turn) = hands[0].turn().or_else(|| hands[1].turn()) else {
                break;
            };
            let actor = usize::from(turn.seat);
            let action = if turn.legal.can_check { Action::Check } else { Action::Call };
            let sends = hands[actor].act(action, &keys[actor], NOW + 60_000).unwrap();
            let mut queue: Vec<(usize, Vec<Send>)> = vec![(actor, sends)];
            while !queue.is_empty() {
                let (from, sends) = queue.remove(0);
                if sends.is_empty() {
                    continue;
                }
                let to = 1 - from;
                let more = deliver(&mut hands[to], &sends, &keys[to]);
                queue.push((to, more));
            }
        }
        let [a, b2] = hands;
        assert!(a.over() && b2.over(), "played out on both sides");
        assert_eq!(a.terminal(), b2.terminal(), "one settlement");
        assert_eq!(a.stacks(), b2.stacks());
    }

    /// `D-033`: the same restart with no secret kept -- or one of another hand.
    /// The next life follows the hand and can fold at its turn; it reads no
    /// cards and makes no share.
    #[test]
    fn a_member_back_without_its_secret_can_only_fold() {
        let ([mut a, b], keys, transcript, a_said, kept) = heads_up_to_the_first_bet();
        let copies: Vec<Vec<u8>> = transcript[..2].to_vec();
        // Another hand's secret is as good as none.
        let mut wrong = kept;
        wrong[0] ^= 1;
        let wrong = HandSecret::kept(&wrong).map(|s| s.keep());
        for kept in [None, wrong.as_ref()] {
            let o = Opening::adopt(heads_up_opening(1), &copies).unwrap();
            let mut b2 = Hand::open_restoring(o, &keys[1], NOW + 60_000, 30_000, kept).unwrap();
            replay_from_the_table(&mut b2, &a, &a_said, &keys[1]);
            let _ = b2.restore_done(&keys[1], NOW + 60_000).unwrap();
            assert!(!b2.can_play_on(), "no secret, no play");
            assert_eq!(b2.street(), b.street());
            assert!(b2.cards().is_none(), "no cards to read");
            if b2.turn().is_some_and(|t| t.mine) {
                let sends = b2.act(Action::Fold, &keys[1], NOW + 60_000).expect("a fold needs no secret");
                let mut hands = [a, b2];
                pump(&mut hands, &keys, 1, sends);
                assert!(hands[0].over() && hands[1].over(), "the fold ends a hand of two");
                assert_eq!(hands[0].terminal(), hands[1].terminal());
                return;
            }
        }
        // The first seat is to act: it acts, then the second folds.
        let turn = a.turn().expect("the first seat is to act");
        let action = if turn.legal.can_check { Action::Check } else { Action::Call };
        let sends = a.act(action, &keys[0], NOW + 60_000).unwrap();
        let o = Opening::adopt(heads_up_opening(1), &copies).unwrap();
        let mut b2 = Hand::open_restoring(o, &keys[1], NOW + 60_000, 30_000, None).unwrap();
        replay_from_the_table(&mut b2, &a, &a_said, &keys[1]);
        let _ = b2.restore_done(&keys[1], NOW + 60_000).unwrap();
        deliver(&mut b2, &sends, &keys[1]);
        assert!(b2.turn().is_some_and(|t| t.mine), "now the second seat is to act");
        let sends = b2.act(Action::Fold, &keys[1], NOW + 60_000).expect("a fold needs no secret");
        let mut hands = [a, b2];
        pump(&mut hands, &keys, 1, sends);
        assert!(hands[0].over() && hands[1].over(), "the fold ends a hand of two");
        assert_eq!(hands[0].terminal(), hands[1].terminal());
    }

    /// `pump`, with every frame heard at `at` on the hearer's clock.
    fn pump_at(hands: &mut [Hand; 2], keys: &[SigningKey; 2], from: usize, sends: Vec<Send>, at: u64) {
        let mut queue: Vec<(usize, Vec<Send>)> = vec![(from, sends)];
        while !queue.is_empty() {
            let (from, sends) = queue.remove(0);
            let to = 1 - from;
            let mut more = Vec::new();
            for Send::Broadcast(bytes) in &sends {
                more.append(&mut hands[to].on_event(bytes, &keys[to], at).expect("a frame of the hand"));
            }
            if !more.is_empty() {
                queue.push((to, more));
            }
        }
    }

    /// `S1-FS`: a turn says when it was given, on the giver's clock (`D-034`),
    /// and when this client could first show it: when it heard of it, or --
    /// for a turn that already stood when a restarted client took the hand up
    /// again -- the take-up, which the turn says it stood at. The owner's game:
    /// heads-up, a client killed at its own turn came back a minute later to
    /// the turn still standing, and its own clock, counted from the stamp, left
    /// its player half a second.
    #[test]
    fn a_turn_says_when_it_was_given_and_when_this_client_could_show_it() {
        // Heard late, never restored: shown from when it was heard.
        let (mut hands, keys, _, _, _) = heads_up_to_the_first_bet();
        let up = usize::from(hands[0].turn().expect("somebody is to act").seat);
        let turn = hands[up].turn().unwrap();
        let action = if turn.legal.can_check { Action::Check } else { Action::Call };
        let sends = hands[up].act(action, &keys[up], NOW + 1_000).unwrap();
        pump_at(&mut hands, &keys, up, sends, NOW + 45_000);
        let other = hands[1 - up].turn().expect("the first to act preflop gives the big blind its option");
        assert!(other.mine);
        assert_eq!(other.began_unix_ms, NOW + 1_000, "given when the action was made");
        assert_eq!(other.shown_unix_ms, NOW + 45_000, "shown when it was heard");
        assert!(!other.taken_up, "a hand never restored takes nothing up");

        // Standing at a take-up: shown from the take-up, and it says so.
        let ([mut a, _b], keys, transcript, mut a_said, kept) = heads_up_to_the_first_bet();
        if a.turn().is_some_and(|t| t.mine) {
            let turn = a.turn().unwrap();
            let action = if turn.legal.can_check { Action::Check } else { Action::Call };
            a_said.extend(bytes_of(&a.act(action, &keys[0], NOW).unwrap()));
        }
        let back = NOW + 64_000;
        let o = Opening::adopt(heads_up_opening(1), &transcript[..2]).unwrap();
        let mut b2 = Hand::open_restoring(o, &keys[1], back, 30_000, Some(&kept)).unwrap();
        replay_from_the_table(&mut b2, &a, &a_said, &keys[1]);
        let _ = b2.restore_done(&keys[1], back).unwrap();
        let standing = b2.turn().expect("the turn stands on the seat that came back");
        assert!(standing.mine && standing.taken_up, "it stood when the hand was taken up");
        assert_eq!(standing.began_unix_ms, NOW, "given before the restart");
        assert_eq!(standing.shown_unix_ms, back, "and shown from the take-up");

        // Played on: the seat's next turn is heard after the take-up and did not stand at it.
        let mut hands = [a, b2];
        let mut at = back;
        let mut next = None;
        for _ in 0..40 {
            let Some(turn) = hands[1].turn() else {
                break;
            };
            let actor = usize::from(turn.seat);
            if actor == 1 && at > back {
                next = Some(turn);
                break;
            }
            at += 1_000;
            let turn = hands[actor].turn().unwrap();
            let action = if turn.legal.can_check { Action::Check } else { Action::Call };
            let sends = hands[actor].act(action, &keys[actor], at).unwrap();
            pump_at(&mut hands, &keys, actor, sends, at);
        }
        let next = next.expect("the seat that came back is to act again");
        assert!(!next.taken_up, "a turn given after the take-up did not stand at it");
        assert_eq!(next.shown_unix_ms, at, "shown when it was heard");
    }
}
