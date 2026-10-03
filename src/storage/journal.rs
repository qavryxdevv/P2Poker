//! `G11-R`, phase A: **the signing journal** -- every own signed table frame
//! recorded durably before it leaves this client, so that an honest client
//! never signs two bodies at one slot key, across a crash, a restart or a power
//! cut. That is the precondition for treating two signed versions of one stage
//! as provable fraud (`G11`), for `G5`'s fork guard and for batch 4's marks.
//! Nothing calls it yet: phase B writes ahead from `publish_hand` in shadow,
//! phase C replays from it, phase D arms it
//! (`p2p-poker-local/audit_1002/MAP_JOURNAL.md`).
//!
//! **Where.** In the profile (`storage::profile::profile_dir`: beside the
//! program on Windows, the package's local data for the Store's copy, XDG's
//! data folder on Linux), `journal/<table id hex>/`: a lock, a header naming
//! the table and the signer, and one segment per hand, `<hand>.wal`. Not in the
//! profile backup (`D-068`): a journal serves the running hand alone.
//!
//! **How big.** Only the running hand and the retained one before it are ever
//! needed -- no seat is asked to sign at an older hand's slots -- so
//! [`Handle::prune`] removes every older segment, after moving the header's
//! first covered hand past them: some 10-16 signed frames a hand, 12-15 kB, tens
//! of kB a table whatever the play time. A segment is capped at
//! [`SEGMENT_CAP`].
//!
//! **What a record is.** `u32` length (little-endian) | the first 16 bytes of
//! the BLAKE3 of the body | the body (CBOR, [`Entry`]). On open the records are
//! read in order. Only a **short tail** is cut -- fewer bytes than a record's
//! head, fewer than its own length says, or zeros to the end: a record torn by
//! a crash, never confirmed durable, whose frame never left. Anything else that
//! does not verify -- a length over the cap, a complete record whose checksum,
//! body or hand is wrong -- leaves the file as it is and marks the hand
//! **uncovered**: the journal cannot vouch for what was signed there, and the
//! caller takes every slot of it as signed.
//!
//! **Who writes.** One thread per journal owns the files; the node's thread
//! sends it a batch and waits a bounded time ([`Handle::write`]). The writer
//! checks the whole batch first -- a second body at a key it holds is refused
//! -- appends, syncs, and only then indexes; a write that fails is cut back to
//! where it began. A write that fails or does not return in time is an error
//! to the caller, who must then send nothing of the batch: a frame never
//! leaves before its record is durable. A write that lands after its timeout
//! is durable and never sent -- harmless -- and until it lands every later
//! write fails at once. Lookups read an index the writer updates only after a
//! sync, so a hung disk never blocks a lookup.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::{Duration, Instant};

/// The journal's own format; a header of another is refused.
pub const JOURNAL_FORMAT: u16 = 1;

/// The largest record body this journal reads or writes: a signed frame never
/// exceeds the table's frame cap (16 KiB), a deck secret is a few dozen bytes.
pub const RECORD_CAP: usize = 64 * 1024;

/// The largest segment: a hand's own frames are some 15 kB. A segment over it
/// is not read (its hand is uncovered), and a write that would pass it is
/// refused.
pub const SEGMENT_CAP: u64 = 4 * 1024 * 1024;

/// Jobs the writer queues before a write is refused as a failure.
pub const WRITER_QUEUE: usize = 32;

/// Bytes before a record's body: its length and its checksum.
const RECORD_HEAD: usize = 4 + 16;

/// How long a journal left untouched stays in the profile: a table not played
/// for a week is not resumed, and its journal serves nothing.
pub const STALE_AFTER: Duration = Duration::from_secs(7 * 24 * 3600);

/// How this client comes to a table's journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The table was just set: a journal made now covers hands from the first
    /// one this client is dealt in.
    Fresh { first_hand: u64 },
    /// Back to a table after a restart, the newest hand of it seen being
    /// `newest_seen`. A journal made now -- the old one lost -- covers hands
    /// from `newest_seen + 2`: the previous life may have signed up to the hand
    /// after the newest one seen, so every slot of those is taken as signed.
    Resume { newest_seen: u64 },
}

impl Mode {
    fn first_covered(self) -> u64 {
        match self {
            Mode::Fresh { first_hand } => first_hand,
            Mode::Resume { newest_seen } => newest_seen.saturating_add(2),
        }
    }
}

/// What the journal says about the table it serves and the key that signs.
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
pub struct Header {
    /// Random, so a journal restored from elsewhere is told from this one.
    #[cbor(n(0), with = "minicbor::bytes")]
    pub id: [u8; 16],
    #[n(1)]
    pub format: u16,
    #[cbor(n(2), with = "minicbor::bytes")]
    pub table_id: [u8; 32],
    /// The application key of the seat that signs the frames recorded here.
    #[cbor(n(3), with = "minicbor::bytes")]
    pub signer: [u8; 32],
    /// The first hand this journal covers. A hand before it was signed, if at
    /// all, by a life -- or in a segment since pruned -- this journal no longer
    /// holds: uncovered. Moved forward, durably, before a prune removes anything.
    #[n(4)]
    pub covers_from: u64,
}

/// One own signed frame, at its slot key. The key is (hand, sequence, parent)
/// for this journal's signer at this journal's table: one slot holds one frame
/// of a seat, whatever its type -- a call and a fold at one turn collide, as a
/// reveal and a muck do.
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
pub struct Entry {
    #[n(0)]
    pub hand: u64,
    #[n(1)]
    pub sequence: u64,
    #[cbor(n(2), with = "minicbor::bytes")]
    pub parent: [u8; 32],
    /// The frame's event type code, for the log and the replay.
    #[n(3)]
    pub kind: u16,
    /// The frame's bytes, exactly as they leave -- a replay sends these again.
    #[cbor(n(4), with = "minicbor::bytes")]
    pub frame: Vec<u8>,
    /// A `DECK_INIT`'s deck secret, in the same sync as its frame: a restart
    /// takes the key back instead of drawing a fresh one at the same slot.
    #[n(5)]
    pub secret: Option<minicbor::bytes::ByteVec>,
}

impl Entry {
    fn key(&self) -> (u64, u64, [u8; 32]) {
        (self.hand, self.sequence, self.parent)
    }
}

/// Why the journal cannot do what was asked. Every one of them means: send
/// nothing of the batch.
#[derive(Debug)]
pub enum JournalError {
    /// Another process holds this table's journal.
    Locked,
    /// The header names another table, another signer or another format.
    Foreign(&'static str),
    /// A record over [`RECORD_CAP`], or a segment past [`SEGMENT_CAP`].
    TooLarge,
    /// A slot this journal holds another body at: the second is never written.
    Conflict { hand: u64, sequence: u64, kind: u16 },
    /// A hand this journal does not cover, or one a failed write left broken.
    Uncovered,
    /// The writer's queue is full.
    Full,
    /// The writer did not answer within the bound, or is still on a job a
    /// caller stopped waiting on; that job may yet land.
    Timeout,
    /// The writer thread is gone.
    Gone,
    Io(io::Error),
}

impl std::fmt::Display for JournalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Locked => f.write_str("another client holds this table's signing journal"),
            Self::Foreign(what) => write!(f, "the signing journal here is another's: {what}"),
            Self::TooLarge => f.write_str("a record or a segment over the journal's cap"),
            Self::Conflict { hand, sequence, kind } => write!(
                f,
                "the signing journal holds another frame at hand #{hand} sequence {sequence} (this one of type {kind:#06x})"
            ),
            Self::Uncovered => f.write_str("the signing journal does not cover that hand"),
            Self::Full => f.write_str("the signing journal's writer is behind"),
            Self::Timeout => f.write_str("the signing journal did not confirm the write in time"),
            Self::Gone => f.write_str("the signing journal's writer is gone"),
            Self::Io(e) => write!(f, "the signing journal could not be written: {e}"),
        }
    }
}

impl From<io::Error> for JournalError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// What the journal holds, read by the node without waiting on the writer.
#[derive(Debug, Default)]
struct Index {
    entries: BTreeMap<(u64, u64, [u8; 32]), Entry>,
    /// Hands whose segment did not verify, or that a failed write left in a
    /// state this life could not cut back: whatever was signed there is
    /// unknown, so the caller takes every slot of it as signed.
    uncovered: BTreeSet<u64>,
}

/// The journal's directory for `table_id` under `profile_dir`.
pub fn journal_dir(profile_dir: &Path, table_id: &[u8; 32]) -> PathBuf {
    let hex: String = table_id.iter().map(|b| format!("{b:02x}")).collect();
    profile_dir.join("journal").join(hex)
}

/// Remove the journal of `table_id` -- the table left for good: a signed
/// leave, or out of it for good. Refused while another holds it; a journal
/// still open here is to be closed first.
pub fn remove(profile_dir: &Path, table_id: &[u8; 32]) -> Result<(), JournalError> {
    remove_dir(&journal_dir(profile_dir, table_id))
}

/// Remove every journal in `profile_dir` untouched for `older_than` and held by
/// nobody, and say how many went -- a table not played for a week is not
/// resumed. Called as the client starts.
pub fn sweep(profile_dir: &Path, older_than: Duration) -> usize {
    let Ok(items) = fs::read_dir(profile_dir.join("journal")) else {
        return 0;
    };
    let mut gone = 0;
    for item in items.flatten() {
        let dir = item.path();
        let newest = fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|f| f.metadata().ok()?.modified().ok())
            .max();
        let stale = newest.is_none_or(|t| t.elapsed().is_ok_and(|age| age >= older_than));
        if dir.is_dir() && stale && remove_dir(&dir).is_ok() {
            gone += 1;
        }
    }
    gone
}

/// Everything in a journal's directory, its lock last, and the directory: only
/// once this client holds the lock.
fn remove_dir(dir: &Path) -> Result<(), JournalError> {
    if !dir.exists() {
        return Ok(());
    }
    let lock = match crate::storage::profile::open_exclusive(&dir.join("lock")) {
        Ok(Some(file)) => file,
        Ok(None) | Err(crate::storage::profile::LockError::InUse) => return Err(JournalError::Locked),
        Err(crate::storage::profile::LockError::Unavailable(e)) => return Err(JournalError::Io(e)),
    };
    for item in fs::read_dir(dir)? {
        let path = item?.path();
        if path.file_name().is_some_and(|n| n != "lock") {
            fs::remove_file(&path)?;
        }
    }
    drop(lock);
    fs::remove_file(dir.join("lock"))?;
    fs::remove_dir(dir)?;
    Ok(())
}

fn segment_path(dir: &Path, hand: u64) -> PathBuf {
    dir.join(format!("{hand}.wal"))
}

/// What of `entries` is to be written: every one the journal does not already
/// hold, once, encoded -- or why none of them is. Checked whole before a byte
/// is written.
fn plan(index: &Index, covers_from: u64, entries: &[Entry]) -> Result<(Vec<Entry>, Vec<Vec<u8>>), JournalError> {
    let mut fresh: Vec<Entry> = Vec::new();
    let mut bodies = Vec::new();
    for e in entries {
        if e.hand < covers_from || index.uncovered.contains(&e.hand) {
            return Err(JournalError::Uncovered);
        }
        match index.entries.get(&e.key()).or_else(|| fresh.iter().find(|f| f.key() == e.key())) {
            Some(held) if held.frame == e.frame => continue,
            Some(_) => return Err(JournalError::Conflict { hand: e.hand, sequence: e.sequence, kind: e.kind }),
            None => {}
        }
        let body = minicbor::to_vec(e).map_err(|err| JournalError::Io(io::Error::other(err.to_string())))?;
        if body.len() > RECORD_CAP {
            return Err(JournalError::TooLarge);
        }
        bodies.push(body);
        fresh.push(e.clone());
    }
    Ok((fresh, bodies))
}

/// The files, owned by the writer thread once the journal is open.
struct Files {
    dir: PathBuf,
    /// Held for the journal's life: the lock is the journal's, and fails closed.
    _lock: fs::File,
    header: Header,
    covers_from: Arc<AtomicU64>,
    segments: BTreeMap<u64, fs::File>,
    /// A test's failure: the write of this record of the next batch fails,
    /// leaving the half record a short write would.
    #[cfg(test)]
    fail_after: Option<usize>,
    /// A test's failure: the next batch's sync fails.
    #[cfg(test)]
    fail_sync: bool,
}

impl Files {
    fn segment(&mut self, hand: u64) -> io::Result<&mut fs::File> {
        if !self.segments.contains_key(&hand) {
            let path = segment_path(&self.dir, hand);
            let file = fs::OpenOptions::new().create(true).append(true).read(true).open(&path)?;
            file.sync_all()?;
            // At every segment's first open in a life: a segment whose creator
            // died before the directory was synced gets it now.
            sync_dir(&self.dir)?;
            self.segments.insert(hand, file);
        }
        Ok(self.segments.get_mut(&hand).expect("just inserted"))
    }

    /// Append `fresh` (encoded as `bodies`) to their hands' segments, then sync
    /// each segment touched: durable when this returns `Ok`. A failure cuts
    /// each segment back to its length before the batch; a hand whose segment
    /// cannot be cut back, or whose sync failed, goes to `broken` -- a failed
    /// sync is never trusted again in this life (Linux may drop the dirty pages
    /// and report the next sync clean).
    fn append(&mut self, fresh: &[Entry], bodies: &[Vec<u8>], broken: &mut Vec<u64>) -> Result<(), JournalError> {
        let mut before: BTreeMap<u64, u64> = BTreeMap::new();
        let mut grown: BTreeMap<u64, u64> = BTreeMap::new();
        for (e, body) in fresh.iter().zip(bodies) {
            if !before.contains_key(&e.hand) {
                let len = self.segment(e.hand)?.metadata()?.len();
                before.insert(e.hand, len);
            }
            *grown.entry(e.hand).or_default() += u64::try_from(RECORD_HEAD + body.len()).expect("under the cap");
        }
        if grown.iter().any(|(hand, g)| before[hand] + g > SEGMENT_CAP) {
            return Err(JournalError::TooLarge);
        }
        let failed = match self.write_records(fresh, bodies) {
            Err(e) => Some((e, false)),
            Ok(touched) => touched.into_iter().find_map(|hand| {
                let synced = if self.injected_sync_failure() {
                    Err(io::Error::other("a failed sync asked for by a test"))
                } else {
                    self.segment(hand).and_then(|f| f.sync_all())
                };
                synced.err().map(|e| (JournalError::Io(e), true))
            }),
        };
        if let Some((e, sync_failed)) = failed {
            for (hand, len) in &before {
                // A handle of its own: an appending one cannot cut on Windows.
                self.segments.remove(hand);
                let cut = fs::OpenOptions::new().write(true).open(segment_path(&self.dir, *hand)).and_then(|f| {
                    f.set_len(*len)?;
                    f.sync_all()
                });
                if sync_failed || cut.is_err() {
                    broken.push(*hand);
                }
            }
            return Err(e);
        }
        Ok(())
    }

    /// Write every record, and say which hands' segments it touched.
    fn write_records(&mut self, fresh: &[Entry], bodies: &[Vec<u8>]) -> Result<BTreeSet<u64>, JournalError> {
        let mut touched = BTreeSet::new();
        for (i, (e, body)) in fresh.iter().zip(bodies).enumerate() {
            let mut rec = Vec::with_capacity(RECORD_HEAD + body.len());
            rec.extend_from_slice(&u32::try_from(body.len()).expect("under the cap").to_le_bytes());
            rec.extend_from_slice(&checksum(body));
            rec.extend_from_slice(body);
            self.injected_failure(i, e.hand, &rec)?;
            self.segment(e.hand)?.write_all(&rec)?;
            touched.insert(e.hand);
        }
        Ok(touched)
    }

    #[cfg(test)]
    fn injected_failure(&mut self, i: usize, hand: u64, rec: &[u8]) -> Result<(), JournalError> {
        if self.fail_after == Some(i) {
            self.fail_after = None;
            self.segment(hand)?.write_all(&rec[..rec.len() / 2])?;
            return Err(JournalError::Io(io::Error::other("a failure asked for by a test")));
        }
        Ok(())
    }

    #[cfg(not(test))]
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    fn injected_failure(&mut self, _i: usize, _hand: u64, _rec: &[u8]) -> Result<(), JournalError> {
        Ok(())
    }

    #[cfg(test)]
    fn injected_sync_failure(&mut self) -> bool {
        std::mem::take(&mut self.fail_sync)
    }

    #[cfg(not(test))]
    #[allow(clippy::unused_self)]
    fn injected_sync_failure(&mut self) -> bool {
        false
    }

    /// Move the first covered hand to `keep_from`, durably, then remove every
    /// segment of a hand before it: a pruned hand reads as uncovered, never as
    /// unsigned.
    fn prune(&mut self, keep_from: u64) -> io::Result<()> {
        if keep_from > self.header.covers_from {
            let mut h = self.header.clone();
            h.covers_from = keep_from;
            write_header(&self.dir, &h)?;
            self.header = h;
            self.covers_from.store(keep_from, Ordering::SeqCst);
        }
        self.segments.retain(|hand, _| *hand >= keep_from);
        for hand in hands_on_disk(&self.dir)? {
            if hand < keep_from {
                fs::remove_file(segment_path(&self.dir, hand))?;
            }
        }
        sync_dir(&self.dir)
    }
}

/// The checksum of a record body: BLAKE3's first 16 bytes.
fn checksum(body: &[u8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    out.copy_from_slice(&blake3::hash(body).as_bytes()[..16]);
    out
}

/// A directory's entries, synced where that is a thing: NTFS journals its own
/// metadata.
fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(not(windows))]
    {
        fs::File::open(dir)?.sync_all()?;
    }
    #[cfg(windows)]
    {
        let _ = dir;
    }
    Ok(())
}

/// The header, written whole or not at all: a temporary file, synced, renamed.
fn write_header(dir: &Path, h: &Header) -> io::Result<()> {
    let bytes = minicbor::to_vec(h).map_err(|e| io::Error::other(e.to_string()))?;
    let tmp = dir.join("header.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, dir.join("header"))?;
    sync_dir(dir)
}

/// The hand numbers of the segments in `dir`. Only this journal's own names
/// (`<hand>.wal`, no sign, no leading zero): any other file is not one of its
/// segments.
fn hands_on_disk(dir: &Path) -> io::Result<Vec<u64>> {
    let mut out = Vec::new();
    for item in fs::read_dir(dir)? {
        let name = item?.file_name();
        let name = name.to_string_lossy();
        if let Some(hand) = name.strip_suffix(".wal").and_then(|n| n.parse::<u64>().ok()) {
            if *name == format!("{hand}.wal") {
                out.push(hand);
            }
        }
    }
    out.sort_unstable();
    Ok(out)
}

/// What one segment says: its entries, and whether it leaves the hand
/// uncovered. Only a short tail is cut from the file, here.
fn read_segment(path: &Path, hand: u64) -> io::Result<(Vec<Entry>, bool)> {
    let mut file = fs::OpenOptions::new().read(true).write(true).open(path)?;
    if file.metadata()?.len() > SEGMENT_CAP {
        return Ok((Vec::new(), true));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let mut entries = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        let rest = &bytes[at..];
        let short = rest.len() < RECORD_HEAD
            || rest.iter().all(|b| *b == 0)
            || usize::try_from(u32::from_le_bytes(rest[..4].try_into().expect("four bytes")))
                .is_ok_and(|len| len <= RECORD_CAP && rest.len() < RECORD_HEAD + len);
        if short {
            // Torn by a crash, never confirmed: cut.
            file.set_len(u64::try_from(at).expect("a file offset"))?;
            file.sync_all()?;
            return Ok((entries, false));
        }
        match parse_record(rest) {
            Some((entry, used)) if entry.hand == hand => {
                entries.push(entry);
                at += used;
            }
            // A complete record that does not verify, a length over the cap,
            // another hand's record: never cut -- the hand is uncovered.
            _ => return Ok((entries, true)),
        }
    }
    Ok((entries, false))
}

/// One record at the start of `bytes`, and its length.
fn parse_record(bytes: &[u8]) -> Option<(Entry, usize)> {
    if bytes.len() < RECORD_HEAD {
        return None;
    }
    let len = usize::try_from(u32::from_le_bytes(bytes[..4].try_into().ok()?)).ok()?;
    if len > RECORD_CAP || bytes.len() < RECORD_HEAD + len {
        return None;
    }
    let body = &bytes[RECORD_HEAD..RECORD_HEAD + len];
    if bytes[4..RECORD_HEAD] != checksum(body) {
        return None;
    }
    let entry: Entry = minicbor::decode(body).ok()?;
    Some((entry, RECORD_HEAD + len))
}

type Reply = mpsc::Sender<Result<(), JournalError>>;

enum Job {
    Write(Vec<Entry>, Reply),
    Prune(u64, Reply),
    #[cfg(test)]
    Stall(Duration),
    #[cfg(test)]
    FailAfter(usize),
    #[cfg(test)]
    FailSync,
}

/// An open journal: the node's handle on it.
pub struct Handle {
    id: [u8; 16],
    covers_from: Arc<AtomicU64>,
    index: Arc<RwLock<Index>>,
    tx: Option<mpsc::SyncSender<(u64, Job)>>,
    /// The number of the last job sent, the last the writer finished, and the
    /// last a caller stopped waiting on: until the writer has finished that
    /// one, every write fails at once instead of queueing behind a hung disk.
    sent: AtomicU64,
    finished: Arc<AtomicU64>,
    stuck: AtomicU64,
    thread: Option<std::thread::JoinHandle<()>>,
    /// The application key whose frames this journal records.
    signer: [u8; 32],
    /// A journal that would not open (`Handle::refusing`): every write fails,
    /// every hand is uncovered -- the table's journal fails closed.
    refusing: bool,
    /// Phase D: the hands this client sits out because the journal refused a
    /// frame of them -- nothing more of such a hand leaves (`silence`).
    silenced: std::sync::Mutex<BTreeSet<u64>>,
    /// What the node is to say about the journal at its next tick -- a write
    /// that failed, a second body refused -- at most `NOTES_CAP`, the oldest
    /// dropped first.
    notes: std::sync::Mutex<std::collections::VecDeque<String>>,
}

/// The notes a journal keeps for the node at most.
const NOTES_CAP: usize = 64;

/// The journal's files and what they hold, as an open finds them.
fn open_files(dir: PathBuf, table_id: [u8; 32], signer: [u8; 32], mode: Mode) -> Result<(Files, Index), JournalError> {
    {
        fs::create_dir_all(&dir)?;
        if let Some(parent) = dir.parent() {
            sync_dir(parent)?;
            if let Some(grand) = parent.parent() {
                sync_dir(grand)?;
            }
        }
        // Fails closed: a lock this client cannot take, for any reason, is a
        // journal it cannot keep.
        let lock = match crate::storage::profile::open_exclusive(&dir.join("lock")) {
            Ok(Some(file)) => file,
            Ok(None) | Err(crate::storage::profile::LockError::InUse) => return Err(JournalError::Locked),
            Err(crate::storage::profile::LockError::Unavailable(e)) => return Err(JournalError::Io(e)),
        };
        let on_disk = hands_on_disk(&dir)?;
        // A header lost -- or one that does not decode, which is the same loss:
        // a new one, covering none of the hands of the segments still beside it,
        // whose records it is not this header's to vouch for.
        let fresh_header = || -> Result<Header, JournalError> {
            let id = crate::security::rng::array::<16>()
                .map_err(|e| JournalError::Io(io::Error::other(format!("no randomness for the journal's id: {e}"))))?;
            let past = on_disk.last().map_or(0, |h| h + 1);
            let h = Header { id, format: JOURNAL_FORMAT, table_id, signer, covers_from: mode.first_covered().max(past) };
            write_header(&dir, &h)?;
            Ok(h)
        };
        let header = match fs::read(dir.join("header")) {
            Ok(bytes) => match minicbor::decode::<Header>(&bytes) {
                Ok(h) => {
                    if h.format != JOURNAL_FORMAT {
                        return Err(JournalError::Foreign("another format"));
                    }
                    if h.table_id != table_id {
                        return Err(JournalError::Foreign("another table"));
                    }
                    if h.signer != signer {
                        return Err(JournalError::Foreign("another signer"));
                    }
                    h
                }
                Err(_) => fresh_header()?,
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => fresh_header()?,
            Err(e) => return Err(JournalError::Io(e)),
        };
        let mut index = Index::default();
        // A segment before the first covered hand is a prune's leftover: the
        // next prune removes it.
        for hand in on_disk.into_iter().filter(|h| *h >= header.covers_from) {
            let (entries, uncovered) = read_segment(&segment_path(&dir, hand), hand)?;
            if uncovered {
                index.uncovered.insert(hand);
            }
            for e in entries {
                index.entries.insert(e.key(), e);
            }
        }
        let files = Files {
            dir,
            _lock: lock,
            covers_from: Arc::new(AtomicU64::new(header.covers_from)),
            header,
            segments: BTreeMap::new(),
            #[cfg(test)]
            fail_after: None,
            #[cfg(test)]
            fail_sync: false,
        };
        Ok((files, index))
    }
}

impl Handle {
    /// Open the journal of `table_id` in `profile_dir` for `signer` as a fresh
    /// table's (`Mode::Fresh`, covering from `covers_from`), waiting as long as
    /// the disk takes -- the tests' and the tools' open.
    pub fn open(profile_dir: &Path, table_id: [u8; 32], signer: [u8; 32], covers_from: u64) -> Result<Handle, JournalError> {
        Self::open_with(profile_dir, table_id, signer, Mode::Fresh { first_hand: covers_from }, Duration::from_secs(3600))
    }

    /// Open the journal of `table_id` in `profile_dir` for `signer` -- creating
    /// it where there is none, as `mode` says, and from past any segment
    /// already there -- take its lock (held by no other process, or the journal
    /// is refused), read every segment, and start its writer, all of it on the
    /// writer's own thread: a hung disk costs the caller `timeout` and no more.
    /// An open that times out goes on alone and lets everything go when it
    /// finds nobody waiting for it.
    pub fn open_with(
        profile_dir: &Path,
        table_id: [u8; 32],
        signer: [u8; 32],
        mode: Mode,
        timeout: Duration,
    ) -> Result<Handle, JournalError> {
        let dir = journal_dir(profile_dir, &table_id);
        let (tx, rx) = mpsc::sync_channel::<(u64, Job)>(WRITER_QUEUE);
        let (opened_tx, opened_rx) = mpsc::channel();
        let finished = Arc::new(AtomicU64::new(0));
        let done = Arc::clone(&finished);
        let thread = std::thread::Builder::new()
            .name("signing-journal".into())
            .spawn(move || {
                let (mut files, index) = match open_files(dir, table_id, signer, mode) {
                    Ok(opened) => opened,
                    Err(e) => {
                        let _ = opened_tx.send(Err(e));
                        return;
                    }
                };
                let shared = Arc::new(RwLock::new(index));
                let cover = Arc::clone(&files.covers_from);
                let id = files.header.id;
                if opened_tx.send(Ok((id, Arc::clone(&cover), Arc::clone(&shared)))).is_err() {
                    // Nobody waits: the open timed out. The lock goes with `files`.
                    return;
                }
                for (n, job) in rx {
                    match job {
                        Job::Write(entries, reply) => {
                            let planned = {
                                let idx = shared.read().unwrap_or_else(PoisonError::into_inner);
                                plan(&idx, cover.load(Ordering::SeqCst), &entries)
                            };
                            let result = planned.and_then(|(fresh, bodies)| {
                                let mut broken = Vec::new();
                                let written = files.append(&fresh, &bodies, &mut broken);
                                let mut idx = shared.write().unwrap_or_else(PoisonError::into_inner);
                                idx.uncovered.extend(broken);
                                written.map(|()| {
                                    for e in fresh {
                                        idx.entries.insert(e.key(), e);
                                    }
                                })
                            });
                            done.store(n, Ordering::SeqCst);
                            let _ = reply.send(result);
                        }
                        Job::Prune(keep_from, reply) => {
                            let result = files.prune(keep_from).map_err(JournalError::Io);
                            if result.is_ok() {
                                let mut idx = shared.write().unwrap_or_else(PoisonError::into_inner);
                                idx.entries.retain(|(hand, _, _), _| *hand >= keep_from);
                                idx.uncovered.retain(|hand| *hand >= keep_from);
                            }
                            done.store(n, Ordering::SeqCst);
                            let _ = reply.send(result);
                        }
                        #[cfg(test)]
                        Job::Stall(d) => std::thread::sleep(d),
                        #[cfg(test)]
                        Job::FailAfter(records) => files.fail_after = Some(records),
                        #[cfg(test)]
                        Job::FailSync => files.fail_sync = true,
                    }
                    done.store(n, Ordering::SeqCst);
                }
            })
            .map_err(JournalError::Io)?;
        let (id, covers_from, index) = match opened_rx.recv_timeout(timeout) {
            Ok(Ok(opened)) => opened,
            Ok(Err(e)) => return Err(e),
            Err(mpsc::RecvTimeoutError::Timeout) => return Err(JournalError::Timeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err(JournalError::Gone),
        };
        Ok(Handle {
            id,
            covers_from,
            index,
            tx: Some(tx),
            sent: AtomicU64::new(0),
            finished,
            stuck: AtomicU64::new(0),
            thread: Some(thread),
            signer,
            refusing: false,
            silenced: std::sync::Mutex::new(BTreeSet::new()),
            notes: std::sync::Mutex::new(std::collections::VecDeque::new()),
        })
    }

    /// The journal of a table whose own would not open: it records nothing and
    /// covers no hand, so every frame of a hand's chain is refused and its hand
    /// sat out (phase D) -- the table's journal fails closed until it opens.
    pub fn refusing(signer: [u8; 32]) -> Handle {
        Handle {
            id: [0; 16],
            covers_from: Arc::new(AtomicU64::new(u64::MAX)),
            index: Arc::new(RwLock::new(Index::default())),
            tx: None,
            sent: AtomicU64::new(0),
            finished: Arc::new(AtomicU64::new(0)),
            stuck: AtomicU64::new(0),
            thread: None,
            signer,
            refusing: true,
            silenced: std::sync::Mutex::new(BTreeSet::new()),
            notes: std::sync::Mutex::new(std::collections::VecDeque::new()),
        }
    }

    /// Whether this is [`Handle::refusing`]'s stand-in for a journal that would
    /// not open.
    pub fn is_refusing(&self) -> bool {
        self.refusing
    }

    /// Phase D: sit `hand` out -- nothing more of it leaves this client -- and
    /// say why, once.
    pub fn silence(&self, hand: u64, why: &str) {
        let fresh = self.silenced.lock().unwrap_or_else(PoisonError::into_inner).insert(hand);
        if fresh {
            self.note(format!(
                "this seat sits hand #{hand} out: {why} -- nothing of it is said, so nobody can hold two versions of a \
                 frame of it signed by this client (G11-R)"
            ));
        }
    }

    /// Whether `hand` is sat out ([`Handle::silence`]).
    pub fn is_silenced(&self, hand: u64) -> bool {
        self.silenced.lock().unwrap_or_else(PoisonError::into_inner).contains(&hand)
    }

    /// The application key whose frames this journal records.
    pub fn signer(&self) -> [u8; 32] {
        self.signer
    }

    /// Keep `line` for the node to say at its next tick.
    pub fn note(&self, line: String) {
        let mut notes = self.notes.lock().unwrap_or_else(PoisonError::into_inner);
        if notes.len() >= NOTES_CAP {
            notes.pop_front();
        }
        notes.push_back(line);
    }

    /// Every note kept since the last call.
    pub fn take_notes(&self) -> Vec<String> {
        self.notes.lock().unwrap_or_else(PoisonError::into_inner).drain(..).collect()
    }

    /// The journal's random id.
    pub fn id(&self) -> [u8; 16] {
        self.id
    }

    /// The first hand the journal covers.
    pub fn covers_from(&self) -> u64 {
        self.covers_from.load(Ordering::SeqCst)
    }

    /// The frame this client signed at (hand, sequence, parent), if the journal
    /// holds one.
    pub fn lookup(&self, hand: u64, sequence: u64, parent: &[u8; 32]) -> Option<Entry> {
        self.index.read().unwrap_or_else(PoisonError::into_inner).entries.get(&(hand, sequence, *parent)).cloned()
    }

    /// Whether the journal cannot vouch for `hand`: before the first hand it
    /// covers, or damaged in a way it does not cut. Every slot of an uncovered
    /// hand is to be taken as signed.
    pub fn is_uncovered(&self, hand: u64) -> bool {
        hand < self.covers_from() || self.index.read().unwrap_or_else(PoisonError::into_inner).uncovered.contains(&hand)
    }

    /// Record `entries` durably, waiting at most `timeout` for the writer.
    /// `Ok` only once every entry is synced -- or already held with the same
    /// frame; anything else means the batch must not be sent. A write that
    /// times out may land later -- durable, never sent, harmless -- and until
    /// it does every write fails at once.
    pub fn write(&self, entries: Vec<Entry>, timeout: Duration) -> Result<(), JournalError> {
        if entries.is_empty() {
            return Ok(());
        }
        let (reply_tx, reply_rx) = mpsc::channel();
        let n = self.send(Job::Write(entries, reply_tx))?;
        self.wait(n, &reply_rx, timeout, true)
    }

    /// Remove every segment of a hand before `keep_from` -- the boundary's
    /// call, once the next hand has left stage 0: the running hand and the
    /// retained one are all a journal ever needs. The first covered hand moves
    /// to `keep_from` first, durably.
    pub fn prune(&self, keep_from: u64, timeout: Duration) -> Result<(), JournalError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        // A prune that does not return in time is housekeeping that ran long:
        // it does not fail the next write at once (a write's own timeout does).
        let n = self.send(Job::Prune(keep_from, reply_tx))?;
        self.wait(n, &reply_rx, timeout, false)?;
        self.silenced.lock().unwrap_or_else(PoisonError::into_inner).retain(|h| *h >= keep_from);
        Ok(())
    }

    /// Stop the writer and wait at most `timeout` for it, so the lock is free
    /// when this returns `true` -- a table left, or a test opening again. A
    /// writer on a hung disk keeps the lock until its job returns.
    pub fn close(mut self, timeout: Duration) -> bool {
        self.tx = None;
        let start = Instant::now();
        while let Some(t) = self.thread.as_ref() {
            if t.is_finished() {
                let _ = self.thread.take().map(std::thread::JoinHandle::join);
                return true;
            }
            if start.elapsed() >= timeout {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        true
    }

    fn send(&self, job: Job) -> Result<u64, JournalError> {
        if self.finished.load(Ordering::SeqCst) < self.stuck.load(Ordering::SeqCst) {
            return Err(JournalError::Timeout);
        }
        let tx = self.tx.as_ref().ok_or(JournalError::Gone)?;
        let n = self.sent.fetch_add(1, Ordering::SeqCst) + 1;
        tx.try_send((n, job)).map_err(|e| match e {
            mpsc::TrySendError::Full(_) => JournalError::Full,
            mpsc::TrySendError::Disconnected(_) => JournalError::Gone,
        })?;
        Ok(n)
    }

    fn wait(
        &self,
        n: u64,
        reply: &mpsc::Receiver<Result<(), JournalError>>,
        timeout: Duration,
        stuck_on_timeout: bool,
    ) -> Result<(), JournalError> {
        match reply.recv_timeout(timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if stuck_on_timeout {
                    self.stuck.fetch_max(n, Ordering::SeqCst);
                }
                Err(JournalError::Timeout)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(JournalError::Gone),
        }
    }

    #[cfg(test)]
    fn test_job(&self, job: Job) {
        let n = self.sent.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(tx) = self.tx.as_ref() {
            let _ = tx.send((n, job));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Duration = Duration::from_secs(5);

    fn temp_profile(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("p2p-journal-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(hand: u64, sequence: u64, byte: u8) -> Entry {
        Entry { hand, sequence, parent: [byte; 32], kind: 0x0301, frame: vec![byte; 100], secret: None }
    }

    fn open(p: &Path, covers_from: u64) -> Handle {
        Handle::open(p, [1; 32], [2; 32], covers_from).unwrap()
    }

    fn append_raw(path: &Path, bytes: &[u8]) {
        let mut f = fs::OpenOptions::new().append(true).open(path).unwrap();
        f.write_all(bytes).unwrap();
    }

    /// **Durable when `write` returns**: a journal closed and opened again
    /// holds it; another parent is another slot.
    #[test]
    fn durable_when_write_returns() {
        let p = temp_profile("durable");
        let j = open(&p, 1);
        j.write(vec![entry(1, 3, 7), entry(1, 4, 8)], T).unwrap();
        assert_eq!(j.lookup(1, 3, &[7; 32]).unwrap().frame, vec![7; 100]);
        assert!(j.close(T), "the lock free at once");
        let j = open(&p, 1);
        assert_eq!(j.lookup(1, 4, &[8; 32]).unwrap().frame, vec![8; 100]);
        assert!(j.lookup(1, 4, &[9; 32]).is_none());
        assert!(!j.is_uncovered(1));
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **Only a short tail is cut**: fewer bytes than a head, fewer than its own
    /// length, or zeros to the end -- each a record torn before it was confirmed.
    #[test]
    fn only_a_short_tail_is_cut() {
        let p = temp_profile("short");
        let dir = journal_dir(&p, &[1; 32]);
        let mut under_its_length = vec![60u8, 0, 0, 0];
        under_its_length.extend_from_slice(&[9u8; 30]);
        for (hand, tail) in [(1u64, vec![200u8, 0, 0, 0, 1, 2, 3]), (2, under_its_length), (3, vec![0u8; 50])] {
            let j = open(&p, 1);
            j.write(vec![entry(hand, 3, 7), entry(hand, 4, 8)], T).unwrap();
            assert!(j.close(T));
            let path = segment_path(&dir, hand);
            let good = fs::metadata(&path).unwrap().len();
            append_raw(&path, &tail);
            let j = open(&p, 1);
            assert!(!j.is_uncovered(hand), "hand {hand}: a torn tail was never sent");
            assert_eq!(fs::metadata(&path).unwrap().len(), good, "hand {hand}: cut back to the last good record");
            assert!(j.lookup(hand, 4, &[8; 32]).is_some());
            assert!(j.close(T));
        }
        let _ = fs::remove_dir_all(&p);
    }

    /// **A complete record that fails is never cut**: the last confirmed record
    /// damaged, or a record of another hand in a segment, leaves the file as it
    /// is and the hand uncovered -- nothing more is signed there.
    #[test]
    fn a_complete_record_that_fails_is_never_cut() {
        let p = temp_profile("damage");
        let dir = journal_dir(&p, &[1; 32]);
        let j = open(&p, 1);
        j.write(vec![entry(1, 3, 7), entry(1, 4, 8)], T).unwrap();
        j.write(vec![entry(2, 3, 9)], T).unwrap();
        assert!(j.close(T));
        // Hand 1: a byte of its LAST record's body flipped.
        let path1 = segment_path(&dir, 1);
        let mut bytes = fs::read(&path1).unwrap();
        let n = bytes.len();
        bytes[n - 5] ^= 0xff;
        fs::write(&path1, &bytes).unwrap();
        // Hand 2's segment holding a record of hand 5.
        let path2 = segment_path(&dir, 2);
        let body = minicbor::to_vec(entry(5, 3, 1)).unwrap();
        let mut rec = u32::try_from(body.len()).unwrap().to_le_bytes().to_vec();
        rec.extend_from_slice(&checksum(&body));
        rec.extend_from_slice(&body);
        append_raw(&path2, &rec);
        let (len1, len2) = (fs::metadata(&path1).unwrap().len(), fs::metadata(&path2).unwrap().len());
        let j = open(&p, 1);
        assert!(j.is_uncovered(1) && j.is_uncovered(2), "what was signed there is unknown");
        assert_eq!(fs::metadata(&path1).unwrap().len(), len1, "never cut");
        assert_eq!(fs::metadata(&path2).unwrap().len(), len2, "never wiped");
        assert!(matches!(j.write(vec![entry(1, 9, 1)], T), Err(JournalError::Uncovered)));
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **A second body at a held slot is refused**; the same one is a copy,
    /// in a batch or after it.
    #[test]
    fn a_second_body_at_a_held_slot_is_refused() {
        let p = temp_profile("conflict");
        let j = open(&p, 1);
        j.write(vec![entry(1, 3, 7)], T).unwrap();
        j.write(vec![entry(1, 3, 7)], T).unwrap();
        let mut other = entry(1, 3, 7);
        other.frame = vec![1u8; 100];
        assert!(matches!(j.write(vec![other], T), Err(JournalError::Conflict { .. })));
        j.write(vec![entry(1, 5, 5), entry(1, 5, 5)], T).unwrap();
        let mut twice = entry(1, 6, 6);
        twice.frame = vec![2u8; 100];
        assert!(matches!(j.write(vec![entry(1, 6, 6), twice], T), Err(JournalError::Conflict { .. })), "two bodies in one batch");
        assert!(j.lookup(1, 6, &[6; 32]).is_none(), "a refused batch writes nothing");
        assert_eq!(j.lookup(1, 3, &[7; 32]).unwrap().frame, vec![7; 100], "the first stands");
        assert!(j.close(T));
        let j = open(&p, 1);
        assert_eq!(j.lookup(1, 5, &[5; 32]).unwrap().frame, vec![5; 100], "a copy is written once");
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **A failed write leaves nothing behind**: cut back to where it began,
    /// the hand still covered, the records before it whole.
    #[test]
    fn a_failed_write_leaves_nothing_behind() {
        let p = temp_profile("residue");
        let dir = journal_dir(&p, &[1; 32]);
        let j = open(&p, 1);
        j.write(vec![entry(1, 3, 7)], T).unwrap();
        let good = fs::metadata(segment_path(&dir, 1)).unwrap().len();
        j.test_job(Job::FailAfter(1));
        assert!(matches!(j.write(vec![entry(1, 4, 8), entry(1, 5, 9)], T), Err(JournalError::Io(_))));
        assert_eq!(fs::metadata(segment_path(&dir, 1)).unwrap().len(), good, "cut back");
        assert!(j.lookup(1, 4, &[8; 32]).is_none());
        assert!(!j.is_uncovered(1));
        j.write(vec![entry(1, 4, 8)], T).unwrap();
        assert!(j.close(T));
        let j = open(&p, 1);
        assert!(!j.is_uncovered(1));
        assert!(j.lookup(1, 3, &[7; 32]).is_some() && j.lookup(1, 4, &[8; 32]).is_some());
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **A failed sync sits the hand out for this life**: cut back, and nothing
    /// more written there -- a sync that failed once is never trusted again;
    /// another hand writes on, and the next life reads the hand as it stands.
    #[test]
    fn a_failed_sync_sits_the_hand_out() {
        let p = temp_profile("sync");
        let j = open(&p, 1);
        j.write(vec![entry(1, 3, 7)], T).unwrap();
        j.test_job(Job::FailSync);
        assert!(matches!(j.write(vec![entry(1, 4, 8)], T), Err(JournalError::Io(_))));
        assert!(j.is_uncovered(1) && j.lookup(1, 4, &[8; 32]).is_none());
        assert!(matches!(j.write(vec![entry(1, 5, 5)], T), Err(JournalError::Uncovered)));
        j.write(vec![entry(2, 3, 1)], T).unwrap();
        assert!(j.close(T));
        let j = open(&p, 1);
        assert!(j.lookup(1, 3, &[7; 32]).is_some() && j.lookup(1, 4, &[8; 32]).is_none(), "cut back");
        assert!(j.lookup(2, 3, &[1; 32]).is_some());
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **A second process is refused** while the first holds the journal; a
    /// closed journal opens again at once.
    #[test]
    fn a_second_open_is_refused() {
        let p = temp_profile("second");
        let j = open(&p, 1);
        assert!(matches!(Handle::open(&p, [1; 32], [2; 32], 1), Err(JournalError::Locked)));
        assert!(j.close(T));
        assert!(open(&p, 1).close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **Another signer's journal is refused**; the header's first covered hand
    /// stands; a lost header covers none of the segments beside it.
    #[test]
    fn the_header_is_the_journals() {
        let p = temp_profile("header");
        let j = open(&p, 5);
        j.write(vec![entry(6, 3, 7)], T).unwrap();
        assert!(j.close(T));
        assert!(matches!(Handle::open(&p, [1; 32], [3; 32], 5), Err(JournalError::Foreign(_))));
        let j = open(&p, 9);
        assert_eq!(j.covers_from(), 5, "the header's, not the caller's, once it exists");
        assert!(j.is_uncovered(4) && !j.is_uncovered(5));
        assert!(j.close(T));
        fs::remove_file(journal_dir(&p, &[1; 32]).join("header")).unwrap();
        let j = open(&p, 2);
        assert!(j.is_uncovered(6), "a lost header vouches for no segment left beside it");
        assert_eq!(j.covers_from(), 7);
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **A hung write times out, then every write fails at once until it
    /// lands** -- durable, never sent, harmless.
    #[test]
    fn a_hung_write_times_out_and_lands_later() {
        let p = temp_profile("hung");
        let j = open(&p, 1);
        j.test_job(Job::Stall(Duration::from_millis(400)));
        assert!(matches!(j.write(vec![entry(1, 3, 7)], Duration::from_millis(50)), Err(JournalError::Timeout)));
        let quick = Instant::now();
        assert!(matches!(j.write(vec![entry(1, 4, 8)], T), Err(JournalError::Timeout)), "fails at once while hung");
        assert!(quick.elapsed() < Duration::from_secs(1));
        assert!(j.lookup(1, 3, &[7; 32]).is_none(), "a lookup is never held up by the disk");
        let landed = (0..100).any(|_| {
            std::thread::sleep(Duration::from_millis(50));
            j.lookup(1, 3, &[7; 32]).is_some()
        });
        assert!(landed, "landed after its timeout");
        // The writer counts the job finished just after indexing it.
        let retried = (0..100).any(|_| match j.write(vec![entry(1, 4, 8)], T) {
            Err(JournalError::Timeout) => {
                std::thread::sleep(Duration::from_millis(10));
                false
            }
            other => {
                other.unwrap();
                true
            }
        });
        assert!(retried, "writes again once the hung one landed");
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **A full queue is a failure**, not a wait.
    #[test]
    fn a_full_queue_is_a_failure() {
        let p = temp_profile("full");
        let j = open(&p, 1);
        j.test_job(Job::Stall(Duration::from_millis(300)));
        let full = (0..WRITER_QUEUE + 4).any(|_| matches!(j.send(Job::Prune(0, mpsc::channel().0)), Err(JournalError::Full)));
        assert!(full, "the queue holds {WRITER_QUEUE} jobs");
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **A deck secret rides with its `DECK_INIT`**, in one sync.
    #[test]
    fn a_deck_secret_rides_with_its_deck_init() {
        let p = temp_profile("secret");
        let j = open(&p, 1);
        let mut e = entry(1, 1, 7);
        e.secret = Some(minicbor::bytes::ByteVec::from(vec![42u8; 32]));
        j.write(vec![e], T).unwrap();
        assert!(j.close(T));
        let j = open(&p, 1);
        let back = j.lookup(1, 1, &[7; 32]).unwrap();
        assert_eq!(back.secret.map(|s| s.to_vec()), Some(vec![42u8; 32]));
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **The running hand and the retained one are all that is kept, and a
    /// pruned hand reads as uncovered, never as unsigned** -- the journal never
    /// grows with the play.
    #[test]
    fn pruning_keeps_two_hands_and_forgets_none() {
        let p = temp_profile("prune");
        let j = open(&p, 1);
        for hand in 1..=40 {
            j.write(vec![entry(hand, 3, 7), entry(hand, 4, 8), entry(hand, 5, 9)], T).unwrap();
            if hand >= 2 {
                j.prune(hand - 1, T).unwrap();
            }
        }
        let dir = journal_dir(&p, &[1; 32]);
        assert_eq!(hands_on_disk(&dir).unwrap(), vec![39, 40]);
        let size: u64 = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().metadata().unwrap().len()).sum();
        assert!(size < 4_096, "forty hands played, two kept: {size} bytes");
        assert!(j.is_uncovered(38), "pruned: uncovered, never unsigned");
        assert!(j.lookup(38, 3, &[7; 32]).is_none());
        assert!(matches!(j.write(vec![entry(10, 3, 1)], T), Err(JournalError::Uncovered)), "nothing signed again there");
        assert!(j.close(T));
        let j = open(&p, 1);
        assert_eq!(j.covers_from(), 39, "durably");
        assert!(j.is_uncovered(38) && !j.is_uncovered(39));
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **A prune's leftover is never read back as covered**: a segment before
    /// the first covered hand -- a prune cut off after moving the header -- is
    /// uncovered, and the next prune removes it.
    #[test]
    fn a_prunes_leftover_stays_uncovered() {
        let p = temp_profile("leftover");
        let dir = journal_dir(&p, &[1; 32]);
        let j = open(&p, 1);
        j.write(vec![entry(1, 3, 7)], T).unwrap();
        j.write(vec![entry(2, 3, 7)], T).unwrap();
        assert!(j.close(T));
        let saved = fs::read(segment_path(&dir, 1)).unwrap();
        let j = open(&p, 1);
        j.prune(2, T).unwrap();
        assert!(j.close(T));
        fs::write(segment_path(&dir, 1), &saved).unwrap();
        let j = open(&p, 1);
        assert!(j.is_uncovered(1) && j.lookup(1, 3, &[7; 32]).is_none());
        j.prune(2, T).unwrap();
        assert_eq!(hands_on_disk(&dir).unwrap(), vec![2]);
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **A resume without a journal covers from the hand after the next**: the
    /// previous life may have signed up to the hand after the newest one seen;
    /// a fresh table's covers from its first hand; an existing header stands
    /// in either.
    #[test]
    fn a_resume_without_a_journal_covers_from_two_past_the_newest_seen() {
        let p = temp_profile("resume");
        let j = Handle::open_with(&p, [1; 32], [2; 32], Mode::Resume { newest_seen: 7 }, T).unwrap();
        assert_eq!(j.covers_from(), 9);
        assert!(j.is_uncovered(8) && !j.is_uncovered(9));
        assert!(j.close(T));
        let j = Handle::open_with(&p, [1; 32], [2; 32], Mode::Fresh { first_hand: 1 }, T).unwrap();
        assert_eq!(j.covers_from(), 9, "the header stands");
        assert!(j.close(T));
        let j = Handle::open_with(&p, [5; 32], [2; 32], Mode::Fresh { first_hand: 3 }, T).unwrap();
        assert_eq!(j.covers_from(), 3);
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **An open that times out lets the journal go**: the caller is told at
    /// once, and the lock is free once the open behind it is done.
    #[test]
    fn an_open_that_times_out_lets_the_journal_go() {
        let p = temp_profile("slowopen");
        assert!(matches!(
            Handle::open_with(&p, [1; 32], [2; 32], Mode::Fresh { first_hand: 1 }, Duration::ZERO),
            Err(JournalError::Timeout)
        ));
        let opened = (0..100).any(|_| {
            std::thread::sleep(Duration::from_millis(50));
            Handle::open_with(&p, [1; 32], [2; 32], Mode::Fresh { first_hand: 1 }, T).is_ok_and(|j| j.close(T))
        });
        assert!(opened, "free once the abandoned open let go");
        let _ = fs::remove_dir_all(&p);
    }

    /// **A left table's journal is removed, never one held**; the sweep takes
    /// the stale ones alone.
    #[test]
    fn a_left_tables_journal_goes_and_a_held_one_stays() {
        let p = temp_profile("remove");
        let j = open(&p, 1);
        j.write(vec![entry(1, 3, 7)], T).unwrap();
        assert!(matches!(remove(&p, &[1; 32]), Err(JournalError::Locked)), "held");
        assert!(j.close(T));
        remove(&p, &[1; 32]).unwrap();
        assert!(!journal_dir(&p, &[1; 32]).exists());
        remove(&p, &[1; 32]).unwrap();
        let a = open(&p, 1);
        assert!(a.close(T));
        let b = Handle::open(&p, [3; 32], [2; 32], 1).unwrap();
        assert_eq!(sweep(&p, STALE_AFTER), 0, "nothing a week old");
        assert_eq!(sweep(&p, Duration::ZERO), 1, "the free one goes, the held one stays");
        assert!(journal_dir(&p, &[3; 32]).exists() && !journal_dir(&p, &[1; 32]).exists());
        assert!(b.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **A journal that would not open refuses everything**: no write lands,
    /// every hand is uncovered; a silenced hand is said once and forgotten
    /// with the hands a prune drops.
    #[test]
    fn a_refusing_journal_refuses_and_a_silenced_hand_is_said_once() {
        let r = Handle::refusing([2; 32]);
        assert!(r.is_refusing() && r.is_uncovered(1) && r.is_uncovered(u64::MAX - 1));
        assert!(r.write(vec![entry(1, 3, 7)], T).is_err());
        assert!(r.lookup(1, 3, &[7; 32]).is_none());
        let p = temp_profile("silence");
        let j = open(&p, 1);
        assert!(!j.is_refusing());
        j.silence(4, "a test");
        j.silence(4, "a test");
        assert!(j.is_silenced(4) && !j.is_silenced(5));
        assert_eq!(j.take_notes().len(), 1, "said once");
        j.prune(5, T).unwrap();
        assert!(!j.is_silenced(4), "forgotten with the hand");
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **A header that does not decode is a lost one**: a new header, covering
    /// none of the hands of the segments beside it -- never a table refused for
    /// its life.
    #[test]
    fn a_header_that_does_not_decode_is_a_lost_one() {
        let p = temp_profile("garbled");
        let j = open(&p, 1);
        j.write(vec![entry(3, 3, 7)], T).unwrap();
        let id = j.id();
        assert!(j.close(T));
        fs::write(journal_dir(&p, &[1; 32]).join("header"), b"not cbor at all").unwrap();
        let j = open(&p, 1);
        assert_ne!(j.id(), id, "a new header");
        assert_eq!(j.covers_from(), 4, "past the segment beside it");
        assert!(j.is_uncovered(3));
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **A record over the cap is refused**, not written.
    #[test]
    fn a_record_over_the_cap_is_refused() {
        let p = temp_profile("cap");
        let j = open(&p, 1);
        let mut e = entry(1, 3, 7);
        e.frame = vec![0u8; RECORD_CAP + 1];
        assert!(matches!(j.write(vec![e], T), Err(JournalError::TooLarge)));
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }

    /// **Only this journal's own segment names are read** -- a `007.wal` or a
    /// `+7.wal` is no segment of hand 7.
    #[test]
    fn only_its_own_segment_names_are_read() {
        let p = temp_profile("names");
        let j = open(&p, 1);
        j.write(vec![entry(7, 3, 7)], T).unwrap();
        assert!(j.close(T));
        let dir = journal_dir(&p, &[1; 32]);
        fs::write(dir.join("007.wal"), b"not a segment").unwrap();
        fs::write(dir.join("+7.wal"), b"not a segment").unwrap();
        assert_eq!(hands_on_disk(&dir).unwrap(), vec![7]);
        let j = open(&p, 1);
        assert!(!j.is_uncovered(7));
        assert!(j.close(T));
        let _ = fs::remove_dir_all(&p);
    }
}
