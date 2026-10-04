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

/// One voter's word: *this seat is proven here, by these frames* -- the frames
/// themselves, so that a seat the evidence never reached judges it from the
/// vote: a rogue that accused itself to one seat only, an accomplice's abort
/// sent to one. It does nothing alone, and nothing where its frames hold.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct CheatVote {
    /// The seat proven. Dealt in to hand `k`.
    #[n(0)]
    pub subject_seat: SeatIdx,
    /// `ANCHOR(k)`: the abort terminal of the hand the cheat was in.
    #[cbor(n(1), with = "minicbor::bytes")]
    pub anchor: Hash,
    /// What the frames prove: [`CAUSE_REVEAL`], [`CAUSE_SHUFFLE`],
    /// [`CAUSE_KEY`], [`CAUSE_ACTION`], [`CAUSE_MONEY`] or
    /// [`CAUSE_EQUIVOCATION`].
    #[n(2)]
    pub cause: u16,
    /// Frames of the subject's own signing that this voter holds it proven by:
    /// one reveal; one deck key; one shuffle step, or a step and the proof
    /// bound to it; one betting action; one settlement; two frames at one slot.
    /// Not in the digest: two judges may hold two broken frames.
    #[n(3)]
    pub evidence: Vec<minicbor::bytes::ByteVec>,
}

/// A card share that does not hold -- §4.10's `cause = 3` (`S1-KR`).
pub const CAUSE_REVEAL: u16 = 3;
/// A shuffle step or proof of the seat's own that is not well formed: a deck
/// that is not fifty-two cards, bytes that do not decode, a proof whose round or
/// output is not its step's (`S1-KS`). The argument itself is `D-084`'s, and
/// since `D-098` this cause's too where the judge holds the prover's own
/// context: the shuffler of that round in its chain, the proof under its
/// aggregate key and from its deck after the round before.
pub const CAUSE_SHUFFLE: u16 = 2;
/// A deck key that does not hold: it does not decode, it is the identity, or its
/// ownership proof fails at its own context (`S1-KS`). Not a cause of §4.10's.
pub const CAUSE_KEY: u16 = 7;
/// A betting action the rules refuse at the betting stage it was signed at: out
/// of turn there, a body that does not decode, or an action the engine refuses
/// at that stage's round (`S1-KT`). Not a cause of §4.10's.
pub const CAUSE_ACTION: u16 = 8;
/// A settlement (`HAND_COMPLETE`) that pays other money than the one every
/// client of the engine derives at its own position -- pots, refunds, final
/// stacks, deltas, busted seats (`HandComplete::money_bytes`), judged against the
/// judge's own derivation and nothing borrowed (`G9`). Not a cause of §4.10's.
pub const CAUSE_MONEY: u16 = 9;

/// Two frames of the hand's chain (`HAND_INIT` to `HAND_COMPLETE`) the seat
/// signed at one slot -- one sequence, one parent -- with different event
/// hashes (`G11`, `D-100`): an honest client signs one body a slot, through
/// its armed signing journal, across any crash or restart, and replays the
/// bytes it recorded. Judged by the pair alone, at any position. Not a cause
/// of §4.10's.
pub const CAUSE_EQUIVOCATION: u16 = 10;

/// The causes the band knows.
pub fn cause_known(cause: u16) -> bool {
    matches!(cause, CAUSE_REVEAL | CAUSE_SHUFFLE | CAUSE_KEY | CAUSE_ACTION | CAUSE_MONEY | CAUSE_EQUIVOCATION)
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
    /// The signed `CHEAT_VOTE`s, one per voter, ascending by seat -- each a
    /// byte string, not a list of numbers twice its size.
    #[n(1)]
    pub votes: Vec<minicbor::bytes::ByteVec>,
}

/// A vote is three fixed fields and its frames: at most a shuffle step and the
/// proof bound to it, each at its cap under the largest envelope.
pub const CHEAT_VOTE_CAP: usize = 13_312;
/// `G11` (`REFUTE_G11_v2` M4): a vote about two versions of one stage carries
/// both -- at most two frames of the largest kind of the hand's chain, a
/// `SHUFFLE_PROOF` at its cap under the largest envelope, and the fixed fields.
/// Above `FRAME_CAP`: such a vote has a ceiling of its own on every road
/// (`hand::frame_ceiling`). A certificate carries two such votes where they fit
/// its cap -- two small pairs do -- and two that do not bank as two votes,
/// carried apart (`S1-KS`). Every other cause's vote stays within
/// [`CHEAT_VOTE_CAP`].
pub const CHEAT_PAIR_VOTE_CAP: usize =
    2 * (crate::table::hand::SHUFFLE_PROOF_CAP + crate::table::hand::ENVELOPE_MAX) + 512;
/// Two signed votes about a card share or a deck key, with room to spare, under
/// the frame cap the certificate itself is sealed in. Two votes about a shuffle
/// do not fit one frame: those bank as two votes, carried apart.
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

    fn frames(sizes: &[usize]) -> Vec<minicbor::bytes::ByteVec> {
        sizes.iter().map(|n| minicbor::bytes::ByteVec::from(vec![9u8; *n])).collect()
    }

    /// The largest evidence there is -- a shuffle step and its proof, each at
    /// its cap under the largest envelope -- fits a vote, and a vote fits a
    /// frame; two votes about a card share fit a certificate.
    #[test]
    fn a_vote_encodes_within_its_cap_and_round_trips() {
        use crate::table::hand::{
            DEAL_PRIVATE_CAP, DECK_INIT_CAP, ENVELOPE_MAX, FRAME_CAP, SHUFFLE_PROOF_CAP, SHUFFLE_STEP_CAP,
        };
        let shuffle = frames(&[SHUFFLE_STEP_CAP + ENVELOPE_MAX, SHUFFLE_PROOF_CAP + ENVELOPE_MAX]);
        let v = CheatVote { subject_seat: 7, anchor: [1; 32], cause: CAUSE_SHUFFLE, evidence: shuffle };
        let bytes = crate::protocol::serialization::to_canonical(&v).expect("encodes");
        assert!(bytes.len() <= CHEAT_VOTE_CAP, "{} bytes", bytes.len());
        let back: CheatVote =
            crate::protocol::serialization::from_canonical(&bytes, CHEAT_VOTE_CAP).expect("decodes");
        assert_eq!(back, v);
        assert!(CHEAT_VOTE_CAP + ENVELOPE_MAX <= FRAME_CAP, "a vote fits a frame");
        for (cause, size) in [(CAUSE_REVEAL, DEAL_PRIVATE_CAP), (CAUSE_KEY, DECK_INIT_CAP)] {
            let v = CheatVote { subject_seat: 7, anchor: [1; 32], cause, evidence: frames(&[size + ENVELOPE_MAX]) };
            let vote_frame = crate::protocol::serialization::to_canonical(&v).unwrap().len() + ENVELOPE_MAX;
            assert!(2 * vote_frame + 64 <= CHEAT_CERT_CAP, "two votes fit a certificate: {vote_frame} bytes each");
        }
        assert!(CHEAT_CERT_CAP + ENVELOPE_MAX <= FRAME_CAP);
    }

    /// `G11` (`REFUTE_G11_v2` M4): two frames of the largest kind of the hand's
    /// chain fit a pair vote, and a pair vote fits the transport -- a Tox
    /// message (`fragment::MAX_MESSAGE`) and a GossipSub one.
    #[test]
    fn a_pair_vote_fits_two_frames_of_the_largest_kind_and_the_transport() {
        use crate::table::hand::{frame_ceiling, ENVELOPE_MAX, FRAME_OPEN_MAX, SHUFFLE_PROOF_CAP};
        use crate::protocol::messages::EventType;
        let biggest = [
            EventType::HandInit,
            EventType::DeckInit,
            EventType::ShuffleStep,
            EventType::ShuffleProof,
            EventType::DeckCommit,
            EventType::DealPrivate,
            EventType::ActionRaise,
            EventType::BoardReveal,
            EventType::ShowdownReveal,
            EventType::ShowdownMuck,
            EventType::HandComplete,
        ]
        .into_iter()
        .map(frame_ceiling)
        .max()
        .unwrap();
        assert_eq!(biggest, SHUFFLE_PROOF_CAP + ENVELOPE_MAX, "the largest chain frame is a shuffle proof");
        let pair = frames(&[biggest, biggest]);
        let v = CheatVote { subject_seat: 7, anchor: [1; 32], cause: CAUSE_EQUIVOCATION, evidence: pair };
        let bytes = crate::protocol::serialization::to_canonical(&v).expect("encodes");
        assert!(bytes.len() <= CHEAT_PAIR_VOTE_CAP, "{} bytes", bytes.len());
        assert_eq!(frame_ceiling(EventType::CheatVote), CHEAT_PAIR_VOTE_CAP + ENVELOPE_MAX);
        assert!(FRAME_OPEN_MAX >= frame_ceiling(EventType::CheatVote));
        assert!(CHEAT_PAIR_VOTE_CAP + ENVELOPE_MAX <= crate::table::fragment::MAX_MESSAGE);
        assert!(CHEAT_PAIR_VOTE_CAP + ENVELOPE_MAX <= crate::protocol::constants::GOSSIP_MAX_TRANSMIT);
    }

    #[test]
    fn the_band_knows_six_causes() {
        for c in [CAUSE_REVEAL, CAUSE_SHUFFLE, CAUSE_KEY, CAUSE_ACTION, CAUSE_MONEY, CAUSE_EQUIVOCATION] {
            assert!(cause_known(c));
        }
        for c in [0u16, 1, 4, 5, 6, 11] {
            assert!(!cause_known(c), "{c}");
        }
    }

    /// The digest is over the seat and the anchor and nothing else -- two
    /// judges holding two different broken frames name one subject -- and in
    /// its own domain.
    #[test]
    fn the_subject_digest_is_the_seat_and_the_anchor_in_its_own_domain() {
        let base = CheatVote { subject_seat: 2, anchor: [1; 32], cause: 3, evidence: frames(&[40]) };
        let d = base.subject_digest();
        let mut other_frame = base.clone();
        other_frame.evidence = frames(&[41]);
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
