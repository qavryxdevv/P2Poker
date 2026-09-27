//! `S1-JB`: the one place this client reads the time of day.
//!
//! **Two clocks, and the difference is the point.** The *wall clock* --
//! milliseconds since the epoch, what a system's date and time settings set --
//! is what every message is stamped with and what the lobby's freshness rules
//! compare: an advert lives `AD_TTL_MS`, chat, the players' list and the
//! search's queue refuse what is more than `CLOCK_SLACK_MS` from this clock,
//! and `D-070`'s hour keys are its hours. The *monotonic clock*
//! (`std::time::Instant`, tokio's timers) counts time elapsed and nothing sets
//! it; every timeout this client keeps for itself runs on it, a turn's once it
//! has started included (`net::run`'s `Clock::apply`). Changing a computer's
//! time -- a player, a correction, a virtual machine resumed from an old
//! snapshot -- moves the first and not the second.
//!
//! Every read of the wall clock goes through [`now_unix_ms`], for two reasons:
//!
//! 1. **A jump can be tested without anybody's clock being set.** A binary
//!    built to be measured is told, by `P2P_POKER_CLOCK_JUMPS`, that its wall
//!    clock reads a given amount off the system's from a given second of its
//!    life. Only this process's reading moves; the system's time, and every
//!    other program on the machine, stay as they are. A player's build has no
//!    such knob.
//! 2. **A jump can be noticed, and outlived.** [`Watch`] compares how far the
//!    two clocks moved between two readings. The node reads it every two
//!    seconds: every moment a table and its hand keep on this client's own wall
//!    clock is moved by the difference ([`rebase`]), so a stage's budget or a
//!    hand's is still the time that passed -- before `S1-JB` a clock set an hour
//!    ahead aborted the hand being played, its budget "spent" -- and a jump of
//!    [`JUMP_MS`] or more is told to the window, which asks the internet's time
//!    again and warns the player when the clock is now out
//!    (`app::update::clock_notice`).
//!
//! The module's tests hold that no other module reads `SystemTime::now()`
//! itself, bar the ones that compare a file's time with the system's -- both
//! stamped by the same clock, whatever it says.

use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// How far the wall clock must move beyond the monotonic clock between two
/// readings to count as a jump the player is told about.
///
/// Well above what a clock corrected by its system's time service moves in two
/// seconds (a slew, or a step of a second or two), and well below the smallest
/// difference the lobby notices (`protocol::constants::AD_TTL_MS`, 90 s).
pub const JUMP_MS: i64 = 30_000;

/// How far the wall clock must move beyond the monotonic clock between two
/// readings for the moments this client keeps on its own wall clock to be
/// moved with it ([`rebase`]). Two clocks read together move together to the
/// millisecond; a second apart is a clock that was set.
pub const REBASE_MS: i64 = 1_000;

/// Whether a difference [`Watch::moved`] found is a jump to tell the player of.
pub fn is_jump(by_ms: i64) -> bool {
    by_ms.abs() >= JUMP_MS
}

/// Milliseconds since this process started, on the monotonic clock: what the
/// time between two moments is measured in when nothing of it crosses the wire
/// (`tox::table`'s reassembler and flood meters). A changed system time does
/// not move it.
pub fn mono_ms() -> u64 {
    u64::try_from(started().elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// `S1-JB`: a moment kept on this client's own wall clock, moved by the jump
/// the clock just made, so that the time since it stays the time that passed:
/// a stage's budget, a hand's, a vote's wait, a word said again. `0` means
/// *never* in every field this is used on, and stays so.
pub fn rebase(at_ms: &mut u64, by_ms: i64) {
    if *at_ms != 0 {
        *at_ms = apply_shift_ms(*at_ms, by_ms);
    }
}

/// [`rebase`], for a moment that may not have happened.
pub fn rebase_opt(at_ms: &mut Option<u64>, by_ms: i64) {
    if let Some(at) = at_ms.as_mut() {
        rebase(at, by_ms);
    }
}

/// The time of day, in milliseconds since the epoch, as this client believes
/// it: the system's, or -- in a binary built to be measured -- the system's
/// moved by `P2P_POKER_CLOCK_JUMPS`.
pub fn now_unix_ms() -> u64 {
    let system = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    shifted(system)
}

/// The moment this process's life is counted from, for the jumps' schedule.
/// Taken at the top of `main`; the first reading takes it otherwise.
pub fn start() {
    started();
}

fn started() -> Instant {
    static STARTED: OnceLock<Instant> = OnceLock::new();
    *STARTED.get_or_init(Instant::now)
}

#[cfg(not(feature = "fault-harness"))]
fn shifted(system_ms: u64) -> u64 {
    system_ms
}

#[cfg(feature = "fault-harness")]
fn shifted(system_ms: u64) -> u64 {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static JUMPS: OnceLock<Vec<(u64, i64)>> = OnceLock::new();
    static SAID: AtomicUsize = AtomicUsize::new(0);
    let jumps = JUMPS.get_or_init(|| {
        let Ok(spec) = std::env::var("P2P_POKER_CLOCK_JUMPS") else {
            return Vec::new();
        };
        parse_jumps(&spec).unwrap_or_else(|| {
            println!("fault-harness: P2P_POKER_CLOCK_JUMPS={spec:?} is not at_s:shift_s[,at_s:shift_s...]; no jump");
            Vec::new()
        })
    });
    let life_s = started().elapsed().as_secs();
    let reached = jumps.iter().take_while(|(at_s, _)| *at_s <= life_s).count();
    let Some(&(at_s, shift_s)) = reached.checked_sub(1).and_then(|i| jumps.get(i)) else {
        return system_ms;
    };
    // Said once per jump, by whichever reading reaches it first.
    if SAID.fetch_max(reached, Ordering::Relaxed) < reached {
        println!(
            "fault-harness: from second {at_s} the wall clock reads {shift_s:+} s off the system's, as \
             P2P_POKER_CLOCK_JUMPS asked (S1-JB)"
        );
    }
    apply_shift(system_ms, shift_s)
}

/// A wall-clock reading moved by `shift_s` seconds, clamped at the epoch.
pub fn apply_shift(system_ms: u64, shift_s: i64) -> u64 {
    apply_shift_ms(system_ms, shift_s.saturating_mul(1_000))
}

/// A wall-clock reading moved by `shift_ms` milliseconds, clamped at the epoch
/// and never to `0`, which means *never* where it is kept.
pub fn apply_shift_ms(at_ms: u64, shift_ms: i64) -> u64 {
    let moved = if shift_ms >= 0 {
        at_ms.saturating_add(shift_ms as u64)
    } else {
        at_ms.saturating_sub(shift_ms.unsigned_abs())
    };
    moved.max(1)
}

/// `P2P_POKER_CLOCK_JUMPS`: `at_s:shift_s[,at_s:shift_s...]`. From `at_s`
/// seconds after the process started the wall clock reads `shift_s` seconds
/// off the system's (`+` ahead, `-` behind), until the next entry's moment;
/// `0` puts it right again. So `120:+3600,600:0` is an hour ahead from the
/// second minute and right from the tenth. The moments must not go back.
pub fn parse_jumps(spec: &str) -> Option<Vec<(u64, i64)>> {
    let mut jumps = Vec::new();
    for entry in spec.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let (at, shift) = entry.split_once(':')?;
        let at_s: u64 = at.trim().parse().ok()?;
        let shift_s: i64 = shift.trim().trim_start_matches('+').parse().ok()?;
        if jumps.last().is_some_and(|&(last, _): &(u64, i64)| at_s < last) {
            return None;
        }
        jumps.push((at_s, shift_s));
    }
    (!jumps.is_empty()).then_some(jumps)
}

/// `S1-JB`: notices the wall clock moving by more than the monotonic clock
/// did between two readings -- a time changed, or a machine that slept where
/// the monotonic clock does not count sleep (Linux's). Which of the two it was
/// is not this reading's to say: the window asks the internet's time again.
#[derive(Debug, Clone, Copy)]
pub struct Watch {
    mono: Instant,
    wall_ms: u64,
}

impl Default for Watch {
    fn default() -> Self {
        Self::new()
    }
}

impl Watch {
    pub fn new() -> Watch {
        Watch { mono: Instant::now(), wall_ms: now_unix_ms() }
    }

    /// How far the wall clock moved beyond the monotonic clock since the last
    /// reading -- positive: ahead. Two clocks that moved together give zero,
    /// give or take the moment between the two readings.
    pub fn moved(&mut self) -> i64 {
        self.moved_at(Instant::now(), now_unix_ms())
    }

    /// [`Self::moved`] at given readings of the two clocks.
    pub fn moved_at(&mut self, mono: Instant, wall_ms: u64) -> i64 {
        let mono_moved = i64::try_from(mono.saturating_duration_since(self.mono).as_millis()).unwrap_or(i64::MAX);
        let wall_moved = wall_ms as i64 - self.wall_ms as i64;
        self.mono = mono;
        self.wall_ms = wall_ms;
        wall_moved.saturating_sub(mono_moved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_schedule_of_jumps_is_read_and_a_bad_one_refused() {
        assert_eq!(parse_jumps("120:+3600,600:0"), Some(vec![(120, 3_600), (600, 0)]));
        assert_eq!(parse_jumps(" 60:-7200 "), Some(vec![(60, -7_200)]));
        assert_eq!(parse_jumps("10:+5,10:-5"), Some(vec![(10, 5), (10, -5)]));
        for bad in ["", "120", "120:+1h", "x:5", "600:0,120:+3600", "-5:10"] {
            assert_eq!(parse_jumps(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_shift_moves_the_reading_and_stops_short_of_never() {
        assert_eq!(apply_shift(1_000_000, 60), 1_060_000);
        assert_eq!(apply_shift(1_000_000, -60), 940_000);
        assert_eq!(apply_shift(1_000, 0), 1_000);
        // Clamped at the epoch, and never to 0, which a kept moment means *never* by.
        assert_eq!(apply_shift(1_000, -60), 1);
        assert_eq!(apply_shift_ms(5, -5), 1);
    }

    /// `S1-JB`: a kept moment moves with the clock, and *never* stays never.
    #[test]
    fn a_kept_moment_moves_with_the_clock_and_never_stays_never() {
        let mut at = 1_000_000;
        rebase(&mut at, 3_600_000);
        assert_eq!(at, 4_600_000);
        rebase(&mut at, -3_600_000);
        assert_eq!(at, 1_000_000);
        let mut never = 0;
        rebase(&mut never, 3_600_000);
        assert_eq!(never, 0);
        let mut maybe = Some(1_000_000);
        rebase_opt(&mut maybe, -1_000);
        assert_eq!(maybe, Some(999_000));
        let mut none: Option<u64> = None;
        rebase_opt(&mut none, 5_000);
        assert_eq!(none, None);
    }

    /// The watch measures how far the wall clock moved beyond the monotonic
    /// clock, of either sign, and nothing for clocks that moved together -- a
    /// slow tick included, which moves both.
    #[test]
    fn the_watch_tells_a_jump_from_time_passing() {
        let t0 = Instant::now();
        let mut w = Watch { mono: t0, wall_ms: 1_000_000 };
        // Two seconds on both clocks, then a slow tick of forty on both.
        assert_eq!(w.moved_at(t0 + Duration::from_secs(2), 1_002_000), 0);
        assert_eq!(w.moved_at(t0 + Duration::from_secs(42), 1_042_000), 0);
        // An hour ahead, in two seconds of monotonic time.
        assert_eq!(w.moved_at(t0 + Duration::from_secs(44), 1_044_000 + 3_600_000), 3_600_000);
        // The next tick is measured from there: nothing new.
        assert_eq!(w.moved_at(t0 + Duration::from_secs(46), 1_046_000 + 3_600_000), 0);
        // Back by the hour.
        assert_eq!(w.moved_at(t0 + Duration::from_secs(48), 1_048_000), -3_600_000);
        // A correction of five seconds: moved with, and no jump to tell of.
        let by = w.moved_at(t0 + Duration::from_secs(50), 1_055_000);
        assert_eq!(by, 5_000);
        assert!(by.abs() >= REBASE_MS && !is_jump(by));
        // Just under and at the line of a jump.
        assert!(!is_jump(JUMP_MS - 1) && !is_jump(-(JUMP_MS - 1)));
        assert!(is_jump(JUMP_MS) && is_jump(-JUMP_MS));
    }

    /// `S1-JB`: the wall clock is read here and nowhere else, so that a jump
    /// can be told to a measured binary and noticed by the node. The files that
    /// may still read it themselves compare a file's time with the system's --
    /// both stamped by the one clock the system keeps.
    ///
    /// **To make this fail:** put `std::time::SystemTime::now()` back into
    /// `tox::table::millis`.
    #[test]
    fn the_wall_clock_is_read_in_one_place() {
        const MAY: [&str; 4] = [
            "clock.rs",          // this module
            "tox/nodes.rs",      // the age of the nodes file, against its own mtime
            "gui/installer.rs",  // when an installation was made, written into it
            "install/copy.rs",   // a test that dates a file into the past
        ];
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("src").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let rel = path.strip_prefix(&root).expect("under src").to_string_lossy().replace('\\', "/");
                if MAY.contains(&rel.as_str()) {
                    continue;
                }
                let text = std::fs::read_to_string(&path).expect("a source file");
                if text.lines().any(|l| !l.trim_start().starts_with("//") && l.contains("SystemTime::now()")) {
                    offenders.push(rel);
                }
            }
        }
        offenders.sort();
        assert!(offenders.is_empty(), "these read the wall clock themselves, not through clock::now_unix_ms: {offenders:?}");
    }
}
