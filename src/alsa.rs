//! `D-079`: sound on Linux -- ALSA's own library, opened at run time.
//!
//! On Windows the table's sounds and the search music go to `waveOut`, which is
//! part of the system. Linux's counterpart is ALSA's `libasound.so.2`: every
//! desktop distribution carries it, and PipeWire and PulseAudio both serve its
//! `default` device through their ALSA plugins, so this one API reaches
//! whichever of them the desktop runs.
//!
//! **Opened with `dlopen`, not linked** -- as the window's OpenGL is, through
//! the same `libloading` the tree already carries for it. A machine without the
//! library starts and is silent, as a Windows machine without an audio device
//! is, and the build needs no ALSA headers. Six of its functions are used, with
//! the types `alsa/pcm.h` gives them, and the library stays loaded for as long
//! as the process runs.

use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
use std::sync::OnceLock;

/// `snd_pcm_stream_t`: playback.
const STREAM_PLAYBACK: c_int = 0;
/// `snd_pcm_format_t`: signed 16-bit little-endian -- what `waveOut` is given
/// on Windows, and what the sounds and the decoded music are.
const FORMAT_S16_LE: c_int = 2;
/// `snd_pcm_access_t`: interleaved frames, written with `snd_pcm_writei`.
const ACCESS_RW_INTERLEAVED: c_int = 3;

type Handle = *mut c_void;

struct Api {
    open: unsafe extern "C" fn(*mut Handle, *const c_char, c_int, c_int) -> c_int,
    set_params: unsafe extern "C" fn(Handle, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int,
    writei: unsafe extern "C" fn(Handle, *const c_void, c_ulong) -> c_long,
    recover: unsafe extern "C" fn(Handle, c_int, c_int) -> c_int,
    drain: unsafe extern "C" fn(Handle) -> c_int,
    close: unsafe extern "C" fn(Handle) -> c_int,
    /// Held for the life of the process: every pointer above is into it.
    _lib: libloading::Library,
}

fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| {
        // SAFETY: loading a system library by its soname runs its
        // initialisers, which for libasound set up nothing but its own state.
        let lib = unsafe { libloading::Library::new("libasound.so.2") }.ok()?;
        // SAFETY: every symbol is taken with the signature `alsa/pcm.h`
        // declares for it, and `lib` outlives each pointer, being kept beside
        // them in the one `Api` that never goes away.
        unsafe {
            let open = *lib.get(b"snd_pcm_open\0").ok()?;
            let set_params = *lib.get(b"snd_pcm_set_params\0").ok()?;
            let writei = *lib.get(b"snd_pcm_writei\0").ok()?;
            let recover = *lib.get(b"snd_pcm_recover\0").ok()?;
            let drain = *lib.get(b"snd_pcm_drain\0").ok()?;
            let close = *lib.get(b"snd_pcm_close\0").ok()?;
            Some(Api { open, set_params, writei, recover, drain, close, _lib: lib })
        }
    })
    .as_ref()
}

/// Whether ALSA's library could be opened on this machine.
pub fn available() -> bool {
    api().is_some()
}

/// An open playback stream on ALSA's `default` device.
pub struct Pcm {
    handle: Handle,
    api: &'static Api,
    frame_bytes: usize,
}

// SAFETY: an ALSA stream may be used from any thread, one thread at a time; a
// `Pcm` is owned by the one thread that plays through it and is never shared.
unsafe impl Send for Pcm {}

impl Pcm {
    /// The `default` device, for interleaved 16-bit samples, `channels` to a
    /// frame at `rate`, with about `latency_ms` of sound queued ahead. `None`
    /// where there is no ALSA library, no device, or the device refuses the
    /// format -- which `soft_resample` makes rare: ALSA converts the rate.
    pub fn open(channels: u16, rate: u32, latency_ms: u32) -> Option<Pcm> {
        let api = api()?;
        let mut handle: Handle = std::ptr::null_mut();
        // SAFETY: `handle` is a valid out-pointer, and the device's name a
        // NUL-terminated string that outlives the call.
        if unsafe { (api.open)(&mut handle, c"default".as_ptr(), STREAM_PLAYBACK, 0) } < 0 || handle.is_null() {
            return None;
        }
        let pcm = Pcm { handle, api, frame_bytes: usize::from(channels.max(1)) * 2 };
        // SAFETY: the stream was just opened; the values are ALSA's own enums.
        let set = unsafe {
            (api.set_params)(
                handle,
                FORMAT_S16_LE,
                ACCESS_RW_INTERLEAVED,
                c_uint::from(channels),
                rate,
                1,
                latency_ms.saturating_mul(1_000),
            )
        };
        // Refused, the stream is closed by `pcm` going out of scope.
        (set >= 0).then_some(pcm)
    }

    /// Play `bytes` -- interleaved little-endian samples -- to their end,
    /// waiting while the device is full. `false` when the device gave up.
    ///
    /// An underrun, or a device suspended and woken, is recovered and the
    /// writing goes on; anything else, or a recovery that keeps failing, ends
    /// the sound rather than the thread playing it.
    pub fn write(&mut self, bytes: &[u8]) -> bool {
        let mut at = 0usize;
        let mut left = bytes.len() / self.frame_bytes;
        let mut failures = 0u32;
        while left > 0 {
            // SAFETY: the pointer and the frame count both stay inside `bytes`.
            let written = unsafe { (self.api.writei)(self.handle, bytes[at..].as_ptr().cast(), left as c_ulong) };
            if written > 0 {
                let frames = (written as usize).min(left);
                at += frames * self.frame_bytes;
                left -= frames;
                failures = 0;
                continue;
            }
            failures += 1;
            if failures > 3 {
                return false;
            }
            if written < 0 {
                // SAFETY: the stream is open; the error is the one it gave.
                if unsafe { (self.api.recover)(self.handle, written as c_int, 1) } < 0 {
                    return false;
                }
            }
        }
        true
    }

    /// Wait until what was written has been heard.
    pub fn drain(&mut self) {
        // SAFETY: the stream is open.
        unsafe { (self.api.drain)(self.handle) };
    }
}

impl Drop for Pcm {
    fn drop(&mut self) {
        // SAFETY: opened by `open`, and closed here once.
        unsafe { (self.api.close)(self.handle) };
    }
}
