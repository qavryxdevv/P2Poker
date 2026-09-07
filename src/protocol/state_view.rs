//! The public state every peer hashes and compares (`PROTOCOL.md` §6.1).
//!
//! At a checkpoint each peer computes a hash of this structure and exchanges
//! it. Agreement means the peers really are playing the same game; a mismatch
//! is the only way a silent divergence is ever caught, so what is **in** this
//! structure decides what can be detected at all.
//!
//! # The field order is the encoding
//!
//! This is a `#[cbor(array)]` struct, so the field order is not a presentation
//! choice — it is the wire format, and a struct with two fields transposed
//! hashes differently while looking identical in a debugger. §6.1's table is the
//! single normative statement of that order; it was given twice, differently, in
//! two documents until one was deleted.
//!
//! # Field 28 is gone, and it is the only field ever measured to disagree
//!
//! `signed_this_hand` was appended last, as *"the seats that signed at least one
//! chained event of this hand **which this peer accepted**"*. That qualifier is
//! the defect: it is an observation of the listener, not a fact about the hand,
//! and two honest peers on a lossy link **must** differ in it. The protocol
//! nonetheless required them to agree on a hash containing it.
//!
//! What that cost, measured on ten-seat two-machine runs (`S1-AZ` … `S1-BD`):
//! **124 settlement disagreements in one 900-second run, 553 in another, and in
//! 100 % of them the differing field was this hash** — while the count
//! mentioning stacks, deltas, pots, refunds or busted seats was **zero**. The
//! money agreed everywhere, always. A refused settlement then left the seat
//! still waited for, its action clock ran out, and it was certified out for a
//! silence it had not committed; **35 % of all timeout accusations named a seat
//! heard at the very stage it was accused of ignoring**. It could never return,
//! because §4.9 readmission wants a state hash that agrees about who took part
//! in a hand the seat did not take part in. And §6.3's freeze, the terminus for
//! exactly this, fired **zero** times against those 124, because the refusal
//! that detects the disagreement is also what stops the checkpoint being
//! reached.
//!
//! §6.1 put it here *"so that agreement there is agreement about `P` itself"*.
//! The measurement says that agreement was never attainable, so the field
//! detected a difference it could not resolve and removed honest players for it.
//! Removing it is a truncation of an append-only encoding, which is why it was
//! last.
//!
//! **The settlement comparison is untouched and is what guards the money.**
//! `on_hand_complete` recomputes and compares the whole `HandComplete` — pots,
//! deltas, final stacks, refunds, busted — independently of this hash. Chip
//! conservation does not rest on field 28 and never did.
//!
//! # Why the ledger is in here
//!
//! `ledger_in` and `ledger_out` look like bookkeeping rather than game state.
//! They are hashed because otherwise two peers could disagree about how many
//! chips have entered and left the table and never find out — chip conservation
//! would hold at each peer separately while the two disagreed about the total.

use minicbor::{Decode, Encode};

use crate::poker::state::Hash;
use crate::protocol::serialization::{self, to_canonical};
use crate::protocol::signatures::{hash, Domain};

/// One seat of the roster, as it appears in the hashed state.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct RosterEntry {
    #[n(0)]
    pub seat: u8,
    #[cbor(n(1), with = "minicbor::bytes")]
    pub app_public_key: [u8; 32],
    #[n(2)]
    pub stack: u64,
}

/// A pot as the players see it: the chips, and who may win them.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct PotView {
    #[n(0)]
    pub size: u64,
    /// Ascending seat indices.
    #[n(1)]
    pub eligible: Vec<u8>,
}

/// Everything two peers must agree on, and nothing they cannot both compute.
///
/// The field numbering is `PROTOCOL.md` §6.1's table, in its order.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct PublicTableState {
    #[n(0)]
    pub protocol_version: u16,
    #[cbor(n(1), with = "minicbor::bytes")]
    pub table_id: [u8; 32],
    #[n(2)]
    pub hand_id: u64,
    /// Which checkpoint of the hand this is, `1..=8`.
    #[n(3)]
    pub checkpoint: u16,

    /// Ascending by seat.
    #[n(4)]
    pub roster: Vec<RosterEntry>,

    /// Positions, which may be empty seats under the dead-button rule.
    #[n(5)]
    pub button_position: u8,
    #[n(6)]
    pub sb_position: u8,
    #[n(7)]
    pub bb_seat: u8,

    #[n(8)]
    pub level: u32,
    #[n(9)]
    pub small_blind: u64,
    #[n(10)]
    pub big_blind: u64,
    #[n(11)]
    pub ante: u64,

    #[n(12)]
    pub street: u16,
    /// Card codes; length 0, 3, 4 or 5.
    #[n(13)]
    pub board: Vec<u8>,

    #[n(14)]
    pub committed_this_round: Vec<u64>,
    #[n(15)]
    pub committed_this_hand: Vec<u64>,

    #[n(16)]
    pub folded: Vec<bool>,
    #[n(17)]
    pub all_in: Vec<bool>,
    #[n(18)]
    pub acted_this_round: Vec<bool>,
    #[n(19)]
    pub sitting_out: Vec<bool>,

    #[n(20)]
    pub current_bet: u64,
    #[n(21)]
    pub last_full_raise: u64,
    #[n(22)]
    pub player_to_act: Option<u8>,

    /// Derived from the commitments, never accumulated.
    #[n(23)]
    pub pots: Vec<PotView>,

    /// The final deck hash, or thirty-two zero bytes before `DECK_COMMIT`.
    #[cbor(n(24), with = "minicbor::bytes")]
    pub deck_commitment: [u8; 32],

    /// Running totals of accepted buy-ins and of removed stacks.
    #[n(25)]
    pub ledger_in: u64,
    #[n(26)]
    pub ledger_out: u64,

    /// The stage hash the settlement **chains from**.
    ///
    /// **§6.1 calls this *“the stage hash of the last completed stage”* and it
    /// cannot be that** (`S1-CJ`). This value is `slot.previous_event_hash`
    /// read while `HandComplete` is being **built** — `state_hash` is a field
    /// of that struct — so the `HAND_COMPLETE` stage has not been sealed, let
    /// alone completed. A field cannot carry the hash of the stage that
    /// carries it, so no implementation can satisfy the sentence.
    ///
    /// It is harmless because every peer computes it identically, and the
    /// comment is corrected rather than the code: which of the two moves is a
    /// wire-format decision under §10.2's pre-release deadline and belongs to
    /// the owner. The `STATE_HASH` frame's field of the same name carries
    /// `TERMINAL(k)` instead, one stage later — see `table::checkwire`.
    #[cbor(n(27), with = "minicbor::bytes")]
    pub transcript_head: [u8; 32],
}

impl PublicTableState {
    /// How many fields the wire format has.
    ///
    /// Pinned because a field added anywhere but the end is a silent
    /// incompatibility: the encoding is positional.
    pub const FIELD_COUNT: usize = 28;

    /// The hash two peers compare at a checkpoint.
    pub fn state_hash(&self) -> Result<Hash, serialization::Error> {
        let bytes = to_canonical(self)?;
        Ok(hash(Domain::State, &[&bytes]))
    }
}

// Above 23, a CBOR array length no longer fits the header byte and the encoding
// takes a one-byte count after it. The encoding test reads exactly those two
// bytes, so it would be checking the wrong ones if the struct ever shrank below
// the boundary or grew past 255 fields.
const _: () = assert!(
    PublicTableState::FIELD_COUNT > 23 && PublicTableState::FIELD_COUNT < 256
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::serialization::from_canonical;

    fn sample() -> PublicTableState {
        PublicTableState {
            protocol_version: 1,
            table_id: [0x11; 32],
            hand_id: 7,
            checkpoint: 3,
            roster: vec![
                RosterEntry { seat: 0, app_public_key: [0xA0; 32], stack: 10_000 },
                RosterEntry { seat: 1, app_public_key: [0xA1; 32], stack: 9_500 },
            ],
            button_position: 0,
            sb_position: 0,
            bb_seat: 1,
            level: 1,
            small_blind: 50,
            big_blind: 100,
            ante: 0,
            street: 1,
            board: vec![3, 17, 42],
            committed_this_round: vec![100, 100],
            committed_this_hand: vec![150, 200],
            folded: vec![false, false],
            all_in: vec![false, false],
            acted_this_round: vec![true, true],
            sitting_out: vec![false, false],
            current_bet: 100,
            last_full_raise: 100,
            player_to_act: Some(1),
            pots: vec![PotView { size: 350, eligible: vec![0, 1] }],
            deck_commitment: [0xDD; 32],
            ledger_in: 20_000,
            ledger_out: 0,
            transcript_head: [0xEE; 32],
        }
    }

    const CAP: usize = 65_536;

    #[test]
    fn the_state_round_trips_through_its_canonical_bytes() {
        let bytes = to_canonical(&sample()).unwrap();
        let back: PublicTableState = from_canonical(&bytes, CAP).unwrap();
        assert_eq!(back, sample());
    }

    #[test]
    fn it_encodes_as_an_array_of_twenty_eight_elements() {
        let bytes = to_canonical(&sample()).unwrap();
        // CBOR major type 4. A length above 23 does not fit the header byte, so
        // 28 elements encode as 0x98 (array, one-byte length) then 0x1C.
        //
        // **It said twenty-nine until 2026-09-07 and asserted twenty-eight.**
        // `signed_this_hand` was field 28 and was deleted on 2026-09-04
        // (041c39e); the assertion followed FIELD_COUNT and the name and the
        // comment did not. A test whose name states a different number from
        // the one it checks is a test a reader has to distrust, and this file
        // is where somebody counting fields would look first.
        assert_eq!(bytes[0], 0x98, "a definite-length array with a one-byte count");
        assert_eq!(
            bytes[1],
            PublicTableState::FIELD_COUNT as u8,
            "a field added anywhere but the end is a silent incompatibility"
        );
    }

    #[test]
    fn the_hash_is_stable_and_depends_on_the_state() {
        let a = sample().state_hash().unwrap();
        assert_eq!(a, sample().state_hash().unwrap());

        let mut other = sample();
        other.current_bet += 1;
        assert_ne!(a, other.state_hash().unwrap());
    }

    /// The reason the field order is normative. Two states differing only by a
    /// transposition of equally-typed fields must hash differently, or a peer
    /// with the fields swapped would agree with one that had them right.
    #[test]
    fn transposing_two_fields_changes_the_hash() {
        let base = sample().state_hash().unwrap();

        let mut swapped = sample();
        std::mem::swap(&mut swapped.ledger_in, &mut swapped.ledger_out);
        assert_ne!(base, swapped.state_hash().unwrap(), "ledger_in and ledger_out");

        let mut flags = sample();
        flags.folded = vec![true, false];
        flags.acted_this_round = vec![false, true];
        assert_ne!(base, flags.state_hash().unwrap(), "the flag vectors");
    }

    /// Every field must reach the hash. A field the encoder skips is a field two
    /// peers can disagree about forever without a checkpoint ever noticing.
    #[test]
    fn every_field_reaches_the_hash() {
        let base = sample().state_hash().unwrap();
        type Mutate = fn(&mut PublicTableState);
        let mutations: [(&str, Mutate); 28] = [
            ("protocol_version", |s| s.protocol_version = 2),
            ("table_id", |s| s.table_id = [0xFF; 32]),
            ("hand_id", |s| s.hand_id = 8),
            ("checkpoint", |s| s.checkpoint = 4),
            ("roster", |s| s.roster[0].stack = 1),
            ("button_position", |s| s.button_position = 1),
            ("sb_position", |s| s.sb_position = 1),
            ("bb_seat", |s| s.bb_seat = 0),
            ("level", |s| s.level = 2),
            ("small_blind", |s| s.small_blind = 100),
            ("big_blind", |s| s.big_blind = 200),
            ("ante", |s| s.ante = 25),
            ("street", |s| s.street = 2),
            ("board", |s| s.board = vec![3, 17, 42, 51]),
            ("committed_this_round", |s| s.committed_this_round = vec![0, 0]),
            ("committed_this_hand", |s| s.committed_this_hand = vec![0, 0]),
            ("folded", |s| s.folded = vec![true, false]),
            ("all_in", |s| s.all_in = vec![true, false]),
            ("acted_this_round", |s| s.acted_this_round = vec![false, false]),
            ("sitting_out", |s| s.sitting_out = vec![true, false]),
            ("current_bet", |s| s.current_bet = 200),
            ("last_full_raise", |s| s.last_full_raise = 200),
            ("player_to_act", |s| s.player_to_act = None),
            ("pots", |s| s.pots[0].size = 1),
            ("deck_commitment", |s| s.deck_commitment = [0; 32]),
            ("ledger_in", |s| s.ledger_in = 1),
            ("ledger_out", |s| s.ledger_out = 1),
            ("transcript_head", |s| s.transcript_head = [0; 32]),
        ];
        assert_eq!(mutations.len(), PublicTableState::FIELD_COUNT);

        for (field, mutate) in mutations {
            let mut s = sample();
            mutate(&mut s);
            assert_ne!(
                base,
                s.state_hash().unwrap(),
                "{field} does not reach the state hash"
            );
        }
    }

    /// The hash is domain-separated, so the same bytes under another
    /// construction do not collide with a state hash.
    #[test]
    fn the_state_hash_is_domain_separated() {
        let bytes = to_canonical(&sample()).unwrap();
        assert_ne!(
            sample().state_hash().unwrap(),
            hash(Domain::Transcript, &[&bytes]),
            "a state must not hash as a transcript event"
        );
    }

    #[test]
    fn a_state_larger_than_its_cap_is_refused() {
        let bytes = to_canonical(&sample()).unwrap();
        let too_small = bytes.len() - 1;
        assert!(from_canonical::<PublicTableState>(&bytes, too_small).is_err());
    }
}
