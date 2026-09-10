//! Chips that move: a seat's bet sliding into the pot when a street closes,
//! and the pot sliding to the seat that won it.
//!
//! `S1-CS`. The window is immediate-mode and stateless between frames, so the
//! motion is a small model kept in [`TableUi`](super::TableUi): it watches
//! successive snapshots of the table, notices what disappeared from where and
//! appeared where, and answers "which chips are in the air, and how far
//! along". Pure data and pure time, tested without a window; the painter
//! asks for positions and draws.
//!
//! Nothing here is a fact about the game. A flight is a picture of a change
//! the engine has already made, and a snapshot that skips a step (two streets
//! closed between two frames) produces at worst a chip that flies from the
//! wrong place.

use super::TableView;
use crate::poker::state::{Chips, SeatIdx};

/// How long a chip takes to get where it is going, in seconds.
pub const FLIGHT_SECS: f64 = 0.45;

/// Where chips come from and go to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Node {
    Seat(SeatIdx),
    Pot,
}

/// One stack of chips on its way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flight {
    pub from: Node,
    pub to: Node,
    pub amount: Chips,
    pub started: f64,
}

impl Flight {
    /// How far along, eased so it starts fast and settles, in `0..=1`.
    pub fn progress(&self, now: f64) -> f32 {
        let t = ((now - self.started) / FLIGHT_SECS).clamp(0.0, 1.0) as f32;
        1.0 - (1.0 - t).powi(3)
    }

    /// Landed at `FLIGHT_SECS`, to within a microsecond: the clock is a float
    /// and `10.1 + 0.45 - 10.1` is not `0.45`.
    pub fn landed(&self, now: f64) -> bool {
        now - self.started >= FLIGHT_SECS - 1e-6
    }
}

/// The last snapshot seen, and the chips in the air.
#[derive(Debug, Clone, Default)]
pub struct Motion {
    seen_hand: u64,
    seen_bets: Vec<(SeatIdx, Chips)>,
    seen_pot: Chips,
    seen_over: bool,
    flights: Vec<Flight>,
}

impl Motion {
    /// Look at the table as it is now and start whatever flights the change
    /// since the last look calls for.
    pub fn observe(&mut self, view: &TableView, now: f64) {
        let bets: Vec<(SeatIdx, Chips)> = view.seats.iter().map(|s| (s.seat, s.bet)).collect();
        if view.hand != self.seen_hand {
            // A new hand: the blinds appear from nowhere, and nothing flies.
            // What was still in the air from the last hand's payout finishes.
            self.seen_hand = view.hand;
            self.seen_bets = bets;
            self.seen_pot = view.pot;
            self.seen_over = view.hand_over;
            return;
        }
        // A street closed, or the hand folded out: every bet that was in front
        // of a seat and is gone went into the pot.
        if view.pot >= self.seen_pot {
            for (seat, before) in &self.seen_bets {
                let now_bet = bets.iter().find(|(s, _)| s == seat).map(|(_, b)| *b).unwrap_or(0);
                if *before > 0 && now_bet == 0 {
                    self.flights.push(Flight {
                        from: Node::Seat(*seat),
                        to: Node::Pot,
                        amount: *before,
                        started: now,
                    });
                }
            }
        }
        // The hand ended: the pot goes to whoever won it.
        if view.hand_over && !self.seen_over {
            for s in view.seats.iter().filter(|s| s.won > 0) {
                self.flights.push(Flight {
                    from: Node::Pot,
                    to: Node::Seat(s.seat),
                    amount: s.won,
                    started: now,
                });
            }
        }
        self.seen_bets = bets;
        self.seen_pot = view.pot;
        self.seen_over = view.hand_over;
        self.flights.retain(|f| !f.landed(now));
    }

    /// The chips in the air right now.
    pub fn in_flight(&self, now: f64) -> Vec<Flight> {
        self.flights.iter().copied().filter(|f| !f.landed(now)).collect()
    }

    /// Whether another frame is owed.
    pub fn active(&self, now: f64) -> bool {
        self.flights.iter().any(|f| !f.landed(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::table::SeatView;

    fn table(hand: u64, pot: Chips, bets: &[Chips], won: &[Chips], over: bool) -> TableView {
        TableView {
            hand,
            pot,
            hand_over: over,
            seats: bets
                .iter()
                .enumerate()
                .map(|(i, b)| SeatView {
                    seat: i as SeatIdx,
                    bet: *b,
                    won: won.get(i).copied().unwrap_or(0),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    /// A street closes: the bets in front of the seats are gone and the pot
    /// has them, so one flight per seat that had chips out, and they land
    /// after `FLIGHT_SECS`.
    #[test]
    fn bets_fly_into_the_pot_when_the_street_closes() {
        let mut m = Motion::default();
        m.observe(&table(3, 0, &[100, 100, 0], &[], false), 10.0);
        assert!(m.in_flight(10.0).is_empty(), "nothing moves on the first look");
        m.observe(&table(3, 200, &[0, 0, 0], &[], false), 10.1);
        let flights = m.in_flight(10.1);
        assert_eq!(flights.len(), 2, "{flights:?}");
        assert!(flights.iter().all(|f| f.to == Node::Pot && f.amount == 100));
        assert!(flights.iter().any(|f| f.from == Node::Seat(0)));
        assert!(flights.iter().any(|f| f.from == Node::Seat(1)));
        let p = flights[0].progress(10.1 + FLIGHT_SECS * 0.5);
        assert!(p > 0.5 && p < 1.0, "eased: {p}");
        assert!(m.active(10.1 + FLIGHT_SECS - 0.01));
        assert!(!m.active(10.1 + FLIGHT_SECS));
        assert!(m.in_flight(10.1 + FLIGHT_SECS).is_empty());
    }

    /// The same snapshot seen twice starts nothing twice, and a bet that
    /// grows (a raise) is not a chip leaving.
    #[test]
    fn a_repeated_look_and_a_raise_start_nothing() {
        let mut m = Motion::default();
        m.observe(&table(3, 0, &[100, 0], &[], false), 1.0);
        m.observe(&table(3, 0, &[100, 0], &[], false), 1.1);
        assert!(m.in_flight(1.1).is_empty());
        m.observe(&table(3, 0, &[100, 300], &[], false), 1.2);
        assert!(m.in_flight(1.2).is_empty(), "a raise is chips arriving, not leaving");
    }

    /// The hand ends: the pot flies to the seat that won it, once.
    #[test]
    fn the_pot_flies_to_the_winner_once() {
        let mut m = Motion::default();
        m.observe(&table(3, 400, &[0, 0], &[], false), 5.0);
        m.observe(&table(3, 400, &[0, 0], &[0, 400], true), 5.1);
        let flights = m.in_flight(5.1);
        assert_eq!(flights.len(), 1);
        assert_eq!(flights[0].from, Node::Pot);
        assert_eq!(flights[0].to, Node::Seat(1));
        assert_eq!(flights[0].amount, 400);
        m.observe(&table(3, 400, &[0, 0], &[0, 400], true), 5.2);
        assert_eq!(m.in_flight(5.2).len(), 1, "seen again, not started again");
    }

    /// A new hand's blinds appear where they are; nothing flies from a hand
    /// that is not the one being watched.
    #[test]
    fn a_new_hand_starts_clean() {
        let mut m = Motion::default();
        m.observe(&table(3, 400, &[0, 0], &[0, 400], true), 5.0);
        m.observe(&table(4, 150, &[50, 100], &[], false), 5.0 + FLIGHT_SECS + 1.0);
        assert!(m.in_flight(5.0 + FLIGHT_SECS + 1.0).is_empty());
        // And the blinds going into the pot on the next street fly normally.
        m.observe(&table(4, 150, &[0, 0], &[], false), 7.0);
        assert_eq!(m.in_flight(7.0).len(), 2);
    }
}
