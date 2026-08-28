//! Actions and the legal-action computation
//! (`docs/research/POKER_RULES.md` A3 to A6).
//!
//! `SPEC_CS.md` section 11 requires every incoming action to be re-validated
//! locally: a peer is never trusted to send a legal one. So this module is
//! both what the GUI asks "what may I do", and what the validator asks "was
//! that allowed" — one function, so the two can never disagree.
//!
//! Amounts are always the player's **total commitment for the round**, never an
//! increment. TDA 43-B: "Without other clarifying information, declaring raise
//! and an amount is the total bet." Carrying a total on the wire removes a
//! whole class of ambiguity.

use crate::poker::state::{Chips, SeatIdx};

/// What a player did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Fold,
    Check,
    Call,
    /// Open the betting. The amount is the total commitment for the round.
    Bet(Chips),
    /// Raise. The amount is the total commitment for the round, not the
    /// increment.
    Raise(Chips),
}

/// Why an action was rejected.
///
/// A rejected action is a protocol violation attributable to whoever signed it,
/// not a state transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Illegal {
    /// The seat cannot act: it has folded, is all-in, or does not exist.
    NotToAct,
    /// Checked while owing chips.
    CheckFacingBet,
    /// Called with nothing to call.
    CallNothingOwed,
    /// Opened while a bet already stands.
    BetWhenBetStands,
    /// Raised with no bet to raise.
    RaiseWithNoBet,
    /// Raised while holding no raising rights (A5).
    RaiseNotReopened,
    /// Below the minimum, and not an all-in.
    BelowMinimum { minimum: Chips },
    /// More than the seat has.
    AboveStack { maximum: Chips },
}

/// Everything the legal-action computation needs about the current round.
///
/// A player is all-in exactly when `stack[p] == 0`, so that is derived rather
/// than stored — one fewer field that could disagree with itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BettingRound {
    /// The big blind, which is the minimum opening bet and the opening value
    /// of `last_full_raise`.
    pub big_blind: Chips,
    /// The highest commitment anyone has made this round.
    pub current_bet: Chips,
    /// The largest *full* bet or raise increment so far this round.
    ///
    /// Monotonically non-decreasing within a round, so TDA 43-A's "largest
    /// prior full raise" and 47-A's "last full valid raise" coincide and one
    /// variable serves both.
    pub last_full_raise: Chips,
    /// What each seat has committed **this round**.
    pub committed: Vec<Chips>,
    /// What each seat still has behind.
    pub stack: Vec<Chips>,
    /// Whether each seat has voluntarily acted this round. Posting a blind is
    /// not acting.
    pub acted: Vec<bool>,
    pub folded: Vec<bool>,
}

/// What a seat may legally do right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegalActions {
    pub can_fold: bool,
    pub can_check: bool,
    pub can_call: bool,
    /// The total this seat would reach by calling: `current_bet`, or its whole
    /// stack when that is less (an all-in call for less).
    pub call_to: Chips,
    pub can_bet: bool,
    pub can_raise: bool,
    /// The smallest legal total for a bet or raise, ignoring the stack.
    pub min_raise_to: Chips,
    /// The largest legal total: everything this seat has.
    pub max_raise_to: Chips,
}

impl BettingRound {
    fn seats(&self) -> usize {
        self.committed.len()
    }

    fn is_all_in(&self, seat: SeatIdx) -> bool {
        self.stack[seat as usize] == 0
    }

    /// What this seat owes to match the current bet.
    pub fn to_call(&self, seat: SeatIdx) -> Chips {
        self.current_bet.saturating_sub(self.committed[seat as usize])
    }

    /// Whether this seat still holds raising rights (A5, from TDA 47-A).
    ///
    /// The predicate deliberately tracks neither the last aggressor nor a
    /// per-player "amount faced when I last acted". Every completed voluntary
    /// action leaves the actor either matching `current_bet` or all-in and
    /// unable to act again, so `current_bet - committed[p]` *is* the increment
    /// `p` has faced since acting. That handles cumulative short all-ins for
    /// free.
    ///
    /// The `!acted` arm is what preserves the big blind's option and the rights
    /// of anyone yet to speak: posting a blind is not acting.
    pub fn can_reopen(&self, seat: SeatIdx) -> bool {
        !self.acted[seat as usize] || self.to_call(seat) >= self.last_full_raise
    }

    /// The full set of legal actions for a seat.
    pub fn legal(&self, seat: SeatIdx) -> Option<LegalActions> {
        let s = seat as usize;
        if s >= self.seats() || self.folded[s] || self.is_all_in(seat) {
            return None;
        }

        let to_call = self.to_call(seat);
        let stack = self.stack[s];
        let max_raise_to = self.committed[s] + stack;

        let opening = self.current_bet == 0;
        let min_raise_to = if opening {
            self.big_blind
        } else {
            self.current_bet + self.last_full_raise
        };

        // All-in is always available while chips remain, even below the
        // minimum. Otherwise the total must reach the minimum.
        let aggression_possible = max_raise_to > self.current_bet;
        let reopened = self.can_reopen(seat);

        Some(LegalActions {
            can_fold: true,
            can_check: to_call == 0,
            can_call: to_call > 0,
            call_to: self.current_bet.min(max_raise_to),
            can_bet: opening && aggression_possible,
            can_raise: !opening && reopened && aggression_possible,
            min_raise_to,
            max_raise_to,
        })
    }

    /// Validate an action and apply it.
    ///
    /// On rejection nothing is mutated, so a caller may treat the round as
    /// untouched and attribute the violation to the signer.
    pub fn apply(&mut self, seat: SeatIdx, action: Action) -> Result<(), Illegal> {
        let legal = self.legal(seat).ok_or(Illegal::NotToAct)?;
        let s = seat as usize;

        match action {
            Action::Fold => {
                self.folded[s] = true;
            }
            Action::Check => {
                if !legal.can_check {
                    return Err(Illegal::CheckFacingBet);
                }
            }
            Action::Call => {
                if !legal.can_call {
                    return Err(Illegal::CallNothingOwed);
                }
                self.move_to(seat, legal.call_to);
            }
            Action::Bet(to) | Action::Raise(to) => {
                let opening = self.current_bet == 0;
                match action {
                    Action::Bet(_) if !opening => return Err(Illegal::BetWhenBetStands),
                    Action::Raise(_) if opening => return Err(Illegal::RaiseWithNoBet),
                    Action::Raise(_) if !legal.can_raise => return Err(Illegal::RaiseNotReopened),
                    _ => {}
                }
                if to > legal.max_raise_to {
                    return Err(Illegal::AboveStack { maximum: legal.max_raise_to });
                }
                let all_in = to == legal.max_raise_to;
                if to < legal.min_raise_to && !all_in {
                    return Err(Illegal::BelowMinimum { minimum: legal.min_raise_to });
                }
                if to <= self.current_bet && !all_in {
                    return Err(Illegal::BelowMinimum { minimum: legal.min_raise_to });
                }

                let increment = to - self.current_bet;
                // A short all-in never moves the yardstick, however many of
                // them stack up. That is the whole of TDA 47-A's second
                // sentence.
                if increment >= self.last_full_raise {
                    self.last_full_raise = increment;
                }
                self.move_to(seat, to);
                self.current_bet = self.current_bet.max(to);
            }
        }

        self.acted[s] = true;
        Ok(())
    }

    /// Move chips so this seat's round commitment becomes `target`.
    fn move_to(&mut self, seat: SeatIdx, target: Chips) {
        let s = seat as usize;
        let delta = target.saturating_sub(self.committed[s]).min(self.stack[s]);
        self.committed[s] += delta;
        self.stack[s] -= delta;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a round with `stacks` behind and nothing committed.
    fn round(big_blind: Chips, stacks: &[Chips]) -> BettingRound {
        BettingRound {
            big_blind,
            current_bet: 0,
            last_full_raise: big_blind,
            committed: vec![0; stacks.len()],
            stack: stacks.to_vec(),
            acted: vec![false; stacks.len()],
            folded: vec![false; stacks.len()],
        }
    }

    /// TDA Illustration Addendum, Rule 47 Example 2, verbatim from the primary
    /// text: "NLHE, Blinds 50-100. Post-flop A opens for 300, B pushes all-in
    /// for 500 total, C goes all-in for 650 total, D goes all-in for 800 total,
    /// E calls 800. What is the min raise for Player F?" Answer: 1100.
    #[test]
    fn tda_47_example_2_short_all_ins_never_move_the_yardstick() {
        // A, B, C, D, E, F
        let mut r = round(100, &[5000, 500, 650, 800, 5000, 5000]);

        r.apply(0, Action::Bet(300)).unwrap();
        assert_eq!(r.last_full_raise, 300, "the opening bet sets the yardstick");

        r.apply(1, Action::Raise(500)).unwrap();
        assert_eq!(r.last_full_raise, 300, "increment 200 < 300");
        r.apply(2, Action::Raise(650)).unwrap();
        assert_eq!(r.last_full_raise, 300, "increment 150 < 300");
        r.apply(3, Action::Raise(800)).unwrap();
        assert_eq!(r.last_full_raise, 300, "increment 150 < 300");
        r.apply(4, Action::Call).unwrap();

        let f = r.legal(5).unwrap();
        assert_eq!(r.current_bet, 800);
        assert_eq!(f.min_raise_to, 1100, "TDA's published answer");
        assert!(f.can_call && f.can_raise);
    }

    /// TDA Rule 47 Example 3-A: a short all-in does not reopen for a player who
    /// has already acted, but the big blind — who has not acted — keeps full
    /// rights.
    #[test]
    fn tda_47_example_3a_short_all_in_does_not_reopen() {
        // A, B, C, SB, BB. Blinds 2000/4000 already posted.
        let mut r = BettingRound {
            big_blind: 4000,
            current_bet: 4000,
            last_full_raise: 4000,
            committed: vec![0, 0, 0, 2000, 4000],
            stack: vec![40_000, 40_000, 7500, 38_000, 56_000],
            acted: vec![false; 5],
            folded: vec![false; 5],
        };

        r.apply(0, Action::Call).unwrap(); // A calls 4000
        r.apply(1, Action::Fold).unwrap();
        r.apply(2, Action::Raise(7500)).unwrap(); // C all-in
        assert_eq!(r.last_full_raise, 4000, "increment 3500 < 4000");
        r.apply(3, Action::Fold).unwrap(); // SB forfeits 2000

        // BB has not acted, so retains full rights via the !acted arm.
        let bb = r.legal(4).unwrap();
        assert!(r.can_reopen(4));
        assert_eq!(bb.min_raise_to, 11_500, "TDA: raise by at least 4000 to 11,500");
        assert_eq!(bb.call_to, 7500);

        // BB smooth-calls. Action returns to A, who has acted and faces 3500.
        r.apply(4, Action::Call).unwrap();
        assert!(!r.can_reopen(0), "3500 is not a full raise");
        let a = r.legal(0).unwrap();
        assert!(!a.can_raise, "TDA: A can only fold or call the 3500");
        assert!(a.can_call && a.can_fold);
        assert_eq!(r.apply(0, Action::Raise(20_000)), Err(Illegal::RaiseNotReopened));
    }

    /// TDA Rule 47 Example 3-B: the same all-in does reopen once a full raise
    /// lands on top of it.
    #[test]
    fn tda_47_example_3b_a_full_raise_reopens() {
        let mut r = BettingRound {
            big_blind: 4000,
            current_bet: 4000,
            last_full_raise: 4000,
            committed: vec![0, 0, 0, 2000, 4000],
            stack: vec![40_000, 40_000, 7500, 38_000, 56_000],
            acted: vec![false; 5],
            folded: vec![false; 5],
        };
        r.apply(0, Action::Call).unwrap();
        r.apply(1, Action::Fold).unwrap();
        r.apply(2, Action::Raise(7500)).unwrap();
        r.apply(3, Action::Fold).unwrap();
        r.apply(4, Action::Raise(11_500)).unwrap(); // BB makes it a full raise

        assert_eq!(r.last_full_raise, 4000, "increment 4000 == yardstick, stays");
        assert!(r.can_reopen(0), "A now faces 7500, more than a full raise");
        let a = r.legal(0).unwrap();
        assert!(a.can_raise);
        assert_eq!(a.min_raise_to, 15_500, "TDA: re-raise to at least 15,500");
    }

    /// TDA Rule 47 Examples 1 and 1-A: cumulative short all-ins reopen for one
    /// player and not for another, in the same round.
    #[test]
    fn tda_47_example_1_cumulative_short_all_ins() {
        // A, B, C, D, E post-flop. Stacks A 5000, B 125, C 5000, D 200, E 5000.
        let mut r = round(100, &[5000, 125, 5000, 200, 5000]);

        r.apply(0, Action::Bet(100)).unwrap();
        assert_eq!(r.last_full_raise, 100);
        r.apply(1, Action::Raise(125)).unwrap(); // all-in, increment 25
        assert_eq!(r.last_full_raise, 100);
        r.apply(2, Action::Call).unwrap();
        r.apply(3, Action::Raise(200)).unwrap(); // all-in, increment 75
        assert_eq!(r.last_full_raise, 100);
        r.apply(4, Action::Call).unwrap();

        // A faces 200 - 100 = 100, exactly a full raise, so it reopens.
        assert!(r.can_reopen(0), "25 + 75 add up to a full raise");
        assert_eq!(r.legal(0).unwrap().min_raise_to, 300);

        // A merely calls. C faces 200 - 125 = 75, which is not a full raise.
        r.apply(0, Action::Call).unwrap();
        assert!(!r.can_reopen(2), "TDA 1-A: 75 is not a full raise for C");
        assert!(!r.legal(2).unwrap().can_raise);
    }

    /// Example 1-B: had A min-raised instead, C would be reopened.
    #[test]
    fn tda_47_example_1b_a_min_raise_reopens_the_short_caller() {
        let mut r = round(100, &[5000, 125, 5000, 200, 5000]);
        r.apply(0, Action::Bet(100)).unwrap();
        r.apply(1, Action::Raise(125)).unwrap();
        r.apply(2, Action::Call).unwrap();
        r.apply(3, Action::Raise(200)).unwrap();
        r.apply(4, Action::Call).unwrap();
        r.apply(0, Action::Raise(300)).unwrap();

        assert!(r.can_reopen(2), "C now faces 300 - 125 = 175 >= 100");
        assert!(r.legal(2).unwrap().can_raise);
    }

    #[test]
    fn an_all_in_call_for_less_takes_the_whole_stack_and_no_more() {
        let mut r = round(100, &[5000, 60]);
        r.apply(0, Action::Bet(500)).unwrap();

        let short = r.legal(1).unwrap();
        assert_eq!(short.call_to, 60, "capped at what the seat has");
        r.apply(1, Action::Call).unwrap();

        assert_eq!(r.committed[1], 60);
        assert_eq!(r.stack[1], 0, "the seat is now all-in");
        assert_eq!(r.current_bet, 500, "a call for less does not lower the bet");
        assert!(r.legal(1).is_none(), "an all-in seat cannot act again");
    }

    /// Over-calling: a player behind a short all-in still owes the full current
    /// bet, not the short amount. This falls out of `to_call` with no special
    /// case, which is why it is worth pinning.
    #[test]
    fn a_player_behind_a_short_all_in_owes_the_full_bet() {
        let mut r = round(100, &[5000, 125, 5000, 200, 5000]);
        r.apply(0, Action::Bet(100)).unwrap();
        r.apply(1, Action::Raise(125)).unwrap();
        r.apply(2, Action::Call).unwrap();
        r.apply(3, Action::Raise(200)).unwrap();

        assert_eq!(r.to_call(4), 200, "E owes 200, not B's 125");
        assert_eq!(r.legal(4).unwrap().call_to, 200);
    }

    #[test]
    fn illegal_actions_are_rejected_and_change_nothing() {
        let mut r = round(100, &[1000, 1000]);
        let before = r.clone();

        assert_eq!(r.apply(0, Action::Call), Err(Illegal::CallNothingOwed));
        assert_eq!(r.apply(0, Action::Raise(300)), Err(Illegal::RaiseWithNoBet));
        assert_eq!(
            r.apply(0, Action::Bet(50)),
            Err(Illegal::BelowMinimum { minimum: 100 }),
            "below the big blind and not an all-in"
        );
        assert_eq!(
            r.apply(0, Action::Bet(2000)),
            Err(Illegal::AboveStack { maximum: 1000 })
        );
        assert_eq!(r, before, "a rejected action must not mutate the round");

        r.apply(0, Action::Bet(300)).unwrap();
        assert_eq!(r.apply(1, Action::Check), Err(Illegal::CheckFacingBet));
        assert_eq!(r.apply(1, Action::Bet(600)), Err(Illegal::BetWhenBetStands));
    }

    #[test]
    fn a_short_all_in_bet_below_the_big_blind_is_still_legal() {
        // All-in is always legal while chips remain, even under the minimum.
        let mut r = round(100, &[1000, 40]);
        r.apply(1, Action::Bet(40)).unwrap();
        assert_eq!(r.current_bet, 40);
        assert_eq!(r.stack[1], 0);
    }

    #[test]
    fn chips_are_conserved_by_every_action() {
        let mut r = round(100, &[5000, 125, 5000, 200, 5000]);
        let total: Chips = r.stack.iter().sum::<Chips>() + r.committed.iter().sum::<Chips>();

        for (seat, action) in [
            (0, Action::Bet(100)),
            (1, Action::Raise(125)),
            (2, Action::Call),
            (3, Action::Raise(200)),
            (4, Action::Call),
            (0, Action::Call),
            (2, Action::Call),
        ] {
            r.apply(seat, action).unwrap();
            let now: Chips = r.stack.iter().sum::<Chips>() + r.committed.iter().sum::<Chips>();
            assert_eq!(now, total, "after {action:?} by seat {seat}");
        }
    }
}
