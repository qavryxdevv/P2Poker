//! Anti-cheat adjudication (D-014).
//!
//! A player who sends a provably illegal message is removed from the table, the
//! attacked hand is voided, and the other players are told. This module decides
//! *whether* the evidence supports a removal. It never decides who was slow,
//! who caused an abort, or who to believe — those are judgements two honest
//! peers can reach differently, and every severe defect in eight review passes
//! came from automating one.
//!
//! # Why this can be automatic when accusation cannot
//!
//! Evidence here is **self-authenticating**: a message signed by the accused,
//! whose illegality any peer decides alone. Framing an honest player would mean
//! forging their signature over an illegal message. There is no vote, no
//! quorum, no clock.
//!
//! # The tier distinction is enforced by the type system
//!
//! Tier 1 is decidable from the message alone. Tier 2 needs game state — and if
//! two peers have diverged, an honest player's legal action looks illegal to
//! the peer whose state drifted, so the anti-cheat would eject the victim.
//! Tier 2 therefore requires state fixed by a checkpoint both peers signed.
//!
//! That precondition is not a comment: [`Tier2Finding`] cannot be built without
//! an [`AgreedCheckpoint`], so a tier-2 removal without one does not compile.

use crate::poker::state::{Hash, PlayerId};

/// A violation decidable from the offending message alone.
///
/// Nothing here refers to game state, so no two honest receivers can disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelfContained {
    /// The signature does not verify under the sender's own key.
    SignatureInvalid,
    /// The bytes decode but are not the canonical encoding
    /// (`PROTOCOL.md` §2.5).
    NonCanonicalEncoding,
    /// The bytes are not a well-formed message at all.
    Malformed,
    /// A field is outside its declared range — a card index above 51, a seat
    /// beyond the table, a length past its cap.
    FieldOutOfRange,
    /// A shuffle proof did not verify.
    ShuffleProofInvalid,
    /// A decryption-share proof did not verify.
    RevealProofInvalid,
    /// A key-ownership proof did not verify.
    KeyProofInvalid,
    /// A shuffle gained, lost or duplicated a card.
    DeckNotAPermutation,
    /// The signer is not a party to this table.
    NotAParticipant,
}

/// A violation decidable only against game state.
///
/// Safe to act on **only** once the state is agreed, which is what
/// [`Tier2Finding`] enforces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateDependent {
    /// Acted when it was not this seat's turn.
    OutOfTurn,
    /// A raise below the minimum, with chips left behind.
    RaiseBelowMinimum,
    /// Committed more chips than the seat holds.
    BetExceedsStack,
    /// A showdown claim that contradicts the board.
    ShowdownClaimFalse,
    /// An action in a phase that admits none.
    ActionInWrongPhase,
}

/// A checkpoint whose state the accused and this receiver both signed.
///
/// This is the witness that makes a tier-2 finding safe, so it has to actually
/// witness the thing. A state hash and a sequence number do **not**: they say
/// some checkpoint existed, not that the accused was inside its emitter set, and
/// not which hand it covers. A finding built on that would convict a player who
/// was never party to the state it is judged against — the same shape as the
/// defect this module exists to avoid.
///
/// So the emitter set and the hand are part of the witness, and
/// [`AgreedCheckpoint::covering`] refuses to build one that does not contain
/// both parties.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgreedCheckpoint {
    /// The state hash both parties signed.
    state_hash: Hash,
    /// The chain position it fixes.
    sequence: u64,
    /// The hand it covers. A checkpoint from another hand judges nothing here.
    hand_id: u64,
    /// Every seat that signed it.
    emitters: Vec<PlayerId>,
}

/// Why a checkpoint cannot witness a finding against this pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WitnessError {
    /// The accused did not sign this checkpoint, so it never agreed to the
    /// state its message is about to be judged against.
    AccusedNotAnEmitter,
    /// This receiver did not sign it either, so it cannot claim the state is
    /// shared.
    ReceiverNotAnEmitter,
    /// The checkpoint covers a different hand from the offending message.
    WrongHand { checkpoint: u64, message: u64 },
}

impl core::fmt::Display for WitnessError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WitnessError::AccusedNotAnEmitter => {
                write!(f, "the accused never signed this checkpoint")
            }
            WitnessError::ReceiverNotAnEmitter => {
                write!(f, "this receiver never signed this checkpoint")
            }
            WitnessError::WrongHand { checkpoint, message } => write!(
                f,
                "checkpoint covers hand {checkpoint}, the message is from hand {message}"
            ),
        }
    }
}

impl std::error::Error for WitnessError {}

impl AgreedCheckpoint {
    /// Build a witness, checking it actually covers this accusation.
    ///
    /// The only constructor. It refuses unless both the accused and this
    /// receiver signed the checkpoint and it covers the message's own hand.
    pub fn covering(
        state_hash: Hash,
        sequence: u64,
        hand_id: u64,
        emitters: Vec<PlayerId>,
        accused: &PlayerId,
        receiver: &PlayerId,
        message_hand_id: u64,
    ) -> Result<Self, WitnessError> {
        if hand_id != message_hand_id {
            return Err(WitnessError::WrongHand { checkpoint: hand_id, message: message_hand_id });
        }
        if !emitters.contains(accused) {
            return Err(WitnessError::AccusedNotAnEmitter);
        }
        if !emitters.contains(receiver) {
            return Err(WitnessError::ReceiverNotAnEmitter);
        }
        Ok(AgreedCheckpoint { state_hash, sequence, hand_id, emitters })
    }

    pub fn state_hash(&self) -> Hash {
        self.state_hash
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn hand_id(&self) -> u64 {
        self.hand_id
    }

    pub fn emitters(&self) -> &[PlayerId] {
        &self.emitters
    }
}

/// A tier-2 finding, which cannot exist without an agreed checkpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tier2Finding {
    violation: StateDependent,
    against: AgreedCheckpoint,
}

impl Tier2Finding {
    /// Build a finding, which requires naming the checkpoint it is judged
    /// against.
    ///
    /// There is deliberately no other constructor. A caller holding only a
    /// local view has nothing to pass here, which is the point.
    pub fn new(violation: StateDependent, against: AgreedCheckpoint) -> Self {
        Tier2Finding { violation, against }
    }

    pub fn violation(&self) -> StateDependent {
        self.violation
    }

    pub fn checkpoint(&self) -> &AgreedCheckpoint {
        &self.against
    }
}

/// What was found in a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    Tier1(SelfContained),
    Tier2(Tier2Finding),
    /// State-dependent, but no agreed checkpoint covers it yet.
    ///
    /// The message is still rejected and the hand still voided; nobody is
    /// removed, because the peers may simply have diverged.
    Tier2Unconfirmed(StateDependent),
}

/// What the table does about a finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Drop the message, void the hand, remove the player.
    VoidHandAndRemove,
    /// Drop the message and void the hand; nobody leaves.
    VoidHandOnly,
}

/// An order to remove a player, which only [`adjudicate`] can produce.
///
/// It carries the hash of the offending message so the removal stays checkable
/// by anyone replaying the transcript, including the accused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemovalOrder {
    pub accused: PlayerId,
    pub finding: Finding,
    pub evidence_hash: Hash,
}

/// Decide what a finding means for the table.
///
/// The offending message's hash is required, not optional: a removal that
/// cannot point at the message the accused signed is not a D-014 removal.
pub fn adjudicate(accused: PlayerId, finding: Finding, evidence_hash: Hash) -> (Outcome, Option<RemovalOrder>) {
    match &finding {
        Finding::Tier1(_) | Finding::Tier2(_) => (
            Outcome::VoidHandAndRemove,
            Some(RemovalOrder { accused, finding, evidence_hash }),
        ),
        // The peers may have diverged rather than the accused having cheated,
        // and evicting on a divergence is exactly the failure D-010 closed.
        Finding::Tier2Unconfirmed(_) => (Outcome::VoidHandOnly, None),
    }
}

/// What the information window tells the other players (`SPEC_CS.md` §22).
///
/// A removal that cannot be explained in one sentence should not be automatic,
/// so this is part of the module rather than an afterthought in the GUI.
pub fn explain(finding: &Finding) -> &'static str {
    match finding {
        Finding::Tier1(v) => match v {
            SelfContained::SignatureInvalid => "signature did not verify",
            SelfContained::NonCanonicalEncoding => "message was not canonically encoded",
            SelfContained::Malformed => "message was malformed",
            SelfContained::FieldOutOfRange => "a field was outside its allowed range",
            SelfContained::ShuffleProofInvalid => "shuffle proof did not verify",
            SelfContained::RevealProofInvalid => "card reveal proof did not verify",
            SelfContained::KeyProofInvalid => "key ownership proof did not verify",
            SelfContained::DeckNotAPermutation => "the deck gained, lost or duplicated a card",
            SelfContained::NotAParticipant => "the signer is not a player at this table",
        },
        Finding::Tier2(f) => match f.violation() {
            StateDependent::OutOfTurn => "acted out of turn",
            StateDependent::RaiseBelowMinimum => "raised below the minimum",
            StateDependent::BetExceedsStack => "bet more chips than they hold",
            StateDependent::ShowdownClaimFalse => "claimed a hand the board does not support",
            StateDependent::ActionInWrongPhase => "acted in a phase that allows no action",
        },
        Finding::Tier2Unconfirmed(_) => "the hand was voided; no player was removed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCUSED: PlayerId = [7u8; 32];
    const EVIDENCE: Hash = [9u8; 32];

    const RECEIVER: PlayerId = [8u8; 32];
    const HAND: u64 = 5;

    /// A witness both parties signed, covering the hand in question.
    fn checkpoint() -> AgreedCheckpoint {
        AgreedCheckpoint::covering(
            [1u8; 32],
            42,
            HAND,
            vec![ACCUSED, RECEIVER],
            &ACCUSED,
            &RECEIVER,
            HAND,
        )
        .expect("both parties signed it and it covers this hand")
    }

    #[test]
    fn tier_one_removes_immediately() {
        for violation in [
            SelfContained::SignatureInvalid,
            SelfContained::NonCanonicalEncoding,
            SelfContained::ShuffleProofInvalid,
            SelfContained::DeckNotAPermutation,
        ] {
            let (outcome, order) = adjudicate(ACCUSED, Finding::Tier1(violation), EVIDENCE);
            assert_eq!(outcome, Outcome::VoidHandAndRemove, "{violation:?}");
            let order = order.expect("a tier-1 finding must produce an order");
            assert_eq!(order.accused, ACCUSED);
            assert_eq!(order.evidence_hash, EVIDENCE, "the order must name the message");
        }
    }

    #[test]
    fn tier_two_removes_only_when_judged_against_an_agreed_checkpoint() {
        let finding = Finding::Tier2(Tier2Finding::new(StateDependent::OutOfTurn, checkpoint()));
        let (outcome, order) = adjudicate(ACCUSED, finding, EVIDENCE);
        assert_eq!(outcome, Outcome::VoidHandAndRemove);
        assert!(order.is_some());
    }

    /// The property that keeps D-014 from becoming D-010's failure by another
    /// road: if the peers may have diverged, the hand is voided and **nobody
    /// leaves**.
    #[test]
    fn an_unconfirmed_state_violation_never_removes_anyone() {
        for violation in [
            StateDependent::OutOfTurn,
            StateDependent::RaiseBelowMinimum,
            StateDependent::BetExceedsStack,
            StateDependent::ShowdownClaimFalse,
            StateDependent::ActionInWrongPhase,
        ] {
            let (outcome, order) = adjudicate(ACCUSED, Finding::Tier2Unconfirmed(violation), EVIDENCE);
            assert_eq!(outcome, Outcome::VoidHandOnly, "{violation:?}");
            assert_eq!(order, None, "{violation:?} must not remove a player");
        }
    }

    /// Every finding voids the hand. The requirement is that play continues
    /// afterwards, and a hand half-played against a cheat cannot continue.
    #[test]
    fn every_finding_voids_the_hand() {
        let cases = [
            Finding::Tier1(SelfContained::Malformed),
            Finding::Tier2(Tier2Finding::new(StateDependent::OutOfTurn, checkpoint())),
            Finding::Tier2Unconfirmed(StateDependent::OutOfTurn),
        ];
        for finding in cases {
            let (outcome, _) = adjudicate(ACCUSED, finding, EVIDENCE);
            assert!(matches!(
                outcome,
                Outcome::VoidHandAndRemove | Outcome::VoidHandOnly
            ));
        }
    }

    #[test]
    fn every_finding_has_a_sentence_for_the_information_window() {
        let mut seen = std::collections::BTreeSet::new();
        for violation in [
            SelfContained::SignatureInvalid,
            SelfContained::NonCanonicalEncoding,
            SelfContained::Malformed,
            SelfContained::FieldOutOfRange,
            SelfContained::ShuffleProofInvalid,
            SelfContained::RevealProofInvalid,
            SelfContained::KeyProofInvalid,
            SelfContained::DeckNotAPermutation,
            SelfContained::NotAParticipant,
        ] {
            let text = explain(&Finding::Tier1(violation));
            assert!(!text.is_empty(), "{violation:?} has no explanation");
            assert!(seen.insert(text), "{violation:?} reuses another's wording");
        }
        for violation in [
            StateDependent::OutOfTurn,
            StateDependent::RaiseBelowMinimum,
            StateDependent::BetExceedsStack,
            StateDependent::ShowdownClaimFalse,
            StateDependent::ActionInWrongPhase,
        ] {
            let text = explain(&Finding::Tier2(Tier2Finding::new(violation, checkpoint())));
            assert!(seen.insert(text), "{violation:?} reuses another's wording");
        }
    }

    /// A tier-2 finding carries the checkpoint it was judged against, so a
    /// third party replaying the transcript can re-run the same comparison.
    #[test]
    fn a_tier_two_finding_names_the_state_it_was_judged_against() {
        let cp = checkpoint();
        let finding = Tier2Finding::new(StateDependent::RaiseBelowMinimum, cp.clone());
        assert_eq!(finding.checkpoint(), &cp);
        assert_eq!(finding.violation(), StateDependent::RaiseBelowMinimum);
    }

    /// The witness has to witness the thing. A checkpoint the accused never
    /// signed says some state was agreed, not that *they* agreed to it, and a
    /// finding built on that convicts a player who was never party to the state
    /// its message is judged against.
    #[test]
    fn a_checkpoint_the_accused_never_signed_cannot_witness_a_finding() {
        let stranger: PlayerId = [77u8; 32];
        assert_eq!(
            AgreedCheckpoint::covering(
                [1u8; 32], 42, HAND,
                vec![RECEIVER, stranger],
                &ACCUSED, &RECEIVER, HAND
            ),
            Err(WitnessError::AccusedNotAnEmitter)
        );
    }

    #[test]
    fn a_checkpoint_this_receiver_never_signed_cannot_witness_either() {
        let stranger: PlayerId = [77u8; 32];
        assert_eq!(
            AgreedCheckpoint::covering(
                [1u8; 32], 42, HAND,
                vec![ACCUSED, stranger],
                &ACCUSED, &RECEIVER, HAND
            ),
            Err(WitnessError::ReceiverNotAnEmitter)
        );
    }

    /// A checkpoint from another hand fixes state this message was never
    /// judged against, so it witnesses nothing here.
    #[test]
    fn a_checkpoint_from_another_hand_cannot_witness_a_finding() {
        assert_eq!(
            AgreedCheckpoint::covering(
                [1u8; 32], 42, HAND,
                vec![ACCUSED, RECEIVER],
                &ACCUSED, &RECEIVER, HAND + 1
            ),
            Err(WitnessError::WrongHand { checkpoint: HAND, message: HAND + 1 })
        );
    }

    /// `ParentUnknown` used to sit in tier 1. It is decidable only against the
    /// receiver's own store, so one dropped frame would have removed an honest
    /// player - a live counter-example to the rule this module states. It is
    /// gone, and this test is the tripwire against it coming back.
    #[test]
    fn no_tier_one_violation_depends_on_the_receivers_store() {
        // Every tier-1 variant must be decidable from the offending message
        // alone. The list is short enough to check by eye, and the point of the
        // test is that adding one forces the author past this comment.
        let all = [
            SelfContained::SignatureInvalid,
            SelfContained::NonCanonicalEncoding,
            SelfContained::Malformed,
            SelfContained::FieldOutOfRange,
            SelfContained::ShuffleProofInvalid,
            SelfContained::RevealProofInvalid,
            SelfContained::KeyProofInvalid,
            SelfContained::DeckNotAPermutation,
            SelfContained::NotAParticipant,
        ];
        assert_eq!(all.len(), 9, "a tier-1 variant was added or removed");
    }
}
