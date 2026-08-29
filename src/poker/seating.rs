//! Sitting down at a cash table that is already playing.
//!
//! A tournament has no such thing: everybody starts together and nobody joins,
//! which is why `RATED_SNG_POKERTH_V1` deals its first hand only when all ten
//! seats are full. A cash table is the opposite — it deals with two and people
//! arrive and leave between hands — and that creates one problem the rest of the
//! rules do not: **the blinds are behind the newcomer**.
//!
//! # Why a newcomer cannot simply be dealt in
//!
//! Blinds move one seat per hand. A player who sits down just after the big
//! blind has passed will not be asked for one until it comes round again, which
//! is up to `n - 1` hands away. Dealing them in immediately gives them most of
//! an orbit of free poker — every hand of it played in late position, which is
//! the good half — and the players who paid for their seats are paying for it.
//!
//! So the newcomer chooses. Both options are ordinary at every cash table and
//! neither is a penalty:
//!
//! * [`Entry::WaitForBigBlind`] — sit out until the big blind reaches this seat
//!   naturally, then join by posting it like anybody else. Costs nothing and
//!   misses hands.
//! * [`Entry::PostBigBlind`] — pay a big blind now, out of position, and be
//!   dealt into the next hand. Costs one big blind and misses nothing.
//!
//! # The one seat that does not have to choose
//!
//! A player who sits down where the blinds are about to arrive is not skipping
//! anything, and asking them to post would charge them twice. [`entering`]
//! returns [`Owed::Nothing`] for exactly those seats, and this is the part a
//! naive implementation gets wrong by asking everybody.
//!
//! # What a posted blind is worth
//!
//! `PROTOCOL.md` D-005 already has the concept this needs: *"an absent seat posts
//! its blind as dead money and takes no cards"*. A posted entry blind is the
//! same kind of money. It goes to the pot, and — unless the poster happens to be
//! in the big-blind position, where it is the blind — it buys no option and does
//! not count towards a call. That is what [`Owed::Post`] carries in `live`, and
//! getting it wrong is worth a big blind to the newcomer on every entry.

use crate::poker::engine::Positions;
use crate::poker::state::{Chips, SeatIdx};

/// How a player sitting down at a running table wants to get into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// Wait for the big blind to reach this seat. Free, and misses hands.
    WaitForBigBlind,
    /// Post a big blind now and play the next hand. Costs one, misses none.
    PostBigBlind,
}

/// What a seat must put in to be dealt into the next hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owed {
    /// Dealt in, posting nothing beyond whatever blind the position asks for.
    ///
    /// Either the blinds are about to reach this seat anyway, or it has already
    /// waited for them.
    Nothing,
    /// Post this much to be dealt in.
    Post {
        amount: Chips,
        /// Whether the money is **live**: whether it counts as this seat's own
        /// bet and buys the option, or is dead money in the pot.
        ///
        /// Live only in the big-blind position, where the post *is* the blind.
        /// Anywhere else the seat has both posted and must still call, which is
        /// what makes entering out of position cost something.
        live: bool,
    },
    /// Not dealt in. Still waiting for the big blind to come round.
    Waiting,
}

/// How many hands until the big blind reaches `seat`, counting the next hand as
/// one.
///
/// The blinds move one **position** per hand — the dead-button rule, which is
/// what makes this countable at all. Under a rule that moved them to the next
/// occupied seat instead, the answer would depend on who busts in between and no
/// newcomer could be told how long they were waiting for.
pub fn hands_until_big_blind(at: &Positions, seat: SeatIdx, seat_count: u8) -> u8 {
    let n = seat_count.max(1) as u16;
    let from = at.big_blind as u16 % n;
    let to = seat as u16 % n;
    let steps = (to + n - from) % n;
    // Zero means this seat holds the blind **now**, and the question is when it
    // holds it next — which is a whole orbit away, not immediately. Returning
    // zero read as "it is about to pay", and a newcomer sitting down in the seat
    // that has just posted would have been dealt in free for the entire orbit
    // before it pays again, which is the exact free ride this module exists to
    // prevent.
    (if steps == 0 { n } else { steps }) as u8
}

/// Whether the blinds are about to arrive at this seat by themselves.
///
/// The small blind next hand, or the big blind next hand — in both cases the
/// seat is paying its way in on the normal schedule and owes nothing extra.
/// Charging it would be charging twice.
fn blinds_are_coming(at: &Positions, seat: SeatIdx, seat_count: u8) -> bool {
    matches!(hands_until_big_blind(at, seat, seat_count), 1 | 2)
}

/// What a seat that has just sat down owes for the next hand.
///
/// `at` is the position of the hand **just played**, so the blinds for the next
/// one are one seat further round.
pub fn entering(
    at: &Positions,
    seat: SeatIdx,
    seat_count: u8,
    big_blind: Chips,
    choice: Entry,
) -> Owed {
    if blinds_are_coming(at, seat, seat_count) {
        // Nothing to skip. This is the case an implementation that asks
        // everybody gets wrong, and it is the common one at a short table:
        // heads-up, every seat is always one of the two blinds.
        return Owed::Nothing;
    }

    match choice {
        Entry::WaitForBigBlind => Owed::Waiting,
        Entry::PostBigBlind => Owed::Post {
            amount: big_blind,
            // Dead: the seat is not in the big-blind position, so the post buys
            // no option and does not count towards a call.
            live: false,
        },
    }
}

/// Whether a seat that has been waiting is dealt in now.
///
/// Called for a seat that chose [`Entry::WaitForBigBlind`]. It joins when the
/// big blind arrives, and then it posts the blind because it **is** the blind —
/// which is the whole point of waiting.
pub fn waiting_seat_joins(at: &Positions, seat: SeatIdx) -> Owed {
    if at.big_blind == seat {
        Owed::Nothing
    } else {
        Owed::Waiting
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(button: SeatIdx, sb: SeatIdx, bb: SeatIdx) -> Positions {
        Positions {
            button,
            small_blind: sb,
            big_blind: bb,
        }
    }

    /// The count is in **positions**, not in players, which is what the dead
    /// button makes possible. Under a rule that skipped empty seats the answer
    /// would depend on who busts in between, and a newcomer could not be told
    /// how long they were waiting.
    #[test]
    fn the_wait_is_counted_in_positions() {
        // Six seats, big blind at 2. Seat 3 is next.
        let p = at(0, 1, 2);
        assert_eq!(hands_until_big_blind(&p, 3, 6), 1);
        assert_eq!(hands_until_big_blind(&p, 4, 6), 2);
        assert_eq!(hands_until_big_blind(&p, 5, 6), 3);
        assert_eq!(hands_until_big_blind(&p, 0, 6), 4);
        assert_eq!(hands_until_big_blind(&p, 1, 6), 5);
        // The seat that has the blind **now** waits a whole orbit for the next
        // one, and the answer is six rather than zero: a newcomer sitting down
        // there has paid nothing and is six hands from being asked.
        assert_eq!(hands_until_big_blind(&p, 2, 6), 6);
    }

    /// A seat the blinds are about to reach owes nothing. It is not skipping
    /// anything, and charging it would charge it twice — the case an
    /// implementation that asks everybody gets wrong.
    #[test]
    fn a_seat_the_blinds_are_about_to_reach_owes_nothing() {
        let p = at(0, 1, 2);
        for seat in [3, 4] {
            assert_eq!(
                entering(&p, seat, 6, 100, Entry::PostBigBlind),
                Owed::Nothing,
                "seat {seat} was charged for a blind it was about to post"
            );
            assert_eq!(
                entering(&p, seat, 6, 100, Entry::WaitForBigBlind),
                Owed::Nothing,
                "seat {seat} was made to wait for a blind one hand away"
            );
        }
    }

    /// And a seat the blinds have just passed has the choice, which is the whole
    /// reason this module exists.
    #[test]
    fn a_seat_behind_the_blinds_chooses() {
        let p = at(0, 1, 2);
        // Seat 5 waits three hands for the big blind: it would otherwise get two
        // free hands of late-position poker.
        assert_eq!(
            entering(&p, 5, 6, 100, Entry::WaitForBigBlind),
            Owed::Waiting
        );
        assert_eq!(
            entering(&p, 5, 6, 100, Entry::PostBigBlind),
            Owed::Post {
                amount: 100,
                live: false
            }
        );
    }

    /// The posted blind is **dead**. A seat entering out of position has both
    /// posted and must still call, and treating the post as live would hand the
    /// newcomer a free big blind on every entry.
    #[test]
    fn a_post_out_of_position_is_dead_money() {
        let p = at(0, 1, 2);
        match entering(&p, 5, 6, 100, Entry::PostBigBlind) {
            Owed::Post { live, amount } => {
                assert!(!live, "the entry post bought an option it did not pay for");
                assert_eq!(amount, 100);
            }
            other => panic!("expected a post, got {other:?}"),
        }
    }

    /// A seat that waited is dealt in when the blind arrives, and posts it as
    /// the blind — which is what it was waiting for.
    #[test]
    fn a_waiting_seat_joins_on_the_big_blind() {
        assert_eq!(waiting_seat_joins(&at(0, 1, 2), 2), Owed::Nothing);
        assert_eq!(waiting_seat_joins(&at(0, 1, 2), 5), Owed::Waiting);
    }

    /// Heads-up, every seat **is** a blind, so nobody ever has to post to enter.
    /// A rule that asked would be asking a player to pay to sit down at a table
    /// where sitting down costs a blind anyway.
    #[test]
    fn nobody_posts_to_enter_a_two_handed_table() {
        let p = at(0, 0, 1);
        for seat in 0..2 {
            assert_eq!(
                entering(&p, seat, 2, 100, Entry::PostBigBlind),
                Owed::Nothing,
                "seat {seat} was charged to enter a heads-up table"
            );
        }
    }

    /// Over a whole orbit, every seat is asked exactly once and the two seats in
    /// front of the blinds are never asked. The count is the property: a table
    /// where more than `n - 2` seats owe a post is a table charging people to sit
    /// in the blinds.
    #[test]
    fn exactly_the_seats_that_would_get_a_free_ride_are_asked() {
        for n in 2..=10u8 {
            let p = at(0, 1 % n, 2 % n);
            let asked = (0..n)
                .filter(|&s| {
                    entering(&p, s, n, 100, Entry::PostBigBlind)
                        != Owed::Nothing
                })
                .count();
            let expected = (n as usize).saturating_sub(2).min(n as usize);
            assert_eq!(
                asked, expected,
                "{n} seats: {asked} were asked to post, expected {expected}"
            );
        }
    }

    /// Waiting is free. Whatever a seat is asked, choosing to wait never costs
    /// chips — that is the difference between the two options and the reason
    /// both exist.
    #[test]
    fn waiting_never_costs_anything() {
        for n in 2..=10u8 {
            let p = at(0, 1 % n, 2 % n);
            for seat in 0..n {
                assert!(
                    !matches!(
                        entering(&p, seat, n, 100, Entry::WaitForBigBlind),
                        Owed::Post { .. }
                    ),
                    "{n} seats, seat {seat}: waiting cost chips"
                );
            }
        }
    }
}
