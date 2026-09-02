//! Who is in the lobby, and what they are saying.
//!
//! Both travel on the lobby's own chat topic and both are signed by the
//! **player** key, which is what a name belongs to: a peer id says which socket
//! you are talking to, and a player is who is sitting there (§20).
//!
//! The protocol reserved `LobbyPlayerPresence = 0x0103` and `LobbyChat = 0x0106`
//! from the beginning and nothing filled them in, so the client showed *nobody
//! yet* and *quiet* for ever. What follows is deliberately the smallest thing
//! that fills them:
//!
//! * **Presence** is a name and a timestamp, re-published on the same rhythm as
//!   a table advertisement and forgotten on the same terms — a player who stops
//!   saying they are here stops being here. There is no goodbye message,
//!   because a client that is switched off does not send one and the code that
//!   handled it would be code that never ran when it mattered.
//! * **Chat** is a line of text. Nothing is stored, nothing is replayed to
//!   somebody who arrives later: GossipSub delivers to whoever is on the topic
//!   at the moment of publishing, and pretending otherwise would mean keeping a
//!   history that this client has no business keeping.
//!
//! # What this is not
//!
//! It is not a chat *service*. There is no moderation, no history, no delivery
//! receipt, and a message reaches whoever happens to be subscribed. Anyone can
//! pick any name: `nickname` is decoration and the player key underneath it is
//! the identity. Two players may choose one name and the client must not let
//! that be confusing — which is why what is shown is the name **and** the first
//! eight characters of the key that signed it.

use ed25519_dalek::SigningKey;

use crate::protocol::constants::{LOBBY_CHAT_MAX, LOBBY_MSG_MAX};
use crate::protocol::messages::{EventBody, EventType, SignedEvent};
use crate::protocol::serialization::{from_canonical, to_canonical};
use crate::protocol::signatures::to_be_signed;
use crate::storage::settings::NAME_MAX;

use super::lobby::RateLimiter;

/// The longest line anybody may say.
///
/// Well under the topic's own cap, which has to hold the envelope and the
/// signature too. Two hundred and fifty-six bytes is four or five lines of
/// ordinary text and it is not an essay, which is the point: a lobby pane is
/// eight lines tall.
pub const SAID_MAX: usize = 256;

// **The presence TTL and heartbeat are `protocol::constants`'.** They were
// defined here too, at `90_000` and `30_000`, and this module's pair is what the
// client actually ran on while the documented pair sat next to a compile-time
// assertion that nothing reached. See `constants::PRESENCE_TTL_MS`.

/// What arrived, once it has been checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    /// Somebody is in the lobby, under this name.
    Here { who: [u8; 32], nickname: String },
    /// Somebody said something.
    Said {
        who: [u8; 32],
        nickname: String,
        text: String,
    },
}

/// Why a message was not accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotHeard {
    /// It is not a canonical signed event, or the body is not one of ours.
    Malformed(&'static str),
    /// The signature does not belong to the key that claims it.
    Forged,
    /// This peer has said too much, too fast.
    TooMuch,
    /// A name or a line over its cap.
    TooLong(&'static str),
    /// A timestamp too far from this client's clock to be a live message.
    Stale,
}

/// How far from local time a message may be stamped.
///
/// Generous, because clocks differ and this is not a security boundary — but
/// bounded, because a message stamped a year ahead would otherwise sit at the
/// top of the pane for ever.
pub const CLOCK_SLACK_MS: u64 = 120_000;

#[derive(Debug, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
struct Body {
    #[n(0)]
    nickname: String,
    /// Empty for presence, the line for chat. One body type rather than two,
    /// because the two differ by a single field and the `EventType` already
    /// says which is which.
    #[n(1)]
    text: String,
}

/// Say that this player is in the lobby.
pub fn presence(key: &SigningKey, nickname: &str, now_ms: u64) -> Result<Vec<u8>, &'static str> {
    seal(key, EventType::LobbyPlayerPresence, nickname, "", now_ms)
}

/// Say something.
pub fn say(
    key: &SigningKey,
    nickname: &str,
    text: &str,
    now_ms: u64,
) -> Result<Vec<u8>, &'static str> {
    if text.trim().is_empty() {
        return Err("nothing to say");
    }
    seal(key, EventType::LobbyChat, nickname, text, now_ms)
}

fn seal(
    key: &SigningKey,
    kind: EventType,
    nickname: &str,
    text: &str,
    now_ms: u64,
) -> Result<Vec<u8>, &'static str> {
    // Trimmed to the cap **in bytes**, and on a character boundary. A name is
    // whatever a player typed, and typing an emoji must not produce a message
    // that this client refuses to parse back.
    let body = Body {
        nickname: clip(nickname, NAME_MAX),
        text: clip(text, SAID_MAX),
    };
    let body_bytes = to_canonical(&body).map_err(|_| "the body does not encode")?;
    if body_bytes.len() > LOBBY_CHAT_MAX {
        return Err("over the cap");
    }

    let envelope = EventBody::unchained(
        kind,
        key.verifying_key().to_bytes(),
        body_bytes,
        now_ms,
    )
    .ok_or("lobby talk is an unchained event")?;
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

/// Cut a string to a byte cap without splitting a character.
///
/// Bytes rather than characters, because the cap the wire enforces is bytes and
/// a name of thirty-two emoji is a hundred and twenty-eight of them. Popping
/// whole characters is what keeps the result a `String`.
fn clip(s: &str, cap: usize) -> String {
    let mut out = s.trim().to_owned();
    while out.len() > cap {
        out.pop();
    }
    out
}

/// Take a presence or a chat line off the wire.
///
/// `from_peer` is the GossipSub source and is charged **before** the signature,
/// which is the expensive part — the same order `advert::receive` uses, and for
/// the same reason: a limiter that ran after verification would let a stranger
/// spend this client's processor at will.
pub fn receive(
    bytes: &[u8],
    from_peer: [u8; 32],
    now_ms: u64,
    limits: &mut RateLimiter,
) -> Result<Heard, NotHeard> {
    if bytes.len() > LOBBY_MSG_MAX {
        return Err(NotHeard::TooLong("the message is over the topic's cap"));
    }
    if !limits.admit_peer_talk(from_peer, now_ms) {
        return Err(NotHeard::TooMuch);
    }

    let signed: SignedEvent =
        from_canonical(bytes, LOBBY_MSG_MAX).map_err(|_| NotHeard::Malformed("not a signed event"))?;
    let envelope: EventBody = from_canonical(&signed.body, LOBBY_MSG_MAX)
        .map_err(|_| NotHeard::Malformed("not an envelope"))?;

    let kind = EventType::try_from(envelope.event_type)
        .map_err(|_| NotHeard::Malformed("an event type this client does not know"))?;
    if !matches!(
        kind,
        EventType::LobbyPlayerPresence | EventType::LobbyChat
    ) {
        return Err(NotHeard::Malformed("not lobby talk"));
    }

    // A message stamped far from now is not a live one. Checked before the
    // signature is verified only in the cheap direction: this is arithmetic and
    // the signature is not.
    if envelope.emitted_at_unix_ms.abs_diff(now_ms) > CLOCK_SLACK_MS {
        return Err(NotHeard::Stale);
    }

    let who = envelope.sender_public_key;
    verify(&who, &signed).map_err(|_| NotHeard::Forged)?;

    let body: Body = from_canonical(&envelope.payload, LOBBY_CHAT_MAX)
        .map_err(|_| NotHeard::Malformed("not a lobby body"))?;
    if body.nickname.len() > NAME_MAX {
        return Err(NotHeard::TooLong("the name is over its cap"));
    }
    if body.text.len() > SAID_MAX {
        return Err(NotHeard::TooLong("the line is over its cap"));
    }

    Ok(match kind {
        EventType::LobbyChat => {
            if body.text.trim().is_empty() {
                return Err(NotHeard::Malformed("an empty line is not a message"));
            }
            Heard::Said {
                who,
                nickname: body.nickname,
                text: body.text,
            }
        }
        _ => Heard::Here {
            who,
            nickname: body.nickname,
        },
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

    #[test]
    fn a_presence_round_trips() {
        let k = key(1);
        let bytes = presence(&k, "Alice", NOW).unwrap();
        let mut limits = RateLimiter::new();
        assert_eq!(
            receive(&bytes, [9u8; 32], NOW, &mut limits).unwrap(),
            Heard::Here {
                who: k.verifying_key().to_bytes(),
                nickname: "Alice".into()
            }
        );
    }

    #[test]
    fn a_line_round_trips() {
        let k = key(2);
        let bytes = say(&k, "Bob", "anyone for a table?", NOW).unwrap();
        let mut limits = RateLimiter::new();
        match receive(&bytes, [9u8; 32], NOW, &mut limits).unwrap() {
            Heard::Said { nickname, text, .. } => {
                assert_eq!(nickname, "Bob");
                assert_eq!(text, "anyone for a table?");
            }
            other => panic!("{other:?}"),
        }
    }

    /// The signature is over the envelope, so changing the body under it must
    /// not verify. This is the whole reason a name is signed at all.
    #[test]
    fn a_tampered_message_is_refused() {
        let k = key(3);
        let bytes = say(&k, "Carol", "hello", NOW).unwrap();
        let mut broken = bytes.clone();
        // Flip a byte inside the signed envelope.
        let at = broken.len() / 2;
        broken[at] ^= 0xff;
        let mut limits = RateLimiter::new();
        assert!(receive(&broken, [9u8; 32], NOW, &mut limits).is_err());
    }

    /// Anybody may claim any name. What must not be claimable is somebody
    /// else's **key**, and the signature is what stops that.
    #[test]
    fn a_name_is_decoration_and_the_key_is_the_identity() {
        let mine = key(4);
        let theirs = key(5);
        let bytes = say(&mine, "Dave", "not really Dave", NOW).unwrap();
        let mut limits = RateLimiter::new();
        match receive(&bytes, [9u8; 32], NOW, &mut limits).unwrap() {
            Heard::Said { who, nickname, .. } => {
                assert_eq!(nickname, "Dave", "the name is whatever was typed");
                assert_eq!(who, mine.verifying_key().to_bytes());
                assert_ne!(who, theirs.verifying_key().to_bytes());
            }
            other => panic!("{other:?}"),
        }
    }

    /// A long name is cut on a character boundary, not in the middle of one.
    #[test]
    fn an_over_long_name_survives_as_a_string() {
        let k = key(6);
        let emoji = "🂡".repeat(40);
        let bytes = presence(&k, &emoji, NOW).unwrap();
        let mut limits = RateLimiter::new();
        match receive(&bytes, [9u8; 32], NOW, &mut limits).unwrap() {
            Heard::Here { nickname, .. } => {
                assert!(nickname.len() <= NAME_MAX);
                assert!(nickname.chars().all(|c| c == '🂡'), "cut on a boundary");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn nothing_to_say_is_not_a_message() {
        assert!(say(&key(7), "Erin", "   ", NOW).is_err());
    }

    /// A message stamped far from now is not live, and would otherwise sit at
    /// the top of the pane until the client restarted.
    #[test]
    fn a_message_from_far_away_in_time_is_refused() {
        let k = key(8);
        let bytes = say(&k, "Frank", "hello from the future", NOW + CLOCK_SLACK_MS * 2).unwrap();
        let mut limits = RateLimiter::new();
        assert_eq!(
            receive(&bytes, [9u8; 32], NOW, &mut limits),
            Err(NotHeard::Stale)
        );
    }

    /// The budget is the peer's, charged before the signature.
    #[test]
    fn a_peer_that_will_not_stop_is_stopped() {
        let k = key(9);
        let mut limits = RateLimiter::new();
        let peer = [3u8; 32];
        let mut refused = 0;
        for i in 0..200 {
            let bytes = say(&k, "Grace", &format!("line {i}"), NOW).unwrap();
            if receive(&bytes, peer, NOW, &mut limits) == Err(NotHeard::TooMuch) {
                refused += 1;
            }
        }
        assert!(refused > 0, "a peer with no limit is a peer with a megaphone");
    }
}
