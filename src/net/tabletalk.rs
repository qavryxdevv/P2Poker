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

/// `D-054`: how many of those a speaker may hold back and spend at once.
///
/// A hard one-per-two-seconds gate drops the second half of a real exchange --
/// *nice hand* and then *wp* half a second later -- which is the shape of
/// people talking and not of a flood. Three is what a person can type in a
/// burst and is still three hundredths of what a flood is.
pub const LINE_BURST: u32 = 3;

/// `D-054`: over how long the byte budget below is counted.
pub const CHAT_BYTES_MS: u64 = 60_000;

/// `D-054`: how many bytes of chat one speaker may put on the carrier a
/// minute.
///
/// Eight full-length lines. The line budget alone bounds the *count* and says
/// nothing about the size, so a speaker inside it can still put thirty
/// maximum-length lines a minute on a link that is carrying a hand; this is
/// what makes chat unable to crowd the game out even while every line of it is
/// legal. A person types a fraction of it.
pub const CHAT_BYTES_MAX: u64 = 2_048;

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
    /// The **member that carried it** has put more on the carrier than a
    /// speaker may (`D-054`). Charged before anything is read, against the key
    /// the carrier itself reports, which nobody can claim to be.
    TooMuchCarried,
    /// This seat has said too much, too fast.
    TooMuch,
    TooLong(&'static str),
    /// `D-054`: the line or the name is not one line of printable text.
    NotPlain(crate::net::plaintext::NotPlain),
    Stale,
}

/// `D-054`: how much noise a refusal is worth, for `D-051`'s meter.
///
/// A budget spent is one point: it is what an eager client looks like, and
/// sixteen of them in a minute is still a flood. Everything else is a message
/// no client of this build sends -- a forgery, another table's line, a stranger
/// claiming a seat, text that is not text -- and is worth what any other bad
/// message is worth.
pub fn noise_points(why: NotHeard) -> u32 {
    use crate::table::membership::{NOISE_BAD_FRAGMENT, NOISE_BAD_MESSAGE};
    match why {
        NotHeard::TooMuch | NotHeard::TooMuchCarried => NOISE_BAD_FRAGMENT,
        // A clock that drifted is not an attack, and two minutes of slack is
        // already generous; count it as the smallest thing there is.
        NotHeard::Stale => NOISE_BAD_FRAGMENT,
        _ => NOISE_BAD_MESSAGE,
    }
}

/// `D-054`: what one speaker may put on a carrier -- lines and bytes.
///
/// A token bucket for the lines, so a burst of a real exchange survives and a
/// stream does not, and a rolling window for the bytes, because the count of
/// lines says nothing about what they weigh.
#[derive(Debug, Clone)]
pub struct Budget {
    tokens: u32,
    at: u64,
    bytes: std::collections::VecDeque<(u64, u64)>,
}

impl Default for Budget {
    fn default() -> Self {
        Self { tokens: LINE_BURST, at: 0, bytes: std::collections::VecDeque::new() }
    }
}

impl Budget {
    /// Whether a line of `bytes` may be spent now, and spend it if so.
    pub fn admit(&mut self, now_ms: u64, bytes: u64) -> bool {
        if self.at == 0 {
            self.at = now_ms;
        }
        // The tokens earned since the last look, never more than the burst.
        let earned = now_ms.saturating_sub(self.at) / LINE_EVERY_MS;
        if earned > 0 {
            self.tokens = (self.tokens as u64 + earned).min(u64::from(LINE_BURST)) as u32;
            self.at += earned * LINE_EVERY_MS;
        }
        while self.bytes.front().is_some_and(|(at, _)| now_ms.saturating_sub(*at) >= CHAT_BYTES_MS) {
            self.bytes.pop_front();
        }
        let carried: u64 = self.bytes.iter().map(|(_, n)| *n).sum();
        if self.tokens == 0 || carried + bytes > CHAT_BYTES_MAX {
            return false;
        }
        self.tokens -= 1;
        self.bytes.push_back((now_ms, bytes));
        true
    }
}

/// `D-054`: the budgets of one table's chat, kept apart on purpose.
///
/// **By carrier** is charged first, against the member key the group itself
/// reports for the bytes -- a fact about who sent them, which no sender can
/// claim to be. **By seat** is charged only *after* the signature verifies,
/// because until then the key in the envelope is a claim, and a claim that
/// could spend a budget is a way to silence the player it names: one unsigned
/// line every two seconds under Alice's key and Alice is mute at every table
/// she sits at, with nothing in anybody's window to say why. `lobby.rs` had
/// exactly this bug against table adverts and its fix is the same one.
#[derive(Debug, Clone, Default)]
pub struct Talk {
    by_carrier: BTreeMap<[u8; 32], Budget>,
    by_seat: BTreeMap<u8, Budget>,
}

/// `D-054`: what a line of this text will weigh on the wire, near enough for a
/// sender to hold itself to the budget its receivers hold it to.
///
/// The envelope, the signature and the CBOR round it are a fixed cost the
/// sender cannot see from here, so it is counted in: a sender that guessed low
/// would send a line every receiver then refused, which is the one outcome this
/// is for avoiding.
pub fn line_weight(text: &str) -> u64 {
    const ENVELOPE: u64 = 200;
    ENVELOPE + text.len() as u64 + NAME_MAX as u64
}

impl Talk {
    /// The carrier's own budget, charged before a byte is read.
    pub fn carried(&mut self, carrier: &[u8; 32], now_ms: u64, bytes: u64) -> bool {
        self.by_carrier.entry(*carrier).or_default().admit(now_ms, bytes)
    }

    /// The seat's budget, charged after the signature.
    pub fn said(&mut self, seat: u8, now_ms: u64, bytes: u64) -> bool {
        self.by_seat.entry(seat).or_default().admit(now_ms, bytes)
    }

    /// Forget a member that has left the group, so the map cannot grow beyond
    /// the table.
    pub fn forget(&mut self, carrier: &[u8; 32]) {
        self.by_carrier.remove(carrier);
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
///
/// `D-054`: and make it one line of printable text first, so what this client
/// signs is what a receiver of this build accepts -- the two rules are one.
fn clip(s: &str, cap: usize) -> String {
    let mut out = crate::net::plaintext::to_plain_line(s);
    while out.len() > cap {
        out.pop();
    }
    // A cut on a byte cap can leave a mark whose base has gone.
    crate::net::plaintext::to_plain_line(&out)
}

/// The line as it will be said: cleaned, trimmed and capped the way `say` caps
/// it.
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
    // `D-054`: a line that cleans to nothing was never a line -- somebody
    // pasted a page of newlines, or a stack of marks.
    if body.text.is_empty() {
        return Err("nothing to say");
    }
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
/// `carrier` is the member key the table's group reports for these bytes: a
/// fact, not a claim. It is charged **before** anything is read, so a stream
/// of rubbish costs its sender its own budget and this client nothing.
///
/// The **seat's** budget is charged only after the signature verifies. `D-054`:
/// charging the key in the envelope before that let anybody silence any player
/// at will -- the key is public, it is in the roster, and an unsigned line
/// under it spent its owner's allowance before the forgery was noticed.
pub fn receive(
    bytes: &[u8],
    table_id: &[u8; 32],
    carrier: &[u8; 32],
    seat_of: impl Fn(&[u8; 32]) -> Option<u8>,
    now_ms: u64,
    limits: &mut Talk,
) -> Result<Said, NotHeard> {
    if bytes.len() > LOBBY_MSG_MAX {
        return Err(NotHeard::TooLong("the message is over the cap"));
    }
    if !limits.carried(carrier, now_ms, bytes.len() as u64) {
        return Err(NotHeard::TooMuchCarried);
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
    verify(&who, &signed).map_err(|_| NotHeard::Forged)?;
    if !limits.said(seat, now_ms, bytes.len() as u64) {
        return Err(NotHeard::TooMuch);
    }

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
    // `D-054`: and it is one line of printable text, at both ends. This client
    // cleans what its own player types before it signs it, so an honest line is
    // already plain and nothing honest is refused here.
    plain(&body.text).map_err(NotHeard::NotPlain)?;
    if !body.nickname.is_empty() {
        plain(&body.nickname).map_err(NotHeard::NotPlain)?;
    }
    Ok(Said {
        seat,
        who,
        nickname: body.nickname,
        text: body.text,
    })
}

/// `S1-FQ`: the one reason a `TABLE_LEAVE` carries: the player left the table.
pub const LEAVE_BY_THE_PLAYER: u16 = 1;

#[derive(Debug, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
struct LeaveBody {
    /// The table left, so the word cannot be carried to another.
    #[cbor(n(0), with = "minicbor::bytes")]
    table_id: [u8; 32],
    #[n(1)]
    reason: u16,
}

/// `S1-FQ`, `PROTOCOL.md` §7.10: this client's player leaves the table, said
/// to its seats -- signed by the seat's key, for this table. The only word from
/// which another seat may say that a player left: the carrier's own report of a
/// member leaving on purpose is also made of a client that merely rejoins.
/// `S1-GP`: the table a leave word names, read without believing anything else
/// of it -- to route the word to its table's slot; `receive_leave` judges it.
pub fn leave_table_of(bytes: &[u8]) -> Option<[u8; 32]> {
    let signed: SignedEvent = from_canonical(bytes, LOBBY_MSG_MAX).ok()?;
    let envelope: EventBody = from_canonical(&signed.body, LOBBY_MSG_MAX).ok()?;
    if EventType::try_from(envelope.event_type).ok()? != EventType::TableLeave {
        return None;
    }
    let body: LeaveBody = from_canonical(&envelope.payload, LOBBY_MSG_MAX).ok()?;
    Some(body.table_id)
}

pub fn leave_word(key: &SigningKey, table_id: &[u8; 32], now_ms: u64) -> Result<Vec<u8>, &'static str> {
    let body = LeaveBody { table_id: *table_id, reason: LEAVE_BY_THE_PLAYER };
    let body_bytes = to_canonical(&body).map_err(|_| "the body does not encode")?;
    let envelope = EventBody::unchained(EventType::TableLeave, key.verifying_key().to_bytes(), body_bytes, now_ms)
        .ok_or("a table leave is an unchained event")?;
    let envelope_bytes = to_canonical(&envelope).map_err(|_| "the envelope does not encode")?;
    let signature = {
        use ed25519_dalek::Signer;
        key.sign(&to_be_signed(&envelope_bytes))
    };
    to_canonical(&SignedEvent { body: envelope_bytes, signature: signature.to_bytes() })
        .map_err(|_| "the signed event does not encode")
}

/// `S1-FQ`: take a seat's word that its player left off the wire, for the table
/// `table_id` -- the seat it holds, the key that said it and when (`S1-GA`: a
/// word older than the seat's present sitting is not about it), or why not.
/// Charged to the carrier first, as a line is (`D-054`); then the type, the
/// clock, the seat, the signature, the table and the reason.
pub fn receive_leave(
    bytes: &[u8],
    table_id: &[u8; 32],
    carrier: &[u8; 32],
    seat_of: impl Fn(&[u8; 32]) -> Option<u8>,
    now_ms: u64,
    limits: &mut Talk,
) -> Result<(u8, [u8; 32], u64), NotHeard> {
    if bytes.len() > LOBBY_MSG_MAX {
        return Err(NotHeard::TooLong("the message is over the cap"));
    }
    if !limits.carried(carrier, now_ms, bytes.len() as u64) {
        return Err(NotHeard::TooMuchCarried);
    }
    let signed: SignedEvent =
        from_canonical(bytes, LOBBY_MSG_MAX).map_err(|_| NotHeard::Malformed("not a signed event"))?;
    let envelope: EventBody = from_canonical(&signed.body, LOBBY_MSG_MAX)
        .map_err(|_| NotHeard::Malformed("not an envelope"))?;
    let kind = EventType::try_from(envelope.event_type)
        .map_err(|_| NotHeard::Malformed("an event type this client does not know"))?;
    if kind != EventType::TableLeave {
        return Err(NotHeard::Malformed("not a table leave"));
    }
    if envelope.emitted_at_unix_ms.abs_diff(now_ms) > CLOCK_SLACK_MS {
        return Err(NotHeard::Stale);
    }
    let who = envelope.sender_public_key;
    let seat = seat_of(&who).ok_or(NotHeard::NotASeat)?;
    verify(&who, &signed).map_err(|_| NotHeard::Forged)?;
    let body: LeaveBody = from_canonical(&envelope.payload, LOBBY_CHAT_MAX)
        .map_err(|_| NotHeard::Malformed("not a table leave"))?;
    if &body.table_id != table_id {
        return Err(NotHeard::AnotherTable);
    }
    if body.reason != LEAVE_BY_THE_PLAYER {
        return Err(NotHeard::Malformed("a leave reason this client does not know"));
    }
    Ok((seat, who, envelope.emitted_at_unix_ms))
}

fn plain(s: &str) -> Result<(), crate::net::plaintext::NotPlain> {
    crate::net::plaintext::is_plain_line(s)
}

fn verify(who: &[u8; 32], signed: &SignedEvent) -> Result<(), ()> {
    use ed25519_dalek::{Signature, VerifyingKey};
    let key = VerifyingKey::from_bytes(who).map_err(|_| ())?;
    let sig = Signature::from_bytes(&signed.signature);
    key.verify_strict(&to_be_signed(&signed.body), &sig)
        .map_err(|_| ())
}

/// `D-060`, `PROTOCOL.md` §7.11: the seats a seat cannot hear, before its table
/// is set -- in the table's group, from the roster of `list_serial`.
#[derive(Debug, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
struct HearingBody {
    /// The table, so the word cannot be carried to another.
    #[cbor(n(0), with = "minicbor::bytes")]
    table_id: [u8; 32],
    /// The roster the word is about: a word about another roster says nothing
    /// of this one.
    #[n(1)]
    list_serial: u64,
    /// The seats this seat cannot hear, ascending; empty when it hears them all.
    #[cbor(n(2), with = "minicbor::bytes")]
    unheard: Vec<u8>,
}

/// A seat's word on whom it hears, once it has been checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hearing {
    pub seat: u8,
    pub list_serial: u64,
    pub unheard: Vec<u8>,
    pub said_ms: u64,
}

/// `D-060`: this client's seat cannot hear the seats `unheard` of the roster
/// `list_serial` -- said to the table before it is set, signed by the seat's key.
pub fn hearing_word(
    key: &SigningKey,
    table_id: &[u8; 32],
    list_serial: u64,
    unheard: &[u8],
    now_ms: u64,
) -> Result<Vec<u8>, &'static str> {
    let mut unheard = unheard.to_vec();
    unheard.sort_unstable();
    unheard.dedup();
    let body = HearingBody { table_id: *table_id, list_serial, unheard };
    let body_bytes = to_canonical(&body).map_err(|_| "the body does not encode")?;
    let envelope = EventBody::unchained(EventType::TableHearing, key.verifying_key().to_bytes(), body_bytes, now_ms)
        .ok_or("a table hearing is an unchained event")?;
    let envelope_bytes = to_canonical(&envelope).map_err(|_| "the envelope does not encode")?;
    let signature = {
        use ed25519_dalek::Signer;
        key.sign(&to_be_signed(&envelope_bytes))
    };
    to_canonical(&SignedEvent { body: envelope_bytes, signature: signature.to_bytes() })
        .map_err(|_| "the signed event does not encode")
}

/// `D-060`: take a seat's word on whom it cannot hear off the wire, for the
/// table `table_id` -- the type, the clock, the seat, the signature, the table,
/// and the list itself: seat numbers of a table, ascending, none twice, never
/// the speaker's own. Not charged to the chat's budget: every seat says one
/// every few seconds while the table forms, which is more than a minute's chat
/// allows, and the node takes one a second from a carrier at most.
pub fn receive_hearing(
    bytes: &[u8],
    table_id: &[u8; 32],
    seat_of: impl Fn(&[u8; 32]) -> Option<u8>,
    now_ms: u64,
) -> Result<Hearing, NotHeard> {
    if bytes.len() > LOBBY_MSG_MAX {
        return Err(NotHeard::TooLong("the message is over the cap"));
    }
    let signed: SignedEvent =
        from_canonical(bytes, LOBBY_MSG_MAX).map_err(|_| NotHeard::Malformed("not a signed event"))?;
    let envelope: EventBody = from_canonical(&signed.body, LOBBY_MSG_MAX)
        .map_err(|_| NotHeard::Malformed("not an envelope"))?;
    let kind = EventType::try_from(envelope.event_type)
        .map_err(|_| NotHeard::Malformed("an event type this client does not know"))?;
    if kind != EventType::TableHearing {
        return Err(NotHeard::Malformed("not a table hearing"));
    }
    if envelope.emitted_at_unix_ms.abs_diff(now_ms) > CLOCK_SLACK_MS {
        return Err(NotHeard::Stale);
    }
    let who = envelope.sender_public_key;
    let seat = seat_of(&who).ok_or(NotHeard::NotASeat)?;
    verify(&who, &signed).map_err(|_| NotHeard::Forged)?;
    let body: HearingBody = from_canonical(&envelope.payload, LOBBY_CHAT_MAX)
        .map_err(|_| NotHeard::Malformed("not a table hearing"))?;
    if &body.table_id != table_id {
        return Err(NotHeard::AnotherTable);
    }
    let max = crate::protocol::constants::MAX_SEATS;
    if body.unheard.len() >= usize::from(max)
        || body.unheard.windows(2).any(|w| w[0] >= w[1])
        || body.unheard.iter().any(|s| *s >= max || *s == seat)
    {
        return Err(NotHeard::Malformed("a list of seats no table has"));
    }
    Ok(Hearing { seat, list_serial: body.list_serial, unheard: body.unheard, said_ms: envelope.emitted_at_unix_ms })
}

/// `D-061`, `PROTOCOL.md` §7.12: a forming table goes on without its founder, at
/// the table a seat of its roster founded for it.
#[derive(Debug, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
struct ContinuesBody {
    /// The table that goes on, so the word cannot be carried to another.
    #[cbor(n(0), with = "minicbor::bytes")]
    table_id: [u8; 32],
    /// The roster of that table the speaker held.
    #[n(1)]
    list_serial: u64,
    /// The signed `LOBBY_TABLE_AD` of the table it goes on at, verbatim.
    #[cbor(n(2), with = "minicbor::bytes")]
    advert: Vec<u8>,
}

/// A seat's word that its forming table goes on at a table it founded, once
/// checked: the seat, its key, the roster serial it held, the advert of the new
/// table with that table's key, and when it was said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continues {
    pub seat: u8,
    pub who: [u8; 32],
    pub list_serial: u64,
    pub advert: Vec<u8>,
    pub table_key: [u8; 32],
    pub said_ms: u64,
}

/// `D-061`: this client's seat founded `advert`'s table for the forming table
/// `table_id` to go on at -- said to that table's seats, signed by the seat's key.
pub fn continues_word(
    key: &SigningKey,
    table_id: &[u8; 32],
    list_serial: u64,
    advert: &[u8],
    now_ms: u64,
) -> Result<Vec<u8>, &'static str> {
    let body = ContinuesBody { table_id: *table_id, list_serial, advert: advert.to_vec() };
    let body_bytes = to_canonical(&body).map_err(|_| "the body does not encode")?;
    let envelope = EventBody::unchained(EventType::TableContinues, key.verifying_key().to_bytes(), body_bytes, now_ms)
        .ok_or("a table continuation is an unchained event")?;
    let envelope_bytes = to_canonical(&envelope).map_err(|_| "the envelope does not encode")?;
    let signature = {
        use ed25519_dalek::Signer;
        key.sign(&to_be_signed(&envelope_bytes))
    };
    to_canonical(&SignedEvent { body: envelope_bytes, signature: signature.to_bytes() })
        .map_err(|_| "the signed event does not encode")
}

/// `D-061`: take a seat's word that its forming table goes on at a table it
/// founded -- the type, the clock, the seat, the signature, the table; then the
/// advert: a real signed advert, of a table that is not this one, whose founder is
/// the speaker itself, playing `game` -- the advert of the table that goes on:
/// its rules, its name and its gate (`S1-GH`). A seat can offer only a table of
/// its own, and only the same game.
pub fn receive_continues(
    bytes: &[u8],
    table_id: &[u8; 32],
    game: &crate::net::lobby::TableAd,
    seat_of: impl Fn(&[u8; 32]) -> Option<u8>,
    now_ms: u64,
) -> Result<Continues, NotHeard> {
    if bytes.len() > LOBBY_MSG_MAX {
        return Err(NotHeard::TooLong("the message is over the cap"));
    }
    let signed: SignedEvent =
        from_canonical(bytes, LOBBY_MSG_MAX).map_err(|_| NotHeard::Malformed("not a signed event"))?;
    let envelope: EventBody = from_canonical(&signed.body, LOBBY_MSG_MAX)
        .map_err(|_| NotHeard::Malformed("not an envelope"))?;
    let kind = EventType::try_from(envelope.event_type)
        .map_err(|_| NotHeard::Malformed("an event type this client does not know"))?;
    if kind != EventType::TableContinues {
        return Err(NotHeard::Malformed("not a table continuation"));
    }
    if envelope.emitted_at_unix_ms.abs_diff(now_ms) > CLOCK_SLACK_MS {
        return Err(NotHeard::Stale);
    }
    let who = envelope.sender_public_key;
    let seat = seat_of(&who).ok_or(NotHeard::NotASeat)?;
    verify(&who, &signed).map_err(|_| NotHeard::Forged)?;
    let body: ContinuesBody = from_canonical(&envelope.payload, LOBBY_MSG_MAX)
        .map_err(|_| NotHeard::Malformed("not a table continuation"))?;
    if &body.table_id != table_id {
        return Err(NotHeard::AnotherTable);
    }
    let (ad, _) = super::advert::verify_echoed(&body.advert)
        .map_err(|_| NotHeard::Malformed("the advert of the table it goes on at does not hold"))?;
    let table_key: [u8; 32] = from_canonical::<SignedEvent>(&body.advert, LOBBY_MSG_MAX)
        .ok()
        .and_then(|s| from_canonical::<EventBody>(&s.body, LOBBY_MSG_MAX).ok())
        .map(|e| e.sender_public_key)
        .ok_or(NotHeard::Malformed("the advert of the table it goes on at does not hold"))?;
    if &table_key == table_id {
        return Err(NotHeard::Malformed("a table does not go on at itself"));
    }
    if ad.founder_app_key != who {
        return Err(NotHeard::Malformed("a seat offers only a table it founded"));
    }
    // `S1-GH`: the founder's authority a continuation gives its seat is over who
    // sits down, never over what the table plays.
    if super::advert::table_params_hash(&ad) != super::advert::table_params_hash(game)
        || ad.table_name != game.table_name
        || ad.password_required != game.password_required
    {
        return Err(NotHeard::Malformed("a continuation plays the table's own game"));
    }
    Ok(Continues { seat, who, list_serial: body.list_serial, advert: body.advert, table_key, said_ms: envelope.emitted_at_unix_ms })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::plaintext::NotPlain;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    const NOW: u64 = 1_700_000_000_000;
    const TABLE: [u8; 32] = [7u8; 32];
    /// The member key the group reports for the bytes: a fact, not a claim.
    const CARRIER: [u8; 32] = [21u8; 32];
    const OTHER_CARRIER: [u8; 32] = [22u8; 32];

    /// Seat 2 is key 2, seat 5 is key 5, nobody else is seated.
    fn seat_of(k: &[u8; 32]) -> Option<u8> {
        [(2u8, key(2)), (5u8, key(5))]
            .iter()
            .find(|(_, s)| &s.verifying_key().to_bytes() == k)
            .map(|(seat, _)| *seat)
    }

    fn hear(bytes: &[u8], carrier: &[u8; 32], now: u64, talk: &mut Talk) -> Result<Said, NotHeard> {
        receive(bytes, &TABLE, carrier, seat_of, now, talk)
    }

    /// `S1-FQ`: a seat's word that its player left the table is heard only from
    /// that seat's own key, for this table, on time -- and a line of chat is no
    /// such word, nor the word a line.
    #[test]
    fn a_leave_word_is_heard_only_from_its_seat_for_its_table() {
        let mut talk = Talk::default();
        let word = leave_word(&key(2), &TABLE, NOW).unwrap();
        assert_eq!(receive_leave(&word, &TABLE, &CARRIER, seat_of, NOW, &mut talk), Ok((2, key(2).verifying_key().to_bytes(), NOW)));
        let stranger = leave_word(&key(9), &TABLE, NOW).unwrap();
        assert_eq!(receive_leave(&stranger, &TABLE, &CARRIER, seat_of, NOW, &mut talk), Err(NotHeard::NotASeat));
        let elsewhere = leave_word(&key(5), &[8u8; 32], NOW).unwrap();
        assert_eq!(
            receive_leave(&elsewhere, &TABLE, &OTHER_CARRIER, seat_of, NOW, &mut talk),
            Err(NotHeard::AnotherTable)
        );
        let mut broken = leave_word(&key(5), &TABLE, NOW).unwrap();
        let at = broken.len() / 2;
        broken[at] ^= 0xff;
        assert!(receive_leave(&broken, &TABLE, &OTHER_CARRIER, seat_of, NOW, &mut talk).is_err());
        let old = leave_word(&key(5), &TABLE, NOW).unwrap();
        assert_eq!(
            receive_leave(&old, &TABLE, &CARRIER, seat_of, NOW + CLOCK_SLACK_MS + 1, &mut Talk::default()),
            Err(NotHeard::Stale)
        );
        let line = say(&key(2), &TABLE, "Bob", "bye", NOW).unwrap();
        assert!(matches!(
            receive_leave(&line, &TABLE, &CARRIER, seat_of, NOW, &mut Talk::default()),
            Err(NotHeard::Malformed(_))
        ));
        assert!(matches!(
            receive(&word, &TABLE, &CARRIER, seat_of, NOW, &mut Talk::default()),
            Err(NotHeard::Malformed(_))
        ));
    }

    /// `S1-GP`: a leave word names its table for routing; nothing else does.
    #[test]
    fn a_leave_word_names_its_table_and_nothing_else_does() {
        let word = leave_word(&key(2), &TABLE, NOW).unwrap();
        assert_eq!(leave_table_of(&word), Some(TABLE));
        let hearing = hearing_word(&key(2), &TABLE, 3, &[], NOW).unwrap();
        assert_eq!(leave_table_of(&hearing), None);
        assert_eq!(leave_table_of(b"nothing"), None);
    }

    /// `D-061`: a seat's word that its forming table goes on at a table it founded
    /// is heard from its own seat, for this table, with a real advert of another
    /// table whose founder is the speaker -- a seat offers only a table of its own.
    #[test]
    fn a_continuation_is_heard_only_with_a_table_its_speaker_founded() {
        use crate::net::lobby::TableAd;
        let new_table = key(40);
        // The table that goes on, founded by seat 0's player; its continuation,
        // the same game under seat 2's key.
        let game = TableAd::sng(6, "goes on".into(), key(0).verifying_key().to_bytes(), vec![9], NOW - 60_000);
        let own = TableAd::sng(6, "goes on".into(), key(2).verifying_key().to_bytes(), vec![1, 2, 3], NOW);
        let advert = crate::net::advert::publish(&own, &new_table).unwrap();
        let word = continues_word(&key(2), &TABLE, 9, &advert, NOW).unwrap();
        let heard = receive_continues(&word, &TABLE, &game, seat_of, NOW).unwrap();
        assert_eq!(
            (heard.seat, heard.who, heard.list_serial, heard.table_key, heard.said_ms),
            (2, key(2).verifying_key().to_bytes(), 9, new_table.verifying_key().to_bytes(), NOW)
        );
        assert_eq!(heard.advert, advert, "the advert, verbatim, for the lobby to take");

        // A table somebody else founded is not this seat's to offer.
        let theirs = TableAd::sng(6, "not mine".into(), key(5).verifying_key().to_bytes(), vec![1], NOW);
        let their_advert = crate::net::advert::publish(&theirs, &key(41)).unwrap();
        let claim = continues_word(&key(2), &TABLE, 9, &their_advert, NOW).unwrap();
        assert!(matches!(receive_continues(&claim, &TABLE, &game, seat_of, NOW), Err(NotHeard::Malformed(_))));

        // `S1-GH`: nor another game -- other rules, another name, another gate --
        // under the speaker's own key.
        let bigger = TableAd::sng(9, "goes on".into(), key(2).verifying_key().to_bytes(), vec![1, 2, 3], NOW);
        let mut renamed = own.clone();
        renamed.table_name = "somewhere else".into();
        let mut gated = own.clone();
        gated.password_required = true;
        let mut longer = own.clone();
        longer.join_deadline_ms += 1_000;
        for (i, other) in [bigger, renamed, gated, longer].iter().enumerate() {
            let other_advert = crate::net::advert::publish(other, &key(60 + i as u8)).unwrap();
            let other_word = continues_word(&key(2), &TABLE, 9, &other_advert, NOW).unwrap();
            assert_eq!(
                receive_continues(&other_word, &TABLE, &game, seat_of, NOW).map(|c| c.seat),
                Err(NotHeard::Malformed("a continuation plays the table's own game")),
                "case {i}"
            );
        }

        // A table does not go on at itself.
        let table_key = key(50);
        let table_id = table_key.verifying_key().to_bytes();
        let same = crate::net::advert::publish(&own, &table_key).unwrap();
        let circle = continues_word(&key(2), &table_id, 9, &same, NOW).unwrap();
        assert!(matches!(receive_continues(&circle, &table_id, &game, seat_of, NOW), Err(NotHeard::Malformed(_))));

        // Nor from a stranger, for another table, late, or with a byte changed.
        let stranger_ad = TableAd::sng(6, "mine".into(), key(9).verifying_key().to_bytes(), vec![1], NOW);
        let stranger_advert = crate::net::advert::publish(&stranger_ad, &key(42)).unwrap();
        let stranger = continues_word(&key(9), &TABLE, 9, &stranger_advert, NOW).unwrap();
        assert_eq!(receive_continues(&stranger, &TABLE, &game, seat_of, NOW), Err(NotHeard::NotASeat));
        let elsewhere = continues_word(&key(2), &[8u8; 32], 9, &advert, NOW).unwrap();
        assert_eq!(receive_continues(&elsewhere, &TABLE, &game, seat_of, NOW), Err(NotHeard::AnotherTable));
        assert_eq!(receive_continues(&word, &TABLE, &game, seat_of, NOW + CLOCK_SLACK_MS + 1), Err(NotHeard::Stale));
        let mut broken = word.clone();
        let at = broken.len() / 3;
        broken[at] ^= 0xff;
        assert!(receive_continues(&broken, &TABLE, &game, seat_of, NOW).is_err());
        let hearing = hearing_word(&key(2), &TABLE, 9, &[], NOW).unwrap();
        assert!(matches!(receive_continues(&hearing, &TABLE, &game, seat_of, NOW), Err(NotHeard::Malformed(_))));
    }

    /// `D-060`: a seat's word on whom it cannot hear is heard from its own seat,
    /// for its table and roster, on time -- and its list is a list of other seats
    /// of a table, or nothing is heard.
    #[test]
    fn a_hearing_word_is_heard_from_its_seat_with_a_list_of_other_seats() {
        let word = hearing_word(&key(2), &TABLE, 7, &[5, 0, 5], NOW).unwrap();
        assert_eq!(
            receive_hearing(&word, &TABLE, seat_of, NOW),
            Ok(Hearing { seat: 2, list_serial: 7, unheard: vec![0, 5], said_ms: NOW }),
            "sorted, and each seat once"
        );
        let all = hearing_word(&key(5), &TABLE, 7, &[], NOW).unwrap();
        assert_eq!(receive_hearing(&all, &TABLE, seat_of, NOW).map(|h| h.unheard), Ok(vec![]));
        let stranger = hearing_word(&key(9), &TABLE, 7, &[2], NOW).unwrap();
        assert_eq!(receive_hearing(&stranger, &TABLE, seat_of, NOW), Err(NotHeard::NotASeat));
        let elsewhere = hearing_word(&key(5), &[8u8; 32], 7, &[2], NOW).unwrap();
        assert_eq!(
            receive_hearing(&elsewhere, &TABLE, seat_of, NOW),
            Err(NotHeard::AnotherTable)
        );
        let itself = hearing_word(&key(5), &TABLE, 7, &[5], NOW).unwrap();
        assert!(matches!(
            receive_hearing(&itself, &TABLE, seat_of, NOW),
            Err(NotHeard::Malformed(_))
        ));
        let no_such_seat = hearing_word(&key(5), &TABLE, 7, &[12], NOW).unwrap();
        assert!(matches!(
            receive_hearing(&no_such_seat, &TABLE, seat_of, NOW),
            Err(NotHeard::Malformed(_))
        ));
        let old = hearing_word(&key(5), &TABLE, 7, &[2], NOW).unwrap();
        assert_eq!(
            receive_hearing(&old, &TABLE, seat_of, NOW + CLOCK_SLACK_MS + 1),
            Err(NotHeard::Stale)
        );
        let leave = leave_word(&key(5), &TABLE, NOW).unwrap();
        assert!(matches!(
            receive_hearing(&leave, &TABLE, seat_of, NOW),
            Err(NotHeard::Malformed(_))
        ));
        let mut broken = hearing_word(&key(5), &TABLE, 7, &[2], NOW).unwrap();
        let at = broken.len() / 2;
        broken[at] ^= 0xff;
        assert!(receive_hearing(&broken, &TABLE, seat_of, NOW).is_err());
    }

    #[test]
    fn a_line_round_trips_under_its_seat() {
        let bytes = say(&key(2), &TABLE, "Bob", "nice hand", NOW).unwrap();
        let mut talk = Talk::default();
        let said = hear(&bytes, &CARRIER, NOW, &mut talk).unwrap();
        assert_eq!(said.seat, 2);
        assert_eq!(said.who, key(2).verifying_key().to_bytes());
        assert_eq!(said.nickname, "Bob");
        assert_eq!(said.text, "nice hand");
    }

    /// A key with no seat has no voice at the table, whatever name it types.
    #[test]
    fn a_stranger_is_not_heard() {
        let bytes = say(&key(9), &TABLE, "Bob", "let me in", NOW).unwrap();
        let mut talk = Talk::default();
        assert_eq!(hear(&bytes, &CARRIER, NOW, &mut talk), Err(NotHeard::NotASeat));
    }

    /// A line said at one table is not a line at another, however the bytes
    /// got there.
    #[test]
    fn a_line_from_another_table_is_dropped() {
        let bytes = say(&key(2), &[8u8; 32], "Bob", "wrong room", NOW).unwrap();
        let mut talk = Talk::default();
        assert_eq!(hear(&bytes, &CARRIER, NOW, &mut talk), Err(NotHeard::AnotherTable));
    }

    /// The signature covers the envelope, so a byte changed under it fails.
    #[test]
    fn a_tampered_line_is_refused() {
        let bytes = say(&key(5), &TABLE, "Erin", "hello", NOW).unwrap();
        let mut broken = bytes.clone();
        let at = broken.len() / 2;
        broken[at] ^= 0xff;
        let mut talk = Talk::default();
        assert!(hear(&broken, &CARRIER, NOW, &mut talk).is_err());
    }

    /// `D-054`: **a forgery must not spend the budget of the seat it names.**
    ///
    /// The key in the envelope is public and sits in every roster. Charging a
    /// seat's allowance before the signature was checked -- which is what this
    /// module did -- meant anybody who could put bytes in front of this client
    /// could silence any player at the table: one unsigned line under Alice's
    /// key every two seconds and Alice's own lines were dropped as *too much*,
    /// with nothing in any window to say why. `lobby.rs` had the same bug
    /// against table adverts, and the rule there is the rule here: a key that
    /// has not been verified is a claim, and a claim spends nothing.
    #[test]
    fn a_forged_line_cannot_silence_the_seat_it_names() {
        let mut talk = Talk::default();
        let honest = say(&key(2), &TABLE, "Bob", "my own line", NOW).unwrap();
        let mut forged = say(&key(2), &TABLE, "Bob", "not my line", NOW).unwrap();
        let at = forged.len() - 1;
        forged[at] ^= 0xff;
        assert_eq!(
            hear(&forged, &OTHER_CARRIER, NOW, &mut talk),
            Err(NotHeard::Forged),
            "the forgery is refused"
        );
        assert!(
            hear(&honest, &CARRIER, NOW, &mut talk).is_ok(),
            "and seat 2 still has everything it had"
        );
    }

    /// The carrier's own budget is charged first, so a stream of rubbish costs
    /// its sender and not this client: nothing of the message is read once it
    /// is spent.
    #[test]
    fn the_carrier_pays_before_a_byte_is_read() {
        let mut talk = Talk::default();
        let junk = vec![0xffu8; 300];
        let mut refused = 0;
        for i in 0..10u64 {
            if hear(&junk, &CARRIER, NOW + i, &mut talk) == Err(NotHeard::TooMuchCarried) {
                refused += 1;
            }
        }
        assert!(refused >= 6, "a burst and then nothing: {refused} of ten refused");
        let bytes = say(&key(5), &TABLE, "Erin", "hello", NOW).unwrap();
        assert!(
            hear(&bytes, &OTHER_CARRIER, NOW, &mut talk).is_ok(),
            "and another member's budget is its own"
        );
    }

    /// A burst of a real exchange survives; a stream does not.
    #[test]
    fn a_seat_may_say_three_at_once_and_then_one_every_two_seconds() {
        let mut talk = Talk::default();
        for i in 0..u64::from(LINE_BURST) {
            let bytes = say(&key(2), &TABLE, "Bob", &format!("line {i}"), NOW + i * 10).unwrap();
            assert!(hear(&bytes, &CARRIER, NOW + i * 10, &mut talk).is_ok(), "burst line {i}");
        }
        let over = say(&key(2), &TABLE, "Bob", "one too many", NOW + 40).unwrap();
        assert_eq!(hear(&over, &CARRIER, NOW + 40, &mut talk), Err(NotHeard::TooMuchCarried));
        let later = say(&key(2), &TABLE, "Bob", "and now", NOW + LINE_EVERY_MS + 40).unwrap();
        assert!(hear(&later, &CARRIER, NOW + LINE_EVERY_MS + 40, &mut talk).is_ok());
    }

    /// The count of lines says nothing about what they weigh, so the bytes are
    /// counted too -- what keeps chat from crowding out a hand while every
    /// line of it is legal.
    #[test]
    fn a_minute_of_chat_is_bounded_in_bytes() {
        let mut budget = Budget::default();
        let mut spent = 0u64;
        let mut at = NOW;
        // One line of chat every two seconds for just under the window: as
        // fast as the line budget allows, so what stops it is the bytes.
        for _ in 0..(CHAT_BYTES_MS / LINE_EVERY_MS - 1) {
            if budget.admit(at, 400) {
                spent += 400;
            }
            at += LINE_EVERY_MS;
        }
        assert!(spent <= CHAT_BYTES_MAX, "{spent} bytes in a minute");
        assert!(spent >= CHAT_BYTES_MAX / 2, "and not so tight nobody can speak: {spent}");
        assert!(
            budget.admit(NOW + CHAT_BYTES_MS * 2, 400),
            "a minute on, the window has rolled and there is room again"
        );
    }

    #[test]
    fn a_stale_line_and_an_empty_one_are_refused() {
        let mut talk = Talk::default();
        let old = say(&key(2), &TABLE, "Bob", "from long ago", NOW - CLOCK_SLACK_MS * 2).unwrap();
        assert_eq!(hear(&old, &CARRIER, NOW, &mut talk), Err(NotHeard::Stale));
        assert!(say(&key(2), &TABLE, "Bob", "   ", NOW).is_err());
    }

    /// The line is capped in bytes on a character boundary, so what is said
    /// is what arrives.
    #[test]
    fn a_long_line_is_cut_where_the_wire_cuts_it() {
        let long = "a".repeat(200);
        let bytes = say(&key(2), &TABLE, "Bob", &long, NOW).unwrap();
        let mut talk = Talk::default();
        let said = hear(&bytes, &CARRIER, NOW, &mut talk).unwrap();
        assert!(said.text.len() <= SAID_MAX);
        assert_eq!(said.text, clip_line(&long));
    }

    /// `D-054`: a message is one line of printable text. What a rogue client
    /// writes instead -- a screenful of newlines inside one message's budget, a
    /// direction override, a stack of marks drawn over the window -- is refused
    /// and counted, and what this client says is cleaned before it is signed so
    /// nothing honest ever trips it.
    #[test]
    fn a_line_that_is_not_one_line_of_text_is_refused() {
        for (raw, why) in [
            ("one\ntwo", NotPlain::Control),
            ("hello \u{202E}dlrow", NotPlain::Deceiving),
            ("a\u{0301}\u{0301}\u{0301}\u{0301}\u{0301}\u{0301}", NotPlain::Stacked),
        ] {
            let mut talk = Talk::default();
            let bytes = forge_body(&key(2), "Bob", raw, &TABLE, NOW);
            assert_eq!(
                hear(&bytes, &CARRIER, NOW, &mut talk),
                Err(NotHeard::NotPlain(why)),
                "{raw:?}"
            );
            let mut talk = Talk::default();
            let honest = say(&key(2), &TABLE, "Bob", raw, NOW).unwrap();
            assert!(hear(&honest, &CARRIER, NOW, &mut talk).is_ok(), "cleaned: {raw:?}");
        }
        let mut talk = Talk::default();
        let bytes = forge_body(&key(2), "Bob\nthe\nsecond", "hi", &TABLE, NOW);
        assert_eq!(
            hear(&bytes, &CARRIER, NOW, &mut talk),
            Err(NotHeard::NotPlain(NotPlain::Control)),
            "the name is text too, so a name cannot draw over the pane"
        );
    }

    /// `D-054`: what a refusal is worth against `D-051`'s meter -- an eager
    /// speaker is not a forger.
    #[test]
    fn a_spent_budget_is_the_smallest_noise_and_a_forgery_is_not() {
        use crate::table::membership::{NOISE_BAD_FRAGMENT, NOISE_BAD_MESSAGE, NOISE_LIMIT};
        assert_eq!(noise_points(NotHeard::TooMuch), NOISE_BAD_FRAGMENT);
        assert_eq!(noise_points(NotHeard::TooMuchCarried), NOISE_BAD_FRAGMENT);
        assert_eq!(noise_points(NotHeard::Stale), NOISE_BAD_FRAGMENT);
        assert_eq!(noise_points(NotHeard::Forged), NOISE_BAD_MESSAGE);
        assert_eq!(noise_points(NotHeard::NotASeat), NOISE_BAD_MESSAGE);
        assert_eq!(noise_points(NotHeard::NotPlain(NotPlain::Control)), NOISE_BAD_MESSAGE);
        // A client spending its chat budget over and over is cut off by the
        // same measure as one filling the group with junk -- it just takes
        // longer, which is the difference between eager and hostile.
        assert!(NOISE_BAD_FRAGMENT * 16 >= NOISE_LIMIT);
    }

    /// A line whose body was written by a client that does not clean.
    fn forge_body(
        k: &SigningKey,
        nickname: &str,
        text: &str,
        table_id: &[u8; 32],
        now_ms: u64,
    ) -> Vec<u8> {
        let body = Body {
            nickname: nickname.to_owned(),
            text: text.to_owned(),
            table_id: *table_id,
        };
        let body_bytes = to_canonical(&body).unwrap();
        let envelope = EventBody::unchained(
            EventType::TableChat,
            k.verifying_key().to_bytes(),
            body_bytes,
            now_ms,
        )
        .unwrap();
        let envelope_bytes = to_canonical(&envelope).unwrap();
        let signature = {
            use ed25519_dalek::Signer;
            k.sign(&to_be_signed(&envelope_bytes))
        };
        to_canonical(&SignedEvent {
            body: envelope_bytes,
            signature: signature.to_bytes(),
        })
        .unwrap()
    }
}
