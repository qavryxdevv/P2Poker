//! The shuffle chain: every seated player permutes and re-masks the whole deck
//! in turn, and the cards are dealt from the last deck of the chain.
//!
//! # A valid shuffle proof is evidence of *a* permutation, never of a *random*
//! one (C-11)
//!
//! This is the single most misread property of the construction, so it is
//! written here, where the chain is driven, rather than left to a reader of the
//! paper.
//!
//! Bayer–Groth proves that the output deck is a permutation of the input under
//! *some* re-masking. It says nothing whatever about *which* permutation, and it
//! constrains the re-masking scalars not at all. The identity permutation
//! verifies. `next == prev` verifies. A shuffle that leaves chosen slots in the
//! clear verifies. A shuffle that reuses one scalar for every card verifies, and
//! then an observer recovers the whole permutation from public data.
//!
//! [`structural_check`](super::protocol::structural_check) refuses the
//! degenerate shapes, and it is worth having — but no structural test can
//! establish that a permutation was chosen at random, because a deliberately
//! chosen permutation and a random one are the same object.
//!
//! **Unpredictability comes only from the honest shufflers.** The chain is
//! secure because a player who shuffles honestly makes the outcome unpredictable
//! to everybody else, whatever the others did. That is why every seated player
//! shuffles, and why one honest participant is enough.
//!
//! # Chain discipline (C-6), normative
//!
//! 1. **Every seated player shuffles**, exactly once.
//! 2. **In a fixed order, committed before the chain starts.**
//! 3. **Cards are dealt only from the last deck of the chain** — an
//!    intermediate deck is a deck some players have not yet touched.
//! 4. **A disconnect aborts the hand.** It never truncates the chain, because a
//!    chain missing a link is a chain the remaining players could have chosen
//!    the outcome of together. There is deliberately no method here that drops a
//!    seat from the order.
//! 5. **A shuffle can be neither replayed nor reordered**: step `k` is accepted
//!    only from the seat that owns position `k`, and only once.
//!
//! Rule 1 is checked when the chain is built, rules 3 and 4 by [`Final`] being
//! mintable only from a complete chain, and rules 2 and 5 by
//! [`ShuffleChain::accept_step`].
//!
//! The review's C-3 (the structural check) makes the security argument
//! independent of this discipline, and that is exactly why **both** are
//! required: neither is the other's excuse.

use crate::poker::state::SeatIdx;

use super::protocol::{
    Ciphertext, CtxFields, DeckCrypto, DeckCtx, Final, ProofPosition, ShuffleAdmission,
    VerifyOutcome, Verified,
};

/// The per-hand values every context in the chain shares.
///
/// The chain builds each step's [`DeckCtx`] itself from these, so no caller ever
/// supplies a context: three of §4.5's fields are `[u8; 32]` and would transpose
/// silently, and two of them — the round and the shuffler's key — are precisely
/// what stops a proof being replayed as another seat's.
#[derive(Debug, Clone, Copy)]
pub struct ChainParams {
    pub protocol_version: u16,
    pub table_id: [u8; 32],
    pub session_id: [u8; 32],
    pub hand_id: u64,
}

/// Why a chain could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainError {
    /// A chain needs at least two shufflers.
    TooFewShufflers { n: usize },
    /// More shufflers than the round byte of §4.5 can address.
    ///
    /// `0xFF` is the sentinel meaning "not a shuffle step", so position 255
    /// could not be distinguished from a reveal proof. Unreachable at ten seats
    /// and refused rather than assumed away.
    TooManyShufflers { n: usize },
    /// A seat appears twice in the order, so it would shuffle twice and some
    /// other seat not at all.
    DuplicateShuffler { seat: SeatIdx },
    /// The order and the key list disagree about how many shufflers there are.
    KeysDoNotMatchOrder { order: usize, keys: usize },
}

/// Why a step was refused.
#[derive(Debug, Clone, PartialEq)]
pub enum StepError {
    /// A seat tried to shuffle out of turn — a reorder, or a replay of a step
    /// that has already been taken.
    NotYourTurn { expected: SeatIdx, got: SeatIdx },
    /// The chain is finished or aborted and takes no more steps.
    Closed,
    /// This seat already has a proof at this position.
    ///
    /// Distinct from [`NotYourTurn`](StepError::NotYourTurn) because it is the
    /// duplicate-suppression that keeps a peer from spending every other
    /// client's core on repeated verification, and it must be visible as that.
    AlreadySubmitted,
    /// The step did not verify. Whether this is evidence against the emitter is
    /// the distinction [`VerifyOutcome`] carries.
    Rejected(VerifyOutcome),
}

/// Why a chain was abandoned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortReason {
    /// A shuffler left before taking its step.
    ///
    /// The hand aborts. It does not continue without that seat's shuffle: a
    /// chain missing a link is one the remaining players could have chosen the
    /// outcome of together.
    ShufflerGone { seat: SeatIdx },
    /// A step was refused, so the deck this hand would use does not exist.
    StepRejected { seat: SeatIdx },
    /// The hand ended for a reason outside the chain.
    HandAbandoned,
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Open,
    Complete,
    Aborted(AbortReason),
}

/// One hand's shuffle chain.
#[derive(Debug, Clone)]
pub struct ShuffleChain {
    params: ChainParams,
    /// The committed order. Fixed at construction and never edited.
    order: Vec<SeatIdx>,
    /// Each shuffler's application Ed25519 key, aligned to `order`.
    keys: Vec<[u8; 32]>,
    /// `decks[k]` is the deck after step `k`. The open deck is **not** here:
    /// nobody shuffled it, nobody verified it, and only the library can state
    /// what it is - so a chain that stored it would be carrying a constant it
    /// cannot check. Kept in full because a dispute is settled against the
    /// chain, not against its last link.
    ///
    /// Held as `Verified`, not as bare bytes: the chain is the only thing that
    /// ran the verification, so it is the only thing that can honestly hand one
    /// out - and the next shuffler needs one to shuffle from.
    decks: Vec<Verified<Vec<Ciphertext>>>,
    admission: ShuffleAdmission,
    state: State,
}

impl ShuffleChain {
    /// Open a chain over a committed order.
    ///
    /// `order` is every seated player exactly once. It is taken as given rather
    /// than derived here, because the order is committed before the chain starts
    /// and is part of the hand's accepted content — deriving it again would be a
    /// second normative owner of one concept.
    pub fn open(
        params: ChainParams,
        order: Vec<SeatIdx>,
        keys: Vec<[u8; 32]>,
    ) -> Result<Self, ChainError> {
        if order.len() != keys.len() {
            return Err(ChainError::KeysDoNotMatchOrder {
                order: order.len(),
                keys: keys.len(),
            });
        }
        if order.len() < 2 {
            return Err(ChainError::TooFewShufflers { n: order.len() });
        }
        // The round byte cannot be the sentinel, so position 254 is the last
        // addressable one.
        if order.len() > 255 {
            return Err(ChainError::TooManyShufflers { n: order.len() });
        }
        for (i, &seat) in order.iter().enumerate() {
            if order[..i].contains(&seat) {
                return Err(ChainError::DuplicateShuffler { seat });
            }
        }

        Ok(ShuffleChain {
            params,
            order,
            keys,
            decks: Vec::new(),
            admission: ShuffleAdmission::default(),
            state: State::Open,
        })
    }

    /// How many steps have been taken.
    pub fn steps_taken(&self) -> usize {
        self.decks.len()
    }

    /// How many shufflers the chain has.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// The seat whose turn it is, or `None` if the chain is done or abandoned.
    pub fn whose_turn(&self) -> Option<SeatIdx> {
        if self.state != State::Open {
            return None;
        }
        self.order.get(self.steps_taken()).copied()
    }

    /// The context step `k` must have been made under.
    ///
    /// Built here rather than accepted from a caller: the round and the
    /// shuffler's key are what make a proof untransferable between positions and
    /// between seats, and a caller that could choose them could hand back the
    /// context of the step it wants to impersonate.
    fn ctx_for(&self, k: usize, sequence: u64) -> DeckCtx {
        DeckCtx::build(&CtxFields {
            protocol_version: self.params.protocol_version,
            table_id: self.params.table_id,
            session_id: self.params.session_id,
            hand_id: self.params.hand_id,
            sequence,
            position: ProofPosition::shuffle_step(k as u8)
                .expect("the chain length is bounded below the sentinel by `open`"),
            sender_public_key: self.keys[k],
        })
    }

    /// Take one step of the chain.
    ///
    /// `next` is the deck the shuffler produced; the deck it must have shuffled
    /// *from* is not a parameter, because it is the last link of this chain and
    /// nothing else. A step that verifies against a deck the sender chose is a
    /// step that never joined the chain at all.
    ///
    /// On refusal the chain is **not** advanced and **not** aborted: whether a
    /// refused step ends the hand is the caller's decision, because
    /// [`VerifyOutcome::CouldNotVerify`] is not evidence and must not remove
    /// anybody.
    pub fn accept_step<C: DeckCrypto>(
        &mut self,
        crypto: &C,
        from: SeatIdx,
        next: Vec<Ciphertext>,
        proof: &[u8],
        sequence: u64,
    ) -> Result<(), StepError> {
        let expected = self.whose_turn().ok_or(StepError::Closed)?;
        if from != expected {
            return Err(StepError::NotYourTurn { expected, got: from });
        }

        let k = self.steps_taken();
        // One attempt per seat per position, taken **before** the argument is
        // verified rather than after (C-9). Rejecting a bogus proof costs 10 to
        // 36 ms at every seat, so a peer that could retry would burn most of a
        // core at every other client for the price of one message.
        //
        // The consequence is deliberate and is the reason this is not merged
        // into `whose_turn`: a seat whose proof is refused does not get a second
        // attempt, so it can no longer take its step, so the chain can never
        // complete and the hand aborts. That is C-6 rule 4 — the chain is
        // abandoned, never shortened.
        let seats = u8::try_from(self.order.len()).expect("`open` bounds the chain below 256");
        self.admission
            .admit(from, k as u8, seats)
            .map_err(|_| StepError::AlreadySubmitted)?;

        let ctx = self.ctx_for(k, sequence);
        // The first link shuffles the open deck, which this chain does not hold
        // and could not check; every later one shuffles the chain's own last
        // deck. In neither case is the input something the sender supplied.
        let verified = match self.decks.last() {
            None => crypto.verify_initial_shuffle(&next, proof, &ctx),
            Some(prev) => crypto.verify_shuffle(prev.as_ref(), &next, proof, &ctx),
        }
        .map_err(StepError::Rejected)?;

        self.decks.push(verified);
        if self.steps_taken() == self.order.len() {
            self.state = State::Complete;
        }
        Ok(())
    }

    /// Abandon the chain.
    ///
    /// The only way out other than completion. There is no method that drops a
    /// seat and carries on with a shorter chain, and that absence is the
    /// enforcement of C-6's fourth rule.
    pub fn abort(&mut self, reason: AbortReason) {
        if self.state == State::Open {
            self.state = State::Aborted(reason);
        }
    }

    /// Why the chain was abandoned, if it was.
    pub fn aborted(&self) -> Option<AbortReason> {
        match self.state {
            State::Aborted(r) => Some(r),
            _ => None,
        }
    }

    /// The deck the hand deals from — available only once every seated player
    /// has shuffled.
    ///
    /// This is the only [`Final`] mint in the crate. An intermediate deck is a
    /// deck some players have not yet touched, and `verify_shuffle` mints a
    /// `Verified` for every link, so `Verified` alone must never be enough to
    /// deal from.
    pub fn finish(&self) -> Option<Final<Verified<Vec<Ciphertext>>>> {
        if self.state != State::Complete {
            return None;
        }
        let last = self.decks.last().expect("a complete chain has a last deck");
        Some(Final::new(Verified::new(last.as_ref().clone())))
    }

    /// Every deck the chain produced, in order. A dispute is settled against
    /// the chain, not against its last link.
    pub fn decks(&self) -> &[Verified<Vec<Ciphertext>>] {
        &self.decks
    }

    /// The context the next step must be proved under.
    ///
    /// `None` once the chain is finished or abandoned. `sequence` is the chain
    /// stage index of the event that will carry the proof, which is the caller's
    /// to know and the only part of the context that is.
    ///
    /// This exists because the alternative failed a test the first time it was
    /// written: a shuffler that builds its own context has to reproduce the
    /// round and the shuffler key exactly as [`accept_step`](Self::accept_step)
    /// will derive them, and a mismatch makes an honest peer unable to shuffle
    /// its own turn. It fails closed, which is the good direction, but there is
    /// no reason to have two derivations of one value at all.
    pub fn next_ctx(&self, sequence: u64) -> Option<DeckCtx> {
        let k = self.steps_taken();
        if self.state != State::Open || k >= self.order.len() {
            return None;
        }
        Some(self.ctx_for(k, sequence))
    }

    /// The deck the next shuffler must shuffle from, or `None` before the first
    /// step - where the input is the open deck and only the library knows it.
    ///
    /// This is the only way to a mid-chain [`Verified`] deck, and it is here
    /// rather than on the crypto boundary because the chain is what ran the
    /// verification. A [`Final`] deck is a different thing and is what
    /// [`finish`](Self::finish) returns: reveal tokens are issued against that
    /// and never against an intermediate one (C-6).
    pub fn last_verified(&self) -> Option<&Verified<Vec<Ciphertext>>> {
        self.decks.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mental_poker::protocol::{InvalidReason, Unavailable};

    /// Accepts any argument, so the tests exercise the chain rather than the
    /// cryptography. The structural check still runs — it is inside
    /// `verify_shuffle` and an implementor cannot reach past it.
    struct ArgumentAccepts;
    impl DeckCrypto for ArgumentAccepts {
        const DECK_LEN: usize = 4;
        fn verify_initial_argument(
            &self,
            _next: &[Ciphertext],
            _proof: &[u8],
            _ctx: &DeckCtx,
        ) -> Result<(), VerifyOutcome> {
            Ok(())
        }
        fn verify_argument(
            &self,
            _prev: &[Ciphertext],
            _next: &[Ciphertext],
            _proof: &[u8],
            _ctx: &DeckCtx,
        ) -> Result<(), VerifyOutcome> {
            Ok(())
        }
    }

    /// Refuses every argument, for the paths that must survive a rejection.
    struct ArgumentRejects;
    impl DeckCrypto for ArgumentRejects {
        const DECK_LEN: usize = 4;
        fn verify_initial_argument(
            &self,
            _next: &[Ciphertext],
            _proof: &[u8],
            _ctx: &DeckCtx,
        ) -> Result<(), VerifyOutcome> {
            Err(VerifyOutcome::Invalid(InvalidReason::ArgumentFailed))
        }
        fn verify_argument(
            &self,
            _prev: &[Ciphertext],
            _next: &[Ciphertext],
            _proof: &[u8],
            _ctx: &DeckCtx,
        ) -> Result<(), VerifyOutcome> {
            Err(VerifyOutcome::Invalid(InvalidReason::ArgumentFailed))
        }
    }

    /// Cannot answer, which is never evidence against anyone.
    struct CannotVerify;
    impl DeckCrypto for CannotVerify {
        const DECK_LEN: usize = 4;
        fn verify_initial_argument(
            &self,
            _next: &[Ciphertext],
            _proof: &[u8],
            _ctx: &DeckCtx,
        ) -> Result<(), VerifyOutcome> {
            Err(VerifyOutcome::CouldNotVerify(
                Unavailable::AggregateKeyUnknown,
            ))
        }
        fn verify_argument(
            &self,
            _prev: &[Ciphertext],
            _next: &[Ciphertext],
            _proof: &[u8],
            _ctx: &DeckCtx,
        ) -> Result<(), VerifyOutcome> {
            Err(VerifyOutcome::CouldNotVerify(Unavailable::AggregateKeyUnknown))
        }
    }

    fn ct(seed: u8) -> Ciphertext {
        let mut c = [0u8; 66];
        c[0] = 0x02;
        c[1] = seed;
        c[33] = 0x03;
        c[34] = seed ^ 0x5A;
        c
    }

    /// Each round produces a deck sharing nothing with any earlier one, which is
    /// what an honest re-masking does.
    fn deck_for_round(round: u8) -> Vec<Ciphertext> {
        (0..4u8).map(|i| ct(round * 16 + i + 1)).collect()
    }

    fn params() -> ChainParams {
        ChainParams {
            protocol_version: 1,
            table_id: [1u8; 32],
            session_id: [2u8; 32],
            hand_id: 5,
        }
    }

    fn chain(order: Vec<SeatIdx>) -> ShuffleChain {
        let keys: Vec<[u8; 32]> = order.iter().map(|&s| [s; 32]).collect();
        ShuffleChain::open(params(), order, keys).unwrap()
    }

    #[test]
    fn a_complete_chain_deals_from_its_last_deck() {
        let mut c = chain(vec![0, 1, 2]);
        assert_eq!(c.whose_turn(), Some(0));

        for (k, &seat) in [0u8, 1, 2].iter().enumerate() {
            assert_eq!(c.whose_turn(), Some(seat));
            c.accept_step(
                &ArgumentAccepts,
                seat,
                deck_for_round(k as u8 + 1),
                b"proof",
                k as u64,
            )
            .unwrap();
        }

        assert_eq!(c.whose_turn(), None);
        let final_deck = c.finish().expect("every seat shuffled");
        assert_eq!(final_deck.as_ref().as_ref(), &deck_for_round(3));
        assert_eq!(c.decks().len(), 3, "one deck per step; the open deck is not one");
    }

    /// Rule 3: an intermediate deck is a deck some players have not touched, and
    /// `verify_shuffle` mints a `Verified` for every one of them.
    #[test]
    fn an_incomplete_chain_yields_no_deck_to_deal_from() {
        let mut c = chain(vec![0, 1, 2]);
        assert!(c.finish().is_none(), "nothing has been shuffled");

        c.accept_step(&ArgumentAccepts, 0, deck_for_round(1), b"p", 0)
            .unwrap();
        assert!(c.finish().is_none());
        c.accept_step(&ArgumentAccepts, 1, deck_for_round(2), b"p", 1)
            .unwrap();
        assert!(
            c.finish().is_none(),
            "one seat has still not touched this deck"
        );
    }

    /// Rule 5, reorder half: seat 2 cannot go first, however valid its proof.
    #[test]
    fn a_seat_cannot_shuffle_out_of_turn() {
        let mut c = chain(vec![0, 1, 2]);
        assert_eq!(
            c.accept_step(&ArgumentAccepts, 2, deck_for_round(1), b"p", 0),
            Err(StepError::NotYourTurn {
                expected: 0,
                got: 2
            })
        );
        assert_eq!(c.steps_taken(), 0, "a refused step does not advance");
    }

    /// Rule 5, replay half: having shuffled at position 0, seat 0 cannot shuffle
    /// again — and the refusal is `NotYourTurn`, because after its step the turn
    /// belongs to somebody else.
    #[test]
    fn a_step_cannot_be_replayed() {
        let mut c = chain(vec![0, 1, 2]);
        c.accept_step(&ArgumentAccepts, 0, deck_for_round(1), b"p", 0)
            .unwrap();
        assert_eq!(
            c.accept_step(&ArgumentAccepts, 0, deck_for_round(9), b"p", 1),
            Err(StepError::NotYourTurn {
                expected: 1,
                got: 0
            })
        );
    }

    /// Rule 4: the chain is abandoned rather than shortened. There is no method
    /// that would shorten it, and a shortened chain could not produce a deck in
    /// any case.
    #[test]
    fn a_disconnect_abandons_the_chain_and_never_truncates_it() {
        let mut c = chain(vec![0, 1, 2]);
        c.accept_step(&ArgumentAccepts, 0, deck_for_round(1), b"p", 0)
            .unwrap();

        c.abort(AbortReason::ShufflerGone { seat: 1 });
        assert_eq!(c.aborted(), Some(AbortReason::ShufflerGone { seat: 1 }));
        assert_eq!(c.whose_turn(), None);
        assert!(c.finish().is_none());
        assert_eq!(
            c.accept_step(&ArgumentAccepts, 2, deck_for_round(2), b"p", 1),
            Err(StepError::Closed),
            "seat 2 does not get to inherit seat 1 position"
        );
    }

    /// A completed chain takes no further step, so a late proof cannot replace
    /// the deck the hand is already dealing from.
    #[test]
    fn a_finished_chain_is_closed() {
        let mut c = chain(vec![0, 1]);
        c.accept_step(&ArgumentAccepts, 0, deck_for_round(1), b"p", 0)
            .unwrap();
        c.accept_step(&ArgumentAccepts, 1, deck_for_round(2), b"p", 1)
            .unwrap();
        assert!(c.finish().is_some());
        assert_eq!(
            c.accept_step(&ArgumentAccepts, 0, deck_for_round(3), b"p", 2),
            Err(StepError::Closed)
        );
        // And the deck did not move under the hand.
        assert_eq!(c.finish().unwrap().as_ref().as_ref(), &deck_for_round(2));
    }

    /// The step builds on the chain's own last deck, never on one the sender
    /// supplied — so a deck shuffled from somewhere else does not join.
    #[test]
    fn the_previous_deck_is_the_chains_and_not_the_senders() {
        let mut c = chain(vec![0, 1]);
        c.accept_step(&ArgumentAccepts, 0, deck_for_round(1), b"p", 0)
            .unwrap();

        // Seat 1 offers a deck that is a re-mask of the *initial* deck, ignoring
        // seat 0's work. It shares no coordinate with round 1, so it is
        // structurally fine against round 0 and must still be judged against
        // round 1 — which it is, and here it passes, because a re-mask of the
        // initial deck is also a valid-looking successor of round 1. The point
        // of the test is where `prev` came from, so the check is on the recorded
        // chain rather than on the verdict.
        c.accept_step(&ArgumentAccepts, 1, deck_for_round(2), b"p", 1)
            .unwrap();
        assert_eq!(c.decks()[0].as_ref(), &deck_for_round(1));
        assert_eq!(c.decks()[1].as_ref(), &deck_for_round(2));
    }

    /// The structural check runs inside `verify_shuffle`, so a chain step gets
    /// it too: a seat that returns the deck it was given does not advance the
    /// chain even though the argument accepts.
    #[test]
    fn a_seat_that_does_not_shuffle_does_not_advance_the_chain() {
        let mut c = chain(vec![0, 1]);
        // Returning the open deck at step 0 is fifty-two cards face up rather
        // than an unchanged deck - see `structural_check_initial`.
        let open: Vec<Ciphertext> = (0..4)
            .map(|_| {
                let mut c = [0u8; 66];
                c[32] = 0x40; // the identity, as tests/deck_constants.rs measures it
                c
            })
            .collect();
        assert_eq!(
            c.accept_step(&ArgumentAccepts, 0, open, b"p", 0),
            Err(StepError::Rejected(VerifyOutcome::Invalid(
                InvalidReason::IdentityCiphertext { position: 0 }
            )))
        );
        assert_eq!(c.steps_taken(), 0);
        assert_eq!(c.whose_turn(), Some(0), "seat 0 still owes a shuffle");
    }

    /// A rejected step leaves the chain open, because whether it ends the hand
    /// is the caller's decision — and `CouldNotVerify` must never remove anyone.
    #[test]
    fn a_rejection_does_not_abort_by_itself() {
        let mut c = chain(vec![0, 1]);
        assert!(matches!(
            c.accept_step(&ArgumentRejects, 0, deck_for_round(1), b"p", 0),
            Err(StepError::Rejected(VerifyOutcome::Invalid(_)))
        ));
        assert_eq!(c.aborted(), None);
        assert_eq!(c.whose_turn(), Some(0));

        let mut c = chain(vec![0, 1]);
        assert!(matches!(
            c.accept_step(&CannotVerify, 0, deck_for_round(1), b"p", 0),
            Err(StepError::Rejected(VerifyOutcome::CouldNotVerify(_)))
        ));
        assert_eq!(c.aborted(), None, "an unanswerable check is not evidence");
    }


    /// C-9, and the reason the admission is taken before verification rather
    /// than after: one attempt per seat per position. A peer that could retry
    /// would spend 10 to 36 ms of every other client per message.
    #[test]
    fn a_seat_whose_proof_was_refused_does_not_get_a_second_attempt() {
        let mut c = chain(vec![0, 1]);
        assert!(matches!(
            c.accept_step(&ArgumentRejects, 0, deck_for_round(1), b"bad", 0),
            Err(StepError::Rejected(_))
        ));

        // Same seat, same position, a proof that would verify. Too late.
        assert_eq!(
            c.accept_step(&ArgumentAccepts, 0, deck_for_round(1), b"good", 1),
            Err(StepError::AlreadySubmitted)
        );

        // So the chain can never complete, and the hand aborts rather than
        // carrying on without seat 0.
        assert_eq!(c.whose_turn(), Some(0));
        assert!(c.finish().is_none());
        c.abort(AbortReason::StepRejected { seat: 0 });
        assert_eq!(c.aborted(), Some(AbortReason::StepRejected { seat: 0 }));
        assert!(c.finish().is_none());
    }


    /// One owner for the step context: what the chain hands out is what the
    /// chain checks against.
    #[test]
    fn the_context_a_shuffler_is_given_is_the_one_it_is_judged_by() {
        let mut c = chain(vec![0, 1]);
        assert_eq!(c.next_ctx(7), Some(c.ctx_for(0, 7)));

        c.accept_step(&ArgumentAccepts, 0, deck_for_round(1), b"p", 0)
            .unwrap();
        assert_eq!(c.next_ctx(8), Some(c.ctx_for(1, 8)));
        assert_ne!(c.next_ctx(8), Some(c.ctx_for(0, 8)), "the round moved");

        c.accept_step(&ArgumentAccepts, 1, deck_for_round(2), b"p", 1)
            .unwrap();
        assert_eq!(c.next_ctx(9), None, "a finished chain has no next step");

        let mut c = chain(vec![0, 1]);
        c.abort(AbortReason::HandAbandoned);
        assert_eq!(c.next_ctx(0), None, "and neither has an abandoned one");
    }

    /// Rule 1: one seat cannot occupy two positions and so shuffle twice while
    /// another seat never shuffles at all.
    #[test]
    fn a_seat_cannot_appear_twice_in_the_order() {
        let keys = vec![[0u8; 32]; 3];
        assert_eq!(
            ShuffleChain::open(params(), vec![0, 1, 0], keys).err(),
            Some(ChainError::DuplicateShuffler { seat: 0 })
        );
    }

    #[test]
    fn a_chain_needs_two_shufflers() {
        assert_eq!(
            ShuffleChain::open(params(), vec![0], vec![[0u8; 32]]).err(),
            Some(ChainError::TooFewShufflers { n: 1 })
        );
        assert_eq!(
            ShuffleChain::open(params(), vec![], vec![]).err(),
            Some(ChainError::TooFewShufflers { n: 0 })
        );
    }

    /// The order and the keys are two lists that must agree, and if they did not
    /// the chain would build every context with the wrong shuffler's key —
    /// which fails closed but fails at every peer for a reason nobody could see.
    #[test]
    fn the_order_and_the_keys_must_agree() {
        assert_eq!(
            ShuffleChain::open(params(), vec![0, 1, 2], vec![[0u8; 32]; 2]).err(),
            Some(ChainError::KeysDoNotMatchOrder { order: 3, keys: 2 })
        );
    }

    /// Each step is verified under its own position and its own shuffler's key.
    /// Two positions of one chain never share a context, which is what makes a
    /// captured proof useless one seat over.
    #[test]
    fn every_position_has_its_own_context() {
        let c = chain(vec![0, 1, 2]);
        let a = c.ctx_for(0, 7);
        let b = c.ctx_for(1, 7);
        assert_ne!(a, b, "same sequence, different position and key");

        // And the same position in another hand is another context again.
        let mut other = params();
        other.hand_id = 6;
        let keys: Vec<[u8; 32]> = vec![[0u8; 32], [1u8; 32], [2u8; 32]];
        let d = ShuffleChain::open(other, vec![0, 1, 2], keys).unwrap();
        assert_ne!(a, d.ctx_for(0, 7));
    }
}
