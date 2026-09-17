//! `D-064`: the automatic search for a game -- the matchmaker.
//!
//! The owner's design (2026-09-17): a player presses one button, chooses a
//! table format and how many games they want to play at once, and the client
//! finds the games by itself. It reserves seats at several forming tables at
//! once, founds one of its own when the lobby offers nothing, lets a table it
//! founded start with the players that came once nobody else does, and the
//! moment one of its tables starts it gives every other seat back.
//!
//! # What this module is, and what it is not
//!
//! This module is a **state machine and nothing else**: no socket, no clock of
//! its own, no table. The node loop hands it what it can see -- the lobby's
//! adverts, the slots it holds, the network's state, the queue -- and takes
//! back a list of [`Step`]s: join this table, found one, leave that one, tell
//! the window this. Every step is one the node already knows how to do for a
//! player pressing a button, so the search cannot reach a table any way a
//! player could not, and every rule here has a test that needs no network.
//!
//! The wire part -- [`presence`] and [`receive_presence`] -- is the one new
//! message, `SEARCH_PRESENCE` (`PROTOCOL.md` §7.13): a searching client says so
//! on the queue topic every `SEARCH_PRESENCE_EVERY_MS`, the mirror of
//! `LOBBY_PLAYER_PRESENCE`, so every client can count who is looking. It is
//! signed, capped, rate-limited and expired exactly as the lobby's presence
//! is, and it is **never an input to any game decision**: it moves a number
//! in a window and decides who founds first, and a forged one would move that
//! number by one.
//!
//! # The rules, in the order the tick applies them
//!
//! 1. **Reservations are real seats.** A reservation is a `JOIN_REQUEST` to a
//!    forming Sit & Go, and the seat it earns is held like any other seat: in
//!    the table's group, ratified when the table is ready. Nothing lighter
//!    exists in the protocol, and nothing lighter would let the table start
//!    the moment it is full.
//! 2. **Dynamic queue expansion.** A search begins reserving at up to
//!    `DQE_START` tables and may hold one more every `dqe_step` of waiting,
//!    up to the format's cap. A heads-up seat costs the client one small group;
//!    a ten-seat one costs it a group whose asking for missing messages grows
//!    with the seats (`D-063` point 4: 0.06 a second at three seats, 1.3 at
//!    ten), so the bigger formats expand more slowly and stop lower.
//! 3. **Capacity adaptation.** A table this client founded for the search
//!    advertises `min_players_to_start = 2` and starts at [`capacity_now`]:
//!    every `CAPACITY_STEP` it has stood unfilled the founder needs one seat
//!    fewer, down to two. The advert never changes -- a founder that re-signed
//!    its parameters would make its own table unjoinable (`§7.2` rule 7) --
//!    only the founder's own rule for when it says it is ready does.
//! 4. **Arming, and why a joiner does not ratify everything it holds.** The
//!    founder ratifies last (`S1-HE`), so a table cannot start without every
//!    seat's ratification -- and a seat's ratification is therefore its
//!    consent to start *now*. A search holding five reservations with one game
//!    wanted ratifies at most one of them at a time: the one closest to
//!    starting, which it keeps for `COMMIT_TTL` before it may prefer another.
//!    Two tables cannot start under it in the same second, so it never has to
//!    walk out of a game that has begun.
//! 5. **Seats flow to the lower key.** Two searchers who both found a table
//!    would each join the other's, and two half tables would start together.
//!    So a client that founded its own table joins another search table only
//!    when that founder's key is lower than its own: every seat flows one way
//!    and the lowest key's table fills first. And a client founds only after
//!    `found_after`, which grows with the number of searchers of its format
//!    whose key is lower -- the lowest founds first, the rest see its table
//!    inside a lobby round and join it instead.
//! 6. **The goal, and the cleanup.** When as many games as asked for are
//!    running -- tables the search started and tables the player sat down at
//!    by hand both count -- every other reservation is left by the player's
//!    own signed word (`TABLE_LEAVE`, free before the set, a resignation after
//!    it under `D-063`), the queue is told, and the window is told which slots
//!    are games now. A cancel does the same without the games.
//! 7. **Every wait is bounded.** A join nobody answers is given up after
//!    `join_wait`; a founder that cannot be heard for `founder_patience` is
//!    left; a table that refused this client is avoided for a while, twice as
//!    long each time; the search itself ends after `SEARCH_MAX`. The bounds
//!    stretch with the network -- a relayed client waits longer for the same
//!    answer -- and never with each other.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use minicbor::{Decode, Encode};

use super::lobby::RateLimiter;
use crate::protocol::constants::{SEARCH_PRESENCE_MAX, SEARCH_PRESENCE_TTL_MS};
use crate::protocol::messages::{EventBody, EventType, SignedEvent};
use crate::protocol::serialization::{from_canonical, to_canonical};
use crate::protocol::signatures::to_be_signed;

// ---------------------------------------------------------------------------
// What the player asks for
// ---------------------------------------------------------------------------

/// The table format a search looks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Format {
    /// Two seats.
    HeadsUp,
    /// Six seats.
    SixMax,
    /// Ten seats: the full ring.
    FullRing,
    /// Whatever tournament table fills first, of any size.
    Any,
}

impl Format {
    pub const ALL: [Format; 4] = [Format::HeadsUp, Format::SixMax, Format::FullRing, Format::Any];

    /// The wire code, and the settings file's: `0` is *not searching*.
    pub const fn code(self) -> u8 {
        match self {
            Format::HeadsUp => 1,
            Format::SixMax => 2,
            Format::FullRing => 3,
            Format::Any => 4,
        }
    }

    pub const fn parse(code: u8) -> Option<Format> {
        match code {
            1 => Some(Format::HeadsUp),
            2 => Some(Format::SixMax),
            3 => Some(Format::FullRing),
            4 => Some(Format::Any),
            _ => None,
        }
    }

    /// The seats a table of this format has; `None` for any.
    pub const fn seats(self) -> Option<u8> {
        match self {
            Format::HeadsUp => Some(2),
            Format::SixMax => Some(6),
            Format::FullRing => Some(10),
            Format::Any => None,
        }
    }

    /// Whether a table of `seats` seats is one this format takes.
    pub const fn accepts(self, seats: u8) -> bool {
        match self.seats() {
            Some(n) => n == seats,
            None => seats >= 2,
        }
    }

    /// Whether two searches could sit at one table.
    pub const fn compatible(self, other: Format) -> bool {
        matches!(self, Format::Any) || matches!(other, Format::Any) || self.code() == other.code()
    }

    pub const fn label(self) -> &'static str {
        match self {
            Format::HeadsUp => "Heads-Up (2-max)",
            Format::SixMax => "6-Max",
            Format::FullRing => "Full ring (10-max)",
            Format::Any => "Automatic",
        }
    }

    /// The most tables a search of this format may hold seats at, and how
    /// long it waits before holding one more. Heads-up and *any* expand at
    /// the owner's pace; the bigger formats more slowly, because every seat
    /// held is a group this client talks in, and a ten-seat group asks for
    /// missing messages ten times as often as a three-seat one.
    pub const fn expansion(self) -> (u8, Duration) {
        match self {
            Format::HeadsUp | Format::Any => (DQE_MAX, Duration::from_secs(30)),
            Format::SixMax => (8, Duration::from_secs(45)),
            Format::FullRing => (6, Duration::from_secs(60)),
        }
    }
}

/// What the player asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchRequest {
    /// The window's number for this search; every report and the end carry
    /// it, so a report of a search already given up is read by nobody.
    pub id: u32,
    pub format: Format,
    /// How many games the player wants to play at once, `1..=MAX_GAMES`.
    pub tables: u8,
    /// The owner's bonus: search again when a game this search started ends.
    pub again: bool,
}

/// The most games one search may aim at: the node's slots for played tables.
pub const MAX_GAMES: u8 = 4;

impl SearchRequest {
    /// The request with its numbers inside their bounds. What arrives from a
    /// window, a settings file or a command line is clamped, never refused:
    /// a search for zero games is a search for one.
    pub fn checked(mut self) -> SearchRequest {
        self.tables = self.tables.clamp(1, MAX_GAMES);
        self
    }
}

/// The lobby's commands to the matchmaker, as the owner named them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LobbyCommand {
    PlayerStartSearch(SearchRequest),
    PlayerCancelSearch,
}

// ---------------------------------------------------------------------------
// The constants
// ---------------------------------------------------------------------------

/// How many tables a search reserves seats at from the start.
pub const DQE_START: u8 = 4;
/// The most tables any search reserves seats at.
pub const DQE_MAX: u8 = 10;
/// `D-064` amended (the owner): an *any* search founds the biggest table
/// and lets the queue say how fast its founder comes down.
pub const AUTO_SEATS: u8 = 10;
/// How long a table this client founded for the search stands unfilled
/// before its founder needs one seat fewer: `STEP_FAST` when nobody waits in
/// the queue, `STEP_SLOW` when enough wait to fill it, between the two in
/// proportion (`capacity_step`).
pub const STEP_FAST: Duration = Duration::from_secs(30);
pub const STEP_SLOW: Duration = Duration::from_secs(180);
/// A queue read within this long of the first poker peer on the line is a
/// queue not heard yet -- unknown, not empty. The gossip mesh takes about
/// that long to carry the first presence, and a client that read the silence
/// as *nobody waits* founded heads-up tables for four players (`S1-HK`).
pub const QUEUE_WARM_S: u64 = 40;
/// A search that has heard no queue and had no poker peer for this long
/// founds anyway: alone in the world, it should still put a table up.
pub const QUEUE_WAIT_MAX: Duration = Duration::from_secs(60);
/// An *any* search forgives a table this many seconds of estimate per seat
/// above two: the bigger table is preferred while it is not much slower.
pub const SEAT_PREFERENCE_S: u64 = 10;
/// An advert older than this is a table that may be gone: not reserved at.
pub const OFFER_FRESH_MS: u64 = 90_000;
/// How long a reservation stays armed -- its ratification allowed -- before
/// the search may prefer another table that is closer to starting.
pub const COMMIT_TTL: Duration = Duration::from_secs(45);
/// How long a table that refused this client, or answered nothing, is left
/// alone the first time; doubled at every repeat up to `AVOID_MAX`. Short the
/// first time: a founder not yet dialled answers nothing for a while and is
/// the only table there is (`search190453-4`: a client that lost both tables
/// for two minutes over one unanswered join).
pub const AVOID_FIRST: Duration = Duration::from_secs(30);
pub const AVOID_MAX: Duration = Duration::from_secs(600);
/// A search that found nothing in this long ends, and says so: a client
/// forgotten with a search on would otherwise hold seats at other people's
/// tables for the night.
pub const SEARCH_MAX: Duration = Duration::from_secs(30 * 60);
/// How often the window is told where the search stands.
pub const REPORT_EVERY: Duration = Duration::from_secs(1);
/// How often the log is.
pub const LOG_EVERY: Duration = Duration::from_secs(10);
/// The seconds one more seat takes to arrive at a forming table, until the
/// search has measured it.
pub const ARRIVAL_DEFAULT_S: u64 = 45;
/// A founder reached through a relay costs every seat a slower group: its
/// table is read as this much further from starting.
pub const RELAY_PENALTY_S: u64 = 20;
/// The name a table founded for the search carries, so a search reads its
/// capacity rule off the advert. Display data like every name: a player who
/// names a table so by hand gets a table the search treats as one of its own,
/// which is a table that starts short of full only if its founder lets it.
pub const SEARCH_TABLE_PREFIX: &str = "Auto Sit & Go ";

/// The most searchers one client remembers; the oldest heard goes first.
pub const MAX_TRACKED_SEARCHERS: usize = 2_048;

// ---------------------------------------------------------------------------
// The wire: SEARCH_PRESENCE
// ---------------------------------------------------------------------------

/// `PROTOCOL.md` §7.13: `n(0) state` (`0` stopped, else `Format::code`),
/// `n(1) tables`, `n(2) since_unix_ms` -- when the search began, on the
/// speaker's clock, so listeners can say how long the queue has waited.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
struct PresenceBody {
    #[n(0)]
    state: u8,
    #[n(1)]
    tables: u8,
    #[n(2)]
    since_unix_ms: u64,
}

/// The body's own cap, well under the message's.
const PRESENCE_BODY_MAX: usize = 64;

/// Why a presence was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotHeard {
    TooLong,
    /// This client's own budget for the neighbour or the author: the message
    /// is not judged, only not taken.
    TooMuch,
    Malformed(&'static str),
    Stale,
    Forged,
}

/// A presence taken off the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heard {
    pub who: [u8; 32],
    /// `None`: the speaker stopped searching.
    pub format: Option<Format>,
    pub tables: u8,
    pub since_unix_ms: u64,
}

/// Say that this client searches (`Some(format)`), or that it stopped
/// (`None`), signed by its application key.
pub fn presence(
    key: &SigningKey,
    format: Option<Format>,
    tables: u8,
    since_unix_ms: u64,
    now_ms: u64,
) -> Result<Vec<u8>, &'static str> {
    let body = PresenceBody {
        state: format.map_or(0, Format::code),
        tables: tables.clamp(1, MAX_GAMES),
        since_unix_ms,
    };
    let body_bytes = to_canonical(&body).map_err(|_| "the body does not encode")?;
    if body_bytes.len() > PRESENCE_BODY_MAX {
        return Err("over the cap");
    }
    let envelope = EventBody::unchained(EventType::SearchPresence, key.verifying_key().to_bytes(), body_bytes, now_ms)
        .ok_or("a search presence is an unchained event")?;
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

/// Take a presence off the wire: the neighbour's budget first, then the
/// shape, then the clock, then the signature, then the author's budget -- the
/// order `lobbytalk::receive` uses, for the same reason: nothing a stranger
/// sends may cost this client a signature check it did not budget for.
pub fn receive_presence(
    bytes: &[u8],
    from_peer: [u8; 32],
    now_ms: u64,
    limits: &mut RateLimiter,
) -> Result<Heard, NotHeard> {
    if bytes.len() > SEARCH_PRESENCE_MAX {
        return Err(NotHeard::TooLong);
    }
    if !limits.admit_peer_talk(from_peer, now_ms) {
        return Err(NotHeard::TooMuch);
    }
    let signed: SignedEvent =
        from_canonical(bytes, SEARCH_PRESENCE_MAX).map_err(|_| NotHeard::Malformed("not a signed event"))?;
    let envelope: EventBody =
        from_canonical(&signed.body, SEARCH_PRESENCE_MAX).map_err(|_| NotHeard::Malformed("not an envelope"))?;
    let kind = EventType::try_from(envelope.event_type)
        .map_err(|_| NotHeard::Malformed("an event type this client does not know"))?;
    if kind != EventType::SearchPresence {
        return Err(NotHeard::Malformed("not a search presence"));
    }
    if envelope.emitted_at_unix_ms.abs_diff(now_ms) > super::lobbytalk::CLOCK_SLACK_MS {
        return Err(NotHeard::Stale);
    }
    let who = envelope.sender_public_key;
    verify(&who, &signed).map_err(|_| NotHeard::Forged)?;
    if !limits.admit_author_queue(who, now_ms) {
        return Err(NotHeard::TooMuch);
    }
    let body: PresenceBody =
        from_canonical(&envelope.payload, PRESENCE_BODY_MAX).map_err(|_| NotHeard::Malformed("not a presence body"))?;
    let format = match body.state {
        0 => None,
        code => Some(Format::parse(code).ok_or(NotHeard::Malformed("a format this client does not know"))?),
    };
    if body.tables == 0 || body.tables > MAX_GAMES {
        return Err(NotHeard::Malformed("a games count outside its bounds"));
    }
    Ok(Heard {
        who,
        format,
        tables: body.tables,
        since_unix_ms: body.since_unix_ms,
    })
}

fn verify(who: &[u8; 32], signed: &SignedEvent) -> Result<(), ()> {
    use ed25519_dalek::{Signature, VerifyingKey};
    let key = VerifyingKey::from_bytes(who).map_err(|_| ())?;
    let sig = Signature::from_bytes(&signed.signature);
    key.verify_strict(&to_be_signed(&signed.body), &sig).map_err(|_| ())
}

// ---------------------------------------------------------------------------
// The queue: who else is searching
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Waiting {
    format: Format,
    tables: u8,
    since_unix_ms: u64,
    heard_ms: u64,
}

/// The searchers this client has heard of, by application key, each until
/// its presence is `SEARCH_PRESENCE_TTL_MS` old or it says it stopped.
#[derive(Debug, Clone, Default)]
pub struct Queue {
    entries: BTreeMap<[u8; 32], Waiting>,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take a presence. Returns whether the count of searchers changed.
    pub fn note(&mut self, heard: Heard, now_ms: u64) -> bool {
        match heard.format {
            None => self.entries.remove(&heard.who).is_some(),
            Some(format) => {
                let fresh = !self.entries.contains_key(&heard.who);
                if fresh && self.entries.len() >= MAX_TRACKED_SEARCHERS {
                    // The oldest heard goes, so a flood of keys cannot hold the
                    // book and a live searcher is never the one dropped.
                    if let Some(oldest) = self.entries.iter().min_by_key(|(_, w)| w.heard_ms).map(|(k, _)| *k) {
                        self.entries.remove(&oldest);
                    }
                }
                self.entries.insert(
                    heard.who,
                    Waiting {
                        format,
                        tables: heard.tables,
                        // A clock ahead of ours says nothing about the wait.
                        since_unix_ms: heard.since_unix_ms.min(now_ms),
                        heard_ms: now_ms,
                    },
                );
                fresh
            }
        }
    }

    /// Forget presences older than their TTL. Returns how many went.
    pub fn expire(&mut self, now_ms: u64) -> usize {
        let before = self.entries.len();
        self.entries.retain(|_, w| now_ms.saturating_sub(w.heard_ms) < SEARCH_PRESENCE_TTL_MS);
        before - self.entries.len()
    }

    /// How many others search for a table `format` could share; every
    /// searcher when `format` is `None`.
    pub fn count(&self, format: Option<Format>, me: &[u8; 32]) -> u32 {
        self.entries
            .iter()
            .filter(|(k, w)| *k != me && format.is_none_or(|f| f.compatible(w.format)))
            .count()
            .try_into()
            .unwrap_or(u32::MAX)
    }

    /// How long the others of `format` have been waiting, on average.
    pub fn mean_wait_s(&self, format: Format, now_ms: u64, me: &[u8; 32]) -> Option<u64> {
        let waits: Vec<u64> = self
            .entries
            .iter()
            .filter(|(k, w)| *k != me && format.compatible(w.format))
            .map(|(_, w)| now_ms.saturating_sub(w.since_unix_ms) / 1_000)
            .collect();
        if waits.is_empty() {
            None
        } else {
            Some(waits.iter().sum::<u64>() / waits.len() as u64)
        }
    }

    /// How many searchers of a compatible format have a lower key than this
    /// client: its rank in the order that founds tables.
    pub fn rank_below(&self, format: Format, me: &[u8; 32]) -> usize {
        self.entries
            .iter()
            .filter(|(k, w)| *k < me && format.compatible(w.format))
            .count()
    }

    /// The seats an *any* search founds: the format most searchers named --
    /// they take no other -- or, where nobody named one, `AUTO_SEATS`: the
    /// biggest table, whose founder comes down as fast as the queue says
    /// (`capacity_step`). Founding heads-up for a queue that read empty made
    /// heads-up games of four players (`S1-HK`).
    pub fn demand_seats(&self, me: &[u8; 32]) -> u8 {
        let mut want = [(Format::SixMax, 0usize), (Format::HeadsUp, 0), (Format::FullRing, 0)];
        for (k, w) in self.entries.iter() {
            if k == me {
                continue;
            }
            for (f, n) in want.iter_mut() {
                if w.format == *f {
                    *n += 1;
                }
            }
        }
        want.iter()
            .filter(|(_, n)| *n > 0)
            .max_by_key(|(f, n)| (*n, tie(*f)))
            .and_then(|(f, _)| f.seats())
            .unwrap_or(AUTO_SEATS)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ---------------------------------------------------------------------------
// What the node hands in
// ---------------------------------------------------------------------------

/// A table the lobby offers, as the search reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub key: [u8; 32],
    pub name: String,
    pub seats: u8,
    pub players: u8,
    pub min_players: u8,
    pub founder: [u8; 32],
    /// What a seat costs, which for a tournament is its stack.
    pub buyin: u64,
    /// When this client first saw the table, by its own clock.
    pub first_seen_ms: u64,
    /// Heard of within `OFFER_FRESH_MS`.
    pub fresh: bool,
    /// Open, and its rules unchanged under a live advert.
    pub joinable: bool,
    pub tournament: bool,
    pub password: bool,
    /// The founder is reached through a relay from here.
    pub relayed: bool,
}

/// One of the node's slots, as the search reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotReading {
    pub slot: u8,
    /// The table the slot holds, if any.
    pub key: Option<[u8; 32]>,
    pub name: String,
    /// Opened by the search.
    pub search: bool,
    pub founder: bool,
    pub players: u8,
    pub seats: u8,
    /// The table is set: a game.
    pub set: bool,
    /// The tournament at it is over -- a table the window still shows, and
    /// no game the player is in (`S1-HJ`).
    pub over: bool,
    /// The table was lost without the player asking.
    pub lost: bool,
    /// The seat was given back and is asked for again.
    pub asking_again: bool,
    /// How long the founder has been unreachable, before the set.
    pub founder_gone_s: Option<u64>,
}

/// The network as the search's timeouts read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NetReading {
    pub poker_peers: usize,
    /// This client is reached through a relay only.
    pub relayed: bool,
    /// The table carrier's line is down, or nobody is connected.
    pub line_down: bool,
    /// AutoNAT's verdict, if any.
    pub public: Option<bool>,
    pub relay: bool,
    /// Seconds since the first poker peer came on the line; `None` before one.
    pub on_line_s: Option<u64>,
}

impl NetReading {
    /// `S1-HK`: whether the queue's silence means anything yet: a presence
    /// was heard, or the line has carried a poker peer for `QUEUE_WARM_S`, or
    /// the search has waited `QUEUE_WAIT_MAX` for either.
    pub fn queue_known(&self, heard: bool, elapsed: Duration) -> bool {
        heard || self.on_line_s.is_some_and(|s| s >= QUEUE_WARM_S) || elapsed >= QUEUE_WAIT_MAX
    }

    /// How long a join is waited for: a relayed client's request and answer
    /// both cross a relay, and a client with almost nobody on the line may be
    /// asking a founder it has not yet dialled.
    pub fn join_wait(&self) -> Duration {
        let mut s = 20;
        if self.relayed {
            s += 15;
        }
        if self.poker_peers < 3 {
            s += 10;
        }
        Duration::from_secs(s.min(60))
    }

    /// How long a reservation waits for a founder it cannot hear before the
    /// search looks elsewhere.
    pub fn founder_patience(&self) -> Duration {
        Duration::from_secs(if self.relayed { 75 } else { 45 })
    }

    /// How long a search looks at what the lobby offers before founding a
    /// table of its own, at `rank` in the founding order (see the module
    /// doc): the lowest key founds after ten seconds, the next fifteen later,
    /// and a client whose lobby answers are slow to arrive waits a little
    /// more for them.
    pub fn found_after(&self, rank: usize) -> Duration {
        let mut s = 10 + 15 * rank.min(4) as u64;
        if self.relayed {
            s += 10;
        }
        Duration::from_secs(s)
    }

    /// The one line the window shows about the network while it searches.
    pub fn warning(&self) -> Option<&'static str> {
        if self.line_down {
            Some("Connection problem -- reconnecting")
        } else if self.poker_peers == 0 {
            Some("No other player on the line yet")
        } else if self.public == Some(false) && !self.relay {
            Some("Behind NAT and no relay found -- tables may not be reachable")
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// What the node takes out
// ---------------------------------------------------------------------------

/// One thing the node does for the search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Ask for a seat at this table.
    Join { key: [u8; 32], buyin: u64 },
    /// Found a Sit & Go of this many seats, starting at two, under this name.
    Found { seats: u8, name: String },
    /// Give this seat back, by the player's own signed word.
    Leave { key: [u8; 32], why: String },
    /// Say on the queue topic that this client searches, or stopped.
    Presence(Option<Format>),
    /// Where the search stands, for the window.
    Report(SearchReport),
    /// The search is over: why, and the slots that are games now.
    Ended { id: u32, why: String, started: Vec<u8> },
    /// A line for the log.
    Log(String),
}

/// One reservation, as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservationView {
    pub slot: Option<u8>,
    pub key: Option<[u8; 32]>,
    pub name: String,
    pub players: u8,
    /// The seats the table needs now to start: its founder's capacity for a
    /// search table, every seat for another.
    pub capacity: u8,
    pub seats: u8,
    pub armed: bool,
    pub mine: bool,
}

/// Where the search stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchReport {
    pub id: u32,
    pub format: Format,
    pub elapsed_s: u64,
    /// The search's own estimate of the seconds to a game, from the table
    /// closest to starting.
    pub eta_s: Option<u64>,
    /// Other clients searching for a table this one could share.
    pub queue: u32,
    /// Whether that count is a reading or the silence before the first one.
    pub queue_known: bool,
    /// How long they have waited, on average.
    pub queue_wait_s: Option<u64>,
    pub reservations: Vec<ReservationView>,
    /// The most tables the search holds seats at right now (`DQE`).
    pub looking_at: u8,
    /// Games running, and how many the player asked for.
    pub running: u8,
    pub limit: u8,
    pub warning: Option<String>,
    pub phase: String,
}

// ---------------------------------------------------------------------------
// The arithmetic
// ---------------------------------------------------------------------------

/// How long a search table's founder waits before needing one seat fewer:
/// `waiting` searchers of a compatible format not seated at the table
/// against the `missing` seats -- nobody waiting, `STEP_FAST`; enough to fill
/// it, `STEP_SLOW`; between, in proportion. A queue not heard yet (`None`)
/// is read as the slow step: the silence is not *nobody*.
pub fn capacity_step(waiting: Option<u32>, missing: u8) -> Duration {
    let Some(w) = waiting else {
        return STEP_SLOW;
    };
    if missing == 0 {
        return STEP_SLOW;
    }
    let r = (f64::from(w) / f64::from(missing)).min(1.0);
    STEP_FAST + Duration::from_secs_f64((STEP_SLOW - STEP_FAST).as_secs_f64() * r)
}

/// The seats a search table's founder needs to start after standing `age`
/// unfilled: one fewer every `step`, never under two.
pub fn capacity_now(seats: u8, age: Duration, step: Duration) -> u8 {
    let steps = (age.as_secs() / step.as_secs().max(1)).min(u64::from(u8::MAX)) as u8;
    seats.saturating_sub(steps).max(2)
}

/// The most tables a search of `format` holds seats at after `elapsed`.
pub fn dqe_limit(format: Format, elapsed: Duration) -> u8 {
    let (cap, step) = format.expansion();
    let grown = (elapsed.as_secs() / step.as_secs()).min(u64::from(u8::MAX)) as u8;
    DQE_START.saturating_add(grown).min(cap)
}

/// The seconds a table is from starting: seats still to arrive at the
/// measured pace, or -- for a search table -- its founder's capacity reaching
/// the seats it has, whichever comes first. `None` for a table that cannot
/// start on its own: one seat, which no capacity reaches.
#[allow(clippy::too_many_arguments)]
pub fn eta_s(
    players: u8,
    seats: u8,
    search_table: bool,
    age: Duration,
    step: Duration,
    per_seat_s: u64,
    relayed: bool,
) -> Option<u64> {
    if players >= seats && seats >= 2 {
        return Some(if relayed { RELAY_PENALTY_S } else { 0 });
    }
    let missing = u64::from(seats.saturating_sub(players));
    let by_fill = missing * per_seat_s;
    let by_capacity = if search_table && players >= 2 {
        let need = u64::from(seats.saturating_sub(players)) * step.as_secs();
        Some(need.saturating_sub(age.as_secs()))
    } else {
        None
    };
    let eta = by_capacity.map_or(by_fill, |c| c.min(by_fill));
    Some(eta + if relayed { RELAY_PENALTY_S } else { 0 })
}

/// Whether a name is one the search gives the tables it founds.
pub fn is_search_table(name: &str, min_players: u8) -> bool {
    name.starts_with(SEARCH_TABLE_PREFIX) && min_players == 2
}

/// The name a search table of `seats` seats carries.
pub fn search_table_name(seats: u8) -> String {
    format!("{SEARCH_TABLE_PREFIX}{seats}-max")
}

/// The pace at which seats have arrived at this client's reservations.
#[derive(Debug, Clone, Default)]
struct Arrivals {
    /// The last few intervals between one seat and the next, in seconds.
    gaps: Vec<u64>,
}

impl Arrivals {
    fn note(&mut self, gap: Duration) {
        if self.gaps.len() >= 8 {
            self.gaps.remove(0);
        }
        self.gaps.push(gap.as_secs().clamp(10, 180));
    }

    fn per_seat_s(&self) -> u64 {
        if self.gaps.is_empty() {
            return ARRIVAL_DEFAULT_S;
        }
        let mut sorted = self.gaps.clone();
        sorted.sort_unstable();
        sorted[sorted.len() / 2]
    }
}

// ---------------------------------------------------------------------------
// The search itself
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Reservation {
    /// The table; `None` for a table this client is founding whose key the
    /// node has not said yet.
    key: Option<[u8; 32]>,
    name: String,
    slot: Option<u8>,
    asked: Instant,
    /// When the node first showed a seat here.
    seated: Option<Instant>,
    mine: bool,
    players: u8,
    seats: u8,
    /// A table founded for a search, whose founder starts at capacity.
    search_table: bool,
    /// When the table began, as far as this client knows: the founding, or
    /// the first sight of the advert.
    began: Instant,
    set: bool,
    /// Since when its ratification is allowed.
    armed: Option<Instant>,
    /// The seats last seen and when they changed, for the arrival pace.
    last_players: (u8, Instant),
    relayed: bool,
}

impl Reservation {
    /// The founder's step at this table: the searchers waiting, less the
    /// others already seated here (they search on until the game starts),
    /// against the seats missing.
    fn step(&self, waiting: Option<u32>) -> Duration {
        let here = u32::from(self.players.saturating_sub(1));
        capacity_step(waiting.map(|w| w.saturating_sub(here)), self.seats.saturating_sub(self.players))
    }

    fn capacity(&self, now: Instant, waiting: Option<u32>) -> u8 {
        if self.search_table {
            capacity_now(self.seats, now.saturating_duration_since(self.began), self.step(waiting))
        } else {
            self.seats
        }
    }

    /// Whether the table could start now, as far as this client can tell: a
    /// search table at its founder's capacity -- one seat under the estimate
    /// for a table another client founded, whose age this client saw only
    /// from the advert's first sight -- and any other table when full.
    fn ready(&self, now: Instant, waiting: Option<u32>) -> bool {
        if self.seated.is_none() || self.set || self.players < 2 {
            return false;
        }
        if self.search_table {
            let cap = self.capacity(now, waiting);
            let need = if self.mine { cap } else { cap.saturating_sub(1).max(2) };
            self.players >= need
        } else {
            self.players >= self.seats
        }
    }

    fn eta(&self, now: Instant, per_seat_s: u64, waiting: Option<u32>) -> Option<u64> {
        eta_s(
            self.players,
            self.seats,
            self.search_table,
            now.saturating_duration_since(self.began),
            self.step(waiting),
            per_seat_s,
            self.relayed,
        )
    }

    fn view(&self, now: Instant, waiting: Option<u32>) -> ReservationView {
        ReservationView {
            slot: self.slot,
            key: self.key,
            name: self.name.clone(),
            players: self.players,
            capacity: self.capacity(now, waiting),
            seats: self.seats,
            armed: self.armed.is_some(),
            mine: self.mine,
        }
    }
}

#[derive(Debug, Clone)]
struct Search {
    req: SearchRequest,
    since: Instant,
    since_unix_ms: u64,
    reservations: Vec<Reservation>,
    /// When this client last founded a table for this search, and whether it
    /// still holds it: one own table at a time, another only after the first
    /// is gone and `found_after` has passed again.
    founded_at: Option<Instant>,
    found_gone_at: Option<Instant>,
    last_report: Option<Instant>,
    last_log: Option<Instant>,
    arrivals: Arrivals,
    /// The slots whose tables set under this search.
    started: Vec<u8>,
    /// The searchers of a compatible format the queue holds, as last read;
    /// `None` while the queue is not heard yet (`S1-HK`).
    waiting: Option<u32>,
}

/// The matchmaker: at most one search at a time, and what it has learned
/// about tables to avoid.
#[derive(Debug, Clone, Default)]
pub struct Matchmaker {
    search: Option<Search>,
    /// Tables that refused this client or answered nothing: until when, and
    /// how many times.
    avoided: HashMap<[u8; 32], (Instant, u32)>,
}

/// Where the founder's start rule stands for a table this client founded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FounderGate {
    /// The seats the founder needs before it says it is ready.
    pub floor: u8,
    /// Whether it may say so at all now: the search wants a game and this
    /// table is the one armed for it.
    pub armed: bool,
}

impl Matchmaker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn searching(&self) -> bool {
        self.search.is_some()
    }

    pub fn request(&self) -> Option<SearchRequest> {
        self.search.as_ref().map(|s| s.req)
    }

    /// When the search began, on the wall clock, for the presence.
    pub fn since_unix_ms(&self) -> Option<u64> {
        self.search.as_ref().map(|s| s.since_unix_ms)
    }

    /// Whether the search holds, or asks for, a seat at this table.
    pub fn is_reserved(&self, key: &[u8; 32]) -> bool {
        self.search.as_ref().is_some_and(|s| s.reservations.iter().any(|r| r.key == Some(*key)))
    }

    /// Whether the search asks for a seat at this table and no slot has held
    /// it yet -- a join that failed to dial its founder, to be asked again.
    pub fn pending(&self, key: &[u8; 32]) -> bool {
        self.search
            .as_ref()
            .is_some_and(|s| s.reservations.iter().any(|r| r.key == Some(*key) && r.seated.is_none()))
    }

    /// The name of the table this search is founding, while the node has
    /// not said its key.
    pub fn founding(&self) -> Option<&str> {
        self.search
            .as_ref()
            .and_then(|s| s.reservations.iter().find(|r| r.mine && r.key.is_none()).map(|r| r.name.as_str()))
    }

    /// `D-064` rule 3: the founder's start rule for a table this client
    /// founded for the search, or `None` for a table that is not one.
    pub fn founder_gate(&self, key: &[u8; 32], now: Instant) -> Option<FounderGate> {
        let s = self.search.as_ref()?;
        let r = s.reservations.iter().find(|r| r.mine && r.key == Some(*key))?;
        Some(FounderGate {
            floor: r.capacity(now, s.waiting),
            armed: r.armed.is_some(),
        })
    }

    /// `D-064` rule 4: whether a seat the search holds at this table may
    /// ratify its roster now.
    pub fn may_ratify(&self, key: &[u8; 32]) -> bool {
        self.search
            .as_ref()
            .and_then(|s| s.reservations.iter().find(|r| r.key == Some(*key)))
            .is_some_and(|r| r.armed.is_some())
    }

    /// Start a search; a search already on is given up first.
    pub fn start(&mut self, req: SearchRequest, now: Instant, now_ms: u64) -> Vec<Step> {
        let req = req.checked();
        let mut steps = if self.search.is_some() {
            self.cancel(now, "a new search replaces it")
        } else {
            Vec::new()
        };
        self.search = Some(Search {
            req,
            since: now,
            since_unix_ms: now_ms,
            reservations: Vec::new(),
            founded_at: None,
            found_gone_at: None,
            last_report: None,
            last_log: None,
            arrivals: Arrivals::default(),
            started: Vec::new(),
            waiting: None,
        });
        steps.push(Step::Log(format!(
            "search: started ({}, {} game(s) at once{})",
            req.format.label(),
            req.tables,
            if req.again { ", again after the game" } else { "" }
        )));
        steps.push(Step::Presence(Some(req.format)));
        steps
    }

    /// The player gave the search up: every seat back, the queue told, the
    /// window told, nothing kept.
    pub fn cancel(&mut self, now: Instant, why: &str) -> Vec<Step> {
        let Some(s) = self.search.take() else {
            return Vec::new();
        };
        let mut steps = Vec::new();
        let mut released = 0usize;
        // A table that set under this search is a game the player is in; a
        // cancel gives the rest back and keeps those.
        for r in s.reservations.iter().filter(|r| !r.set) {
            if let Some(key) = r.key {
                released += 1;
                steps.push(Step::Leave { key, why: format!("the search was given up: {why}") });
            }
        }
        steps.push(Step::Presence(None));
        steps.push(Step::Log(format!(
            "search: cancelled ({why}); {released} reservation(s) released after {} s",
            now.saturating_duration_since(s.since).as_secs()
        )));
        steps.push(Step::Ended {
            id: s.req.id,
            why: format!("cancelled: {why}"),
            started: s.started.clone(),
        });
        let _ = now;
        steps
    }

    /// One tick of the search: reconcile with the node's slots, reserve,
    /// found, arm, report.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        now: Instant,
        now_ms: u64,
        candidates: &[Candidate],
        slots: &[SlotReading],
        net: &NetReading,
        queue: &Queue,
        me: &[u8; 32],
    ) -> Vec<Step> {
        let mut steps = Vec::new();
        self.avoided.retain(|_, (until, _)| *until > now);
        let Some(s) = self.search.as_mut() else {
            return steps;
        };
        let elapsed = now.saturating_duration_since(s.since);
        let join_wait = net.join_wait();
        let patience = net.founder_patience();

        // 1. Reconcile every reservation with the slot that holds it.
        let mut keep: Vec<Reservation> = Vec::with_capacity(s.reservations.len());
        let mut avoid: Vec<[u8; 32]> = Vec::new();
        for mut r in s.reservations.drain(..) {
            // `S1-HI`: a slot that holds the key without the table -- a seat given
            // back and asked for again, a table lost -- is no seat of this
            // client's; the slot's own roads say what it is.
            let found = slots.iter().find(|x| match r.key {
                Some(k) => x.search && x.key == Some(k),
                None => r.mine && x.search && x.founder && x.key.is_some() && x.name == r.name,
            });
            match found {
                Some(x) => {
                    if r.key.is_none() {
                        r.key = x.key;
                        steps.push(Step::Log(format!(
                            "search: founded {} ({} seats, slot {})",
                            short(&x.key.unwrap_or([0; 32])),
                            x.seats,
                            x.slot
                        )));
                    }
                    r.slot = Some(x.slot);
                    // The first sight of the slot shows this client's own seat
                    // among the players: no arrival to measure by.
                    let first_sight = r.seated.is_none();
                    if r.seated.is_none() && x.key.is_some() && x.seats >= 2 && !x.asking_again && !x.lost {
                        r.seated = Some(now);
                        if !r.mine {
                            steps.push(Step::Log(format!(
                                "search: reserved at {} ({}/{}, slot {})",
                                short(&x.key.unwrap_or([0; 32])),
                                x.players,
                                x.seats,
                                x.slot
                            )));
                        }
                    }
                    if x.seats >= 2 {
                        r.seats = x.seats;
                    }
                    if x.players != r.last_players.0 {
                        if x.players > r.last_players.0 && !first_sight {
                            s.arrivals.note(now.saturating_duration_since(r.last_players.1));
                        }
                        r.last_players = (x.players, now);
                    }
                    r.players = x.players;
                    if x.set && !r.set {
                        r.set = true;
                        if !s.started.contains(&x.slot) {
                            s.started.push(x.slot);
                        }
                        steps.push(Step::Log(format!(
                            "search: a game started at {} (slot {}, {} seats)",
                            short(&x.key.unwrap_or([0; 32])),
                            x.slot,
                            x.players
                        )));
                    }
                    let gone_too_long = x.founder_gone_s.is_some_and(|g| Duration::from_secs(g) >= patience);
                    let asking_too_long =
                        x.asking_again && now.saturating_duration_since(r.asked) >= join_wait * 2;
                    if !x.set && (x.lost || gone_too_long || asking_too_long) {
                        let why = if x.lost {
                            "the table was lost".to_string()
                        } else if gone_too_long {
                            format!("its founder could not be heard for {} s", patience.as_secs())
                        } else {
                            "the seat given back was not given again".to_string()
                        };
                        if let Some(k) = r.key {
                            steps.push(Step::Leave { key: k, why: format!("search: {why}") });
                            avoid.push(k);
                        }
                        steps.push(Step::Log(format!("search: released {}: {why}", r.key.map(|k| short(&k)).unwrap_or_default())));
                        if r.mine {
                            s.found_gone_at = Some(now);
                            s.founded_at = None;
                        }
                        continue;
                    }
                    keep.push(r);
                }
                None => {
                    if r.seated.is_some() {
                        // The seat is gone: refused, given back for good, or
                        // left by the node's own reading. Elsewhere, then.
                        steps.push(Step::Log(format!(
                            "search: the seat at {} is gone; looking elsewhere",
                            r.key.map(|k| short(&k)).unwrap_or_default()
                        )));
                        if let Some(k) = r.key {
                            avoid.push(k);
                        }
                        if r.mine {
                            s.found_gone_at = Some(now);
                            s.founded_at = None;
                        }
                    } else if now.saturating_duration_since(r.asked) >= join_wait {
                        steps.push(Step::Log(format!(
                            "search: no seat at {} in {} s; looking elsewhere",
                            if r.mine { r.name.clone() } else { r.key.map(|k| short(&k)).unwrap_or_default() },
                            join_wait.as_secs()
                        )));
                        if let Some(k) = r.key {
                            avoid.push(k);
                        }
                        if r.mine {
                            s.found_gone_at = Some(now);
                            s.founded_at = None;
                        }
                    } else {
                        keep.push(r);
                    }
                }
            }
        }
        s.reservations = keep;
        for k in avoid {
            let strikes = self.avoided.get(&k).map_or(0, |(_, n)| *n) + 1;
            let for_how_long = AVOID_FIRST.saturating_mul(1u32 << (strikes - 1).min(4)).min(AVOID_MAX);
            self.avoided.insert(k, (now + for_how_long, strikes));
        }
        let Some(s) = self.search.as_mut() else {
            return steps;
        };

        // 2. The goal: as many games as asked for, whoever started them. A
        // tournament that is over is no game (`S1-HJ`: a search started again
        // when the game ended counted the finished table and ended at once).
        let running = slots.iter().filter(|x| x.set && !x.lost && !x.over).count();
        let limit = usize::from(s.req.tables);
        if running >= limit {
            let mut extra: Vec<&Reservation> = s.reservations.iter().filter(|r| r.set).collect();
            // More games started under the search than asked for, in the
            // same moment: the ones started last are left by the player's
            // word -- windows the search opened itself and may close.
            let over = running.saturating_sub(limit);
            extra.sort_by_key(|r| std::cmp::Reverse(r.slot));
            let mut started = s.started.clone();
            for r in extra.into_iter().take(over) {
                if let Some(k) = r.key {
                    steps.push(Step::Leave {
                        key: k,
                        why: "search: more games started at once than asked for; this one is left".to_string(),
                    });
                    started.retain(|x| Some(*x) != r.slot);
                }
            }
            let mut released = 0usize;
            for r in s.reservations.iter().filter(|r| !r.set) {
                if let Some(k) = r.key {
                    released += 1;
                    steps.push(Step::Leave { key: k, why: "search: a game started elsewhere".to_string() });
                }
            }
            let why = if started.is_empty() {
                "the games asked for are running".to_string()
            } else {
                format!("a game started (slot {})", started.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "))
            };
            steps.push(Step::Log(format!(
                "search: ended -- {why} after {} s; {released} reservation(s) released",
                elapsed.as_secs()
            )));
            steps.push(Step::Presence(None));
            steps.push(Step::Report(report(s, now, now_ms, net, queue, me, running, "a game is starting", true)));
            steps.push(Step::Ended { id: s.req.id, why, started });
            self.search = None;
            return steps;
        }
        if elapsed >= SEARCH_MAX {
            let mut released = 0usize;
            for r in s.reservations.iter().filter(|r| !r.set) {
                if let Some(k) = r.key {
                    released += 1;
                    steps.push(Step::Leave { key: k, why: "search: no game found in time".to_string() });
                }
            }
            steps.push(Step::Log(format!(
                "search: ended -- no game found in {} min; {released} reservation(s) released",
                SEARCH_MAX.as_secs() / 60
            )));
            steps.push(Step::Presence(None));
            steps.push(Step::Ended {
                id: s.req.id,
                why: format!("no game found in {} minutes", SEARCH_MAX.as_secs() / 60),
                started: Vec::new(),
            });
            self.search = None;
            return steps;
        }
        let want = limit - running;
        let per_seat_s = s.arrivals.per_seat_s();
        // `S1-HK`: the queue is a reading only once it could have been heard;
        // before that its silence says nothing, and the search waits.
        let queue_known = net.queue_known(!queue.is_empty(), elapsed);
        s.waiting = queue_known.then(|| queue.count(Some(s.req.format), me));
        let waiting = s.waiting;

        // 3. Reserve at the tables closest to starting, up to the expansion's limit.
        let looking_at = dqe_limit(s.req.format, elapsed);
        let founded_here = s.founded_at.is_some();
        let room = usize::from(looking_at).saturating_sub(s.reservations.len());
        if room > 0 {
            let mut eligible: Vec<(u64, u8, [u8; 32], &Candidate)> = candidates
                .iter()
                .filter(|c| c.tournament && !c.password && c.joinable && c.fresh)
                .filter(|c| c.founder != *me && c.players < c.seats && c.seats >= 2)
                .filter(|c| s.req.format.accepts(c.seats))
                .filter(|c| !s.reservations.iter().any(|r| r.key == Some(c.key)))
                .filter(|c| !self.avoided.contains_key(&c.key))
                // Rule 5: seats flow to the lower key.
                .filter(|c| !(founded_here && is_search_table(&c.name, c.min_players) && c.founder > *me))
                .filter_map(|c| {
                    let age = Duration::from_millis(now_ms.saturating_sub(c.first_seen_ms));
                    let here = u32::from(c.players.saturating_sub(1));
                    let step = capacity_step(waiting.map(|w| w.saturating_sub(here)), c.seats.saturating_sub(c.players));
                    let eta = eta_s(c.players, c.seats, is_search_table(&c.name, c.min_players), age, step, per_seat_s, c.relayed)?;
                    // An *any* search prefers the bigger table while it is not
                    // much slower: the owner wants games, not heads-ups.
                    let ranked = if s.req.format == Format::Any {
                        eta.saturating_sub(SEAT_PREFERENCE_S * u64::from(c.seats.saturating_sub(2)))
                    } else {
                        eta
                    };
                    Some((ranked, c.seats.saturating_sub(c.players), c.key, c))
                })
                .collect();
            eligible.sort_by_key(|(eta, missing, key, _)| (*eta, *missing, *key));
            for (_, _, _, c) in eligible.into_iter().take(room) {
                s.reservations.push(Reservation {
                    key: Some(c.key),
                    name: c.name.clone(),
                    slot: None,
                    asked: now,
                    seated: None,
                    mine: false,
                    players: c.players,
                    seats: c.seats,
                    search_table: is_search_table(&c.name, c.min_players),
                    began: now.checked_sub(Duration::from_millis(now_ms.saturating_sub(c.first_seen_ms))).unwrap_or(now),
                    set: false,
                    armed: None,
                    last_players: (c.players, now),
                    relayed: c.relayed,
                });
                steps.push(Step::Log(format!(
                    "search: asking for a seat at {} ({}/{}{})",
                    short(&c.key),
                    c.players,
                    c.seats,
                    if c.relayed { ", founder relayed" } else { "" }
                )));
                steps.push(Step::Join { key: c.key, buyin: c.buyin });
            }
        }

        // 4. Found a table when the lobby offers nothing this search can sit at.
        let may_found = s.founded_at.is_none()
            && s.reservations.is_empty()
            && !net.line_down
            && queue_known
            && s.found_gone_at.is_none_or(|at| now.saturating_duration_since(at) >= net.found_after(0))
            && elapsed >= net.found_after(queue.rank_below(s.req.format, me));
        if may_found {
            let seats = s.req.format.seats().unwrap_or_else(|| queue.demand_seats(me));
            let name = search_table_name(seats);
            s.reservations.push(Reservation {
                key: None,
                name: name.clone(),
                slot: None,
                asked: now,
                seated: None,
                mine: true,
                players: 1,
                seats,
                search_table: true,
                began: now,
                set: false,
                armed: None,
                last_players: (1, now),
                relayed: false,
            });
            s.founded_at = Some(now);
            steps.push(Step::Log(format!(
                "search: nothing to sit at after {} s; founding {name}",
                elapsed.as_secs()
            )));
            steps.push(Step::Found { seats, name });
        }

        // 5. Arm at most `want` reservations: the ones closest to starting,
        // each kept for `COMMIT_TTL` once armed while it stays ready.
        let mut order: Vec<(u64, [u8; 32], usize)> = s
            .reservations
            .iter()
            .enumerate()
            .filter(|(_, r)| r.ready(now, waiting))
            .map(|(i, r)| (r.eta(now, per_seat_s, waiting).unwrap_or(u64::MAX), r.key.unwrap_or([0xff; 32]), i))
            .collect();
        order.sort_unstable();
        let kept: Vec<usize> = s
            .reservations
            .iter()
            .enumerate()
            .filter(|(_, r)| r.ready(now, waiting) && r.armed.is_some_and(|at| now.saturating_duration_since(at) < COMMIT_TTL))
            .map(|(i, _)| i)
            .collect();
        let mut armed: Vec<usize> = kept.into_iter().take(want).collect();
        for (_, _, i) in order {
            if armed.len() >= want {
                break;
            }
            if !armed.contains(&i) {
                armed.push(i);
            }
        }
        for (i, r) in s.reservations.iter_mut().enumerate() {
            let arm = armed.contains(&i);
            match (arm, r.armed) {
                (true, None) => {
                    r.armed = Some(now);
                    steps.push(Step::Log(format!(
                        "search: armed {} ({}/{} at capacity {})",
                        r.key.map(|k| short(&k)).unwrap_or_default(),
                        r.players,
                        r.seats,
                        r.capacity(now, waiting)
                    )));
                }
                (false, Some(_)) => {
                    r.armed = None;
                    steps.push(Step::Log(format!(
                        "search: disarmed {}",
                        r.key.map(|k| short(&k)).unwrap_or_default()
                    )));
                }
                _ => {}
            }
        }

        // 6. Tell the window, and the log.
        if s.last_report.is_none_or(|at| now.saturating_duration_since(at) >= REPORT_EVERY) {
            s.last_report = Some(now);
            let phase = if net.line_down || net.poker_peers == 0 {
                "waiting for the network"
            } else if s.reservations.iter().any(|r| r.armed.is_some()) {
                "a table is about to start"
            } else if s.reservations.iter().any(|r| r.seated.is_some()) {
                "seats reserved; waiting for players"
            } else if s.reservations.is_empty() {
                "looking for tables"
            } else {
                "asking for seats"
            };
            steps.push(Step::Report(report(s, now, now_ms, net, queue, me, running, phase, queue_known)));
        }
        if s.last_log.is_none_or(|at| now.saturating_duration_since(at) >= LOG_EVERY) {
            s.last_log = Some(now);
            let held: Vec<String> = s
                .reservations
                .iter()
                .map(|r| {
                    format!(
                        "{} {}/{}{}{}",
                        r.key.map(|k| short(&k)).unwrap_or_else(|| "founding".into()),
                        r.players,
                        r.capacity(now, waiting),
                        if r.mine { " mine" } else { "" },
                        if r.armed.is_some() { " armed" } else { "" }
                    )
                })
                .collect();
            let eta = s.reservations.iter().filter(|r| !r.set).filter_map(|r| r.eta(now, per_seat_s, waiting)).min();
            steps.push(Step::Log(format!(
                "search: {} s, eta {}, queue {}, reserved {} of up to {} [{}], playing {}/{}{}",
                elapsed.as_secs(),
                eta.map_or("--".to_string(), |e| format!("~{e} s")),
                waiting.map_or("?".to_string(), |w| w.to_string()),
                s.reservations.len(),
                looking_at,
                held.join("; "),
                running,
                s.req.tables,
                net.warning().map_or(String::new(), |w| format!(", {w}"))
            )));
        }
        steps
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    s: &Search,
    now: Instant,
    now_ms: u64,
    net: &NetReading,
    queue: &Queue,
    me: &[u8; 32],
    running: usize,
    phase: &str,
    queue_known: bool,
) -> SearchReport {
    let per_seat_s = s.arrivals.per_seat_s();
    let waiting = s.waiting;
    SearchReport {
        id: s.req.id,
        format: s.req.format,
        elapsed_s: now.saturating_duration_since(s.since).as_secs(),
        eta_s: s
            .reservations
            .iter()
            .filter(|r| r.seated.is_some() && !r.set)
            .filter_map(|r| r.eta(now, per_seat_s, waiting))
            .min(),
        queue: queue.count(Some(s.req.format), me),
        queue_known,
        queue_wait_s: queue.mean_wait_s(s.req.format, now_ms, me),
        // A table that set under this search is a game, counted among those.
        reservations: s.reservations.iter().filter(|r| !r.set).map(|r| r.view(now, waiting)).collect(),
        looking_at: dqe_limit(s.req.format, now.saturating_duration_since(s.since)),
        running: running.try_into().unwrap_or(u8::MAX),
        limit: s.req.tables,
        warning: net.warning().map(str::to_string),
        phase: phase.to_string(),
    }
}

/// Between formats wanted equally: six seats, then heads-up, then nine.
fn tie(f: Format) -> u8 {
    match f {
        Format::SixMax => 2,
        Format::HeadsUp => 1,
        _ => 0,
    }
}

/// Eight characters of a key, for the log.
pub fn short(key: &[u8; 32]) -> String {
    key[..4].iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::constants::{RATED_START_STACK, SEARCH_PRESENCE_EVERY_MS};

    const NOW_MS: u64 = 1_700_000_000_000;

    fn key(n: u8) -> [u8; 32] {
        [n; 32]
    }

    fn candidate(n: u8, seats: u8, players: u8, founder: u8) -> Candidate {
        Candidate {
            key: key(n),
            name: format!("table {n}"),
            seats,
            players,
            min_players: seats,
            founder: key(founder),
            buyin: RATED_START_STACK,
            first_seen_ms: NOW_MS,
            fresh: true,
            joinable: true,
            tournament: true,
            password: false,
            relayed: false,
        }
    }

    fn search_candidate(n: u8, seats: u8, players: u8, founder: u8, age_s: u64) -> Candidate {
        Candidate {
            name: search_table_name(seats),
            min_players: 2,
            first_seen_ms: NOW_MS - age_s * 1_000,
            ..candidate(n, seats, players, founder)
        }
    }

    fn slot(slot: u8, key: [u8; 32], players: u8, seats: u8) -> SlotReading {
        SlotReading {
            slot,
            key: Some(key),
            name: format!("table {}", key[0]),
            search: true,
            founder: false,
            players,
            seats,
            set: false,
            over: false,
            lost: false,
            asking_again: false,
            founder_gone_s: None,
        }
    }

    fn req(format: Format, tables: u8) -> SearchRequest {
        SearchRequest { id: 7, format, tables, again: false }
    }

    fn joins(steps: &[Step]) -> Vec<[u8; 32]> {
        steps.iter().filter_map(|s| match s { Step::Join { key, .. } => Some(*key), _ => None }).collect()
    }

    fn leaves(steps: &[Step]) -> Vec<[u8; 32]> {
        steps.iter().filter_map(|s| match s { Step::Leave { key, .. } => Some(*key), _ => None }).collect()
    }

    fn founds(steps: &[Step]) -> Vec<u8> {
        steps.iter().filter_map(|s| match s { Step::Found { seats, .. } => Some(*seats), _ => None }).collect()
    }

    fn ended(steps: &[Step]) -> Option<(String, Vec<u8>)> {
        steps.iter().find_map(|s| match s { Step::Ended { why, started, .. } => Some((why.clone(), started.clone())), _ => None })
    }

    #[test]
    fn capacity_falls_one_seat_every_step_and_never_under_two() {
        let step = Duration::from_secs(90);
        assert_eq!(capacity_now(6, Duration::ZERO, step), 6);
        assert_eq!(capacity_now(6, Duration::from_secs(89), step), 6);
        assert_eq!(capacity_now(6, Duration::from_secs(90), step), 5);
        assert_eq!(capacity_now(6, Duration::from_secs(359), step), 3);
        assert_eq!(capacity_now(6, Duration::from_secs(360), step), 2);
        assert_eq!(capacity_now(6, Duration::from_secs(10_000), step), 2);
        assert_eq!(capacity_now(2, Duration::from_secs(10_000), step), 2);
    }

    /// The owner's amendment: the queue says how fast a founder comes down.
    #[test]
    fn the_step_is_fast_for_nobody_waiting_and_slow_for_enough() {
        assert_eq!(capacity_step(None, 6), STEP_SLOW, "a queue not heard is not empty");
        assert_eq!(capacity_step(Some(0), 6), STEP_FAST);
        assert_eq!(capacity_step(Some(3), 6), Duration::from_secs(105));
        assert_eq!(capacity_step(Some(6), 6), STEP_SLOW);
        assert_eq!(capacity_step(Some(9), 6), STEP_SLOW, "capped");
        assert_eq!(capacity_step(Some(0), 0), STEP_SLOW, "nothing missing: nothing to come down for");
        let net = NetReading::default();
        assert!(!net.queue_known(false, Duration::from_secs(30)), "no peer, nothing heard, half a minute: unknown");
        assert!(net.queue_known(true, Duration::ZERO), "one presence heard: known");
        assert!(NetReading { on_line_s: Some(QUEUE_WARM_S), ..Default::default() }.queue_known(false, Duration::ZERO));
        assert!(net.queue_known(false, QUEUE_WAIT_MAX), "a minute alone: found anyway");
    }

    #[test]
    fn the_expansion_starts_at_four_and_grows_by_format() {
        assert_eq!(dqe_limit(Format::Any, Duration::ZERO), 4);
        assert_eq!(dqe_limit(Format::Any, Duration::from_secs(29)), 4);
        assert_eq!(dqe_limit(Format::Any, Duration::from_secs(30)), 5);
        assert_eq!(dqe_limit(Format::Any, Duration::from_secs(600)), 10);
        assert_eq!(dqe_limit(Format::HeadsUp, Duration::from_secs(600)), 10);
        // Less aggressive for the bigger tables: slower, and lower.
        assert_eq!(dqe_limit(Format::SixMax, Duration::from_secs(30)), 4);
        assert_eq!(dqe_limit(Format::SixMax, Duration::from_secs(45)), 5);
        assert_eq!(dqe_limit(Format::SixMax, Duration::from_secs(3_600)), 8);
        assert_eq!(dqe_limit(Format::FullRing, Duration::from_secs(59)), 4);
        assert_eq!(dqe_limit(Format::FullRing, Duration::from_secs(60)), 5);
        assert_eq!(dqe_limit(Format::FullRing, Duration::from_secs(3_600)), 6);
    }

    #[test]
    fn the_eta_is_the_sooner_of_the_fill_and_the_capacity() {
        // Four seats to fill at 45 s each, or the capacity reaching two seats
        // in four steps minus the age.
        let step = Duration::from_secs(90);
        assert_eq!(eta_s(2, 6, true, Duration::from_secs(300), step, 45, false), Some(60));
        assert_eq!(eta_s(2, 6, true, Duration::ZERO, step, 45, false), Some(180));
        // A table another player founded starts only full.
        assert_eq!(eta_s(2, 6, false, Duration::from_secs(300), step, 45, false), Some(180));
        // One seat: no capacity reaches it; the fill alone.
        assert_eq!(eta_s(1, 2, true, Duration::from_secs(1_000), step, 45, false), Some(45));
        assert_eq!(eta_s(6, 6, false, Duration::ZERO, step, 45, true), Some(RELAY_PENALTY_S));
    }

    #[test]
    fn a_search_reserves_at_the_tables_closest_to_starting_up_to_the_limit() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        let _ = mm.start(req(Format::SixMax, 1), now, NOW_MS);
        let cands = vec![
            candidate(1, 6, 1, 0x01),
            candidate(2, 6, 5, 0x02),
            candidate(3, 6, 3, 0x03),
            candidate(4, 6, 4, 0x04),
            candidate(5, 6, 2, 0x05),
            candidate(6, 9, 8, 0x06), // wrong format
            Candidate { password: true, ..candidate(7, 6, 5, 0x07) },
            candidate(8, 6, 5, 0xaa), // my own table
        ];
        let steps = mm.tick(now, NOW_MS, &cands, &[], &NetReading::default(), &Queue::new(), &me);
        let asked = joins(&steps);
        assert_eq!(asked.len(), 4, "four tables at the start: {asked:?}");
        assert_eq!(asked[0], key(2), "the fullest first");
        assert!(!asked.contains(&key(6)) && !asked.contains(&key(7)) && !asked.contains(&key(8)));
        assert!(founds(&steps).is_empty(), "nothing founded while there is something to sit at");
        // Nothing is asked twice.
        let again = mm.tick(now + Duration::from_secs(1), NOW_MS + 1_000, &cands, &[], &NetReading::default(), &Queue::new(), &me);
        assert!(joins(&again).is_empty());
    }

    #[test]
    fn a_search_founds_a_table_once_when_nothing_is_on_offer() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        let net = NetReading { on_line_s: Some(QUEUE_WARM_S), ..Default::default() };
        let _ = mm.start(req(Format::HeadsUp, 1), now, NOW_MS);
        let early = mm.tick(now + Duration::from_secs(5), NOW_MS, &[], &[], &net, &Queue::new(), &me);
        assert!(founds(&early).is_empty(), "not before found_after");
        let due = mm.tick(now + net.found_after(0), NOW_MS, &[], &[], &net, &Queue::new(), &me);
        assert_eq!(founds(&due), vec![2]);
        assert_eq!(mm.founding(), Some(search_table_name(2).as_str()));
        let once = mm.tick(now + net.found_after(0) + Duration::from_secs(1), NOW_MS, &[], &[], &net, &Queue::new(), &me);
        assert!(founds(&once).is_empty(), "one own table at a time");
        // The node opens the slot: the reservation binds to its key.
        let mine = SlotReading {
            slot: 1,
            key: Some(key(0x99)),
            name: search_table_name(2),
            search: true,
            founder: true,
            players: 1,
            seats: 2,
            set: false,
            over: false,
            lost: false,
            asking_again: false,
            founder_gone_s: None,
        };
        let _ = mm.tick(now + net.found_after(0) + Duration::from_secs(2), NOW_MS, &[], &[mine.clone()], &net, &Queue::new(), &me);
        assert!(mm.is_reserved(&key(0x99)));
        let gate = mm.founder_gate(&key(0x99), now + net.found_after(0) + Duration::from_secs(2)).expect("my table");
        assert_eq!(gate.floor, 2);
        assert!(!gate.armed, "one seat: nothing to start");
        // A second seat: heads-up is at capacity, the founder may set.
        let later = now + net.found_after(0) + Duration::from_secs(3);
        let _ = mm.tick(later, NOW_MS, &[], &[SlotReading { players: 2, ..mine }], &net, &Queue::new(), &me);
        assert!(mm.founder_gate(&key(0x99), later).expect("my table").armed);
    }

    #[test]
    fn an_any_search_founds_what_the_queue_wants() {
        let mut q = Queue::new();
        let me = key(0xaa);
        assert_eq!(q.demand_seats(&me), AUTO_SEATS, "nobody named a format: the biggest table");
        for n in 1..=3u8 {
            q.note(Heard { who: key(n), format: Some(Format::SixMax), tables: 1, since_unix_ms: NOW_MS }, NOW_MS);
        }
        q.note(Heard { who: key(9), format: Some(Format::Any), tables: 1, since_unix_ms: NOW_MS }, NOW_MS);
        assert_eq!(q.demand_seats(&me), 6, "the format the others named");
        // A crowd of *any* searchers, however big, is the biggest table too.
        let mut crowd = Queue::new();
        for n in 1..=9u8 {
            crowd.note(Heard { who: key(n), format: Some(Format::Any), tables: 1, since_unix_ms: NOW_MS }, NOW_MS);
        }
        assert_eq!(crowd.demand_seats(&me), AUTO_SEATS);
        assert_eq!(q.count(Some(Format::SixMax), &me), 4);
        assert_eq!(q.count(Some(Format::HeadsUp), &me), 1);
        assert_eq!(q.count(None, &me), 4);
        assert_eq!(q.rank_below(Format::SixMax, &me), 4);
        assert_eq!(q.rank_below(Format::SixMax, &key(2)), 1);
        // A stop takes the searcher out at once; the TTL takes the rest.
        assert!(q.note(Heard { who: key(1), format: None, tables: 1, since_unix_ms: 0 }, NOW_MS));
        assert_eq!(q.count(None, &me), 3);
        assert_eq!(q.expire(NOW_MS + SEARCH_PRESENCE_TTL_MS), 3);
        assert!(q.is_empty());
    }

    #[test]
    fn the_founding_order_is_by_key_and_seats_flow_to_the_lower_one() {
        let net = NetReading { on_line_s: Some(QUEUE_WARM_S), ..Default::default() };
        assert!(net.found_after(0) < net.found_after(1));
        assert_eq!(net.found_after(9), net.found_after(4), "capped");
        // A client that founded joins another search table only below its key.
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0x80);
        let _ = mm.start(req(Format::HeadsUp, 1), now, NOW_MS);
        let t = now + net.found_after(0);
        let steps = mm.tick(t, NOW_MS, &[], &[], &net, &Queue::new(), &me);
        assert_eq!(founds(&steps), vec![2]);
        let mine = SlotReading {
            slot: 1,
            key: Some(key(0x99)),
            name: search_table_name(2),
            search: true,
            founder: true,
            players: 1,
            seats: 2,
            set: false,
            over: false,
            lost: false,
            asking_again: false,
            founder_gone_s: None,
        };
        let cands = vec![search_candidate(0x90, 2, 1, 0x90, 5), search_candidate(0x70, 2, 1, 0x70, 5)];
        let steps = mm.tick(t + Duration::from_secs(1), NOW_MS, &cands, &[mine], &net, &Queue::new(), &me);
        assert_eq!(joins(&steps), vec![key(0x70)], "the higher key's table is not joined");
    }

    #[test]
    fn a_table_that_answers_nothing_is_avoided_and_another_taken() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        let net = NetReading::default();
        let _ = mm.start(req(Format::HeadsUp, 1), now, NOW_MS);
        let one = vec![candidate(1, 2, 1, 0x01)];
        let steps = mm.tick(now, NOW_MS, &one, &[], &net, &Queue::new(), &me);
        assert_eq!(joins(&steps), vec![key(1)]);
        let later = now + net.join_wait();
        let both = vec![candidate(1, 2, 1, 0x01), candidate(2, 2, 1, 0x02)];
        let steps = mm.tick(later, NOW_MS, &both, &[], &net, &Queue::new(), &me);
        assert_eq!(joins(&steps), vec![key(2)], "the silent one is avoided, the other asked");
        assert!(mm.avoided.contains_key(&key(1)));
        // And after the avoidance it may be asked again.
        let after = later + AVOID_FIRST + Duration::from_secs(1);
        let steps = mm.tick(after, NOW_MS, &both, &[], &net, &Queue::new(), &me);
        assert!(joins(&steps).contains(&key(1)));
    }

    #[test]
    fn at_most_the_games_wanted_are_armed_and_the_closest_first() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        let net = NetReading::default();
        let _ = mm.start(req(Format::HeadsUp, 1), now, NOW_MS);
        let cands = vec![candidate(1, 2, 1, 0x01), candidate(2, 2, 1, 0x02)];
        let _ = mm.tick(now, NOW_MS, &cands, &[], &net, &Queue::new(), &me);
        // Both seats held, both tables full: one is armed.
        let slots = vec![slot(1, key(1), 2, 2), slot(2, key(2), 2, 2)];
        let _ = mm.tick(now + Duration::from_secs(1), NOW_MS, &cands, &slots, &net, &Queue::new(), &me);
        let armed = [key(1), key(2)].iter().filter(|k| mm.may_ratify(k)).count();
        assert_eq!(armed, 1);
        assert!(mm.may_ratify(&key(1)), "the lower key between equals");
        // Two games wanted: both.
        let mut two = Matchmaker::new();
        let _ = two.start(req(Format::HeadsUp, 2), now, NOW_MS);
        let _ = two.tick(now, NOW_MS, &cands, &[], &net, &Queue::new(), &me);
        let _ = two.tick(now + Duration::from_secs(1), NOW_MS, &cands, &slots, &net, &Queue::new(), &me);
        assert!(two.may_ratify(&key(1)) && two.may_ratify(&key(2)));
    }

    #[test]
    fn a_joiner_arms_a_search_table_one_seat_under_its_capacity_estimate() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        // The queue not heard yet: the slow step, so a table seen 100 s old
        // is at its six seats still, and four of them are not one under.
        let unknown = NetReading::default();
        let _ = mm.start(req(Format::SixMax, 1), now, NOW_MS);
        let cands = vec![search_candidate(1, 6, 3, 0x01, 100)];
        let _ = mm.tick(now, NOW_MS, &cands, &[], &unknown, &Queue::new(), &me);
        let mut s = slot(1, key(1), 3, 6);
        s.name = search_table_name(6);
        let _ = mm.tick(now + Duration::from_secs(1), NOW_MS, &cands, &[s.clone()], &unknown, &Queue::new(), &me);
        assert!(!mm.may_ratify(&key(1)), "three of a capacity of six");
        s.players = 4;
        let _ = mm.tick(now + Duration::from_secs(2), NOW_MS, &cands, &[s.clone()], &unknown, &Queue::new(), &me);
        assert!(!mm.may_ratify(&key(1)), "four of a capacity of six, the queue unknown");
        // The queue heard and nobody waiting: the fast step, a capacity of
        // three at 100 s, and four seats are ready.
        let known = NetReading { on_line_s: Some(QUEUE_WARM_S), ..Default::default() };
        let _ = mm.tick(now + Duration::from_secs(3), NOW_MS, &cands, &[s], &known, &Queue::new(), &me);
        assert!(mm.may_ratify(&key(1)));
    }

    /// `S1-HK`: an *any* search founds nothing while the queue's silence may
    /// be the mesh still forming, founds the biggest table once the queue is
    /// known and nobody named a format, and prefers the bigger table on offer.
    #[test]
    fn an_any_search_waits_for_the_queue_founds_ten_and_prefers_the_bigger_table() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        let unknown = NetReading::default();
        let _ = mm.start(req(Format::Any, 1), now, NOW_MS);
        let early = mm.tick(now + Duration::from_secs(30), NOW_MS, &[], &[], &unknown, &Queue::new(), &me);
        assert!(founds(&early).is_empty(), "the queue is not heard yet");
        let late = mm.tick(now + QUEUE_WAIT_MAX, NOW_MS, &[], &[], &unknown, &Queue::new(), &me);
        assert_eq!(founds(&late), vec![AUTO_SEATS], "alone for a minute: the biggest table");
        // A queue heard, with a heads-up searcher below this key: heads-up is
        // what it can take, and the founding waits its rank.
        let mut q = Queue::new();
        q.note(Heard { who: key(0x01), format: Some(Format::HeadsUp), tables: 1, since_unix_ms: NOW_MS }, NOW_MS);
        let mut hu = Matchmaker::new();
        let _ = hu.start(req(Format::Any, 1), now, NOW_MS);
        let steps = hu.tick(now + unknown.found_after(1), NOW_MS, &[], &[], &unknown, &q, &me);
        assert_eq!(founds(&steps), vec![2]);
        // On offer: a heads-up table with one seat, and a ten-seat search
        // table with eight; the bigger one is asked for first.
        let mut big = Matchmaker::new();
        let known = NetReading { on_line_s: Some(QUEUE_WARM_S), ..Default::default() };
        let _ = big.start(req(Format::Any, 1), now, NOW_MS);
        let cands = vec![candidate(0x05, 2, 1, 0x05), search_candidate(0x06, 10, 8, 0x06, 0)];
        let steps = big.tick(now, NOW_MS, &cands, &[], &known, &Queue::new(), &me);
        assert_eq!(joins(&steps)[0], key(0x06));
    }

    #[test]
    fn a_game_starting_ends_the_search_and_gives_every_other_seat_back() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        let net = NetReading::default();
        let _ = mm.start(req(Format::HeadsUp, 1), now, NOW_MS);
        let cands = vec![candidate(1, 2, 1, 0x01), candidate(2, 2, 1, 0x02), candidate(3, 2, 1, 0x03)];
        let _ = mm.tick(now, NOW_MS, &cands, &[], &net, &Queue::new(), &me);
        let mut slots = vec![slot(1, key(1), 2, 2), slot(2, key(2), 1, 2), slot(3, key(3), 1, 2)];
        let _ = mm.tick(now + Duration::from_secs(1), NOW_MS, &cands, &slots, &net, &Queue::new(), &me);
        slots[0].set = true;
        let steps = mm.tick(now + Duration::from_secs(2), NOW_MS, &cands, &slots, &net, &Queue::new(), &me);
        let mut left = leaves(&steps);
        left.sort();
        assert_eq!(left, vec![key(2), key(3)]);
        let (why, started) = ended(&steps).expect("ended");
        assert!(why.contains("a game started"), "{why}");
        assert_eq!(started, vec![1]);
        assert!(steps.contains(&Step::Presence(None)));
        assert!(!mm.searching());
        // Nothing more comes out of a search that ended.
        let silent = mm.tick(now + Duration::from_secs(3), NOW_MS, &cands, &slots, &net, &Queue::new(), &me);
        assert!(silent.is_empty());
    }

    #[test]
    fn two_games_started_in_one_moment_over_the_limit_leave_the_later_one() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        let net = NetReading::default();
        let _ = mm.start(req(Format::HeadsUp, 1), now, NOW_MS);
        let cands = vec![candidate(1, 2, 1, 0x01), candidate(2, 2, 1, 0x02)];
        let _ = mm.tick(now, NOW_MS, &cands, &[], &net, &Queue::new(), &me);
        let mut slots = vec![slot(1, key(1), 2, 2), slot(2, key(2), 2, 2)];
        slots[0].set = true;
        slots[1].set = true;
        let steps = mm.tick(now + Duration::from_secs(1), NOW_MS, &cands, &slots, &net, &Queue::new(), &me);
        assert_eq!(leaves(&steps), vec![key(2)], "the later slot is left");
        assert_eq!(ended(&steps).expect("ended").1, vec![1]);
    }

    #[test]
    fn a_game_the_player_sat_down_at_by_hand_counts_against_the_limit() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        let net = NetReading::default();
        let _ = mm.start(req(Format::HeadsUp, 1), now, NOW_MS);
        let cands = vec![candidate(1, 2, 1, 0x01)];
        let _ = mm.tick(now, NOW_MS, &cands, &[], &net, &Queue::new(), &me);
        let by_hand = SlotReading { search: false, set: true, ..slot(0, key(0x55), 6, 6) };
        let steps = mm.tick(now + Duration::from_secs(1), NOW_MS, &cands, &[by_hand, slot(1, key(1), 1, 2)], &net, &Queue::new(), &me);
        assert_eq!(leaves(&steps), vec![key(1)]);
        assert_eq!(ended(&steps).expect("ended").1, Vec::<u8>::new());
    }

    /// `S1-HJ`: the game the search started is over, the player asked to
    /// search again, and the finished table -- still shown -- is no game.
    #[test]
    fn a_tournament_that_is_over_counts_as_no_game() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        let net = NetReading::default();
        let _ = mm.start(req(Format::HeadsUp, 1), now, NOW_MS);
        let finished = SlotReading { search: false, set: true, over: true, ..slot(1, key(0x55), 2, 2) };
        let steps = mm.tick(now, NOW_MS, &[candidate(1, 2, 1, 0x01)], &[finished], &net, &Queue::new(), &me);
        assert!(ended(&steps).is_none(), "the finished table ends nothing");
        assert_eq!(joins(&steps), vec![key(1)], "and the search looks on");
    }

    #[test]
    fn a_cancel_leaves_every_reservation_and_tells_the_queue() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        let net = NetReading::default();
        let _ = mm.start(req(Format::SixMax, 2), now, NOW_MS);
        let cands = vec![candidate(1, 6, 3, 0x01), candidate(2, 6, 2, 0x02)];
        let _ = mm.tick(now, NOW_MS, &cands, &[], &net, &Queue::new(), &me);
        let steps = mm.cancel(now + Duration::from_secs(5), "the player pressed cancel");
        let mut left = leaves(&steps);
        left.sort();
        assert_eq!(left, vec![key(1), key(2)]);
        assert!(steps.contains(&Step::Presence(None)));
        assert!(ended(&steps).expect("ended").0.contains("cancelled"));
        assert!(!mm.searching());
        assert!(mm.cancel(now, "again").is_empty(), "nothing to cancel twice");
    }

    #[test]
    fn a_lost_table_and_a_founder_gone_too_long_are_left() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        let net = NetReading::default();
        let _ = mm.start(req(Format::HeadsUp, 1), now, NOW_MS);
        let cands = vec![candidate(1, 2, 1, 0x01), candidate(2, 2, 1, 0x02)];
        let _ = mm.tick(now, NOW_MS, &cands, &[], &net, &Queue::new(), &me);
        let slots = vec![
            SlotReading { lost: true, ..slot(1, key(1), 1, 2) },
            SlotReading { founder_gone_s: Some(net.founder_patience().as_secs()), ..slot(2, key(2), 1, 2) },
        ];
        let steps = mm.tick(now + Duration::from_secs(1), NOW_MS, &cands, &slots, &net, &Queue::new(), &me);
        let mut left = leaves(&steps);
        left.sort();
        assert_eq!(left, vec![key(1), key(2)]);
        assert!(!mm.is_reserved(&key(1)) && !mm.is_reserved(&key(2)));
        assert!(mm.searching(), "the search goes on elsewhere");
    }

    /// `search190453-4`: this client's own seat, seen the moment the slot
    /// held the table, was measured as a seat arriving ten seconds after the
    /// founder's -- and every estimate read forty seconds to a full table.
    #[test]
    fn this_clients_own_seat_is_no_arrival_and_a_later_one_is() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        let net = NetReading::default();
        let _ = mm.start(req(Format::SixMax, 1), now, NOW_MS);
        let cands = vec![candidate(1, 6, 1, 0x01)];
        let _ = mm.tick(now, NOW_MS, &cands, &[], &net, &Queue::new(), &me);
        // The slot's first sight shows two seated: the founder and this client.
        let _ = mm.tick(now + Duration::from_secs(1), NOW_MS, &cands, &[slot(1, key(1), 2, 6)], &net, &Queue::new(), &me);
        assert_eq!(mm.search.as_ref().expect("a search").arrivals.per_seat_s(), ARRIVAL_DEFAULT_S);
        // A third seat sixty seconds later is an arrival.
        let _ = mm.tick(now + Duration::from_secs(61), NOW_MS, &cands, &[slot(1, key(1), 3, 6)], &net, &Queue::new(), &me);
        assert_eq!(mm.search.as_ref().expect("a search").arrivals.per_seat_s(), 60);
    }

    /// `S1-HI`: a slot holding the key and no table -- the seat given back and
    /// asked for again -- is no seat; the reservation stays pending, and goes
    /// at `join_wait` like any ask nobody answered.
    #[test]
    fn a_slot_without_its_table_is_no_seat() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        let net = NetReading::default();
        let _ = mm.start(req(Format::HeadsUp, 1), now, NOW_MS);
        let cands = vec![candidate(1, 2, 1, 0x01)];
        let _ = mm.tick(now, NOW_MS, &cands, &[], &net, &Queue::new(), &me);
        let asked_again = SlotReading { seats: 0, players: 0, asking_again: true, ..slot(1, key(1), 0, 0) };
        let _ = mm.tick(now + Duration::from_secs(1), NOW_MS, &cands, &[asked_again.clone()], &net, &Queue::new(), &me);
        assert!(mm.pending(&key(1)), "no table held: still pending");
        let steps = mm.tick(now + net.join_wait() * 2, NOW_MS, &cands, &[asked_again], &net, &Queue::new(), &me);
        assert_eq!(leaves(&steps), vec![key(1)], "the seat asked for again too long is given up");
        assert!(!mm.is_reserved(&key(1)));
    }

    #[test]
    fn the_timeouts_stretch_with_the_network_and_never_past_their_caps() {
        let easy = NetReading { poker_peers: 10, ..Default::default() };
        let hard = NetReading { poker_peers: 1, relayed: true, ..Default::default() };
        assert_eq!(easy.join_wait(), Duration::from_secs(20));
        assert_eq!(hard.join_wait(), Duration::from_secs(45));
        assert!(hard.founder_patience() > easy.founder_patience());
        assert!(hard.found_after(0) > easy.found_after(0));
        let down = NetReading { line_down: true, ..Default::default() };
        assert!(down.warning().is_some_and(|w| w.contains("reconnecting")));
        assert_eq!(NetReading { poker_peers: 4, public: Some(true), ..Default::default() }.warning(), None);
    }

    #[test]
    fn the_search_ends_by_itself_after_its_own_limit() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        let net = NetReading::default();
        let _ = mm.start(req(Format::SixMax, 1), now, NOW_MS);
        let cands = vec![candidate(1, 6, 3, 0x01)];
        let _ = mm.tick(now, NOW_MS, &cands, &[], &net, &Queue::new(), &me);
        let steps = mm.tick(now + SEARCH_MAX, NOW_MS, &cands, &[slot(1, key(1), 3, 6)], &net, &Queue::new(), &me);
        assert_eq!(leaves(&steps), vec![key(1)]);
        assert!(ended(&steps).expect("ended").0.contains("no game found"));
        assert!(!mm.searching());
    }

    #[test]
    fn the_report_says_where_the_search_stands() {
        let mut mm = Matchmaker::new();
        let now = Instant::now();
        let me = key(0xaa);
        let net = NetReading { poker_peers: 3, on_line_s: Some(QUEUE_WARM_S), ..Default::default() };
        let _ = mm.start(req(Format::SixMax, 1), now, NOW_MS);
        let cands = vec![search_candidate(1, 6, 4, 0x01, 200)];
        let steps = mm.tick(now, NOW_MS, &cands, &[], &net, &Queue::new(), &me);
        let r = steps.iter().find_map(|s| match s { Step::Report(r) => Some(r.clone()), _ => None }).expect("a report");
        assert_eq!(r.id, 7);
        assert_eq!(r.limit, 1);
        assert_eq!(r.looking_at, 4);
        assert!(r.queue_known);
        assert_eq!(r.reservations.len(), 1);
        assert_eq!(r.reservations[0].capacity, 2, "200 s old with nobody waiting: the fast step, down to two");
        assert_eq!(r.phase, "asking for seats");
        assert_eq!(r.eta_s, None, "no seat yet");
        let mut s = slot(1, key(1), 4, 6);
        s.name = search_table_name(6);
        let steps = mm.tick(now + REPORT_EVERY, NOW_MS, &cands, &[s], &net, &Queue::new(), &me);
        let r = steps.iter().find_map(|s| match s { Step::Report(r) => Some(r.clone()), _ => None }).expect("a report");
        assert_eq!(r.eta_s, Some(0), "at capacity");
        assert!(r.reservations[0].armed);
        assert_eq!(r.phase, "a table is about to start");
    }

    #[test]
    fn a_presence_goes_round_the_wire_and_a_forgery_does_not() {
        let key_a = SigningKey::from_bytes(&[3u8; 32]);
        let mut limits = RateLimiter::new();
        let bytes = presence(&key_a, Some(Format::SixMax), 2, NOW_MS - 5_000, NOW_MS).expect("sealed");
        assert!(bytes.len() <= SEARCH_PRESENCE_MAX);
        let heard = receive_presence(&bytes, [9u8; 32], NOW_MS, &mut limits).expect("heard");
        assert_eq!(heard.who, key_a.verifying_key().to_bytes());
        assert_eq!(heard.format, Some(Format::SixMax));
        assert_eq!(heard.tables, 2);
        assert_eq!(heard.since_unix_ms, NOW_MS - 5_000);
        // Stopped.
        let stop = presence(&key_a, None, 1, NOW_MS, NOW_MS).expect("sealed");
        assert_eq!(receive_presence(&stop, [9u8; 32], NOW_MS, &mut limits).expect("heard").format, None);
        // Far from now.
        assert_eq!(
            receive_presence(&bytes, [8u8; 32], NOW_MS + super::super::lobbytalk::CLOCK_SLACK_MS + 1, &mut limits),
            Err(NotHeard::Stale)
        );
        // A flipped byte in the signature.
        let mut forged = bytes.clone();
        let last = forged.len() - 1;
        forged[last] ^= 0x01;
        assert_eq!(receive_presence(&forged, [7u8; 32], NOW_MS, &mut limits), Err(NotHeard::Forged));
        // Another message type is not a presence.
        let chat = super::super::lobbytalk::presence(&key_a, "x", NOW_MS).expect("sealed");
        assert!(matches!(receive_presence(&chat, [6u8; 32], NOW_MS, &mut limits), Err(NotHeard::Malformed(_))));
        // Over the cap.
        assert_eq!(receive_presence(&vec![0u8; SEARCH_PRESENCE_MAX + 1], [5u8; 32], NOW_MS, &mut limits), Err(NotHeard::TooLong));
        // The author's own budget: four a minute.
        let mut tight = RateLimiter::new();
        for _ in 0..crate::protocol::constants::MAX_PRESENCE_PER_PEER_PER_MIN {
            assert!(receive_presence(&bytes, [1u8; 32], NOW_MS, &mut tight).is_ok());
        }
        assert_eq!(receive_presence(&bytes, [2u8; 32], NOW_MS, &mut tight), Err(NotHeard::TooMuch));
    }

    #[test]
    fn the_presence_heartbeat_fits_its_ttl_three_times() {
        assert!(SEARCH_PRESENCE_EVERY_MS * 3 <= SEARCH_PRESENCE_TTL_MS);
    }

    #[test]
    fn a_request_is_clamped_never_refused() {
        assert_eq!(SearchRequest { id: 1, format: Format::Any, tables: 0, again: false }.checked().tables, 1);
        assert_eq!(SearchRequest { id: 1, format: Format::Any, tables: 9, again: false }.checked().tables, MAX_GAMES);
        assert_eq!(Format::parse(Format::FullRing.code()), Some(Format::FullRing));
        assert_eq!(Format::parse(0), None);
        assert!(Format::Any.accepts(10) && !Format::SixMax.accepts(9) && !Format::Any.accepts(1));
        assert_eq!(Format::FullRing.seats(), Some(10));
        assert!(Format::Any.compatible(Format::HeadsUp) && !Format::HeadsUp.compatible(Format::SixMax));
    }
}
