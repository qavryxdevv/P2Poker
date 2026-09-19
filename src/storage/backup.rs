//! `D-068`: a backup of the profile in one file, to carry the player -- the
//! key, the settings, the record, the notes and the rewards -- to another
//! computer, and one day to a phone.
//!
//! **Always under a password** (the owner, 2026-09-19): the file holds the
//! player's secret key, and whoever reads that key can sit down as this
//! player. The password is stretched with Argon2id and the contents are sealed
//! with ChaCha20-Poly1305, the header included as associated data; both crates
//! were in `Cargo.toml` from the beginning. There is no backup without a
//! password and no way round one: a forgotten password is a lost backup, and
//! the dialog says so before it is made.
//!
//! **The player travels; the machine does not.** The backup carries who is
//! playing -- `player.key` -- and what that player keeps. It leaves behind what
//! belongs to the machine: the network identity (a peer id says which socket,
//! §20), the Tox key, the record of an unfinished game, the lock. So nothing in
//! it names a computer, a network card, a path or a user, and the rewards'
//! seal, keyed from `player.key` alone, verifies wherever the profile is
//! restored: a phone that shows a new network address to every network it joins
//! reads the same file the same way.
//!
//! **A restore is staged and applied at the next start**, before anything is
//! loaded: the running client holds the old key in memory and would write its
//! own files over the restored ones as it closes. What a restore replaces is
//! kept beside the profile, never deleted.
//!
//! One profile is one player at one table at a time: a backup restored on a
//! second machine is for moving, not for playing on both (see the lobby's
//! notice, `NodeEvent::ProfileElsewhere`).

use std::io;
use std::path::{Path, PathBuf};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use minicbor::{Decode, Encode};

/// The files a backup carries, by their names in the profile directory. Names
/// only, never paths: a restore writes these names and no other.
pub const FILES: [&str; 5] = ["player.key", "settings.cbor", "results.cbor", "notes.cbor", "progress.json"];
/// The shortest password a backup takes.
pub const PASSWORD_MIN: usize = 8;
pub const EXTENSION: &str = "p2pbackup";

const MAGIC: &[u8; 16] = b"P2POKER-BACKUP\r\n";
const VERSION: u8 = 1;
/// Argon2id, 64 MiB and three passes: about a second on a desktop, and within
/// a phone's reach. Written into the header, so a later version may raise them.
const M_COST_KIB: u32 = 64 * 1024;
const T_COST: u32 = 3;
const HEADER_LEN: usize = 16 + 1 + 4 + 4 + 16 + 12;
/// What a restore will read at most: a header that asks for more is refused
/// before any memory is spent on it.
const M_COST_MAX_KIB: u32 = 1024 * 1024;
const T_COST_MAX: u32 = 16;
const FILE_MAX: usize = 8 * 1024 * 1024;
const BACKUP_MAX: u64 = 48 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
struct Packed {
    #[n(0)]
    format: u32,
    #[n(1)]
    created_unix_ms: u64,
    #[n(2)]
    files: Vec<PackedFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
struct PackedFile {
    #[n(0)]
    name: String,
    #[n(1)]
    #[cbor(with = "minicbor::bytes")]
    bytes: Vec<u8>,
}

/// What a backup held, once its password opened it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contents {
    pub created_unix_ms: u64,
    files: Vec<PackedFile>,
}

impl Contents {
    pub fn names(&self) -> Vec<&str> {
        self.files.iter().map(|f| f.name.as_str()).collect()
    }

    /// The first characters of the player key inside, as the lobby shows a
    /// player: so a restore can say *whose* profile this is before it is made.
    pub fn player(&self) -> Option<String> {
        let seed: [u8; 32] = self.files.iter().find(|f| f.name == "player.key")?.bytes.as_slice().try_into().ok()?;
        let public = ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key().to_bytes();
        Some(super::profile::short_name(&public))
    }
}

#[derive(Debug)]
pub enum BackupError {
    /// Shorter than `PASSWORD_MIN`.
    PasswordTooShort,
    /// Not one of this client's backups, or damaged.
    NotABackup,
    /// Made by a newer version.
    Newer,
    /// The password does not open it (or the file was altered: the two cannot
    /// be told apart, and must not be).
    WrongPassword,
    /// There is no player key to back up.
    NothingToBackUp,
    Io(io::Error),
}

impl std::fmt::Display for BackupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackupError::PasswordTooShort => write!(f, "The password needs at least {PASSWORD_MIN} characters."),
            BackupError::NotABackup => write!(f, "This file is not a p2p-poker backup, or it is damaged."),
            BackupError::Newer => write!(f, "This backup was made by a newer version of p2p-poker."),
            BackupError::WrongPassword => write!(f, "The password does not open this backup."),
            BackupError::NothingToBackUp => write!(f, "This profile has no player key yet."),
            BackupError::Io(e) => write!(f, "The file could not be read or written: {e}"),
        }
    }
}

impl From<io::Error> for BackupError {
    fn from(e: io::Error) -> Self {
        BackupError::Io(e)
    }
}

pub fn backups_dir(dir: &Path) -> PathBuf {
    dir.join("backups")
}

fn pending_dir(dir: &Path) -> PathBuf {
    dir.join("restore-pending")
}

const READY: &str = "READY";

fn stretch(password: &str, salt: &[u8; 16], m_cost: u32, t_cost: u32) -> Result<zeroize::Zeroizing<[u8; 32]>, BackupError> {
    let params = argon2::Params::new(m_cost, t_cost, 1, Some(32)).map_err(|_| BackupError::NotABackup)?;
    let mut key = zeroize::Zeroizing::new([0u8; 32]);
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut key[..])
        .map_err(|_| BackupError::NotABackup)?;
    Ok(key)
}

fn seal(password: &str, packed: &Packed, m_cost: u32, t_cost: u32) -> Result<Vec<u8>, BackupError> {
    if password.chars().count() < PASSWORD_MIN {
        return Err(BackupError::PasswordTooShort);
    }
    let (mut salt, mut nonce) = ([0u8; 16], [0u8; 12]);
    crate::security::rng::fill(&mut salt).map_err(|e| BackupError::Io(io::Error::other(e.to_string())))?;
    crate::security::rng::fill(&mut nonce).map_err(|e| BackupError::Io(io::Error::other(e.to_string())))?;
    let mut out = Vec::with_capacity(HEADER_LEN);
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&m_cost.to_le_bytes());
    out.extend_from_slice(&t_cost.to_le_bytes());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    let plain = zeroize::Zeroizing::new(
        minicbor::to_vec(packed).map_err(|e| BackupError::Io(io::Error::other(format!("the backup does not encode: {e}"))))?,
    );
    let key = stretch(password, &salt, m_cost, t_cost)?;
    let sealed = ChaCha20Poly1305::new(&Key::from(*key))
        .encrypt(&Nonce::from(nonce), Payload { msg: &plain, aad: &out })
        .map_err(|_| BackupError::NotABackup)?;
    out.extend_from_slice(&sealed);
    Ok(out)
}

fn open(bytes: &[u8], password: &str) -> Result<Contents, BackupError> {
    if bytes.len() < HEADER_LEN + 16 || &bytes[..16] != MAGIC {
        return Err(BackupError::NotABackup);
    }
    if bytes[16] > VERSION {
        return Err(BackupError::Newer);
    }
    let m_cost = u32::from_le_bytes(bytes[17..21].try_into().map_err(|_| BackupError::NotABackup)?);
    let t_cost = u32::from_le_bytes(bytes[21..25].try_into().map_err(|_| BackupError::NotABackup)?);
    if m_cost > M_COST_MAX_KIB || t_cost > T_COST_MAX {
        return Err(BackupError::NotABackup);
    }
    let salt: [u8; 16] = bytes[25..41].try_into().map_err(|_| BackupError::NotABackup)?;
    let nonce: [u8; 12] = bytes[41..53].try_into().map_err(|_| BackupError::NotABackup)?;
    let key = stretch(password, &salt, m_cost, t_cost)?;
    let plain = zeroize::Zeroizing::new(
        ChaCha20Poly1305::new(&Key::from(*key))
            .decrypt(&Nonce::from(nonce), Payload { msg: &bytes[HEADER_LEN..], aad: &bytes[..HEADER_LEN] })
            .map_err(|_| BackupError::WrongPassword)?,
    );
    let packed: Packed = minicbor::decode(&plain).map_err(|_| BackupError::NotABackup)?;
    if packed.format > 1 {
        return Err(BackupError::Newer);
    }
    // Names from the list and no other: nothing in a backup chooses where it
    // is written.
    let files: Vec<PackedFile> =
        packed.files.into_iter().filter(|f| FILES.contains(&f.name.as_str()) && f.bytes.len() <= FILE_MAX).collect();
    if !files.iter().any(|f| f.name == "player.key" && f.bytes.len() == 32) {
        return Err(BackupError::NotABackup);
    }
    Ok(Contents { created_unix_ms: packed.created_unix_ms, files })
}

/// `p2p-poker-backup-20260919-142501.p2pbackup`, by the local clock.
fn file_name(now_unix_ms: u64, offset_min: i32) -> String {
    let local_s = (now_unix_ms / 1_000) as i64 + i64::from(offset_min) * 60;
    let (y, m, d) = crate::app::rewards::civil(local_s.div_euclid(86_400).max(0) as u64);
    let s = local_s.rem_euclid(86_400);
    format!("p2p-poker-backup-{y:04}{m:02}{d:02}-{:02}{:02}{:02}.{EXTENSION}", s / 3_600, s / 60 % 60, s % 60)
}

/// Make a backup of the profile in `dir`, in its `backups` folder. Returns
/// where it is.
pub fn create(dir: &Path, password: &str, now_unix_ms: u64, offset_min: i32) -> Result<PathBuf, BackupError> {
    create_with(dir, password, now_unix_ms, offset_min, M_COST_KIB, T_COST)
}

fn create_with(dir: &Path, password: &str, now_unix_ms: u64, offset_min: i32, m_cost: u32, t_cost: u32) -> Result<PathBuf, BackupError> {
    if password.chars().count() < PASSWORD_MIN {
        return Err(BackupError::PasswordTooShort);
    }
    let mut files = Vec::new();
    for name in FILES {
        match std::fs::read(dir.join(name)) {
            Ok(bytes) if bytes.len() <= FILE_MAX => files.push(PackedFile { name: name.to_string(), bytes }),
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    if !files.iter().any(|f| f.name == "player.key") {
        return Err(BackupError::NothingToBackUp);
    }
    let sealed = seal(password, &Packed { format: 1, created_unix_ms: now_unix_ms, files }, m_cost, t_cost)?;
    let folder = backups_dir(dir);
    std::fs::create_dir_all(&folder)?;
    let path = folder.join(file_name(now_unix_ms, offset_min));
    let tmp = path.with_extension("tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&sealed)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Open a backup with its password.
pub fn read(path: &Path, password: &str) -> Result<Contents, BackupError> {
    if std::fs::metadata(path)?.len() > BACKUP_MAX {
        return Err(BackupError::NotABackup);
    }
    open(&std::fs::read(path)?, password)
}

/// The backups in the profile's folder, newest first.
pub fn list(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(backups_dir(dir))
        .map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == EXTENSION)).collect())
        .unwrap_or_default();
    found.sort();
    found.reverse();
    found
}

/// Stage a restore: the files are written beside the profile's and take their
/// places at the next start. Nothing of the running profile is touched.
pub fn stage_restore(dir: &Path, contents: &Contents) -> io::Result<()> {
    let staging = pending_dir(dir);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    for f in &contents.files {
        use std::io::Write;
        let mut out = std::fs::File::create(staging.join(&f.name))?;
        out.write_all(&f.bytes)?;
        out.sync_all()?;
    }
    // Last, so a staging cut short is never applied.
    std::fs::write(staging.join(READY), b"")
}

pub fn restore_is_pending(dir: &Path) -> bool {
    pending_dir(dir).join(READY).exists()
}

/// At the start, before anything of the profile is read: put a staged restore
/// in place. What it replaces is moved to `backups/replaced-<time>`, never
/// deleted. Returns whether a restore was applied.
pub fn apply_pending(dir: &Path, now_unix_ms: u64) -> io::Result<bool> {
    let staging = pending_dir(dir);
    if !staging.join(READY).exists() {
        // A staging cut short is only litter.
        let _ = std::fs::remove_dir_all(&staging);
        return Ok(false);
    }
    let kept = backups_dir(dir).join(format!("replaced-{now_unix_ms:013}"));
    std::fs::create_dir_all(&kept)?;
    // The old rewards file's copies go with it: they are the old key's.
    for name in FILES.iter().copied().chain(["progress.json.bak", "progress.json.tmp"]) {
        let from = dir.join(name);
        if from.exists() {
            std::fs::rename(&from, kept.join(name))?;
        }
    }
    for name in FILES {
        let from = staging.join(name);
        if from.exists() {
            std::fs::rename(&from, dir.join(name))?;
        }
    }
    std::fs::remove_dir_all(&staging)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("p2p-poker-backup-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A profile with a key, settings, a record and sealed rewards.
    fn profile(name: &str, seed: u8) -> (PathBuf, ed25519_dalek::SigningKey) {
        let dir = scratch(name);
        std::fs::write(dir.join("player.key"), [seed; 32]).unwrap();
        let key = super::super::profile::load_or_create_app_key(&dir).unwrap();
        std::fs::write(dir.join("settings.cbor"), b"settings").unwrap();
        std::fs::write(dir.join("identity.key"), b"this machine's and no other's").unwrap();
        let mut store = super::super::progress::Store::open(&dir, &key, 0).store;
        let mut p = super::super::progress::Progress::default();
        p.xp = 4_321;
        store.save(&p).unwrap();
        (dir, key)
    }

    fn quick(dir: &Path, password: &str) -> Result<PathBuf, BackupError> {
        create_with(dir, password, 1_789_820_701_000, 120, 8 * 1024, 1)
    }

    /// The whole road: a backup made here, restored into a fresh profile on
    /// "another machine", applied at its start -- and the rewards' seal
    /// verifies there, because its key travelled and nothing of the machine
    /// was ever in it.
    #[test]
    fn a_profile_moves_to_another_machine_with_its_rewards() {
        let (here, _) = profile("move-here", 7);
        let path = quick(&here, "correct horse").unwrap();
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "p2p-poker-backup-20260919-142501.p2pbackup");
        assert_eq!(list(&here), vec![path.clone()]);

        let there = scratch("move-there");
        std::fs::write(there.join("player.key"), [9u8; 32]).unwrap();
        std::fs::write(there.join("identity.key"), b"the other machine's").unwrap();
        let contents = read(&path, "correct horse").unwrap();
        assert_eq!(contents.created_unix_ms, 1_789_820_701_000);
        assert!(contents.names().contains(&"progress.json") && contents.names().contains(&"player.key"));
        assert!(!contents.names().contains(&"identity.key"), "the network identity stays with its machine");
        assert_eq!(contents.player().unwrap().len(), 8);

        stage_restore(&there, &contents).unwrap();
        assert!(restore_is_pending(&there));
        assert_eq!(std::fs::read(there.join("player.key")).unwrap(), vec![9u8; 32], "nothing is touched before the restart");
        assert!(apply_pending(&there, 5_000).unwrap());
        assert!(!restore_is_pending(&there) && !apply_pending(&there, 6_000).unwrap());

        assert_eq!(std::fs::read(there.join("player.key")).unwrap(), vec![7u8; 32]);
        assert_eq!(std::fs::read(there.join("identity.key")).unwrap(), b"the other machine's");
        let key = super::super::profile::load_or_create_app_key(&there).unwrap();
        let loaded = super::super::progress::Store::open(&there, &key, 7_000);
        assert_eq!((loaded.progress.xp, loaded.notice), (4_321, None), "the seal verifies on the other machine");
        let kept = backups_dir(&there).join("replaced-0000000005000");
        assert_eq!(std::fs::read(kept.join("player.key")).unwrap(), vec![9u8; 32], "what was replaced is kept");
    }

    /// No backup without a password, and none that a short one guards.
    #[test]
    fn a_backup_is_always_under_a_password() {
        let (dir, _) = profile("password", 3);
        for weak in ["", "short", "1234567"] {
            assert!(matches!(quick(&dir, weak), Err(BackupError::PasswordTooShort)), "{weak:?}");
        }
        assert!(list(&dir).is_empty(), "nothing was written");
        let path = quick(&dir, "p\u{159}\u{ed}li\u{161} \u{17e}lu\u{165}ou\u{10d}k\u{fd}").unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let key = [3u8; 32];
        assert!(!bytes.windows(32).any(|w| w == key), "the player key is nowhere in the file in the clear");
        assert!(!bytes.windows(8).any(|w| w == b"settings"));
    }

    /// The wrong password, a changed byte, a cut file, noise and a header that
    /// asks for a gigabyte are all refused, and none is told apart from a wrong
    /// password where telling would help a guesser.
    #[test]
    fn a_wrong_password_and_a_damaged_file_are_refused() {
        let (dir, _) = profile("refused", 4);
        let path = quick(&dir, "correct horse").unwrap();
        assert!(matches!(read(&path, "correct horsf"), Err(BackupError::WrongPassword)));
        let good = std::fs::read(&path).unwrap();

        let mut flipped = good.clone();
        *flipped.last_mut().unwrap() ^= 1;
        assert!(matches!(open(&flipped, "correct horse"), Err(BackupError::WrongPassword)));
        let mut header = good.clone();
        header[30] ^= 1;
        assert!(matches!(open(&header, "correct horse"), Err(BackupError::WrongPassword)), "the header is sealed too");
        assert!(matches!(open(&good[..HEADER_LEN + 4], "correct horse"), Err(BackupError::NotABackup)));
        assert!(matches!(open(b"not a backup at all, only some bytes that are long enough to look at", "correct horse"), Err(BackupError::NotABackup)));
        let mut greedy = good.clone();
        greedy[17..21].copy_from_slice(&(4u32 * 1024 * 1024).to_le_bytes());
        assert!(matches!(open(&greedy, "correct horse"), Err(BackupError::NotABackup)), "refused before the memory is spent");
        let mut newer = good;
        newer[16] = 2;
        assert!(matches!(open(&newer, "correct horse"), Err(BackupError::Newer)));
    }

    /// A backup names files from the list only: nothing in it chooses a path.
    #[test]
    fn a_backup_writes_only_the_names_it_may() {
        let packed = Packed {
            format: 1,
            created_unix_ms: 1,
            files: vec![
                PackedFile { name: "player.key".into(), bytes: vec![1; 32] },
                PackedFile { name: "..\\..\\evil.exe".into(), bytes: vec![2; 8] },
                PackedFile { name: "identity.key".into(), bytes: vec![3; 8] },
            ],
        };
        let sealed = seal("correct horse", &packed, 8 * 1024, 1).unwrap();
        let contents = open(&sealed, "correct horse").unwrap();
        assert_eq!(contents.names(), vec!["player.key"]);
    }

    /// A staging cut short is never applied, and an empty profile has nothing
    /// to back up.
    #[test]
    fn a_staging_cut_short_is_not_applied() {
        let dir = scratch("cut-short");
        assert!(matches!(quick(&dir, "correct horse"), Err(BackupError::NothingToBackUp)));
        std::fs::create_dir_all(pending_dir(&dir)).unwrap();
        std::fs::write(pending_dir(&dir).join("player.key"), [5u8; 32]).unwrap();
        assert!(!apply_pending(&dir, 1).unwrap());
        assert!(!dir.join("player.key").exists() && !pending_dir(&dir).exists());
    }
}
