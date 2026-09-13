//! `D-051`: who a member of the table's carrier group is, and whether it is
//! flooding the group.
//!
//! # The binding
//!
//! A carrier group names its members by a key of the group's own, made fresh
//! for every join, which is not a player's signing key and says nothing about
//! one. Until `D-051` the bridge between the two was the first signed message
//! a member carried (`S1-I`), and a message can be carried by anybody: a
//! member that says another seat's message again is paired with that seat
//! (`S1-DW` refused the pairing only while the real seat was speaking).
//!
//! So every seat's client says who it is itself, before it says anything
//! else: the member name it gives the group is a **binding** --
//! [`BINDING_TAG`], its application key, and its signature over the group and
//! its own member key. The group hands a member's name to every other member
//! with the member's key, so a binding is checked against the key the group
//! itself vouches for; a copy of another member's binding names a different
//! member key and does not verify.
//!
//! # The meter
//!
//! A member that floods the group is cut off by every member it floods, on
//! that member's own count of the traffic that reached it and on nobody's
//! word. [`Meter`] is the count: packets and bytes over a short and a long
//! window, and points for traffic an honest client of this build never
//! produces -- a fragment the reassembler refuses, a message that is not a
//! signed event or does not verify, a message kind this client never sends.

use std::collections::VecDeque;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

use crate::protocol::signatures::{hash, Domain};

/// The first bytes of a member name that is a binding.
pub const BINDING_TAG: [u8; 4] = *b"p2pm";

/// A binding's length: the tag, the application key, the signature.
pub const BINDING_LEN: usize = 4 + 32 + 64;

/// What a binding signs: the group and the member key, under its own domain.
fn binding_digest(chat_id: &[u8; 32], member_key: &[u8; 32]) -> [u8; 32] {
    hash(Domain::MemberBinding, &[chat_id, member_key])
}

/// The member name a seat's client gives the group: its binding.
pub fn binding_for(key: &SigningKey, chat_id: &[u8; 32], member_key: &[u8; 32]) -> [u8; BINDING_LEN] {
    let signature = key.sign(&binding_digest(chat_id, member_key));
    let mut out = [0u8; BINDING_LEN];
    out[..4].copy_from_slice(&BINDING_TAG);
    out[4..36].copy_from_slice(key.verifying_key().as_bytes());
    out[36..].copy_from_slice(&signature.to_bytes());
    out
}

/// The application key a member name binds this member key to in this group,
/// or `None` when the name is no binding of this member's.
pub fn bound_to(name: &[u8], chat_id: &[u8; 32], member_key: &[u8; 32]) -> Option<[u8; 32]> {
    if !looks_like_a_binding(name) {
        return None;
    }
    let app: [u8; 32] = name[4..36].try_into().ok()?;
    let signature: [u8; 64] = name[36..].try_into().ok()?;
    let key = VerifyingKey::from_bytes(&app).ok()?;
    key.verify_strict(&binding_digest(chat_id, member_key), &Signature::from_bytes(&signature))
        .ok()?;
    Some(app)
}

/// Whether a name has a binding's shape, whatever it binds. A name of that
/// shape that does not verify is a binding made for another member or
/// another group -- a copy, or a forgery -- and never a display name.
pub fn looks_like_a_binding(name: &[u8]) -> bool {
    name.len() == BINDING_LEN && name[..4] == BINDING_TAG
}

/// The short window the meter counts over, in seconds.
pub const FLOOD_SHORT_S: u64 = 10;
/// Packets from one member within [`FLOOD_SHORT_S`] that no honest client
/// sends: a flood.
pub const FLOOD_SHORT_PACKETS: u64 = 6_000;
/// Bytes from one member within [`FLOOD_SHORT_S`] that no honest client sends.
pub const FLOOD_SHORT_BYTES: u64 = 3_000_000;
/// The long window, in seconds.
pub const FLOOD_LONG_S: u64 = 60;
/// Packets from one member within [`FLOOD_LONG_S`] that no honest client sends.
pub const FLOOD_LONG_PACKETS: u64 = 15_000;
/// Bytes from one member within [`FLOOD_LONG_S`] that no honest client sends.
pub const FLOOD_LONG_BYTES: u64 = 7_500_000;
/// How long a noise point counts, in milliseconds.
pub const NOISE_WINDOW_MS: u64 = 60_000;
/// Noise points within [`NOISE_WINDOW_MS`] that make a member a flooder.
pub const NOISE_LIMIT: u32 = 16;
/// A fragment the reassembler refuses. One, because a group peer id is reused
/// after a member leaves and the partial it left can meet a new member's
/// message; the driver forgets a leaving member's partials, so this is a
/// margin and not an allowance.
pub const NOISE_BAD_FRAGMENT: u32 = 1;
/// A whole message that is not a signed event, or whose signature does not
/// verify under the key inside it. No client of this build builds one.
pub const NOISE_BAD_MESSAGE: u32 = 4;
/// A message kind no client of this build sends: a group text message, a
/// private message, a private packet.
pub const NOISE_STRAY: u32 = 4;

/// Why a member was judged to be flooding the group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flood {
    /// This many packets within the window of this many seconds.
    Packets { window_s: u64, packets: u64 },
    /// This many bytes within the window.
    Bytes { window_s: u64, bytes: u64 },
    /// This many noise points within [`NOISE_WINDOW_MS`].
    Noise { points: u32 },
}

impl std::fmt::Display for Flood {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Flood::Packets { window_s, packets } => write!(f, "{packets} packets in {window_s} s"),
            Flood::Bytes { window_s, bytes } => write!(f, "{bytes} bytes in {window_s} s"),
            Flood::Noise { points } => write!(
                f,
                "{points} points of traffic no client of this build sends within {} s",
                NOISE_WINDOW_MS / 1_000
            ),
        }
    }
}

/// One member's traffic at this client.
#[derive(Debug, Default, Clone)]
pub struct Meter {
    /// `(second, packets, bytes)`, oldest first, the last [`FLOOD_LONG_S`]
    /// seconds at most.
    buckets: VecDeque<(u64, u64, u64)>,
    /// `(millisecond, points)`, oldest first, within [`NOISE_WINDOW_MS`].
    noise: VecDeque<(u64, u32)>,
}

impl Meter {
    /// One packet of this many bytes arrived.
    pub fn packet(&mut self, now_ms: u64, bytes: usize) {
        let second = now_ms / 1_000;
        match self.buckets.back_mut() {
            Some((s, p, b)) if *s == second => {
                *p += 1;
                *b += bytes as u64;
            }
            _ => self.buckets.push_back((second, 1, bytes as u64)),
        }
        while self
            .buckets
            .front()
            .is_some_and(|(s, _, _)| second.saturating_sub(*s) >= FLOOD_LONG_S)
        {
            self.buckets.pop_front();
        }
    }

    /// Traffic no client of this build sends, worth this many points.
    pub fn noise(&mut self, now_ms: u64, points: u32) {
        self.noise.push_back((now_ms, points));
        self.expire_noise(now_ms);
    }

    fn expire_noise(&mut self, now_ms: u64) {
        while self
            .noise
            .front()
            .is_some_and(|(t, _)| now_ms.saturating_sub(*t) >= NOISE_WINDOW_MS)
        {
            self.noise.pop_front();
        }
    }

    /// Packets and bytes within the last `window_s` seconds.
    pub fn within(&self, now_ms: u64, window_s: u64) -> (u64, u64) {
        let second = now_ms / 1_000;
        self.buckets
            .iter()
            .filter(|(s, _, _)| second.saturating_sub(*s) < window_s)
            .fold((0, 0), |(p, b), (_, bp, bb)| (p + bp, b + bb))
    }

    /// Noise points within [`NOISE_WINDOW_MS`].
    pub fn points(&self, now_ms: u64) -> u32 {
        self.noise
            .iter()
            .filter(|(t, _)| now_ms.saturating_sub(*t) < NOISE_WINDOW_MS)
            .map(|(_, p)| *p)
            .sum()
    }

    /// Packets in the current second, for the member that yields first when
    /// the inbox is filling.
    pub fn this_second(&self, now_ms: u64) -> u64 {
        match self.buckets.back() {
            Some((s, p, _)) if *s == now_ms / 1_000 => *p,
            _ => 0,
        }
    }

    /// Whether this member is flooding the group, and by which limit.
    pub fn over(&self, now_ms: u64) -> Option<Flood> {
        let points = self.points(now_ms);
        if points >= NOISE_LIMIT {
            return Some(Flood::Noise { points });
        }
        let (packets, bytes) = self.within(now_ms, FLOOD_SHORT_S);
        if packets >= FLOOD_SHORT_PACKETS {
            return Some(Flood::Packets { window_s: FLOOD_SHORT_S, packets });
        }
        if bytes >= FLOOD_SHORT_BYTES {
            return Some(Flood::Bytes { window_s: FLOOD_SHORT_S, bytes });
        }
        let (packets, bytes) = self.within(now_ms, FLOOD_LONG_S);
        if packets >= FLOOD_LONG_PACKETS {
            return Some(Flood::Packets { window_s: FLOOD_LONG_S, packets });
        }
        if bytes >= FLOOD_LONG_BYTES {
            return Some(Flood::Bytes { window_s: FLOOD_LONG_S, bytes });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: u8) -> SigningKey {
        SigningKey::from_bytes(&[n; 32])
    }

    #[test]
    fn a_binding_names_its_own_member_in_its_own_group_and_nothing_else() {
        let chat = [7u8; 32];
        let member = [9u8; 32];
        let name = binding_for(&key(1), &chat, &member);
        assert_eq!(bound_to(&name, &chat, &member), Some(*key(1).verifying_key().as_bytes()));
        // Another member carrying it: a copy, not a binding of its own.
        assert_eq!(bound_to(&name, &chat, &[8u8; 32]), None);
        assert!(looks_like_a_binding(&name));
        // Another group.
        assert_eq!(bound_to(&name, &[6u8; 32], &member), None);
        // Another key claimed over the same signature.
        let mut forged = name;
        forged[4..36].copy_from_slice(key(2).verifying_key().as_bytes());
        assert_eq!(bound_to(&forged, &chat, &member), None);
        // A display name, and a binding cut short.
        assert_eq!(bound_to(b"player", &chat, &member), None);
        assert!(!looks_like_a_binding(b"player"));
        assert_eq!(bound_to(&name[..BINDING_LEN - 1], &chat, &member), None);
        // It fits the group's member name, whose limit is 128 bytes.
        const _FITS: () = assert!(BINDING_LEN <= 128);
    }

    #[test]
    fn a_member_is_counted_over_both_windows_and_forgotten_after_the_long_one() {
        let mut m = Meter::default();
        let t0 = 1_000_000;
        for i in 0..100 {
            m.packet(t0 + i, 500);
        }
        assert_eq!(m.within(t0 + 99, FLOOD_SHORT_S), (100, 50_000));
        assert_eq!(m.this_second(t0 + 99), 100);
        // Eleven seconds on: out of the short window, inside the long one.
        assert_eq!(m.within(t0 + 11_000, FLOOD_SHORT_S), (0, 0));
        assert_eq!(m.within(t0 + 11_000, FLOOD_LONG_S), (100, 50_000));
        m.packet(t0 + 61_000, 1);
        assert_eq!(m.within(t0 + 61_000, FLOOD_LONG_S), (1, 1));
        assert!(m.over(t0 + 61_000).is_none());
    }

    #[test]
    fn a_flood_is_over_a_limit_and_honest_traffic_is_not() {
        let t0 = 5_000_000;
        // An honest seat's heaviest minute, generously: a re-said burst of
        // sixty-four messages of three fragments every five seconds, and a
        // restarted seat's whole hand of two hundred and fifty on top.
        let mut honest = Meter::default();
        for burst in 0..12u64 {
            for i in 0..(64 * 3) {
                honest.packet(t0 + burst * 5_000 + i, 500);
            }
        }
        for i in 0..250 {
            honest.packet(t0 + 30_000 + i, 500);
        }
        assert_eq!(honest.over(t0 + 59_999), None);

        let mut fast = Meter::default();
        for i in 0..FLOOD_SHORT_PACKETS {
            fast.packet(t0 + i / 10, 60);
        }
        assert!(matches!(fast.over(t0 + 1_000), Some(Flood::Packets { window_s: FLOOD_SHORT_S, .. })));

        let mut heavy = Meter::default();
        for i in 0..2_200 {
            heavy.packet(t0 + i, 1_373);
        }
        assert!(matches!(heavy.over(t0 + 2_200), Some(Flood::Bytes { window_s: FLOOD_SHORT_S, .. })));

        // Slow and steady, under the short window's limits all along.
        let mut steady = Meter::default();
        let per_second = FLOOD_LONG_PACKETS / FLOOD_LONG_S + 1;
        for s in 0..FLOOD_LONG_S {
            for i in 0..per_second {
                steady.packet(t0 + s * 1_000 + i, 100);
            }
            assert!(!matches!(steady.over(t0 + s * 1_000 + 999), Some(Flood::Packets { window_s: FLOOD_SHORT_S, .. })));
        }
        assert!(matches!(steady.over(t0 + 59_999), Some(Flood::Packets { window_s: FLOOD_LONG_S, .. })));

        let mut noisy = Meter::default();
        for i in 0..NOISE_LIMIT {
            assert_eq!(noisy.over(t0 + u64::from(i)), None);
            noisy.noise(t0 + u64::from(i), NOISE_BAD_FRAGMENT);
        }
        assert_eq!(noisy.over(t0 + 100), Some(Flood::Noise { points: NOISE_LIMIT }));
        // Noise ages out.
        assert_eq!(noisy.over(t0 + NOISE_WINDOW_MS + 100), None);
    }
}
