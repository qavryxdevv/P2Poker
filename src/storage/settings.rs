//! What the player has chosen: their name, and how big the text is.
//!
//! Beside the keys, in the same profile directory, so the whole folder is still
//! the whole client and copying it copies the settings too.
//!
//! # Why this file may be overwritten and the key files may not
//!
//! [`profile`](super::profile) refuses to overwrite an `identity.key` it cannot
//! read, because a file that is there and is not a key might be somebody's data
//! and losing it silently is not a trade that code gets to make. A settings file
//! is not that: it holds nothing that cannot be chosen again in five seconds,
//! and a client that refuses to start because its settings are unreadable is
//! worse than one that starts with the defaults and says so. So an unreadable
//! settings file yields the defaults, and is replaced the next time anything is
//! saved.
//!
//! # No floating point on disk
//!
//! The text scale is a percentage, an integer. `CRYPTO_LIBS.md` forbids floats
//! in any signed or hashed type; nothing here is either, but a settings file is
//! read by a future version of this client and a float is a value two versions
//! can disagree about for free. It costs nothing to not have one.
//!
//! # The name is display data, and this is where that starts
//!
//! `PROTOCOL.md` §4.3: a `display_name` is untrusted display data forever and is
//! **never an identifier**. That is a rule about names received from the network,
//! and it is easy to read as being only about those. It is not: this is where
//! *this* client's own name is chosen, and it is bounded here — 32 bytes of
//! UTF-8, no control characters — so that the name a player picks is a name a
//! roster will accept. A client that let a player choose a name its own protocol
//! refuses would fail at the join, one round trip later, with a message about
//! somebody else's rule.

use std::io;
use std::path::{Path, PathBuf};

use minicbor::{Decode, Encode};

/// The longest a display name may be, from `PROTOCOL.md` §4.3.
pub const NAME_MAX: usize = 32;

/// The range the text may be scaled through.
///
/// Below 80 the smallest text in the client is back under the size that was
/// called illegible; above 200 the lobby's three columns no longer fit a
/// reasonable window. Both ends are a judgement, and both are enforced on load
/// as well as on save, because a hand-edited file is one of the ways a value
/// arrives.
pub const SCALE_MIN: u16 = 80;
pub const SCALE_MAX: u16 = 200;

/// What the player has chosen.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct Settings {
    /// Shown to other players. Never an identifier.
    #[n(0)]
    pub nickname: String,
    /// Text size, as a percentage of the designed size.
    #[n(1)]
    pub text_percent: u16,
    /// PokerTH's sound switches. `None` in a file written before they existed,
    /// which reads as PokerTH's defaults: everything on, volume eight.
    #[n(2)]
    pub sound: Option<SoundSettings>,
}

/// PokerTH's sound settings (`SoundSettings.qml`, `configfile.cpp`'s defaults):
/// the master switch, the volume from one to ten, and the four categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct SoundSettings {
    /// *Enable sound effects*.
    #[n(0)]
    pub on: bool,
    /// *Volume*, 1..=10.
    #[n(1)]
    pub volume: u8,
    /// *Game actions (check, call, raise ...)*.
    #[n(2)]
    pub game_actions: bool,
    /// *Lobby chat notifications*.
    #[n(3)]
    pub lobby_chat: bool,
    /// *Network game notifications*.
    #[n(4)]
    pub network_game: bool,
    /// *Blind raise notification*.
    #[n(5)]
    pub blind_raise: bool,
}

impl Default for SoundSettings {
    fn default() -> Self {
        SoundSettings {
            on: true,
            volume: 8,
            game_actions: true,
            lobby_chat: true,
            network_game: true,
            blind_raise: true,
        }
    }
}

impl SoundSettings {
    /// Whether `cue` plays under these switches.
    pub fn allows(&self, cue: crate::sound::Cue) -> bool {
        use crate::sound::Category;
        self.on
            && match cue.category() {
                Category::GameActions => self.game_actions,
                Category::LobbyChat => self.lobby_chat,
                Category::NetworkGame => self.network_game,
                Category::BlindRaise => self.blind_raise,
                Category::Always => true,
            }
    }
}

impl Settings {
    /// The settings a client has before anybody has chosen anything.
    ///
    /// The name defaults to eight characters of the player's own key: stable,
    /// theirs, and never confusable with somebody else's — which an empty name
    /// or a "Player 1" would both be.
    pub fn defaults(app_key: &ed25519_dalek::SigningKey) -> Settings {
        Settings {
            nickname: super::profile::short_name(&app_key.verifying_key().to_bytes()),
            text_percent: 100,
            sound: None,
        }
    }

    /// The sound switches in force: the player's, or PokerTH's defaults.
    pub fn sound(&self) -> SoundSettings {
        self.sound.unwrap_or_default()
    }

    /// The scale as egui wants it.
    pub fn zoom(&self) -> f32 {
        self.text_percent as f32 / 100.0
    }

    /// Force this value into the range the protocol and the window will accept.
    ///
    /// Applied on load **and** on save. A settings file is a file a person can
    /// edit, so it is one of the ways an out-of-range value arrives, and the
    /// place to catch it is the boundary rather than the widget that draws it.
    pub fn repair(&mut self, app_key: &ed25519_dalek::SigningKey) {
        self.nickname.retain(|c| !c.is_control());
        while self.nickname.len() > NAME_MAX {
            self.nickname.pop();
        }
        let trimmed = self.nickname.trim();
        if trimmed.len() != self.nickname.len() {
            self.nickname = trimmed.to_string();
        }
        if self.nickname.is_empty() {
            self.nickname = Settings::defaults(app_key).nickname;
        }
        self.text_percent = self.text_percent.clamp(SCALE_MIN, SCALE_MAX);
        if let Some(s) = self.sound.as_mut() {
            s.volume = s.volume.clamp(1, 10);
        }
    }
}

/// Where the settings live.
pub fn settings_path(dir: &Path) -> PathBuf {
    dir.join("settings.cbor")
}

/// Load the settings, or the defaults.
///
/// Never fails: an absent file is a first run and an unreadable one is a
/// corrupt file, and neither is a reason to refuse to start a poker client.
pub fn load(dir: &Path, app_key: &ed25519_dalek::SigningKey) -> Settings {
    let mut s = std::fs::read(settings_path(dir))
        .ok()
        .and_then(|bytes| minicbor::decode::<Settings>(&bytes).ok())
        .unwrap_or_else(|| Settings::defaults(app_key));
    s.repair(app_key);
    s
}

/// Save the settings, atomically.
///
/// Written to a temporary file, flushed, and renamed over the old one. A client
/// killed halfway through a save leaves either the old settings or the new ones
/// and never half of each — the same rule the key files follow, for the same
/// reason: a truncated file is a file that reads as corrupt.
pub fn save(dir: &Path, settings: &Settings, app_key: &ed25519_dalek::SigningKey) -> io::Result<()> {
    let mut whole = settings.clone();
    whole.repair(app_key);

    let bytes = minicbor::to_vec(&whole)
        .map_err(|e| io::Error::other(format!("the settings do not encode: {e}")))?;

    std::fs::create_dir_all(dir)?;
    let tmp = settings_path(dir).with_extension("cbor.tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, settings_path(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("p2p-poker-settings-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn key() -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[5u8; 32])
    }

    #[test]
    fn what_is_saved_is_what_is_loaded() {
        let dir = scratch("round-trip");
        let k = key();
        let s = Settings {
            nickname: "Alice".into(),
            text_percent: 130,
            sound: None,
        };
        save(&dir, &s, &k).unwrap();
        assert_eq!(load(&dir, &k), s);
    }

    /// A first run has no file and is not an error.
    #[test]
    fn a_client_with_no_settings_has_defaults() {
        let dir = scratch("first-run");
        let k = key();
        let s = load(&dir, &k);
        assert_eq!(s.text_percent, 100);
        assert_eq!(s.nickname.len(), 8, "eight characters of the player's key");
        assert!(!settings_path(&dir).exists(), "loading writes nothing");
    }

    /// And a corrupt one is not an error either. A client that will not start
    /// because of its settings file is worse than one that starts with the
    /// defaults.
    #[test]
    fn an_unreadable_file_gives_the_defaults() {
        let dir = scratch("corrupt");
        let k = key();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(settings_path(&dir), b"this is not cbor").unwrap();
        assert_eq!(load(&dir, &k), Settings::defaults(&k));
    }

    /// The name a player may choose is a name a roster will accept. §4.3 caps it
    /// at 32 bytes and forbids control characters, and catching that here is
    /// what stops the failure surfacing one round trip later as somebody else's
    /// rule.
    #[test]
    fn a_name_that_a_roster_would_refuse_is_repaired() {
        let k = key();
        let mut s = Settings {
            nickname: "x".repeat(64),
            text_percent: 100,
            sound: None,
        };
        s.repair(&k);
        assert!(s.nickname.len() <= NAME_MAX);

        let mut s = Settings {
            nickname: "a\u{7}b\nc".into(),
            text_percent: 100,
            sound: None,
        };
        s.repair(&k);
        assert!(!s.nickname.chars().any(|c| c.is_control()));
        assert_eq!(s.nickname, "abc");

        // And an empty one, which a roster would take and nobody could read.
        let mut s = Settings {
            nickname: "   ".into(),
            text_percent: 100,
            sound: None,
        };
        s.repair(&k);
        assert_eq!(s.nickname, Settings::defaults(&k).nickname);
    }

    /// Trimming must not cut a character in half. A name is bytes on the wire
    /// and characters on the screen, and the two are not the same length.
    #[test]
    fn trimming_a_name_leaves_valid_text() {
        let k = key();
        for n in 1..40 {
            let mut s = Settings {
                // Four bytes each, so the cap never falls on a boundary.
                nickname: "\u{1F0A1}".repeat(n),
                text_percent: 100,
                sound: None,
            };
            s.repair(&k);
            assert!(s.nickname.len() <= NAME_MAX, "{n}");
            // The invariant a byte-wise truncation would break: it is still
            // text, and `String` guarantees that only because `pop` removes a
            // whole character.
            assert!(std::str::from_utf8(s.nickname.as_bytes()).is_ok());
        }
    }

    /// The scale is clamped on the way in as well as on the way out, because a
    /// hand-edited file is one of the ways a value arrives.
    #[test]
    fn the_scale_is_clamped_from_a_file_too() {
        let dir = scratch("scale");
        let k = key();
        let absurd = Settings {
            nickname: "Alice".into(),
            text_percent: 5_000,
            sound: None,
        };
        // Written past `save`'s own repair, the way a person editing the file
        // would.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(settings_path(&dir), minicbor::to_vec(&absurd).unwrap()).unwrap();
        assert_eq!(load(&dir, &k).text_percent, SCALE_MAX);

        let tiny = Settings {
            nickname: "Alice".into(),
            text_percent: 1,
            sound: None,
        };
        std::fs::write(settings_path(&dir), minicbor::to_vec(&tiny).unwrap()).unwrap();
        assert_eq!(load(&dir, &k).text_percent, SCALE_MIN);
    }

    /// The zoom is what egui wants: 100 per cent is unscaled.
    #[test]
    fn a_hundred_per_cent_changes_nothing() {
        let s = Settings {
            nickname: "Alice".into(),
            text_percent: 100,
            sound: None,
        };
        assert_eq!(s.zoom(), 1.0);
    }

    /// A save leaves no temporary file behind, and a settings file that already
    /// exists is replaced rather than appended to.
    #[test]
    fn saving_twice_leaves_one_file() {
        let dir = scratch("twice");
        let k = key();
        save(
            &dir,
            &Settings {
                nickname: "First".into(),
                text_percent: 100,
                sound: None,
            },
            &k,
        )
        .unwrap();
        save(
            &dir,
            &Settings {
                nickname: "Second".into(),
                text_percent: 120,
                sound: None,
            },
            &k,
        )
        .unwrap();

        let files: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(files, vec!["settings.cbor".to_string()]);
        assert_eq!(load(&dir, &k).nickname, "Second");
    }
}
