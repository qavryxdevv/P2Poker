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

use crate::poker::state::{Hash, SeatIdx};

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

#[cfg(test)]
mod tests {
    use super::*;

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
