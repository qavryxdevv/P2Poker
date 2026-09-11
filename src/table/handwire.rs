//! `HAND_INIT` on the wire, and how to check one.
//!
//! `PROTOCOL.md` §4.3's twelve fields, in its own numbering, and the sentence
//! that governs all of them:
//!
//! > **`HAND_INIT` announces nothing and decides nothing.** Every field is a
//! > pure function of `TERMINAL(k-1)` and the table parameters, so every
//! > receiver recomputes all of them and rejects a copy in which any field
//! > differs.
//!
//! That is why the stage is collective rather than single-writer: a body with no
//! choices in it must not give one seat a veto over a hand-to-hand transition
//! that `SPEC_CS.md` §4 requires to be automatic. A seat that emits a wrong copy
//! has committed an attributable violation and its copy simply does not complete
//! the stage for it — the honest seats carry on without it, and it is not heard.
//!
//! # Comparing, not trusting
//!
//! [`HandInit::disagreement`] is the whole point of this module. A receiver
//! builds its own body from its own state and compares field by field, and what
//! comes back is **which field** differed. A boolean would be useless: the
//! symptom on screen is a table that does not move, and "seat 3's copy differs"
//! is not something anybody can act on, while "seat 3 says the button is at 2
//! and I say 1" is.

use crate::poker::state::{Hash, SeatIdx, Street};
use crate::protocol::constants::ABORT_EVIDENCE_MAX;

/// The body of a `HAND_INIT`, in `PROTOCOL.md` §4.3's field order.
///
/// `#[cbor(array)]` like every other body in this project: a map would encode
/// the field names on every copy of every hand, and canonical CBOR over an
/// array is what the signature is taken across.
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct HandInit {
    /// Must equal the envelope's `hand_id`; carried anyway, because the body is
    /// what is compared and a field that lived only in the envelope could not be
    /// disagreed about in the same breath as the rest.
    #[n(0)]
    pub hand_id: u64,
    #[n(1)]
    pub button_position: SeatIdx,
    #[n(2)]
    pub sb_position: SeatIdx,
    /// Need **not** be in `dealt_in`: a seat that posts dead money still posts
    /// the big blind (D-005).
    #[n(3)]
    pub bb_seat: SeatIdx,
    #[n(4)]
    pub level: u16,
    #[n(5)]
    pub small_blind: u64,
    /// `== 2 * small_blind`.
    #[n(6)]
    pub big_blind: u64,
    /// `0` in version 1.
    #[n(7)]
    pub ante: u64,
    /// The cryptographic parties to this hand: ascending, unique, a subset of
    /// `P(k-1)`.
    #[n(8)]
    pub dealt_in: Vec<SeatIdx>,
    /// One per occupied seat, ascending by seat.
    #[n(9)]
    pub stacks: Vec<u64>,
    #[cbor(n(10), with = "minicbor::bytes")]
    pub roster_hash: Hash,
    /// The per-seat ledger change at this hand boundary — positive for a buy-in,
    /// negative for a departing stack. Ascending by seat, unique.
    #[n(11)]
    pub ledger_delta: Vec<(SeatIdx, i64)>,
}

/// Which field two copies disagree about.
///
/// Named rather than numbered so a log line reads as a sentence. The first
/// difference is enough: two peers that disagree about the button will disagree
/// about everything downstream of it, and listing all twelve would bury the one
/// that matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    HandId,
    ButtonPosition,
    SbPosition,
    BbSeat,
    Level,
    SmallBlind,
    BigBlind,
    Ante,
    DealtIn,
    Stacks,
    RosterHash,
    LedgerDelta,
}

impl std::fmt::Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::HandId => "hand_id",
            Self::ButtonPosition => "button_position",
            Self::SbPosition => "sb_position",
            Self::BbSeat => "bb_seat",
            Self::Level => "level",
            Self::SmallBlind => "small_blind",
            Self::BigBlind => "big_blind",
            Self::Ante => "ante",
            Self::DealtIn => "dealt_in",
            Self::Stacks => "stacks",
            Self::RosterHash => "roster_hash",
            Self::LedgerDelta => "ledger_delta",
        };
        f.write_str(name)
    }
}

/// Why a body is not one this client would have written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotOurs {
    /// A field differs from what this receiver derived. The first one found.
    Differs(Field),
    /// The body contradicts itself, whatever anybody else thinks.
    Malformed(&'static str),
}

impl std::fmt::Display for NotOurs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Differs(field) => write!(f, "{field} differs from what I derived"),
            Self::Malformed(why) => write!(f, "{why}"),
        }
    }
}

impl HandInit {
    /// What is wrong with this body on its own terms.
    ///
    /// Checked before any comparison, because a body that contradicts itself is
    /// wrong whoever else agrees with it — and because the comparison below is
    /// only meaningful between two well-formed bodies.
    pub fn self_consistent(&self, max_players: u8) -> Result<(), NotOurs> {
        for (seat, what) in [
            (self.button_position, "button_position is not a seat"),
            (self.sb_position, "sb_position is not a seat"),
            (self.bb_seat, "bb_seat is not a seat"),
        ] {
            if seat >= max_players {
                return Err(NotOurs::Malformed(what));
            }
        }
        if self.big_blind != self.small_blind.saturating_mul(2) {
            return Err(NotOurs::Malformed("big_blind is not twice small_blind"));
        }
        if self.ante != 0 {
            return Err(NotOurs::Malformed("version 1 has no ante"));
        }
        if !ascending_unique(&self.dealt_in) {
            return Err(NotOurs::Malformed("dealt_in is not ascending and unique"));
        }
        if self.dealt_in.len() > crate::protocol::constants::MAX_SEATS as usize {
            return Err(NotOurs::Malformed("dealt_in is longer than the table"));
        }
        let ledger_seats: Vec<SeatIdx> = self.ledger_delta.iter().map(|(s, _)| *s).collect();
        if !ascending_unique(&ledger_seats) {
            return Err(NotOurs::Malformed(
                "ledger_delta is not ascending and unique",
            ));
        }
        Ok(())
    }

    /// The first field in which somebody else's copy differs from ours.
    ///
    /// `Ok(())` means the two are identical, which is what completing the stage
    /// for that seat requires.
    pub fn disagreement(&self, theirs: &HandInit) -> Result<(), NotOurs> {
        macro_rules! same {
            ($field:ident, $name:ident) => {
                if self.$field != theirs.$field {
                    return Err(NotOurs::Differs(Field::$name));
                }
            };
        }
        same!(hand_id, HandId);
        same!(button_position, ButtonPosition);
        same!(sb_position, SbPosition);
        same!(bb_seat, BbSeat);
        same!(level, Level);
        same!(small_blind, SmallBlind);
        same!(big_blind, BigBlind);
        same!(ante, Ante);
        same!(dealt_in, DealtIn);
        same!(stacks, Stacks);
        same!(roster_hash, RosterHash);
        same!(ledger_delta, LedgerDelta);
        Ok(())
    }
}

fn ascending_unique(v: &[SeatIdx]) -> bool {
    v.windows(2).all(|w| w[0] < w[1])
}


// ---------------------------------------------------------------------------
// DECK_INIT
// ---------------------------------------------------------------------------

/// One seat's per-hand deck key and its proof of ownership.
///
/// Unlike `HAND_INIT`, this body is **not** derived and cannot be compared
/// against anything: a key is a fresh secret its owner chose, and the only
/// question a receiver can ask is whether the proof holds. `PROTOCOL.md` §4.4:
/// thirty-three bytes of key, sixty-five of proof.
///
/// The two fields are `Vec<u8>` and not fixed arrays on purpose. The length is
/// checked where the bytes become a curve point — `DeckWire::decode`, which
/// checks the length, parses with validation on, and then compares the
/// re-encoding — and a length check in two places is a length check that can
/// disagree with itself.
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct DeckInit {
    #[cbor(n(0), with = "minicbor::bytes")]
    pub key: Vec<u8>,
    #[cbor(n(1), with = "minicbor::bytes")]
    pub proof: Vec<u8>,
}

/// `SHUFFLE_STEP 0x0305`: the deck one shuffler produced (`PROTOCOL.md` §4.5).
///
/// The deck and the proof that justifies it travel as **two** stages, not one.
/// That is the protocol's choice and it is not arbitrary: the proof is 5547 B
/// against the deck's 3432 B, and a receiver that has the deck can start
/// nothing with it until the proof verifies anyway — so splitting them lets the
/// step be admitted, hashed and chained at its own sequence while the expensive
/// half is still arriving.
///
/// `deck` is `Vec<u8>` rather than `[[u8; 66]; 52]` for the same reason
/// [`DeckInit`]'s fields are: the length is checked where the bytes become
/// ciphertexts, once, and a second length check in the decoder is a check that
/// can disagree with the first.
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct ShuffleStep {
    #[n(0)]
    pub shuffle_round: u8,
    #[cbor(n(1), with = "minicbor::bytes")]
    pub deck: Vec<u8>,
}

/// `SHUFFLE_PROOF 0x0306`: the argument for the step at the preceding stage.
///
/// The two hashes are what bind the proof to one transition. Without them a
/// proof is a statement about *some* pair of decks, and a peer that had seen a
/// valid shuffle in any hand could replay its argument here. With them the
/// receiver checks that the input named is the deck it already holds as this
/// shuffler's input and the output named is the deck that just arrived, before
/// it spends 42 ms verifying anything.
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct ShuffleProof {
    #[n(0)]
    pub shuffle_round: u8,
    #[cbor(n(1), with = "minicbor::bytes")]
    pub input_deck_hash: [u8; 32],
    #[cbor(n(2), with = "minicbor::bytes")]
    pub output_deck_hash: [u8; 32],
    #[cbor(n(3), with = "minicbor::bytes")]
    pub proof: Vec<u8>,
}

/// `DECK_COMMIT 0x0307`: the barrier before any card exists.
///
/// Three values every dealt-in seat derived independently and must have
/// derived identically. It is deliberately cheap - two hashes and a point -
/// because its whole purpose is to catch two peers holding different decks at
/// the last moment when catching it costs nothing. After this stage a hole
/// card exists, and a disagreement discovered then is a dispute rather than a
/// restart.
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct DeckCommit {
    #[cbor(n(0), with = "minicbor::bytes")]
    pub final_deck_hash: [u8; 32],
    #[cbor(n(1), with = "minicbor::bytes")]
    pub index_map_hash: [u8; 32],
    #[cbor(n(2), with = "minicbor::bytes")]
    pub apk: Vec<u8>,
}

impl DeckCommit {
    /// Which of the three fields differs, if any.
    ///
    /// Named rather than a bare inequality for the same reason `HandInit`'s is:
    /// the three mean different things gone wrong, and a client that reports
    /// "the deck does not match" when the index map is what differs has sent
    /// its owner to look in the wrong place.
    pub fn disagreement(&self, theirs: &DeckCommit) -> Result<(), &'static str> {
        if self.final_deck_hash != theirs.final_deck_hash {
            return Err("the final deck");
        }
        if self.index_map_hash != theirs.index_map_hash {
            return Err("the deck-index map");
        }
        if self.apk != theirs.apk {
            return Err("the aggregate key");
        }
        Ok(())
    }
}

/// One peer's decryption share for one card, with the proof it was computed
/// with the key that peer committed to (`PROTOCOL.md` §4.6).
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct RevealEntry {
    #[n(0)]
    pub deck_index: u8,
    #[cbor(n(1), with = "minicbor::bytes")]
    pub token: Vec<u8>,
    #[cbor(n(2), with = "minicbor::bytes")]
    pub proof: Vec<u8>,
}

/// `DEAL_PRIVATE 0x0401`: every seat's shares for everybody else's hole cards.
///
/// The message is broadcast and the result is private, which reads like a
/// contradiction and is not: each seat withholds only its own share of its own
/// cards, so every hole card is one share short for everybody except its owner.
/// `PROTOCOL.md` §4.6 owns that argument and corrected two other documents that
/// had rested privacy on point-to-point delivery instead.
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct DealPrivate {
    #[n(0)]
    pub entries: Vec<RevealEntry>,
}

impl DealPrivate {
    /// Ascending by index and no index twice.
    ///
    /// A canonicality rule, not a policy one: two orderings of one set would be
    /// two byte strings for one message, and a collective stage compares byte
    /// strings. Checked before anything is verified, because it costs a pass
    /// over a short list and the alternative costs a DLEQ verification per
    /// duplicate a sender chose to send.
    pub fn canonical(&self) -> bool {
        self.entries
            .windows(2)
            .all(|w| w[0].deck_index < w[1].deck_index)
    }

    /// The indices, in order.
    pub fn indices(&self) -> Vec<u8> {
        self.entries.iter().map(|e| e.deck_index).collect()
    }
}

/// `BOARD_REVEAL 0x0402`: the shares that open one street's board cards.
///
/// Collective, and **every dealt-in seat owes one, folded seats included**. That
/// is the price of `n`-of-`n`: a folded player still holds a key share until the
/// hand ends, and a folded player who goes silent stalls the hand exactly as an
/// active one would. It is not a rule this layer may soften — a board card needs
/// every share that exists, and a seat that folded did not stop existing.
///
/// The cards themselves never travel. Every peer derives them from the same
/// shares against the same committed deck, which is why no receiver can ever be
/// shown a card it did not verify itself (`SPEC_CS.md` §22).
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct BoardReveal {
    /// §4.7's street code — `3` flop, `4` turn, `5` river. Never `0`: there is
    /// no board before the flop and so no `BOARD_REVEAL` for it.
    #[n(0)]
    pub street: u16,
    #[n(1)]
    pub entries: Vec<RevealEntry>,
}

/// `SHOWDOWN_REVEAL 0x0403`: the sender's own two shares, which open its hand.
///
/// The whole showdown is this one message per seat, because the other `m-1`
/// shares for those two indices have been on the transcript since
/// `DEAL_PRIVATE`. Two entries, 131 bytes each.
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct ShowdownReveal {
    /// Exactly the sender's own two hole-card indices, ascending.
    #[n(0)]
    pub entries: Vec<RevealEntry>,
}

/// `SHOWDOWN_MUCK 0x0404`: the alternative to showing, under D-021's policy.
///
/// A seat emits **exactly one** of this and [`ShowdownReveal`] at the showdown
/// stage. They are the one pair of `event_class = 0` types that share a
/// `sequence`, so a seat that emitted both would have put two bodies in one slot
/// and manufactured an equivocation proof against itself. The exclusivity is
/// what keeps that slot at capacity one, and it is safe to leave it there
/// because no rule anywhere permits a seat to both show and muck.
///
/// It is illegal under `MANDATORY_REVEAL`, and illegal in any showdown where a
/// seat is all in (TDA 16) — there every live seat must show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct ShowdownMuck {
    /// Must be `true`. Irrevocably forfeits all claim to every pot in this hand.
    ///
    /// A field that may hold one value looks like a field that should not
    /// exist, and it is the protocol's: a body with no fields at all encodes to
    /// the same bytes whatever it means, and this one has to be able to gain a
    /// second field later without the first hand of that version being an
    /// equivocation against the last hand of this one.
    #[n(0)]
    pub forfeit: bool,
}

/// One pot and where it went, inside [`HandComplete`].
///
/// Every list ascending and unique. `odd_chips` names the seats that received
/// the remainder when a pot did not divide, distributed clockwise from the
/// button — which is a rule and not a rounding convenience, because two peers
/// that split a remainder differently disagree about a stack for ever.
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct PotAward {
    #[n(0)]
    pub size: u64,
    #[n(1)]
    pub eligible: Vec<u8>,
    #[n(2)]
    pub winners: Vec<u8>,
    #[n(3)]
    pub odd_chips: Vec<u8>,
}

/// The spec's `(u8, u64)` refund pair, named.
///
/// A two-field `#[cbor(array)]` struct and a two-element tuple are the same
/// bytes; the name is here because `refunds.0` and `refunds.1` at the call site
/// are two numbers nobody can tell apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct Refund {
    #[n(0)]
    pub seat: u8,
    #[n(1)]
    pub amount: u64,
}

/// `HAND_COMPLETE 0x0801`: the hand's terminal stage.
///
/// Collective, and **every field is derived**, so there is no writer: each seat
/// computes the byte-identical body from its own engine and signs its own copy.
/// A receiver recomputes all of it — the pot layering, the eligible sets, the
/// winners and the odd-chip distribution — rather than believing any of it.
///
/// The required emitter set is the set that emitted `HAND_INIT`, and it is
/// deliberately not narrowed within the hand: a seat that signed `HAND_INIT` and
/// then went quiet blocks this stage and the hand ends at its deadline. Exactly
/// one hand pays for a seat going silent, the one it went silent in.
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct HandComplete {
    #[n(0)]
    pub pots: Vec<PotAward>,
    /// Uncalled excess, returned **before** the pots are awarded.
    #[n(1)]
    pub refunds: Vec<Refund>,
    /// One per occupied seat ascending. Must sum to zero — chips are conserved
    /// and a hand that created or destroyed one is a hand nobody may accept.
    #[n(2)]
    pub deltas: Vec<i64>,
    #[n(3)]
    pub final_stacks: Vec<u64>,
    /// Seats reaching zero, ascending.
    #[n(4)]
    pub busted: Vec<u8>,
    #[cbor(n(5), with = "minicbor::bytes")]
    pub state_hash: [u8; 32],
}

impl HandComplete {
    /// Whether the deltas conserve chips.
    ///
    /// Checked as an `i128` sum rather than an `i64` one: ten seats of `i64`
    /// can overflow, and an overflow here would turn "this hand invented money"
    /// into "this hand is fine", which is the wrong direction for the one
    /// invariant the whole settlement rests on.
    pub fn conserves_chips(&self) -> bool {
        self.deltas.iter().map(|d| i128::from(*d)).sum::<i128>() == 0
    }

    /// Which fields two copies of this settlement disagree about, and what each
    /// side holds there.
    ///
    /// # Why the report enumerates fields instead of printing two bodies
    ///
    /// A receiver compares the whole body, because every field is derived and a
    /// difference anywhere means two engines disagree about the hand. But the
    /// *report* was two `final_stacks` vectors and two pot counts, and four of
    /// the six fields are neither: a hand that disagreed only about `deltas`,
    /// `busted` or `state_hash` printed two **identical** vectors and two equal
    /// counts under the words "settlement disagreement".
    ///
    /// That is worse than saying nothing. One of the two disagreements this
    /// project has instrumented and cannot explain is a settlement mismatch seen
    /// once, and a reader looking at two identical vectors concludes the
    /// instrument is broken rather than that the difference is somewhere the
    /// instrument does not look.
    ///
    /// # The empty case is a finding, not a gap
    ///
    /// The caller reaches this after `self != theirs`, so an empty result means
    /// `PartialEq` and this function disagree — which happens exactly when a
    /// field is added to the struct and not added here. The caller says so
    /// rather than printing nothing, because a silent report is how this class
    /// of defect survives a second time.
    pub fn disagreement(&self, theirs: &Self) -> Vec<String> {
        fn short(h: &[u8; 32]) -> String {
            h[..4].iter().map(|b| format!("{b:02x}")).collect()
        }
        // **Destructured, so a seventh field is a compile error here.** The
        // bindings are unused — every comparison below reads `self` and `theirs`
        // directly — and that is the point: this pattern must name every field
        // of the struct, so adding one without extending the report cannot
        // build. A test could only have checked the fields somebody remembered
        // to list, which is the same memory that would have failed.
        let Self {
            pots: _,
            refunds: _,
            deltas: _,
            final_stacks: _,
            busted: _,
            state_hash: _,
        } = self;

        let mut out = Vec::new();
        if self.pots != theirs.pots {
            out.push(format!("pots {:?} against {:?}", self.pots, theirs.pots));
        }
        if self.refunds != theirs.refunds {
            out.push(format!(
                "refunds {:?} against {:?}",
                self.refunds, theirs.refunds
            ));
        }
        if self.deltas != theirs.deltas {
            out.push(format!("deltas {:?} against {:?}", self.deltas, theirs.deltas));
        }
        if self.final_stacks != theirs.final_stacks {
            out.push(format!(
                "final stacks {:?} against {:?}",
                self.final_stacks, theirs.final_stacks
            ));
        }
        if self.busted != theirs.busted {
            out.push(format!("busted {:?} against {:?}", self.busted, theirs.busted));
        }
        if self.state_hash != theirs.state_hash {
            // The one field that is a hash of everything else, so it can differ
            // while all five above agree: the checkpoint-8 state carries the
            // board, the button, the transcript head and `signed_this_hand`,
            // none of which the settlement repeats. Alone in the list, it means
            // the two engines agree about the money and disagree about the hand.
            out.push(format!(
                "checkpoint-8 state hash {} against {}",
                short(&self.state_hash),
                short(&theirs.state_hash)
            ));
        }
        out
    }
}

/// `HAND_ABORT 0x0802`: the other way a hand can end.
///
/// A **witness-independent terminal** (`PROTOCOL.md` §3.2): there is no required
/// emitter set, any seat of this hand may emit its copy, and the stage closes at
/// a receiver on the first copy that verifies. That shape is the whole point —
/// an abort is by definition the outcome in which the peers could not agree
/// about the middle of the hand, so a terminal that needed their agreement
/// could not be reached. `TERMINAL(k)` is taken from §3.1's
/// `ABORT_TERMINAL(k)`, a function of `GENESIS(k)` and nothing else, so two
/// peers cannot derive different values and `GENESIS(k+1)` always exists.
///
/// The last two fields are what make D-010 enforceable at the **receiver**
/// rather than trusted at the emitter: an abort moves no chips, and a copy that
/// says otherwise is rejected like any other mismatch, whoever signed it.
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct HandAbort {
    /// `1` a required cryptographic contribution never came; `2` invalid
    /// shuffle proof; `3` invalid reveal proof; `4` unresolvable divergence;
    /// `6` anti-cheat void. `5` is deleted and its code point is not reused.
    #[n(0)]
    pub cause: u16,
    /// Ascending by encoded bytes; **empty** on the hand-deadline path.
    ///
    /// Evidence only. Nothing in the protocol reads it to move a chip or to
    /// remove a player (D-010), and this client reads it for the log alone.
    #[n(1)]
    pub attributed: Vec<[u8; 32]>,
    /// `None` on every `attributed = []` path — unanimity was by construction
    /// never reached there, so no certificate can exist.
    #[cbor(n(2), with = "minicbor::bytes")]
    pub cert_hash: Option<[u8; 32]>,
    /// Required for causes 2 and 3, which are the two a single chained event
    /// proves on its own. Empty otherwise.
    #[n(3)]
    pub evidence: Vec<Vec<u8>>,
    /// **All zeroes, always.** An abort moves no chips, so there is no delta to
    /// carry. The field is kept rather than removed so that `HAND_ABORT` and
    /// `HAND_COMPLETE` stay directly comparable to a verifier, and so that "the
    /// deltas sum to zero" is one check across both terminals rather than two.
    #[n(4)]
    pub deltas: Vec<i64>,
    /// **The start-of-hand stacks, always.** Exactly the values already bound
    /// into `roster_hash(k)` and hence into `GENESIS(k)`, so every peer holds
    /// them from the genesis and two honest peers cannot derive them
    /// differently.
    #[n(5)]
    pub final_stacks: Vec<u64>,
}

impl HandAbort {
    /// The hand-deadline abort: nobody is named and no chip moves.
    pub fn on_deadline(stacks: Vec<u64>) -> Self {
        HandAbort {
            cause: 1,
            attributed: Vec::new(),
            cert_hash: None,
            evidence: Vec::new(),
            deltas: vec![0; stacks.len()],
            final_stacks: stacks,
        }
    }

    /// `cause = 2`: a shuffle proof that does not hold, with the two frames
    /// that prove it.
    ///
    /// `attributed` names the seat's key, and there is **no `cert_hash`**:
    /// `PROTOCOL.md` §4.10 pairs `attributed` with a certificate only on the
    /// `cause = 1` path, where the accusation rests on other seats agreeing
    /// that a deadline passed. Here it rests on arithmetic every receiver can
    /// redo, so a certificate would add nothing and [`consistent`] does not ask
    /// for one — its check is `cause == 1 && !attributed.is_empty()`.
    ///
    /// [`consistent`]: HandAbort::consistent
    pub fn on_bad_shuffle(accused: [u8; 32], evidence: [Vec<u8>; 2], stacks: Vec<u64>) -> Self {
        HandAbort {
            cause: 2,
            attributed: vec![accused],
            cert_hash: None,
            evidence: evidence.into(),
            deltas: vec![0; stacks.len()],
            final_stacks: stacks,
        }
    }

    /// `cause = 3`: a reveal share whose proof does not hold against the
    /// committed deck, with the one frame that carries it.
    ///
    /// **One entry, where `cause = 2` needs two.** A shuffle argument is about a
    /// *pair* of decks and the proof event names them by hash only, so the step
    /// has to travel with it. A reveal share carries its own token and its own
    /// proof, and the deck they are checked against is the committed one every
    /// seat already holds — there is nothing else to send.
    pub fn on_bad_reveal(accused: [u8; 32], evidence: Vec<u8>, stacks: Vec<u64>) -> Self {
        HandAbort {
            cause: 3,
            attributed: vec![accused],
            cert_hash: None,
            evidence: vec![evidence],
            deltas: vec![0; stacks.len()],
            final_stacks: stacks,
        }
    }

    /// Whether this body is one the receiver's own rules allow, given the
    /// stacks that receiver holds from the genesis of the hand.
    ///
    /// The two stack checks are not a formality: they are what turns D-010 from
    /// a request to emitters into something a receiver enforces.
    pub fn consistent(&self, my_stacks: &[u64]) -> Result<(), &'static str> {
        if !matches!(self.cause, 1 | 2 | 3 | 4 | 6) {
            return Err("a cause this protocol does not define");
        }
        if self.cause == 1 && !self.attributed.is_empty() && self.cert_hash.is_none() {
            return Err("a named subject with no certificate");
        }
        if self.cert_hash.is_some() && self.attributed.is_empty() {
            return Err("a certificate naming nobody");
        }
        if matches!(self.cause, 2 | 3) == self.evidence.is_empty() {
            return Err("evidence is required for causes 2 and 3 and forbidden elsewhere");
        }
        // **And bounded, which nothing checked.** The decoder's cap bounds the
        // whole body, so a two-entry `evidence` could not have been enormous —
        // but a *hundred*-entry one of small elements passed every test here
        // and reached a gate that reads `evidence[0]` and `evidence[1]`. The
        // count is §4.10's, and the per-element bound is this client's
        // transport-honest one; both are refused before anything is opened.
        if self.evidence.len() > 2 {
            return Err("more evidence than the two entries a cause may carry");
        }
        if self.evidence.iter().any(|e| e.len() > ABORT_EVIDENCE_MAX) {
            return Err("an evidence entry larger than the transport can carry");
        }
        if self.deltas.iter().any(|d| *d != 0) {
            return Err("an abort that moves a chip");
        }
        if self.final_stacks != my_stacks {
            return Err("stacks that are not this hand's own starting stacks");
        }
        if self.deltas.len() != self.final_stacks.len() {
            return Err("a delta for every seat, or none");
        }
        Ok(())
    }
}

/// `TIMEOUT_VOTE 0x0601`: one seat's word that a deadline passed.
///
/// Not an accusation and not evidence on its own. It says only *"my own timer
/// for this stage expired and I have accepted nothing from that seat at it"* —
/// and a vote alone does nothing at all. Only a complete set, one from every
/// seat in the voter set, becomes a [`TimeoutCert`].
///
/// Its envelope is `event_class = 1`, which puts it in its own anti-replay
/// slot: a voter that has already contributed to the collective stage `s` can
/// vote *about* stage `s` without equivocating against itself. A vote
/// references the stage it is about; it does not occupy it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct TimeoutVote {
    /// The stage that failed to complete.
    #[n(0)]
    pub subject_sequence: u64,
    /// The seat that failed to emit.
    #[n(1)]
    pub subject_seat: SeatIdx,
    /// What that seat owed.
    #[n(2)]
    pub subject_event_type: u16,
    /// `stage_hash(subject_sequence - 1)`.
    #[cbor(n(3), with = "minicbor::bytes")]
    pub parent_event_hash: Hash,
    /// The `next_deadline_ms` the parent stage's events carried, which is what
    /// makes the deadline a value both sides can check rather than one the
    /// voter chose.
    #[n(4)]
    pub deadline_ms: u32,
    /// `1` an action deadline, `2` a cryptographic-step deadline.
    #[n(5)]
    pub kind: u16,
}

impl TimeoutVote {
    /// The digest a certificate identifies this subject by.
    ///
    /// It commits to every field but the sequence's own position, so two
    /// certificates by one emitter at one stage share a slot **exactly when**
    /// they concern the same subject under the same deadline. That is what
    /// keeps a seat which is entitled to certify two different subjects at one
    /// stage — two mutually partitioned peers each voting against the other is
    /// enough — from manufacturing an equivocation proof against itself.
    pub fn subject_digest(&self) -> Hash {
        CertSubject::of(self).digest()
    }

    /// Whether two votes are about the same thing.
    ///
    /// Compared field by field rather than by digest, so that a mismatch can
    /// say which field differs — a voter that disagrees about `deadline_ms` has
    /// a different view of the parent stage, which is a different fault from
    /// one that names a different seat.
    pub fn same_subject(&self, other: &TimeoutVote) -> bool {
        self.subject_sequence == other.subject_sequence
            && self.subject_seat == other.subject_seat
            && self.subject_event_type == other.subject_event_type
            && self.parent_event_hash == other.parent_event_hash
            && self.deadline_ms == other.deadline_ms
            && self.kind == other.kind
    }
}

/// D-036: what a `TIMEOUT_CERT` is about -- the seats quiet at one stage under
/// one deadline, one or several. A vote names one seat; a certificate names
/// the whole set `S` and carries, for every member of `S`, a vote from every
/// seat of `V(S)` = the dealt-in seats less `S` less the seats already
/// certified. With one seat this is D-023's certificate exactly, digest and
/// all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertSubject {
    pub subject_sequence: u64,
    /// Ascending, no duplicates, never empty.
    pub subject_seats: Vec<SeatIdx>,
    pub subject_event_type: u16,
    pub parent_event_hash: Hash,
    pub deadline_ms: u32,
    pub kind: u16,
}

impl CertSubject {
    /// The one-seat subject a vote names.
    pub fn of(vote: &TimeoutVote) -> Self {
        CertSubject {
            subject_sequence: vote.subject_sequence,
            subject_seats: vec![vote.subject_seat],
            subject_event_type: vote.subject_event_type,
            parent_event_hash: vote.parent_event_hash,
            deadline_ms: vote.deadline_ms,
            kind: vote.kind,
        }
    }

    /// The digest a certificate identifies its subject by: every field but
    /// the position, the seats as one ascending byte string -- so one seat
    /// hashes to what `TimeoutVote::subject_digest` always did.
    pub fn digest(&self) -> Hash {
        crate::protocol::serialization::h(
            crate::protocol::signatures::Domain::TimeoutCert.context(),
            &[
                &self.subject_sequence.to_be_bytes(),
                self.subject_seats.as_slice(),
                &self.subject_event_type.to_be_bytes(),
                &self.parent_event_hash,
                &self.deadline_ms.to_be_bytes(),
                &self.kind.to_be_bytes(),
            ],
        )
    }

    /// The vote this certificate must carry about `seat`.
    pub fn vote_about(&self, seat: SeatIdx) -> TimeoutVote {
        TimeoutVote {
            subject_sequence: self.subject_sequence,
            subject_seat: seat,
            subject_event_type: self.subject_event_type,
            parent_event_hash: self.parent_event_hash,
            deadline_ms: self.deadline_ms,
            kind: self.kind,
        }
    }

    /// Whether a vote is about this stage under this deadline, whichever
    /// seat it names.
    pub fn same_stage(&self, v: &TimeoutVote) -> bool {
        self.subject_sequence == v.subject_sequence
            && self.subject_event_type == v.subject_event_type
            && self.parent_event_hash == v.parent_event_hash
            && self.deadline_ms == v.deadline_ms
            && self.kind == v.kind
    }
}

/// `TIMEOUT_CERT 0x0602`: the voter set, unanimously, saying a deadline passed.
///
/// D-036: about one seat or several -- `votes` carries, for every seat named
/// (ascending), the vote of every seat of `V(S)` (ascending by voter), and
/// the subject digest commits to the whole set.
///
/// A **collective** stage whose required emitter set is `V(subject)` — the same
/// set that had to vote — so each voter emits its own copy. Legal only when
/// `|V(subject)| >= 2` and a vote has been collected from **every** seat in it.
/// A certificate with `|V| < 2` is inert: not chained, not evidence, no
/// terminating effect, silently ignored.
///
/// That floor is the whole security property. At `|V| = 1` "unanimity" is the
/// signature of the single party with an interest in the outcome, which is why
/// heads-up this message can never do anything and the hand deadline is the
/// only terminus there.
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct TimeoutCert {
    /// [`TimeoutVote::subject_digest`] of the subject every carried vote names.
    #[cbor(n(0), with = "minicbor::bytes")]
    pub subject_digest: Hash,
    /// A complete `SignedEvent` per voter, ascending by voter seat.
    ///
    /// The whole signed events and not just their signatures: a receiver
    /// verifies each one the way it verifies any other event, against the
    /// sender key inside it, so a certificate carries its own proof and needs
    /// nothing from the receiver's store to be checkable.
    #[n(1)]
    pub votes: Vec<Vec<u8>>,
}

/// The street code `PROTOCOL.md` §4.7 defines: **the number of board cards**.
///
/// Pre-flop `0`, flop `3`, turn `4`, river `5`. Not the engine's `Street`
/// discriminants, which are `0,1,2,3` — that is the reading §4.7 withdraws, and
/// it is withdrawn because it was never stated either way for seven passes of
/// the document. Two conforming clients each picking a reasonable enumeration
/// mismatch on `n(0)` of every action of every hand.
///
/// It is written here as a match on `board_cards()` rather than as a table of
/// literals, so the wire code and the board length cannot drift apart: the
/// reason the count was chosen over the discriminants is that a receiver can
/// check it against the message it travels with.
pub fn street_code(street: Street) -> u16 {
    street.board_cards() as u16
}

/// The reverse, refusing anything that is not a street.
///
/// `1` and `2` are refused rather than mapped to something near them. They are
/// exactly what a client that used the engine's discriminants would send for
/// the flop and the turn, and answering that with a street is how the withdrawn
/// reading would survive in the field.
pub fn street_from_code(code: u16) -> Option<Street> {
    match code {
        0 => Some(Street::PreFlop),
        3 => Some(Street::Flop),
        4 => Some(Street::Turn),
        5 => Some(Street::River),
        _ => None,
    }
}

/// The three fields every betting action carries (`PROTOCOL.md` §4.7).
///
/// `ACTION_CHECK`, `ACTION_CALL` and `ACTION_FOLD` are exactly this and nothing
/// more. The other two add a total, and are [`ActionAmount`].
///
/// All three fields are redundant with what a receiver already knows, and that
/// is their purpose: a receiver re-runs the engine from its own state and
/// compares. `action_index` in particular is what makes a replayed action
/// visible as one — the same action at the same street from the same seat, one
/// action later, is a different `action_index` and does not verify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct ActionHead {
    #[n(0)]
    pub street: u16,
    #[n(1)]
    pub seat: u8,
    #[n(2)]
    pub action_index: u32,
}

/// `ACTION_BET`'s `amount` and `ACTION_RAISE`'s `raise_to`.
///
/// One type for both, because the field is the same field: **the seat's total
/// commitment for the round**, never an increment. TDA 43-B, and on a street
/// where `current_bet == 0` a bet's total and its increment are the same
/// number, which is exactly why carrying the total removes the ambiguity
/// instead of relocating it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct ActionAmount {
    #[n(0)]
    pub street: u16,
    #[n(1)]
    pub seat: u8,
    #[n(2)]
    pub action_index: u32,
    /// The total for the round. `amount` in `ACTION_BET`, `raise_to` in
    /// `ACTION_RAISE`; one name here because it is one quantity.
    #[n(3)]
    pub total: u64,
}

impl ActionAmount {
    /// The head, for the checks that do not care about the total.
    pub fn head(&self) -> ActionHead {
        ActionHead {
            street: self.street,
            seat: self.seat,
            action_index: self.action_index,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The street code is the board length, pinned against the engine rather
    /// than against a table of literals — because the reason §4.7 chose the
    /// count over the discriminants is that it is checkable against the message
    /// it travels with.
    #[test]
    fn the_street_code_is_the_board_length() {
        for street in [Street::PreFlop, Street::Flop, Street::Turn, Street::River] {
            assert_eq!(street_code(street) as usize, street.board_cards());
            assert_eq!(street_from_code(street_code(street)), Some(street));
        }
        assert_eq!(street_code(Street::PreFlop), 0);
        assert_eq!(street_code(Street::River), 5);
    }

    /// `1` and `2` are exactly what a client using the engine's discriminants
    /// would send for the flop and the turn. Answering them with a street is
    /// how the withdrawn reading would survive in the field.
    #[test]
    fn the_engine_discriminants_are_not_street_codes() {
        assert_eq!(street_from_code(1), None, "the flop is 3, not 1");
        assert_eq!(street_from_code(2), None, "the turn is 4, not 2");
        assert_eq!(street_from_code(6), None);
        assert_eq!(street_from_code(u16::MAX), None);
    }

    #[test]
    fn an_action_survives_the_wire() {
        let head = ActionHead {
            street: street_code(Street::Flop),
            seat: 3,
            action_index: 7,
        };
        let bytes = minicbor::to_vec(head).unwrap();
        assert_eq!(minicbor::decode::<ActionHead>(&bytes).unwrap(), head);

        let raise = ActionAmount {
            street: street_code(Street::Turn),
            seat: 3,
            action_index: 8,
            total: 1_200,
        };
        let bytes = minicbor::to_vec(raise).unwrap();
        assert_eq!(minicbor::decode::<ActionAmount>(&bytes).unwrap(), raise);
        assert_eq!(raise.head().seat, 3);
        assert_eq!(raise.head().action_index, 8);
    }

    /// The settlement report must name the field that differs, including the
    /// three fields the first version of it could not see.
    ///
    /// The case that matters is `state_hash` alone: the money agrees, the pots
    /// agree, the stacks agree, and the hand still disagrees — which is what
    /// two engines that played different hands and settled the same way look
    /// like. The old report printed two identical stack vectors there.
    #[test]
    fn a_settlement_disagreement_names_the_field() {
        let mine = HandComplete {
            pots: vec![PotAward {
                size: 300,
                eligible: vec![0, 1, 2],
                winners: vec![1],
                odd_chips: vec![],
            }],
            refunds: vec![Refund { seat: 2, amount: 50 }],
            deltas: vec![-100, 200, -100],
            final_stacks: vec![900, 1200, 900],
            busted: vec![],
            state_hash: [7u8; 32],
        };

        assert!(
            mine.disagreement(&mine).is_empty(),
            "a body must not disagree with itself"
        );

        // Only the checkpoint-8 hash differs. Every visible quantity matches.
        let theirs = HandComplete {
            state_hash: [9u8; 32],
            ..mine.clone()
        };
        let what = mine.disagreement(&theirs);
        assert_eq!(what.len(), 1, "one field differs, so one line: {what:?}");
        assert!(
            what[0].contains("state hash") && what[0].contains("07070707"),
            "it must name the field and both values: {what:?}"
        );

        // And the three fields the old report could not see are each named.
        for (label, other) in [
            (
                "deltas",
                HandComplete {
                    deltas: vec![-100, 100, 0],
                    ..mine.clone()
                },
            ),
            (
                "busted",
                HandComplete {
                    busted: vec![2],
                    ..mine.clone()
                },
            ),
            (
                "refunds",
                HandComplete {
                    refunds: vec![],
                    ..mine.clone()
                },
            ),
        ] {
            let what = mine.disagreement(&other);
            assert_eq!(what.len(), 1, "{label}: {what:?}");
            assert!(what[0].starts_with(label), "{label}: {what:?}");
        }

        // Two at once are two lines, not the first one found.
        let both = HandComplete {
            busted: vec![0],
            state_hash: [9u8; 32],
            ..mine.clone()
        };
        assert_eq!(mine.disagreement(&both).len(), 2);
    }

    #[test]
    fn a_hand_complete_survives_the_wire_and_counts_its_chips() {
        let done = HandComplete {
            pots: vec![PotAward {
                size: 300,
                eligible: vec![0, 1, 2],
                winners: vec![1],
                odd_chips: vec![],
            }],
            refunds: vec![Refund {
                seat: 2,
                amount: 50,
            }],
            deltas: vec![-100, 200, -100],
            final_stacks: vec![900, 1200, 900],
            busted: vec![],
            state_hash: [7u8; 32],
        };
        let bytes = minicbor::to_vec(&done).unwrap();
        assert_eq!(minicbor::decode::<HandComplete>(&bytes).unwrap(), done);
        assert!(done.conserves_chips());

        let invented = HandComplete {
            deltas: vec![-100, 201, -100],
            ..done
        };
        assert!(!invented.conserves_chips(), "a hand may not invent a chip");
    }

    /// Ten seats of `i64` can overflow an `i64` sum. The check must not answer
    /// "this hand is fine" because the arithmetic wrapped.
    #[test]
    fn the_chip_count_does_not_wrap() {
        let wrapping = HandComplete {
            pots: vec![],
            refunds: vec![],
            deltas: vec![i64::MAX, i64::MAX, 2],
            final_stacks: vec![],
            busted: vec![],
            state_hash: [0u8; 32],
        };
        assert!(!wrapping.conserves_chips());
    }

    #[test]
    fn the_reveal_bodies_survive_the_wire() {
        let entry = RevealEntry {
            deck_index: 4,
            token: vec![1u8; 33],
            proof: vec![2u8; 98],
        };
        let board = BoardReveal {
            street: street_code(Street::Flop),
            entries: vec![entry.clone(), entry.clone(), entry.clone()],
        };
        let bytes = minicbor::to_vec(&board).unwrap();
        assert_eq!(minicbor::decode::<BoardReveal>(&bytes).unwrap(), board);
        assert_eq!(
            board.street as usize,
            board.entries.len(),
            "the flop's code is its own entry count, which is why it was chosen"
        );

        let show = ShowdownReveal {
            entries: vec![entry.clone(), entry],
        };
        let bytes = minicbor::to_vec(&show).unwrap();
        assert_eq!(minicbor::decode::<ShowdownReveal>(&bytes).unwrap(), show);

        let muck = ShowdownMuck { forfeit: true };
        let bytes = minicbor::to_vec(muck).unwrap();
        assert_eq!(minicbor::decode::<ShowdownMuck>(&bytes).unwrap(), muck);
    }

    fn body() -> HandInit {
        HandInit {
            hand_id: 1,
            button_position: 0,
            sb_position: 0,
            bb_seat: 1,
            level: 1,
            small_blind: 50,
            big_blind: 100,
            ante: 0,
            dealt_in: vec![0, 1],
            stacks: vec![10_000, 10_000],
            roster_hash: [3; 32],
            ledger_delta: vec![(0, 10_000), (1, 10_000)],
        }
    }

    #[test]
    fn a_body_survives_the_wire() {
        let b = body();
        let bytes = crate::protocol::serialization::to_canonical(&b).unwrap();
        let back: HandInit =
            crate::protocol::serialization::from_canonical(&bytes, 512).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn two_identical_copies_agree() {
        assert_eq!(body().disagreement(&body()), Ok(()));
    }

    /// The whole reason this module exists: the receiver is told **which** field,
    /// because "the copies differ" is not something a player or a maintainer can
    /// act on and "the button" is.
    #[test]
    fn a_disagreement_names_the_field() {
        let mine = body();
        let mut theirs = body();
        theirs.button_position = 1;
        assert_eq!(
            mine.disagreement(&theirs),
            Err(NotOurs::Differs(Field::ButtonPosition))
        );
        assert_eq!(
            mine.disagreement(&theirs).unwrap_err().to_string(),
            "button_position differs from what I derived"
        );
    }

    #[test]
    fn every_field_can_be_the_one_that_differs() {
        let mine = body();
        let cases: Vec<(HandInit, Field)> = vec![
            (HandInit { hand_id: 2, ..body() }, Field::HandId),
            (HandInit { sb_position: 1, ..body() }, Field::SbPosition),
            (HandInit { bb_seat: 0, ..body() }, Field::BbSeat),
            (HandInit { level: 2, ..body() }, Field::Level),
            (
                HandInit { small_blind: 25, big_blind: 100, ..body() },
                Field::SmallBlind,
            ),
            (HandInit { big_blind: 200, ..body() }, Field::BigBlind),
            (HandInit { ante: 1, ..body() }, Field::Ante),
            (HandInit { dealt_in: vec![0], ..body() }, Field::DealtIn),
            (HandInit { stacks: vec![9_000, 10_000], ..body() }, Field::Stacks),
            (HandInit { roster_hash: [4; 32], ..body() }, Field::RosterHash),
            (
                HandInit { ledger_delta: vec![(0, 1)], ..body() },
                Field::LedgerDelta,
            ),
        ];
        for (theirs, expected) in cases {
            assert_eq!(
                mine.disagreement(&theirs),
                Err(NotOurs::Differs(expected)),
                "{expected}"
            );
        }
    }

    /// A body that contradicts itself is wrong whoever agrees with it, so it is
    /// refused before anybody's copy is compared.
    #[test]
    fn a_body_that_contradicts_itself_is_refused_on_its_own() {
        let ok = body();
        assert_eq!(ok.self_consistent(2), Ok(()));

        for (b, why) in [
            (HandInit { big_blind: 150, ..body() }, "big_blind"),
            (HandInit { ante: 5, ..body() }, "ante"),
            (HandInit { dealt_in: vec![1, 0], ..body() }, "dealt_in order"),
            (HandInit { dealt_in: vec![0, 0], ..body() }, "dealt_in repeat"),
            (
                HandInit { ledger_delta: vec![(1, 1), (0, 1)], ..body() },
                "ledger order",
            ),
        ] {
            assert!(b.self_consistent(2).is_err(), "{why}");
        }
    }

    /// A seat number outside the table is refused, and the table size is the
    /// caller's to state — this module does not guess it.
    #[test]
    fn a_seat_outside_the_table_is_refused() {
        let b = HandInit { bb_seat: 9, ..body() };
        assert!(b.self_consistent(2).is_err());
        assert!(b.self_consistent(10).is_ok(), "the same body at a bigger table");
    }
}
