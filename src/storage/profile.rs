//! The portable profile beside the executable.
//!
//! `SPEC_CS.md` §22: the whole directory can be copied to another machine and
//! the client is that client again. Nothing goes to the registry, nothing goes
//! outside this folder, and there is no installer.
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

/// The profile directory, beside the executable.
///
/// Falls back to the working directory if the executable's location cannot be
/// determined — which happens on some sandboxes — because a client that refuses
/// to start over a path lookup is worse than one that keeps its profile
/// somewhere slightly unexpected and says so.
pub fn profile_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("profile")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("p2p-poker-test-{name}"));
        let _ = fs::remove_dir_all(&dir);
        dir
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
