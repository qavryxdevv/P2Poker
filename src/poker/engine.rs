//! Betting-round progression: blinds, action order, and when a round closes
//! (`docs/research/POKER_RULES.md` A1 and A2).
//!
//! Legality lives in [`crate::poker::actions`]; this module answers *whose turn
//! is it* and *is the round over*. Both are pure functions of the round state,
//! with no clock and no randomness, as the determinism contract requires.

use crate::poker::actions::BettingRound;
use crate::poker::state::{Chips, SeatIdx, Street};

/// Which seats were dealt into this hand.
///
/// An absent seat posts its blind as dead money and takes no cards (D-005), so
/// being dealt in is not the same as occupying a seat.
pub type DealtIn = [bool];

/// Seats in clockwise order starting after `from`, wrapping once.
fn ring_after(from: SeatIdx, seat_count: u8) -> impl Iterator<Item = SeatIdx> {
    let n = seat_count as u16;
    (1..=n).map(move |offset| ((from as u16 + offset) % n) as SeatIdx)
}

/// Dealt in and not folded. Includes players who are all-in.
pub fn is_live(round: &BettingRound, dealt_in: &DealtIn, seat: SeatIdx) -> bool {
    let s = seat as usize;
    s < dealt_in.len() && dealt_in[s] && !round.folded[s]
}

/// Live and with chips behind, so still able to make a decision.
pub fn can_act(round: &BettingRound, dealt_in: &DealtIn, seat: SeatIdx) -> bool {
    is_live(round, dealt_in, seat) && round.stack[seat as usize] > 0
}

/// How many seats are still live.
pub fn live_count(round: &BettingRound, dealt_in: &DealtIn) -> usize {
    (0..round.committed.len())
        .filter(|&s| is_live(round, dealt_in, s as SeatIdx))
        .count()
}

/// Post the blinds and open the pre-flop round (A1.1).
///
/// `small` and `big` are the level's nominal amounts. A blind seat with too
/// few chips posts what it has and is all-in immediately, before any voluntary
/// action.
///
/// The rule implementers most often get wrong is step 3: `current_bet` becomes
/// the **nominal** big blind whether or not the big-blind seat could post it in
/// full. If the big blind has 60 chips at 50/100, it posts 60 and is all-in,
/// and an under-the-gun player still has to put in 100 to call. PokerTH does
/// the same, unconditionally
/// (`src/engine/local_engine/localberopreflop.cpp:44`).
///
/// Posting a blind is **not** acting, which is what gives the big blind its
/// option (A2).
pub fn post_blinds(round: &mut BettingRound, sb_seat: SeatIdx, bb_seat: SeatIdx, small: Chips, big: Chips) {
    for (seat, amount) in [(sb_seat, small), (bb_seat, big)] {
        let s = seat as usize;
        let posted = amount.min(round.stack[s]);
        round.committed[s] += posted;
        round.stack[s] -= posted;
    }
    round.current_bet = big;
    round.last_full_raise = big;
    round.acted.iter_mut().for_each(|a| *a = false);
}

/// The seat that opens a betting round (A2).
///
/// Heads-up is an explicit branch, not a special case of the general formula.
/// The two-player post-flop order is the exact inverse of the three-handed one:
/// the button is the small blind, acts **first** pre-flop and **last** on every
/// other street. Writing it as "first live seat clockwise from the button"
/// happens to give the right answer heads-up, and that coincidence is exactly
/// how the bug survives review, so it is spelled out here instead.
/// PokerTH branches explicitly for the same reason
/// (`src/engine/local_engine/localhand.cpp:435-476`).
pub fn first_to_act(
    street: Street,
    round: &BettingRound,
    dealt_in: &DealtIn,
    button: SeatIdx,
    bb_seat: SeatIdx,
    seat_count: u8,
) -> Option<SeatIdx> {
    let heads_up = live_count(round, dealt_in) == 2;

    let start = match (street, heads_up) {
        // The button is the small blind and acts first.
        (Street::PreFlop, true) => return seek(round, dealt_in, button, seat_count, true),
        // Under the gun: the first live seat after the big blind.
        (Street::PreFlop, false) => bb_seat,
        // The big blind opens; the button closes.
        (_, true) => return seek(round, dealt_in, bb_seat, seat_count, true),
        // The first live seat after the button *position*, which may be an
        // empty seat under the dead-button rule.
        (_, false) => button,
    };

    seek(round, dealt_in, start, seat_count, false)
}

/// The next seat that can act, clockwise from `from`.
pub fn next_to_act(
    round: &BettingRound,
    dealt_in: &DealtIn,
    from: SeatIdx,
    seat_count: u8,
) -> Option<SeatIdx> {
    seek(round, dealt_in, from, seat_count, false)
}

/// First seat able to act at or after `start`, wrapping once.
///
/// `include_start` decides whether `start` itself is a candidate.
fn seek(
    round: &BettingRound,
    dealt_in: &DealtIn,
    start: SeatIdx,
    seat_count: u8,
    include_start: bool,
) -> Option<SeatIdx> {
    if include_start && can_act(round, dealt_in, start) {
        return Some(start);
    }
    ring_after(start, seat_count).find(|&seat| can_act(round, dealt_in, seat))
}

/// Whether the betting round is over (A2).
///
/// Both conditions must hold across every live seat that still has chips:
/// it has acted, and it has matched `current_bet`. A seat that is all-in for
/// less is exempt from the second, because it has nothing left to match with.
///
/// The big blind's option falls out of the first condition rather than needing
/// a rule: posting a blind does not set `acted`, so an unraised pot returns to
/// the big blind, who may check or raise. Heads-up the same holds for the small
/// blind, who acts first and whose opponent closes.
pub fn round_complete(round: &BettingRound, dealt_in: &DealtIn) -> bool {
    (0..round.committed.len())
        .map(|s| s as SeatIdx)
        .filter(|&seat| can_act(round, dealt_in, seat))
        .all(|seat| {
            let s = seat as usize;
            round.acted[s] && round.committed[s] == round.current_bet
        })
}

/// Whether any further betting is possible this hand.
///
/// With at most one seat able to act and nothing owed, the remaining streets
/// are still dealt — they decide the pots — but no action happens between them.
/// The hand is not abandoned.
pub fn betting_is_closed(round: &BettingRound, dealt_in: &DealtIn) -> bool {
    let able = (0..round.committed.len())
        .map(|s| s as SeatIdx)
        .filter(|&seat| can_act(round, dealt_in, seat))
        .count();
    able <= 1 && round_complete(round, dealt_in)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poker::actions::Action;

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

    /// A1.1 rule 3, the one implementers get wrong: a big blind who cannot post
    /// in full does not lower the bet.
    #[test]
    fn a_short_big_blind_does_not_lower_the_current_bet() {
        let mut r = round(100, &[5000, 5000, 60]); // seat 2 is the BB with 60
        post_blinds(&mut r, 1, 2, 50, 100);

        assert_eq!(r.committed[2], 60, "posts what it has");
        assert_eq!(r.stack[2], 0, "and is all-in immediately");
        assert_eq!(r.current_bet, 100, "the NOMINAL big blind, not 60");
        assert_eq!(r.last_full_raise, 100);

        // Under the gun must still call 100, and raise to at least 200.
        assert_eq!(r.to_call(0), 100);
        assert_eq!(r.legal(0).unwrap().min_raise_to, 200);
    }

    #[test]
    fn posting_a_blind_is_not_acting() {
        let mut r = round(100, &[5000, 5000, 5000]);
        post_blinds(&mut r, 1, 2, 50, 100);
        assert!(r.acted.iter().all(|&a| !a), "nobody has acted yet");
    }

    /// The big blind's option: an unraised pot comes back to them.
    #[test]
    fn an_unraised_pot_returns_to_the_big_blind() {
        let dealt = [true, true, true];
        let mut r = round(100, &[5000, 5000, 5000]);
        post_blinds(&mut r, 1, 2, 50, 100);

        // Seat 0 is UTG (first after the BB at seat 2, wrapping).
        assert_eq!(first_to_act(Street::PreFlop, &r, &dealt, 0, 2, 3), Some(0));

        r.apply(0, Action::Call).unwrap(); // UTG calls 100
        r.apply(1, Action::Call).unwrap(); // SB completes to 100
        assert!(
            !round_complete(&r, &dealt),
            "the big blind has not acted, so the round is not over"
        );
        assert_eq!(next_to_act(&r, &dealt, 1, 3), Some(2));

        r.apply(2, Action::Check).unwrap();
        assert!(round_complete(&r, &dealt), "now it is");
    }

    /// TDA 34-B. The heads-up order is the inverse of the three-handed one
    /// post-flop, which is the usual source of bugs.
    #[test]
    fn heads_up_the_button_acts_first_preflop_and_last_afterwards() {
        let dealt = [true, true];
        let mut r = round(100, &[5000, 5000]);
        // Seat 0 is the button and therefore the small blind.
        post_blinds(&mut r, 0, 1, 50, 100);

        assert_eq!(
            first_to_act(Street::PreFlop, &r, &dealt, 0, 1, 2),
            Some(0),
            "the button is the small blind and acts first pre-flop"
        );
        for street in [Street::Flop, Street::Turn, Street::River] {
            assert_eq!(
                first_to_act(street, &r, &dealt, 0, 1, 2),
                Some(1),
                "post-flop the big blind opens and the button closes"
            );
        }
    }

    /// Three-handed post-flop the first live seat after the button opens — and
    /// the button position may be an empty seat under the dead-button rule.
    #[test]
    fn three_handed_post_flop_opens_left_of_the_button_position() {
        // Four seats, but seat 1 was not dealt in: the button is dead on it.
        let dealt = [true, false, true, true];
        let r = round(100, &[5000, 0, 5000, 5000]);

        assert_eq!(
            first_to_act(Street::Flop, &r, &dealt, 1, 3, 4),
            Some(2),
            "the dead button position still orders the ring"
        );
    }

    #[test]
    fn action_skips_folded_and_all_in_seats() {
        let dealt = [true, true, true, true];
        let mut r = round(100, &[5000, 200, 5000, 5000]);
        r.apply(0, Action::Bet(300)).unwrap();
        r.apply(1, Action::Call).unwrap(); // all-in for 200
        r.apply(2, Action::Fold).unwrap();

        assert!(!can_act(&r, &dealt, 1), "all-in seats do not act again");
        assert!(!can_act(&r, &dealt, 2), "folded seats do not act again");
        assert_eq!(next_to_act(&r, &dealt, 2, 4), Some(3));
        assert_eq!(next_to_act(&r, &dealt, 3, 4), Some(0), "wraps past both");
    }

    #[test]
    fn a_round_is_over_only_when_everyone_has_acted_and_matched() {
        let dealt = [true, true, true];
        let mut r = round(100, &[5000, 5000, 5000]);

        r.apply(0, Action::Bet(300)).unwrap();
        assert!(!round_complete(&r, &dealt), "two seats have not acted");
        r.apply(1, Action::Call).unwrap();
        assert!(!round_complete(&r, &dealt), "one seat has not acted");
        r.apply(2, Action::Call).unwrap();
        assert!(round_complete(&r, &dealt));

        // A raise reopens: the raiser has acted but the others no longer match.
        let mut r2 = r.clone();
        r2.acted[1] = false;
        assert!(!round_complete(&r2, &dealt));
    }

    #[test]
    fn an_all_in_seat_does_not_hold_the_round_open() {
        let dealt = [true, true];
        let mut r = round(100, &[5000, 200]);
        r.apply(0, Action::Bet(300)).unwrap();
        r.apply(1, Action::Call).unwrap(); // all-in for 200, cannot match 300

        assert!(
            round_complete(&r, &dealt),
            "a seat all-in for less has nothing left to match with"
        );
        assert!(betting_is_closed(&r, &dealt), "only one seat can still act");
    }

    #[test]
    fn betting_is_not_closed_while_someone_still_owes() {
        let dealt = [true, true, true];
        let mut r = round(100, &[5000, 200, 5000]);
        r.apply(0, Action::Bet(300)).unwrap();
        r.apply(1, Action::Call).unwrap(); // all-in for 200

        assert!(
            !betting_is_closed(&r, &dealt),
            "seat 2 has chips and has not acted"
        );
        r.apply(2, Action::Call).unwrap();
        assert!(!betting_is_closed(&r, &dealt), "two seats can still act");
    }

    #[test]
    fn first_to_act_returns_none_when_nobody_can_act() {
        let dealt = [true, true];
        let mut r = round(100, &[0, 0]);
        r.stack = vec![0, 0];
        r.acted = vec![true, true];
        assert_eq!(first_to_act(Street::Flop, &r, &dealt, 0, 1, 2), None);
        assert_eq!(next_to_act(&r, &dealt, 0, 2), None);
    }

    /// A seat that was not dealt in is never offered the action, even though it
    /// occupies a seat and may have posted a blind (D-005).
    #[test]
    fn an_absent_seat_is_never_offered_the_action() {
        let dealt = [true, false, true];
        let r = round(100, &[5000, 5000, 5000]);
        assert!(!can_act(&r, &dealt, 1));
        assert_eq!(live_count(&r, &dealt), 2);
        assert_eq!(next_to_act(&r, &dealt, 0, 3), Some(2), "seat 1 is skipped");
    }
}
