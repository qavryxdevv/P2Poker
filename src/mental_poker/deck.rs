//! The deck-index map: which of the 52 positions is whose, fixed before the
//! shuffle chain starts.
//!
//! `PROTOCOL.md` §4.5 owns this map and no other document restates it. The map
//! must be fixed *before* anyone shuffles, because otherwise a malicious last
//! shuffler argues after the fact about which index is "the button's first hole
//! card" — and by then it knows the deck.
//!
//! It is a pure function of the hand's own state, so there is nothing in it to
//! manipulate: given `button_position` and who is dealt in, every peer computes
//! the same map or has a different idea of the hand.
//!
//! # No burn cards
//!
//! A burn defeats physical marked-card and edge-sorting attacks, and there are
//! no physical cards here. A burn that is never opened is indistinguishable
//! from an unused index, so it consumes a deck position and adds a place to get
//! the map wrong, for nothing. That is entry 3 in the deviation register of
//! `THREAT_MODEL.md` §9.1.1 and it is not a per-implementation choice: a burn
//! changes this map, hence `index_map_hash`, hence `DECK_COMMIT`, so a client
//! that burned would mismatch every conforming client every hand.

use crate::poker::state::{Hash, SeatIdx};
use crate::protocol::constants::MAX_SEATS;
use crate::protocol::serialization::h;
use crate::protocol::signatures::Domain;

/// What a deck index is for.
///
/// The numeric codes are `PROTOCOL.md` §4.5's `role_code` and they enter
/// `index_map_hash`, so they are wire-visible and may never be renumbered
/// without a protocol version. Code 0 is reserved and never emitted, so a
/// zeroed buffer is not a valid map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    HoleFirst(SeatIdx),
    HoleSecond(SeatIdx),
    Flop,
    Turn,
    River,
}

impl Role {
    /// §4.5's `role_code`.
    pub const fn code(self) -> u8 {
        match self {
            Role::HoleFirst(_) => 1,
            Role::HoleSecond(_) => 2,
            Role::Flop => 3,
            Role::Turn => 4,
            Role::River => 5,
        }
    }

    /// §4.5's `owner_seat_or_0xFF`: the seat's index at the table — not its
    /// position in deal order — or `0xFF` for a board card.
    pub const fn owner(self) -> u8 {
        match self {
            Role::HoleFirst(s) | Role::HoleSecond(s) => s,
            Role::Flop | Role::Turn | Role::River => 0xFF,
        }
    }

    /// Whether this index is a board card, which is the whole of the difference
    /// between a token every seat may publish and one only its owner may hold.
    pub const fn is_board(self) -> bool {
        matches!(self, Role::Flop | Role::Turn | Role::River)
    }
}

/// A deck index that came from this map, never from the wire.
///
/// The crypto boundary takes one of these rather than a `u8` so that "index 12"
/// cannot be a number a sender chose. It carries no bounds of its own: it is
/// minted by [`DeckIndexMap`] and by nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CardIndex(u8);

impl CardIndex {
    /// The raw index, for handing to the deck library.
    pub const fn get(self) -> u8 {
        self.0
    }

    /// A bare deck **position**, for tests only.
    ///
    /// Gated on `cfg(test)` rather than merely kept private, because in the
    /// running client there is no such thing: every index comes from
    /// [`DeckIndexMap`], and an index the hand gave no role to is one no token
    /// is ever legal for.
    ///
    /// What it is for is the one property that belongs to the deck rather than
    /// to any hand - that a shuffled deck is a permutation of the open one, all
    /// fifty-two positions of it - which cannot be stated through a map that
    /// only ever mints `2m + 5`.
    #[cfg(test)]
    pub(crate) const fn position(index: u8, deck_len: usize) -> Option<Self> {
        if (index as usize) < deck_len {
            Some(CardIndex(index))
        } else {
            None
        }
    }
}

/// Why a map could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapError {
    /// Fewer than two seats are dealt in, which is not a hand.
    TooFewSeats { m: usize },
    /// More dealt-in seats than the table can hold.
    TooManySeats { m: usize },
    /// `2m + 5` exceeds the deck.
    ///
    /// Unreachable at `MAX_SEATS = 10` and checked anyway, because it is the
    /// arithmetic that decides whether the river index exists and it must not
    /// become true silently if the table limit ever moves.
    DeckTooSmall { need: usize, have: usize },
    /// The button is not a seat at this table.
    ButtonOutOfRange { button: SeatIdx },
}

/// The map from deck index to role, for one hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckIndexMap {
    /// `D = [d_0, …, d_{m-1}]`: dealt-in seats clockwise from the first one
    /// strictly clockwise of the button. Small blind first — normal deal order.
    order: Vec<SeatIdx>,
    seat_count: u8,
}

impl DeckIndexMap {
    /// Build the map for a hand.
    ///
    /// `dealt_in` is indexed by seat and is the table's own width, so a seat
    /// that busted out of the tournament is `false` rather than absent.
    pub fn build(dealt_in: &[bool], button: SeatIdx, deck_len: usize) -> Result<Self, MapError> {
        let seat_count = dealt_in.len();
        if seat_count > MAX_SEATS as usize {
            return Err(MapError::TooManySeats { m: seat_count });
        }
        if button as usize >= seat_count {
            return Err(MapError::ButtonOutOfRange { button });
        }

        let seat_count = seat_count as u8;
        let order: Vec<SeatIdx> = (1..=seat_count)
            .map(|o| (button + o) % seat_count)
            .filter(|s| dealt_in[*s as usize])
            .collect();

        let m = order.len();
        if m < 2 {
            return Err(MapError::TooFewSeats { m });
        }
        let need = 2 * m + 5;
        if need > deck_len {
            return Err(MapError::DeckTooSmall { need, have: deck_len });
        }

        Ok(DeckIndexMap { order, seat_count })
    }

    /// `m`, the dealt-in count. The symbol is `m` in every document of the
    /// corpus and it is `m` here.
    pub fn m(&self) -> usize {
        self.order.len()
    }

    /// Deal order, small blind first.
    pub fn deal_order(&self) -> &[SeatIdx] {
        &self.order
    }

    /// The role of a deck index, or `None` if the index is unused this hand.
    ///
    /// No token for an unused index is ever legal, which is why this is an
    /// `Option` rather than a role meaning "spare".
    pub fn role(&self, index: u8) -> Option<Role> {
        let m = self.m();
        let i = index as usize;
        if i < m {
            Some(Role::HoleFirst(self.order[i]))
        } else if i < 2 * m {
            Some(Role::HoleSecond(self.order[i - m]))
        } else if i < 2 * m + 3 {
            Some(Role::Flop)
        } else if i == 2 * m + 3 {
            Some(Role::Turn)
        } else if i == 2 * m + 4 {
            Some(Role::River)
        } else {
            None
        }
    }

    /// The two hole-card indices of a seat, if it is dealt in.
    pub fn hole_cards(&self, seat: SeatIdx) -> Option<[CardIndex; 2]> {
        let j = self.order.iter().position(|&s| s == seat)?;
        let m = self.m();
        Some([CardIndex(j as u8), CardIndex((j + m) as u8)])
    }

    /// The three flop indices.
    pub fn flop(&self) -> [CardIndex; 3] {
        let b = 2 * self.m();
        [
            CardIndex(b as u8),
            CardIndex((b + 1) as u8),
            CardIndex((b + 2) as u8),
        ]
    }

    pub fn turn(&self) -> CardIndex {
        CardIndex((2 * self.m() + 3) as u8)
    }

    pub fn river(&self) -> CardIndex {
        CardIndex((2 * self.m() + 4) as u8)
    }

    /// Mint an index from a number that arrived from outside.
    ///
    /// The only path from a wire `u8` to a [`CardIndex`], and it refuses every
    /// index this hand does not use — so a peer cannot ask for a token on an
    /// index that has no role.
    pub fn index_from_wire(&self, index: u8) -> Option<CardIndex> {
        self.role(index).map(|_| CardIndex(index))
    }

    /// `index_map_hash` of `PROTOCOL.md` §4.5.
    ///
    /// One part per §2.8's hasher: `u8(m)`, then for each used index the triple
    /// `u8(i) || u8(role_code) || u8(owner_or_0xFF)`. The separator is the
    /// length prefix and there is no other; the triple is one part, exactly as
    /// the construction writes it.
    pub fn index_map_hash(&self) -> Hash {
        let m = self.m();
        let mut triples: Vec<[u8; 3]> = Vec::with_capacity(2 * m + 5);
        for i in 0..(2 * m + 5) {
            let role = self
                .role(i as u8)
                .expect("every index below 2m+5 has a role by construction");
            triples.push([i as u8, role.code(), role.owner()]);
        }

        let m_byte = [m as u8];
        let mut parts: Vec<&[u8]> = Vec::with_capacity(1 + triples.len());
        parts.push(&m_byte);
        for t in &triples {
            parts.push(t);
        }
        h(Domain::DeckCommit.context(), &parts)
    }

    /// The table's width, which the map needs in order to say what a seat index
    /// means.
    pub fn seat_count(&self) -> u8 {
        self.seat_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_in(n: usize) -> Vec<bool> {
        vec![true; n]
    }

    /// Deal order is small blind first, which at a full table is the seat
    /// immediately clockwise of the button.
    #[test]
    fn deal_order_starts_left_of_the_button() {
        let map = DeckIndexMap::build(&all_in(6), 2, 52).unwrap();
        assert_eq!(map.deal_order(), &[3, 4, 5, 0, 1, 2]);
        assert_eq!(map.m(), 6);
    }

    /// Sitting-out seats are skipped, and the order still starts from the first
    /// dealt-in seat strictly clockwise of the button.
    #[test]
    fn seats_not_dealt_in_are_skipped() {
        let dealt = vec![true, false, true, false, false, true];
        let map = DeckIndexMap::build(&dealt, 0, 52).unwrap();
        assert_eq!(map.deal_order(), &[2, 5, 0]);
        assert_eq!(map.m(), 3);
    }

    /// The layout of §4.5, index by index.
    #[test]
    fn the_layout_is_the_one_the_protocol_writes_down() {
        let map = DeckIndexMap::build(&all_in(3), 0, 52).unwrap();
        // D = [1, 2, 0], m = 3
        assert_eq!(map.role(0), Some(Role::HoleFirst(1)));
        assert_eq!(map.role(1), Some(Role::HoleFirst(2)));
        assert_eq!(map.role(2), Some(Role::HoleFirst(0)));
        assert_eq!(map.role(3), Some(Role::HoleSecond(1)));
        assert_eq!(map.role(4), Some(Role::HoleSecond(2)));
        assert_eq!(map.role(5), Some(Role::HoleSecond(0)));
        assert_eq!(map.role(6), Some(Role::Flop));
        assert_eq!(map.role(7), Some(Role::Flop));
        assert_eq!(map.role(8), Some(Role::Flop));
        assert_eq!(map.role(9), Some(Role::Turn));
        assert_eq!(map.role(10), Some(Role::River));
        assert_eq!(map.role(11), None, "no burn card, and no spare role");
        assert_eq!(map.role(51), None);
    }

    /// A player's two cards are `j` and `j + m` — one round of dealing, then a
    /// second — and the accessors agree with the index map.
    #[test]
    fn a_seat_holds_j_and_j_plus_m() {
        let map = DeckIndexMap::build(&all_in(6), 4, 52).unwrap();
        for (j, &seat) in map.deal_order().iter().enumerate() {
            let [a, b] = map.hole_cards(seat).unwrap();
            assert_eq!(a.get() as usize, j);
            assert_eq!(b.get() as usize, j + map.m());
            assert_eq!(map.role(a.get()), Some(Role::HoleFirst(seat)));
            assert_eq!(map.role(b.get()), Some(Role::HoleSecond(seat)));
        }
    }

    #[test]
    fn a_seat_that_is_not_dealt_in_holds_nothing() {
        let dealt = vec![true, false, true, true];
        let map = DeckIndexMap::build(&dealt, 0, 52).unwrap();
        assert_eq!(map.hole_cards(1), None);
        assert_eq!(map.hole_cards(9), None, "not even a seat at this table");
    }

    #[test]
    fn the_board_indices_follow_the_hole_cards() {
        let map = DeckIndexMap::build(&all_in(9), 0, 52).unwrap();
        let m = map.m();
        assert_eq!(
            map.flop().map(|c| c.get() as usize),
            [2 * m, 2 * m + 1, 2 * m + 2]
        );
        assert_eq!(map.turn().get() as usize, 2 * m + 3);
        assert_eq!(map.river().get() as usize, 2 * m + 4);
        assert_eq!(map.role(map.river().get()), Some(Role::River));
    }

    /// Every index used by the hand is claimed exactly once. A map that
    /// overlapped would hand two players one ciphertext and neither would ever
    /// find out from the proofs.
    #[test]
    fn no_index_is_claimed_twice() {
        for seats in 2..=MAX_SEATS as usize {
            for button in 0..seats as u8 {
                let map = DeckIndexMap::build(&all_in(seats), button, 52).unwrap();
                let used: Vec<u8> = (0..52u8).filter(|&i| map.role(i).is_some()).collect();
                assert_eq!(used.len(), 2 * map.m() + 5);

                let mut owners: Vec<(u8, u8)> = used
                    .iter()
                    .map(|&i| {
                        let r = map.role(i).unwrap();
                        (r.code(), r.owner())
                    })
                    .collect();
                owners.sort_unstable();
                let before = owners.len();
                owners.dedup();
                assert_eq!(
                    owners.len(),
                    before - 2, // the two extra flop cards share (3, 0xFF)
                    "only the flop repeats a (role, owner) pair"
                );
            }
        }
    }

    /// Only the owner's own two indices are hole cards of that owner, and the
    /// board belongs to nobody.
    #[test]
    fn the_board_has_no_owner() {
        let map = DeckIndexMap::build(&all_in(4), 1, 52).unwrap();
        for c in map.flop() {
            assert_eq!(map.role(c.get()).unwrap().owner(), 0xFF);
            assert!(map.role(c.get()).unwrap().is_board());
        }
        assert_eq!(map.role(map.turn().get()).unwrap().owner(), 0xFF);
        assert!(!map.role(0).unwrap().is_board());
    }

    /// The only path from a wire byte to an index, and it refuses everything
    /// this hand does not use.
    #[test]
    fn an_unused_index_cannot_be_minted_from_the_wire() {
        let map = DeckIndexMap::build(&all_in(2), 0, 52).unwrap();
        assert_eq!(map.index_from_wire(0).map(|c| c.get()), Some(0));
        assert_eq!(map.index_from_wire(8).map(|c| c.get()), Some(8)); // river
        assert_eq!(map.index_from_wire(9), None);
        assert_eq!(map.index_from_wire(255), None);
    }

    /// The map is a pure function of the hand's state: same inputs, same hash,
    /// on every peer.
    #[test]
    fn the_hash_is_a_function_of_the_hand_alone() {
        let a = DeckIndexMap::build(&all_in(6), 3, 52).unwrap();
        let b = DeckIndexMap::build(&all_in(6), 3, 52).unwrap();
        assert_eq!(a.index_map_hash(), b.index_map_hash());
    }

    /// Two different hands must not share a map hash, or `DECK_COMMIT` stops
    /// distinguishing them.
    #[test]
    fn a_different_button_or_field_gives_a_different_hash() {
        let base = DeckIndexMap::build(&all_in(6), 0, 52)
            .unwrap()
            .index_map_hash();
        let moved = DeckIndexMap::build(&all_in(6), 1, 52)
            .unwrap()
            .index_map_hash();
        assert_ne!(base, moved, "the button moved, so the map moved");

        let dealt = vec![true, true, true, true, true, false];
        let fewer = DeckIndexMap::build(&dealt, 0, 52).unwrap().index_map_hash();
        assert_ne!(base, fewer, "a seat sat out");
    }

    /// The codes are wire-visible and enter `index_map_hash`, so they are
    /// pinned. Renumbering them is a protocol version, not a refactor.
    ///
    /// They were undefined in `PROTOCOL.md` for seven passes: the construction
    /// reads complete, so nobody asked. Two conforming clients picking two
    /// reasonable encodings mismatch `DECK_COMMIT` every hand.
    #[test]
    fn the_role_codes_are_the_protocols_role_codes() {
        assert_eq!(Role::HoleFirst(0).code(), 1);
        assert_eq!(Role::HoleSecond(0).code(), 2);
        assert_eq!(Role::Flop.code(), 3);
        assert_eq!(Role::Turn.code(), 4);
        assert_eq!(Role::River.code(), 5);

        let codes = [
            Role::HoleFirst(0).code(),
            Role::HoleSecond(0).code(),
            Role::Flop.code(),
            Role::Turn.code(),
            Role::River.code(),
        ];
        assert!(
            !codes.contains(&0),
            "0 is reserved: a zeroed buffer is not a map"
        );
    }

    /// The owner is the seat at the table, not the position in deal order. They
    /// differ whenever the button is not at seat 0, and confusing them would
    /// give a peer somebody else's hole cards while every hash still agreed.
    #[test]
    fn the_owner_is_the_seat_not_the_deal_position() {
        let map = DeckIndexMap::build(&all_in(6), 3, 52).unwrap();
        assert_eq!(map.deal_order()[0], 4);
        assert_eq!(map.role(0).unwrap().owner(), 4, "seat 4, not position 0");
    }

    #[test]
    fn a_hand_needs_two_seats() {
        let dealt = vec![true, false, false];
        assert_eq!(
            DeckIndexMap::build(&dealt, 0, 52),
            Err(MapError::TooFewSeats { m: 1 })
        );
        assert_eq!(
            DeckIndexMap::build(&[false, false], 0, 52),
            Err(MapError::TooFewSeats { m: 0 })
        );
    }

    #[test]
    fn a_table_wider_than_the_protocol_allows_is_refused() {
        assert_eq!(
            DeckIndexMap::build(&all_in(11), 0, 52),
            Err(MapError::TooManySeats { m: 11 })
        );
    }

    #[test]
    fn the_button_must_be_a_seat() {
        assert_eq!(
            DeckIndexMap::build(&all_in(6), 6, 52),
            Err(MapError::ButtonOutOfRange { button: 6 })
        );
    }

    /// Unreachable at ten seats and checked anyway: it is the arithmetic that
    /// decides whether the river index exists.
    #[test]
    fn a_deck_too_small_for_the_table_is_refused() {
        assert_eq!(
            DeckIndexMap::build(&all_in(10), 0, 20),
            Err(MapError::DeckTooSmall { need: 25, have: 20 })
        );
        assert!(DeckIndexMap::build(&all_in(10), 0, 52).is_ok());
    }
}
