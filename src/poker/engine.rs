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

/// Whether everyone has folded to one seat, which ends the hand at once.
///
/// This is a **separate** hand-ending condition from [`round_complete`], and
/// forgetting that is a real bug rather than a nicety: the last live seat has
/// not acted, so the round is not complete by that rule, yet offering it the
/// action lets it fold too and leaves a pot nobody is eligible for. The
/// random-hand harness found exactly that, with two heads-up seats both folded
/// and 150 chips in a pot with an empty eligible set.
///
/// The survivor wins every pot it is eligible for and never reveals a card.
pub fn only_one_live(round: &BettingRound, dealt_in: &DealtIn) -> bool {
    live_count(round, dealt_in) == 1
}

/// Whether any further betting is possible **in this hand**.
///
/// Distinct from [`round_complete`], which is about one street. With at most
/// one seat able to act and nothing owed, there is nobody left to bet into:
/// the remaining board cards are still dealt, because they decide the pots, but
/// no action happens between them and the hand is not abandoned
/// (`POKER_RULES.md` A2, A6).
///
/// The condition is deliberately **not** "and that seat has acted". A lone
/// seat facing no bet has nothing to do, whether or not it has acted this
/// street, and offering it the action lets it fold a hand it has already won.
/// The random-hand harness found that: two seats contested a side pot above an
/// all-in player, one folded, and the survivor was then offered the action and
/// folded too, leaving 19 326 chips in a pot with nobody eligible for it.
pub fn betting_is_closed(round: &BettingRound, dealt_in: &DealtIn) -> bool {
    let actors: Vec<SeatIdx> = (0..round.committed.len())
        .map(|s| s as SeatIdx)
        .filter(|&seat| can_act(round, dealt_in, seat))
        .collect();

    match actors.as_slice() {
        [] => true,
        // One seat with chips: it acts only if it still owes something.
        [only] => round.to_call(*only) == 0,
        _ => false,
    }
}

/// Where the button and the blinds sit for one hand.
///
/// `button` and `small_blind` are **positions**, not necessarily occupied
/// seats: under the dead-button rule either may land on a seat whose player has
/// busted. `big_blind` is always a live seat — the big-blind position never
/// dies, because somebody has to post it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Positions {
    pub button: SeatIdx,
    pub small_blind: SeatIdx,
    pub big_blind: SeatIdx,
}

/// The next seat with chips, clockwise from `from`, excluding `from`.
fn succ_alive(from: SeatIdx, alive: &[bool], seat_count: u8) -> Option<SeatIdx> {
    ring_after(from, seat_count).find(|&s| alive[s as usize])
}

/// The two seats still holding chips, if exactly two do.
fn the_two_alive(alive: &[bool]) -> Option<(SeatIdx, SeatIdx)> {
    let mut it = alive.iter().enumerate().filter(|(_, &a)| a).map(|(s, _)| s as SeatIdx);
    match (it.next(), it.next(), it.next()) {
        (Some(a), Some(b), None) => Some((a, b)),
        _ => None,
    }
}

/// Positions for the first hand, given where the button starts.
pub fn initial_positions(button: SeatIdx, alive: &[bool], seat_count: u8) -> Option<Positions> {
    if alive.iter().filter(|&&a| a).count() < 2 {
        return None;
    }
    if let Some((a, b)) = the_two_alive(alive) {
        // Heads-up: the button *is* the small blind (TDA 34-B).
        let (button, bb) = if alive[button as usize] { (button, if button == a { b } else { a }) } else { (a, b) };
        return Some(Positions { button, small_blind: button, big_blind: bb });
    }
    let small_blind = succ_alive(button, alive, seat_count)?;
    let big_blind = succ_alive(small_blind, alive, seat_count)?;
    Some(Positions { button, small_blind, big_blind })
}

/// Advance the button and blinds for the next hand — the **dead button** rule.
///
/// TDA 32: "Tournament play will use a dead button." The blind positions move
/// one seat *position* each hand regardless of who is still alive, so no player
/// ever posts the big blind twice running and no player ever skips it:
///
/// ```text
/// next_big_blind    = succ_alive(big_blind)   // the next survivor, clockwise
/// next_small_blind  = big_blind               // the previous BB position
/// next_button       = small_blind             // the previous SB position
/// ```
///
/// If the new button position is empty the button is **dead** — it marks action
/// order and the odd-chip start, nothing more. If the new small-blind position
/// is empty the small blind is **dead**: it is simply not posted, and the pot is
/// one small blind lighter.
///
/// This is deliberately **not** what PokerTH does. It shifts the dealer to the
/// next surviving player (`src/engine/game.cpp:191-210`, whose own comment reads
/// `// shifting dealer button -> TODO exception-rule !!!`), which lets a player
/// post the big blind on two consecutive hands when the seat between the blinds
/// busts. The preset is PokerTH's; this rule is the tournament standard.
///
/// Returns `None` when fewer than two seats still hold chips, which is the
/// tournament's end condition rather than an error.
pub fn advance_positions(prev: Positions, alive: &[bool], seat_count: u8) -> Option<Positions> {
    // Guarded explicitly rather than left to `succ_alive`, which wraps the
    // whole ring and would hand back the lone survivor as its own successor.
    if alive.iter().filter(|&&a| a).count() < 2 {
        return None;
    }
    if let Some((a, b)) = the_two_alive(alive) {
        // Heads-up: alternate the big blind, and the other seat is both button
        // and small blind. Stated directly rather than derived, because the
        // TDA 34-B adjustment exists precisely so the general rotation cannot
        // hand one player the big blind twice.
        let big_blind = if prev.big_blind == a { b } else { a };
        let other = if big_blind == a { b } else { a };
        return Some(Positions { button: other, small_blind: other, big_blind });
    }

    let big_blind = succ_alive(prev.big_blind, alive, seat_count)?;
    Some(Positions {
        button: prev.small_blind,
        small_blind: prev.big_blind,
        big_blind,
    })
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

    /// The regression the random-hand harness produced: heads-up, the button
    /// folds pre-flop, and if the survivor is then offered the action it can
    /// fold too, leaving a 150-chip pot with an empty eligible set.
    #[test]
    fn a_fold_out_ends_the_hand_before_the_survivor_can_fold_too() {
        let dealt = [true, true];
        let mut r = round(100, &[5000, 5000]);
        post_blinds(&mut r, 0, 1, 50, 100);

        assert!(!only_one_live(&r, &dealt));
        r.apply(0, Action::Fold).unwrap();

        assert!(only_one_live(&r, &dealt), "the hand is over now");
        assert!(
            !round_complete(&r, &dealt),
            "and the round is NOT complete - the survivor has not acted, which              is exactly why this needs its own condition"
        );
        // The survivor is still technically able to act, which is the trap.
        assert!(can_act(&r, &dealt, 1));
    }

    #[test]
    fn only_one_live_ignores_seats_that_were_never_dealt_in() {
        let dealt = [true, false, true];
        let mut r = round(100, &[5000, 5000, 5000]);
        assert!(!only_one_live(&r, &dealt));
        r.apply(0, Action::Fold).unwrap();
        assert!(only_one_live(&r, &dealt), "seat 1 was never in the hand");
    }

    /// The second regression from the random-hand harness, and the sharper of
    /// the two. Two seats contest a side pot above an all-in player; one folds;
    /// the survivor owes nothing and must not be asked again.
    #[test]
    fn a_lone_seat_owing_nothing_is_not_asked_to_act() {
        let dealt = [true, true, true];
        let mut r = round(100, &[5000, 300, 5000]);

        r.apply(0, Action::Bet(300)).unwrap();
        r.apply(1, Action::Call).unwrap(); // seat 1 all-in for 300
        r.apply(2, Action::Call).unwrap();
        assert!(!betting_is_closed(&r, &dealt), "two seats still have chips");

        // A later street: seats 0 and 2 bet on, seat 1 is all-in and out of it.
        r.current_bet = 0;
        r.last_full_raise = 100;
        r.committed.iter_mut().for_each(|c| *c = 0);
        r.acted.iter_mut().for_each(|a| *a = false);

        r.apply(0, Action::Bet(500)).unwrap();
        r.apply(2, Action::Fold).unwrap();

        assert!(
            betting_is_closed(&r, &dealt),
            "seat 0 owes nothing and is the only seat with chips"
        );
        assert!(
            !only_one_live(&r, &dealt),
            "seat 1 is still live while all-in, so the fold-out rule does not fire"
        );
    }

    #[test]
    fn a_lone_seat_that_still_owes_must_act() {
        let dealt = [true, true];
        let mut r = round(100, &[5000, 300]);
        r.apply(1, Action::Bet(300)).unwrap(); // seat 1 all-in
        assert!(
            !betting_is_closed(&r, &dealt),
            "seat 0 owes 300 and must call or fold"
        );
        r.apply(0, Action::Call).unwrap();
        assert!(betting_is_closed(&r, &dealt));
    }

    #[test]
    fn with_a_full_table_the_positions_simply_advance_by_one() {
        let alive = [true; 5];
        let mut pos = initial_positions(0, &alive, 5).unwrap();
        assert_eq!(pos, Positions { button: 0, small_blind: 1, big_blind: 2 });

        for expected in [
            Positions { button: 1, small_blind: 2, big_blind: 3 },
            Positions { button: 2, small_blind: 3, big_blind: 4 },
            Positions { button: 3, small_blind: 4, big_blind: 0 },
        ] {
            pos = advance_positions(pos, &alive, 5).unwrap();
            assert_eq!(pos, expected);
        }
    }

    /// The case the dead button exists for. PokerTH, which moves the button to
    /// the next surviving player, would give one seat the big blind twice here.
    #[test]
    fn the_button_may_land_on_a_busted_seat_rather_than_repeat_a_big_blind() {
        // Five seats; seat 3 busts after the hand where it was the big blind.
        let mut alive = [true; 5];
        let pos = Positions { button: 1, small_blind: 2, big_blind: 3 };
        alive[3] = false;

        let next = advance_positions(pos, &alive, 5).unwrap();
        assert_eq!(next.big_blind, 4, "the next survivor posts the big blind");
        assert_eq!(next.small_blind, 3, "the previous BB position, now empty");
        assert!(!alive[next.small_blind as usize], "so the small blind is dead");
        assert_eq!(next.button, 2, "the previous SB position");

        // The point: seat 4 has the big blind now and seat 0 next, so nobody
        // posts it twice.
        let after = advance_positions(next, &alive, 5).unwrap();
        assert_eq!(after.big_blind, 0);
        assert_ne!(after.big_blind, next.big_blind);
    }

    #[test]
    fn heads_up_the_button_is_the_small_blind_and_the_big_blind_alternates() {
        let alive = [true, false, true, false];
        let mut pos = initial_positions(0, &alive, 4).unwrap();
        assert_eq!(pos.button, pos.small_blind, "TDA 34-B: the button is the SB");

        let first_bb = pos.big_blind;
        pos = advance_positions(pos, &alive, 4).unwrap();
        assert_ne!(pos.big_blind, first_bb, "the big blind must alternate");
        assert_eq!(pos.button, pos.small_blind);

        pos = advance_positions(pos, &alive, 4).unwrap();
        assert_eq!(pos.big_blind, first_bb, "and alternate back");
    }

    /// The invariant TDA 32 is written to produce, checked over a long run with
    /// players busting at arbitrary moments: **nobody posts the big blind twice
    /// running.** This is the property, not the formula, so it is asserted
    /// directly rather than inferred from the positions.
    #[test]
    fn no_seat_ever_posts_the_big_blind_twice_in_a_row() {
        for seed in 0..64u64 {
            let mut alive = [true; 6];
            let mut pos = initial_positions(0, &alive, 6).unwrap();
            let mut previous_bb = pos.big_blind;
            let mut rng = seed | 1;

            for hand in 0..200 {
                // Deterministic xorshift; bust a seat now and then.
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                if hand % 7 == 0 {
                    let victim = (rng % 6) as usize;
                    if alive.iter().filter(|&&a| a).count() > 2 {
                        alive[victim] = false;
                    }
                }

                let Some(next) = advance_positions(pos, &alive, 6) else {
                    break; // fewer than two seats left: the tournament is over
                };
                assert!(
                    alive[next.big_blind as usize],
                    "seed {seed} hand {hand}: the big blind must be a live seat"
                );
                assert_ne!(
                    next.big_blind, previous_bb,
                    "seed {seed} hand {hand}: seat {} posted the big blind twice running",
                    next.big_blind
                );
                previous_bb = next.big_blind;
                pos = next;
            }
        }
    }

    #[test]
    fn a_field_of_one_has_no_next_hand() {
        let alive = [true, false, false];
        let pos = Positions { button: 0, small_blind: 0, big_blind: 0 };
        assert_eq!(
            advance_positions(pos, &alive, 3),
            None,
            "fewer than two seats with chips is the end condition, not an error"
        );
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
