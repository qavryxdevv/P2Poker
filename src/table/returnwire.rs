//! The return certificate's wire: `RETURN_VOTE 0x0603` and `RETURN_CERT
//! 0x0604`, and the sequence band they are sealed in.
//!
//! `S1-BM`, the answer to `Q-10`. A seat certified out of a table could never
//! come back: `next_hand` filters `R(k)` in both of its branches, so the
//! roster only ever shrinks (`D-024`). The shrink side is certificate-gated
//! — a `TIMEOUT_CERT` is a unanimous, byte-identical, standalone-decidable
//! artefact — and the grow side must reuse exactly that class of artefact or
//! introduce a per-receiver quantity into `GENESIS(k+1)`, which `D-012`
//! forbids above all others. So:
//!
//! ```text
//! R(k+1) = ((R(k) \ OUT(k)) ∪ IN(k)) ∩ ALIVE(k+1)
//! ```
//!
//! where `OUT(k)` is the subject set of complete `TIMEOUT_CERT`s of hand `k`
//! and `IN(k)` the subject set of complete `RETURN_CERT`s of hand `k`.
//!
//! **Everything here is parented on `TERMINAL(k)`**, the one anchor in the
//! boundary that §4.10 proves agreed on both the settled and the aborted
//! path. A vote and a certificate about subject `s` are sealed at sequence
//! [`return_sequence`]`(s)` with `previous_event_hash = TERMINAL(k)`, the vote
//! in event class 1 and the certificate in class 2, exactly as the timeout
//! pair references its stage without occupying it.
//!
//! **The certificate carries the subject's own evidence** — its signed
//! `PLAYER_SIT_IN` from the boundary window and its signed checkpoint-8
//! `STATE_HASH` — so a receiver that never heard the subject can verify the
//! whole claim by itself. That is `D-024`'s *the artefact proves its own
//! position*, applied to the subject rather than to the stage, and it is what
//! makes a certificate admissible where a bare request is not.

use minicbor::{Decode, Encode};

use crate::poker::state::SeatIdx;
use crate::protocol::constants::{MAX_SEATS, RETURN_SEQUENCE_BASE};
use crate::poker::state::Hash;

/// One voter's word: *I hold this seat's request and its checkpoint value,
/// and that value is my own*. It does nothing alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct ReturnVote {
    /// The seat asking to sit back in. Outside `R(k)`.
    #[n(0)]
    pub subject_seat: SeatIdx,
    /// `TERMINAL(k)`, the hash every party to this boundary holds.
    #[cbor(n(1), with = "minicbor::bytes")]
    pub terminal: Hash,
    /// `event_hash` of the subject's `PLAYER_SIT_IN` at this boundary.
    #[cbor(n(2), with = "minicbor::bytes")]
    pub request_hash: Hash,
    /// The subject's checkpoint-8 `state_hash`, which the voter attests equals
    /// its own.
    #[cbor(n(3), with = "minicbor::bytes")]
    pub state_hash: Hash,
}

impl ReturnVote {
    /// The name every vote about one subject hashes to, in its own domain so
    /// it can never collide with a timeout subject's digest.
    pub fn subject_digest(&self) -> Hash {
        crate::protocol::serialization::h(
            crate::protocol::signatures::Domain::ReturnCert.context(),
            &[
                &[self.subject_seat],
                &self.terminal,
                &self.request_hash,
                &self.state_hash,
            ],
        )
    }

    pub fn same_subject(&self, other: &ReturnVote) -> bool {
        self.subject_seat == other.subject_seat
            && self.terminal == other.terminal
            && self.request_hash == other.request_hash
            && self.state_hash == other.state_hash
    }
}

/// The unanimous artefact: every voter's signed `RETURN_VOTE`, ascending by
/// voter seat, and the subject's own two signed events.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct ReturnCert {
    #[cbor(n(0), with = "minicbor::bytes")]
    pub subject_digest: Hash,
    /// The signed `RETURN_VOTE`s, one per voter, ascending by seat.
    #[n(1)]
    pub votes: Vec<Vec<u8>>,
    /// The subject's signed `PLAYER_SIT_IN`, sealed in the boundary window.
    #[n(2)]
    pub request: Vec<u8>,
    /// The subject's signed checkpoint-8 `STATE_HASH`.
    #[n(3)]
    pub checkpoint: Vec<u8>,
}

/// A vote is four fixed fields; the cap is generous and still a tenth of a
/// frame.
pub const RETURN_VOTE_CAP: usize = 160;
/// Nine votes of at most a frame each cannot fit in a body cap, so the cap is
/// on what a certificate *reasonably* is: nine signed votes of a few hundred
/// bytes, a sit-in of a hundred and a checkpoint of two hundred.
pub const RETURN_CERT_CAP: usize = 8_192;

/// The one slot a return about `subject` is sealed at, for a vote and for the
/// certificate alike. `None` for a seat off the table.
pub fn return_sequence(subject: SeatIdx) -> Option<u64> {
    if subject >= MAX_SEATS {
        return None;
    }
    Some(RETURN_SEQUENCE_BASE + u64::from(subject))
}

/// The inverse: which subject a sequence in the return band is about.
pub fn subject_of(sequence: u64) -> Option<SeatIdx> {
    let offset = sequence.checked_sub(RETURN_SEQUENCE_BASE)?;
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
        BOUNDARY_CHECKPOINT_BASE, BOUNDARY_SEQUENCE_BASE, MAX_RECONCILIATION_ROUNDS,
        MAX_STAGES_PER_HAND,
    };

    #[test]
    fn every_seat_has_one_return_slot_and_the_map_is_a_bijection() {
        let mut seen = std::collections::BTreeSet::new();
        for seat in 0..MAX_SEATS {
            let s = return_sequence(seat).expect("a seat of the table has a return slot");
            assert!(seen.insert(s), "seat {seat} shares a slot");
            assert_eq!(subject_of(s), Some(seat));
            assert!(in_the_band(s));
        }
        assert_eq!(return_sequence(MAX_SEATS), None);
        assert_eq!(subject_of(RETURN_SEQUENCE_BASE - 1), None);
        assert_eq!(subject_of(u64::MAX), None);
    }

    /// The band sits above the hand's stages, above the boundary window and
    /// above the whole reconciliation band, and touches none of them.
    #[test]
    fn the_return_band_meets_no_neighbour() {
        let last_ack = BOUNDARY_CHECKPOINT_BASE + 2 * u64::from(MAX_RECONCILIATION_ROUNDS) + 1;
        assert!(RETURN_SEQUENCE_BASE > last_ack, "above the last reconciliation ack");
        assert!(RETURN_SEQUENCE_BASE > BOUNDARY_SEQUENCE_BASE + u64::from(MAX_SEATS));
        assert!(RETURN_SEQUENCE_BASE > MAX_STAGES_PER_HAND);
        for seat in 0..MAX_SEATS {
            let s = return_sequence(seat).unwrap();
            assert!(crate::table::seatwire::seat_of(s).is_none(), "not a window slot");
            assert!(crate::table::checkwire::round_of(s).is_none(), "not a checkpoint stage");
        }
    }

    #[test]
    fn a_vote_encodes_within_its_cap_and_round_trips() {
        let v = ReturnVote {
            subject_seat: 7,
            terminal: [1; 32],
            request_hash: [2; 32],
            state_hash: [3; 32],
        };
        let bytes = crate::protocol::serialization::to_canonical(&v).expect("encodes");
        assert!(bytes.len() <= RETURN_VOTE_CAP, "{} bytes", bytes.len());
        let back: ReturnVote =
            crate::protocol::serialization::from_canonical(&bytes, RETURN_VOTE_CAP).expect("decodes");
        assert_eq!(back, v);
        assert!(v.same_subject(&back));
    }

    /// The digest is over every field and in its own domain: two votes that
    /// differ in any one field name different subjects, and a return digest
    /// can never equal a timeout digest of the same bytes.
    #[test]
    fn the_subject_digest_covers_every_field_in_its_own_domain() {
        let base = ReturnVote {
            subject_seat: 2,
            terminal: [1; 32],
            request_hash: [2; 32],
            state_hash: [3; 32],
        };
        let d = base.subject_digest();
        let mut seat = base;
        seat.subject_seat = 3;
        let mut term = base;
        term.terminal = [9; 32];
        let mut req = base;
        req.request_hash = [9; 32];
        let mut st = base;
        st.state_hash = [9; 32];
        for (name, other) in [("seat", seat), ("terminal", term), ("request", req), ("state", st)] {
            assert_ne!(other.subject_digest(), d, "{name} is not in the digest");
            assert!(!other.same_subject(&base), "{name} is not in same_subject");
        }
        let timeout_domain = crate::protocol::serialization::h(
            crate::protocol::signatures::Domain::TimeoutCert.context(),
            &[&[base.subject_seat], &base.terminal, &base.request_hash, &base.state_hash],
        );
        assert_ne!(d, timeout_domain, "a return digest lives in its own domain");
    }
}
