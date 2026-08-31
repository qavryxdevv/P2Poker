//! Carrying this protocol's messages over a transport with a small MTU.
//!
//! # Why this exists
//!
//! D-019 moves a table's traffic onto Tox, and **every Tox channel caps at
//! about 1372 bytes** — custom lossless packets, custom lossy packets, group
//! messages and friend messages alike. File transfer is the only unbounded path
//! and it is a file transfer, not a message. So the choice was never *which*
//! Tox channel avoids fragmenting; it is that this protocol fragments.
//!
//! In this protocol's own sizes, at Tox's 1373-byte packet: a `SHUFFLE_STEP` is
//! about 9 KB and takes seven fragments, a `SHUFFLE_PROOF` five, a `cause = 2`
//! `HAND_ABORT` carrying both twelve, and the largest this client will ever
//! build one — `HAND_ABORT_MAX` — thirty-eight.
//!
//! A fragment count is **not** a byte cap, which is the whole point: a libp2p
//! relay circuit stops at 128 KiB and the hand dies with it; a Tox session
//! costs more packets and has no ceiling to hit.
//!
//! # Why it is here and not under `src/tox`
//!
//! Nothing in this module knows what Tox is. The MTU is a parameter, the
//! identity of a sender is a parameter, and what is carried is bytes. Putting
//! it behind `--features tox` would have meant its tests only run on a machine
//! with a C toolchain — and this is the one piece of the Tox work that is pure
//! logic, which is to say the one piece where a test is worth most.
//!
//! # The rule this module is written to
//!
//! **Everything here is fed by the network, so `SPEC_CS.md` §27 applies to all
//! of it.** A reassembler that trusts a sender's fragment count is a container
//! keyed on a quantity an attacker chooses, which is the shape this project has
//! already refused twice. Concretely:
//!
//! * the claimed total is checked against [`MAX_FRAGMENTS`](crate::table::fragment::MAX_FRAGMENTS) **before** anything
//!   is allocated;
//! * the number of part-built messages held for one sender is bounded, and the
//!   oldest is dropped rather than the newest refused;
//! * a stream that stops half way is swept on a timer instead of held for ever;
//! * a second copy of a fragment is idempotent, and a *different* second copy of
//!   one is a fault — a sender cannot rewrite a message it has begun.

use std::collections::BTreeMap;

/// The header every fragment carries, in bytes.
///
/// `message_id` (4) + `index` (2) + `total` (2). Fixed and unversioned: this
/// sits under the signed envelope, so a change here is a change of transport
/// framing and not of protocol, and both ends of one table run one build.
pub const HEADER: usize = 8;

/// Tox's own limit, and the smallest MTU this is expected to run over.
///
/// `TOX_GROUP_MAX_CUSTOM_LOSSLESS_PACKET_LENGTH`, from
/// `vendor/c-toxcore/toxcore/tox.h:3106`.
pub const TOX_PACKET: usize = 1373;

/// How much of one packet is message.
pub const fn payload_for(mtu: usize) -> usize {
    mtu - HEADER
}

/// The largest message this protocol will ever ask to be carried.
///
/// `HAND_ABORT` with two embedded events is the biggest thing on the wire, and
/// its own cap is the transport-honest one derived in `protocol::constants`.
/// Taking the number from there rather than choosing one here is what stops the
/// two drifting into a message that can be built and not sent.
pub const MAX_MESSAGE: usize = crate::protocol::constants::HAND_ABORT_MAX;

/// The most fragments any legal message can need, at the smallest MTU.
///
/// **This is the bound that matters.** It is derived, not chosen: a sender
/// claiming more than this is claiming a message larger than the protocol can
/// produce, and it is refused before a single byte is allocated for it.
pub const MAX_FRAGMENTS: usize = MAX_MESSAGE.div_ceil(payload_for(TOX_PACKET));

/// How many part-built messages one sender may have in flight.
///
/// Small on purpose. A peer legitimately has one message in the air at a time
/// per stage; two allows for a stage boundary crossing in flight, and four is
/// already generous. The cost of the bound being too tight is a retransmission;
/// the cost of it being absent is a sender choosing this client's memory.
pub const MAX_PARTIAL_PER_SENDER: usize = 4;

/// How long a part-built message waits for the rest of itself.
///
/// A lossless ordered channel delivers the remainder in milliseconds or not at
/// all — this is not a retransmission window, it is how long a corpse is kept.
pub const PARTIAL_TTL_MS: u64 = 30_000;

/// Why a fragment was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bad {
    /// Shorter than a header, so there is nothing to read.
    Runt,
    /// `total` is zero, or larger than any legal message needs.
    ///
    /// The value is reported because it is the one number an attacker picks and
    /// the one worth seeing in a log.
    ImpossibleTotal { total: u16 },
    /// `index >= total`.
    IndexOutOfRange { index: u16, total: u16 },
    /// Two fragments of one message disagree about how long it is.
    TotalChanged { was: u16, now: u16 },
    /// The same index arrived twice with different bytes.
    ///
    /// Not a duplicate — a duplicate is ordinary weather on any mesh and is
    /// accepted silently. This is a sender rewriting a message it has begun.
    Rewritten { index: u16 },
    /// A fragment longer than the MTU allows, which cannot have arrived over
    /// the transport it claims to have arrived over.
    Oversized { len: usize },
    /// The completed message is larger than the protocol can produce.
    ///
    /// Checked at completion as well as at the header, because `total` bounds
    /// the fragment count and the last fragment's length is what fixes the
    /// message's actual size.
    TooLong { len: usize },
}

impl std::fmt::Display for Bad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Runt => write!(f, "a fragment shorter than its own header"),
            Self::ImpossibleTotal { total } => {
                write!(f, "a message claiming {total} fragments, which no legal message needs")
            }
            Self::IndexOutOfRange { index, total } => {
                write!(f, "fragment {index} of {total}")
            }
            Self::TotalChanged { was, now } => {
                write!(f, "the message was {was} fragments and is now {now}")
            }
            Self::Rewritten { index } => {
                write!(f, "fragment {index} arrived twice with different bytes")
            }
            Self::Oversized { len } => write!(f, "a {len}-byte fragment over the MTU"),
            Self::TooLong { len } => write!(f, "a {len}-byte message, over what the protocol builds"),
        }
    }
}

/// Cut a message into fragments for a transport of this MTU.
///
/// The `id` is the sender's own counter and only has to be unique among that
/// sender's messages currently in flight; it is never compared across senders.
pub fn split(message: &[u8], id: u32, mtu: usize) -> Result<Vec<Vec<u8>>, Bad> {
    if message.len() > MAX_MESSAGE {
        return Err(Bad::TooLong { len: message.len() });
    }
    let payload = payload_for(mtu);
    // A zero-length message is one fragment carrying nothing, not zero
    // fragments: `total == 0` is refused at the far end, and a message that
    // vanished into no packets is a message nobody can tell from one that was
    // never sent.
    let total = message.len().div_ceil(payload).max(1);
    debug_assert!(total <= MAX_FRAGMENTS || mtu < TOX_PACKET);

    // `chunks` yields nothing for an empty slice, which is the one input that
    // would otherwise produce zero fragments - and `total == 0` is refused at
    // the far end, so an empty message would have been unsendable rather than
    // empty. One empty chunk is the whole of the special case.
    let chunks: Vec<&[u8]> = if message.is_empty() {
        vec![&[]]
    } else {
        message.chunks(payload).collect()
    };
    debug_assert_eq!(chunks.len(), total);

    let mut out = Vec::with_capacity(total);
    for (i, chunk) in chunks.into_iter().enumerate() {
        let mut frame = Vec::with_capacity(HEADER + chunk.len());
        frame.extend_from_slice(&id.to_be_bytes());
        frame.extend_from_slice(&(i as u16).to_be_bytes());
        frame.extend_from_slice(&(total as u16).to_be_bytes());
        frame.extend_from_slice(chunk);
        out.push(frame);
    }
    Ok(out)
}

/// One message being put back together.
struct Partial {
    total: u16,
    /// One slot per fragment, filled as they arrive. The slots cost
    /// `total * size_of::<Option<Vec<u8>>>()` — under a kilobyte at
    /// [`MAX_FRAGMENTS`] — so the allocation is bounded by the *count* and the
    /// bytes grow only with what actually arrived.
    parts: Vec<Option<Vec<u8>>>,
    have: usize,
    /// When the first fragment arrived, for the sweep.
    began_ms: u64,
}

/// Puts fragments back into messages, one accumulator per sender.
///
/// `S` is whatever the transport calls a sender — a Tox group peer id, a public
/// key, a test's `u8`. Nothing here interprets it, and in particular **nothing
/// here treats it as authority**: who signed a message is decided by the
/// signature inside it, after reassembly, exactly as `table::transport` says.
pub struct Reassembler<S: Ord + Clone> {
    mtu: usize,
    senders: BTreeMap<S, BTreeMap<u32, Partial>>,
}

impl<S: Ord + Clone> Reassembler<S> {
    /// A reassembler for a transport of this MTU.
    pub fn new(mtu: usize) -> Self {
        assert!(mtu > HEADER, "an MTU with no room for a header carries nothing");
        Self {
            mtu,
            senders: BTreeMap::new(),
        }
    }

    /// Take one fragment. `Ok(Some(_))` when it completed a message.
    pub fn accept(&mut self, from: &S, frame: &[u8], now_ms: u64) -> Result<Option<Vec<u8>>, Bad> {
        if frame.len() < HEADER {
            return Err(Bad::Runt);
        }
        if frame.len() > self.mtu {
            return Err(Bad::Oversized { len: frame.len() });
        }
        let id = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]);
        let index = u16::from_be_bytes([frame[4], frame[5]]);
        let total = u16::from_be_bytes([frame[6], frame[7]]);
        let body = &frame[HEADER..];

        // **Before any allocation.** `total` is the one field a sender chooses
        // that decides how much this client sets aside.
        if total == 0 || usize::from(total) > MAX_FRAGMENTS {
            return Err(Bad::ImpossibleTotal { total });
        }
        if index >= total {
            return Err(Bad::IndexOutOfRange { index, total });
        }

        let by_id = self.senders.entry(from.clone()).or_default();

        // The single-fragment case never enters the store at all. It is the
        // common one - most of this protocol's messages fit in a packet - and
        // an accumulator for it would be an entry created and removed for every
        // action, every vote and every certificate.
        if total == 1 && !by_id.contains_key(&id) {
            if by_id.is_empty() {
                self.senders.remove(from);
            }
            return finish(vec![body.to_vec()]);
        }

        if !by_id.contains_key(&id) && by_id.len() >= MAX_PARTIAL_PER_SENDER {
            // **The oldest goes, not this one.** Refusing the newest lets a
            // sender that once filled the table wedge itself out of it for
            // ever; dropping the oldest means a stalled message costs its own
            // slot and nothing else's.
            if let Some(&oldest) = by_id
                .iter()
                .min_by_key(|(_, p)| p.began_ms)
                .map(|(k, _)| k)
            {
                by_id.remove(&oldest);
            }
        }

        let partial = by_id.entry(id).or_insert_with(|| Partial {
            total,
            parts: vec![None; usize::from(total)],
            have: 0,
            began_ms: now_ms,
        });
        if partial.total != total {
            let was = partial.total;
            by_id.remove(&id);
            return Err(Bad::TotalChanged { was, now: total });
        }

        let slot = &mut partial.parts[usize::from(index)];
        match slot {
            // Ordinary weather: a mesh redelivers.
            Some(had) if had == body => return Ok(None),
            Some(_) => {
                by_id.remove(&id);
                return Err(Bad::Rewritten { index });
            }
            None => {
                *slot = Some(body.to_vec());
                partial.have += 1;
            }
        }

        if partial.have < usize::from(partial.total) {
            return Ok(None);
        }
        let done = by_id.remove(&id).expect("just written");
        if by_id.is_empty() {
            self.senders.remove(from);
        }
        finish(done.parts.into_iter().map(|p| p.expect("all present")).collect())
    }

    /// Drop part-built messages that have waited too long.
    ///
    /// Called on the transport's own tick. Without it a sender that opens a
    /// message and stops holds a slot until it reconnects, and four such
    /// senders hold every slot they are allowed.
    pub fn sweep(&mut self, now_ms: u64) {
        self.senders.retain(|_, by_id| {
            by_id.retain(|_, p| now_ms.saturating_sub(p.began_ms) < PARTIAL_TTL_MS);
            !by_id.is_empty()
        });
    }

    /// How many part-built messages are held, over every sender. For tests and
    /// for a status line; nothing decides anything on it.
    pub fn outstanding(&self) -> usize {
        self.senders.values().map(BTreeMap::len).sum()
    }
}

/// Join the parts, refusing a message the protocol could not have built.
///
/// The count was bounded on arrival; this is the byte bound, and it is separate
/// because the last fragment's length is what fixes the size. A sender can send
/// `MAX_FRAGMENTS` full packets and still be inside the count while exceeding
/// what any legal message weighs.
fn finish(parts: Vec<Vec<u8>>) -> Result<Option<Vec<u8>>, Bad> {
    let len: usize = parts.iter().map(Vec::len).sum();
    if len > MAX_MESSAGE {
        return Err(Bad::TooLong { len });
    }
    let mut out = Vec::with_capacity(len);
    for p in parts {
        out.extend_from_slice(&p);
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_700_000_000_000;

    fn message(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i % 251) as u8).collect()
    }

    /// The numbers this module exists for, checked rather than asserted in
    /// prose: the protocol's own message sizes against Tox's packet.
    #[test]
    fn the_fragment_counts_are_what_the_sizes_say() {
        let p = payload_for(TOX_PACKET);
        assert_eq!(p, 1365);
        // A SHUFFLE_STEP is about 9 KB, a SHUFFLE_PROOF about 5.6 KB.
        assert_eq!(9_000usize.div_ceil(p), 7);
        assert_eq!(5_600usize.div_ceil(p), 5);
        // And the largest HAND_ABORT this client will build.
        assert_eq!(MAX_FRAGMENTS, MAX_MESSAGE.div_ceil(p));
        assert!(
            MAX_FRAGMENTS < usize::from(u16::MAX),
            "the count must fit the header field it travels in"
        );
    }

    /// A round trip at every size that changes the shape: empty, one byte
    /// short of a packet, exactly a packet, one over, and the largest legal
    /// message.
    #[test]
    fn every_size_survives_the_round_trip() {
        let p = payload_for(TOX_PACKET);
        for n in [0, 1, p - 1, p, p + 1, 2 * p, 9_000, MAX_MESSAGE] {
            let msg = message(n);
            let frames = split(&msg, 7, TOX_PACKET).expect("it fits");
            assert!(
                frames.iter().all(|f| f.len() <= TOX_PACKET),
                "{n}: a fragment over the MTU"
            );
            let mut r = Reassembler::new(TOX_PACKET);
            let mut got = None;
            for f in &frames {
                if let Some(done) = r.accept(&1u8, f, NOW).expect("a legal fragment") {
                    got = Some(done);
                }
            }
            assert_eq!(got.as_deref(), Some(msg.as_slice()), "at {n} bytes");
            assert_eq!(r.outstanding(), 0, "at {n} bytes: nothing left behind");
        }
    }

    /// Fragments out of order, which a transport may deliver even when it is
    /// ordered per sender: two messages interleave.
    #[test]
    fn two_messages_from_one_sender_interleave() {
        let a = message(5_000);
        let b = message(3_000);
        let fa = split(&a, 1, TOX_PACKET).unwrap();
        let fb = split(&b, 2, TOX_PACKET).unwrap();
        let mut r = Reassembler::new(TOX_PACKET);
        let mut done_a = None;
        let mut done_b = None;

        for i in 0..fa.len().max(fb.len()) {
            if let Some(f) = fa.get(i) {
                if let Some(m) = r.accept(&1u8, f, NOW).unwrap() {
                    done_a = Some(m);
                }
            }
            if let Some(f) = fb.get(i) {
                if let Some(m) = r.accept(&1u8, f, NOW).unwrap() {
                    done_b = Some(m);
                }
            }
        }
        assert_eq!(done_a.as_deref(), Some(a.as_slice()));
        assert_eq!(done_b.as_deref(), Some(b.as_slice()));
        assert_eq!(r.outstanding(), 0);
    }

    /// **The bound that matters.** A sender claiming more fragments than any
    /// legal message needs is refused before anything is allocated for it.
    #[test]
    fn an_impossible_fragment_count_allocates_nothing() {
        let mut r = Reassembler::new(TOX_PACKET);
        let mut frame = Vec::new();
        frame.extend_from_slice(&1u32.to_be_bytes());
        frame.extend_from_slice(&0u16.to_be_bytes());
        frame.extend_from_slice(&u16::MAX.to_be_bytes());
        frame.push(0);
        assert_eq!(
            r.accept(&1u8, &frame, NOW),
            Err(Bad::ImpossibleTotal { total: u16::MAX })
        );
        assert_eq!(r.outstanding(), 0, "nothing was set aside for it");

        // And zero, which would otherwise make an empty accumulator that never
        // completes.
        let mut zero = frame.clone();
        zero[6..8].copy_from_slice(&0u16.to_be_bytes());
        assert_eq!(
            r.accept(&1u8, &zero, NOW),
            Err(Bad::ImpossibleTotal { total: 0 })
        );
        assert_eq!(r.outstanding(), 0);
    }

    /// A duplicate is weather; a rewrite is a fault, and it takes the whole
    /// part-built message with it.
    #[test]
    fn a_duplicate_is_free_and_a_rewrite_is_a_fault() {
        let msg = message(4_000);
        let frames = split(&msg, 1, TOX_PACKET).unwrap();
        let mut r = Reassembler::new(TOX_PACKET);

        assert_eq!(r.accept(&1u8, &frames[0], NOW), Ok(None));
        assert_eq!(r.accept(&1u8, &frames[0], NOW), Ok(None), "a duplicate");
        assert_eq!(r.outstanding(), 1);

        let mut rewritten = frames[0].clone();
        let n = rewritten.len();
        rewritten[n - 1] ^= 0xff;
        assert_eq!(
            r.accept(&1u8, &rewritten, NOW),
            Err(Bad::Rewritten { index: 0 })
        );
        assert_eq!(
            r.outstanding(),
            0,
            "a sender that rewrites a message loses the message"
        );
    }

    /// Two fragments that disagree about the length of their own message.
    #[test]
    fn a_changed_total_is_refused_and_drops_the_message() {
        let msg = message(4_000);
        let frames = split(&msg, 1, TOX_PACKET).unwrap();
        let mut r = Reassembler::new(TOX_PACKET);
        r.accept(&1u8, &frames[0], NOW).unwrap();

        let mut lying = frames[1].clone();
        lying[6..8].copy_from_slice(&9u16.to_be_bytes());
        assert_eq!(
            r.accept(&1u8, &lying, NOW),
            Err(Bad::TotalChanged { was: 3, now: 9 })
        );
        assert_eq!(r.outstanding(), 0);
    }

    /// **The oldest goes, not the newest.** A sender that once filled its slots
    /// must not be able to wedge itself out of the transport.
    #[test]
    fn the_oldest_partial_is_the_one_dropped() {
        let mut r = Reassembler::new(TOX_PACKET);
        let msg = message(4_000);
        // Five messages opened, one more than the bound, each a millisecond
        // apart so "oldest" is defined.
        for id in 0..5u32 {
            let frames = split(&msg, id, TOX_PACKET).unwrap();
            r.accept(&1u8, &frames[0], NOW + u64::from(id)).unwrap();
        }
        assert_eq!(r.outstanding(), MAX_PARTIAL_PER_SENDER);

        // The newest is still there and still completes.
        let frames = split(&msg, 4, TOX_PACKET).unwrap();
        let mut done = None;
        for f in &frames[1..] {
            if let Some(m) = r.accept(&1u8, f, NOW + 4).unwrap() {
                done = Some(m);
            }
        }
        assert_eq!(done.as_deref(), Some(msg.as_slice()));
    }

    /// Two senders do not share a budget, and do not share message ids.
    #[test]
    fn senders_are_kept_apart() {
        let a = message(4_000);
        let b = message(2_000);
        let fa = split(&a, 1, TOX_PACKET).unwrap();
        let fb = split(&b, 1, TOX_PACKET).unwrap();
        let mut r = Reassembler::new(TOX_PACKET);

        // The same message id from two senders must not be one message.
        r.accept(&1u8, &fa[0], NOW).unwrap();
        r.accept(&2u8, &fb[0], NOW).unwrap();
        assert_eq!(r.outstanding(), 2);

        let mut done_b = None;
        for f in &fb[1..] {
            if let Some(m) = r.accept(&2u8, f, NOW).unwrap() {
                done_b = Some(m);
            }
        }
        assert_eq!(done_b.as_deref(), Some(b.as_slice()), "and it is B's message");
    }

    /// A message that stops half way is swept, not held.
    #[test]
    fn a_stalled_message_is_swept() {
        let msg = message(4_000);
        let frames = split(&msg, 1, TOX_PACKET).unwrap();
        let mut r = Reassembler::new(TOX_PACKET);
        r.accept(&1u8, &frames[0], NOW).unwrap();
        assert_eq!(r.outstanding(), 1);

        r.sweep(NOW + PARTIAL_TTL_MS - 1);
        assert_eq!(r.outstanding(), 1, "not yet");
        r.sweep(NOW + PARTIAL_TTL_MS);
        assert_eq!(r.outstanding(), 0);
    }

    /// A fragment longer than the transport can carry did not arrive over it.
    #[test]
    fn an_oversized_fragment_is_refused() {
        let mut r = Reassembler::new(TOX_PACKET);
        let frame = vec![0u8; TOX_PACKET + 1];
        assert_eq!(
            r.accept(&1u8, &frame, NOW),
            Err(Bad::Oversized { len: TOX_PACKET + 1 })
        );
        assert_eq!(r.accept(&1u8, &[0u8; 3], NOW), Err(Bad::Runt));
    }

    /// The byte bound is separate from the count bound, and it is the one that
    /// catches a legal fragment count carrying an illegal message.
    #[test]
    fn a_message_over_the_protocols_own_cap_is_refused() {
        assert_eq!(
            split(&message(MAX_MESSAGE + 1), 1, TOX_PACKET),
            Err(Bad::TooLong {
                len: MAX_MESSAGE + 1
            })
        );
    }
}
