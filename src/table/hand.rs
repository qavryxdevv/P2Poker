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

use crate::mental_poker::backend::{DeckParams, HandDeck, HandSecret, VerifiedKey, WireKey,
    WireKeyProof};
use crate::mental_poker::protocol::{CtxFields, DeckCtx, DeckWire, ProofPosition};

use super::handwire::{DeckInit, HandInit, NotOurs};
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
const FRAME_CAP: usize = 4_096;

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
    /// Stage 1 completed: every seat's key is verified and the deck exists.
    Dealt(HandDeck),
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
        let (sb_position, bb_seat) = blind_positions(button, &occupied);

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
        if !matches!(kind, EventType::HandInit | EventType::DeckInit) {
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
            Phase::Deck { .. } => self.on_deck_init(bytes),
            Phase::Dealt(_) => Err(Failed::NothingFurther),
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

        self.phase = Phase::Deck {
            stage,
            keys: vec![own],
            secret,
        };
        Ok(vec![Send::Broadcast(bytes)])
    }

    fn on_deck_init(&mut self, bytes: &[u8]) -> Result<Vec<Send>, Failed> {
        let opened = self.opened(bytes, EventType::DeckInit)?;
        let seat = self.seat_of(&opened.sender)?;
        let body: DeckInit = chained::payload(&opened, DECK_INIT_CAP).map_err(Failed::Wire)?;

        let wire_key = WireKey::decode(&body.key).map_err(|_| Failed::BadKey {
            seat,
            why: "not a point on the curve",
        })?;
        let proof = WireKeyProof::decode(&body.proof).map_err(|_| Failed::BadKey {
            seat,
            why: "not a well-formed ownership proof",
        })?;
        let ctx = self.deck_ctx(&opened.sender);

        let Phase::Deck { stage, keys, .. } = &mut self.phase else {
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
            Heard::Counted | Heard::Bystander => keys.push(verified),
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
        let Phase::Deck { keys, .. } = &self.phase else {
            unreachable!("just matched")
        };
        self.phase = Phase::Dealt(HandDeck::new(self.params.clone(), keys));
        Ok(Vec::new())
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
    pub fn replay_early(
        &mut self,
        key: &SigningKey,
        now_ms: u64,
    ) -> (Vec<Send>, Vec<Failed>) {
        let waiting: Vec<Vec<u8>> = self.early.drain(..).collect();
        let mut sends = Vec::new();
        let mut failures = Vec::new();
        for bytes in waiting {
            match self.on_event(&bytes, key, now_ms) {
                Ok(mut out) => sends.append(&mut out),
                Err(e) => failures.push(e),
            }
        }
        (sends, failures)
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
        matches!(self.phase, Phase::Dealt(_))
    }

    /// Which seats the stage now open is still waiting for.
    pub fn waiting_for(&self) -> Vec<SeatIdx> {
        match &self.phase {
            Phase::Init(stage) => stage.waiting_for(),
            Phase::Deck { stage, .. } => stage.waiting_for(),
            Phase::Dealt(_) => Vec::new(),
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
            _ => None,
        }
    }

    /// The deck, once every seat's key is verified.
    pub fn deck(&self) -> Option<&HandDeck> {
        match &self.phase {
            Phase::Dealt(deck) => Some(deck),
            _ => None,
        }
    }

    /// What this client derived, for the interface to draw.
    pub fn init(&self) -> &HandInit {
        &self.mine
    }

    pub fn hand_id(&self) -> u64 {
        self.open.hand_id
    }
}

/// Where the blinds sit relative to the button.
///
/// Heads-up is the exception every poker implementation gets wrong once: with
/// two players the **button is the small blind**, and the other seat is the big
/// blind. With three or more the small blind is the next occupied seat after the
/// button and the big blind the one after that.
fn blind_positions(button: SeatIdx, occupied: &[SeatIdx]) -> (SeatIdx, SeatIdx) {
    let at = occupied.iter().position(|s| *s == button).unwrap_or(0);
    let next = |from: usize, by: usize| occupied[(from + by) % occupied.len()];
    if occupied.len() == 2 {
        (button, next(at, 1))
    } else {
        (next(at, 1), next(at, 2))
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

    /// Deliver one client's output to the other, and hand back whatever that
    /// produced. A stage completing is what emits the next stage's message, so
    /// a test that dropped the return value would stall one step in.
    fn deliver(to: &mut Hand, sends: &[Send], key: &SigningKey) -> Vec<Send> {
        let mut out = Vec::new();
        for Send::Broadcast(bytes) in sends {
            match to.on_event(bytes, key, NOW) {
                Ok(mut more) => out.append(&mut more),
                Err(e) => panic!("{e}"),
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

        // And stage 1: each verifies the other's key and the deck exists.
        assert!(deliver(&mut b, &a_deck, &key(11)).is_empty());
        assert!(deliver(&mut a, &b_deck, &key(10)).is_empty());
        assert!(a.deck_ready() && b.deck_ready(), "every key verified");
        assert_eq!(a.slot(), b.slot(), "and one parent for stage 2");
        assert_eq!(a.slot().sequence, 2);
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
        assert_eq!(blind_positions(0, &[0, 1]), (0, 1));
        assert_eq!(blind_positions(1, &[0, 1]), (1, 0));
    }

    /// Three-handed and more, the blinds are the next two occupied seats.
    #[test]
    fn at_three_the_blinds_follow_the_button() {
        assert_eq!(blind_positions(0, &[0, 1, 2]), (1, 2));
        assert_eq!(blind_positions(2, &[0, 1, 2]), (0, 1));
        assert_eq!(blind_positions(3, &[0, 3, 7]), (7, 0));
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
