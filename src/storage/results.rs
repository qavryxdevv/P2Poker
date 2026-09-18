//! What this player's tournaments came to: the place finished in, at which
//! table, when. For the lobby's card about the player (`D-067`).
//!
//! **Local and private**, like the notes: nothing here reaches the wire, and
//! nothing here is a ranking of anybody else. The card it feeds says how many
//! Sit & Gos this player has played, how many they won and their best place
//! -- their own record, as a card room's regular might keep it -- and the last
//! game's result once, neutrally: a place is a place, and no line of it asks
//! for the chips back.
//!
//! **Written by the window, from the node's word.** A place comes from the
//! node's `Finished` event at a hand boundary (`S1-FL`), the same word the
//! table window's *You finished in Nth place* is drawn from; the window never
//! works a place out itself. A cash game has no place and leaves no entry.
//!
//! Beside the settings, in the profile directory, written the same way: a
//! temporary file, flushed, renamed over the old one. An unreadable file reads
//! as no record, and is replaced at the next save.

use std::io;
use std::path::{Path, PathBuf};

use minicbor::{Decode, Encode};

/// How many games the file holds at most; the oldest go first.
pub const RESULTS_MAX: usize = 500;

/// One finished tournament.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct Entry {
    /// When the deciding hand ended, by this machine's clock.
    #[n(0)]
    pub when_unix_ms: u64,
    /// The table's name -- display data, kept for the card and nothing else.
    #[n(1)]
    pub table: String,
    /// How many seats the table had.
    #[n(2)]
    pub seats: u8,
    /// The place finished in, 1 the winner.
    #[n(3)]
    pub place: u8,
    /// `S1-FL`: another seat shared the place.
    #[n(4)]
    pub tied: bool,
}

impl Entry {
    /// Whether this was the win.
    pub fn won(&self) -> bool {
        self.place == 1
    }
}

/// The record, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct Results {
    #[n(0)]
    pub entries: Vec<Entry>,
}

/// What the card says: the counts, the best place, and the last game.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub games: usize,
    pub wins: usize,
    /// The best place ever finished in; `None` before the first game.
    pub best_place: Option<u8>,
    pub last: Option<Entry>,
}

impl Results {
    /// Keep one more game, bounded: past `RESULTS_MAX` the oldest goes.
    pub fn record(&mut self, entry: Entry) {
        self.entries.push(entry);
        while self.entries.len() > RESULTS_MAX {
            self.entries.remove(0);
        }
    }

    pub fn summary(&self) -> Summary {
        Summary {
            games: self.entries.len(),
            wins: self.entries.iter().filter(|e| e.won()).count(),
            best_place: self.entries.iter().map(|e| e.place).min(),
            last: self.entries.last().cloned(),
        }
    }
}

/// Where the record lives.
pub fn results_path(dir: &Path) -> PathBuf {
    dir.join("results.cbor")
}

/// Load the record, or none: an absent or unreadable file is not a reason to
/// refuse to start a poker client.
pub fn load(dir: &Path) -> Results {
    std::fs::read(results_path(dir))
        .ok()
        .and_then(|bytes| minicbor::decode::<Results>(&bytes).ok())
        .unwrap_or_default()
}

/// Save the record, atomically, as the settings are saved.
pub fn save(dir: &Path, results: &Results) -> io::Result<()> {
    let bytes = minicbor::to_vec(results)
        .map_err(|e| io::Error::other(format!("the results do not encode: {e}")))?;
    std::fs::create_dir_all(dir)?;
    let tmp = results_path(dir).with_extension("cbor.tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, results_path(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("p2p-poker-results-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn game(place: u8) -> Entry {
        Entry { when_unix_ms: 1_700_000_000_000 + u64::from(place), table: "Riverside".into(), seats: 6, place, tied: false }
    }

    /// The card's numbers are the record's: games, wins, the best place, the
    /// last game -- and nothing before the first game.
    #[test]
    fn the_summary_is_the_record_counted() {
        let mut r = Results::default();
        assert_eq!(r.summary(), Summary::default());
        for place in [4, 1, 2] {
            r.record(game(place));
        }
        let s = r.summary();
        assert_eq!(s.games, 3);
        assert_eq!(s.wins, 1);
        assert_eq!(s.best_place, Some(1));
        assert_eq!(s.last.as_ref().map(|e| e.place), Some(2), "the last game is the last recorded");
    }

    /// The file is bounded, and the oldest game is what goes.
    #[test]
    fn the_record_keeps_the_newest_games() {
        let mut r = Results::default();
        for i in 0..(RESULTS_MAX as u64 + 7) {
            r.record(Entry { when_unix_ms: i, table: String::new(), seats: 2, place: 2, tied: false });
        }
        assert_eq!(r.entries.len(), RESULTS_MAX);
        assert_eq!(r.entries[0].when_unix_ms, 7, "the seven oldest went");
    }

    /// What is saved is what is loaded, and no file is no record.
    #[test]
    fn the_record_round_trips_and_an_absent_file_is_empty() {
        let dir = scratch("round-trip");
        assert_eq!(load(&dir), Results::default());
        let mut r = Results::default();
        r.record(game(3));
        save(&dir, &r).unwrap();
        assert_eq!(load(&dir), r);
        std::fs::write(results_path(&dir), b"not cbor").unwrap();
        assert_eq!(load(&dir), Results::default(), "an unreadable file reads as no record");
    }
}
