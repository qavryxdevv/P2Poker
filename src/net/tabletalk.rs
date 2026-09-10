//! What the seats of one table say to each other.
//!
//! `S1-CS`, `PROTOCOL.md` §7.8. The lobby's chat (`lobbytalk`) reaches whoever
//! is on the lobby's topic; this one reaches the seats of one table, on the
//! table's own carriers -- its GossipSub topic and its group, the same two a
//! hand event rides -- and is accepted only from a key that holds a seat in
//! the receiver's roster for the table the line names.
//!
//! Like the lobby's, it is a line of text and nothing more: not stored, not
//! replayed, not evidence of anything. A name is decoration; the key is the
//! identity, and the seat it holds is what the window shows the line under.
//! Muting a seat is the receiver's own affair and never reaches the wire.

use std::collections::BTreeMap;

use ed25519_dalek::SigningKey;

use crate::protocol::constants::{LOBBY_CHAT_MAX, LOBBY_MSG_MAX};
use crate::protocol::messages::{EventBody, EventType, SignedEvent};
use crate::protocol::serialization::{from_canonical, to_canonical};
use crate::protocol::signatures::to_be_signed;
use crate::storage::settings::NAME_MAX;

pub use super::lobbytalk::{CLOCK_SLACK_MS, SAID_MAX};

/// How often one seat may be heard: one line per this many milliseconds.
pub const LINE_EVERY_MS: u64 = 2_000;

/// A line, once it has been checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    pub seat: u8,
    pub who: [u8; 32],
    pub nickname: String,
    pub text: String,
}

/// Why a line was not accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotHeard {
    Malformed(&'static str),
    Forged,
    /// The key that signed it holds no seat at this table.
    NotASeat,
    /// A line said at another table.
    AnotherTable,
    /// This seat has said too much, too fast.
    TooMuch,
    TooLong(&'static str),
    Stale,
}

/// One line per two seconds per seat, charged before the signature.
#[derive(Debug, Clone, Default)]
pub struct SeatLimiter {
    last: BTreeMap<u8, u64>,
}

impl SeatLimiter {
    pub fn admit(&mut self, seat: u8, now_ms: u64) -> bool {
        match self.last.get(&seat) {
            Some(at) if now_ms.saturating_sub(*at) < LINE_EVERY_MS => false,
            _ => {
                self.last.insert(seat, now_ms);
                true
            }
        }
    }
}

#[derive(Debug, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
struct Body {
    #[n(0)]
    nickname: String,
    #[n(1)]
    text: String,
    /// The table the line is said at, so a line cannot be carried to another.
    #[cbor(n(2), with = "minicbor::bytes")]
    table_id: [u8; 32],
}

/// Cut a string to a byte cap without splitting a character.
fn clip(s: &str, cap: usize) -> String {
    let mut out = s.trim().to_owned();
    while out.len() > cap {
        out.pop();
    }
    out
}

/// The line as it will be said: trimmed and capped the way `say` caps it.
pub fn clip_line(text: &str) -> String {
    clip(text, SAID_MAX)
}

/// Say something to the table.
pub fn say(
    key: &SigningKey,
    table_id: &[u8; 32],
    nickname: &str,
    text: &str,
    now_ms: u64,
) -> Result<Vec<u8>, &'static str> {
    if text.trim().is_empty() {
        return Err("nothing to say");
    }
    let body = Body {
        nickname: clip(nickname, NAME_MAX),
        text: clip(text, SAID_MAX),
        table_id: *table_id,
    };
    let body_bytes = to_canonical(&body).map_err(|_| "the body does not encode")?;
    if body_bytes.len() > LOBBY_CHAT_MAX {
        return Err("over the cap");
    }
    let envelope = EventBody::unchained(
        EventType::TableChat,
        key.verifying_key().to_bytes(),
        body_bytes,
        now_ms,
    )
    .ok_or("table talk is an unchained event")?;
    let envelope_bytes = to_canonical(&envelope).map_err(|_| "the envelope does not encode")?;
    let signature = {
        use ed25519_dalek::Signer;
        key.sign(&to_be_signed(&envelope_bytes))
    };
    to_canonical(&SignedEvent {
        body: envelope_bytes,
        signature: signature.to_bytes(),
    })
    .map_err(|_| "the signed event does not encode")
}

/// Take a line off the wire, for the table `table_id`, where `seat_of` says
/// which seat a key holds.
///
/// The seat's budget is charged before the signature is verified, which is
/// the expensive part, and after the key has been placed in the roster --
/// a stranger has no budget to spend.
pub fn receive(
    bytes: &[u8],
    table_id: &[u8; 32],
    seat_of: impl Fn(&[u8; 32]) -> Option<u8>,
    now_ms: u64,
    limits: &mut SeatLimiter,
) -> Result<Said, NotHeard> {
    if bytes.len() > LOBBY_MSG_MAX {
        return Err(NotHeard::TooLong("the message is over the cap"));
    }
    let signed: SignedEvent =
        from_canonical(bytes, LOBBY_MSG_MAX).map_err(|_| NotHeard::Malformed("not a signed event"))?;
    let envelope: EventBody = from_canonical(&signed.body, LOBBY_MSG_MAX)
        .map_err(|_| NotHeard::Malformed("not an envelope"))?;
    let kind = EventType::try_from(envelope.event_type)
        .map_err(|_| NotHeard::Malformed("an event type this client does not know"))?;
    if kind != EventType::TableChat {
        return Err(NotHeard::Malformed("not table talk"));
    }
    if envelope.emitted_at_unix_ms.abs_diff(now_ms) > CLOCK_SLACK_MS {
        return Err(NotHeard::Stale);
    }
    let who = envelope.sender_public_key;
    let seat = seat_of(&who).ok_or(NotHeard::NotASeat)?;
    if !limits.admit(seat, now_ms) {
        return Err(NotHeard::TooMuch);
    }
    verify(&who, &signed).map_err(|_| NotHeard::Forged)?;

    let body: Body = from_canonical(&envelope.payload, LOBBY_CHAT_MAX)
        .map_err(|_| NotHeard::Malformed("not a table line"))?;
    if &body.table_id != table_id {
        return Err(NotHeard::AnotherTable);
    }
    if body.nickname.len() > NAME_MAX {
        return Err(NotHeard::TooLong("the name is over its cap"));
    }
    if body.text.len() > SAID_MAX {
        return Err(NotHeard::TooLong("the line is over its cap"));
    }
    if body.text.trim().is_empty() {
        return Err(NotHeard::Malformed("an empty line is not a message"));
    }
    Ok(Said {
        seat,
        who,
        nickname: body.nickname,
        text: body.text,
    })
}

fn verify(who: &[u8; 32], signed: &SignedEvent) -> Result<(), ()> {
    use ed25519_dalek::{Signature, VerifyingKey};
    let key = VerifyingKey::from_bytes(who).map_err(|_| ())?;
    let sig = Signature::from_bytes(&signed.signature);
    key.verify_strict(&to_be_signed(&signed.body), &sig)
        .map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    const NOW: u64 = 1_700_000_000_000;
    const TABLE: [u8; 32] = [7u8; 32];

    /// Seat 2 is key 2, seat 5 is key 5, nobody else is seated.
    fn seat_of(k: &[u8; 32]) -> Option<u8> {
        [(2u8, key(2)), (5u8, key(5))]
            .iter()
            .find(|(_, s)| &s.verifying_key().to_bytes() == k)
            .map(|(seat, _)| *seat)
    }

    #[test]
    fn a_line_round_trips_under_its_seat() {
        let bytes = say(&key(2), &TABLE, "Bob", "nice hand", NOW).unwrap();
        let mut limits = SeatLimiter::default();
        let said = receive(&bytes, &TABLE, seat_of, NOW, &mut limits).unwrap();
        assert_eq!(said.seat, 2);
        assert_eq!(said.who, key(2).verifying_key().to_bytes());
        assert_eq!(said.nickname, "Bob");
        assert_eq!(said.text, "nice hand");
    }

    /// A key with no seat has no voice at the table, whatever name it types.
    #[test]
    fn a_stranger_is_not_heard() {
        let bytes = say(&key(9), &TABLE, "Bob", "let me in", NOW).unwrap();
        let mut limits = SeatLimiter::default();
        assert_eq!(receive(&bytes, &TABLE, seat_of, NOW, &mut limits), Err(NotHeard::NotASeat));
    }

    /// A line said at one table is not a line at another, however the bytes
    /// got there.
    #[test]
    fn a_line_from_another_table_is_dropped() {
        let bytes = say(&key(2), &[8u8; 32], "Bob", "wrong room", NOW).unwrap();
        let mut limits = SeatLimiter::default();
        assert_eq!(receive(&bytes, &TABLE, seat_of, NOW, &mut limits), Err(NotHeard::AnotherTable));
    }

    /// The signature covers the envelope, so a byte changed under it fails.
    #[test]
    fn a_tampered_line_is_refused() {
        let bytes = say(&key(5), &TABLE, "Erin", "hello", NOW).unwrap();
        let mut broken = bytes.clone();
        let at = broken.len() / 2;
        broken[at] ^= 0xff;
        let mut limits = SeatLimiter::default();
        assert!(receive(&broken, &TABLE, seat_of, NOW, &mut limits).is_err());
    }

    /// One line per two seconds per seat; the other seat's budget is its own.
    #[test]
    fn a_seat_is_heard_once_in_two_seconds() {
        let mut limits = SeatLimiter::default();
        let first = say(&key(2), &TABLE, "Bob", "one", NOW).unwrap();
        let second = say(&key(2), &TABLE, "Bob", "two", NOW + 500).unwrap();
        let other = say(&key(5), &TABLE, "Erin", "me too", NOW + 600).unwrap();
        let third = say(&key(2), &TABLE, "Bob", "three", NOW + LINE_EVERY_MS).unwrap();
        assert!(receive(&first, &TABLE, seat_of, NOW, &mut limits).is_ok());
        assert_eq!(receive(&second, &TABLE, seat_of, NOW + 500, &mut limits), Err(NotHeard::TooMuch));
        assert!(receive(&other, &TABLE, seat_of, NOW + 600, &mut limits).is_ok());
        assert!(receive(&third, &TABLE, seat_of, NOW + LINE_EVERY_MS, &mut limits).is_ok());
    }

    #[test]
    fn a_stale_line_and_an_empty_one_are_refused() {
        let mut limits = SeatLimiter::default();
        let old = say(&key(2), &TABLE, "Bob", "from long ago", NOW - CLOCK_SLACK_MS * 2).unwrap();
        assert_eq!(receive(&old, &TABLE, seat_of, NOW, &mut limits), Err(NotHeard::Stale));
        assert!(say(&key(2), &TABLE, "Bob", "   ", NOW).is_err());
    }

    /// The line is capped in bytes on a character boundary, so what is said
    /// is what arrives.
    #[test]
    fn a_long_line_is_cut_where_the_wire_cuts_it() {
        let long = "🂡".repeat(200);
        let bytes = say(&key(2), &TABLE, "Bob", &long, NOW).unwrap();
        let mut limits = SeatLimiter::default();
        let said = receive(&bytes, &TABLE, seat_of, NOW, &mut limits).unwrap();
        assert!(said.text.len() <= SAID_MAX);
        assert_eq!(said.text, clip_line(&long));
    }
}
