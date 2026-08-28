//! The canonical state types (`docs/STATE_MACHINE.md` section 2).
//!
//! Only the primitives are here so far; `TableConfig`, `Seat` and `TableState`
//! land with the transition function.
//!
//! Everything in this file is **canonical**: these values are serialised,
//! hashed into the transcript and signed, so their encodings are part of the
//! wire format. Changing one is a protocol break, not a refactor.

use core::fmt;

/// A seat position, `0 .. config.seats - 1`, treated as a ring.
pub type SeatIdx = u8;

/// A position in the shuffled deck, `0..=51`. Not a card - the mapping from
/// deck position to card is what the cryptographic layer hides.
pub type CardIndex = u8;

/// Chips, in one denomination whose smallest unit is 1.
///
/// Integers only. `docs/research/CRYPTO_LIBS.md` forbids floats in any signed
/// or hashed type, and the odd-chip rule is defined over integer remainder so
/// that no rounding decision ever exists.
pub type Chips = u64;

/// BLAKE3-256 output.
pub type Hash = [u8; 32];

/// An Ed25519 application signing public key.
///
/// Deliberately not a libp2p `PeerId`: section 20 keeps the two identities
/// apart, because a `PeerId` says which socket you are talking to, not who is
/// playing.
pub type PlayerId = [u8; 32];

/// A table identifier.
pub type TableId = [u8; 32];

/// The four suits, in canonical order.
///
/// The discriminants are part of the wire format via [`Card`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Suit {
    Clubs = 0,
    Diamonds = 1,
    Hearts = 2,
    Spades = 3,
}

impl Suit {
    pub const ALL: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];

    /// The single lowercase letter used in hand histories and in tests.
    pub const fn letter(self) -> char {
        match self {
            Suit::Clubs => 'c',
            Suit::Diamonds => 'd',
            Suit::Hearts => 'h',
            Suit::Spades => 's',
        }
    }
}

/// The thirteen ranks, lowest first.
///
/// `Two = 0` through `Ace = 12`. The ordering is the poker ordering for every
/// purpose except the wheel straight (A-2-3-4-5), which the evaluator handles
/// as a special case rather than by giving the ace two values here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Rank {
    Two = 0,
    Three = 1,
    Four = 2,
    Five = 3,
    Six = 4,
    Seven = 5,
    Eight = 6,
    Nine = 7,
    Ten = 8,
    Jack = 9,
    Queen = 10,
    King = 11,
    Ace = 12,
}

impl Rank {
    pub const ALL: [Rank; 13] = [
        Rank::Two,
        Rank::Three,
        Rank::Four,
        Rank::Five,
        Rank::Six,
        Rank::Seven,
        Rank::Eight,
        Rank::Nine,
        Rank::Ten,
        Rank::Jack,
        Rank::Queen,
        Rank::King,
        Rank::Ace,
    ];

    /// The character used in hand histories: `2`-`9`, then `T J Q K A`.
    pub const fn symbol(self) -> char {
        match self {
            Rank::Two => '2',
            Rank::Three => '3',
            Rank::Four => '4',
            Rank::Five => '5',
            Rank::Six => '6',
            Rank::Seven => '7',
            Rank::Eight => '8',
            Rank::Nine => '9',
            Rank::Ten => 'T',
            Rank::Jack => 'J',
            Rank::Queen => 'Q',
            Rank::King => 'K',
            Rank::Ace => 'A',
        }
    }
}

/// One of the fifty-two cards, encoded as `rank * 4 + suit`.
///
/// **This encoding is canonical and may never change.** It is what the
/// cryptographic layer maps deck positions onto, and it is hashed into the
/// transcript, so a different encoding is a different protocol.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Card(u8);

/// A byte outside `0..=51` was offered as a card.
///
/// This is a real network condition, not an internal bug: section 17 assumes a
/// fully modified peer that emits arbitrary bytes, so the constructor returns
/// an error rather than panicking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidCard(pub u8);

impl fmt::Display for InvalidCard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "card index {} is outside 0..=51", self.0)
    }
}

impl std::error::Error for InvalidCard {}

impl Card {
    /// Every card, in canonical order.
    pub const COUNT: usize = 52;

    /// Build a card from its rank and suit.
    pub const fn new(rank: Rank, suit: Suit) -> Self {
        Card((rank as u8) * 4 + suit as u8)
    }

    /// Build a card from its canonical index, rejecting anything out of range.
    pub const fn from_index(index: u8) -> Result<Self, InvalidCard> {
        if index < Self::COUNT as u8 {
            Ok(Card(index))
        } else {
            Err(InvalidCard(index))
        }
    }

    /// The canonical index, `0..=51`.
    pub const fn index(self) -> u8 {
        self.0
    }

    pub const fn rank(self) -> Rank {
        // Safe by construction: `self.0 < 52`, so `self.0 / 4 < 13`.
        Rank::ALL[(self.0 / 4) as usize]
    }

    pub const fn suit(self) -> Suit {
        Suit::ALL[(self.0 % 4) as usize]
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.rank().symbol(), self.suit().letter())
    }
}

impl fmt::Debug for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // A card is far easier to read as `Ah` than as `Card(50)`, and this
        // type appears in every failing assertion in the engine tests.
        write!(f, "{self}")
    }
}

/// The four betting rounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Street {
    PreFlop = 0,
    Flop = 1,
    Turn = 2,
    River = 3,
}

impl Street {
    /// How many board cards are face up once this street has been dealt.
    pub const fn board_cards(self) -> usize {
        match self {
            Street::PreFlop => 0,
            Street::Flop => 3,
            Street::Turn => 4,
            Street::River => 5,
        }
    }

    /// The next street, or `None` after the river.
    pub const fn next(self) -> Option<Street> {
        match self {
            Street::PreFlop => Some(Street::Flop),
            Street::Flop => Some(Street::Turn),
            Street::Turn => Some(Street::River),
            Street::River => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_index_round_trips_for_every_card() {
        for i in 0..Card::COUNT as u8 {
            let card = Card::from_index(i).expect("0..=51 is in range");
            assert_eq!(card.index(), i);
            assert_eq!(Card::new(card.rank(), card.suit()), card);
        }
    }

    #[test]
    fn the_canonical_encoding_is_rank_times_four_plus_suit() {
        // Pinned deliberately. These values are hashed into the transcript, so
        // a change here is a protocol break and this test is the tripwire.
        assert_eq!(Card::new(Rank::Two, Suit::Clubs).index(), 0);
        assert_eq!(Card::new(Rank::Two, Suit::Spades).index(), 3);
        assert_eq!(Card::new(Rank::Three, Suit::Clubs).index(), 4);
        assert_eq!(Card::new(Rank::Ace, Suit::Spades).index(), 51);
    }

    #[test]
    fn out_of_range_indices_are_rejected_not_panicked_on() {
        // Section 17 assumes a peer that emits arbitrary bytes.
        assert_eq!(Card::from_index(52), Err(InvalidCard(52)));
        assert_eq!(Card::from_index(255), Err(InvalidCard(255)));
    }

    #[test]
    fn every_card_is_distinct() {
        let mut seen = std::collections::HashSet::new();
        for i in 0..Card::COUNT as u8 {
            let card = Card::from_index(i).unwrap();
            assert!(seen.insert((card.rank(), card.suit())), "{card} repeated");
        }
        assert_eq!(seen.len(), Card::COUNT);
    }

    #[test]
    fn cards_display_the_way_hand_histories_write_them() {
        assert_eq!(Card::new(Rank::Ace, Suit::Hearts).to_string(), "Ah");
        assert_eq!(Card::new(Rank::Ten, Suit::Diamonds).to_string(), "Td");
        assert_eq!(Card::new(Rank::Two, Suit::Clubs).to_string(), "2c");
    }

    #[test]
    fn streets_run_preflop_to_river_and_stop() {
        assert_eq!(Street::PreFlop.next(), Some(Street::Flop));
        assert_eq!(Street::River.next(), None);
        assert_eq!(Street::PreFlop.board_cards(), 0);
        assert_eq!(Street::River.board_cards(), 5);
    }
}
