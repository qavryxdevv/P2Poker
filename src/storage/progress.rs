//! `D-068`: what this player has earned here -- experience, cards, quests, a
//! season's stars, the table-manners meter -- in one file, `progress.json`.
//!
//! **Local and private**, like the results and the notes: nothing in it
//! reaches the wire, no other player sees any of it, and nothing the game
//! decides reads it.
//!
//! **JSON, by the owner's word**, where every other file of the profile is
//! CBOR. The envelope is four fields:
//!
//! ```text
//! { "format": 1, "generation": N, "seal": "<hex>", "body": { ... } }
//! ```
//!
//! **A crash at any moment costs the last change at most.** A save writes the
//! whole envelope to `progress.json.tmp` and flushes it, renames
//! `progress.json` to `progress.json.bak`, and renames the temporary file to
//! `progress.json`. A load reads all three and takes the valid one with the
//! highest `generation`, so whichever step a crash interrupted, the newest
//! complete copy is found -- and the interrupted rename is finished before
//! anything else is written.
//!
//! **An edited file is noticed.** `seal` is a BLAKE3 keyed hash over the format,
//! the generation and the exact bytes of `body`, under a key derived from this
//! profile's secret application key. A value changed in an editor fails it, and
//! so does a file copied in from another profile. **Nothing of the machine is in
//! it** -- no network address, host name, disk or user: the key is the
//! profile's alone, so a profile carried to another computer, or one day to a
//! phone whose network address changes with every network it joins, reads its
//! own file there as it did here (the backup in the settings carries both).
//! Such a file is **not
//! deleted**: it is set aside as `progress.rejected-<time>.json` (three at
//! most), the last valid copy is used, and the lobby says one neutral sentence.
//!
//! **What this is not** (`D-068`, the threat model): a secret. The key is in
//! the profile and the code is public; whoever reads both can compute the hash.
//! The file discourages, it does not secure -- which is why nothing another
//! player can see stands on it.
//!
//! A file of a **newer format** is never overwritten: the client shows nothing
//! it cannot read, keeps what it earns in memory, and leaves the file alone.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The format this build writes and reads.
pub const FORMAT: u32 = 1;
/// How many set-aside files are kept; the oldest go first.
pub const REJECTED_MAX: usize = 3;
/// The most lines the journal of changes holds.
pub const JOURNAL_MAX: usize = 200;
/// The most settled games remembered, so that none is counted twice.
pub const DONE_MAX: usize = 128;
/// The most games under way remembered at once.
pub const PENDING_MAX: usize = 8;
/// The most opponents remembered (as keyed hashes, never names or keys).
pub const OPPONENTS_MAX: usize = 1024;

/// BLAKE3's key-derivation context for the file's hash. A context string is a
/// domain separator and never a secret.
const SEAL_CONTEXT: &str = "p2p-poker 2026-09-19 progress.json keyed hash v1";
/// And for the hashes kept *inside* the file: a game's identity, an opponent.
const TAG_CONTEXT: &str = "p2p-poker 2026-09-19 progress.json tags v1";

/// A card of the album, once earned: the moment it remembers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Earned {
    pub when_unix_ms: u64,
    /// The table it was earned at; display data.
    pub table: String,
    /// The place finished in there, `0` where the card is not about a place.
    pub place: u32,
    /// One short line about the moment.
    pub note: String,
}

/// One quest: what it counts and how far it is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Quest {
    /// The template's id in the catalogue.
    pub id: String,
    pub have: u64,
    pub need: u64,
    pub xp: u64,
    pub done: bool,
    /// Distinct things counted so far, for quests that count *different* ones.
    pub seen: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Quests {
    /// The local day the dailies are for.
    pub day: u64,
    pub daily: Vec<Quest>,
    /// The day a daily was last swapped; one swap a day.
    pub swapped_day: u64,
    /// The week the weekly challenge is for (Monday-based).
    pub week: u64,
    /// The three offered, until the player picks one.
    pub weekly_offer: Vec<Quest>,
    pub weekly: Option<Quest>,
    /// How many quests were ever drawn -- varies a swap's draw.
    pub draws: u64,
}

/// The season: a calendar month of stars.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Season {
    /// `year * 12 + month0`, local.
    pub month: u64,
    pub stars: u32,
    /// The most stars held this season; the floors are read off it.
    pub best: u32,
    /// Top-half finishes in a row, for the third one's extra star.
    pub top_run: u32,
    /// The rank the last season ended at, shown on the season's card.
    pub last_rank: Option<u32>,
    /// The highest rank ever reached.
    pub best_rank_ever: u32,
}

/// A game under way: what it has earned so far, held until it ends.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pending {
    /// A keyed hash of the table's session and this seat; never the session.
    pub id: String,
    pub table: String,
    /// How many players the first hand dealt in.
    pub players: u32,
    pub hands: u64,
    /// Experience for the hands played, granted when the game ends.
    pub xp_hands: u64,
    pub began_unix_ms: u64,
    pub last_unix_ms: u64,
    /// The stack the game began with, and the lowest it fell to.
    pub start_stack: u64,
    pub low_stack: u64,
    /// This seat's clock ran out in this game.
    pub timeouts: u32,
}

/// One line of the journal: every change of a number says why.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Line {
    pub when_unix_ms: u64,
    /// `xp`, `stars`, `manners`, `card`, `quest`, `level`.
    pub axis: String,
    pub delta: i64,
    pub why: String,
}

/// What the last game came to, for the lobby's card after it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameSummary {
    pub when_unix_ms: u64,
    pub table: String,
    /// `0` for a game that was left or lost.
    pub place: u32,
    pub players: u32,
    /// The experience earned, reason by reason.
    pub xp: Vec<(String, u64)>,
    pub cards: Vec<String>,
    pub quests: Vec<String>,
    pub level_before: u32,
    pub level_after: u32,
    pub manners: i64,
    pub stars: i64,
    /// The headline, already in words.
    pub headline: String,
    /// The player has looked at it.
    pub seen: bool,
}

/// The body of `progress.json`. Whole numbers, strings, lists and ordered maps
/// only, so the bytes written are the same for the same state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    /// Effort. Never falls.
    pub xp: u64,
    /// Conduct, `0..=100`.
    pub manners: u32,
    /// The latest moment this file has seen, by this machine's clock. A clock
    /// turned back earns nothing until it is past this again.
    pub clock_high_unix_ms: u64,
    /// The counters the cards and the quests are read off, by metric name.
    pub stats: BTreeMap<String, u64>,
    /// The album: card id to the moment it was earned.
    pub cards: BTreeMap<String, Earned>,
    pub quests: Quests,
    pub season: Season,
    /// Settled games, newest last, so none is counted twice.
    pub done: Vec<String>,
    pub pending: Vec<Pending>,
    pub journal: Vec<Line>,
    /// Opponents met, as keyed hashes.
    pub opponents: Vec<String>,
    /// The card shown on the lobby's card about the player; empty for none.
    pub showcase: String,
    /// `results.cbor` has been read into this file once.
    pub imported: bool,
    pub last_game: Option<GameSummary>,
}

impl Default for Progress {
    fn default() -> Self {
        Progress {
            xp: 0,
            manners: 100,
            clock_high_unix_ms: 0,
            stats: BTreeMap::new(),
            cards: BTreeMap::new(),
            quests: Quests::default(),
            season: Season::default(),
            done: Vec::new(),
            pending: Vec::new(),
            journal: Vec::new(),
            opponents: Vec::new(),
            showcase: String::new(),
            imported: false,
            last_game: None,
        }
    }
}

impl Progress {
    pub fn stat(&self, metric: &str) -> u64 {
        self.stats.get(metric).copied().unwrap_or(0)
    }

    /// Keep one more line of the journal, bounded.
    pub fn log(&mut self, when_unix_ms: u64, axis: &str, delta: i64, why: impl Into<String>) {
        self.journal.push(Line { when_unix_ms, axis: axis.to_string(), delta, why: why.into() });
        let over = self.journal.len().saturating_sub(JOURNAL_MAX);
        self.journal.drain(..over);
    }

    /// Force every list into its bound and every number into its range: a
    /// valid file of an older build may hold more.
    pub fn repair(&mut self) {
        self.manners = self.manners.min(100);
        let over = self.done.len().saturating_sub(DONE_MAX);
        self.done.drain(..over);
        let over = self.opponents.len().saturating_sub(OPPONENTS_MAX);
        self.opponents.drain(..over);
        let over = self.pending.len().saturating_sub(PENDING_MAX);
        self.pending.drain(..over);
        let over = self.journal.len().saturating_sub(JOURNAL_MAX);
        self.journal.drain(..over);
    }
}

/// The two keys this file needs, both derived from the profile's secret
/// application key and neither of any use anywhere else.
#[derive(Clone)]
pub struct Keys {
    seal: [u8; 32],
    tag: [u8; 32],
}

impl Keys {
    pub fn derive(app_key: &ed25519_dalek::SigningKey) -> Keys {
        let secret = zeroize::Zeroizing::new(app_key.to_bytes());
        Keys { seal: blake3::derive_key(SEAL_CONTEXT, &secret[..]), tag: blake3::derive_key(TAG_CONTEXT, &secret[..]) }
    }

    /// A short keyed hash of something this file must recognise again and must
    /// not name: a game's session, an opponent's key.
    pub fn tag(&self, what: &[u8]) -> String {
        hex(&blake3::keyed_hash(&self.tag, what).as_bytes()[..10])
    }

    fn seal(&self, format: u32, generation: u64, body: &[u8]) -> blake3::Hash {
        let mut h = blake3::Hasher::new_keyed(&self.seal);
        h.update(&format.to_le_bytes());
        h.update(&generation.to_le_bytes());
        h.update(body);
        h.finalize()
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 || !text.is_ascii() {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

pub fn progress_path(dir: &Path) -> PathBuf {
    dir.join("progress.json")
}

fn tmp_path(dir: &Path) -> PathBuf {
    dir.join("progress.json.tmp")
}

fn bak_path(dir: &Path) -> PathBuf {
    dir.join("progress.json.bak")
}

const REJECTED_PREFIX: &str = "progress.rejected-";

/// What a load has to tell the player, once, in neutral words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// A copy did not pass its check and was set aside; an earlier one is used.
    SetAside { recovered: bool },
    /// The file was written by a newer version and is left alone.
    NewerFormat,
}

impl Notice {
    /// The one sentence the lobby says. No accusation in it: a disk does this
    /// too.
    pub fn words(self) -> &'static str {
        match self {
            Notice::SetAside { recovered: true } => {
                "Your rewards file could not be verified, so the last good copy is in use. The other file was kept beside it."
            }
            Notice::SetAside { recovered: false } => {
                "Your rewards file could not be verified and no earlier copy was found, so rewards start afresh. The file was kept beside it."
            }
            Notice::NewerFormat => {
                "Your rewards file was written by a newer version of p2p-poker. It is left untouched, and rewards are not saved by this version."
            }
        }
    }
}

enum Read {
    Absent,
    Invalid,
    Newer,
    Valid { generation: u64, progress: Box<Progress> },
}

#[derive(Deserialize)]
struct Head {
    format: u32,
}

#[derive(Deserialize)]
struct Envelope<'a> {
    generation: u64,
    seal: String,
    #[serde(borrow)]
    body: &'a serde_json::value::RawValue,
}

fn read_one(path: &Path, keys: &Keys) -> Read {
    let Ok(bytes) = std::fs::read(path) else {
        return Read::Absent;
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return Read::Invalid;
    };
    let Ok(head) = serde_json::from_str::<Head>(text) else {
        return Read::Invalid;
    };
    if head.format > FORMAT {
        return Read::Newer;
    }
    let Ok(env) = serde_json::from_str::<Envelope>(text) else {
        return Read::Invalid;
    };
    let body = env.body.get();
    // `blake3::Hash` compares in constant time.
    if unhex(&env.seal).map(blake3::Hash::from_bytes) != Some(keys.seal(head.format, env.generation, body.as_bytes())) {
        return Read::Invalid;
    }
    match serde_json::from_str::<Progress>(body) {
        Ok(mut progress) => {
            progress.repair();
            Read::Valid { generation: env.generation, progress: Box::new(progress) }
        }
        Err(_) => Read::Invalid,
    }
}

/// Where a save may be made to stop, for the tests that crash it there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Half of the temporary file is on the disk.
    TornTmp,
    /// The temporary file is whole and flushed.
    TmpWritten,
    /// `progress.json` has been renamed to `.bak`; there is no `progress.json`.
    MainMovedAside,
}

/// The file, open: where it lives, its keys and the generation last written.
pub struct Store {
    dir: PathBuf,
    keys: Keys,
    generation: u64,
    /// A newer format is on the disk: nothing is written.
    read_only: bool,
    /// The last save failed; the next one is owed.
    pub dirty: bool,
}

pub struct Loaded {
    pub progress: Progress,
    pub store: Store,
    pub notice: Option<Notice>,
}

impl Store {
    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// Open the profile's file. Never fails: whatever is on the disk, a poker
    /// client starts.
    pub fn open(dir: &Path, app_key: &ed25519_dalek::SigningKey, now_unix_ms: u64) -> Loaded {
        let keys = Keys::derive(app_key);
        let (main, tmp, bak) = (progress_path(dir), tmp_path(dir), bak_path(dir));
        let reads = [read_one(&main, &keys), read_one(&tmp, &keys), read_one(&bak, &keys)];
        let newer = reads.iter().any(|r| matches!(r, Read::Newer));

        // The valid copy with the highest generation; `progress.json` on a tie.
        let mut best: Option<(usize, u64)> = None;
        for (i, r) in reads.iter().enumerate() {
            if let Read::Valid { generation, .. } = r {
                if best.is_none_or(|(_, g)| *generation > g) {
                    best = Some((i, *generation));
                }
            }
        }

        let mut notice = None;
        if newer {
            notice = Some(Notice::NewerFormat);
        } else {
            // A file that does not pass is set aside, never deleted. A torn
            // temporary file is what a crash leaves, and only that goes.
            if matches!(reads[0], Read::Invalid) {
                set_aside(dir, &main, now_unix_ms);
                notice = Some(Notice::SetAside { recovered: best.is_some() });
            }
            if matches!(reads[2], Read::Invalid) {
                set_aside(dir, &bak, now_unix_ms.saturating_add(1));
                notice.get_or_insert(Notice::SetAside { recovered: best.is_some() });
            }
            if matches!(reads[1], Read::Invalid) {
                let _ = std::fs::remove_file(&tmp);
            }
            // A save interrupted after its temporary file was whole is finished
            // here, before anything else is written over that file.
            match best {
                Some((1, _)) => {
                    if main.exists() {
                        let _ = std::fs::rename(&main, &bak);
                    }
                    let _ = std::fs::rename(&tmp, &main);
                }
                // The crash fell between the two renames and the temporary
                // file is gone too: the backup is the file.
                Some((2, _)) if !main.exists() => {
                    let _ = std::fs::copy(&bak, &main);
                }
                _ => {}
            }
        }

        let generation = best.map_or(0, |(_, g)| g);
        let progress = match best {
            Some((i, _)) => match &reads[i] {
                Read::Valid { progress, .. } => (**progress).clone(),
                _ => Progress::default(),
            },
            None => Progress::default(),
        };
        Loaded { progress, store: Store { dir: dir.to_path_buf(), keys, generation, read_only: newer, dirty: false }, notice }
    }

    /// Write the state. A failure leaves `dirty` set and the state in memory
    /// as it was; the caller saves again at the next change and at exit.
    pub fn save(&mut self, progress: &Progress) -> io::Result<()> {
        let done = self.save_until(progress, None);
        self.dirty = done.is_err();
        done
    }

    fn save_until(&mut self, progress: &Progress, crash_at: Option<Step>) -> io::Result<()> {
        if self.read_only {
            return Ok(());
        }
        let generation = self.generation + 1;
        let body = serde_json::to_string_pretty(progress)
            .map_err(|e| io::Error::other(format!("the progress does not encode: {e}")))?;
        let seal = self.keys.seal(FORMAT, generation, body.as_bytes());
        let text = format!(
            "{{\n\"format\": {FORMAT},\n\"generation\": {generation},\n\"note\": \"Checked when the game starts. An edited copy is set aside and the last good copy is used.\",\n\"seal\": \"{}\",\n\"body\": {body}\n}}\n",
            seal.to_hex()
        );
        std::fs::create_dir_all(&self.dir)?;
        let (main, tmp, bak) = (progress_path(&self.dir), tmp_path(&self.dir), bak_path(&self.dir));
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&tmp)?;
            if crash_at == Some(Step::TornTmp) {
                f.write_all(&text.as_bytes()[..text.len() / 2])?;
                return Ok(());
            }
            f.write_all(text.as_bytes())?;
            f.sync_all()?;
        }
        if crash_at == Some(Step::TmpWritten) {
            return Ok(());
        }
        match std::fs::rename(&main, &bak) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        if crash_at == Some(Step::MainMovedAside) {
            return Ok(());
        }
        std::fs::rename(&tmp, &main)?;
        self.generation = generation;
        Ok(())
    }
}

/// Move a file that did not pass its check out of the way, and keep the
/// newest `REJECTED_MAX` of those.
fn set_aside(dir: &Path, path: &Path, now_unix_ms: u64) {
    let target = dir.join(format!("{REJECTED_PREFIX}{now_unix_ms:013}.json"));
    if std::fs::rename(path, &target).is_err() {
        return;
    }
    let mut kept: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(REJECTED_PREFIX) && n.ends_with(".json"))
                })
                .collect()
        })
        .unwrap_or_default();
    kept.sort();
    let over = kept.len().saturating_sub(REJECTED_MAX);
    for old in kept.into_iter().take(over) {
        let _ = std::fs::remove_file(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("p2p-poker-progress-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn key(n: u8) -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[n; 32])
    }

    fn with_xp(xp: u64) -> Progress {
        Progress { xp, ..Progress::default() }
    }

    fn rejected(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.starts_with(REJECTED_PREFIX))
            .collect();
        v.sort();
        v
    }

    /// What is saved is what is loaded; no file is a clean start and no notice.
    #[test]
    fn the_file_round_trips_and_an_absent_file_is_a_clean_start() {
        let dir = scratch("round-trip");
        let l = Store::open(&dir, &key(1), 1_000);
        assert_eq!(l.progress, Progress::default());
        assert_eq!(l.progress.manners, 100, "table manners begin full");
        assert_eq!(l.notice, None);
        let mut store = l.store;
        let mut p = with_xp(140);
        p.stats.insert("games".into(), 3);
        p.log(5, "xp", 140, "Game finished");
        store.save(&p).unwrap();
        store.save(&p).unwrap();
        let again = Store::open(&dir, &key(1), 2_000);
        assert_eq!(again.progress, p);
        assert_eq!(again.store.generation(), 2);
        assert_eq!(again.notice, None);
        assert!(bak_path(&dir).exists(), "the second save kept the first as the backup");
    }

    /// One byte changed in an editor: the file is set aside, not deleted, the
    /// backup is used, and the player is told once in neutral words.
    #[test]
    fn a_changed_byte_is_noticed_and_the_last_good_copy_is_used() {
        let dir = scratch("changed-byte");
        let mut store = Store::open(&dir, &key(1), 0).store;
        store.save(&with_xp(100)).unwrap();
        store.save(&with_xp(200)).unwrap();
        let text = std::fs::read_to_string(progress_path(&dir)).unwrap();
        assert!(text.contains("\"xp\": 200"));
        std::fs::write(progress_path(&dir), text.replace("\"xp\": 200", "\"xp\": 999999")).unwrap();

        let l = Store::open(&dir, &key(1), 7_000);
        assert_eq!(l.progress.xp, 100, "the last valid copy, not the edited one and not a clean start");
        assert_eq!(l.notice, Some(Notice::SetAside { recovered: true }));
        assert!(!l.notice.unwrap().words().to_lowercase().contains("cheat"));
        let aside = rejected(&dir);
        assert_eq!(aside.len(), 1, "the edited file was kept: {aside:?}");
        assert!(std::fs::read_to_string(dir.join(&aside[0])).unwrap().contains("999999"));
        // And the file is whole again for the next start.
        let mut store = l.store;
        store.save(&l.progress).unwrap();
        assert_eq!(Store::open(&dir, &key(1), 8_000).notice, None);
    }

    /// The generation is under the hash too: an old body under a new number
    /// does not pass.
    #[test]
    fn the_generation_is_part_of_what_is_checked() {
        let dir = scratch("generation");
        let mut store = Store::open(&dir, &key(1), 0).store;
        store.save(&with_xp(100)).unwrap();
        let text = std::fs::read_to_string(progress_path(&dir)).unwrap();
        std::fs::write(progress_path(&dir), text.replace("\"generation\": 1", "\"generation\": 50")).unwrap();
        let l = Store::open(&dir, &key(1), 1);
        assert_eq!(l.notice, Some(Notice::SetAside { recovered: false }));
        assert_eq!(l.progress, Progress::default());
    }

    /// A file from another profile is another key's: it does not pass here.
    #[test]
    fn a_file_from_another_profile_does_not_pass() {
        let (theirs, mine) = (scratch("foreign-theirs"), scratch("foreign-mine"));
        let mut store = Store::open(&theirs, &key(2), 0).store;
        store.save(&with_xp(50_000)).unwrap();
        std::fs::copy(progress_path(&theirs), progress_path(&mine)).unwrap();
        let l = Store::open(&mine, &key(1), 3_000);
        assert_eq!(l.progress, Progress::default(), "a clean start: there is no earlier copy here");
        assert_eq!(l.notice, Some(Notice::SetAside { recovered: false }));
        assert_eq!(rejected(&mine).len(), 1);
    }

    /// A file cut short, an empty file and a file that is not JSON all read as
    /// not valid, and none stops the client.
    #[test]
    fn a_cut_file_and_an_empty_file_are_survived() {
        for (name, make) in [
            ("cut", Box::new(|t: &str| t[..t.len() / 3].to_string()) as Box<dyn Fn(&str) -> String>),
            ("empty", Box::new(|_: &str| String::new())),
            ("noise", Box::new(|_: &str| "\u{0}\u{1}not json".to_string())),
        ] {
            let dir = scratch(&format!("broken-{name}"));
            let mut store = Store::open(&dir, &key(1), 0).store;
            store.save(&with_xp(10)).unwrap();
            store.save(&with_xp(20)).unwrap();
            let text = std::fs::read_to_string(progress_path(&dir)).unwrap();
            std::fs::write(progress_path(&dir), make(&text)).unwrap();
            let l = Store::open(&dir, &key(1), 9);
            assert_eq!(l.progress.xp, 10, "{name}: the backup is used");
            assert_eq!(l.notice, Some(Notice::SetAside { recovered: true }), "{name}");
        }
    }

    /// A crash at every step of a save: the newest complete copy is what the
    /// next start reads, no notice is shown for it, and the save after it works.
    #[test]
    fn a_crash_at_any_step_of_a_save_loses_the_last_change_at_most() {
        for (step, expect) in [(Step::TornTmp, 200), (Step::TmpWritten, 300), (Step::MainMovedAside, 300)] {
            let dir = scratch(&format!("crash-{step:?}"));
            let mut store = Store::open(&dir, &key(1), 0).store;
            store.save(&with_xp(100)).unwrap();
            store.save(&with_xp(200)).unwrap();
            store.save_until(&with_xp(300), Some(step)).unwrap();

            let l = Store::open(&dir, &key(1), 10);
            assert_eq!(l.progress.xp, expect, "{step:?}");
            assert_eq!(l.notice, None, "{step:?}: a crash is not a finding");
            assert!(progress_path(&dir).exists(), "{step:?}: the interrupted rename was finished");
            assert!(!tmp_path(&dir).exists(), "{step:?}: no temporary file is left to be written over");
            assert!(rejected(&dir).is_empty(), "{step:?}");
            let mut store = l.store;
            store.save(&with_xp(400)).unwrap();
            assert_eq!(Store::open(&dir, &key(1), 11).progress.xp, 400, "{step:?}");
        }
    }

    /// The very first save, crashed: there is nothing older, and the start is
    /// clean or the whole first copy.
    #[test]
    fn a_crash_in_the_first_save_is_a_clean_start_or_the_first_copy() {
        let dir = scratch("crash-first");
        let mut store = Store::open(&dir, &key(1), 0).store;
        store.save_until(&with_xp(100), Some(Step::TornTmp)).unwrap();
        let l = Store::open(&dir, &key(1), 1);
        assert_eq!((l.progress.xp, l.notice), (0, None));
        let mut store = l.store;
        store.save_until(&with_xp(100), Some(Step::TmpWritten)).unwrap();
        assert_eq!(Store::open(&dir, &key(1), 2).progress.xp, 100);
    }

    /// A newer format is never overwritten, whatever this build earns meanwhile.
    #[test]
    fn a_newer_format_is_left_alone() {
        let dir = scratch("newer");
        let newer = "{\n\"format\": 2,\n\"generation\": 9,\n\"seal\": \"00\",\n\"body\": {\"levels\": [1, 2]}\n}\n";
        std::fs::write(progress_path(&dir), newer).unwrap();
        let l = Store::open(&dir, &key(1), 5);
        assert_eq!(l.notice, Some(Notice::NewerFormat));
        assert_eq!(l.progress, Progress::default());
        let mut store = l.store;
        assert!(store.read_only());
        store.save(&with_xp(777)).unwrap();
        assert_eq!(std::fs::read_to_string(progress_path(&dir)).unwrap(), newer, "byte for byte");
        assert!(rejected(&dir).is_empty() && !tmp_path(&dir).exists() && !bak_path(&dir).exists());
    }

    /// Three set-aside files at most; the oldest go.
    #[test]
    fn the_set_aside_files_are_bounded() {
        let dir = scratch("bounded");
        for round in 0..6u64 {
            let mut store = Store::open(&dir, &key(1), round * 10).store;
            store.save(&with_xp(round)).unwrap();
            std::fs::write(progress_path(&dir), format!("edited {round}")).unwrap();
            let _ = Store::open(&dir, &key(1), 1_000 + round * 10);
        }
        let aside = rejected(&dir);
        assert_eq!(aside.len(), REJECTED_MAX, "{aside:?}");
        assert!(std::fs::read_to_string(dir.join(aside.last().unwrap())).unwrap().contains("edited 5"), "the newest are kept");
    }

    /// The bytes are a function of the state: the same state, the same body.
    #[test]
    fn the_same_state_writes_the_same_body() {
        let mut a = with_xp(5);
        for k in ["wins", "games", "hands"] {
            a.stats.insert(k.into(), 1);
        }
        let mut b = with_xp(5);
        for k in ["hands", "wins", "games"] {
            b.stats.insert(k.into(), 1);
        }
        assert_eq!(serde_json::to_string_pretty(&a).unwrap(), serde_json::to_string_pretty(&b).unwrap());
        assert!(!serde_json::to_string(&a).unwrap().contains('.'), "whole numbers only");
    }

    /// A failed write says so and is owed again; a list over its bound is cut.
    #[test]
    fn a_failed_save_is_owed_and_lists_are_bounded() {
        let dir = scratch("owed");
        let mut store = Store::open(&dir, &key(1), 0).store;
        // A directory where the temporary file should go: the write fails.
        std::fs::create_dir_all(tmp_path(&dir)).unwrap();
        assert!(store.save(&with_xp(1)).is_err());
        assert!(store.dirty);
        std::fs::remove_dir_all(tmp_path(&dir)).unwrap();
        store.save(&with_xp(1)).unwrap();
        assert!(!store.dirty);

        let mut p = Progress::default();
        for i in 0..(JOURNAL_MAX as u64 + 20) {
            p.log(i, "xp", 1, "x");
        }
        assert_eq!(p.journal.len(), JOURNAL_MAX);
        assert_eq!(p.journal[0].when_unix_ms, 20, "the oldest lines went");
        p.manners = 400;
        p.done = (0..DONE_MAX + 5).map(|i| i.to_string()).collect();
        p.repair();
        assert_eq!((p.manners, p.done.len()), (100, DONE_MAX));
    }

    /// The tags kept inside the file name nothing: they are keyed, short, and
    /// another profile's differ.
    #[test]
    fn a_tag_is_keyed_and_names_nothing() {
        let (a, b) = (Keys::derive(&key(1)), Keys::derive(&key(2)));
        let session = [9u8; 32];
        assert_eq!(a.tag(&session), a.tag(&session));
        assert_ne!(a.tag(&session), b.tag(&session));
        assert_eq!(a.tag(&session).len(), 20);
        assert!(!a.tag(&session).contains(&hex(&session[..4])));
    }
}
