//! Pot construction and award (`docs/research/POKER_RULES.md` A7 and A8).
//!
//! Pots are **derived, never incrementally mutated**. They are computed as a
//! pure function of what each seat has committed this hand and who has folded.
//! That is what lets two independent implementations agree bit for bit, and it
//! is what makes the state hash of `SPEC_CS.md` section 15 well defined: there
//! is no accumulated pot counter that could drift.

use crate::poker::evaluator::HandRank;
use crate::poker::state::{Chips, SeatIdx};

/// One pot layer: the chips, who paid into it, and who may win it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pot {
    pub size: Chips,
    /// Every seat that paid into this layer, folded or not.
    ///
    /// Not needed to award the pot, but kept because it is the only sound
    /// fallback if `eligible` is ever empty — see [`award`].
    pub contributors: Vec<SeatIdx>,
    /// The seats that may win it: contributors who did not fold.
    pub eligible: Vec<SeatIdx>,
}

/// Chips returned to a seat because nobody could contest them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refund {
    pub seat: SeatIdx,
    pub amount: Chips,
}

/// Build the main pot and every side pot from this hand's commitments.
///
/// `committed[s]` is what seat `s` has put in during the whole hand;
/// `folded[s]` says whether it folded. Both are indexed by seat.
///
/// Returns the pots in ascending order — index 0 is the main pot — together
/// with any uncalled excess, which is handed straight back. No player may win
/// more than they can be called for, and the single-contributor test below is
/// exactly that rule.
pub fn build_pots(committed: &[Chips], folded: &[bool]) -> (Vec<Pot>, Vec<Refund>) {
    debug_assert_eq!(committed.len(), folded.len());

    let mut levels: Vec<Chips> = committed.iter().copied().filter(|&c| c > 0).collect();
    levels.sort_unstable();
    levels.dedup();

    let mut pots = Vec::new();
    let mut refunds = Vec::new();
    let mut prev: Chips = 0;

    for level in levels {
        let contributors: Vec<SeatIdx> = (0..committed.len())
            .filter(|&s| committed[s] > prev)
            .map(|s| s as SeatIdx)
            .collect();

        let each = level - prev;
        let size = each * contributors.len() as Chips;

        if contributors.len() == 1 {
            // Nobody could contest this layer, so it was never really at stake.
            refunds.push(Refund { seat: contributors[0], amount: size });
        } else {
            let eligible: Vec<SeatIdx> = contributors
                .iter()
                .copied()
                .filter(|&s| !folded[s as usize])
                .collect();
            pots.push(Pot { size, contributors, eligible });
        }
        prev = level;
    }

    (pots, refunds)
}

/// Seats in clockwise order starting from the first seat left of the button.
///
/// The button position may be an empty seat under the dead-button rule. That is
/// fine: it is still a well-defined index, and this ordering needs no card data
/// at all, so it cannot leak anything and cannot disagree with the evaluator.
fn clockwise_from_left_of_button(button: SeatIdx, seat_count: u8) -> impl Iterator<Item = SeatIdx> {
    let n = seat_count as u16;
    (1..=n).map(move |offset| ((button as u16 + offset) % n) as SeatIdx)
}

/// Award one pot.
///
/// `rank[s]` is the showdown strength of seat `s`, or `None` if that seat has
/// no live hand. Returns the chips each winner receives.
///
/// Ties split by integer division, and the remainder is handed out one chip at
/// a time clockwise from the first seat left of the button — TDA rule 20-A,
/// generalised from a single odd chip to as many as the split leaves over. That
/// convention is a pure function of the button and the winner set, both of
/// which are public state already in the hash chain.
pub fn award(pot: &Pot, rank: &[Option<HandRank>], button: SeatIdx, seat_count: u8) -> Vec<(SeatIdx, Chips)> {
    // `eligible` is non-empty in any legally reachable state: whoever last put
    // chips into a layer voluntarily was not folding at that moment. It can
    // only be empty through a byzantine peer, and losing the chips would break
    // conservation, so fall back to the contributors pro rata.
    let candidates: Vec<SeatIdx> = if pot.eligible.is_empty() {
        debug_assert!(false, "a pot with no eligible seat is not legally reachable");
        pot.contributors.clone()
    } else {
        pot.eligible.clone()
    };

    let best = candidates
        .iter()
        .filter_map(|&s| rank[s as usize].map(|r| (s, r)))
        .map(|(_, r)| r)
        .max();

    let winners: Vec<SeatIdx> = match best {
        Some(best) => candidates
            .iter()
            .copied()
            .filter(|&s| rank[s as usize] == Some(best))
            .collect(),
        // Nobody eligible has a live hand. Same reasoning as above: give it
        // back rather than destroy chips.
        None => candidates,
    };

    let n = winners.len() as Chips;
    let share = pot.size / n;
    let remainder = pot.size % n;

    let mut payout: Vec<(SeatIdx, Chips)> = winners.iter().map(|&s| (s, share)).collect();

    if remainder > 0 {
        let mut handed = 0;
        for seat in clockwise_from_left_of_button(button, seat_count) {
            if handed == remainder {
                break;
            }
            if let Some(entry) = payout.iter_mut().find(|(s, _)| *s == seat) {
                entry.1 += 1;
                handed += 1;
            }
        }
        debug_assert_eq!(handed, remainder, "every odd chip must find a winner");
    }

    payout
}

/// Total chips held by the pots plus the refunds.
pub fn total(pots: &[Pot], refunds: &[Refund]) -> Chips {
    pots.iter().map(|p| p.size).sum::<Chips>() + refunds.iter().map(|r| r.amount).sum::<Chips>()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four-player, three-all-in example worked out in POKER_RULES.md A7.
    ///
    /// Blinds 50/100, button S1. Stacks S1=1000, S2=300, S3=600, S4=200, so
    /// 2100 chips at the table. Commitments end at S1=600, S2=300, S3=600,
    /// S4=200.
    #[test]
    fn three_all_ins_at_different_amounts_build_three_layers() {
        let committed = [600, 300, 600, 200];
        let folded = [false, false, false, false];
        let (pots, refunds) = build_pots(&committed, &folded);

        assert!(refunds.is_empty(), "every layer had at least two contributors");
        assert_eq!(pots.len(), 3);

        // Main pot: 0 -> 200, all four seats.
        assert_eq!(pots[0].size, 800);
        assert_eq!(pots[0].eligible, vec![0, 1, 2, 3]);

        // Side pot 1: 200 -> 300, S4 is out of it.
        assert_eq!(pots[1].size, 300);
        assert_eq!(pots[1].eligible, vec![0, 1, 2]);

        // Side pot 2: 300 -> 600, only S1 and S3 reached it.
        assert_eq!(pots[2].size, 600);
        assert_eq!(pots[2].eligible, vec![0, 2]);

        assert_eq!(total(&pots, &refunds), committed.iter().sum::<Chips>());
        assert_eq!(total(&pots, &refunds), 1700);
    }

    /// The same example's award, with showdown strength S4 > S3 > S1 > S2.
    #[test]
    fn the_short_stack_wins_only_the_main_pot() {
        let committed = [600, 300, 600, 200];
        let folded = [false; 4];
        let (pots, _) = build_pots(&committed, &folded);

        // Ranks are opaque, so borrow real ones and order them to match.
        // `distinct_ranks()` is strongest-first; seats are indexed S1..S4.
        let ranks = distinct_ranks();
        let rank = [
            Some(ranks[2]), // S1 third
            Some(ranks[3]), // S2 worst
            Some(ranks[1]), // S3 second
            Some(ranks[0]), // S4 best
        ];

        let main = award(&pots[0], &rank, 0, 4);
        assert_eq!(main, vec![(3, 800)], "S4 takes the main pot");

        let side1 = award(&pots[1], &rank, 0, 4);
        assert_eq!(side1, vec![(2, 300)], "S3 takes side pot 1");

        let side2 = award(&pots[2], &rank, 0, 4);
        assert_eq!(side2, vec![(2, 600)], "S3 takes side pot 2");

        // S4 finishes on 800, S3 on 900, S1 keeps the 400 it never committed,
        // S2 is eliminated. The table started with 2100 chips.
        let s1_behind: Chips = 400;
        let awarded: Chips = main[0].1 + side1[0].1 + side2[0].1;
        assert_eq!(awarded, 1700);
        assert_eq!(awarded + s1_behind, 2100, "chips are conserved across the hand");
    }

    /// A bet nobody could call comes straight back.
    #[test]
    fn uncalled_excess_is_refunded_not_won() {
        // S1 bets 800, S3 calls all-in for 500.
        let committed = [800, 0, 500, 0];
        let folded = [false, false, false, false];
        let (pots, refunds) = build_pots(&committed, &folded);

        assert_eq!(pots.len(), 1);
        assert_eq!(pots[0].size, 1000);
        assert_eq!(pots[0].eligible, vec![0, 2]);

        assert_eq!(refunds, vec![Refund { seat: 0, amount: 300 }]);
        assert_eq!(total(&pots, &refunds), 1300);
    }

    /// "A bets 300, B raises all-in to 900, A folds": B wins A's 300 and gets
    /// the uncalled 600 back. The folded player's chips stay in the pot.
    #[test]
    fn a_folded_player_pays_in_but_cannot_win() {
        let committed = [300, 900];
        let folded = [true, false];
        let (pots, refunds) = build_pots(&committed, &folded);

        assert_eq!(pots.len(), 1);
        assert_eq!(pots[0].size, 600, "0 -> 300 across both seats");
        assert_eq!(pots[0].contributors, vec![0, 1], "A did contribute");
        assert_eq!(pots[0].eligible, vec![1], "but A folded, so cannot win");

        assert_eq!(refunds, vec![Refund { seat: 1, amount: 600 }]);
        assert_eq!(total(&pots, &refunds), 1200);
    }

    /// The odd-chip example from A8: main pot 800, S1/S2/S4 tie, button on S1.
    /// Clockwise from the seat left of the button is S2, S3, S4, S1; restricted
    /// to the winners that is S2, S4, S1, so the two odd chips go to S2 and S4.
    #[test]
    fn odd_chips_go_clockwise_from_the_first_seat_left_of_the_button() {
        let committed = [600, 300, 600, 200];
        let folded = [false; 4];
        let (pots, _) = build_pots(&committed, &folded);

        let ranks = distinct_ranks();
        // S1, S2 and S4 tie on the best hand; S3 is beaten.
        let rank = [Some(ranks[0]), Some(ranks[0]), Some(ranks[1]), Some(ranks[0])];

        let payout = award(&pots[0], &rank, 0, 4);
        let get = |seat: SeatIdx| payout.iter().find(|(s, _)| *s == seat).map(|(_, c)| *c);

        assert_eq!(get(1), Some(267), "S2 is first left of the button");
        assert_eq!(get(3), Some(267), "S4 is next among the winners");
        assert_eq!(get(0), Some(266), "S1 is last and gets no odd chip");
        assert_eq!(get(2), None, "S3 lost");
        assert_eq!(payout.iter().map(|(_, c)| *c).sum::<Chips>(), 800);
    }

    #[test]
    fn splitting_never_creates_or_destroys_a_chip() {
        let ranks = distinct_ranks();
        for size in 0..200u64 {
            for winners in 1..=4usize {
                let pot = Pot {
                    size,
                    contributors: (0..4).collect(),
                    eligible: (0..winners as SeatIdx).collect(),
                };
                let mut rank = [None; 4];
                for seat in 0..winners {
                    rank[seat] = Some(ranks[0]);
                }
                for button in 0..4u8 {
                    let payout = award(&pot, &rank, button, 4);
                    let paid: Chips = payout.iter().map(|(_, c)| *c).sum();
                    assert_eq!(paid, size, "size {size}, {winners} winners, button {button}");
                }
            }
        }
    }

    #[test]
    fn a_hand_where_nobody_committed_produces_no_pots() {
        let (pots, refunds) = build_pots(&[0, 0, 0], &[false, false, false]);
        assert!(pots.is_empty());
        assert!(refunds.is_empty());
    }

    /// Chip conservation over a wide sweep of commitment patterns. This is
    /// invariant I1 of the state machine, checked here at the level that
    /// actually moves chips.
    #[test]
    fn pots_plus_refunds_always_equal_what_was_committed() {
        for a in 0..8u64 {
            for b in 0..8u64 {
                for c in 0..8u64 {
                    for d in 0..8u64 {
                        let committed = [a * 25, b * 25, c * 25, d * 25];
                        for mask in 0..16u8 {
                            let folded = [
                                mask & 1 != 0,
                                mask & 2 != 0,
                                mask & 4 != 0,
                                mask & 8 != 0,
                            ];
                            let (pots, refunds) = build_pots(&committed, &folded);
                            assert_eq!(
                                total(&pots, &refunds),
                                committed.iter().sum::<Chips>(),
                                "committed {committed:?} folded {folded:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// Four hand ranks in strictly descending strength, for tests that only
    /// care about the ordering.
    fn distinct_ranks() -> [HandRank; 4] {
        use crate::poker::evaluator::evaluate;
        use crate::poker::state::{Card, Rank, Suit};
        let c = Card::new;
        let straight_flush = [
            c(Rank::Nine, Suit::Hearts),
            c(Rank::Ten, Suit::Hearts),
            c(Rank::Jack, Suit::Hearts),
            c(Rank::Queen, Suit::Hearts),
            c(Rank::King, Suit::Hearts),
        ];
        let flush = [
            c(Rank::Two, Suit::Clubs),
            c(Rank::Five, Suit::Clubs),
            c(Rank::Nine, Suit::Clubs),
            c(Rank::Jack, Suit::Clubs),
            c(Rank::King, Suit::Clubs),
        ];
        let pair = [
            c(Rank::Two, Suit::Clubs),
            c(Rank::Two, Suit::Diamonds),
            c(Rank::Nine, Suit::Hearts),
            c(Rank::Jack, Suit::Spades),
            c(Rank::King, Suit::Clubs),
        ];
        let high = [
            c(Rank::Two, Suit::Clubs),
            c(Rank::Five, Suit::Diamonds),
            c(Rank::Nine, Suit::Hearts),
            c(Rank::Jack, Suit::Spades),
            c(Rank::King, Suit::Clubs),
        ];
        let out = [
            evaluate(&straight_flush).unwrap(),
            evaluate(&flush).unwrap(),
            evaluate(&pair).unwrap(),
            evaluate(&high).unwrap(),
        ];
        assert!(out[0] > out[1] && out[1] > out[2] && out[2] > out[3]);
        out
    }
}
