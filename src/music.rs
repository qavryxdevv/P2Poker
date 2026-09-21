//! `D-069`: the tune a search waits to.
//!
//! While the automatic search (`D-064`) looks for a game and the player sits at
//! no table, the lobby plays one track round and round, quietly. The moment the
//! search ends -- a table found, or the search given up -- the music **fades**
//! and stops; it is never cut. It is one switch in the settings, under the
//! sound's master switch, and on unless the player says otherwise.
//!
//! **The track is in the binary** (`MUSIC_TRACK`): the client is one portable
//! file and reads nothing from beside itself but its profile. It is Ogg Vorbis
//! at about 110 kbit/s -- 2.5 MB for three minutes of stereo at 44.1 kHz, a
//! little over half of the MP3 it was made from, with no loss a lobby could
//! show -- decoded as it plays by `lewton`, which is Rust and nothing else. An
//! MP3 also starts with a gap its encoder put there; a Vorbis stream does not,
//! so the turn from the end back to the beginning is the track's own.
//!
//! **A thread of its own.** The paint thread only says what should be
//! (`Music::set`, every frame, a comparison when nothing changed). The music's
//! thread owns the audio device, decodes a twentieth of a second at a time into
//! four small buffers, and shapes every frame with the envelope -- so a fade is
//! heard within a fifth of a second of being asked for, no other sound of the
//! client is touched, and the volume Windows keeps for this application is
//! never moved. A client that never searches never opens the device and never
//! starts the thread.
//!
//! The device is Windows' `waveOut`, as `sound::Player`'s is, and since `D-079`
//! on Linux ALSA's `default` device (`crate::alsa`), where a write waits while
//! the device plays, so the device itself sets the pace; elsewhere the music is
//! silent and everything here but the device still runs and is tested.

use std::io::Cursor;

use lewton::inside_ogg::OggStreamReader;

/// The track: *The Dealer's Shuffle*, see `assets/music/README.md`.
pub const MUSIC_TRACK: &[u8] = include_bytes!("../assets/music/the-dealers-shuffle.ogg");
/// From silence to the music's level as a search begins.
pub const MUSIC_FADE_IN_MS: u32 = 900;
/// And down to silence as it ends: long enough to be heard as a fade.
pub const MUSIC_FADE_OUT_MS: u32 = 1_800;
/// The music's level under the sound effects', in per cent of the volume the
/// player chose: an accompaniment, not a performance.
pub const MUSIC_LEVEL_PERCENT: u32 = 30;

/// The gain the music plays at for a volume of the settings (`1..=10`).
pub fn gain_for(volume: u8) -> f32 {
    f32::from(volume.clamp(1, 10)) / 10.0 * (MUSIC_LEVEL_PERCENT as f32 / 100.0)
}

/// The fade, frame by frame: a level that walks towards its target at the
/// fade's own pace, heard as its square -- a straight line in amplitude sounds
/// like a drop at the end, the square like a hand on a volume knob.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Envelope {
    level: f32,
    target: f32,
    up: f32,
    down: f32,
}

impl Envelope {
    pub fn new(rate: u32) -> Envelope {
        let per = |ms: u32| 1_000.0 / (rate.max(1) as f32 * ms.max(1) as f32);
        Envelope { level: 0.0, target: 0.0, up: per(MUSIC_FADE_IN_MS), down: per(MUSIC_FADE_OUT_MS) }
    }

    /// Play, or fade away. Turning round in the middle of a fade goes on from
    /// where the level is.
    pub fn aim(&mut self, on: bool) {
        self.target = if on { 1.0 } else { 0.0 };
    }

    /// The gain of the next frame.
    pub fn next_gain(&mut self) -> f32 {
        if self.level < self.target {
            self.level = (self.level + self.up).min(self.target);
        } else if self.level > self.target {
            self.level = (self.level - self.down).max(self.target);
        }
        self.level * self.level
    }

    /// Faded all the way out, and not asked to play.
    pub fn silent(&self) -> bool {
        self.level <= 0.0 && self.target <= 0.0
    }
}

/// The track as a source of samples that never ends: at its last frame it
/// goes round to its first.
pub struct Track {
    reader: OggStreamReader<Cursor<&'static [u8]>>,
    pending: Vec<i16>,
    at: usize,
    /// How many times it has gone round.
    pub rounds: u32,
}

impl Track {
    pub fn open() -> Option<Track> {
        let reader = OggStreamReader::new(Cursor::new(MUSIC_TRACK)).ok()?;
        Some(Track { reader, pending: Vec::new(), at: 0, rounds: 0 })
    }

    pub fn rate(&self) -> u32 {
        self.reader.ident_hdr.audio_sample_rate
    }

    pub fn channels(&self) -> u16 {
        u16::from(self.reader.ident_hdr.audio_channels)
    }

    /// Fill `out` with the next interleaved samples, all of it, whatever
    /// happens: a stream that will not decode plays as silence and never as a
    /// stalled thread.
    pub fn fill(&mut self, out: &mut [i16]) {
        let (mut done, mut barren) = (0, 0);
        while done < out.len() {
            if self.at < self.pending.len() {
                let n = (out.len() - done).min(self.pending.len() - self.at);
                out[done..done + n].copy_from_slice(&self.pending[self.at..self.at + n]);
                done += n;
                self.at += n;
                barren = 0;
                continue;
            }
            match self.reader.read_dec_packet_itl() {
                Ok(Some(packet)) => {
                    self.pending = packet;
                    self.at = 0;
                }
                // The end -- or a page that will not decode, which is treated
                // as one: from the beginning again.
                Ok(None) | Err(_) => {
                    barren += 1;
                    match OggStreamReader::new(Cursor::new(MUSIC_TRACK)) {
                        Ok(reader) if barren <= 2 => {
                            self.reader = reader;
                            self.pending.clear();
                            self.at = 0;
                            self.rounds += 1;
                        }
                        _ => {
                            out[done..].fill(0);
                            return;
                        }
                    }
                }
            }
        }
    }

    /// Go to a frame of the track, for the test that listens to it going round.
    #[cfg(test)]
    fn seek(&mut self, frame: u64) {
        self.reader.seek_absgp_pg(frame).expect("the track seeks");
        self.pending.clear();
        self.at = 0;
    }
}

/// Shape `samples` (interleaved, `channels` to a frame) with the envelope at
/// `gain`, into little-endian bytes for the device.
pub fn shape(samples: &[i16], channels: usize, gain: f32, envelope: &mut Envelope, bytes: &mut [u8]) {
    for (f, frame) in samples.chunks(channels.max(1)).enumerate() {
        let g = envelope.next_gain() * gain;
        for (c, s) in frame.iter().enumerate() {
            let v = (f32::from(*s) * g).round().clamp(-32_768.0, 32_767.0) as i16;
            let i = (f * channels.max(1) + c) * 2;
            bytes[i..i + 2].copy_from_slice(&v.to_le_bytes());
        }
    }
}

#[cfg(windows)]
mod device {
    use super::{shape, Envelope, Track};
    use std::sync::mpsc::{Receiver, RecvTimeoutError};
    use windows_sys::Win32::Media::Audio::{
        waveOutClose, waveOutOpen, waveOutPrepareHeader, waveOutReset, waveOutUnprepareHeader, waveOutWrite,
        CALLBACK_NULL, HWAVEOUT, WAVEFORMATEX, WAVEHDR, WAVE_FORMAT_PCM, WAVE_MAPPER, WHDR_DONE,
    };

    pub enum Cmd {
        /// Play at this gain, from the beginning if nothing is playing.
        Play(f32),
        /// Fade away and stop.
        Fade,
    }

    /// Four buffers of a twentieth of a second: a fifth of a second queued,
    /// which is how soon a fade is heard.
    const CHUNKS: usize = 4;
    const CHUNK_MS: u32 = 50;
    const HDR: u32 = std::mem::size_of::<WAVEHDR>() as u32;

    struct Chunk {
        /// Boxed: `waveOut` holds its address while it is queued.
        header: Box<WAVEHDR>,
        /// Never resized once the header points at it.
        bytes: Vec<u8>,
        samples: Vec<i16>,
        prepared: bool,
        queued: bool,
    }

    struct Voice {
        handle: HWAVEOUT,
        chunks: Vec<Chunk>,
        track: Track,
        envelope: Envelope,
        gain: f32,
        channels: usize,
    }

    impl Voice {
        fn open() -> Option<Voice> {
            let track = Track::open()?;
            let (rate, channels) = (track.rate(), track.channels());
            if rate == 0 || !(1..=2).contains(&channels) {
                return None;
            }
            let align = channels * 2;
            let format = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_PCM as u16,
                nChannels: channels,
                nSamplesPerSec: rate,
                nAvgBytesPerSec: rate * u32::from(align),
                nBlockAlign: align,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            let mut handle: HWAVEOUT = std::ptr::null_mut();
            // SAFETY: `handle` is a valid out-pointer and `format` a valid PCM
            // description that outlives the call.
            let ok = unsafe { waveOutOpen(&mut handle, WAVE_MAPPER, &format, 0, 0, CALLBACK_NULL) };
            if ok != 0 || handle.is_null() {
                return None;
            }
            let frames = (rate * CHUNK_MS / 1_000) as usize;
            let samples = frames * usize::from(channels);
            let chunks = (0..CHUNKS)
                .map(|_| Chunk {
                    header: Box::new(WAVEHDR::default()),
                    bytes: vec![0u8; samples * 2],
                    samples: vec![0i16; samples],
                    prepared: false,
                    queued: false,
                })
                .collect();
            Some(Voice { handle, chunks, track, envelope: Envelope::new(rate), gain: 0.0, channels: usize::from(channels) })
        }

        /// Whether every buffer handed to the device has been played.
        fn drained(&self) -> bool {
            self.chunks.iter().all(|c| !c.queued || (c.header.dwFlags & WHDR_DONE) != 0)
        }

        /// Hand the device every buffer it has finished with, filled with what
        /// comes next. Faded out, nothing more is handed over.
        fn pump(&mut self) {
            if self.envelope.silent() {
                return;
            }
            for c in &mut self.chunks {
                if c.queued && (c.header.dwFlags & WHDR_DONE) == 0 {
                    continue;
                }
                self.track.fill(&mut c.samples);
                shape(&c.samples, self.channels, self.gain, &mut self.envelope, &mut c.bytes);
                // SAFETY: the handle is open. The header points at `bytes`,
                // which the chunk keeps alive, unmoved and at one length until
                // the header is unprepared in `drop`; a header is written again
                // only once the device has marked it done.
                unsafe {
                    if !c.prepared {
                        *c.header = WAVEHDR::default();
                        c.header.lpData = c.bytes.as_mut_ptr();
                        c.header.dwBufferLength = c.bytes.len() as u32;
                        if waveOutPrepareHeader(self.handle, &mut *c.header, HDR) != 0 {
                            continue;
                        }
                        c.prepared = true;
                    }
                    c.queued = waveOutWrite(self.handle, &mut *c.header, HDR) == 0;
                }
            }
        }
    }

    impl Drop for Voice {
        fn drop(&mut self) {
            // SAFETY: the handle was opened by `open`; a reset returns every
            // buffer before it is unprepared and freed.
            unsafe {
                waveOutReset(self.handle);
                for c in &mut self.chunks {
                    if c.prepared {
                        waveOutUnprepareHeader(self.handle, &mut *c.header, HDR);
                    }
                }
                waveOutClose(self.handle);
            }
        }
    }

    /// The music's thread: until the window lets go of its end of the channel.
    pub fn run(rx: Receiver<Cmd>) {
        let mut voice: Option<Voice> = None;
        loop {
            let cmd = if voice.is_some() {
                rx.recv_timeout(std::time::Duration::from_millis(10))
            } else {
                rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
            };
            match cmd {
                Ok(Cmd::Play(gain)) => {
                    if voice.is_none() {
                        voice = Voice::open();
                    }
                    if let Some(v) = voice.as_mut() {
                        v.gain = gain;
                        v.envelope.aim(true);
                    }
                }
                Ok(Cmd::Fade) => {
                    if let Some(v) = voice.as_mut() {
                        v.envelope.aim(false);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if let Some(v) = voice.as_mut() {
                v.pump();
                // Faded out and played out: the device is given back, and the
                // next search hears the track from its beginning.
                if v.envelope.silent() && v.drained() {
                    voice = None;
                }
            }
        }
    }
}

/// `D-079`: the music on Linux -- the same track, envelope and commands, into
/// a stream on ALSA's `default` device. A write waits while the device holds
/// a fifth of a second, so the device sets the pace, and a command is looked
/// for between two twentieths of a second: a fade is heard as soon as on
/// Windows.
#[cfg(target_os = "linux")]
mod device {
    use super::{shape, Envelope, Track};
    use std::sync::mpsc::{Receiver, TryRecvError};

    pub enum Cmd {
        /// Play at this gain, from the beginning if nothing is playing.
        Play(f32),
        /// Fade away and stop.
        Fade,
    }

    /// A twentieth of a second at a time, into a device holding a fifth.
    const CHUNK_MS: u32 = 50;
    const LATENCY_MS: u32 = 200;

    struct Voice {
        pcm: crate::alsa::Pcm,
        track: Track,
        envelope: Envelope,
        gain: f32,
        channels: usize,
        samples: Vec<i16>,
        bytes: Vec<u8>,
    }

    impl Voice {
        fn open() -> Option<Voice> {
            let track = Track::open()?;
            let (rate, channels) = (track.rate(), track.channels());
            if rate == 0 || !(1..=2).contains(&channels) {
                return None;
            }
            let pcm = crate::alsa::Pcm::open(channels, rate, LATENCY_MS)?;
            let samples = (rate * CHUNK_MS / 1_000) as usize * usize::from(channels);
            Some(Voice {
                pcm,
                track,
                envelope: Envelope::new(rate),
                gain: 0.0,
                channels: usize::from(channels),
                samples: vec![0; samples],
                bytes: vec![0; samples * 2],
            })
        }

        /// The next twentieth of a second, written -- once the device has room
        /// for it. `false` when the device gave up.
        fn pump(&mut self) -> bool {
            self.track.fill(&mut self.samples);
            shape(&self.samples, self.channels, self.gain, &mut self.envelope, &mut self.bytes);
            self.pcm.write(&self.bytes)
        }
    }

    /// The music's thread: until the window lets go of its end of the channel.
    pub fn run(rx: Receiver<Cmd>) {
        let mut voice: Option<Voice> = None;
        loop {
            // Playing, the device sets the pace and a command is looked for
            // between two chunks; with nothing playing, the thread sleeps until
            // it is told something.
            let cmd = if voice.is_some() {
                match rx.try_recv() {
                    Ok(cmd) => Some(cmd),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => break,
                }
            } else {
                match rx.recv() {
                    Ok(cmd) => Some(cmd),
                    Err(_) => break,
                }
            };
            match cmd {
                Some(Cmd::Play(gain)) => {
                    if voice.is_none() {
                        voice = Voice::open();
                    }
                    if let Some(v) = voice.as_mut() {
                        v.gain = gain;
                        v.envelope.aim(true);
                    }
                }
                Some(Cmd::Fade) => {
                    if let Some(v) = voice.as_mut() {
                        v.envelope.aim(false);
                    }
                }
                None => {}
            }
            // Faded out -- or a device that gave up -- the stream is closed,
            // and the next search hears the track from its beginning.
            if voice.as_mut().is_some_and(|v| v.envelope.silent() || !v.pump()) {
                voice = None;
            }
        }
    }
}

/// What the window owns: it says what should be, the thread does it.
#[derive(Default)]
pub struct Music {
    #[cfg(any(windows, target_os = "linux"))]
    tx: Option<std::sync::mpsc::Sender<device::Cmd>>,
    on: bool,
    volume: u8,
}

impl Music {
    pub fn new() -> Music {
        Music::default()
    }

    /// Whether the music is asked for right now.
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// Say what should be: playing at this volume of the settings, or not.
    /// Called every frame; nothing happens unless the answer changed.
    pub fn set(&mut self, on: bool, volume: u8) {
        if on == self.on && (!on || volume == self.volume) {
            return;
        }
        self.on = on;
        self.volume = volume;
        #[cfg(any(windows, target_os = "linux"))]
        {
            if on {
                let tx = self.tx.get_or_insert_with(|| {
                    let (tx, rx) = std::sync::mpsc::channel();
                    let _ = std::thread::Builder::new().name("music".into()).spawn(move || device::run(rx));
                    tx
                });
                let _ = tx.send(device::Cmd::Play(gain_for(volume)));
            } else if let Some(tx) = self.tx.as_ref() {
                let _ = tx.send(device::Cmd::Fade);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The track in the binary is what the device is opened for: Vorbis,
    /// stereo, 44.1 kHz, and small -- a suitable compression, by the owner's
    /// word -- with nothing in it but the music.
    #[test]
    fn the_track_in_the_binary_is_stereo_vorbis_and_small() {
        let t = Track::open().expect("the track decodes");
        assert_eq!((t.rate(), t.channels()), (44_100, 2));
        assert!(MUSIC_TRACK.len() < 3 * 1024 * 1024, "{} bytes", MUSIC_TRACK.len());
        let comments = &t.reader.comment_hdr.comment_list;
        assert!(comments.iter().all(|(k, _)| k.eq_ignore_ascii_case("encoder")), "only the encoder's name: {comments:?}");
    }

    /// It plays, and at its end it goes round to its beginning without a
    /// hole: the buffer that spans the turn is filled to its last sample.
    #[test]
    fn the_track_goes_round() {
        let mut t = Track::open().unwrap();
        let mut first = vec![0i16; 44_100 * 2];
        t.fill(&mut first);
        assert!(first.iter().any(|s| *s != 0), "the first second is music");
        assert_eq!(t.rounds, 0);

        // Three seconds from the end, then five seconds of samples.
        let frames = last_granule(MUSIC_TRACK);
        assert!((170 * 44_100..200 * 44_100).contains(&frames), "{frames} frames");
        t.seek(frames - 3 * 44_100);
        let mut turn = vec![0i16; 5 * 44_100 * 2];
        t.fill(&mut turn);
        assert_eq!(t.rounds, 1, "the end was passed once");
        let after = &turn[4 * 44_100 * 2..];
        assert!(after.iter().any(|s| s.unsigned_abs() > 50), "and the beginning is playing again");
    }

    /// The last page's position is the track's length in frames.
    fn last_granule(ogg: &[u8]) -> u64 {
        let at = ogg.windows(4).rposition(|w| w == b"OggS").expect("an Ogg page");
        u64::from_le_bytes(ogg[at + 6..at + 14].try_into().unwrap())
    }

    /// The fade is a fade: up in its time, down in its longer time, monotonic
    /// both ways, silent only at the very end, and it turns round in the middle
    /// from where it is.
    #[test]
    fn the_envelope_fades_in_and_out() {
        let rate = 1_000;
        let mut e = Envelope::new(rate);
        assert!(e.silent());
        e.aim(true);
        assert!(!e.silent());
        let up: Vec<f32> = (0..MUSIC_FADE_IN_MS).map(|_| e.next_gain()).collect();
        assert!(up.windows(2).all(|w| w[1] >= w[0]));
        assert!(up[0] > 0.0 && up[0] < 0.001, "no click at the start: {}", up[0]);
        assert!((up[up.len() - 1] - 1.0).abs() < 1e-3, "full after the fade-in: {}", up[up.len() - 1]);
        assert!((e.next_gain() - 1.0).abs() < 1e-6, "and it stays there");

        e.aim(false);
        let down: Vec<f32> = (0..MUSIC_FADE_OUT_MS).map(|_| e.next_gain()).collect();
        assert!(down.windows(2).all(|w| w[1] <= w[0]));
        assert!(down[down.len() / 2] > 0.1, "half way down is still heard: a fade, not a cut");
        assert!(down[down.len() - 1] < 1e-4);
        e.next_gain();
        assert!(e.silent());

        // A table found and another search begun before the fade is over.
        e.aim(true);
        for _ in 0..300 {
            e.next_gain();
        }
        e.aim(false);
        let here = e.next_gain();
        e.aim(true);
        assert!(e.next_gain() >= here, "it goes on from where it is");
    }

    /// The music sits under the effects, scales with the volume, never clips,
    /// and the bytes are the device's little-endian samples.
    #[test]
    fn the_music_is_shaped_under_the_effects() {
        assert!(gain_for(10) <= 0.5 && gain_for(10) > gain_for(1) && gain_for(1) > 0.0);
        assert_eq!(gain_for(0), gain_for(1));
        assert_eq!(gain_for(200), gain_for(10));
        let mut e = Envelope::new(10);
        e.aim(true);
        for _ in 0..20_000 {
            e.next_gain();
        }
        let samples = [i16::MAX, i16::MIN, 1_000, -1_000];
        let mut bytes = [0u8; 8];
        shape(&samples, 2, 0.5, &mut e, &mut bytes);
        let out: Vec<i16> = bytes.chunks(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        assert_eq!(out, vec![16_384, -16_384, 500, -500]);
        shape(&samples, 2, 4.0, &mut e, &mut bytes);
        let loud: Vec<i16> = bytes.chunks(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        assert_eq!(&loud[..2], &[i16::MAX, i16::MIN], "clamped, never wrapped round");
    }

    /// The window's side: nothing happens unless the answer changed.
    #[test]
    fn the_handle_only_notes_a_change() {
        let mut m = Music::new();
        assert!(!m.is_on());
        m.set(false, 8);
        assert!(!m.is_on());
    }
}
