//! Seven-card hand ranking, behind our own facade.
//!
//! The backing crate is `rs_poker = "=5.0.0"`, chosen in
//! `docs/research/POKER_RULES.md` A4. Pinned exactly, because 5.1.0 does not
//! build on stable rustc 1.95.0 — a verified negative result, not caution.
//!
//! Everything the rest of the engine sees is defined here, so the crate can be
//! replaced without touching a single call site.
//!
//! # The mapping is the dangerous part
//!
//! `rs_poker` orders suits `Spade = 0, Club = 1, Heart = 2, Diamond = 3`. Our
//! canonical encoding orders them `Clubs = 0, Diamonds = 1, Hearts = 2,
//! Spades = 3`, and ours is hashed into the transcript, so it cannot move to
//! match. The conversion below is therefore explicit and pinned by a test. A
//! silent `as u8` between the two would corrupt every card in the protocol
//! while still producing a plausible-looking hand ranking.

use rs_poker::core::{Card as RsCard, Rankable, Suit as RsSuit, Value as RsValue};

use crate::poker::state::{Card, Rank, Suit};

/// The category of a made hand, best last.
///
/// Ours rather than the backing crate's, so the dependency does not leak into
/// the engine or the GUI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    HighCard,
    OnePair,
    TwoPair,
    ThreeOfAKind,
    Straight,
    Flush,
    FullHouse,
    FourOfAKind,
    StraightFlush,
}

/// The strength of a five-card hand, total-ordered: greater is better.
///
/// Two hands compare equal exactly when they split the pot, which is what
/// `docs/STATE_MACHINE.md` section 7.6 needs in order to divide chips.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HandRank(rs_poker::core::Rank);

impl HandRank {
    /// The category this hand falls into.
    pub fn category(self) -> Category {
        use rs_poker::core::CoreRank as C;
        match self.0.category() {
            C::HighCard => Category::HighCard,
            C::OnePair => Category::OnePair,
            C::TwoPair => Category::TwoPair,
            C::ThreeOfAKind => Category::ThreeOfAKind,
            C::Straight => Category::Straight,
            C::Flush => Category::Flush,
            C::FullHouse => Category::FullHouse,
            C::FourOfAKind => Category::FourOfAKind,
            C::StraightFlush => Category::StraightFlush,
        }
    }
}

const fn to_rs_suit(suit: Suit) -> RsSuit {
    match suit {
        Suit::Clubs => RsSuit::Club,
        Suit::Diamonds => RsSuit::Diamond,
        Suit::Hearts => RsSuit::Heart,
        Suit::Spades => RsSuit::Spade,
    }
}

const fn to_rs_value(rank: Rank) -> RsValue {
    match rank {
        Rank::Two => RsValue::Two,
        Rank::Three => RsValue::Three,
        Rank::Four => RsValue::Four,
        Rank::Five => RsValue::Five,
        Rank::Six => RsValue::Six,
        Rank::Seven => RsValue::Seven,
        Rank::Eight => RsValue::Eight,
        Rank::Nine => RsValue::Nine,
        Rank::Ten => RsValue::Ten,
        Rank::Jack => RsValue::Jack,
        Rank::Queen => RsValue::Queen,
        Rank::King => RsValue::King,
        Rank::Ace => RsValue::Ace,
    }
}

const fn to_rs(card: Card) -> RsCard {
    RsCard {
        value: to_rs_value(card.rank()),
        suit: to_rs_suit(card.suit()),
    }
}

/// Rank the best five-card hand available from `cards`.
///
/// Takes five to seven cards. Fewer than five cannot be ranked, and the engine
/// never asks: a showdown only happens once the board is complete.
pub fn evaluate(cards: &[Card]) -> Option<HandRank> {
    if cards.len() < 5 || cards.len() > 7 {
        return None;
    }
    let hand: Vec<RsCard> = cards.iter().copied().map(to_rs).collect();
    Some(HandRank(hand.rank()))
}

/// Rank a hold'em hand: two hole cards plus the five board cards.
pub fn evaluate_holdem(hole: [Card; 2], board: &[Card; 5]) -> HandRank {
    let mut all = [hole[0], hole[1], board[0], board[1], board[2], board[3], board[4]];
    all.sort_unstable();
    evaluate(&all).expect("seven cards is always rankable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poker::state::{Rank, Suit};

    fn c(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    /// The one test that must never be deleted.
    ///
    /// Our suit order and `rs_poker`'s differ. If somebody ever "simplifies"
    /// the conversion into a cast, this catches it.
    #[test]
    fn suit_conversion_is_explicit_and_not_a_cast() {
        assert_eq!(to_rs_suit(Suit::Clubs), RsSuit::Club);
        assert_eq!(to_rs_suit(Suit::Diamonds), RsSuit::Diamond);
        assert_eq!(to_rs_suit(Suit::Hearts), RsSuit::Heart);
        assert_eq!(to_rs_suit(Suit::Spades), RsSuit::Spade);

        // And the numeric values genuinely disagree, which is why the explicit
        // match is load-bearing rather than decorative.
        assert_eq!(Suit::Clubs as u8, 0);
        assert_eq!(RsSuit::Club as u8, 1);
        assert_eq!(Suit::Spades as u8, 3);
        assert_eq!(RsSuit::Spade as u8, 0);
    }

    #[test]
    fn categories_are_recognised() {
        let straight_flush = [
            c(Rank::Nine, Suit::Hearts),
            c(Rank::Ten, Suit::Hearts),
            c(Rank::Jack, Suit::Hearts),
            c(Rank::Queen, Suit::Hearts),
            c(Rank::King, Suit::Hearts),
        ];
        assert_eq!(
            evaluate(&straight_flush).unwrap().category(),
            Category::StraightFlush
        );

        let full_house = [
            c(Rank::Four, Suit::Clubs),
            c(Rank::Four, Suit::Diamonds),
            c(Rank::Four, Suit::Hearts),
            c(Rank::Nine, Suit::Spades),
            c(Rank::Nine, Suit::Clubs),
        ];
        assert_eq!(
            evaluate(&full_house).unwrap().category(),
            Category::FullHouse
        );

        let high_card = [
            c(Rank::Two, Suit::Clubs),
            c(Rank::Five, Suit::Diamonds),
            c(Rank::Nine, Suit::Hearts),
            c(Rank::Jack, Suit::Spades),
            c(Rank::King, Suit::Clubs),
        ];
        assert_eq!(evaluate(&high_card).unwrap().category(), Category::HighCard);
    }

    /// A-2-3-4-5 is a straight, and the ace plays low. This is the classic
    /// place a hand evaluator is wrong.
    #[test]
    fn the_wheel_is_a_straight_and_is_the_lowest_one() {
        let wheel = [
            c(Rank::Ace, Suit::Clubs),
            c(Rank::Two, Suit::Diamonds),
            c(Rank::Three, Suit::Hearts),
            c(Rank::Four, Suit::Spades),
            c(Rank::Five, Suit::Clubs),
        ];
        let six_high = [
            c(Rank::Two, Suit::Clubs),
            c(Rank::Three, Suit::Diamonds),
            c(Rank::Four, Suit::Hearts),
            c(Rank::Five, Suit::Spades),
            c(Rank::Six, Suit::Clubs),
        ];
        let w = evaluate(&wheel).unwrap();
        let s = evaluate(&six_high).unwrap();
        assert_eq!(w.category(), Category::Straight);
        assert_eq!(s.category(), Category::Straight);
        assert!(w < s, "the wheel is the lowest straight");
    }

    #[test]
    fn ordering_runs_from_high_card_up_to_straight_flush() {
        let hands: [(&str, [Card; 5]); 5] = [
            (
                "high card",
                [
                    c(Rank::Two, Suit::Clubs),
                    c(Rank::Five, Suit::Diamonds),
                    c(Rank::Nine, Suit::Hearts),
                    c(Rank::Jack, Suit::Spades),
                    c(Rank::King, Suit::Clubs),
                ],
            ),
            (
                "one pair",
                [
                    c(Rank::Two, Suit::Clubs),
                    c(Rank::Two, Suit::Diamonds),
                    c(Rank::Nine, Suit::Hearts),
                    c(Rank::Jack, Suit::Spades),
                    c(Rank::King, Suit::Clubs),
                ],
            ),
            (
                "trips",
                [
                    c(Rank::Two, Suit::Clubs),
                    c(Rank::Two, Suit::Diamonds),
                    c(Rank::Two, Suit::Hearts),
                    c(Rank::Jack, Suit::Spades),
                    c(Rank::King, Suit::Clubs),
                ],
            ),
            (
                "flush",
                [
                    c(Rank::Two, Suit::Clubs),
                    c(Rank::Five, Suit::Clubs),
                    c(Rank::Nine, Suit::Clubs),
                    c(Rank::Jack, Suit::Clubs),
                    c(Rank::King, Suit::Clubs),
                ],
            ),
            (
                "straight flush",
                [
                    c(Rank::Nine, Suit::Hearts),
                    c(Rank::Ten, Suit::Hearts),
                    c(Rank::Jack, Suit::Hearts),
                    c(Rank::Queen, Suit::Hearts),
                    c(Rank::King, Suit::Hearts),
                ],
            ),
        ];
        for pair in hands.windows(2) {
            let (lo_name, lo) = &pair[0];
            let (hi_name, hi) = &pair[1];
            let lo = evaluate(lo).unwrap();
            let hi = evaluate(hi).unwrap();
            assert!(lo < hi, "{lo_name} should rank below {hi_name}");
        }
    }

    /// Two players holding the same five-card hand through the board must
    /// compare equal, or the split-pot rule can never fire.
    #[test]
    fn a_board_that_plays_gives_both_players_the_same_rank() {
        let board = [
            c(Rank::Ace, Suit::Clubs),
            c(Rank::King, Suit::Diamonds),
            c(Rank::Queen, Suit::Hearts),
            c(Rank::Jack, Suit::Spades),
            c(Rank::Ten, Suit::Clubs),
        ];
        let alice = evaluate_holdem([c(Rank::Two, Suit::Hearts), c(Rank::Three, Suit::Spades)], &board);
        let bob = evaluate_holdem([c(Rank::Four, Suit::Diamonds), c(Rank::Five, Suit::Clubs)], &board);
        assert_eq!(alice.category(), Category::Straight);
        assert_eq!(alice, bob, "the board plays; this hand is a split pot");
    }

    #[test]
    fn seven_cards_pick_the_best_five() {
        // The two hole cards are irrelevant; the flush is entirely on board.
        let board = [
            c(Rank::Two, Suit::Clubs),
            c(Rank::Five, Suit::Clubs),
            c(Rank::Nine, Suit::Clubs),
            c(Rank::Jack, Suit::Clubs),
            c(Rank::King, Suit::Clubs),
        ];
        let r = evaluate_holdem([c(Rank::Seven, Suit::Hearts), c(Rank::Eight, Suit::Diamonds)], &board);
        assert_eq!(r.category(), Category::Flush);
    }

    #[test]
    fn evaluate_refuses_hands_it_cannot_rank() {
        let four = [
            c(Rank::Two, Suit::Clubs),
            c(Rank::Five, Suit::Diamonds),
            c(Rank::Nine, Suit::Hearts),
            c(Rank::Jack, Suit::Spades),
        ];
        assert!(evaluate(&four).is_none());
    }

    /// Ranking must not depend on the order the cards arrive in. The engine
    /// sorts before hashing, and different peers assemble the seven cards in
    /// different orders, so a permutation-sensitive evaluator would produce
    /// different showdown results on different clients.
    #[test]
    fn ranking_is_independent_of_card_order() {
        let mut cards = [
            c(Rank::Ace, Suit::Spades),
            c(Rank::King, Suit::Spades),
            c(Rank::Seven, Suit::Hearts),
            c(Rank::Seven, Suit::Clubs),
            c(Rank::Two, Suit::Diamonds),
            c(Rank::Nine, Suit::Spades),
            c(Rank::Four, Suit::Hearts),
        ];
        let expected = evaluate(&cards).unwrap();

        // Deterministic permutations: rotate, and reverse.
        for shift in 1..cards.len() {
            cards.rotate_left(shift);
            assert_eq!(evaluate(&cards).unwrap(), expected, "rotation changed the rank");
        }
        cards.reverse();
        assert_eq!(evaluate(&cards).unwrap(), expected, "reversal changed the rank");
    }
}
