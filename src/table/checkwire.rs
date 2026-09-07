//! `STATE_HASH` and `STATE_ACK` on the wire (`PROTOCOL.md` §4.9, §6.2).
//!
//! The two bodies of the boundary checkpoint, and nothing else: the records
//! they feed are `protocol::checkpoint`'s and the stage they occupy is
//! `table::checkpoint`'s. This module is the encoding and the reasons for it.
//!
//! # Every field is derived, so there is no writer
//!
//! Both bodies are **collective** stages. Each seat computes its copy from its
//! own engine and signs it; a receiver compares rather than believes. That is
//! the whole point of the checkpoint — §6.1's `state_hash` is *"the only way a
//! silent divergence is ever caught"*, and a body one seat could choose would
//! be a body that proves nothing.
//!
//! # The sequence band, and why it is above everything else
//!
//! §4.9 puts the `STATE_HASH` stage at `sequence = BOUNDARY_CHECKPOINT_BASE`
//! (8 192) and its `STATE_ACK` at `+ 1`, with reconciliation round `r` taking
//! `+ 2r` and `+ 2r + 1` for `1 <= r <= 7` — the sixteen values `8 192 … 8 207`.
//!
//! Every one of them is disjoint from an ordinary stage index, which
//! `MAX_STAGES_PER_HAND = 2 048` bounds at 2 047, and from the hand boundary
//! window at `4 096 … 4 105`. A reconciliation round extends **upwards**, so a
//! base below the window would let a disputed checkpoint walk into the slots a
//! seat's `PLAYER_SIT_IN` reserves.

use minicbor::{Decode, Encode};

use crate::protocol::constants::{BOUNDARY_CHECKPOINT_BASE, MAX_RECONCILIATION_ROUNDS};

/// `0x0701 STATE_HASH` — §4.9's three fields, in its numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct StateHash {
    /// Which checkpoint of the hand. The boundary checkpoint is **8**.
    #[n(0)]
    pub checkpoint: u16,
    /// This peer's own hash of `PublicTableState` (§6.1).
    #[cbor(n(1), with = "minicbor::bytes")]
    pub state_hash: [u8; 32],
    /// `TERMINAL(k)` — on the settled path, the `HAND_COMPLETE` stage hash.
    ///
    /// **This is NOT the same value as `PublicTableState`'s field of the same
    /// name, and both carried §6.1's one sentence** (`S1-CJ`). The hashed one
    /// is the stage the settlement chains **from**, one stage earlier, and it
    /// cannot be anything else. This one is filled from `Hand::checkpoint8`'s
    /// terminal (`table::hand`) and from `Boundary::terminal`
    /// (`table::boundary`).
    ///
    /// Which of the two §6.1 means is the owner's, under §10.2's pre-release
    /// deadline. Neither comment is a licence to change the value.
    #[cbor(n(2), with = "minicbor::bytes")]
    pub transcript_head: [u8; 32],
}

/// `0x0702 STATE_ACK` — the stage immediately after.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct StateAck {
    #[n(0)]
    pub checkpoint: u16,
    /// The value the `STATE_HASH` stage agreed on. Repeated here so an
    /// acknowledgement says **what** it acknowledges rather than only that it
    /// does: a bare ack of a stage is an ack of whatever the reader thinks that
    /// stage said.
    #[cbor(n(1), with = "minicbor::bytes")]
    pub agreed_state_hash: [u8; 32],
    /// The `stage_hash` of the `STATE_HASH` stage this confirms.
    #[cbor(n(2), with = "minicbor::bytes")]
    pub checkpoint_hash: [u8; 32],
}

/// The boundary checkpoint's number. §6.2 row 8.
pub const BOUNDARY_CHECKPOINT: u16 = 8;

/// Where round `r`'s `STATE_HASH` stage sits.
///
/// Round 0 is the checkpoint itself; `1 ..= MAX_RECONCILIATION_ROUNDS` are
/// §6.3 step 3's re-derivations. A round outside that range has no sequence and
/// returns `None` rather than an index outside the band, because §4.0 step 12
/// treats a checkpoint-8 event outside `8 192 … 8 207` as a stage violation and
/// producing one would be emitting the violation rather than refusing it.
pub fn hash_sequence(round: u16) -> Option<u64> {
    if round > MAX_RECONCILIATION_ROUNDS {
        return None;
    }
    Some(BOUNDARY_CHECKPOINT_BASE + 2 * u64::from(round))
}

/// Where round `r`'s `STATE_ACK` stage sits: always the sequence after its hash.
///
/// **Kept adjacent on purpose.** §4.9: *"the pair kept together so a round's
/// acknowledgement can never land on the next round's hash."*
pub fn ack_sequence(round: u16) -> Option<u64> {
    hash_sequence(round).map(|s| s + 1)
}

/// Which round a sequence in the band belongs to, and whether it is a hash or
/// an ack. `None` for anything outside the band.
pub fn round_of(sequence: u64) -> Option<(u16, Kind)> {
    let last = ack_sequence(MAX_RECONCILIATION_ROUNDS)?;
    if !(BOUNDARY_CHECKPOINT_BASE..=last).contains(&sequence) {
        return None;
    }
    let offset = sequence - BOUNDARY_CHECKPOINT_BASE;
    let round = u16::try_from(offset / 2).ok()?;
    Some((
        round,
        if offset % 2 == 0 {
            Kind::Hash
        } else {
            Kind::Ack
        },
    ))
}

/// Which half of a round a sequence names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Hash,
    Ack,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The band is `8 192 … 8 207` and every value in it is a round and a half.
    ///
    /// **The arithmetic is the specification's**, so it is checked against the
    /// specification's own numbers rather than against itself: §4.9 names the
    /// base, the `+ 2r` / `+ 2r + 1` pairing, `1 <= r <= 7`, and the band's two
    /// ends. A change to any of them that this test survives is a change that
    /// has left the corpus behind.
    #[test]
    fn the_band_is_the_sixteen_values_the_corpus_names() {
        assert_eq!(hash_sequence(0), Some(8_192));
        assert_eq!(ack_sequence(0), Some(8_193));
        assert_eq!(hash_sequence(7), Some(8_206));
        assert_eq!(ack_sequence(7), Some(8_207));

        // Eight rounds would be 8 208, which is outside the band §4.0 step 12
        // admits. Refused here rather than emitted and refused there.
        assert_eq!(hash_sequence(8), None);
        assert_eq!(ack_sequence(8), None);

        for round in 0..=MAX_RECONCILIATION_ROUNDS {
            let h = hash_sequence(round).expect("a round in range has a sequence");
            let a = ack_sequence(round).expect("and an ack beside it");
            assert_eq!(a, h + 1, "the pair is adjacent, so an ack cannot land on the next round's hash");
            assert_eq!(round_of(h), Some((round, Kind::Hash)));
            assert_eq!(round_of(a), Some((round, Kind::Ack)));
        }
    }

    /// Nothing outside the band is read as a checkpoint stage.
    ///
    /// The two neighbours matter most: `8 191` is one below the base and
    /// `8 208` is one above the last ack, and a reader that accepted either
    /// would be admitting an event at a sequence §4.0 step 12 calls a stage
    /// violation.
    #[test]
    fn nothing_outside_the_band_is_a_checkpoint_stage() {
        for outside in [0u64, 1, 2_047, 4_096, 4_105, 8_191, 8_208, u64::MAX] {
            assert_eq!(
                round_of(outside),
                None,
                "{outside} is outside 8 192..=8 207 and is not a checkpoint stage"
            );
        }
    }

    /// A body round-trips, and the field order is the encoding.
    ///
    /// `#[cbor(array)]`, so two fields transposed hash differently while
    /// looking identical in a debugger — the same trap `state_view.rs` records
    /// for `PublicTableState`.
    #[test]
    fn both_bodies_round_trip_in_the_order_the_corpus_gives() {
        let h = StateHash {
            checkpoint: BOUNDARY_CHECKPOINT,
            state_hash: [1u8; 32],
            transcript_head: [2u8; 32],
        };
        let bytes = minicbor::to_vec(h).expect("a body encodes");
        let back: StateHash = minicbor::decode(&bytes).expect("and decodes");
        assert_eq!(back, h);

        let a = StateAck {
            checkpoint: BOUNDARY_CHECKPOINT,
            agreed_state_hash: [3u8; 32],
            checkpoint_hash: [4u8; 32],
        };
        let bytes = minicbor::to_vec(a).expect("a body encodes");
        let back: StateAck = minicbor::decode(&bytes).expect("and decodes");
        assert_eq!(back, a);
    }
}
