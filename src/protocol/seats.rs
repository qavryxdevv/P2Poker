//! `SeatSet`: which seats a stage was required of, and which have been heard.
//!
//! A bitset over `MAX_SEATS`, not a `Vec` and not a container fed from the
//! network. `SPEC_CS.md` §27 forbids the second, and the first would put an
//! allocation on a path a sender can reach.
//!
//! The whole of the emitter-set machinery — `PROTOCOL.md` §3.2's collective
//! stages, D-013's liveness rule, the checkpoint records of §6.2 — is subset
//! tests and unions over sets of at most ten members. As a `u16` those are one
//! instruction each and the set has one representation, so two peers holding the
//! same members hold the same bytes.

use crate::protocol::constants::MAX_SEATS;

/// A set of seats at one table.
///
/// The bit for seat `s` is `1 << s`. Seats at or above [`MAX_SEATS`] have no
/// bit and cannot be inserted, which is where a seat index that arrived from the
/// network stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord, Hash)]
pub struct SeatSet(u16);

/// A seat index outside the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatOutOfRange {
    pub seat: u8,
}

impl SeatSet {
    /// The empty set.
    pub const EMPTY: SeatSet = SeatSet(0);

    /// Build from seat indices, refusing any outside the table.
    pub fn from_seats(seats: &[u8]) -> Result<Self, SeatOutOfRange> {
        let mut set = SeatSet::EMPTY;
        for &s in seats {
            set.insert(s)?;
        }
        Ok(set)
    }

    /// Add a seat.
    ///
    /// Returns an error rather than silently ignoring an out-of-range index:
    /// a seat number is a field of a signed message, and a set that quietly
    /// dropped one would make two peers disagree about whether a stage is
    /// complete while both believed they agreed.
    pub fn insert(&mut self, seat: u8) -> Result<(), SeatOutOfRange> {
        if seat >= MAX_SEATS {
            return Err(SeatOutOfRange { seat });
        }
        self.0 |= 1 << seat;
        Ok(())
    }

    pub fn contains(&self, seat: u8) -> bool {
        seat < MAX_SEATS && (self.0 & (1 << seat)) != 0
    }

    /// Whether this set contains every member of `other`.
    ///
    /// The test every collective stage is completed by: `heard ⊇ required`.
    pub fn is_superset_of(&self, other: SeatSet) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn union(self, other: SeatSet) -> SeatSet {
        SeatSet(self.0 | other.0)
    }

    pub fn len(&self) -> usize {
        self.0.count_ones() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// The one seat in the set, if it has exactly one.
    ///
    /// The solitary-regime test is about a set of size one, and it is asked for
    /// often enough that the alternative would be `len() == 1` followed by a
    /// search — two places to get the same question wrong.
    pub fn solitary(&self) -> Option<u8> {
        if self.len() == 1 {
            Some(self.0.trailing_zeros() as u8)
        } else {
            None
        }
    }

    /// The members, ascending.
    pub fn iter(&self) -> impl Iterator<Item = u8> + '_ {
        (0..MAX_SEATS).filter(move |&s| self.contains(s))
    }

    /// The raw bits, for hashing into a transcript.
    ///
    /// The set has one representation, so this is canonical: two peers holding
    /// the same members produce the same bytes without agreeing on an ordering
    /// first.
    pub fn bits(&self) -> u16 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_holds_what_was_put_in_it() {
        let s = SeatSet::from_seats(&[0, 3, 9]).unwrap();
        assert_eq!(s.len(), 3);
        assert!(s.contains(0) && s.contains(3) && s.contains(9));
        assert!(!s.contains(1));
        assert_eq!(s.iter().collect::<Vec<_>>(), vec![0, 3, 9]);
    }

    /// A seat number is a field of a signed message. A set that dropped an
    /// out-of-range one would let two peers disagree about whether a stage is
    /// complete while each believed the other agreed.
    #[test]
    fn a_seat_outside_the_table_is_refused_and_not_ignored() {
        assert_eq!(
            SeatSet::from_seats(&[0, MAX_SEATS]),
            Err(SeatOutOfRange { seat: MAX_SEATS })
        );
        assert_eq!(
            SeatSet::from_seats(&[255]),
            Err(SeatOutOfRange { seat: 255 })
        );

        let mut s = SeatSet::EMPTY;
        assert!(s.insert(MAX_SEATS - 1).is_ok());
        assert!(s.insert(MAX_SEATS).is_err());
        assert_eq!(s.len(), 1, "the refused seat is not in the set");
    }

    /// The test every collective stage completes on.
    #[test]
    fn superset_is_the_stage_completion_test() {
        let required = SeatSet::from_seats(&[1, 2, 3]).unwrap();
        let mut heard = SeatSet::from_seats(&[1, 2]).unwrap();
        assert!(!heard.is_superset_of(required));

        heard.insert(3).unwrap();
        assert!(heard.is_superset_of(required));

        // A seat outside the required set does not complete it, and does not
        // stop it being complete either.
        heard.insert(7).unwrap();
        assert!(heard.is_superset_of(required));

        assert!(
            SeatSet::EMPTY.is_superset_of(SeatSet::EMPTY),
            "an empty requirement is met by anything, including nothing"
        );
    }

    #[test]
    fn a_set_of_one_names_its_member() {
        assert_eq!(SeatSet::from_seats(&[4]).unwrap().solitary(), Some(4));
        assert_eq!(SeatSet::from_seats(&[0, 4]).unwrap().solitary(), None);
        assert_eq!(SeatSet::EMPTY.solitary(), None);
    }

    /// One representation per membership, which is what lets the set be hashed
    /// without two peers first agreeing on an ordering.
    #[test]
    fn one_membership_is_one_encoding() {
        let a = SeatSet::from_seats(&[9, 0, 3]).unwrap();
        let b = SeatSet::from_seats(&[0, 3, 9]).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.bits(), b.bits());

        let mut c = SeatSet::EMPTY;
        c.insert(3).unwrap();
        c.insert(3).unwrap(); // twice
        c.insert(0).unwrap();
        c.insert(9).unwrap();
        assert_eq!(a, c, "a set, not a list");
    }

    #[test]
    fn the_whole_table_fits() {
        let all: Vec<u8> = (0..MAX_SEATS).collect();
        let s = SeatSet::from_seats(&all).unwrap();
        assert_eq!(s.len(), MAX_SEATS as usize);
        assert!(s.is_superset_of(SeatSet::from_seats(&[0, 5, 9]).unwrap()));
    }
}
