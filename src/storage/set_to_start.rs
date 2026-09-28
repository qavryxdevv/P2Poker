//! `S1-JJ`: the tables this client knows were set to start (`D-044`).
//!
//! A table set to start -- a roster at the advert's minimum adopted -- goes on
//! with the seats that remain, two at the least, when a seat is given back
//! before the first hand. `Formation::started` learns that only by adopting such
//! a roster, and a client that restarts before its table is set has no session
//! record to learn it from (`S1-JI`): it took a roster below the minimum as
//! short, never said it was ready, and was given back. So the fact is kept here,
//! per table key, the moment it turns true, and a formation for that table is
//! built knowing it (`Formation::with_set_to_start`).
//!
//! Beside the session record, in the profile directory, and like it data that
//! playing re-derives: an unreadable file is *nothing known*, and it is no part
//! of a backup. Small by construction -- at most `KEEP` tables, none older than
//! `RESUME_RECORD_MAX_AGE_MS`, the age a session record is acted on -- and it
//! names nothing but table keys, which every lobby shows.

use std::io;
use std::path::{Path, PathBuf};

use crate::protocol::constants::RESUME_RECORD_MAX_AGE_MS;

/// The most tables kept: a client plays at a handful at once.
const KEEP: usize = 16;

/// One table: its key, and when it was noted (Unix ms, big-endian).
const ENTRY: usize = 32 + 8;

fn path(dir: &Path) -> PathBuf {
    dir.join("set_to_start.bin")
}

/// The tables kept, oldest first; nothing when the file is missing or is not
/// whole entries.
fn read(dir: &Path) -> Vec<([u8; 32], u64)> {
    let Ok(bytes) = std::fs::read(path(dir)) else {
        return Vec::new();
    };
    if bytes.len() % ENTRY != 0 {
        return Vec::new();
    }
    bytes
        .chunks_exact(ENTRY)
        .map(|c| {
            let mut key = [0u8; 32];
            key.copy_from_slice(&c[..32]);
            let mut at = [0u8; 8];
            at.copy_from_slice(&c[32..]);
            (key, u64::from_be_bytes(at))
        })
        .collect()
}

/// Note that the table with `table_key` was set to start, at `now_ms` --
/// written whole, through a temporary file, so a crash leaves the previous
/// file or this one and never half of either.
pub fn note(dir: &Path, table_key: &[u8; 32], now_ms: u64) -> io::Result<()> {
    let mut kept: Vec<([u8; 32], u64)> = read(dir)
        .into_iter()
        .filter(|(k, at)| k != table_key && now_ms.saturating_sub(*at) <= RESUME_RECORD_MAX_AGE_MS)
        .collect();
    kept.push((*table_key, now_ms));
    if kept.len() > KEEP {
        let over = kept.len() - KEEP;
        kept.drain(..over);
    }
    let mut bytes = Vec::with_capacity(kept.len() * ENTRY);
    for (k, at) in &kept {
        bytes.extend_from_slice(k);
        bytes.extend_from_slice(&at.to_be_bytes());
    }
    std::fs::create_dir_all(dir)?;
    let tmp = path(dir).with_extension("bin.tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path(dir))
}

/// Whether the table with `table_key` is known here to have been set to start.
pub fn was(dir: &Path, table_key: &[u8; 32], now_ms: u64) -> bool {
    read(dir)
        .iter()
        .any(|(k, at)| k == table_key && now_ms.saturating_sub(*at) <= RESUME_RECORD_MAX_AGE_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_700_000_000_000;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("p2p-poker-set-to-start-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A table noted is known, another is not, and nothing is known before the
    /// first note or from a file that is not whole entries.
    #[test]
    fn a_table_noted_is_known_and_no_other() {
        let dir = scratch("known");
        assert!(!was(&dir, &[1; 32], NOW), "nothing before the first note");
        note(&dir, &[1; 32], NOW).unwrap();
        assert!(was(&dir, &[1; 32], NOW + 1_000));
        assert!(!was(&dir, &[2; 32], NOW + 1_000), "another table");
        assert!(!path(&dir).with_extension("bin.tmp").exists(), "no temporary file left behind");
        std::fs::write(path(&dir), b"not whole entries").unwrap();
        assert!(!was(&dir, &[1; 32], NOW + 1_000), "an unreadable file is nothing known");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Kept only as long as a session record is acted on, and never more than
    /// `KEEP` tables; noting a table again renews it.
    #[test]
    fn it_forgets_old_tables_and_keeps_few() {
        let dir = scratch("few");
        note(&dir, &[1; 32], NOW).unwrap();
        assert!(was(&dir, &[1; 32], NOW + RESUME_RECORD_MAX_AGE_MS));
        assert!(!was(&dir, &[1; 32], NOW + RESUME_RECORD_MAX_AGE_MS + 1), "too old to act on");
        note(&dir, &[1; 32], NOW + RESUME_RECORD_MAX_AGE_MS).unwrap();
        assert!(was(&dir, &[1; 32], NOW + RESUME_RECORD_MAX_AGE_MS + 1), "noted again, renewed");
        for n in 0..(KEEP as u8 + 3) {
            note(&dir, &[100 + n; 32], NOW + RESUME_RECORD_MAX_AGE_MS + 10 + u64::from(n)).unwrap();
        }
        assert_eq!(read(&dir).len(), KEEP, "no more than KEEP tables");
        assert!(was(&dir, &[100 + KEEP as u8 + 2; 32], NOW + RESUME_RECORD_MAX_AGE_MS + 100), "the newest kept");
        assert!(!was(&dir, &[1; 32], NOW + RESUME_RECORD_MAX_AGE_MS + 100), "the oldest gone");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
