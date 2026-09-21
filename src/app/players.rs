//! `D-076`: who is in the lobby -- a window of at most `MAX_TRACKED_PRESENCE`
//! players, turned over the way the tables' window is (`D-055`).
//!
//! The owner's word (2026-09-21): *limit the players shown in "Players in the
//! lobby" the way the tables shown are limited, the refresh of the list
//! included.* The list was a map of every key that had said *I am here* in the
//! last two minutes, with no bound at all -- in a list fed by strangers, where
//! one signature from a fresh key is one more row. So the tables' three rules,
//! and the tables' own counter for the third:
//!
//! 1. **At most 512 rows**: `MAX_TRACKED_PRESENCE`, which is
//!    `MAX_TRACKED_TABLES`, and an assertion below makes that a build error to
//!    forget.
//! 2. **A full window turns over: the player shown longest makes way**, so a
//!    lobby larger than the window is a moving sample of who is here and not a
//!    record of whoever arrived first. Players who stopped saying so go first,
//!    and at no cost: a window full of the departed is not full.
//! 3. **At a pace a person can read**: a full window gives up at most
//!    `ROWS_ROTATED_PER_MIN` rows a minute. A window with room takes everybody
//!    at once, and a player already shown costs nothing to hear again, so a busy
//!    minute never stops the names on the screen being kept up to date.
//!
//! **What is not carried over is the tables' hold.** The table a player sits at
//! is never displaced because a table that fills stops advertising, and its row
//! would go from under the player while they played there. A player's client
//! says *I am here* every `PRESENCE_HEARTBEAT_MS` for as long as it runs, so no
//! row is at risk of that; one that stops is somebody who left.
//!
//! *Shown longest* is the order of arrival in this window, counted, and not a
//! time: the clock this list is fed is the node's sweep, thirty seconds coarse,
//! and every player heard between two sweeps would have tied.

use std::collections::BTreeMap;

use crate::net::lobby::{Window, ROWS_ROTATED_PER_MIN};
use crate::protocol::constants::{MAX_TRACKED_PRESENCE, MAX_TRACKED_TABLES, PRESENCE_TTL_MS};

// `D-076`: the players' window is the tables' window.
const _: () = assert!(MAX_TRACKED_PRESENCE == MAX_TRACKED_TABLES);

/// What became of a player who was heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heard {
    /// New in the window.
    Arrived,
    /// Shown already: the name and the time are refreshed, and nothing moves.
    Again,
    /// The window is full and has given up its share of rows for this minute.
    /// Nothing about the player: they are heard again at their next heartbeat.
    NotNow,
}

#[derive(Debug, Clone)]
struct Row {
    name: String,
    /// When they last said so, on the sweep's clock.
    heard_ms: u64,
    /// Their place in the order of arrival in this window.
    arrived: u64,
}

/// The players this client shows, by key.
///
/// Keyed on the **key**, never the name: two players may choose one name, and a
/// list keyed on names would let either of them evict the other.
#[derive(Debug, Clone, Default)]
pub struct Players {
    rows: BTreeMap<[u8; 32], Row>,
    arrivals: u64,
    /// How many rows a full window has given up in the current minute.
    rotated: Window,
}

impl Players {
    /// `who` said they are here -- a presence, or a line in the chat -- at
    /// `now_ms` on the sweep's clock.
    pub fn heard(&mut self, who: [u8; 32], name: String, now_ms: u64) -> Heard {
        if let Some(row) = self.rows.get_mut(&who) {
            row.name = name;
            row.heard_ms = row.heard_ms.max(now_ms);
            return Heard::Again;
        }
        if self.rows.len() >= MAX_TRACKED_PRESENCE {
            self.expire(now_ms);
        }
        if self.rows.len() >= MAX_TRACKED_PRESENCE {
            if !self.rotated.admit(now_ms, ROWS_ROTATED_PER_MIN) {
                return Heard::NotNow;
            }
            let longest = self.rows.iter().min_by_key(|(_, r)| r.arrived).map(|(k, _)| *k);
            if let Some(k) = longest {
                self.rows.remove(&k);
            }
        }
        self.arrivals += 1;
        self.rows.insert(who, Row { name, heard_ms: now_ms, arrived: self.arrivals });
        Heard::Arrived
    }

    /// Forget the players who stopped saying so; how many went. There is no
    /// goodbye message, because a client that is switched off does not send one.
    pub fn expire(&mut self, now_ms: u64) -> usize {
        let before = self.rows.len();
        self.rows.retain(|_, r| now_ms.saturating_sub(r.heard_ms) < PRESENCE_TTL_MS);
        before - self.rows.len()
    }

    pub fn contains(&self, who: &[u8; 32]) -> bool {
        self.rows.contains_key(who)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Every player shown, in key order, with the name they gave last.
    pub fn iter(&self) -> impl Iterator<Item = (&[u8; 32], &str)> {
        self.rows.iter().map(|(k, r)| (k, r.name.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: u64 = 1_700_000_000_000;

    fn key(n: usize) -> [u8; 32] {
        let mut k = [0u8; 32];
        k[..8].copy_from_slice(&(n as u64).to_be_bytes());
        k
    }

    fn name(n: usize) -> String {
        format!("player {n}")
    }

    /// **`D-076`: a full window turns over as the tables' does.** The player
    /// shown longest makes way, the bound holds, and the newest is shown.
    ///
    /// The window is filled from the HIGHEST key down, so that the order of
    /// arrival is the reverse of the keys' own: a window that let the smallest
    /// key go, or the first in its map's order, would pass a test filled the
    /// other way round.
    #[test]
    fn a_full_window_of_players_turns_over_as_the_tables_do() {
        let mut p = Players::default();
        let first = 5_000;
        let last = first - (MAX_TRACKED_PRESENCE - 1);
        for n in (last..=first).rev() {
            assert_eq!(p.heard(key(n), name(n), AT), Heard::Arrived, "player {n}, into a window with room");
        }
        assert_eq!(p.len(), MAX_TRACKED_PRESENCE);
        // Hearing everybody again -- in the other order -- moves nothing and
        // costs nothing: the order of arrival is not the order of the last word.
        for n in last..=first {
            assert_eq!(p.heard(key(n), name(n), AT + 1_000), Heard::Again);
        }
        // Ten newcomers, and the ten shown longest make way, in the order they came.
        for n in 90_000..90_010 {
            assert_eq!(p.heard(key(n), name(n), AT + 2_000), Heard::Arrived);
        }
        assert_eq!(p.len(), MAX_TRACKED_PRESENCE, "the bound holds");
        for n in first - 9..=first {
            assert!(!p.contains(&key(n)), "player {n}, among the first to arrive, made way");
        }
        assert!(p.contains(&key(first - 10)), "and the eleventh to arrive did not");
        assert!(p.contains(&key(last)), "nor the smallest key, which arrived last");
        assert!(p.contains(&key(90_009)), "the newest is shown");
    }

    /// **`D-076`: a full window gives up only so many rows a minute** -- the
    /// tables' share, by the tables' counter -- and a player already shown is
    /// heard again for nothing, the minute's share spent or not.
    #[test]
    fn a_full_window_of_players_gives_up_only_so_many_rows_a_minute() {
        let mut p = Players::default();
        for n in 0..MAX_TRACKED_PRESENCE {
            p.heard(key(n), name(n), AT);
        }
        let mut shown = 0;
        for n in 10_000..11_000 {
            match p.heard(key(n), name(n), AT + 1_000) {
                Heard::Arrived => shown += 1,
                Heard::NotNow => {}
                Heard::Again => panic!("player {n} was never heard before"),
            }
        }
        assert_eq!(shown, ROWS_ROTATED_PER_MIN as usize, "the minute's share, and no more");
        assert_eq!(p.len(), MAX_TRACKED_PRESENCE, "the bound still holds");

        // The share is spent, and a player on the screen still refreshes.
        assert_eq!(p.heard(key(100), "renamed".into(), AT + 2_000), Heard::Again);
        assert!(p.iter().any(|(k, n)| *k == key(100) && n == "renamed"), "the name is the one said last");

        // A minute on, another share.
        let mut later = 0;
        for n in 20_000..21_000 {
            if p.heard(key(n), name(n), AT + 62_000) == Heard::Arrived {
                later += 1;
            }
        }
        assert_eq!(later, ROWS_ROTATED_PER_MIN as usize, "a minute on, another share");
    }

    /// Players who stopped saying so make room first, and the room they leave
    /// is not charged to the minute's share: a window full of the departed is
    /// not full, and newcomers take their places at once.
    #[test]
    fn a_window_full_of_players_who_left_is_not_full() {
        let mut p = Players::default();
        for n in 0..MAX_TRACKED_PRESENCE {
            p.heard(key(n), name(n), AT);
        }
        // Half of them keep saying so; the other half left, a whole lifetime ago.
        let later = AT + PRESENCE_TTL_MS;
        let stayed = MAX_TRACKED_PRESENCE / 2;
        for n in 0..stayed {
            assert_eq!(p.heard(key(n), name(n), later), Heard::Again);
        }
        let mut shown = 0;
        for n in 10_000..11_000 {
            if p.heard(key(n), name(n), later) == Heard::Arrived {
                shown += 1;
            }
        }
        assert_eq!(
            shown,
            (MAX_TRACKED_PRESENCE - stayed) + ROWS_ROTATED_PER_MIN as usize,
            "every place the departed left, and then the minute's share"
        );
        assert!((stayed..MAX_TRACKED_PRESENCE).all(|n| !p.contains(&key(n))), "the departed are gone");
        assert_eq!(p.len(), MAX_TRACKED_PRESENCE);

        // And the sweep forgets them without anybody arriving.
        let mut q = Players::default();
        q.heard(key(1), name(1), AT);
        q.heard(key(2), name(2), AT + 60_000);
        assert_eq!(q.expire(AT + PRESENCE_TTL_MS), 1, "the one who stopped saying so");
        assert!(q.contains(&key(2)) && !q.contains(&key(1)));
    }
}
