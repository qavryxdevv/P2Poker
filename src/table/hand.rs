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

use std::collections::VecDeque;

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
use crate::poker::engine::{initial_positions, Positions};
use crate::poker::state::Card;
use crate::protocol::serialization::h;
use crate::protocol::signatures::Domain;
use crate::protocol::transcript::stage_hash_single;

use super::dealing::{self, Dealing, Identity, Refused, Share};
use super::handwire::{DealPrivate, DeckCommit, DeckInit, HandInit, NotOurs, RevealEntry,
    ShuffleProof, ShuffleStep};
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
    /// Seat, public key and starting stack, ascending by seat.
    pub seats: Vec<(SeatIdx, [u8; 32], u64)>,
    pub max_players: u8,
    pub small_blind: u64,
    pub big_blind: u64,
    pub level: u16,
    pub my_seat: SeatIdx,
    /// How long a peer has to answer a cryptographic step, from the table's own
    /// parameters. Carried on the envelope of every stage this hand emits.
    pub crypto_step_timeout_ms: u32,
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
            // hand has completed.
            level: 1,
            my_seat: f.my_seat()?,
            crypto_step_timeout_ms: ad.crypto_step_timeout_ms,
        })
    }
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
const FRAME_CAP: usize = 16_384;

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
    /// This client holds its two cards.
    Holding {
        deal: Deal,
        table: Table,
        dealing: Box<Dealing>,
        cards: [Card; 2],
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
}

/// One hand in progress.
pub struct Hand {
    open: Opening,
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
        let occupied: Vec<SeatIdx> = o.seats.iter().map(|(s, _, _)| *s).collect();
        let button = provisional_button(&o.session_id, &occupied);
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
            // Everybody on the roster is a party to the first hand. A seat that
            // has busted or sat out is not, and neither exists yet.
            dealt_in: occupied.clone(),
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

        let mut stage = Collective::closed(0, EventType::HandInit.code(), &o.required)
            .ok_or(Failed::NotInThisStage)?;
        let own_hash = chained::open(&bytes, FRAME_CAP, EventType::HandInit, &slot)
            .map_err(Failed::Wire)?
            .event_hash;
        stage.hear(o.my_seat, own_hash);

        Ok((
            Hand {
                open: o,
                mine,
                slot,
                phase: Phase::Init(stage),
                params: DeckParams::new(),
                early: VecDeque::new(),
            },
            vec![Send::Broadcast(bytes)],
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
        let (kind, hand_id, sequence) = chained::peek(bytes, FRAME_CAP).map_err(Failed::Wire)?;
        if !matches!(
            kind,
            EventType::HandInit
                | EventType::DeckInit
                | EventType::ShuffleStep
                | EventType::ShuffleProof
                | EventType::DeckCommit
                | EventType::DealPrivate
        ) {
            return Err(Failed::Wire(WireError::WrongType));
        }
        if hand_id != self.open.hand_id {
            return Err(Failed::NotYet);
        }
        if sequence < self.slot.sequence {
            // A stage this hand has left. Not a fault and not worth a word: the
            // mesh delivers a message more than once as a matter of course.
            return Ok(Vec::new());
        }
        if sequence > self.slot.sequence {
            return Err(Failed::NotYet);
        }

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
            Phase::Dealing { .. } => self.on_deal_private(bytes),
            Phase::Holding { .. } | Phase::Between => Err(Failed::NothingFurther),
        }
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
            Heard::Uninvited => return Err(Failed::NotInThisStage),
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

        let me = self.open.seats[self.seat_index()].1;
        let ctx = self.deck_ctx(&me);
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
        stage.hear(self.open.my_seat, own_hash);

        let mut by_seat = vec![None; usize::from(self.open.max_players)];
        if let Some(slot) = by_seat.get_mut(usize::from(self.open.my_seat)) {
            *slot = Some(own);
        }
        self.phase = Phase::Deck {
            stage,
            by_seat,
            keys: vec![own],
            secret,
        };
        Ok(vec![Send::Broadcast(bytes)])
    }

    fn on_deck_init(
        &mut self,
        bytes: &[u8],
        key: &SigningKey,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, EventType::DeckInit)?;
        let seat = self.seat_of(&opened.sender)?;
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
            Heard::Uninvited => return Err(Failed::NotInThisStage),
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
        self.slot = self.slot.then(stage_hash_single(
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
            &self.slot,
            &body,
            key,
            now_ms,
            self.open.crypto_step_timeout_ms,
            SHUFFLE_PROOF_CAP,
        )
        .map_err(Failed::Wire)?;
        let proof_hash = self.opened(&proof_event, EventType::ShuffleProof)?.event_hash;

        // This client's own step goes through `accept_step` like anybody
        // else's, which costs it the 42 ms of verifying its own argument. That
        // is the price of having one path into the chain: a second, trusting
        // path would be a path an attacker only has to find once.
        let me = self.open.my_seat;
        let Phase::Shuffling { deal, chain, .. } = &mut self.phase else {
            unreachable!("just matched")
        };
        chain
            .accept_step(&deal.deck, me, next, &body.proof, self.slot.sequence)
            .map_err(|e| step_failure(me, e))?;
        self.slot = self.slot.then(stage_hash_single(
            self.slot.sequence,
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
        chain
            .accept_step(&deal.deck, seat, deck, &body.proof, self.slot.sequence)
            .map_err(|e| step_failure(seat, e))?;
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
            Heard::Uninvited => return Err(Failed::NotInThisStage),
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

    fn on_deal_private(&mut self, bytes: &[u8]) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, EventType::DealPrivate)?;
        let seat = self.seat_of(&opened.sender)?;
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
            Heard::Uninvited => return Err(Failed::NotInThisStage),
        }
        if !stage.complete() {
            return Ok(Vec::new());
        }
        let parent = stage.hash().expect("a complete stage has one");
        self.slot = self.slot.then(parent);
        self.read_my_cards()
    }

    /// Every share is in: open this client's two cards.
    fn read_my_cards(&mut self) -> Result<Vec<Send>, Failed> {
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
        let indices = table.map.hole_cards(me).ok_or(Failed::NotInThisStage)?;
        let mut cards = Vec::with_capacity(2);
        for index in indices {
            let card = dealing
                .open(&deal.as_ref(&table), index)
                .map_err(|_| Failed::BadToken {
                    seat: me,
                    why: "the stage completed without every share for a card",
                })?;
            cards.push(card);
        }
        let cards: [Card; 2] = cards.try_into().map_err(|_| Failed::NotInThisStage)?;
        self.phase = Phase::Holding {
            deal,
            table,
            dealing,
            cards,
        };
        Ok(Vec::new())
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
            self.open.crypto_step_timeout_ms,
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
        DeckCtx::build(&CtxFields {
            protocol_version: crate::protocol::messages::PROTOCOL_VERSION,
            table_id: self.open.table_id,
            session_id: self.open.session_id,
            hand_id: self.open.hand_id,
            sequence: self.slot.sequence,
            position: ProofPosition::NotAShuffleStep,
            sender_public_key: *sender,
        })
    }

    fn seat_index(&self) -> usize {
        self.open
            .seats
            .iter()
            .position(|(s, _, _)| *s == self.open.my_seat)
            .unwrap_or(0)
    }

    /// Hold an event that belongs to a stage this client has not reached.
    pub fn hold(&mut self, bytes: Vec<u8>) {
        // Bounded: this is fed from the network, and everything fed from the
        // network is bounded where it is consumed.
        if self.early.len() >= 64 {
            self.early.pop_front();
        }
        self.early.push_back(bytes);
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
                    Err(Failed::NotYet) => self.hold(bytes),
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
            Phase::Committing { .. } | Phase::Dealing { .. } | Phase::Holding { .. }
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
            Phase::Holding { cards, .. } => Some(*cards),
            _ => None,
        }
    }

    /// Whether this client is holding its hole cards.
    pub fn dealt_cards(&self) -> bool {
        matches!(self.phase, Phase::Holding { .. })
    }

    /// The board, as far as it has been opened.
    ///
    /// Empty before the flop, and it stays empty until a `BOARD_REVEAL` stage
    /// completes: a board card comes out of a complete set of verified shares
    /// or it does not exist. Nobody is ever shown one this client did not open
    /// itself.
    pub fn board(&self) -> Vec<Card> {
        match &self.phase {
            Phase::Holding { dealing, .. } => dealing.board(),
            _ => Vec::new(),
        }
    }

    /// Which seats are still owed for the stage now open, by seat.
    ///
    /// For a reveal stage this is the seats whose shares have not arrived, and
    /// it is what a table window puts next to "waiting for".
    pub fn outstanding_shares(&self, index: crate::mental_poker::deck::CardIndex)
        -> Option<Vec<SeatIdx>>
    {
        match &self.phase {
            Phase::Dealing { dealing, .. } | Phase::Holding { dealing, .. } => {
                dealing.outstanding(index)
            }
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
    pub fn waiting_for(&self) -> Vec<SeatIdx> {
        match &self.phase {
            Phase::Init(stage) => stage.waiting_for(),
            Phase::Deck { stage, .. } => stage.waiting_for(),
            // One seat at a time: a single-writer stage is waiting for exactly
            // the shuffler whose turn it is, and naming the others would put
            // seats on screen that owe the table nothing.
            Phase::Shuffling { chain, .. } => chain.whose_turn().into_iter().collect(),
            Phase::Committing { stage, .. } | Phase::Dealing { stage, .. } => stage.waiting_for(),
            Phase::Holding { .. } | Phase::Between => Vec::new(),
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
            | Phase::Holding { deal, .. } => Some(&deal.secret),
            Phase::Init(_) | Phase::Between => None,
        }
    }

    /// The deck, once every seat's key has been verified.
    pub fn deck(&self) -> Option<&HandDeck> {
        match &self.phase {
            Phase::Shuffling { deal, .. }
            | Phase::Committing { deal, .. }
            | Phase::Dealing { deal, .. }
            | Phase::Holding { deal, .. } => Some(&deal.deck),
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
            | Phase::Holding { deal, .. } => &deal.keys,
            Phase::Init(_) | Phase::Between => &[],
        }
    }

    /// The final deck, once the chain has closed.
    pub fn final_deck(&self) -> Option<&Final<Verified<Vec<Ciphertext>>>> {
        match &self.phase {
            Phase::Committing { table, .. }
            | Phase::Dealing { table, .. }
            | Phase::Holding { table, .. } => Some(&table.deck),
            _ => None,
        }
    }

    /// The deck-index map, once it has been fixed.
    pub fn index_map(&self) -> Option<&DeckIndexMap> {
        match &self.phase {
            Phase::Committing { table, .. }
            | Phase::Dealing { table, .. }
            | Phase::Holding { table, .. } => Some(&table.map),
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
        Refused::DidNotVerify(_) => Failed::BadToken {
            seat,
            why: "the proof does not hold against the committed deck",
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
        }
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

    /// An argument that does not hold is a different failure from a mismatched
    /// hash: it names the shuffle, because the consequence is that the chain
    /// can never complete and the hand is abandoned rather than shortened.
    #[test]
    fn an_argument_that_does_not_hold_is_refused_as_a_shuffle() {
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
        let e = b.on_event(&broken, &key(11), NOW).unwrap_err();
        assert!(matches!(e, Failed::BadShuffle { seat: 0, .. }), "{e}");
        assert!(!b.shuffled(), "and the chain did not advance");
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
