//! What this player keeps about other players: a rating of one to five stars
//! and a note, PokerTH's *Note about player ...* (`PlayerNoteDialog.qml`).
//!
//! Local and private, as PokerTH says under its dialog: *notes and ratings are
//! stored locally and are only visible to you*. Nothing here reaches the wire.
//!
//! **Kept by application key, not by name.** PokerTH files a note under the
//! player's nickname; here a name is display data and never an identifier
//! (`PROTOCOL.md` §4.3), so a stranger who takes a rated player's name takes
//! none of the rating with it.
//!
//! Beside the settings, in the profile directory, written the same way: a
//! temporary file, flushed, renamed over the old one. An unreadable file reads
//! as no notes, and is replaced at the next save.

use std::io;
use std::path::{Path, PathBuf};

use minicbor::{Decode, Encode};

/// PokerTH's longest note.
pub const NOTE_MAX_CHARS: usize = 500;
/// How many players the file holds at most; the oldest-written go first.
pub const ENTRIES_MAX: usize = 2_000;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct Entry {
    #[cbor(n(0), with = "minicbor::bytes")]
    pub key: [u8; 32],
    /// 0 for none, 1..=5 stars.
    #[n(1)]
    pub rating: u8,
    #[n(2)]
    pub note: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct Notes {
    #[n(0)]
    pub entries: Vec<Entry>,
}

impl Notes {
    /// The rating and the note kept about `key`: none and empty when nothing is.
    pub fn about(&self, key: &[u8; 32]) -> (u8, &str) {
        self.entries
            .iter()
            .find(|e| &e.key == key)
            .map(|e| (e.rating, e.note.as_str()))
            .unwrap_or((0, ""))
    }

    /// Keep a rating and a note about `key`, bounded as PokerTH bounds them. A
    /// rating of none and an empty note forget the player.
    pub fn set(&mut self, key: [u8; 32], rating: u8, note: &str) {
        let rating = rating.min(5);
        let note: String = note.chars().filter(|c| !c.is_control() || *c == '\n').take(NOTE_MAX_CHARS).collect();
        self.entries.retain(|e| e.key != key);
        if rating == 0 && note.trim().is_empty() {
            return;
        }
        self.entries.push(Entry { key, rating, note });
        if self.entries.len() > ENTRIES_MAX {
            let over = self.entries.len() - ENTRIES_MAX;
            self.entries.drain(..over);
        }
    }
}

/// Where the notes live.
pub fn notes_path(dir: &Path) -> PathBuf {
    dir.join("notes.cbor")
}

/// The notes, or none: an absent file is a player who rated nobody yet, and an
/// unreadable one is no reason to refuse to start.
pub fn load(dir: &Path) -> Notes {
    std::fs::read(notes_path(dir))
        .ok()
        .and_then(|bytes| minicbor::decode::<Notes>(&bytes).ok())
        .unwrap_or_default()
}

/// Save the notes, atomically.
pub fn save(dir: &Path, notes: &Notes) -> io::Result<()> {
    let bytes = minicbor::to_vec(notes).map_err(|e| io::Error::other(format!("the notes do not encode: {e}")))?;
    std::fs::create_dir_all(dir)?;
    let tmp = notes_path(dir).with_extension("cbor.tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, notes_path(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("p2p-poker-notes-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_note_is_kept_by_key_and_comes_back() {
        let dir = scratch("round-trip");
        let mut n = Notes::default();
        n.set([1u8; 32], 4, "calls too much");
        n.set([2u8; 32], 2, "");
        save(&dir, &n).unwrap();
        let back = load(&dir);
        assert_eq!(back.about(&[1u8; 32]), (4, "calls too much"));
        assert_eq!(back.about(&[2u8; 32]), (2, ""));
        assert_eq!(back.about(&[3u8; 32]), (0, ""), "a player nobody rated");
    }

    /// PokerTH's bounds: five stars, five hundred characters; and nothing kept
    /// is the same as a player forgotten.
    #[test]
    fn a_note_is_bounded_and_an_empty_one_forgets() {
        let mut n = Notes::default();
        n.set([7u8; 32], 9, &"x".repeat(900));
        let (r, note) = n.about(&[7u8; 32]);
        assert_eq!(r, 5);
        assert_eq!(note.chars().count(), NOTE_MAX_CHARS);
        n.set([7u8; 32], 0, "   ");
        assert!(n.entries.is_empty());
    }

    #[test]
    fn an_unreadable_file_is_no_notes() {
        let dir = scratch("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(notes_path(&dir), b"not cbor").unwrap();
        assert_eq!(load(&dir), Notes::default());
    }
}
