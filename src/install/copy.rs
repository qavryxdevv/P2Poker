//! `D-073`: the two things the installer does to files -- a program copied and
//! **verified before it takes its name**, and a profile moved without ever
//! being in two runnable places or in none.
//!
//! Plain files and nothing of Windows, so every line of it runs in a test on a
//! scratch folder.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

/// A partial copy younger than this belongs to an installer that is still
/// working -- a double-click that was really two. Older, and it is what a
/// crash or a pulled cable left behind.
const PARTIAL_IS_LIVE_FOR: Duration = Duration::from_secs(120);

/// `ERROR_ACCESS_DENIED`, `ERROR_SHARING_VIOLATION`, `ERROR_LOCK_VIOLATION`,
/// `ERROR_USER_MAPPED_FILE`: what Windows answers when the file to be replaced
/// is a program that is running.
const HELD: [i32; 4] = [5, 32, 33, 1224];

pub fn sha256_file(path: &Path) -> io::Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().into())
}

pub fn hex(hash: &[u8; 32]) -> String {
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug)]
pub enum CopyError {
    /// Another installer is writing this very file right now.
    Busy,
    /// The program that is there could not be replaced: it is running, or
    /// something holds it.
    InUse(io::Error),
    /// What was read back from the disk is not what was written. Nothing was
    /// installed.
    Mismatch,
    Io(io::Error),
}

impl std::fmt::Display for CopyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CopyError::Busy => write!(f, "Another installation is in progress. Give it a moment and try again."),
            CopyError::InUse(e) => write!(
                f,
                "The installed P2Poker could not be replaced. It is probably running: close it and try again. \
                 Nothing was changed. (Windows said: {e})"
            ),
            CopyError::Mismatch => write!(
                f,
                "The copy on the disk does not match this file, so nothing was installed. A failing disk, or a \
                 security tool changing the file, would do that."
            ),
            CopyError::Io(e) => write!(f, "{e}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    pub sha256: [u8; 32],
    pub bytes: u64,
    /// A program was there before and this took its place.
    pub replaced: bool,
}

fn partial_of(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    target.with_file_name(name)
}

/// Copy `source` to `target`, and let it have the name only once the bytes on
/// the disk are proven to be the bytes that were read.
///
/// 1. Written beside the target under another name, hashed as it goes, and
///    synced -- so a crash leaves a `.partial`, never half a program under the
///    program's name.
/// 2. **Read back from the disk and hashed again.** The first hash says what
///    was handed to the system; only the second says what it kept.
/// 3. Renamed over the target, which the file system does in one step. A
///    target that is a running program refuses, and that is [`CopyError::InUse`]
///    rather than a game cut off in the middle of a hand.
///
/// A **new file**, not a system copy: it carries this file's bytes and none of
/// its alternate streams, so the mark a browser put on the download stays on
/// the download. That is what every installer's output is -- the question the
/// mark asks was answered when the player started this file.
pub fn install_file(source: &Path, target: &Path) -> Result<Copied, CopyError> {
    let dir = target.parent().ok_or_else(|| CopyError::Io(io::Error::other("the target has no folder")))?;
    fs::create_dir_all(dir).map_err(CopyError::Io)?;
    let partial = partial_of(target);

    if let Ok(meta) = fs::metadata(&partial) {
        let age = meta.modified().ok().and_then(|m| m.elapsed().ok());
        if age.is_some_and(|a| a < PARTIAL_IS_LIVE_FOR) {
            return Err(CopyError::Busy);
        }
        fs::remove_file(&partial).map_err(CopyError::Io)?;
    }

    let written = write_hashed(source, &partial);
    let (sha256, bytes) = match written {
        Ok(done) => done,
        Err(e) => {
            let _ = fs::remove_file(&partial);
            return Err(CopyError::Io(e));
        }
    };
    match sha256_file(&partial) {
        Ok(kept) if kept == sha256 => {}
        Ok(_) => {
            let _ = fs::remove_file(&partial);
            return Err(CopyError::Mismatch);
        }
        Err(e) => {
            let _ = fs::remove_file(&partial);
            return Err(CopyError::Io(e));
        }
    }

    let replaced = target.exists();
    if let Err(e) = fs::rename(&partial, target) {
        let _ = fs::remove_file(&partial);
        return Err(if e.raw_os_error().is_some_and(|code| HELD.contains(&code)) {
            CopyError::InUse(e)
        } else {
            CopyError::Io(e)
        });
    }
    Ok(Copied { sha256, bytes, replaced })
}

fn write_hashed(source: &Path, partial: &Path) -> io::Result<([u8; 32], u64)> {
    let mut input = fs::File::open(source)?;
    // `create_new`: two installers started together cannot both hold it.
    let mut output = fs::OpenOptions::new().write(true).create_new(true).open(partial)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    let mut bytes = 0u64;
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        output.write_all(&buffer[..n])?;
        bytes += n as u64;
    }
    output.flush()?;
    output.sync_all()?;
    Ok((hasher.finalize().into(), bytes))
}

#[derive(Debug)]
pub enum MoveError {
    /// A client is running on that profile.
    InUse,
    /// A profile stands at the destination. Two profiles are two players, and
    /// one is never put over the other.
    Occupied,
    Io(io::Error),
    /// A file of the copy is not the file it was copied from.
    Mismatch(PathBuf),
}

impl std::fmt::Display for MoveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MoveError::InUse => write!(f, "a P2Poker client is running with that profile; close it first"),
            MoveError::Occupied => write!(f, "the installed copy already has a profile of its own"),
            MoveError::Io(e) => write!(f, "{e}"),
            MoveError::Mismatch(p) => write!(f, "{} did not copy faithfully", p.display()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Moved {
    /// One rename: the same folder under its new name, and nothing left behind.
    Renamed,
    /// Another disk, so it was copied and proven. The original is where
    /// `kept_as` says -- out of the portable copy's way, **and not deleted**.
    Copied { kept_as: PathBuf },
}

/// Move a player's profile from `from` to `to`.
///
/// **The player's identity is in there, so the rule is that it is never in no
/// place and never deleted by this code.** On one disk that is a single rename.
/// Across two it is copied under a name nothing reads (`profile.incoming`),
/// every file is hashed against its original, the copy takes the name
/// `profile`, and only then is the original renamed out of the way -- to
/// `profile.moved-<time>`, which the portable copy does not open and the
/// player may delete or keep. If that last rename fails the original simply
/// stays, and the caller says so.
pub fn move_profile(from: &Path, to: &Path, now_unix_ms: u64) -> Result<Moved, MoveError> {
    if to.exists() {
        return Err(MoveError::Occupied);
    }
    // Asked by taking the lock a client takes, and let go at once: a held file
    // inside the folder would stop the rename below.
    match crate::storage::profile::lock_profile(from) {
        Ok(lock) => drop(lock),
        Err(crate::storage::profile::LockError::InUse) => return Err(MoveError::InUse),
        Err(crate::storage::profile::LockError::Unavailable(e)) => return Err(MoveError::Io(e)),
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(MoveError::Io)?;
    }
    if fs::rename(from, to).is_ok() {
        return Ok(Moved::Renamed);
    }

    let incoming = to.with_file_name("profile.incoming");
    let _ = fs::remove_dir_all(&incoming);
    let copied = copy_tree(from, &incoming).and_then(|()| prove_tree(from, &incoming));
    if let Err(e) = copied {
        let _ = fs::remove_dir_all(&incoming);
        return Err(e);
    }
    if let Err(e) = fs::rename(&incoming, to) {
        let _ = fs::remove_dir_all(&incoming);
        return Err(MoveError::Io(e));
    }
    let aside = from.with_file_name(format!("profile.moved-{now_unix_ms}"));
    let kept_as = if fs::rename(from, &aside).is_ok() { aside } else { from.to_path_buf() };
    Ok(Moved::Copied { kept_as })
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), MoveError> {
    fs::create_dir_all(to).map_err(MoveError::Io)?;
    for entry in fs::read_dir(from).map_err(MoveError::Io)? {
        let entry = entry.map_err(MoveError::Io)?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if entry.file_type().map_err(MoveError::Io)?.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            fs::copy(&src, &dst).map_err(MoveError::Io)?;
        }
    }
    Ok(())
}

/// Every file of `original` is in `copy`, byte for byte.
fn prove_tree(original: &Path, copy: &Path) -> Result<(), MoveError> {
    for entry in fs::read_dir(original).map_err(MoveError::Io)? {
        let entry = entry.map_err(MoveError::Io)?;
        let (src, dst) = (entry.path(), copy.join(entry.file_name()));
        if entry.file_type().map_err(MoveError::Io)?.is_dir() {
            prove_tree(&src, &dst)?;
        } else {
            let same = matches!((sha256_file(&src), sha256_file(&dst)), (Ok(a), Ok(b)) if a == b);
            if !same {
                return Err(MoveError::Mismatch(dst));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("p2p-poker-test-install-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn program(dir: &Path, name: &str, fill: u8) -> PathBuf {
        let path = dir.join(name);
        // Bigger than one buffer, and not a multiple of it.
        let bytes: Vec<u8> = (0..(1 << 20) + 12_345).map(|i| (i as u8).wrapping_mul(31).wrapping_add(fill)).collect();
        fs::write(&path, bytes).unwrap();
        path
    }

    /// **`D-073`: the program gets its name only when the disk holds the bytes
    /// that were read**, in a folder with a name no English machine has.
    #[test]
    fn a_program_is_copied_proven_and_only_then_named() {
        let dir = scratch("copy");
        let source = program(&dir, "download.exe", 1);
        let target = dir.join("Příliš žluťoučký").join("P2Poker").join("p2p-poker.exe");

        let done = install_file(&source, &target).expect("a plain copy");
        assert!(!done.replaced);
        assert_eq!(done.bytes, fs::metadata(&source).unwrap().len());
        assert_eq!(done.sha256, sha256_file(&source).unwrap());
        assert_eq!(sha256_file(&target).unwrap(), done.sha256, "the disk holds what was read");
        assert!(!partial_of(&target).exists(), "and no partial is left");
        assert!(source.exists(), "the file the player started is never touched");

        // A newer build over it: replaced in one step, the old bytes gone.
        let newer = program(&dir, "newer.exe", 2);
        let again = install_file(&newer, &target).expect("an update");
        assert!(again.replaced);
        assert_eq!(sha256_file(&target).unwrap(), sha256_file(&newer).unwrap());
    }

    /// A partial an installer is still writing is left alone; one a crash left
    /// is cleared away. The break this catches: two double-clicks racing into
    /// one file, and an installer wedged for ever behind a dead one's leftovers.
    #[test]
    fn a_live_partial_is_respected_and_a_dead_one_is_cleared() {
        let dir = scratch("partial");
        let source = program(&dir, "download.exe", 3);
        let target = dir.join("home").join("p2p-poker.exe");
        fs::create_dir_all(target.parent().unwrap()).unwrap();

        fs::write(partial_of(&target), b"somebody is writing this").unwrap();
        assert!(matches!(install_file(&source, &target), Err(CopyError::Busy)));
        assert!(!target.exists(), "nothing took the program's name");

        // The same leftovers, but old: from a crash, not from a neighbour.
        let old = std::time::SystemTime::now() - Duration::from_secs(600);
        let file = fs::OpenOptions::new().write(true).open(partial_of(&target)).unwrap();
        file.set_modified(old).unwrap();
        drop(file);
        install_file(&source, &target).expect("a dead partial does not wedge the installer");
        assert_eq!(sha256_file(&target).unwrap(), sha256_file(&source).unwrap());
    }

    /// A program that is running is not replaced under its player. On Windows
    /// an open handle without delete-sharing is what a running image is to the
    /// file system.
    #[cfg(windows)]
    #[test]
    fn a_running_program_is_not_replaced() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = scratch("in-use");
        let source = program(&dir, "download.exe", 4);
        let target = program(&dir, "p2p-poker.exe", 5);
        let before = sha256_file(&target).unwrap();

        let held = fs::OpenOptions::new().read(true).share_mode(1).open(&target).unwrap();
        let refused = install_file(&source, &target);
        assert!(matches!(refused, Err(CopyError::InUse(_))), "{refused:?}");
        drop(held);
        assert_eq!(sha256_file(&target).unwrap(), before, "the running program is untouched");
        assert!(!partial_of(&target).exists(), "and the attempt cleaned up after itself");

        install_file(&source, &target).expect("free once it has been closed");
    }

    fn a_profile(dir: &Path) -> PathBuf {
        let profile = dir.join("profile");
        fs::create_dir_all(profile.join("backups")).unwrap();
        fs::write(profile.join("identity.key"), [7u8; 32]).unwrap();
        fs::write(profile.join("player.key"), [9u8; 32]).unwrap();
        fs::write(profile.join("progress.json"), b"{\"level\":5}").unwrap();
        fs::write(profile.join("backups").join("one.p2pbackup"), vec![3u8; 70_000]).unwrap();
        profile
    }

    /// **The player's identity moves whole, is never in two runnable places,
    /// and is never put over another player's.**
    #[test]
    fn a_profile_moves_whole_and_never_over_another() {
        let dir = scratch("move");
        let from = a_profile(&dir.join("portable"));
        let to = dir.join("installed").join("profile");

        assert_eq!(move_profile(&from, &to, 1).expect("one disk: one rename"), Moved::Renamed);
        assert!(!from.exists(), "it is not in two places");
        assert_eq!(fs::read(to.join("identity.key")).unwrap(), [7u8; 32]);
        assert_eq!(fs::read(to.join("backups").join("one.p2pbackup")).unwrap().len(), 70_000);

        // A second portable copy with a player of its own: refused, and both
        // players are exactly where they were.
        let other = a_profile(&dir.join("second"));
        fs::write(other.join("identity.key"), [1u8; 32]).unwrap();
        assert!(matches!(move_profile(&other, &to, 2), Err(MoveError::Occupied)));
        assert_eq!(fs::read(other.join("identity.key")).unwrap(), [1u8; 32]);
        assert_eq!(fs::read(to.join("identity.key")).unwrap(), [7u8; 32]);
    }

    /// A profile a client is running on stays where it is.
    #[cfg(windows)]
    #[test]
    fn a_profile_in_use_is_not_moved() {
        let dir = scratch("move-in-use");
        let from = a_profile(&dir.join("portable"));
        let to = dir.join("installed").join("profile");
        let client = crate::storage::profile::lock_profile(&from).expect("a client holds it");
        assert!(matches!(move_profile(&from, &to, 1), Err(MoveError::InUse)));
        assert!(from.join("identity.key").exists() && !to.exists());
        drop(client);
        assert_eq!(move_profile(&from, &to, 1).unwrap(), Moved::Renamed);
    }

    /// The other road, taken when the rename cannot be: copied, **proven**,
    /// named, and the original set aside rather than deleted.
    #[test]
    fn a_copied_profile_is_proven_file_by_file() {
        let dir = scratch("prove");
        let from = a_profile(&dir.join("portable"));
        let copy = dir.join("copy");
        copy_tree(&from, &copy).unwrap();
        prove_tree(&from, &copy).expect("a faithful copy proves");

        // One byte of one file in a sub-folder: caught, and named.
        let victim = copy.join("backups").join("one.p2pbackup");
        let mut bytes = fs::read(&victim).unwrap();
        bytes[40_000] ^= 1;
        fs::write(&victim, bytes).unwrap();
        match prove_tree(&from, &copy) {
            Err(MoveError::Mismatch(p)) => assert_eq!(p, victim),
            other => panic!("a changed byte went unnoticed: {other:?}"),
        }
        // And a file that did not arrive at all.
        fs::remove_file(copy.join("player.key")).unwrap();
        assert!(prove_tree(&from, &copy).is_err());
    }

    #[test]
    fn a_hash_reads_as_the_words_other_tools_print() {
        let dir = scratch("hex");
        let path = dir.join("empty");
        fs::write(&path, b"").unwrap();
        // SHA-256 of nothing, as `certutil -hashfile` and `Get-FileHash` say it.
        assert_eq!(hex(&sha256_file(&path).unwrap()), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }
}
