//! The cheat certificate's wire: `CHEAT_VOTE 0x0605` and `CHEAT_CERT 0x0606`,
//! and the sequence band they are sealed in.
//!
//! `S1-KR`, the rogue-cheat audit of 2026-10-01 (its G4 and G8). A seat whose
//! card share does not hold voided the hand (`PROTOCOL.md` §4.10, `cause = 3`)
//! and was dealt the next one: nobody was put out, so one modified client
//! voided every hand it liked, for ever. `D-084` certifies a shuffle cheat out
//! in the hand itself and left the card share out on purpose (*why not a card
//! share*): a reveal stage closes at a seat the rogue sent a good share to,
//! while one sent a bad share would stand on it. So the hand still ends at once
//! on the evidence, as before, and the table certifies the seat out here.
//!
//! **Evidence decides, not unanimity.** A vote is a seat's word that it holds
//! the subject proven -- a reveal frame of the subject's own signing, naming
//! this client's deck, whose share is `Invalid` against this client's own deck
//! (`Hand::judge_reveal`) -- and it moves nothing alone. A certificate is two
//! votes and one such frame, and a receiver banks it only where the subject is
//! proven by its own check, before or on the frame the certificate carries.
//! Two rogues certifying an honest seat bank nowhere honest: an honest seat's
//! shares hold wherever its deck is held. A partner of the cheat that never
//! votes blocks nothing: two honest judges are two votes.
//!
//! **Everything is parented on `ANCHOR(k)`**, the abort terminal of hand `k`
//! (`transcript::abort_terminal`), a function of the hand's genesis that every
//! seat of the hand holds from its opening, whether the hand then settled or
//! was given up -- so a rogue that settles the hand at some seats and has it
//! given up at others does not part the votes. The subject digest is over the
//! seat and the anchor alone: a rogue that sends each judge another broken
//! share still meets two votes.

use minicbor::{Decode, Encode};

use crate::poker::state::{Hash, SeatIdx};
use crate::protocol::constants::{CHEAT_SEQUENCE_BASE, MAX_SEATS};

/// One voter's word: *this seat is proven here, by this frame* -- the frame
/// itself, so that a seat the evidence never reached judges it from the vote:
/// a rogue that accused itself to one seat only, an accomplice's abort sent to
/// one. It does nothing alone, and nothing where its frame holds.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct CheatVote {
    /// The seat proven. Dealt in to hand `k`.
    #[n(0)]
    pub subject_seat: SeatIdx,
    /// `ANCHOR(k)`: the abort terminal of the hand the cheat was in.
    #[cbor(n(1), with = "minicbor::bytes")]
    pub anchor: Hash,
    /// §4.10's cause the frame proves: `3`, a card share that does not hold.
    #[n(2)]
    pub cause: u16,
    /// A reveal frame of the subject's own signing that this voter holds it
    /// proven by. Not in the digest: two judges may hold two broken frames.
    #[cbor(n(3), with = "minicbor::bytes")]
    pub evidence: Vec<u8>,
}

impl CheatVote {
    /// The name every vote about one subject of one hand hashes to.
    pub fn subject_digest(&self) -> Hash {
        subject_digest(self.subject_seat, &self.anchor)
    }
}

/// `H(Domain::CheatCert, subject, anchor)` -- in its own domain, so it can
/// never collide with a timeout or a return digest.
pub fn subject_digest(subject: SeatIdx, anchor: &Hash) -> Hash {
    crate::protocol::serialization::h(
        crate::protocol::signatures::Domain::CheatCert.context(),
        &[&[subject], anchor],
    )
}

/// Two voters' signed `CHEAT_VOTE`s, ascending by voter seat -- each carrying
/// its frame, which every receiver judges itself.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct CheatCert {
    #[cbor(n(0), with = "minicbor::bytes")]
    pub subject_digest: Hash,
    /// The signed `CHEAT_VOTE`s, one per voter, ascending by seat.
    #[n(1)]
    pub votes: Vec<Vec<u8>>,
}

/// A vote is three fixed fields and one reveal frame: at most a
/// `DEAL_PRIVATE` body and its envelope, with room to spare.
pub const CHEAT_VOTE_CAP: usize = 5_120;
/// Two signed votes, each under the vote's cap and its own envelope, with room
/// to spare, under the frame cap the certificate itself is sealed in.
pub const CHEAT_CERT_CAP: usize = 12_288;

/// The one slot a cheat about `subject` is sealed at, for a vote and for the
/// certificate alike. `None` for a seat off the table.
pub fn cheat_sequence(subject: SeatIdx) -> Option<u64> {
    if subject >= MAX_SEATS {
        return None;
    }
    Some(CHEAT_SEQUENCE_BASE + u64::from(subject))
}

/// The inverse: which subject a sequence in the cheat band is about.
pub fn subject_of(sequence: u64) -> Option<SeatIdx> {
    let offset = sequence.checked_sub(CHEAT_SEQUENCE_BASE)?;
    let seat = SeatIdx::try_from(offset).ok()?;
    if seat >= MAX_SEATS {
        return None;
    }
    Some(seat)
}

pub fn in_the_band(sequence: u64) -> bool {
    subject_of(sequence).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::constants::{
        BOUNDARY_CHECKPOINT_BASE, BOUNDARY_SEQUENCE_BASE, MAX_RECONCILIATION_ROUNDS, MAX_STAGES_PER_HAND,
    };

    #[test]
    fn every_seat_has_one_cheat_slot_and_the_map_is_a_bijection() {
        let mut seen = std::collections::BTreeSet::new();
        for seat in 0..MAX_SEATS {
            let s = cheat_sequence(seat).expect("a seat of the table has a cheat slot");
            assert!(seen.insert(s), "seat {seat} shares a slot");
            assert_eq!(subject_of(s), Some(seat));
            assert!(in_the_band(s));
        }
        assert_eq!(cheat_sequence(MAX_SEATS), None);
        assert_eq!(subject_of(CHEAT_SEQUENCE_BASE - 1), None);
        assert_eq!(subject_of(u64::MAX), None);
    }

    /// The band sits above the hand's stages, the boundary window, the whole
    /// reconciliation band and the return band, and touches none of them.
    #[test]
    fn the_cheat_band_meets_no_neighbour() {
        let last_ack = BOUNDARY_CHECKPOINT_BASE + 2 * u64::from(MAX_RECONCILIATION_ROUNDS) + 1;
        assert!(CHEAT_SEQUENCE_BASE > last_ack, "above the last reconciliation ack");
        assert!(CHEAT_SEQUENCE_BASE > BOUNDARY_SEQUENCE_BASE + u64::from(MAX_SEATS));
        assert!(CHEAT_SEQUENCE_BASE > MAX_STAGES_PER_HAND);
        for seat in 0..MAX_SEATS {
            let s = cheat_sequence(seat).unwrap();
            assert!(crate::table::seatwire::seat_of(s).is_none(), "not a window slot");
            assert!(crate::table::checkwire::round_of(s).is_none(), "not a checkpoint stage");
            assert!(crate::table::returnwire::subject_of(s).is_none(), "not a return slot");
        }
        for seat in 0..MAX_SEATS {
            let r = crate::table::returnwire::return_sequence(seat).unwrap();
            assert!(subject_of(r).is_none(), "a return slot is not a cheat slot");
        }
    }

    /// A vote carrying the largest reveal frame there is -- a `DEAL_PRIVATE`
    /// body at its cap under the largest envelope -- fits its cap, and two of
    /// them signed fit the certificate's, under the frame cap.
    #[test]
    fn a_vote_encodes_within_its_cap_and_round_trips() {
        let frame = vec![9u8; crate::table::hand::DEAL_PRIVATE_CAP + crate::table::hand::ENVELOPE_MAX];
        let v = CheatVote { subject_seat: 7, anchor: [1; 32], cause: 3, evidence: frame };
        let bytes = crate::protocol::serialization::to_canonical(&v).expect("encodes");
        assert!(bytes.len() <= CHEAT_VOTE_CAP, "{} bytes", bytes.len());
        let back: CheatVote =
            crate::protocol::serialization::from_canonical(&bytes, CHEAT_VOTE_CAP).expect("decodes");
        assert_eq!(back, v);
        let vote_frame = bytes.len() + crate::table::hand::ENVELOPE_MAX;
        assert!(2 * vote_frame + 64 <= CHEAT_CERT_CAP, "two votes fit a certificate: {vote_frame} bytes each");
        assert!(CHEAT_CERT_CAP + crate::table::hand::ENVELOPE_MAX <= crate::table::hand::FRAME_CAP);
    }

    /// The digest is over the seat and the anchor and nothing else -- two
    /// judges holding two different broken frames name one subject -- and in
    /// its own domain.
    #[test]
    fn the_subject_digest_is_the_seat_and_the_anchor_in_its_own_domain() {
        let base = CheatVote { subject_seat: 2, anchor: [1; 32], cause: 3, evidence: vec![2; 40] };
        let d = base.subject_digest();
        let mut other_frame = base.clone();
        other_frame.evidence = vec![9; 40];
        assert_eq!(other_frame.subject_digest(), d, "the evidence is not in the digest");
        let mut seat = base.clone();
        seat.subject_seat = 3;
        assert_ne!(seat.subject_digest(), d, "the seat is");
        let mut anchor = base.clone();
        anchor.anchor = [9; 32];
        assert_ne!(anchor.subject_digest(), d, "the anchor is");
        let return_domain = crate::protocol::serialization::h(
            crate::protocol::signatures::Domain::ReturnCert.context(),
            &[&[base.subject_seat], &base.anchor],
        );
        assert_ne!(d, return_domain, "a cheat digest lives in its own domain");
    }
}
