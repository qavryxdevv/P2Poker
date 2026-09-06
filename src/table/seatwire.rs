//! `PLAYER_SIT_OUT`, `PLAYER_SIT_IN` and `PLAYER_LEAVE` on the wire, and the
//! sequence band they occupy (`PROTOCOL.md` §4.10's hand boundary window).
//!
//! The three bodies and the envelope arithmetic, and nothing else: what a
//! receiver does with one is [`crate::table::boundary`]'s, because the window
//! outlives the `Hand` it belongs to. This module is the encoding and the
//! reasons for it, exactly as [`crate::table::checkwire`] is for the checkpoint.
//!
//! # The window, in one paragraph
//!
//! A boundary event belongs to **chain `k`**, the hand that has just ended, and
//! carries `hand_id = k`. Its `sequence` is `BOUNDARY_SEQUENCE_BASE + seat` —
//! one reserved slot per seat and no other — and its parent is `TERMINAL(k)`
//! for every event of the window whichever seats emit and in whatever order
//! they arrive. The window is therefore a **fan and not a chain**: a seat signs
//! without knowing which other seats will emit, so a running parent is not
//! computable by the emitter and a reserved `sequence` is what replaces it.
//!
//! # Why the band is checked here and the position is not checked by `open`
//!
//! `chained::open_in_hand` relaxes both positional comparisons, because the
//! only callers that need it are the ones with a clause saying so — and this is
//! the **second** of §4.0 step 10a's two exemptions, after the terminal
//! `HAND_ABORT`'s. Relaxed does not mean unchecked: what §4.10 replaces the
//! cursor with is an exact arithmetic identity, `sequence == BOUNDARY_SEQUENCE_BASE
//! + sender_seat`, and an exact parent, `TERMINAL(k)`. Both are stricter than a
//! cursor comparison, not looser, and a receiver that skipped them would accept
//! two events from one seat at two sequences — which is the defect class §5.2
//! closed for `DISPUTE` and §4.8 closed for `TIMEOUT_VOTE`.
//!
//! # The band is disjoint from everything else, by assertion
//!
//! `MAX_STAGES_PER_HAND = 2 048` bounds an ordinary stage index at 2 047; this
//! window is `4 096 … 4 105`; §4.9's checkpoint takes `8 192 … 8 207` and
//! extends **upwards** through its reconciliation rounds. `constants.rs` asserts
//! the three do not meet, at compile time, and [`in_the_window`] is the only
//! predicate in this tree that names the middle band.

use minicbor::{Decode, Encode};

use crate::poker::state::SeatIdx;
use crate::protocol::constants::{BOUNDARY_SEQUENCE_BASE, MAX_SEATS};
use crate::protocol::messages::EventType;

/// `0x0803 PLAYER_SIT_OUT` — §4.10's one field.
///
/// `1` voluntary, `2` derived from consecutive auto-actions. The derived case
/// needs no message at all — after `MAX_CONSECUTIVE_AUTO_ACTIONS` every peer
/// marks the seat sitting out identically (D-006 point 4) — so this message
/// exists for the voluntary case and as an explicit record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct SitOut {
    #[n(0)]
    pub reason: u16,
}

/// `0x0805 PLAYER_LEAVE` — §4.10's one field.
///
/// `1` voluntary, `2` client shutting down. **A leave is never required**: a
/// client that vanishes must be handled identically, because a departing peer
/// announcing anything can never be a precondition [NAT §7.3].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct Leave {
    #[n(0)]
    pub reason: u16,
}

/// `0x0804 PLAYER_SIT_IN` has **no fields beyond the envelope** (§4.10), so
/// there is no struct for it and this constant is the whole of its payload.
///
/// An empty CBOR array rather than an absent payload: `EventBody::payload` is
/// bytes and every other chained type puts a canonical value there, so a type
/// with nothing to say still says nothing canonically.
pub const SIT_IN_PAYLOAD: [u8; 1] = [0x80];

/// The cap for any of the three payloads.
///
/// The largest is a one-element array holding a `u16`, which is four bytes
/// canonically. This is a bound and not a size, the way every cap in this tree
/// is: `SPEC_CS.md` §17 wants a limit on everything that arrives from the
/// network, and a body this small still arrives from the network.
pub const BOUNDARY_EVENT_CAP: usize = 64;

/// Voluntary, for either of the two types that carry a reason.
pub const REASON_VOLUNTARY: u16 = 1;
/// `PLAYER_SIT_OUT`: derived from consecutive auto-actions.
pub const REASON_AUTO_ACTIONS: u16 = 2;
/// `PLAYER_LEAVE`: the client is shutting down.
pub const REASON_SHUTTING_DOWN: u16 = 2;

/// Where seat `seat`'s boundary event sits, and nowhere else.
///
/// `None` for a seat index outside the table's maximum, so a caller cannot
/// silently address a slot the band does not reserve.
pub fn window_sequence(seat: SeatIdx) -> Option<u64> {
    if seat >= MAX_SEATS {
        return None;
    }
    Some(BOUNDARY_SEQUENCE_BASE + u64::from(seat))
}

/// The seat a window `sequence` belongs to, or `None` if it is not in the band.
///
/// The inverse of [`window_sequence`] and it must stay the inverse: a receiver
/// reads the seat off the **sender's key** and compares, rather than believing
/// this, which is why the identity is checked in one direction only at the one
/// site that matters.
pub fn seat_of(sequence: u64) -> Option<SeatIdx> {
    let offset = sequence.checked_sub(BOUNDARY_SEQUENCE_BASE)?;
    let seat = SeatIdx::try_from(offset).ok()?;
    if seat >= MAX_SEATS {
        return None;
    }
    Some(seat)
}

/// Whether a `sequence` falls in the hand boundary window's band.
///
/// **Used to refuse, as well as to admit.** §4.10: a receiver *"rejects any
/// other chained type at a `sequence >= BOUNDARY_SEQUENCE_BASE`"* — the band is
/// the three types' and nothing else may occupy it.
pub fn in_the_window(sequence: u64) -> bool {
    seat_of(sequence).is_some()
}

/// Whether this type belongs in the window at all.
///
/// A thin wrapper over the catalogue's own predicate, kept so that every
/// window rule in this tree reads from one module.
pub fn is_a_boundary_event(kind: EventType) -> bool {
    kind.is_boundary()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::constants::{BOUNDARY_CHECKPOINT_BASE, MAX_STAGES_PER_HAND};

    #[test]
    fn every_seat_has_one_slot_and_the_map_is_a_bijection() {
        let mut seen = std::collections::BTreeSet::new();
        for seat in 0..MAX_SEATS {
            let s = window_sequence(seat).expect("a seat of the table has a slot");
            assert!(seen.insert(s), "seat {seat} shares a slot");
            assert_eq!(seat_of(s), Some(seat), "the inverse is not the inverse");
            assert!(in_the_window(s));
        }
        assert_eq!(window_sequence(MAX_SEATS), None, "a seat off the table has no slot");
    }

    #[test]
    fn the_band_meets_neither_neighbour() {
        // Below: an ordinary stage index. `MAX_STAGES_PER_HAND` bounds it, and
        // the exemption §4.10 grants the window is only meaningful if the two
        // ranges are disjoint in the first place.
        assert!(!in_the_window(MAX_STAGES_PER_HAND - 1));
        assert!(!in_the_window(0));
        // Above: §4.9's checkpoint band, which extends upwards through its
        // reconciliation rounds and must never walk down into these slots.
        assert!(!in_the_window(BOUNDARY_CHECKPOINT_BASE));
        assert!(!in_the_window(BOUNDARY_SEQUENCE_BASE + u64::from(MAX_SEATS)));
    }

    #[test]
    fn a_sequence_below_the_base_is_not_in_the_window() {
        // `checked_sub`, not a wrapping one: the arithmetic that reads a seat
        // out of a sequence is fed by the wire.
        assert_eq!(seat_of(BOUNDARY_SEQUENCE_BASE - 1), None);
        assert_eq!(seat_of(0), None);
        assert_eq!(seat_of(u64::MAX), None);
    }

    #[test]
    fn the_three_types_are_the_window_and_nothing_else_is() {
        for kind in EventType::ALL {
            assert_eq!(
                is_a_boundary_event(kind),
                matches!(
                    kind,
                    EventType::PlayerSitOut | EventType::PlayerSitIn | EventType::PlayerLeave
                ),
                "{} is classified inconsistently",
                kind.name()
            );
        }
    }

    #[test]
    fn a_reason_round_trips_under_its_own_cap() {
        let body = SitOut {
            reason: REASON_VOLUNTARY,
        };
        let bytes = crate::protocol::serialization::to_canonical(&body).expect("encodes");
        assert!(
            bytes.len() <= BOUNDARY_EVENT_CAP,
            "a boundary payload is {} bytes, above its own cap",
            bytes.len()
        );
        let back: SitOut =
            crate::protocol::serialization::from_canonical(&bytes, BOUNDARY_EVENT_CAP).expect("decodes");
        assert_eq!(back, body);
    }

    #[test]
    fn sit_in_carries_a_canonical_nothing() {
        // The constant is the encoding of the empty array and not a guess at
        // it: if the codec ever encodes one differently, this fails here rather
        // than on the wire.
        let empty: Vec<u16> = Vec::new();
        let bytes = crate::protocol::serialization::to_canonical(&empty).expect("encodes");
        assert_eq!(bytes, SIT_IN_PAYLOAD.to_vec());
    }
}
