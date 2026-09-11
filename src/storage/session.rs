//! The session record: what a client needs after a restart to know it left a
//! game unfinished, and to get back to it (`S1-CR`).
//!
//! Beside the keys, in the profile directory, like the settings — and like the
//! settings it is data that can be re-derived by playing, so an unreadable
//! record yields *no record* rather than a client that refuses to start.
//!
//! **What it holds, and why each field.** The table and the session it was
//! playing (`table_id`, `session_id`, `table_key`), the founder's peer id and
//! the advertisement as it was received — the signed `LOBBY_TABLE_AD` frame —
//! (as its body's bytes and the hash a request names) because a
//! `JOIN_REQUEST` cannot be built without the advert and its hash
//! (§4.3 `n(0) advert_hash`), and a table that has started is not advertised
//! any more; this client's seat; the last boundary it reached (`hand_id` and
//! `TERMINAL(k)`, so the record can say how far it got and a rejoin can tell a
//! stale copy from a fresh one); its stack at that boundary, for the window's
//! question *rejoin table X, seat N, stack S?*; and when it was written.
//!
//! **When it is written and when it goes.** Written at every hand boundary by
//! the node, and at the moment the table is set. Removed when the session ends
//! for this seat — it left by its own choice, the table closed, the tournament
//! is over — and when a rejoin learns the session is gone. It is never removed
//! by a crash, which is the whole point.
//!
//! **When a rejoin gives up** ([`give_up`]): the table's advertisement is not
//! seen any more AND no peer of the session has been reachable for
//! `RESUME_GIVE_UP_MS` (ten minutes). While the advert is up, or a session peer
//! answers, the client keeps trying at the lobby's own cadence. A finished
//! session is learned from the founder's refusal, not from a timer.

use std::io;
use std::path::{Path, PathBuf};

use minicbor::{Decode, Encode};

use crate::protocol::constants::{RESUME_GIVE_UP_MS, RESUME_RECORD_MAX_AGE_MS};

/// The record's own version, so a later shape can refuse an older one rather
/// than misread it.
pub const RECORD_VERSION: u8 = 2;

/// An unfinished session, as the node last knew it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct Record {
    #[n(0)]
    pub version: u8,
    #[cbor(n(1), with = "minicbor::bytes")]
    pub table_id: [u8; 32],
    #[cbor(n(2), with = "minicbor::bytes")]
    pub session_id: [u8; 32],
    #[cbor(n(3), with = "minicbor::bytes")]
    pub table_key: [u8; 32],
    #[n(4)]
    pub founder_peer_id: Vec<u8>,
    #[n(5)]
    pub table_name: String,
    #[n(6)]
    pub my_seat: u8,
    /// The last boundary this client reached: the hand that ended there.
    #[n(7)]
    pub hand_id: u64,
    #[cbor(n(8), with = "minicbor::bytes")]
    pub terminal: [u8; 32],
    #[n(9)]
    pub my_stack: u64,
    #[n(10)]
    pub written_unix_ms: u64,
    /// The advert's body, as canonical bytes (`advert::to_body_bytes`), and
    /// the hash a `JOIN_REQUEST` names for it. A rejoin puts the advert back
    /// on offer from these; nothing else about the table survives a restart.
    #[n(11)]
    pub advert: Vec<u8>,
    #[cbor(n(12), with = "minicbor::bytes")]
    pub advert_hash: [u8; 32],
    /// This client's own `TABLE_READY`, verbatim, so that a rejoin says it
    /// again rather than ratifying anew: the ratification's `event_hash` is
    /// inside the `session_id`, and a new one is a session nobody else has
    /// (`run202634-3`). Version 2 of the record.
    #[n(13)]
    pub ratification: Vec<u8>,
}

pub fn session_path(dir: &Path) -> PathBuf {
    dir.join("session.cbor")
}

/// Write the record atomically: a temporary file, synced, renamed over the old
/// one — so a crash between two boundaries leaves the previous record whole
/// rather than a truncated one.
pub fn save(dir: &Path, record: &Record) -> io::Result<()> {
    let bytes = minicbor::to_vec(record)
        .map_err(|e| io::Error::other(format!("the session record does not encode: {e}")))?;
    std::fs::create_dir_all(dir)?;
    let tmp = session_path(dir).with_extension("cbor.tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, session_path(dir))
}

/// The record, if there is one this version can read. A missing file, a
/// truncated one, or one of another version is *no record*.
pub fn load(dir: &Path) -> Option<Record> {
    let bytes = std::fs::read(session_path(dir)).ok()?;
    let record: Record = minicbor::decode(&bytes).ok()?;
    if record.version != RECORD_VERSION {
        return None;
    }
    Some(record)
}

/// The record, if there is one this version can read and it is recent
/// enough to act on (`S1-CY`): a record older than `RESUME_RECORD_MAX_AGE_MS`
/// names a game that is long over, so it is dropped rather than offered.
pub fn load_recent(dir: &Path, now_ms: u64) -> Option<Record> {
    let record = load(dir)?;
    if now_ms.saturating_sub(record.written_unix_ms) > RESUME_RECORD_MAX_AGE_MS {
        let _ = forget(dir);
        return None;
    }
    Some(record)
}

/// Remove the record. Not an error when there is none: the session ended, and
/// whether a record was ever written for it is not this caller's business.
pub fn forget(dir: &Path) -> io::Result<()> {
    match std::fs::remove_file(session_path(dir)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Whether a rejoin should stop trying: the advertisement is gone AND no peer
/// of the session has been reachable for `RESUME_GIVE_UP_MS`, counted from the
/// later of the last time one was seen and the moment the attempt began.
///
/// An advert that is still up means the table exists, whatever the peers say;
/// a peer that answered a minute ago means the session may still be there
/// behind a bad link. Neither is a reason to forget a game somebody has chips
/// in. `since_ms` is when the attempt began, so a client that starts with no
/// peer in sight is given the whole allowance rather than none of it.
pub fn give_up(
    now_ms: u64,
    advert_seen: bool,
    last_peer_seen_ms: Option<u64>,
    since_ms: u64,
) -> bool {
    if advert_seen {
        return false;
    }
    let anchor = last_peer_seen_ms.map_or(since_ms, |t| t.max(since_ms));
    now_ms.saturating_sub(anchor) >= RESUME_GIVE_UP_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("p2p-poker-session-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn a_record() -> Record {
        Record {
            version: RECORD_VERSION,
            table_id: [1; 32],
            session_id: [2; 32],
            table_key: [3; 32],
            founder_peer_id: vec![9; 38],
            table_name: "Friday".into(),
            my_seat: 4,
            hand_id: 17,
            terminal: [7; 32],
            my_stack: 12_350,
            written_unix_ms: 1_700_000_000_000,
            advert: vec![0xAA; 300],
            advert_hash: [8; 32],
            ratification: vec![0xBB; 210],
        }
    }

    /// What is written is what is read, field by field, and it is the only
    /// thing in the profile directory that this module touches.
    #[test]
    fn a_record_round_trips_through_the_profile_directory() {
        let dir = scratch("round-trip");
        assert_eq!(load(&dir), None, "no record before one is written");
        let r = a_record();
        save(&dir, &r).unwrap();
        assert_eq!(load(&dir), Some(r.clone()));
        // A second boundary overwrites the first whole, never appends.
        let mut later = r.clone();
        later.hand_id = 18;
        later.my_stack = 9_000;
        save(&dir, &later).unwrap();
        assert_eq!(load(&dir), Some(later));
        assert!(!session_path(&dir).with_extension("cbor.tmp").exists(), "no temporary file left behind");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A record this version cannot read is no record, not a refusal to start.
    #[test]
    fn an_unreadable_or_foreign_record_is_no_record() {
        let dir = scratch("unreadable");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(session_path(&dir), b"not cbor at all").unwrap();
        assert_eq!(load(&dir), None);
        let mut other = a_record();
        other.version = RECORD_VERSION + 1;
        std::fs::write(session_path(&dir), minicbor::to_vec(&other).unwrap()).unwrap();
        assert_eq!(load(&dir), None, "another version is not misread as this one");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Forgetting is idempotent: the session ended, and whether a record was
    /// ever written is not the caller's business.
    #[test]
    fn forgetting_removes_the_record_and_is_idempotent() {
        let dir = scratch("forget");
        forget(&dir).unwrap();
        save(&dir, &a_record()).unwrap();
        assert!(load(&dir).is_some());
        forget(&dir).unwrap();
        assert_eq!(load(&dir), None);
        forget(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The give-up rule, every branch: an advert holds the attempt open; a
    /// peer seen recently holds it open; the allowance is counted from the
    /// later of the last peer and the start of the attempt.
    #[test]
    fn a_rejoin_gives_up_only_with_no_advert_and_no_peer_for_ten_minutes() {
        let t0 = 1_000_000u64;
        let ten = RESUME_GIVE_UP_MS;
        assert!(!give_up(t0 + ten * 3, true, None, t0), "an advert that is up is a table that exists");
        assert!(!give_up(t0 + ten - 1, false, None, t0), "one tick short of the allowance");
        assert!(give_up(t0 + ten, false, None, t0), "the allowance, with nobody ever seen");
        assert!(!give_up(t0 + ten + 5_000, false, Some(t0 + 10_000), t0), "a peer seen after the start restarts the count");
        assert!(give_up(t0 + ten + 10_000, false, Some(t0 + 10_000), t0), "and the count runs out ten minutes after that peer");
        assert!(!give_up(t0 + ten - 1, false, Some(t0 - ten), t0), "a peer seen BEFORE the attempt began does not shorten it");
    }

    /// `S1-CY`: a record written half an hour ago or more is not offered and
    /// is gone from the profile; a fresh one is offered as before.
    #[test]
    fn a_stale_record_is_dropped_rather_than_offered() {
        let dir = scratch("stale");
        std::fs::create_dir_all(&dir).unwrap();
        let mut r = a_record();
        r.written_unix_ms = 1_700_000_000_000;
        save(&dir, &r).unwrap();
        assert!(load_recent(&dir, r.written_unix_ms + RESUME_RECORD_MAX_AGE_MS).is_some(), "at the limit it is still offered");
        assert_eq!(load(&dir).as_ref(), Some(&r));
        assert!(load_recent(&dir, r.written_unix_ms + RESUME_RECORD_MAX_AGE_MS + 1).is_none(), "past it, not");
        assert_eq!(load(&dir), None, "and the record is gone, so the next start does not ask either");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
