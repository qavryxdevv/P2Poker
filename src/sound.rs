//! PokerTH's sounds (the owner, 2026-09-13: every sound of the game's actions
//! and situations from PokerTH, with a switch in the settings).
//!
//! # What plays when
//!
//! PokerTH's own rules, read from its sources (`gamehandler.cpp`,
//! `soundevents.cpp`, `lobbyhandler.cpp`, commit `944e8b83`):
//!
//! * **an action** -- fold, check, call, bet, raise, all in -- whoever takes
//!   it, under *game actions*;
//! * **your turn**, three seconds before this client's own decision runs out,
//!   when it still has not acted -- under the master switch only, as PokerTH
//!   plays it;
//! * **the blinds went up**: the first two raises `blinds_raises_level1`, the
//!   third and fourth `level2`, from the fifth on `level3`, under *blind raise
//!   notification*;
//! * **somebody sat down** while the table fills (`playerconnected`), and the
//!   table full and set (`onlinegameready`), under *network game
//!   notifications*;
//! * **the lobby chat named this client**, under *lobby chat notifications*.
//!
//! `dealtwocards` ships with PokerTH and nothing there plays it any more; it
//! is played here when this client's own two cards are dealt, under *game
//! actions* -- what its name says it is for.
//!
//! # How it plays
//!
//! Like PokerTH's WinMM backend (`qtaudioplayer.cpp`): a small pool of
//! `waveOut` handles, each sound written to a free one, so a call and the
//! turn warning overlap instead of cutting each other off; the volume scales
//! the samples. The fourteen files are 16-bit stereo PCM at 44.1 kHz, embedded
//! in the binary so a copied client still sounds; their provenance is
//! `assets/pokerth/PROVENANCE.md`. Anywhere but Windows the player is silent.

/// One of PokerTH's sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Cue {
    Fold,
    Check,
    Call,
    Bet,
    Raise,
    AllIn,
    DealTwoCards,
    YourTurn,
    BlindsRaiseLevel1,
    BlindsRaiseLevel2,
    BlindsRaiseLevel3,
    PlayerConnected,
    OnlineGameReady,
    LobbyChatNotify,
}

/// Which of PokerTH's switches a sound is under (`SoundSettings.qml`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// *Game actions (check, call, raise ...)*.
    GameActions,
    /// *Lobby chat notifications*.
    LobbyChat,
    /// *Network game notifications*.
    NetworkGame,
    /// *Blind raise notification*.
    BlindRaise,
    /// Only the master switch: PokerTH's turn warning.
    Always,
}

impl Cue {
    pub const ALL: [Cue; 14] = [
        Cue::Fold,
        Cue::Check,
        Cue::Call,
        Cue::Bet,
        Cue::Raise,
        Cue::AllIn,
        Cue::DealTwoCards,
        Cue::YourTurn,
        Cue::BlindsRaiseLevel1,
        Cue::BlindsRaiseLevel2,
        Cue::BlindsRaiseLevel3,
        Cue::PlayerConnected,
        Cue::OnlineGameReady,
        Cue::LobbyChatNotify,
    ];

    pub fn category(self) -> Category {
        match self {
            Cue::Fold | Cue::Check | Cue::Call | Cue::Bet | Cue::Raise | Cue::AllIn | Cue::DealTwoCards => {
                Category::GameActions
            }
            Cue::YourTurn => Category::Always,
            Cue::BlindsRaiseLevel1 | Cue::BlindsRaiseLevel2 | Cue::BlindsRaiseLevel3 => Category::BlindRaise,
            Cue::PlayerConnected | Cue::OnlineGameReady => Category::NetworkGame,
            Cue::LobbyChatNotify => Category::LobbyChat,
        }
    }

    /// The file's name in PokerTH's `data/sounds/default`.
    pub fn name(self) -> &'static str {
        match self {
            Cue::Fold => "fold",
            Cue::Check => "check",
            Cue::Call => "call",
            Cue::Bet => "bet",
            Cue::Raise => "raise",
            Cue::AllIn => "allin",
            Cue::DealTwoCards => "dealtwocards",
            Cue::YourTurn => "yourturn",
            Cue::BlindsRaiseLevel1 => "blinds_raises_level1",
            Cue::BlindsRaiseLevel2 => "blinds_raises_level2",
            Cue::BlindsRaiseLevel3 => "blinds_raises_level3",
            Cue::PlayerConnected => "playerconnected",
            Cue::OnlineGameReady => "onlinegameready",
            Cue::LobbyChatNotify => "lobbychatnotify",
        }
    }

    /// The whole WAV file, as embedded.
    pub fn wav(self) -> &'static [u8] {
        match self {
            Cue::Fold => include_bytes!("../assets/pokerth/sounds/fold.wav"),
            Cue::Check => include_bytes!("../assets/pokerth/sounds/check.wav"),
            Cue::Call => include_bytes!("../assets/pokerth/sounds/call.wav"),
            Cue::Bet => include_bytes!("../assets/pokerth/sounds/bet.wav"),
            Cue::Raise => include_bytes!("../assets/pokerth/sounds/raise.wav"),
            Cue::AllIn => include_bytes!("../assets/pokerth/sounds/allin.wav"),
            Cue::DealTwoCards => include_bytes!("../assets/pokerth/sounds/dealtwocards.wav"),
            Cue::YourTurn => include_bytes!("../assets/pokerth/sounds/yourturn.wav"),
            Cue::BlindsRaiseLevel1 => include_bytes!("../assets/pokerth/sounds/blinds_raises_level1.wav"),
            Cue::BlindsRaiseLevel2 => include_bytes!("../assets/pokerth/sounds/blinds_raises_level2.wav"),
            Cue::BlindsRaiseLevel3 => include_bytes!("../assets/pokerth/sounds/blinds_raises_level3.wav"),
            Cue::PlayerConnected => include_bytes!("../assets/pokerth/sounds/playerconnected.wav"),
            Cue::OnlineGameReady => include_bytes!("../assets/pokerth/sounds/onlinegameready.wav"),
            Cue::LobbyChatNotify => include_bytes!("../assets/pokerth/sounds/lobbychatnotify.wav"),
        }
    }

    /// `soundevents.cpp`: the sound for the `level`th raise of the blinds in a
    /// game, counting from one.
    pub fn blinds_raised(level: u32) -> Option<Cue> {
        match level {
            0 => None,
            1 | 2 => Some(Cue::BlindsRaiseLevel1),
            3 | 4 => Some(Cue::BlindsRaiseLevel2),
            _ => Some(Cue::BlindsRaiseLevel3),
        }
    }
}

/// The format of a WAV file this player plays, and where its samples are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pcm<'a> {
    pub channels: u16,
    pub rate: u32,
    pub bits: u16,
    pub data: &'a [u8],
}

/// Read a RIFF/WAVE file's `fmt ` and `data` chunks. Only uncompressed PCM
/// is accepted, which is what PokerTH ships and what `waveOut` takes as it is.
pub fn parse_wav(bytes: &[u8]) -> Option<Pcm<'_>> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12;
    let mut format: Option<(u16, u16, u32, u16)> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]]) as usize;
        let body = pos + 8;
        let end = body.checked_add(size)?;
        if id == b"fmt " && size >= 16 && end <= bytes.len() {
            let tag = u16::from_le_bytes([bytes[body], bytes[body + 1]]);
            let channels = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]);
            let rate = u32::from_le_bytes([bytes[body + 4], bytes[body + 5], bytes[body + 6], bytes[body + 7]]);
            let bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
            format = Some((tag, channels, rate, bits));
        } else if id == b"data" {
            let (tag, channels, rate, bits) = format?;
            if tag != 1 || channels == 0 || bits != 16 {
                return None;
            }
            let end = end.min(bytes.len());
            let usable = (end - body) / usize::from(channels * 2) * usize::from(channels * 2);
            return Some(Pcm {
                channels,
                rate,
                bits,
                data: &bytes[body..body + usable],
            });
        }
        // Chunks are padded to an even length.
        pos = end + (size & 1);
    }
    None
}

/// The samples at `volume` tenths of their loudness (PokerTH's 1..=10).
pub fn scaled(data: &[u8], volume: u8) -> Vec<u8> {
    let v = i32::from(volume.min(10));
    if v >= 10 {
        return data.to_vec();
    }
    let mut out = Vec::with_capacity(data.len());
    for pair in data.chunks_exact(2) {
        let s = i32::from(i16::from_le_bytes([pair[0], pair[1]]));
        let t = (s * v / 10).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        out.extend_from_slice(&t.to_le_bytes());
    }
    out
}

pub use player::Player;

#[cfg(windows)]
mod player {
    use super::{parse_wav, scaled, Cue};
    use windows_sys::Win32::Media::Audio::{
        waveOutClose, waveOutOpen, waveOutPrepareHeader, waveOutReset, waveOutUnprepareHeader, waveOutWrite,
        CALLBACK_NULL, HWAVEOUT, WAVEFORMATEX, WAVEHDR, WAVE_FORMAT_PCM, WAVE_MAPPER, WHDR_DONE,
    };

    /// How many sounds may overlap. PokerTH's pool is a handful as well; a
    /// table never has more going at once than a call, a turn warning and a
    /// raise of the blinds.
    const POOL: usize = 4;

    struct Slot {
        handle: HWAVEOUT,
        /// Boxed, because `waveOut` holds its address until the sound is done.
        header: Box<WAVEHDR>,
        /// The samples the header points at; alive for as long as it plays.
        buffer: Vec<u8>,
        prepared: bool,
    }

    /// The sounds, played on this thread's `waveOut` handles.
    pub struct Player {
        slots: Vec<Slot>,
        opened: bool,
    }

    impl Default for Player {
        fn default() -> Self {
            Player::new()
        }
    }

    impl Player {
        pub fn new() -> Player {
            Player { slots: Vec::new(), opened: false }
        }

        /// Open the pool the first time a sound is asked for: a client whose
        /// sounds are off never touches the audio device.
        fn open(&mut self) {
            if self.opened {
                return;
            }
            self.opened = true;
            let format = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_PCM as u16,
                nChannels: 2,
                nSamplesPerSec: 44_100,
                nAvgBytesPerSec: 44_100 * 4,
                nBlockAlign: 4,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            for _ in 0..POOL {
                let mut handle: HWAVEOUT = std::ptr::null_mut();
                // SAFETY: `handle` is a valid out-pointer and `format` a valid
                // PCM description that outlives the call.
                let ok = unsafe { waveOutOpen(&mut handle, WAVE_MAPPER, &format, 0, 0, CALLBACK_NULL) };
                if ok == 0 && !handle.is_null() {
                    self.slots.push(Slot {
                        handle,
                        header: Box::new(WAVEHDR::default()),
                        buffer: Vec::new(),
                        prepared: false,
                    });
                }
            }
        }

        /// Play `cue` at `volume` tenths (1..=10). Returns at once; the
        /// device plays it.
        pub fn play(&mut self, cue: Cue, volume: u8) {
            self.open();
            let Some(pcm) = parse_wav(cue.wav()) else {
                return;
            };
            if pcm.channels != 2 || pcm.rate != 44_100 {
                return;
            }
            let pick = self
                .slots
                .iter()
                // SAFETY: reading the flags of a header this player owns.
                .position(|s| !s.prepared || (s.header.dwFlags & WHDR_DONE) != 0)
                // Every handle busy: the oldest-opened one gives way, as
                // PokerTH's pool does.
                .or(if self.slots.is_empty() { None } else { Some(0) });
            let Some(i) = pick else {
                return;
            };
            let slot = &mut self.slots[i];
            // SAFETY: the handle is open; a prepared header is unprepared
            // before it is reused, after a reset has returned it.
            unsafe {
                if slot.prepared {
                    if (slot.header.dwFlags & WHDR_DONE) == 0 {
                        waveOutReset(slot.handle);
                    }
                    waveOutUnprepareHeader(slot.handle, &mut *slot.header, std::mem::size_of::<WAVEHDR>() as u32);
                    slot.prepared = false;
                }
            }
            slot.buffer = scaled(pcm.data, volume.clamp(1, 10));
            *slot.header = WAVEHDR::default();
            slot.header.lpData = slot.buffer.as_mut_ptr();
            slot.header.dwBufferLength = slot.buffer.len() as u32;
            // SAFETY: the header points at `buffer`, which the slot keeps
            // alive and unmoved until the header is unprepared.
            unsafe {
                if waveOutPrepareHeader(slot.handle, &mut *slot.header, std::mem::size_of::<WAVEHDR>() as u32) == 0 {
                    slot.prepared = true;
                    waveOutWrite(slot.handle, &mut *slot.header, std::mem::size_of::<WAVEHDR>() as u32);
                }
            }
        }
    }

    impl Drop for Player {
        fn drop(&mut self) {
            for slot in &mut self.slots {
                // SAFETY: every handle here was opened by `open`; a reset
                // returns the buffer before it is unprepared and freed.
                unsafe {
                    waveOutReset(slot.handle);
                    if slot.prepared {
                        waveOutUnprepareHeader(slot.handle, &mut *slot.header, std::mem::size_of::<WAVEHDR>() as u32);
                    }
                    waveOutClose(slot.handle);
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod player {
    use super::Cue;

    /// Silent: this client plays sounds through Windows' `waveOut` only.
    #[derive(Default)]
    pub struct Player;

    impl Player {
        pub fn new() -> Player {
            Player
        }

        pub fn play(&mut self, _cue: Cue, _volume: u8) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every embedded file is the PCM PokerTH ships, and plays as it is.
    #[test]
    fn every_sound_is_sixteen_bit_stereo_pcm() {
        for cue in Cue::ALL {
            let pcm = parse_wav(cue.wav()).unwrap_or_else(|| panic!("{} does not parse", cue.name()));
            assert_eq!((pcm.channels, pcm.rate, pcm.bits), (2, 44_100, 16), "{}", cue.name());
            assert!(pcm.data.len() > 4_000, "{} is {} bytes of samples", cue.name(), pcm.data.len());
            assert_eq!(pcm.data.len() % 4, 0, "{} ends mid-frame", cue.name());
        }
    }

    /// PokerTH's levels: one or two raises, three or four, five and more.
    #[test]
    fn the_blinds_sound_follows_the_raise_count() {
        assert_eq!(Cue::blinds_raised(0), None);
        assert_eq!(Cue::blinds_raised(1), Some(Cue::BlindsRaiseLevel1));
        assert_eq!(Cue::blinds_raised(2), Some(Cue::BlindsRaiseLevel1));
        assert_eq!(Cue::blinds_raised(3), Some(Cue::BlindsRaiseLevel2));
        assert_eq!(Cue::blinds_raised(4), Some(Cue::BlindsRaiseLevel2));
        assert_eq!(Cue::blinds_raised(5), Some(Cue::BlindsRaiseLevel3));
        assert_eq!(Cue::blinds_raised(9), Some(Cue::BlindsRaiseLevel3));
    }

    #[test]
    fn the_volume_scales_the_samples_and_ten_is_untouched() {
        let loud: Vec<u8> = [1000i16, -2000, 32767, -32768].iter().flat_map(|s| s.to_le_bytes()).collect();
        assert_eq!(scaled(&loud, 10), loud);
        let half = scaled(&loud, 5);
        let back: Vec<i16> = half.chunks_exact(2).map(|p| i16::from_le_bytes([p[0], p[1]])).collect();
        assert_eq!(back, vec![500, -1000, 16383, -16384]);
    }

    #[test]
    fn a_file_that_is_not_pcm_is_refused() {
        assert!(parse_wav(b"RIFF\0\0\0\0WAVEjunk").is_none());
        assert!(parse_wav(b"not a wav").is_none());
    }
}
