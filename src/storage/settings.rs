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
    /// The odds beside the table's action bar (the owner, 2026-09-13: shown
    /// or hidden in the settings, shown by default). `None` in a file written
    /// before the switch existed, which reads as shown.
    #[n(3)]
    pub show_odds: Option<bool>,
    /// The table chat beside the action bar, on its other side (the owner,
    /// 2026-09-13: *in the same way, the chat in the left corner*). `None`
    /// reads as shown.
    #[n(4)]
    pub show_chat: Option<bool>,
    /// `D-050`: a hand that may muck at a showdown is mucked at once (`true`,
    /// the owner's default) or waits three seconds for *Show cards*. `None`
    /// reads as on.
    #[n(5)]
    pub auto_muck: Option<bool>,
    /// `D-064`: what the automatic search was last asked for, and how long
    /// past searches took. `None` in a file written before the search existed.
    #[n(6)]
    pub search: Option<SearchSettings>,
    /// `D-002` as the owner ruled it (2026-09-18): whether this client relays
    /// connections for other players of this game -- on by default, turned off
    /// here. `None` until the player has seen the first-run notice about it,
    /// and read as on.
    #[n(7)]
    pub relay: Option<bool>,
}

/// `D-064`: the most past searches remembered.
pub const SEARCH_HISTORY_MAX: usize = 20;

/// `D-064`: the automatic search's last request and its history -- the
/// window's word on what to expect before a search has measured anything.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct SearchSettings {
    /// `net::matchmaker::Format::code`.
    #[n(0)]
    pub format: u8,
    /// How many games at once, `1..=MAX_GAMES`.
    #[n(1)]
    pub tables: u8,
    /// Search again when the game ends.
    #[n(2)]
    pub again: bool,
    /// The searches that found a game: the format's code and the seconds each
    /// took, oldest first, at most `SEARCH_HISTORY_MAX`.
    #[n(3)]
    pub history: Vec<(u8, u32)>,
}

impl Default for SearchSettings {
    /// Any format, one game, once: the choice that finds a game soonest.
    fn default() -> Self {
        SearchSettings {
            format: crate::net::matchmaker::Format::Any.code(),
            tables: 1,
            again: false,
            history: Vec::new(),
        }
    }
}

impl SearchSettings {
    /// What a search of `format` took before, typically: the median of the
    /// remembered searches of that format -- of every format for *any*.
    pub fn typical_s(&self, format: u8) -> Option<u32> {
        let any = crate::net::matchmaker::Format::Any.code();
        let mut took: Vec<u32> = self
            .history
            .iter()
            .filter(|(f, _)| format == any || *f == format)
            .map(|(_, s)| *s)
            .collect();
        if took.is_empty() {
            return None;
        }
        took.sort_unstable();
        Some(took[took.len() / 2])
    }

    /// Remember a search that found a game.
    pub fn remember(&mut self, format: u8, took_s: u32) {
        self.history.push((format, took_s));
        while self.history.len() > SEARCH_HISTORY_MAX {
            self.history.remove(0);
        }
    }

    /// Inside every bound: a hand-edited file arrives here too.
    fn repair(&mut self) {
        self.tables = self.tables.clamp(1, crate::net::matchmaker::MAX_GAMES);
        if crate::net::matchmaker::Format::parse(self.format).is_none() {
            self.format = crate::net::matchmaker::Format::Any.code();
        }
        while self.history.len() > SEARCH_HISTORY_MAX {
            self.history.remove(0);
        }
    }
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
            show_odds: None,
            show_chat: None,
            auto_muck: None,
            search: None,
            relay: None,
        }
    }

    /// `D-002`: whether this client relays for other players of this game: yes
    /// unless the player said no.
    pub fn relay(&self) -> bool {
        self.relay.unwrap_or(true)
    }

    /// `D-064`: the search's settings in force: the player's, or the defaults.
    pub fn search(&self) -> SearchSettings {
        self.search.clone().unwrap_or_default()
    }

    /// The sound switches in force: the player's, or PokerTH's defaults.
    pub fn sound(&self) -> SoundSettings {
        self.sound.unwrap_or_default()
    }

    /// Whether the table shows the odds beside its action bar: yes unless the
    /// player said no.
    pub fn show_odds(&self) -> bool {
        self.show_odds.unwrap_or(true)
    }

    /// Whether the table shows its chat beside the action bar: yes unless the
    /// player said no.
    pub fn show_chat(&self) -> bool {
        self.show_chat.unwrap_or(true)
    }

    /// `D-050`: whether a hand that may muck is mucked at once: yes unless the
    /// player said no.
    pub fn auto_muck(&self) -> bool {
        self.auto_muck.unwrap_or(true)
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
        if let Some(s) = self.search.as_mut() {
            s.repair();
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
            show_odds: None,
            show_chat: None,
            auto_muck: None,
            search: None,
            relay: None,
        };
        save(&dir, &s, &k).unwrap();
        assert_eq!(load(&dir, &k), s);
    }

    /// `D-064`: the search's settings and history round-trip, and a file's
    /// numbers outside their bounds are pulled inside them.
    #[test]
    fn the_search_settings_round_trip_and_repair() {
        let dir = scratch("search");
        let k = key();
        let mut search = SearchSettings { format: 2, tables: 9, again: true, history: Vec::new() };
        for i in 0..(SEARCH_HISTORY_MAX as u32 + 5) {
            search.remember(2, 100 + i);
        }
        search.remember(1, 30);
        let s = Settings { search: Some(search), ..Settings::defaults(&k) };
        save(&dir, &s, &k).unwrap();
        let back = load(&dir, &k).search();
        assert_eq!(back.tables, crate::net::matchmaker::MAX_GAMES, "clamped on save");
        assert_eq!(back.history.len(), SEARCH_HISTORY_MAX);
        assert_eq!(back.typical_s(1), Some(30));
        assert!(back.typical_s(2).is_some_and(|t| t > 100));
        assert!(back.typical_s(3).is_none());
        assert!(back.typical_s(4).is_some(), "any format reads every search");
        let mut odd = SearchSettings { format: 9, tables: 0, again: false, history: Vec::new() };
        odd.repair();
        assert_eq!((odd.format, odd.tables), (4, 1));
    }

    /// A file written before the odds switch existed still reads, and reads as
    /// the odds shown.
    #[test]
    fn a_file_from_before_the_odds_switch_shows_the_odds() {
        #[derive(minicbor::Encode)]
        #[cbor(array)]
        struct Before {
            #[n(0)]
            nickname: String,
            #[n(1)]
            text_percent: u16,
            #[n(2)]
            sound: Option<SoundSettings>,
        }
        let dir = scratch("before-odds");
        let k = key();
        std::fs::create_dir_all(&dir).unwrap();
        let old = Before { nickname: "Alice".into(), text_percent: 110, sound: Some(SoundSettings { volume: 3, ..Default::default() }) };
        std::fs::write(settings_path(&dir), minicbor::to_vec(&old).unwrap()).unwrap();
        let s = load(&dir, &k);
        assert_eq!(s.nickname, "Alice");
        assert_eq!(s.sound().volume, 3);
        assert_eq!(s.show_odds, None);
        assert!(s.show_odds());
        assert!(s.show_chat());
        assert!(s.auto_muck(), "and a hand that may muck is mucked at once");
        assert_eq!(s.relay, None, "the first-run notice about relaying not yet seen");
        assert!(s.relay(), "and the client relays for other players, as by default");
        let hidden = Settings { show_odds: Some(false), ..s };
        save(&dir, &hidden, &k).unwrap();
        assert!(!load(&dir, &k).show_odds());
        // `D-002`: the player's word on relaying round-trips.
        let off = Settings { relay: Some(false), ..load(&dir, &k) };
        save(&dir, &off, &k).unwrap();
        assert!(!load(&dir, &k).relay());
    }

    /// A first run has no file and is not an error.
    #[test]
    fn a_client_with_no_settings_has_defaults() {
        let dir = scratch("first-run");
        let k = key();
        let s = load(&dir, &k);
        assert_eq!(s.text_percent, 100);
        assert!(s.show_odds(), "the odds are shown by default");
        assert!(s.show_chat(), "and so is the chat beside the bar");
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
            show_odds: None,
            show_chat: None,
            auto_muck: None,
            search: None,
            relay: None,
        };
        s.repair(&k);
        assert!(s.nickname.len() <= NAME_MAX);

        let mut s = Settings {
            nickname: "a\u{7}b\nc".into(),
            text_percent: 100,
            sound: None,
            show_odds: None,
            show_chat: None,
            auto_muck: None,
            search: None,
            relay: None,
        };
        s.repair(&k);
        assert!(!s.nickname.chars().any(|c| c.is_control()));
        assert_eq!(s.nickname, "abc");

        // And an empty one, which a roster would take and nobody could read.
        let mut s = Settings {
            nickname: "   ".into(),
            text_percent: 100,
            sound: None,
            show_odds: None,
            show_chat: None,
            auto_muck: None,
            search: None,
            relay: None,
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
                show_odds: None,
                show_chat: None,
                auto_muck: None,
                search: None,
                relay: None,
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
            show_odds: None,
            show_chat: None,
            auto_muck: None,
            search: None,
            relay: None,
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
            show_odds: None,
            show_chat: None,
            auto_muck: None,
            search: None,
            relay: None,
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
            show_odds: None,
            show_chat: None,
            auto_muck: None,
            search: None,
            relay: None,
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
                show_odds: None,
                show_chat: None,
                auto_muck: None,
                search: None,
                relay: None,
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
                show_odds: None,
                show_chat: None,
                auto_muck: None,
                search: None,
                relay: None,
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
