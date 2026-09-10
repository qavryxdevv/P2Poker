//! What the hero's hand is called, and how likely it is to get better.
//!
//! `S1-CS`: the table window shows, on the left, the name of the hand the
//! player holds and -- while there are cards to come -- the chance that it
//! improves, category by category. PokerTH names the hand; the odds are the
//! owner's *ideally*, and they are exact rather than estimated: every card
//! that can still come is enumerated (19 600 flops, 1 081 turn-and-river
//! pairs, 46 rivers), which costs a few milliseconds once per street.
//!
//! Nothing here touches the game. It reads the two cards this client opened
//! for itself and the board it verified, and says what they add up to; the
//! engine's own ranking (`evaluator`) decides the category, and this module
//! only puts words to it.

use super::evaluator::{evaluate, Category};
use super::state::{Card, Rank, Suit};

/// When the odds run out: the next street, or the whole way to the river.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Horizon {
    ByTheFlop,
    ByTheRiver,
}

/// The hero's hand, in words and in numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct Strength {
    /// "pair of kings", "ace-king suited", "flush, queen high".
    pub name: String,
    /// The category of the best five cards, once there is a board.
    pub category: Option<Category>,
    /// The chance of ending up in each better category, best last, over the
    /// cards still to come. Empty on the river, and empty when nothing can
    /// improve (a royal flush).
    pub improve: Vec<(Category, f32)>,
    /// The chance of any improvement at all.
    pub improve_total: f32,
    /// What the odds count to. `None` on the river.
    pub horizon: Option<Horizon>,
}

/// The category's name, as a player says it.
pub const fn category_name(c: Category) -> &'static str {
    match c {
        Category::HighCard => "high card",
        Category::OnePair => "a pair",
        Category::TwoPair => "two pair",
        Category::ThreeOfAKind => "three of a kind",
        Category::Straight => "a straight",
        Category::Flush => "a flush",
        Category::FullHouse => "a full house",
        Category::FourOfAKind => "four of a kind",
        Category::StraightFlush => "a straight flush",
    }
}

fn rank_name(r: Rank) -> &'static str {
    match r {
        Rank::Two => "two",
        Rank::Three => "three",
        Rank::Four => "four",
        Rank::Five => "five",
        Rank::Six => "six",
        Rank::Seven => "seven",
        Rank::Eight => "eight",
        Rank::Nine => "nine",
        Rank::Ten => "ten",
        Rank::Jack => "jack",
        Rank::Queen => "queen",
        Rank::King => "king",
        Rank::Ace => "ace",
    }
}

fn plural(r: Rank) -> &'static str {
    match r {
        Rank::Two => "twos",
        Rank::Three => "threes",
        Rank::Four => "fours",
        Rank::Five => "fives",
        Rank::Six => "sixes",
        Rank::Seven => "sevens",
        Rank::Eight => "eights",
        Rank::Nine => "nines",
        Rank::Ten => "tens",
        Rank::Jack => "jacks",
        Rank::Queen => "queens",
        Rank::King => "kings",
        Rank::Ace => "aces",
    }
}

/// How many of each rank, indexed by `Rank as usize`.
fn counts(cards: &[Card]) -> [u8; 13] {
    let mut n = [0u8; 13];
    for c in cards {
        n[c.rank() as usize] += 1;
    }
    n
}

/// The highest rank that has at least `at_least` copies, skipping `not`.
fn highest_with(n: &[u8; 13], at_least: u8, not: Option<Rank>) -> Option<Rank> {
    Rank::ALL
        .iter()
        .rev()
        .copied()
        .find(|r| n[*r as usize] >= at_least && Some(*r) != not)
}

/// The top rank of the highest straight among these ranks, the wheel included.
fn straight_top(present: impl Fn(Rank) -> bool) -> Option<Rank> {
    // Ace counts low as well as high: the wheel is five-high.
    let has = |i: i32| -> bool {
        if i < 0 {
            present(Rank::Ace)
        } else {
            present(Rank::ALL[i as usize])
        }
    };
    (3..13).rev().find_map(|top: i32| {
        let run = (top - 4..=top).all(has);
        run.then(|| Rank::ALL[top as usize])
    })
}

/// The suit with five or more cards, if there is one.
fn flush_suit(cards: &[Card]) -> Option<Suit> {
    Suit::ALL
        .iter()
        .copied()
        .find(|s| cards.iter().filter(|c| c.suit() == *s).count() >= 5)
}

/// The hand's name from its category and the cards that make it.
///
/// The category is the engine's verdict; the names come from counting ranks,
/// which for hold'em identifies the best five exactly: the highest pair is the
/// pair that plays, the highest straight is the straight that plays.
fn name_of(category: Category, cards: &[Card]) -> String {
    let n = counts(cards);
    match category {
        Category::HighCard => format!("{} high", rank_name(highest_with(&n, 1, None).unwrap_or(Rank::Two))),
        Category::OnePair => format!("pair of {}", plural(highest_with(&n, 2, None).unwrap_or(Rank::Two))),
        Category::TwoPair => {
            let first = highest_with(&n, 2, None).unwrap_or(Rank::Two);
            let second = highest_with(&n, 2, Some(first)).unwrap_or(Rank::Two);
            format!("two pair, {} and {}", plural(first), plural(second))
        }
        Category::ThreeOfAKind => format!("three of a kind, {}", plural(highest_with(&n, 3, None).unwrap_or(Rank::Two))),
        Category::Straight => format!(
            "straight, {} high",
            rank_name(straight_top(|r| n[r as usize] > 0).unwrap_or(Rank::Five))
        ),
        Category::Flush => {
            let suit = flush_suit(cards).unwrap_or(Suit::Spades);
            let high = cards
                .iter()
                .filter(|c| c.suit() == suit)
                .map(|c| c.rank())
                .max()
                .unwrap_or(Rank::Two);
            format!("flush, {} high", rank_name(high))
        }
        Category::FullHouse => {
            let trips = highest_with(&n, 3, None).unwrap_or(Rank::Two);
            let pair = highest_with(&n, 2, Some(trips)).unwrap_or(Rank::Two);
            format!("full house, {} full of {}", plural(trips), plural(pair))
        }
        Category::FourOfAKind => format!("four of a kind, {}", plural(highest_with(&n, 4, None).unwrap_or(Rank::Two))),
        Category::StraightFlush => {
            let suit = flush_suit(cards).unwrap_or(Suit::Spades);
            let top = straight_top(|r| cards.iter().any(|c| c.suit() == suit && c.rank() == r))
                .unwrap_or(Rank::Five);
            if top == Rank::Ace {
                "royal flush".to_string()
            } else {
                format!("straight flush, {} high", rank_name(top))
            }
        }
    }
}

/// Two cards and nothing else: what they are called before the flop.
fn preflop_name(hole: [Card; 2]) -> String {
    let (a, b) = if hole[0].rank() >= hole[1].rank() {
        (hole[0], hole[1])
    } else {
        (hole[1], hole[0])
    };
    if a.rank() == b.rank() {
        format!("pair of {}", plural(a.rank()))
    } else if a.suit() == b.suit() {
        format!("{}-{} suited", rank_name(a.rank()), rank_name(b.rank()))
    } else {
        format!("{}-{} offsuit", rank_name(a.rank()), rank_name(b.rank()))
    }
}

/// The category the hole cards and the board make now, or before the flop
/// the category two cards can be: a pair, or a high card.
fn current_category(hole: [Card; 2], board: &[Card]) -> Category {
    if board.len() >= 3 {
        let mut all: Vec<Card> = board.to_vec();
        all.extend_from_slice(&hole);
        evaluate(&all).map(|r| r.category()).unwrap_or(Category::HighCard)
    } else if hole[0].rank() == hole[1].rank() {
        Category::OnePair
    } else {
        Category::HighCard
    }
}

/// The hand's name now.
pub fn describe(hole: [Card; 2], board: &[Card]) -> String {
    if board.len() < 3 {
        return preflop_name(hole);
    }
    let mut all: Vec<Card> = board.to_vec();
    all.extend_from_slice(&hole);
    name_of(current_category(hole, board), &all)
}

/// Every card not yet seen.
fn unseen(known: &[Card]) -> Vec<Card> {
    (0..Card::COUNT as u8)
        .filter_map(|i| Card::from_index(i).ok())
        .filter(|c| !known.contains(c))
        .collect()
}

/// The chance of each better category over the cards still to come, and the
/// horizon those cards reach.
fn improvement(hole: [Card; 2], board: &[Card]) -> (Vec<(Category, f32)>, Option<Horizon>) {
    let now = current_category(hole, board);
    let mut known: Vec<Card> = board.to_vec();
    known.extend_from_slice(&hole);
    let rest = unseen(&known);
    let mut tally = [0u32; 9];
    let mut total = 0u32;
    let mut count = |cards: &[Card]| {
        if let Some(r) = evaluate(cards) {
            let c = r.category();
            if c > now {
                tally[c as usize] += 1;
            }
        }
        total += 1;
    };
    let horizon = match board.len() {
        0 => {
            // Three cards to come, and the category at the flop is judged on
            // five cards.
            let mut five = [hole[0], hole[1], hole[0], hole[0], hole[0]];
            for i in 0..rest.len() {
                for j in (i + 1)..rest.len() {
                    for k in (j + 1)..rest.len() {
                        five[2] = rest[i];
                        five[3] = rest[j];
                        five[4] = rest[k];
                        count(&five);
                    }
                }
            }
            Some(Horizon::ByTheFlop)
        }
        3 => {
            let mut seven = [hole[0], hole[1], board[0], board[1], board[2], hole[0], hole[0]];
            for i in 0..rest.len() {
                for j in (i + 1)..rest.len() {
                    seven[5] = rest[i];
                    seven[6] = rest[j];
                    count(&seven);
                }
            }
            Some(Horizon::ByTheRiver)
        }
        4 => {
            let mut seven = [hole[0], hole[1], board[0], board[1], board[2], board[3], hole[0]];
            for c in &rest {
                seven[6] = *c;
                count(&seven);
            }
            Some(Horizon::ByTheRiver)
        }
        _ => None,
    };
    if total == 0 {
        return (Vec::new(), horizon);
    }
    let out = [
        Category::HighCard,
        Category::OnePair,
        Category::TwoPair,
        Category::ThreeOfAKind,
        Category::Straight,
        Category::Flush,
        Category::FullHouse,
        Category::FourOfAKind,
        Category::StraightFlush,
    ]
    .into_iter()
    .filter(|c| tally[*c as usize] > 0)
    .map(|c| (c, tally[c as usize] as f32 / total as f32))
    .collect();
    (out, horizon)
}

/// The hero's hand: its name, its category and its odds.
pub fn strength(hole: [Card; 2], board: &[Card]) -> Strength {
    let name = describe(hole, board);
    let category = (board.len() >= 3).then(|| current_category(hole, board));
    let (improve, horizon) = improvement(hole, board);
    let improve_total = improve.iter().map(|(_, p)| *p).sum::<f32>().min(1.0);
    Strength {
        name,
        category,
        improve,
        improve_total,
        horizon,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(r: Rank, s: Suit) -> Card {
        Card::new(r, s)
    }

    /// Two cards are named the way a player names them, high card first.
    #[test]
    fn two_cards_are_named_before_the_flop() {
        assert_eq!(describe([c(Rank::Ace, Suit::Spades), c(Rank::King, Suit::Spades)], &[]), "ace-king suited");
        assert_eq!(describe([c(Rank::King, Suit::Hearts), c(Rank::Ace, Suit::Spades)], &[]), "ace-king offsuit");
        assert_eq!(describe([c(Rank::Ace, Suit::Spades), c(Rank::Ace, Suit::Diamonds)], &[]), "pair of aces");
        assert_eq!(describe([c(Rank::Two, Suit::Clubs), c(Rank::Seven, Suit::Diamonds)], &[]), "seven-two offsuit");
    }

    /// With a board the engine's category gets the words that name the cards
    /// making it: the pair that plays, the straight's top card, the flush's
    /// high card.
    #[test]
    fn a_made_hand_is_named_by_the_cards_that_make_it() {
        let kh8h = [c(Rank::King, Suit::Hearts), c(Rank::Eight, Suit::Hearts)];
        assert_eq!(
            describe(kh8h, &[c(Rank::King, Suit::Spades), c(Rank::Eight, Suit::Diamonds), c(Rank::Three, Suit::Clubs)]),
            "two pair, kings and eights"
        );
        assert_eq!(
            describe(kh8h, &[c(Rank::King, Suit::Spades), c(Rank::Two, Suit::Diamonds), c(Rank::Three, Suit::Clubs)]),
            "pair of kings"
        );
        assert_eq!(
            describe(kh8h, &[c(Rank::Ace, Suit::Spades), c(Rank::Two, Suit::Diamonds), c(Rank::Three, Suit::Clubs)]),
            "ace high"
        );
        assert_eq!(
            describe(
                [c(Rank::Ace, Suit::Clubs), c(Rank::Four, Suit::Clubs)],
                &[c(Rank::Two, Suit::Diamonds), c(Rank::Three, Suit::Diamonds), c(Rank::Five, Suit::Spades)]
            ),
            "straight, five high"
        );
        assert_eq!(
            describe(
                [c(Rank::Queen, Suit::Spades), c(Rank::Nine, Suit::Spades)],
                &[c(Rank::Two, Suit::Spades), c(Rank::Seven, Suit::Spades), c(Rank::Jack, Suit::Spades), c(Rank::Ace, Suit::Hearts)]
            ),
            "flush, queen high"
        );
        assert_eq!(
            describe(kh8h, &[c(Rank::King, Suit::Spades), c(Rank::King, Suit::Diamonds), c(Rank::Eight, Suit::Clubs)]),
            "full house, kings full of eights"
        );
        assert_eq!(
            describe(kh8h, &[c(Rank::King, Suit::Spades), c(Rank::King, Suit::Diamonds), c(Rank::King, Suit::Clubs)]),
            "four of a kind, kings"
        );
        assert_eq!(
            describe(
                [c(Rank::Ace, Suit::Spades), c(Rank::King, Suit::Spades)],
                &[c(Rank::Queen, Suit::Spades), c(Rank::Jack, Suit::Spades), c(Rank::Ten, Suit::Spades)]
            ),
            "royal flush"
        );
        assert_eq!(
            describe(
                [c(Rank::Six, Suit::Spades), c(Rank::Five, Suit::Spades)],
                &[c(Rank::Four, Suit::Spades), c(Rank::Three, Suit::Spades), c(Rank::Two, Suit::Spades), c(Rank::Ace, Suit::Hearts), c(Rank::Ace, Suit::Clubs)]
            ),
            "straight flush, six high"
        );
        assert_eq!(
            describe(kh8h, &[c(Rank::Nine, Suit::Spades), c(Rank::Nine, Suit::Diamonds), c(Rank::Nine, Suit::Clubs)]),
            "three of a kind, nines"
        );
    }

    /// The odds are the enumeration's, not a rule of thumb: a flush draw on
    /// the flop makes its flush by the river in 378 of 1 081 ways -- and one
    /// of those is the royal flush, which is counted where it lands.
    #[test]
    fn a_flush_draw_on_the_flop_has_its_exact_odds() {
        let s = strength(
            [c(Rank::Ace, Suit::Spades), c(Rank::King, Suit::Spades)],
            &[c(Rank::Queen, Suit::Spades), c(Rank::Seven, Suit::Spades), c(Rank::Two, Suit::Diamonds)],
        );
        assert_eq!(s.name, "ace high");
        assert_eq!(s.horizon, Some(Horizon::ByTheRiver));
        let flush = s.improve.iter().find(|(k, _)| *k == Category::Flush).map(|(_, p)| *p).unwrap();
        let royal = s.improve.iter().find(|(k, _)| *k == Category::StraightFlush).map(|(_, p)| *p).unwrap();
        assert!((flush - 377.0 / 1081.0).abs() < 1e-4, "flush {flush}");
        assert!((royal - 1.0 / 1081.0).abs() < 1e-4, "royal {royal}");
        assert!(s.improve.iter().any(|(k, _)| *k == Category::OnePair), "a pair is an improvement on ace high");
        assert!(s.improve_total > flush && s.improve_total <= 1.0);
        // Best last, so the list reads upward.
        assert!(s.improve.windows(2).all(|w| w[0].0 < w[1].0));
    }

    /// On the turn there is one card to come and nine of forty-six make it.
    #[test]
    fn a_flush_draw_on_the_turn_is_nine_of_forty_six() {
        let s = strength(
            [c(Rank::Ace, Suit::Spades), c(Rank::Nine, Suit::Spades)],
            &[c(Rank::Queen, Suit::Spades), c(Rank::Seven, Suit::Spades), c(Rank::Two, Suit::Diamonds), c(Rank::Three, Suit::Clubs)],
        );
        let flush = s.improve.iter().find(|(k, _)| *k == Category::Flush).map(|(_, p)| *p).unwrap();
        assert!((flush - 9.0 / 46.0).abs() < 1e-4, "flush {flush}");
    }

    /// On the river nothing is to come, and before the flop the odds run to
    /// the flop and a pair improves less often than an unpaired hand does.
    #[test]
    fn the_horizon_is_the_cards_still_to_come() {
        let river = strength(
            [c(Rank::Ace, Suit::Spades), c(Rank::King, Suit::Spades)],
            &[c(Rank::Queen, Suit::Spades), c(Rank::Seven, Suit::Spades), c(Rank::Two, Suit::Diamonds), c(Rank::Three, Suit::Clubs), c(Rank::Four, Suit::Clubs)],
        );
        assert!(river.improve.is_empty());
        assert_eq!(river.horizon, None);
        assert_eq!(river.improve_total, 0.0);

        let aces = strength([c(Rank::Ace, Suit::Spades), c(Rank::Ace, Suit::Diamonds)], &[]);
        assert_eq!(aces.horizon, Some(Horizon::ByTheFlop));
        assert_eq!(aces.category, None, "no category before there is a board");
        // Three of a kind or better on the flop: at least one more ace among
        // the three cards (C(50,3) - C(48,3) = 2 304 flops), or a board of
        // three of one rank under the aces, which is a full house without an
        // ace (12 ranks, 4 ways each: 48 flops). 2 352 of 19 600.
        let at_least_a_set: f32 = aces
            .improve
            .iter()
            .filter(|(k, _)| *k >= Category::ThreeOfAKind)
            .map(|(_, p)| *p)
            .sum();
        assert!(
            (at_least_a_set - 2_352.0 / 19_600.0).abs() < 1e-4,
            "set or better {at_least_a_set}"
        );
        assert!(aces.improve.iter().any(|(k, _)| *k == Category::FullHouse), "a paired board fills up");
        assert!(!aces.improve.iter().any(|(k, _)| *k == Category::OnePair), "a pair is not an improvement on a pair");

        let unpaired = strength([c(Rank::Ace, Suit::Spades), c(Rank::King, Suit::Diamonds)], &[]);
        assert!(unpaired.improve_total > aces.improve_total);
        assert!(unpaired.improve.iter().any(|(k, _)| *k == Category::OnePair));
    }

    /// A hand that cannot improve says so with an empty list rather than a
    /// zero.
    #[test]
    fn the_best_hand_has_nothing_to_improve_to() {
        let s = strength(
            [c(Rank::Ace, Suit::Spades), c(Rank::King, Suit::Spades)],
            &[c(Rank::Queen, Suit::Spades), c(Rank::Jack, Suit::Spades), c(Rank::Ten, Suit::Spades)],
        );
        assert_eq!(s.name, "royal flush");
        assert!(s.improve.is_empty());
        assert_eq!(s.improve_total, 0.0);
    }

    /// Every category has a name a player would say.
    #[test]
    fn every_category_has_words() {
        for c in [
            Category::HighCard,
            Category::OnePair,
            Category::TwoPair,
            Category::ThreeOfAKind,
            Category::Straight,
            Category::Flush,
            Category::FullHouse,
            Category::FourOfAKind,
            Category::StraightFlush,
        ] {
            assert!(!category_name(c).is_empty());
        }
    }
}
