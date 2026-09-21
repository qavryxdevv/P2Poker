//! The portable profile beside the executable.
//!
//! `SPEC_CS.md` §22: the whole directory can be copied to another machine and
//! the client is that client again. Nothing goes to the registry and nothing of
//! the profile goes outside this folder.
//!
//! §22 also says *no installer*, and `D-073` amends that on the owner's word: a
//! first run is offered a home (`crate::install`). What it installs is still
//! this -- the program with its profile beside it, one folder that can be
//! copied whole -- and the only things it writes outside are two shortcuts.
//!
//! # Why the identity is persisted at all
//!
//! A fresh key every run means a fresh `PeerId` every run, and a `PeerId` is what
//! every other client knows this one by. A client that changes identity on
//! restart cannot be reconnected to, cannot be recognised at a table it was
//! seated at, and appears to the network as an endless stream of strangers.
//!
//! # What is deliberately not here
//!
//! **No password on the key file.** The alternative is a passphrase prompt this
//! project has no way to make meaningful — the key sits next to the executable on
//! a machine whose owner is the player, and an attacker who can read the file can
//! read the process. Pretending otherwise would be the kind of security theatre
//! `SPEC_CS.md` §36 forbids: it would look like protection and stop nobody.
//!
//! The file's permissions are left to the operating system for the same reason,
//! and this is stated rather than quietly omitted so that nobody reads the
//! absence as an oversight.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The profile directory: beside the executable on Windows; on Linux, in the
/// user's data folder unless a profile already stands beside the program.
///
/// Falls back to the working directory if the executable's location cannot be
/// determined — which happens on some sandboxes — because a client that refuses
/// to start over a path lookup is worse than one that keeps its profile
/// somewhere slightly unexpected and says so.
pub fn profile_dir() -> PathBuf {
    let exe_dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
    let data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute());
    let home = std::env::var_os("HOME").map(PathBuf::from).filter(|p| p.is_absolute());
    profile_dir_for(cfg!(windows), exe_dir.as_deref(), data_home, home, |p| p.is_dir())
}

/// `D-078`: where the profile lives, from what the system says.
///
/// **Windows: beside the program, always** -- `SPEC_CS.md` §22's portable
/// folder, and `D-073`'s installed copy is that same folder in the user's
/// programs folder.
///
/// **Linux: in the user's data folder** (`$XDG_DATA_HOME/p2poker/profile`, or
/// `~/.local/share/p2poker/profile`). A package puts the program where a user
/// cannot write -- `/usr/lib` for the `.deb` and the `.rpm`, a read-only mount
/// for the AppImage -- so *beside the program* is nowhere there. **Unless a
/// profile already stands beside the program**: then that one, which is how a
/// Linux player keeps a portable copy, as on Windows -- make a `profile` folder
/// beside the program, and it is used. `exists` is the test for that folder,
/// so the rule is a table.
pub fn profile_dir_for(
    windows: bool,
    exe_dir: Option<&Path>,
    data_home: Option<PathBuf>,
    home: Option<PathBuf>,
    exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    let beside = exe_dir.unwrap_or_else(|| Path::new(".")).join("profile");
    if windows || exists(&beside) {
        return beside;
    }
    match data_home.or_else(|| home.map(|h| h.join(".local").join("share"))) {
        Some(data) => data.join("p2poker").join("profile"),
        None => beside,
    }
}

/// Where the libp2p identity lives.
pub fn identity_path(dir: &Path) -> PathBuf {
    dir.join("identity.key")
}

/// Load this client's identity, creating one on first run.
///
/// The file holds the 32-byte Ed25519 seed and nothing else: no length prefix,
/// no version tag, no format. A fixed-size file of a fixed-size secret has
/// nothing to parse, and therefore nothing to get wrong on a path that runs
/// before anything else.
pub fn load_or_create_identity(dir: &Path) -> io::Result<libp2p::identity::Keypair> {
    let path = identity_path(dir);

    if let Ok(bytes) = fs::read(&path) {
        if bytes.len() == 32 {
            let mut seed = [0u8; 32];
            seed.copy_from_slice(&bytes);
            if let Ok(key) = libp2p::identity::Keypair::ed25519_from_bytes(seed) {
                return Ok(key);
            }
        }
        // A file that is there and is not an identity is not overwritten. It
        // might be somebody's data in the wrong place, and losing it silently to
        // make a client start is not a trade this code gets to make.
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} exists and is not a 32-byte identity; move it aside rather than \
                 letting this overwrite it",
                path.display()
            ),
        ));
    }

    fs::create_dir_all(dir)?;
    let seed = crate::security::rng::secret_32()
        .map_err(|e| io::Error::other(format!("no operating system randomness: {e}")))?;
    let key = libp2p::identity::Keypair::ed25519_from_bytes(seed)
        .map_err(|e| io::Error::other(format!("the generated seed is not a key: {e}")))?;

    fs::write(&path, seed)?;
    Ok(key)
}

/// Eight characters of a key: what a person can compare at a glance, and what
/// this client calls a player who has not chosen a name.
///
/// Stable and theirs. An empty name or a "Player 1" would be neither, and two
/// of them would be indistinguishable.
pub fn short_name(key: &[u8; 32]) -> String {
    key[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// Where this client's **application** key lives.
///
/// A second file, deliberately. §20 keeps the two identities apart: a `PeerId`
/// says which socket you are talking to, an application key says who is playing.
/// One file holding both would make them one secret, and a client that had to
/// rotate its network identity would be a different player.
pub fn app_key_path(dir: &Path) -> PathBuf {
    dir.join("player.key")
}

/// Load the application key, creating one on first run.
///
/// The same shape as the identity for the same reason: 32 raw bytes, nothing to
/// parse, and an existing file that is not one is never overwritten.
///
/// This is the key that signs `JOIN_REQUEST` and `TABLE_READY`, that appears in
/// every roster, and that a table's `roster_hash(0)` is computed over. It must
/// survive a restart or a player rejoining their own table is a stranger.
pub fn load_or_create_app_key(dir: &Path) -> io::Result<ed25519_dalek::SigningKey> {
    let path = app_key_path(dir);

    if let Ok(bytes) = fs::read(&path) {
        if bytes.len() == 32 {
            let mut seed = [0u8; 32];
            seed.copy_from_slice(&bytes);
            return Ok(ed25519_dalek::SigningKey::from_bytes(&seed));
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} exists and is not a 32-byte key; move it aside rather than \
                 letting this overwrite it",
                path.display()
            ),
        ));
    }

    fs::create_dir_all(dir)?;
    let seed = crate::security::rng::secret_32()
        .map_err(|e| io::Error::other(format!("no operating system randomness: {e}")))?;
    fs::write(&path, seed)?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&seed))
}

/// The Tox secret key, in a **third** file (D-019).
///
/// Three identities, three files, for the reason §20 gives for the first two: a
/// `PeerId` says which socket you are talking to, an application key says who is
/// playing, and this says where a table's traffic reaches you. One file holding
/// two of them would make them one secret, and a client that had to rotate one
/// would be rotating the other.
pub fn tox_key_path(dir: &Path) -> PathBuf {
    dir.join("tox.key")
}

/// Load the Tox secret key, creating one on first run.
///
/// # Why it has to persist
///
/// A Tox identity that changed on every start would take the table with it.
/// The founder's Tox key is in the advertisement and the joiner's is in the
/// `JOIN_REQUEST`; both ends add each other from those, and a friendship is
/// two-sided. A founder that restarted would come back as a stranger to every
/// seated player — they would still hold the old key, add it, and wait for a
/// friend that no longer exists — and the table would sit there looking
/// reachable and be unreachable.
///
/// It is also what makes the advert's `founder_tox_key` and `tox_chat_id`
/// stable enough to be worth publishing. They are outside `table_params_hash`
/// precisely so that a founder *can* come back on a new group; that is the
/// escape hatch, not the ordinary path.
///
/// The same shape as the other two: 32 raw bytes, nothing to parse, and an
/// existing file that is not one is never overwritten. `tox_options_set_savedata_type`
/// with `TOX_SAVEDATA_TYPE_SECRET_KEY` takes exactly these bytes.
pub fn load_or_create_tox_key(dir: &Path) -> io::Result<[u8; 32]> {
    let path = tox_key_path(dir);

    if let Ok(bytes) = fs::read(&path) {
        if bytes.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} exists and is not a 32-byte key; move it aside rather than \
                 letting this overwrite it",
                path.display()
            ),
        ));
    }

    fs::create_dir_all(dir)?;
    let key = crate::security::rng::secret_32()
        .map_err(|e| io::Error::other(format!("no operating system randomness: {e}")))?;
    fs::write(&path, key)?;
    Ok(key)
}

/// Where the profile's lock lives.
pub fn lock_path(dir: &Path) -> PathBuf {
    dir.join("profile.lock")
}

/// `S1-FV`: the profile, held by the one client running with it.
///
/// Two clients started with one profile are one player twice: one identity,
/// one application key and one Tox key on the network at once, one session
/// record written by both, and both taking the same seat back -- the owner
/// did it by mistake (2026-09-15) and the table stood, each copy waiting for
/// the other to let it in. Held for the life of the process and let go by the
/// operating system when the process ends, however it ends, so a client that
/// was killed leaves nothing behind to clear.
///
/// On Windows the lock file is opened with no sharing, which no other open can
/// pass while it is held. `D-078`: elsewhere it is locked with `File::try_lock`
/// -- an advisory lock every copy of this client asks for, and the reason this
/// crate's `rust-version` is 1.89.
pub struct ProfileLock {
    _file: Option<fs::File>,
}

/// Why the profile was not held.
#[derive(Debug)]
pub enum LockError {
    /// Another running client holds it.
    InUse,
    /// The lock could not be taken for some other reason -- a read-only
    /// folder, say. The client starts anyway: the guard is not worth a client
    /// that will not start.
    Unavailable(io::Error),
}

impl ProfileLock {
    /// No lock held, for the client that starts without one.
    pub fn none() -> ProfileLock {
        ProfileLock { _file: None }
    }
}

/// `S1-FV`: hold the profile, or say that another client holds it.
pub fn lock_profile(dir: &Path) -> Result<ProfileLock, LockError> {
    fs::create_dir_all(dir).map_err(LockError::Unavailable)?;
    open_exclusive(&lock_path(dir)).map(|file| ProfileLock { _file: file })
}

#[cfg(windows)]
fn open_exclusive(path: &Path) -> Result<Option<fs::File>, LockError> {
    use std::os::windows::fs::OpenOptionsExt;
    /// `ERROR_SHARING_VIOLATION` and `ERROR_LOCK_VIOLATION`.
    const HELD: [i32; 2] = [32, 33];
    match fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).share_mode(0).open(path) {
        Ok(file) => Ok(Some(file)),
        Err(e) if e.raw_os_error().is_some_and(|code| HELD.contains(&code)) => Err(LockError::InUse),
        Err(e) => Err(LockError::Unavailable(e)),
    }
}

#[cfg(not(windows))]
fn open_exclusive(path: &Path) -> Result<Option<fs::File>, LockError> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(LockError::Unavailable)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(fs::TryLockError::WouldBlock) => Err(LockError::InUse),
        Err(fs::TryLockError::Error(e)) => Err(LockError::Unavailable(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **`D-078`: where the profile lives.** Windows keeps it beside the
    /// program, whatever stands where; Linux keeps it in the user's data folder
    /// -- a package's program sits where nobody can write -- unless a profile
    /// already stands beside the program, which is a portable copy.
    #[test]
    fn the_profile_is_beside_the_program_on_windows_and_in_the_data_folder_on_linux() {
        let exe = Path::new("/usr/lib/p2poker");
        let beside = exe.join("profile");
        let none = |_: &Path| false;
        let there = |p: &Path| p == Path::new("/usr/lib/p2poker/profile");
        let data = || Some(PathBuf::from("/home/someone/.data"));
        let home = || Some(PathBuf::from("/home/someone"));

        assert_eq!(profile_dir_for(true, Some(exe), data(), home(), none), beside, "Windows: beside, always");
        assert_eq!(
            profile_dir_for(false, Some(exe), data(), home(), none),
            Path::new("/home/someone/.data/p2poker/profile"),
            "Linux: the XDG data folder"
        );
        assert_eq!(
            profile_dir_for(false, Some(exe), None, home(), none),
            Path::new("/home/someone/.local/share/p2poker/profile"),
            "and its default under the home folder"
        );
        assert_eq!(profile_dir_for(false, Some(exe), data(), home(), there), beside, "a portable copy keeps its own");
        assert_eq!(profile_dir_for(false, Some(exe), None, None, none), beside, "no home at all: beside, as before");
    }

    /// `S1-FV`: a profile one client holds is refused to a second, and is
    /// free again the moment the first lets it go -- on Windows by the sharing
    /// mode, elsewhere by `File::try_lock` (`D-078`).
    #[test]
    fn a_profile_is_held_by_one_client_at_a_time() {
        let dir = scratch("profile-lock");
        let first = lock_profile(&dir).expect("the first client holds the profile");
        assert!(matches!(lock_profile(&dir), Err(LockError::InUse)), "a second is refused while the first runs");
        assert!(matches!(lock_profile(&dir), Err(LockError::InUse)), "and again");
        drop(first);
        let again = lock_profile(&dir).expect("free once the first has gone");
        let other = lock_profile(&scratch("profile-lock-other")).expect("another profile is another lock");
        assert!(matches!(lock_profile(&dir), Err(LockError::InUse)));
        drop((again, other));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("p2p-poker-test-{name}"));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// The Tox key survives a restart, or a founder comes back as a stranger to
    /// its own table.
    ///
    /// Both ends add each other from keys carried in the advertisement and the
    /// join request, and a Tox friendship is two-sided — so a founder with a new
    /// key is a founder every seated player is waiting for and none of them can
    /// reach.
    #[test]
    fn the_tox_key_survives_a_restart() {
        let dir = scratch("tox-key");
        let first = load_or_create_tox_key(&dir).unwrap();
        let again = load_or_create_tox_key(&dir).unwrap();
        assert_eq!(first, again);
        assert_ne!(first, [0u8; 32], "and it is not thirty-two zero bytes");
    }

    /// Three identities, three files, and no two of them are the same secret.
    #[test]
    fn the_three_identities_are_three_secrets() {
        let dir = scratch("three-keys");
        let app = load_or_create_app_key(&dir).unwrap().to_bytes();
        let tox = load_or_create_tox_key(&dir).unwrap();
        let net = load_or_create_identity(&dir).unwrap();
        assert_ne!(app, tox, "the player and the Tox key are not one secret");
        assert_ne!(
            tox_key_path(&dir),
            app_key_path(&dir),
            "and they are not one file"
        );
        assert_ne!(tox_key_path(&dir), identity_path(&dir));
        // The network identity is a different type, so it is compared through
        // what it produces rather than through its bytes.
        assert_ne!(net.public().to_peer_id().to_bytes()[..32], tox[..]);
    }

    /// A file that is not a key is never overwritten. Silently replacing one
    /// would be silently changing who this client is.
    #[test]
    fn a_tox_key_file_that_is_not_a_key_is_refused() {
        let dir = scratch("tox-key-bad");
        fs::create_dir_all(&dir).unwrap();
        fs::write(tox_key_path(&dir), b"not a key").unwrap();
        assert!(load_or_create_tox_key(&dir).is_err());
        assert_eq!(
            fs::read(tox_key_path(&dir)).unwrap(),
            b"not a key",
            "and it is still there"
        );
    }

    /// The player's own key survives a restart too, or rejoining your own table
    /// makes you a stranger to it.
    #[test]
    fn the_application_key_survives_a_restart() {
        let dir = scratch("app-key");
        let first = load_or_create_app_key(&dir).unwrap();
        let again = load_or_create_app_key(&dir).unwrap();
        assert_eq!(
            first.verifying_key().to_bytes(),
            again.verifying_key().to_bytes()
        );
    }

    /// And it is a **different** key from the network identity. §20 keeps the
    /// two apart, and one file holding both would make them one secret.
    #[test]
    fn the_two_identities_are_two_keys() {
        let dir = scratch("two-keys");
        let node = load_or_create_identity(&dir).unwrap();
        let player = load_or_create_app_key(&dir).unwrap();
        let node_bytes = node.clone().try_into_ed25519().unwrap().to_bytes();
        assert_ne!(
            &node_bytes[..32],
            &player.to_bytes()[..],
            "the network identity and the player identity are the same secret"
        );
        assert_ne!(
            identity_path(&dir),
            app_key_path(&dir),
            "one file cannot hold two identities"
        );
    }

    /// A file that is there and is not a key is left alone. It might be
    /// somebody's data in the wrong place.
    #[test]
    fn an_unrecognised_file_is_never_overwritten() {
        let dir = scratch("not-a-key");
        fs::create_dir_all(&dir).unwrap();
        fs::write(app_key_path(&dir), b"this is not a key").unwrap();
        assert!(load_or_create_app_key(&dir).is_err());
        assert_eq!(
            fs::read(app_key_path(&dir)).unwrap(),
            b"this is not a key",
            "the file was overwritten"
        );
    }

    /// The point of persisting it: the same directory is the same client.
    #[test]
    fn the_identity_survives_a_restart() {
        let dir = scratch("identity");
        let first = load_or_create_identity(&dir).unwrap();
        let again = load_or_create_identity(&dir).unwrap();

        assert_eq!(
            libp2p::PeerId::from(first.public()),
            libp2p::PeerId::from(again.public()),
            "a client that changes identity on restart is a stream of strangers"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// Two directories are two clients, which is what makes the profile portable
    /// rather than global.
    #[test]
    fn two_profiles_are_two_clients() {
        let a = scratch("two-a");
        let b = scratch("two-b");
        let ka = load_or_create_identity(&a).unwrap();
        let kb = load_or_create_identity(&b).unwrap();
        assert_ne!(
            libp2p::PeerId::from(ka.public()),
            libp2p::PeerId::from(kb.public())
        );
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    /// A file that is there and is not an identity is not overwritten. It might
    /// be somebody's data in the wrong place, and losing it silently to make a
    /// client start is not a trade this code gets to make.
    #[test]
    fn a_foreign_file_is_refused_rather_than_replaced() {
        let dir = scratch("foreign");
        fs::create_dir_all(&dir).unwrap();
        let path = identity_path(&dir);
        fs::write(&path, b"this is not a key").unwrap();

        let err = load_or_create_identity(&dir).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            fs::read(&path).unwrap(),
            b"this is not a key",
            "and it is still there"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// The file is the seed and nothing else, so there is no format to get wrong
    /// on a path that runs before anything else does.
    #[test]
    fn the_file_is_exactly_the_seed() {
        let dir = scratch("size");
        load_or_create_identity(&dir).unwrap();
        assert_eq!(fs::read(identity_path(&dir)).unwrap().len(), 32);
        let _ = fs::remove_dir_all(&dir);
    }

    /// Beside the executable, in a named subdirectory — not in the executable's
    /// own directory, so that copying the profile is copying one folder.
    #[test]
    fn the_profile_sits_beside_the_executable() {
        let dir = profile_dir();
        assert_eq!(dir.file_name().unwrap(), "profile");
        assert!(dir.is_absolute() || dir.starts_with("."));
    }
}
