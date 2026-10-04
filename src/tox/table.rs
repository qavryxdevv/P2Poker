//! A table's traffic over a Tox group: the driver, and a [`TableTransport`]
//! onto it.
//!
//! # The shape, and why it is a thread and not a task
//!
//! `tox_iterate` wants a steady loop on **one** thread — the instance is not
//! thread-safe unless it is built with the experimental option this client does
//! not set — and everything toxcore does happens inside that call. So the Tox
//! instance lives on a dedicated OS thread and the rest of the client speaks to
//! it through channels, which is the same arrangement `ChannelTransport` has
//! with the libp2p swarm and for the same reason: the thing that must be driven
//! is borrowed for the whole run.
//!
//! The driver is where fragmentation lives. A caller hands over a whole
//! protocol message; the driver cuts it to Tox's 1373-byte packet, sends the
//! pieces, and puts messages back together on the way in. Nothing above this
//! module ever sees a fragment.
//!
//! # What the driver decides, and what it must not
//!
//! It decides **transport** questions: who to add as a Tox friend, when to
//! invite, when to accept an invitation, which group peer to remove. Every one
//! of those answers comes from the roster it was given.
//!
//! It decides **no protocol question at all**. In particular
//! [`FromTable::claimed`] is left `None`: a Tox group peer id resolves to a Tox
//! public key, which is not a player's signing key and is not evidence about
//! one. Who signed a message is settled after reassembly, by the signature
//! inside it against the ratified roster — `table::transport`'s rule, and the
//! reason a chat id travelling in a public advertisement costs nothing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc as sync_mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::table::fragment::{self, Reassembler};
use crate::table::transport::{FromTable, PlayerId, TableTransport, TransportError};
use crate::tox::{Event, Tox};

/// How this client stands to the table's group.
/// **Starve this joiner's group handshake, on purpose, so `S1-AA` shape (i) can
/// be measured rather than waited for.**
///
/// `P2P_POKER_STALL_JOIN=<seconds>` makes a joiner stop iterating toxcore for
/// that long immediately after it accepts an invitation. The handshake needs
/// four attempts at `GC_SEND_HANDSHAKE_INTERVAL` = 3 s inside
/// `GC_UNCONFIRMED_PEER_TIMEOUT` = 12 s, so anything above twelve reaps the
/// inviter's address-less entry: no `peer_exit` fires, no log is written, and
/// libtoxcore has no path back, because a fresh invitation is refused inside
/// `Messenger.c` when a chat with that id already exists.
///
/// # It is blunter than the failure it models, and the measurement said so
///
/// Not iterating stops **everything**, not only the handshake. Measured at 30 s
/// on a four-seat table: the seat came back with `tox self udp` but only **one
/// of three** friendships up, restarted its join three times to the
/// `MAX_REJOINS` bound, and never got past `group 1 seen/0 confirmed/3`. The
/// recovery ran, was counted, and stopped where it should — and it did not
/// rescue the seat.
///
/// The measured `S1-AA` shape (i) is narrower than that: `tox friends up 3`,
/// `tox self udp`, everything healthy except the group. **So this knob proves
/// the recovery mechanism runs and does not prove it cures the real failure.**
/// A shorter stall — thirteen to fifteen seconds — trips the twelve-second
/// group reaper while the friend connections, which time out far later,
/// survive. That is the closer model and it is what to reach for.
///
/// **It fires ONCE per process, and that is a 2026-09-07 correction.** The
/// sleep sits in the invite-accept arm and this function caches its variable, so
/// every acceptance used to sleep -- including the ones the driver's own
/// `MAX_REJOINS` recovery makes. A run reporting *"group join restarted 3
/// time(s)"* was reporting three re-stalls, and the harness was guaranteeing
/// that the recovery it exists to test could not work. Reading that as three
/// failed recoveries is exactly the mistake the arrangement invited.
///
/// **Why a knob and not a wait.** The failure appeared in seven runs of 134 and
/// nothing makes it happen. `-DivergeAt` exists for the same reason and the row
/// that added it says why: so §6.3's answer *“can be measured rather than
/// reasoned about”*. A recovery nothing has ever triggered is a recovery nobody
/// has tested.
///
/// Compiled out entirely without `--features fault-harness`, where this is the
/// constant zero and the environment is never read.
#[cfg(feature = "fault-harness")]
fn stall_join_secs() -> u64 {
    use std::sync::OnceLock;
    static SECS: OnceLock<u64> = OnceLock::new();
    *SECS.get_or_init(|| {
        std::env::var("P2P_POKER_STALL_JOIN")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(0)
    })
}

/// Zero, in every build that did not ask for the harness.
#[cfg(not(feature = "fault-harness"))]
fn stall_join_secs() -> u64 {
    0
}

/// fault-harness: the line cut at run time, until this moment -- the node's
/// `P2P_POKER_AT_SET=cut:<s>`, applied when the table is set.
#[cfg(feature = "fault-harness")]
static CUT_UNTIL: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

/// fault-harness: cut this client's line from now until `until`.
#[cfg(feature = "fault-harness")]
pub fn cut_line_until(until: Instant) {
    if let Ok(mut u) = CUT_UNTIL.lock() {
        *u = Some(until);
    }
}

/// **The client's internet goes away, at the socket** (`patches/0035`).
///
/// `P2P_POKER_OFFLINE_AT=<s>` and `P2P_POKER_OFFLINE_FOR=<s>`: from that
/// second of the driver's life -- the client's start, since `D-042` starts
/// the instance with the client -- and for that long, nothing this instance
/// sends leaves the machine and nothing reaches it, and the library does what
/// a real outage makes it do: friends go offline, the table's group times its
/// members out at 58 s, relays are dropped, and all of it comes back through
/// the DHT once the line does. The process, the node loop and the lobby's own
/// transport live on.
///
/// **Different from `P2P_POKER_LINK_DOWN_AT`**, which drops the table's
/// messages above a transport that stays up and so never makes the library
/// forget anybody: the owner's report of 2026-09-12 -- a seat's internet cut
/// mid-hand, both sides choosing to wait, and no way back once the line
/// returned (`S1-EB`) -- lives entirely past the library's timeout, where
/// that knob cannot reach. Compiled out without `--features fault-harness`.
///
/// `P2P_POKER_OFFLINE_EVERY=<s>`: the outage again every that many seconds
/// from the first, for the same length each time -- four absences in one run
/// is how `D-047` is measured. Zero, the default, cuts once.
#[cfg(feature = "fault-harness")]
fn offline_window() -> Option<(Duration, Duration, Duration)> {
    use std::sync::OnceLock;
    static WINDOW: OnceLock<Option<(Duration, Duration, Duration)>> = OnceLock::new();
    *WINDOW.get_or_init(|| {
        let read = |name: &str| {
            std::env::var(name)
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(0)
        };
        let at = read("P2P_POKER_OFFLINE_AT");
        let dur = read("P2P_POKER_OFFLINE_FOR");
        let every = read("P2P_POKER_OFFLINE_EVERY");
        (at > 0 && dur > 0)
            .then(|| (Duration::from_secs(at), Duration::from_secs(dur), Duration::from_secs(every)))
    })
}

/// Whether an outage `window` -- from `at`, for `dur`, again every `every`
/// (zero: once) -- covers `since_start` of the client's life.
#[cfg_attr(not(feature = "fault-harness"), allow(dead_code))]
fn offline_in(window: Option<(Duration, Duration, Duration)>, since_start: Duration) -> bool {
    window.is_some_and(|(at, dur, every)| match since_start.checked_sub(at) {
        None => false,
        Some(since) if every.is_zero() => since < dur,
        Some(since) => since.as_secs() % every.as_secs() < dur.as_secs(),
    })
}

/// fault-harness, `S1-IB`/`S1-ID`: whether the harness's outage covers the
/// lobby's transport as well as the table's line, `since_start` of the
/// client's life -- `P2P_POKER_OFFLINE_BOTH=1` beside the window above. The
/// table's line is cut at the socket by this driver; `net::run` reads this
/// and takes every libp2p connection down with it, so the founder of a
/// search table goes dark on both lines, its advert going stale, as the
/// owner's far client did on 2026-09-18.
#[cfg(feature = "fault-harness")]
pub fn both_lines_dark(since_start: Duration) -> bool {
    use std::sync::OnceLock;
    static BOTH: OnceLock<bool> = OnceLock::new();
    *BOTH.get_or_init(|| std::env::var_os("P2P_POKER_OFFLINE_BOTH").is_some_and(|v| v == "1"))
        && offline_in(offline_window(), since_start)
}

/// fault-harness, `D-051`: what a flooder sends.
#[cfg(feature = "fault-harness")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FloodKind {
    /// Lossy packets of random bytes, longer than any fragment this build
    /// cuts: each one a refused fragment at every member.
    Noise,
    /// Lossless packets of random bytes a fragment long.
    Lossless,
    /// Well-framed one-fragment messages of random bytes: every one reaches
    /// the node, which finds no signed event in it.
    Frames,
    /// Well-framed first halves of two-fragment messages that never finish:
    /// nothing is refused anywhere, so only the count says it is a flood.
    Halves,
}

/// fault-harness, `D-051`: `P2P_POKER_FLOOD_RATE`, packets a second, 500
/// unless said -- for this client's flood and for its stranger's.
#[cfg(feature = "fault-harness")]
fn flood_rate() -> u64 {
    std::env::var("P2P_POKER_FLOOD_RATE")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(500)
        .max(1)
}

/// fault-harness, `D-051`: this client floods every table's group from
/// `P2P_POKER_FLOOD_AT`, that second of the driver's life, at the flood rate,
/// with `P2P_POKER_FLOOD_KIND` (`noise`, `lossless`, `frames`, `halves`;
/// `noise` unless said).
#[cfg(feature = "fault-harness")]
fn flood_plan() -> Option<(Duration, u64, FloodKind)> {
    use std::sync::OnceLock;
    static PLAN: OnceLock<Option<(Duration, u64, FloodKind)>> = OnceLock::new();
    *PLAN.get_or_init(|| {
        let at = std::env::var("P2P_POKER_FLOOD_AT").ok()?.trim().parse::<u64>().ok()?;
        let rate = flood_rate();
        let kind = match std::env::var("P2P_POKER_FLOOD_KIND").unwrap_or_default().trim() {
            "lossless" => FloodKind::Lossless,
            "frames" => FloodKind::Frames,
            "halves" => FloodKind::Halves,
            _ => FloodKind::Noise,
        };
        Some((Duration::from_secs(at), rate, kind))
    })
}

/// fault-harness, `D-051`: a stranger -- a second Tox instance in this process,
/// no seat of any table -- that this client befriends at that second and
/// invites into its first table's group, as any member of a private group
/// can. `P2P_POKER_STRANGER_FLOOD=1`: once in, the stranger floods the group
/// at the flood rate. `P2P_POKER_STRANGER_NAME=copy`: its name is a copy of a
/// seat's binding.
#[cfg(feature = "fault-harness")]
fn stranger_plan() -> Option<(Duration, bool, bool)> {
    use std::sync::OnceLock;
    static PLAN: OnceLock<Option<(Duration, bool, bool)>> = OnceLock::new();
    *PLAN.get_or_init(|| {
        let at = std::env::var("P2P_POKER_STRANGER_AT").ok()?.trim().parse::<u64>().ok()?;
        let flood = std::env::var("P2P_POKER_STRANGER_FLOOD").is_ok_and(|v| v.trim() == "1");
        let copy = std::env::var("P2P_POKER_STRANGER_NAME").is_ok_and(|v| v.trim() == "copy");
        Some((Duration::from_secs(at), flood, copy))
    })
}

/// fault-harness, `S1-KI`: `P2P_POKER_PUPPETS=N` -- with `P2P_POKER_STRANGER_AT`,
/// N strangers in place of one, each named with THIS client's own binding for
/// its member key: entries of this client's seat that its player brought into
/// the group itself, as a rogue can.
#[cfg(feature = "fault-harness")]
fn puppets() -> Option<usize> {
    use std::sync::OnceLock;
    static N: OnceLock<Option<usize>> = OnceLock::new();
    *N.get_or_init(|| {
        std::env::var("P2P_POKER_PUPPETS")
            .ok()?
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|n| *n > 0)
            .map(|n| n.min(16))
    })
}

/// fault-harness, `S1-KI`: `P2P_POKER_COUNT_BY_MEMBER=1` -- the control: the
/// group counted by member and metered by member key, and no entry cut for
/// being one too many, as before `S1-KI`.
#[cfg(feature = "fault-harness")]
fn count_by_member() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("P2P_POKER_COUNT_BY_MEMBER").is_ok_and(|v| v.trim() == "1"))
}

#[cfg(not(feature = "fault-harness"))]
fn count_by_member() -> bool {
    false
}

/// fault-harness, `S1-KH`: `P2P_POKER_NO_INVITES_AT=<s>` -- from that second
/// this client, where it is the founder, offers the table's group to nobody:
/// a rogue founder that keeps a seat out of the group without locking
/// anything.
#[cfg(feature = "fault-harness")]
fn no_invites_at() -> Option<Duration> {
    use std::sync::OnceLock;
    static AT: OnceLock<Option<Duration>> = OnceLock::new();
    *AT.get_or_init(|| {
        std::env::var("P2P_POKER_NO_INVITES_AT")
            .ok()?
            .trim()
            .parse::<u64>()
            .ok()
            .map(Duration::from_secs)
    })
}

/// fault-harness: set once `P2P_POKER_NO_INVITES_AT`'s second is reached.
#[cfg(feature = "fault-harness")]
static NO_INVITES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether this client's invitations as the founder have stopped -- the
/// harness's rogue founder (`S1-KH`); never in a build without the harness.
fn founder_stopped_offering() -> bool {
    #[cfg(feature = "fault-harness")]
    {
        NO_INVITES.load(Ordering::Relaxed)
    }
    #[cfg(not(feature = "fault-harness"))]
    {
        false
    }
}

/// fault-harness: the stranger's instance and how far it got.
#[cfg(feature = "fault-harness")]
struct Stranger {
    tox: Tox,
    /// The stranger as this client's friend, and this client as the stranger's.
    friend_here: u32,
    /// When this client last invited it, until it takes an invitation.
    invited: Option<Instant>,
    group: Option<u32>,
    joined: bool,
    flood_sent: u64,
    flood_since: Option<Instant>,
}

/// fault-harness: a xorshift, so a flood costs no dependency and no entropy.
#[cfg(feature = "fault-harness")]
fn junk(state: &mut u64, len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        out.extend_from_slice(&state.to_le_bytes());
    }
    out.truncate(len);
    out
}

/// fault-harness: one flood packet of this kind into this group.
#[cfg(feature = "fault-harness")]
fn flood_one(tox: &mut Tox, g: u32, kind: FloodKind, state: &mut u64, id: u32) -> bool {
    match kind {
        FloodKind::Noise => {
            let bytes = junk(state, 1_200);
            tox.send_lossy(g, &bytes)
        }
        FloodKind::Lossless => {
            let bytes = junk(state, fragment::TOX_PACKET);
            tox.send(g, &bytes).is_ok()
        }
        FloodKind::Frames | FloodKind::Halves => {
            let (index, total) = if kind == FloodKind::Frames { (0u16, 1u16) } else { (0u16, 2u16) };
            let mut frame = Vec::with_capacity(fragment::TOX_PACKET);
            frame.extend_from_slice(&id.to_be_bytes());
            frame.extend_from_slice(&index.to_be_bytes());
            frame.extend_from_slice(&total.to_be_bytes());
            frame.extend_from_slice(&junk(state, fragment::payload_for(fragment::TOX_PACKET)));
            tox.send(g, &frame).is_ok()
        }
    }
}

/// How long a joiner waits for its own join to finish before it gives it up.
///
/// **Comfortably past toxcore's own reaper, and deliberately so.**
/// `GC_UNCONFIRMED_PEER_TIMEOUT` is twelve seconds, and the handshake has about
/// four attempts inside it at `GC_SEND_HANDSHAKE_INTERVAL` = 3 s. Twenty-five
/// seconds is long enough that a slow handshake is never interrupted and short
/// enough that a seat is not lost for a whole tournament — measured group entry
/// on one machine is 10-40 s, but that is entry *finishing*, which is exactly
/// what `self_join` reports and what this waits for.
const JOIN_GRACE: Duration = Duration::from_secs(25);

/// How many times a joiner will do that before it stops.
///
/// **A client that leaves and rejoins for ever is worse than one that sits
/// still**: it burns the founder's invitations and looks, from every other
/// seat, like a peer flapping. Three attempts is seventy-five seconds of
/// trying; past that the failure is not transient and the count on the status
/// line is the useful thing.
const MAX_REJOINS: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// The founder. Creates the group, invites every seat, removes anybody the
    /// roster does not seat.
    Host,
    /// A player. Waits to be invited by the founder and by nobody else.
    Joiner {
        /// The founder's **Tox** public key, from the table's advertisement.
        /// An invitation from any other friend is refused.
        founder: [u8; 32],
        /// The `chat_id` the advertisement named, if it named one.
        ///
        /// **An invitation says nothing about which group it is for.** Without
        /// this the founder could invite a player into a group nobody
        /// advertised, and every other client would be watching a different
        /// one — a table whose traffic nobody else can see, which is the shape
        /// a founder colluding with one player would want. (`D-037`: the
        /// founder itself, back after a restart, is `Back` below -- the same
        /// check, an invitation from any roster member.) So the group is
        /// joined, its id read back, and compared; a mismatch is left again at
        /// once.
        ///
        /// `None` when the advertisement named none, which is an advert from a
        /// build with no Tox. There is then nothing to compare against and
        /// nothing to be invited to, so an invitation is refused outright
        /// rather than accepted on trust.
        chat_id: Option<[u8; 32]>,
    },
    /// `D-037`: the founder, back after a restart. The group it created went
    /// with the old process; the members still hold it, and one of them
    /// offers it again when this key's friendship comes back up. Any roster
    /// member's invitation is taken, and only into the group the
    /// advertisement named.
    Back {
        chat_id: Option<[u8; 32]>,
    },
}

/// What the driver needs before it starts.
#[derive(Debug, Clone)]
pub struct Setup {
    pub role: Role,
    /// The group's name. Cosmetic — a table is identified by its `table_id` and
    /// its roster, never by what somebody called a chat room, and the driver
    /// never reads the name off an invitation for that reason.
    pub group_name: String,
    /// This client's display name inside the group. Cosmetic in the same way.
    pub self_name: String,
    /// Every **Tox** public key on the ratified roster, this client's excepted.
    ///
    /// Both ends add each other, because a Tox friendship is two-sided and one
    /// side of it establishes nothing.
    pub roster: Vec<[u8; 32]>,
    /// `D-051`: this seat's signing key, for the member binding its name in
    /// the group carries (`table::membership`). With it the driver also
    /// holds every other member to one; `None` only where there is no seat
    /// to sign for -- the tests of the transport alone.
    pub binder: Option<ed25519_dalek::SigningKey>,
}

/// Something the client tells the driver after it has started.
#[derive(Debug, Clone)]
pub enum Command {
    /// A seat joined: add it, and invite it if this client is the founder.
    Seated([u8; 32]),
    /// A seat left the roster: remove it from the group. Founder only, and a
    /// no-op elsewhere — a peer that is not the admin cannot remove anybody,
    /// and pretending otherwise would put a second membership answer beside
    /// the roster's.
    Unseated([u8; 32]),
    /// `D-044`: the roster as the formation holds it now, by Tox key; seats
    /// it no longer names leave this table's roster here.
    Roster(Vec<[u8; 32]>),
    /// `D-045`: the table's word removed a seat -- the players' certificate
    /// with the group's silence, or the roster said again before the first
    /// hand (`for_good`). Every entry the seat has, by application key or by
    /// the line its invitation went over, is allowed to be kicked, kicked
    /// where this client is the founder, and dropped from this client's
    /// view of the group.
    Remove {
        app_key: Option<[u8; 32]>,
        tox_key: Option<[u8; 32]>,
        for_good: bool,
    },
    /// fault-harness: the founder kicks this seat without the table's word,
    /// for measuring that nobody honours it (`D-045`).
    KickWithoutWord([u8; 32]),
    /// fault-harness (`S1-KH`): the founder locks the group -- the peer limit
    /// to one, a password, public -- as a rogue founder can.
    LockGroup,
    /// **`patches/0011`. Ask this seat for the message a stage is waiting on.**
    ///
    /// A lost group message is normally repaired in one round trip: the
    /// receiver sees a hole, sends `GR_ACK_REQ`, and the sender answers with an
    /// immediate retransmission and no backoff — 36 to 326 ms on the
    /// two-machine bed. But a hole is only *visible* once a later message has
    /// arrived, and a stage waiting on one seat usually has nothing later to
    /// reveal it. Recovery then falls to the sender's blind ladder, whose
    /// attempts land in the seconds T+3, T+5, T+9, T+17 and T+33 — against a
    /// 30 s stage budget, so the seat is voted out for a message that was in
    /// flight.
    ///
    /// The application is the only party that knows it is waiting. This carries
    /// that knowledge down to the carrier. Named by **application** key, which
    /// is what a seat is; the driver maps it through `known_as`.
    Nudge {
        app_key: [u8; 32],
        /// The seat this key sits at, so the driver can record what it finds in
        /// `Trouble::mid_delivery` without knowing anything about rosters.
        seat: u8,
    },
    /// A group peer's own key, and the application key that was verified to
    /// have signed a message from it. See `peer_for`.
    KnownAs {
        group_key: [u8; 32],
        app_key: [u8; 32],
    },
    /// **This seat is here again and needs the group offered to it afresh.**
    ///
    /// A client that restarts has left the group, and nothing the founder can
    /// see says so. `invited` records what this client *did*, not who is
    /// there, so `invite_pending` skips a peer it once invited for ever. The
    /// friend connection is no help either: toxcore's outlives an outage far
    /// longer than the ones that matter, so no down-edge fires to clear the
    /// entry — measured, a client back after **twenty seconds** sat at
    /// *waiting to be invited* for the rest of the run while the table
    /// finished thirty hands without it.
    ///
    /// Counting the group's members is no help either, and that was the first
    /// attempt: `tox_group_peer_get_public_key` still resolves for a peer whose
    /// client has died, so `peer_count` returns the whole roster and the group
    /// does not look short.
    ///
    /// **The signal that does work is the peer asking to join a table it
    /// already has a seat at.** A seat that is playing does not ask; one that
    /// asks has restarted and lost everything it held, the group among it.
    /// `net::run` sends this from the `AlreadySeated` path and nowhere else.
    Rejoined([u8; 32]),
    /// `D-049`: this seat sits out (`true`) or plays (`false`), said as the
    /// group's own status of this member.
    Away(bool),
    /// `D-051`: the application keys of the seats this table seats, and
    /// whether the list is the table's for good -- the roster ratified -- or
    /// still forming. A member whose binding names no key here is no seat.
    /// `S1-KO`: and each seat's line, the Tox key its join declared, by its
    /// application key -- what a member whose founder is out for good offers
    /// the group to a missing seat over.
    Seats {
        apps: Vec<[u8; 32]>,
        lines: Vec<([u8; 32], [u8; 32])>,
        fixed: bool,
    },
    /// `S1-KO`: the seats still in the game, by application key, this client's
    /// own among them -- with chips, not out for good, as this client's own hand
    /// counts them (`from_hand`), or its session record before it holds a hand
    /// again -- and of them the seats that hand counts as playing (`present`:
    /// dealt in or back, not certified out).
    InGame {
        apps: Vec<[u8; 32]>,
        present: Vec<[u8; 32]>,
        from_hand: bool,
    },
    /// `S1-KP` (`D-103`): the founder is away from the table's hands, as the
    /// node reads them -- certified out of them with chips for three hands or
    /// three minutes, or gone by its own signed word, until a hand has it back
    /// in the required set (`away`); `outside`: from the first such hand, for
    /// a copy held empty to take a counted seat's invitation. A seat back from
    /// a restart through the other seats is told both while it holds no hand.
    /// Never a bar: the founder's line stays, and its invitation is taken.
    FounderAway { away: bool, outside: bool },
    /// `D-051`: a whole message from this member was not a signed event, or
    /// did not verify under the key inside it -- which no client of this
    /// build sends.
    Noise { member_key: [u8; 32], points: u32 },
    /// Close this table: say what it still holds, leave its group. The
    /// instance and its thread stay for the next table, and the friends no
    /// open table needs go `FRIEND_LINGER` later (`D-042`).
    Leave,
}

/// What the driver is having trouble with, for a caller that wants to say so.
///
/// **Counters and not events**, because the driver has no channel to the
/// interface and should not grow one: it runs on its own thread, and a channel
/// it could block on is a channel that could stop `tox_iterate`.
///
/// They exist because two stalls in a row were diagnosed by reading logs that
/// said nothing at all. A group send that toxcore refuses is the whole
/// mechanism by which a busy table falls behind, and it was being swallowed.
#[derive(Default)]
pub struct Trouble {
    /// `S1-EB`: how many times an emptied copy of a table's group was left
    /// here so the group's fresh offer could be taken.
    pub left_empty: AtomicU64,
    /// **Which seats the carrier is mid-delivery with, one bit per seat.**
    ///
    /// Set while `gcc_recv_pending` reports messages from that seat sitting in
    /// the receive array — they arrived out of order, so the seat is *talking*
    /// and something earlier has not landed yet. That is the one fact which
    /// separates a seat that is late from a seat that is silent, and a timeout
    /// vote has never had it.
    ///
    /// Written by the driver when the application nudges a seat, which it does
    /// for exactly the seats a stage is waiting on. A bit is therefore as fresh
    /// as the last tick that cared about it, and stale bits cost nothing: the
    /// suppression they feed is bounded by the carrier's own patience.
    pub mid_delivery: AtomicU32,
    /// Fragments toxcore would not take. The queue was full or the group had
    /// nobody in it; either way the message stays and is tried again.
    pub refused: AtomicU64,
    /// **And which of those it was.** Indexed by
    /// `Tox_Err_Group_Send_Custom_Packet`: 0 ok, 1 group-not-found, 2 too-long,
    /// 3 empty, **4 disconnected**, 5 fail-send.
    ///
    /// The code was always there -- `Tox::send` returns
    /// `Failed::Api { call, error }` -- and `flush` threw it away for a bare
    /// `refused += 1`. That cost a whole reading. In `split182531-10` `n0`
    /// refused 2 481 fragments of 6 804 and `far-n1` 1 854 of 5 638, and
    /// nothing said whether the group was disconnected under this client's feet
    /// (code 4, decided in `tox_group_send_custom_packet` before any peer is
    /// consulted) or whether a peer would not take it (code 5, which is where
    /// `patches/0003` lives). Those have completely different remedies, and the
    /// difference is one array.
    ///
    /// It also settles a question about this project's own patch: 0003 logs
    /// *“custom packet reached N of M confirmed peers”* whenever it refuses,
    /// and that line appears **zero** times in that run -- so the refusals were
    /// upstream of it. This counter says which upstream.
    pub refused_why: [AtomicU64; 6],
    /// Whole messages waiting to go out. A number that does not come down is a
    /// transport that has stopped keeping up, which at a table shows as seats
    /// being certified late for saying things they did say.
    pub waiting: AtomicU64,
    /// Fragments handed to toxcore and accepted.
    pub sent: AtomicU64,
    /// **Hand events received off the group and thrown away because the node
    /// loop was not draining.**
    ///
    /// Dropping is the right thing to do — blocking here would stop
    /// `tox_iterate`, and a transport that stalls the network to wait for its
    /// reader loses the connection as well as the message. Dropping *silently*
    /// is not: a seat that never sees an event is a seat that diverges, and
    /// with no counter and no log line a fork of that shape is invisible in
    /// every log this client writes. Which is the same defect as reading a
    /// zero that was never measured.
    pub inbox_dropped: AtomicU64,
    /// `S1-DW`: claims refused -- a member's signed traffic said it is a
    /// seat that another confirmed member holds and has spoken from within
    /// the last twenty seconds: that seat's message said again, not a seat
    /// back under a fresh key (S1-DU), whose old entry has been quiet.
    pub claims_refused: AtomicU64,
    /// `S1-DB`: entries of a seat's dead process this client dropped from a
    /// group, the seat being there under a fresh key.
    pub stale_dropped: AtomicU64,
    /// `D-045`: members this client dropped from a group by the table's
    /// word, and the times this client was itself removed by it.
    pub removed: AtomicU64,
    pub kicked_out: AtomicU64,
    /// `D-051`: the members this client cut off since the node last asked --
    /// flooders, and members that are no seat of this table.
    pub cut_off: std::sync::Mutex<Vec<CutOff>>,
    /// `D-051`: messages held back from a member sending past `INBOX_LOUD`
    /// while the inbox was three quarters full.
    pub inbox_yielded: AtomicU64,
    /// `D-035`: the roster seats whose client is a confirmed member of the
    /// group right now, by application key -- recomputed every sweep, for
    /// the window's link indicator.
    pub present: std::sync::Mutex<std::collections::HashSet<[u8; 32]>>,
    /// `D-041`: the roster seats whose friend connection is up right now,
    /// by Tox key -- a seat that has not spoken in the group yet is still
    /// on the line by this.
    pub friends_on: std::sync::Mutex<std::collections::HashSet<[u8; 32]>>,
    /// `S1-DS`: every application key this table's group has taught the
    /// driver (`Command::KnownAs`). A seat here that is not `present` has
    /// spoken in the group and is not in it now: gone, whatever its friend
    /// link says -- a friendship lingers two minutes past a table (D-042).
    pub known: std::sync::Mutex<std::collections::HashSet<[u8; 32]>>,
    /// `S1-DT`: how many seconds ago each present seat's last packet arrived,
    /// by application key -- read every sweep, so a seat whose client died
    /// is quiet here long before the library gives it up (58 s).
    pub quiet: std::sync::Mutex<HashMap<[u8; 32], u64>>,
    /// `D-049`: the present seats whose group status says they sit out, by
    /// application key -- each seat as its most recently heard entry says.
    pub away: std::sync::Mutex<std::collections::HashSet<[u8; 32]>>,
    /// `D-049`: the same by the Tox key of the line a member came in over.
    pub away_lines: std::sync::Mutex<std::collections::HashSet<[u8; 32]>>,
    /// `D-035`: seats whose client left the group since the node last
    /// asked, by application key, each with whether it quit on purpose.
    pub gone: std::sync::Mutex<Vec<([u8; 32], bool)>>,
    /// `S1-DV`: the same three by the TOX key of the friend a member's
    /// invitation came over (`patches/0033`) -- known from the join, so
    /// they say something before the first hand, when the group has taught
    /// no application key yet: present members, their silence, and exits.
    pub present_lines: std::sync::Mutex<std::collections::HashSet<[u8; 32]>>,
    pub quiet_lines: std::sync::Mutex<HashMap<[u8; 32], u64>>,
    pub gone_lines: std::sync::Mutex<Vec<([u8; 32], bool)>>,
    /// Whether every other seat on the roster is in the group right now.
    ///
    /// **Here because a table whose group is not complete deals a hand nobody
    /// receives.** The hand rides the group (D-019) and the roster ratifies on
    /// libp2p, which is now much the faster of the two: formation completes in
    /// about seven seconds and group entry runs on toxcore's LAN discovery
    /// cadence, ten to forty. So a seat can ratify, open hand 1, and be alone
    /// with it - measured once, a seat that entered the group at 15.1 s and
    /// received not one fragment before its own clock ran out at 66.2 s, while
    /// the founder played on to hand 22. Nothing catches such a peer up: the
    /// re-send window is the last three stages of the hand in progress.
    ///
    /// Written by the driver's sweep, read by the node loop, and **false until
    /// the sweep has run at least once**, so a caller that gates on it waits
    /// rather than races.
    pub complete: AtomicBool,
    /// What the last sweep counted in the group, and what it wanted.
    ///
    /// **Reported because `complete` alone cannot say why it is false.** The
    /// founder's gate began working the moment it counted instead of matching
    /// keys, and every joiner still ran to the sixty-second fallback — and
    /// nothing in a log distinguished *the group really is short* from *this
    /// peer cannot see the others yet*. Two numbers do.
    pub in_group: AtomicU64,
    pub want_in_group: AtomicU64,
    /// `S1-KI`: the OTHER seats the group holds, a seat once however many entries
    /// it has in the group (`seats_here`) -- where `in_group` counts members,
    /// which is what says one arrived.
    pub seats_in_group: AtomicU64,
    /// `S1-KH`: the group's shared state keeps members out -- a peer limit or a
    /// password, which no client of this build sets -- as first read once this
    /// copy of the group had confirmed a member. Kept for the table's life: a
    /// founder that locked its group once runs a client that is not ours.
    pub group_lock: std::sync::Mutex<Option<crate::tox::GroupLock>>,
    /// `S1-KJ`: seats, by application key, seen in the group under a key they
    /// never had here before -- a new process of the seat, whose hand may sign
    /// again a step its previous life signed (`D-033`) -- since the node last
    /// asked.
    pub new_entries: std::sync::Mutex<Vec<[u8; 32]>>,
    /// Invitations toxcore **accepted** from the founder.
    ///
    /// Beside `invites_refused` because zero refusals means one of two very
    /// different things — every invitation went, or none was attempted — and a
    /// reconnection that never completes looks identical under both. Three
    /// fixes were made against that ambiguity before this counter existed.
    pub invites_sent: AtomicU64,
    /// The friends the founder currently believes are connected, so an
    /// invitation has somewhere to go. `invite_pending` sends to these and to
    /// no others.
    pub friends_up: AtomicU64,
    /// This instance's own connection to the Tox network: `0` none, `1` TCP,
    /// `2` UDP.
    ///
    /// **The first question to ask of a peer that cannot be reached**, and
    /// `S1-N` spent five hypotheses without it: a client whose own Tox never
    /// reaches the network cannot be found by anybody, and that looks from the
    /// outside exactly like a friendship that will not form.
    pub self_connection: AtomicU64,
    /// Invitations the founder tried to send and toxcore refused.
    ///
    /// **Here because a seat arriving late is two different failures that look
    /// the same in a log.** Either the friend connection has not come up yet —
    /// nothing to invite over, and only waiting fixes it — or the invitation
    /// was attempted and refused. Measured at six seats, group entry landed at
    /// 15, 35, 40, 40 and 40 seconds, and nothing said which of the two it was.
    pub invites_refused: AtomicU64,
    /// **How many times this client gave a stalled join up and started again.**
    ///
    /// Zero on a healthy run. A number here is `S1-AA` shape (i) happening and
    /// being survived, which is the difference between a seat that recovers and
    /// one that sits at `group 0/N` for the rest of the tournament.
    /// **Peers toxcore has confirmed in the group**, which is not the same as
    /// the peers it counts.
    ///
    /// `peer_count` walks the peer list and does not check `confirmed`, while
    /// the group send path skips exactly the peers that are not confirmed — and
    /// reports success when there are none. So `in_group` could read `3/3`
    /// while a broadcast reached nobody, and every reading of the status line
    /// before this had to hedge about it. `seen` beside `confirmed` is what
    /// makes the difference visible: `seen > confirmed` is the library's skip
    /// happening, and the two agreeing rules it out.
    pub confirmed_peers: AtomicU64,
    /// **How this joiner reaches the founder: `0` not at all, `1` over a TCP
    /// relay, `2` directly over UDP.** `3` on a founder, which has no founder
    /// of its own.
    ///
    /// The number `S1-AA` shape (i) turned out to need. `handle_gc_invite_
    /// confirmed_packet` accepts a join only if the confirmation carried a TCP
    /// relay **or** the inviter's `IP:port` could be copied off the friend
    /// connection, and returns `-5` with *"Got invalid connection info from
    /// peer"* when it has neither. Which of the two failed is not something the
    /// client could see, and the retry was blind to it: it re-entered as soon
    /// as *any* friendship was up, which says nothing about whether the
    /// founder's link carries an address.
    pub founder_link: AtomicU64,
    pub rejoins: AtomicU64,
    /// Joins toxcore itself abandoned, with `tox_group_join_fail`.
    ///
    /// Distinct from [`rejoins`](Self::rejoins): this is the library saying so,
    /// that is this client noticing silence.
    pub join_fails: AtomicU64,
}

/// `D-051`: one member this client cut off from a table's group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutOff {
    /// The application key the member's binding named, when it had one.
    pub app_key: Option<[u8; 32]>,
    pub member_key: [u8; 32],
    pub why: Cut,
}

/// `D-051`: why a member was cut off. Every reason rests on what reached this
/// client from that member's key, or on the binding in its name against the
/// roster -- on nobody's word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cut {
    /// It flooded the group. Its seat is barred here for the table's life.
    Flood(crate::table::membership::Flood),
    /// Its binding names an application key this table does not seat.
    NotASeat,
    /// Its name is a binding made for another member or another group: a
    /// copy, or a forgery.
    NotItsBinding,
    /// It carried no binding within `NAME_GRACE` of joining.
    Nameless,
    /// It is bound to a seat barred here: one this client cut off for
    /// flooding, or one the table put out for good.
    Barred,
    /// `S1-KI`: its seat holds `ENTRIES_A_SEAT` other entries in the group the
    /// group has heard from more recently.
    Surplus,
}

impl std::fmt::Display for Cut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cut::Flood(why) => write!(f, "it flooded the table's group ({why})"),
            Cut::NotASeat => write!(f, "its name binds it to no seat of this table"),
            Cut::NotItsBinding => write!(f, "its name is a binding made for another member"),
            Cut::Nameless => write!(f, "it said no seat within {} s of joining", NAME_GRACE.as_secs()),
            Cut::Barred => write!(f, "it is bound to a seat barred from this table"),
            Cut::Surplus => write!(
                f,
                "its seat holds {ENTRIES_A_SEAT} other entries in the group that spoke more recently"
            ),
        }
    }
}

/// `D-051`: how long a member may be in the group before its name binds it to
/// a seat. A seat's client names itself before its first handshake, so the
/// binding arrives with the member; this is a margin, not a wait.
pub const NAME_GRACE: Duration = Duration::from_secs(10);

/// `D-051`: packets in one second from one member past which, with the inbox
/// three quarters full, its messages wait for the others'.
pub const INBOX_LOUD: u64 = 100;

/// A table's number inside the client's one driver (`D-042`).
pub type TableId = u32;

/// What a handle sends the driver: a table to open, a table's command, or the
/// end of the client.
enum Ctl {
    Open {
        id: TableId,
        setup: Setup,
        out: tokio::sync::mpsc::Receiver<Vec<u8>>,
        inbox: tokio::sync::mpsc::Sender<FromTable>,
        chat: tokio::sync::watch::Sender<Option<[u8; 32]>>,
        trouble: Arc<Trouble>,
    },
    For {
        id: TableId,
        command: Command,
    },
    Stop,
}

/// **The client's one Tox instance, on its own thread, for every table this
/// client sits at (`D-042`).**
///
/// It used to be an instance per table, made when the table was founded or
/// joined and killed when it was left. Each new instance bootstrapped into the
/// Tox DHT from nothing and found its friends again from nothing, so a second
/// table in the same client had *tox friends up 0* for its first minute and
/// dealt its first hand 85 s after the hosting against 20 s for a first table
/// (`S1-DN`, `run202725-3`) -- and the owner's players restarted their clients
/// to sit at a new table. The instance now starts with the client and carries
/// every table as a group; a table is a group and a roster, and closing it
/// leaves the group. The friendships are the instance's, shared by every
/// table, and go `FRIEND_LINGER` after the last table that needed them.
///
/// Started by the node at launch (`TableSink::boot`), so the DHT is warm before
/// any table exists.
pub struct Driver {
    control: sync_mpsc::Sender<Ctl>,
    next: AtomicU32,
    mine: [u8; 32],
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Driver {
    /// Take the instance onto its thread and keep it there for the client's
    /// life. Bootstrapping is the caller's, before this.
    pub fn start(tox: Tox) -> Driver {
        Driver::start_with_nodes(tox, Vec::new())
    }

    /// `S1-FB`: start the driver with the bootstrap nodes the instance was
    /// started from, so it can offer them again while the instance is offline.
    /// An empty list, as [`start`](Self::start) gives, never bootstraps again.
    pub fn start_with_nodes(tox: Tox, nodes: Vec<crate::tox::nodes::Node>) -> Driver {
        let mine: [u8; 32] = tox.address()[..32].try_into().unwrap_or([0u8; 32]);
        let (ctl_tx, ctl_rx) = sync_mpsc::channel::<Ctl>();
        let thread = std::thread::Builder::new()
            .name("tox".into())
            .spawn(move || run(tox, ctl_rx, nodes))
            .expect("a thread for the client's Tox instance");
        Driver {
            control: ctl_tx,
            next: AtomicU32::new(1),
            mine,
            thread: Some(thread),
        }
    }

    /// This client's own Tox public key.
    pub fn mine(&self) -> [u8; 32] {
        self.mine
    }

    /// Open a table: its group (created here for a founder, awaited from an
    /// invitation otherwise), its roster, its own channels and counters.
    pub fn open(&self, setup: Setup) -> ToxTable {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        // **Deep enough for a re-send burst plus the event that matters.** The
        // node re-broadcasts everything it has said every five seconds, which
        // is up to sixty-four messages at once, and a fresh action arriving
        // while that backlog is in the channel must not be the one dropped.
        let (out_tx, out_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(512);
        // **Deep enough to absorb one peer's ring releasing at once.** A peer
        // whose stream is blocked buffers up to GCC_BUFFER_SIZE = 2048
        // messages; when the missing one arrives they are all delivered in a
        // rush. Measured, `split101212-10`: 1 282 hand events dropped on one
        // node at 256, in bursts of 174 to 602.
        let (in_tx, in_rx) = tokio::sync::mpsc::channel::<FromTable>(2_048);
        let (chat_tx, chat_rx) = tokio::sync::watch::channel::<Option<[u8; 32]>>(None);
        let trouble = Arc::new(Trouble::default());
        let _ = self.control.send(Ctl::Open {
            id,
            setup,
            out: out_rx,
            inbox: in_tx,
            chat: chat_tx,
            trouble: Arc::clone(&trouble),
        });
        ToxTable {
            id,
            out: out_tx,
            inbox: in_rx,
            control: self.control.clone(),
            chat: chat_rx,
            trouble,
        }
    }
}

impl Drop for Driver {
    /// The client is ending: every table is closed, what they still held is
    /// said, and the thread is joined -- it owns a socket, and a client that
    /// exits while one is still running leaves a seat looking occupied to
    /// everybody else.
    fn drop(&mut self) {
        let _ = self.control.send(Ctl::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The handle the rest of the client holds: one table at the driver.
///
/// Implements [`TableTransport`], so nothing above it knows a Tox group is
/// underneath -- which is the whole point of `table::transport` existing.
pub struct ToxTable {
    id: TableId,
    out: tokio::sync::mpsc::Sender<Vec<u8>>,
    inbox: tokio::sync::mpsc::Receiver<FromTable>,
    control: sync_mpsc::Sender<Ctl>,
    /// The group's `chat_id`, once there is a group.
    ///
    /// The founder **needs** this: `TableAd::on_tox` puts it in the
    /// advertisement, and until it is there nobody can check that the group
    /// they were invited into is the one the table named. A `watch` rather
    /// than a return value because the driver creates the group on its own
    /// thread, and a constructor that blocked until it did would block the
    /// client on a socket.
    chat: tokio::sync::watch::Receiver<Option<[u8; 32]>>,
    trouble: Arc<Trouble>,
}

impl ToxTable {
    /// What the transport is having trouble with, if anything.
    pub fn trouble(&self) -> &Trouble {
        &self.trouble
    }

    /// Send one whole message without waiting. **Not async.**
    ///
    /// The node loop publishes from inside arms that already hold the swarm,
    /// and an `await` there would be an await in the middle of handling one
    /// event. The channel is bounded, so a caller that outruns the driver gets
    /// `false` rather than a stall -- and a message dropped here is re-sent by
    /// the loop that exists because no transport in this design keeps history.
    pub fn try_broadcast(&self, bytes: &[u8]) -> bool {
        bytes.len() <= fragment::MAX_MESSAGE && self.out.try_send(bytes.to_vec()).is_ok()
    }

    /// The group's `chat_id`, or `None` while there is not one yet.
    pub fn chat_id(&self) -> Option<[u8; 32]> {
        *self.chat.borrow()
    }

    /// Wait until there is a group, and give back its `chat_id`.
    ///
    /// `None` if the table was closed without ever having one -- a founder
    /// whose `tox_group_new` failed, or a joiner that was never invited.
    pub async fn wait_for_chat_id(&mut self) -> Option<[u8; 32]> {
        loop {
            if let Some(id) = *self.chat.borrow_and_update() {
                return Some(id);
            }
            if self.chat.changed().await.is_err() {
                return None;
            }
        }
    }

    /// Tell the driver something the roster decided.
    ///
    /// Best effort: a driver that has already stopped answers nothing, which is
    /// the same as the table being over.
    pub fn tell(&self, c: Command) {
        let _ = self.control.send(Ctl::For {
            id: self.id,
            command: c,
        });
    }
}

impl Drop for ToxTable {
    /// Closing the handle closes the table: what it still holds is said, its
    /// group is left. The instance stays for the next table (`D-042`).
    fn drop(&mut self) {
        let _ = self.control.send(Ctl::For {
            id: self.id,
            command: Command::Leave,
        });
    }
}

/// One instance for one table, as before `D-042` -- for the tests, whose
/// driver then outlives the handle for the process's life.
pub fn spawn(tox: Tox, setup: Setup) -> ToxTable {
    let driver = Driver::start(tox);
    let table = driver.open(setup);
    std::mem::forget(driver);
    table
}

#[async_trait::async_trait]
impl TableTransport for ToxTable {
    async fn broadcast(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        if bytes.len() > fragment::MAX_MESSAGE {
            return Err(TransportError::TooLarge {
                bytes: bytes.len(),
                cap: fragment::MAX_MESSAGE,
            });
        }
        self.out
            .send(bytes.to_vec())
            .await
            .map_err(|_| TransportError::Closed)
    }

    /// One group, so a message to one player is a message to the table.
    ///
    /// Tox has `tox_group_send_custom_private_packet` and it is deliberately not
    /// used: everything this transport carries is signed and a snapshot is
    /// public to the table by construction, so a private packet would buy
    /// nothing and would add a second path with its own delivery rules.
    async fn send_to(&mut self, _to: &PlayerId, bytes: &[u8]) -> Result<(), TransportError> {
        self.broadcast(bytes).await
    }

    async fn next(&mut self) -> Option<FromTable> {
        self.inbox.recv().await
    }

    async fn leave(&mut self) {
        let _ = self.control.send(Ctl::For {
            id: self.id,
            command: Command::Leave,
        });
        self.inbox.close();
    }

    /// The largest **message**, not the largest packet.
    ///
    /// A caller decides what to send against this; the driver decides how many
    /// packets it takes. That is the division `table::fragment` exists for.
    fn cap(&self) -> usize {
        fragment::MAX_MESSAGE
    }
}

/// How long the driver sleeps when toxcore has nothing to say.
///
/// Bounded below toxcore's own interval as well, so a busy instance is iterated
/// as often as it asks and an idle one does not spin.
const MAX_TICK: Duration = Duration::from_millis(50);

/// How often stalled reassemblies are swept.
const SWEEP_EVERY: Duration = Duration::from_secs(5);

/// How often the founder offers the group again to seats that are not in it.
///
/// **Because `invited` is a record of what this client did, not of who is
/// there.** A peer whose client restarts has left the group and needs a fresh
/// invitation; the founder's `invited` still names it, so `invite_pending`
/// skips it for ever. The down-edge that would clear the entry never comes
/// either, because toxcore's friend connection outlives an outage far longer
/// than the ones that matter — measured, a client back after **twenty seconds**
/// waited out the rest of a run at *waiting to be invited* and played nothing,
/// while the table finished thirty hands without it.
///
/// So while the group is **short**, the record is dropped and everybody
/// connected is offered it again. It costs nothing when the group is whole,
/// because then nothing is short and nothing is sent; and a peer that is
/// already in a group ignores a second invitation, since the joiner accepts
/// only when it holds none.
///
/// `S1-FB`: **ten seconds, not thirty.** This is the safety net and not the way
/// back -- the friend link's up-edge offers at once, and so does the group's own
/// report of a member gone (`offer_again`) -- and a sweep that fires only while
/// the group is short costs a handful of invitations to peers who are not in it.
const REINVITE_EVERY: Duration = Duration::from_secs(10);

/// `S1-FB`: how long the instance may be offline before the driver bootstraps
/// it again itself.
///
/// Long enough not to fight toxcore's own reconnection over a blip -- which it
/// handles, and which `S1-EH` measured the library reporting as *offline* for
/// tens of seconds at every start -- and short against an outage a player sits
/// through.
const REBOOTSTRAP_AFTER: Duration = Duration::from_secs(10);

/// `S1-FB`: how often, while still offline, the bootstrap nodes are offered
/// again. A packet per node, into a line that may carry nothing yet: cheap
/// while the outage lasts, and the first one to land after it ends is the
/// instance's way back.
const REBOOTSTRAP_EVERY: Duration = Duration::from_secs(15);

/// `S1-FE`: how long a founder whose copy of the table's group holds nobody
/// else keeps a member's invitation back into the group before it takes it.
///
/// **A founder that lost its line had no way back at a table of three or
/// more.** Only a founder invites, and a member's copy that still holds another
/// member does not deliver a founder's invitation (`patches/0035` delivers one
/// only into a copy held empty) -- while the founder refused every member's
/// invitation, `invitation_fits` naming no inviter for it at all. The owner's
/// far machine founded a three-seat table, lost its line for about 48 s, was
/// certified out and removed by the other two, and read *nobody at the table
/// has been reachable* for as long as it was left running, the other two
/// dealing on (2026-09-14, 15:21 UTC).
///
/// So a founder takes a member's invitation back -- but not at once. At two
/// seats the other copy is empty as well, and that member takes the FOUNDER's
/// invitation (`S1-EB`); a founder leaving its copy the same moment would leave
/// the group the member is joining. Past `JOIN_GRACE`, a member that could take
/// the founder's invitation has taken it or given its join up.
const FOUNDER_YIELDS_AFTER: Duration = Duration::from_secs(30);

/// `S1-KO`: how long after a lone copy of the group -- a member's whose founder
/// is out for good, holding nobody else -- offered itself to a member of a
/// higher line that offered it the group, that member's own offer has to come
/// for the lone copy to be left for it.
///
/// **Proof by order, not by time alone.** A lone copy of the higher line is
/// left at once for the lower line's offer (the `GroupInvite` arm), and so
/// stops offering itself the moment the lower one's reaches it. So the lower
/// line answers the first offer of a higher one with one offer of its own copy,
/// and nothing more to that member: an offer of the higher member's that comes
/// later than this was made by a copy that did not leave for it -- it holds
/// another seat (one staying to watch, an entry not timed out yet) -- and the
/// lone copy goes to it. Two copies each left for the other would be no copy at
/// all; one offer each way, in that order, never leaves both.
///
/// **Later by more than two friend timeouts.** An offer is a lossless packet a
/// live friend connection holds while its line stalls, and toxcore keeps a
/// connection alive 32 s without a packet: the answer can reach the higher
/// member up to that late, and that member's own offer, sent just before the
/// answer reached it and the copy was left for it, up to that late again. An
/// offer that comes later than both was made after the answer arrived -- or
/// after a connection that lost it was reset -- by a copy that did not leave.
const LONE_YIELDS_AFTER: Duration = Duration::from_secs(70);

/// `S1-KO`: how long a higher member's offers to a lone copy may pause before
/// the lone copy forgets them -- the member gone, its copy left, or this
/// client's own line down -- and offers itself again as if none had come.
/// Past a friend timeout, and past the longest a copy's member takes to come
/// round to one missing seat again: one seat a `SWEEP_EVERY`, ten seats at most.
const OFFER_FORGOTTEN_AFTER: Duration = Duration::from_secs(60);

/// `S1-KO`: how long a member whose founder is out for good, holding a lone copy
/// at a game of three seats or more, has to be on the line with no other
/// member's offer before it offers its own copy. A copy the other seats play in
/// offers itself to each seat missing from it in turn, one a `SWEEP_EVERY`, at
/// once when the seat's friendship comes up -- round ten seats in less than
/// this; a lone copy that offered itself in that time could be taken by a seat
/// back from a restart and hold it apart from them. Past this, no such copy has
/// reached this one -- the other seats are all alone, restarted or away -- and
/// some copy must offer itself, or nobody does.
const LONE_OFFERS_AFTER: Duration = Duration::from_secs(60);

/// `S1-KO`: how long such a lone copy waits for every seat its hand counts as
/// playing to be within reach before it offers itself all the same. A seat out
/// of reach may hold the copy the others play in -- two seats behind one router
/// back on the line first would otherwise meet in a copy of their own -- or may
/// be a client that died in the hand; past this the line had time to come back,
/// and a table every copy of which waits on a dead seat would never deal again.
const LONE_OFFERS_ANYWAY_AFTER: Duration = Duration::from_secs(180);

/// `S1-KP` (`D-103`): how long every confirmed entry of a seat may be quiet before
/// a member whose founder is away offers that seat the group as missing -- the
/// sweep's own `S1-DT` reading, well inside the 58 s the library takes to time a
/// dead entry out, and past a live member's longest gap (its status is said every
/// `AWAY_RESAY`, 20 s). A live seat read missing is offered the group its copy
/// already holds, and the library swallows the offer.
const QUIET_MISSING_AFTER: Duration = Duration::from_secs(30);

/// `D-049`: how often a member says its sitting-out status in the group again,
/// changed or not. The library broadcasts a status losslessly and exchanges it
/// with every peer a member connects to; this bounds what any gap in that
/// could leave behind.
const AWAY_RESAY: Duration = Duration::from_secs(20);

// **There is deliberately no wait here for the founder's transport, and the
// reason is worth keeping because the wait was written and then removed.**
//
// The argument for one was that `init_gc_tcp_connection` seeds the chat's own
// `TCP_Connections` by copying whatever the main instance has *connected* at
// the moment the chat is created (`group_chats.c:7271`), so a group created
// before any relay has finished its handshake is seeded with nothing. That is
// true, and it is not the whole rule.
//
// `do_gc_tcp` runs every `TCP_RELAYS_CHECK_INTERVAL` = 10 s and re-seeds the
// chat whenever main's connected count **differs** from the count the chat
// last recorded (`group_chats.c:7060-7066`). `chat->connected_tcp_relays`
// starts at zero and `init_gc_tcp_connection` does not set it, so a group
// created with nothing is re-seeded within about ten seconds of main's relays
// coming up. The seeding is self-correcting, not one-shot; it only latches
// once the two counts agree, which is the state you want it to latch in.
//
// So a wait bought roughly ten seconds of earliness, and it cost the node
// loop: `tox_sink.start` creates the Tox instance at the moment a table is
// founded rather than at startup, so the wait ran *before* the group existed
// and `net::run`'s `chat_id_ready` await blocked the whole loop for its
// duration -- exactly while the founder should have been forming its lobby
// mesh. Bad trade, and the first draft of it was worse still: twenty seconds
// against a caller that waited five, so no group was created at all.
//
// If this is ever wanted again, the shape that works is **pre-warming** -- 
// create the Tox instance when the client starts and only the group when the
// table is founded -- not blocking at the point of use. `D-042` built exactly
// that: the instance starts with the client (`TableSink::boot`) and a table
// is a group on it.

/// Start the driver on its own thread.
///
/// `tox` is moved onto that thread and stays there. The returned handle is the
/// only way to reach it, and dropping the handle stops it.

/// How long a friendship no open table needs is kept before it is dropped.
///
/// **The friends of a finished game go with it -- the owner's rule -- and the
/// next table the same players sit at is the case `S1-DN` is about.** A
/// friendship dropped and re-added is looked up again through the onion from
/// nothing, which is a good part of the minute a second table used to wait
/// for; one kept for two minutes past the game's end is still up when the
/// founder opens the next table and the others join it. Past that a client
/// at the lobby holds no game's connections.
pub const FRIEND_LINGER: Duration = Duration::from_secs(120);

/// One table's state at the driver: its group, its roster, the bridge from
/// group keys to application keys, what it has invited, who is confirmed,
/// what it still has to say, and the channels and counters the node holds
/// the other end of.
struct TableState {
    setup: Setup,
    /// The seats' Tox keys, this client's excepted (see `Setup::roster`).
    roster: Vec<[u8; 32]>,
    /// Group key -> application key, learned from verified traffic (`S1-I`).
    known_as: HashMap<[u8; 32], [u8; 32]>,
    /// `S1-DS`: peer number -> group key, remembered as each peer is seen,
    /// so an exit is mapped to its seat even when the key cannot be read at
    /// that moment (run095833-2: a joiner's leave reached the founder as
    /// *deleting group peer 1, exit type 0* and never as a seat gone).
    peer_keys: HashMap<u32, [u8; 32]>,
    /// `S1-DV`: peer number -> the Tox key of the friend its invitation
    /// came over (`patches/0033`): for the founder every seat it invited,
    /// for a joiner its founder. Known from the join, so an exit or a
    /// silence maps to a seat before the group has taught anything --
    /// before the first hand.
    peer_lines: HashMap<u32, [u8; 32]>,
    /// `S1-DV`: whether this table's group ever held another member. A
    /// host's `self_joined` never turns true (the library says self-join
    /// only for a join, not for a group it created), so this is what says a
    /// table got into its group at all -- and whether its friendships linger.
    had_members: bool,
    /// `S1-EK`: whether THIS copy of the group has confirmed a member. A copy
    /// still settling -- self-joined, its founder a round trip from confirmed
    /// -- is not an emptied one, and an invitation that lands then is not
    /// taken by leaving it.
    settled: bool,
    group: Option<u32>,
    /// `D-049`: whether this seat sits out, and when the group was last told.
    away: bool,
    away_said_at: Option<Instant>,
    /// `D-051`: the group this client's name carries its binding in.
    named: Option<u32>,
    /// `D-051`: the application keys of the seats, and whether that list is
    /// the ratified roster's.
    seat_apps: Vec<[u8; 32]>,
    seats_fixed: bool,
    /// `S1-KJ`: the seats placed here at least once, and every member key
    /// placed -- so that a seat seen again under a key it never had here is
    /// told as a new entry of that seat (`Trouble::new_entries`).
    placed_seats: std::collections::HashSet<[u8; 32]>,
    placed_keys: std::collections::HashSet<[u8; 32]>,
    /// `D-051`: member key -> the application key its name binds it to,
    /// verified against the group's own key for it.
    bound: HashMap<[u8; 32], [u8; 32]>,
    /// `D-051`: members not placed at a seat yet, and since when -- no
    /// binding yet, or bound to a key the forming roster does not name yet.
    unplaced: HashMap<[u8; 32], Instant>,
    /// `D-051`: seats barred from this group here, by application key: cut
    /// off for flooding, or put out for good by the table.
    barred: std::collections::HashSet<[u8; 32]>,
    /// `D-051`: the same seats by the Tox key of the line their members came
    /// in over, where this client invited them: offered the group no more.
    /// `S1-KL`: and every seat the table's word removed for good (`D-047`), by
    /// its own line -- never taken back into the roster, never offered the
    /// group, no invitation of its taken, for the table's life.
    barred_lines: std::collections::HashSet<[u8; 32]>,
    /// `D-051`: each member's traffic, by member key.
    meters: HashMap<[u8; 32], crate::table::membership::Meter>,
    /// `D-051`: members cut off here, by member key.
    cut: std::collections::HashSet<[u8; 32]>,
    /// fault-harness: the most one member has sent within each window, for
    /// measuring what an honest table sends.
    #[cfg(feature = "fault-harness")]
    peak: (u64, u64, u64, u64),
    #[cfg(feature = "fault-harness")]
    peak_said: (u64, u64, u64, u64),
    invited: Vec<u32>,
    confirmed: std::collections::HashSet<u32>,
    accepted_at: Option<Instant>,
    self_joined: bool,
    rejoins: u32,
    stalled_once: bool,
    was_reachable: bool,
    last_reinvite: Instant,
    pending: Vec<Vec<u8>>,
    next_id: u32,
    reassembler: Reassembler<u32>,
    /// Turns left to say what `pending` holds before the table is closed;
    /// `None` for a table that is open.
    closing: Option<usize>,
    /// When the founder's last invitation went, and how many members were
    /// confirmed then. See `invite_pending`.
    last_invite: Option<(Instant, usize)>,
    /// `S1-FB`: the lines -- friend keys -- whose member the table's group has
    /// just reported gone, for the founder to offer the group to again at once.
    ///
    /// **An edge of the group's own, beside the friend link's.** The friend
    /// link's down-edge is what clears a friend from `invited`, and a line cut
    /// for longer than the friend timeout (about 32 s, sooner than the group's
    /// 58 s) produces it: `run154144-2` had the invitation 0.3 s after the line
    /// came back. A member the group times out while its friend link still
    /// reads up produces none -- a restarted client whose old connection has
    /// not timed out yet is one -- and was left to `REINVITE_EVERY`. The group
    /// reporting the member gone is the edge this carries.
    offer_again: Vec<[u8; 32]>,
    /// `S1-FE`: a founder's copy of the group holding nobody else -- a member's
    /// invitation back into the table's group, the friend it came over, and
    /// when the first such invitation arrived. See `FOUNDER_YIELDS_AFTER`.
    held_offer: Option<(u32, Vec<u8>, Instant)>,
    /// `S1-FE`: the group a founder left to take a member's invitation back
    /// into it -- the one invitation it then accepts.
    own_chat: Option<[u8; 32]>,
    /// `S1-FE`: when a member last offered the group to its absent founder.
    founder_offered: Option<Instant>,
    /// `S1-KO`: each seat's line by its application key (`Command::Seats`).
    seat_lines: Vec<([u8; 32], [u8; 32])>,
    /// `S1-KO`: the lines the table's word put out for good (`Command::Remove`
    /// with `for_good`) -- and not the lines this client cut off on its own
    /// meter, which `barred_lines` holds as well: a member's founder is out by
    /// the table's word and by nothing else.
    out_lines: std::collections::HashSet<[u8; 32]>,
    /// `S1-KO`: the seats still in the game, by application key, this client's
    /// own among them -- with chips, not out for good, as the node's own hand
    /// counts them (`Command::InGame`). Once a member's founder is out for good
    /// it offers the group to, and takes an invitation from, these seats alone.
    in_game: Vec<[u8; 32]>,
    /// `S1-KO`: when this member last offered the group to each line, once its
    /// founder is out for good.
    seat_offered: HashMap<[u8; 32], Instant>,
    /// `S1-KO`: whether `in_game` came from this client's own hand -- and not
    /// from its session record, before it holds a hand again: a seat back from a
    /// restart offers the group to nobody until then.
    in_game_from_hand: bool,
    /// `S1-KO`: of `in_game`, the seats this client's hand counts as playing --
    /// those a lone copy at a larger game must be able to reach before it offers
    /// itself (`member_offers`).
    in_game_present: Vec<[u8; 32]>,
    /// `S1-KO`: since when this copy of the group has held nobody else, as the
    /// sweep last read it (`held_empty`).
    empty_since: Option<Instant>,
    /// `S1-KO`: since when this client has been on the line again -- its own
    /// connection up and a friend this table needs up -- as the sweep reads it.
    reachable_since: Option<Instant>,
    /// `S1-KO`: the members of a higher line that have offered this lone copy
    /// the group since it was last not alone -- each by its line, with when this
    /// copy offered itself to it in answer (once) and when its last offer came;
    /// one whose offers paused for `OFFER_FORGOTTEN_AFTER` is forgotten.
    waiting_on: HashMap<[u8; 32], (Instant, Instant)>,
    /// `S1-KP` (`D-103`): the node's reading that the founder is away from the
    /// table's hands (`Command::FounderAway`) -- the members take its part in
    /// the group as for a founder out for good, without barring it.
    founder_away_by_hands: bool,
    /// `S1-KP` (`D-103`): and outside them since the first such hand -- a copy
    /// held empty takes a counted seat's invitation.
    founder_outside: bool,
    out: tokio::sync::mpsc::Receiver<Vec<u8>>,
    inbox: tokio::sync::mpsc::Sender<FromTable>,
    chat: tokio::sync::watch::Sender<Option<[u8; 32]>>,
    trouble: Arc<Trouble>,
}

impl TableState {
    /// The Tox keys this table needs a friendship with: its roster, and for a
    /// joiner its founder.
    fn needs(&self, key: &[u8; 32]) -> bool {
        self.roster.contains(key)
            || matches!(&self.setup.role, Role::Joiner { founder, .. } if founder == key && !self.barred_lines.contains(key))
    }
}

fn friend_number(friends: &HashMap<u32, [u8; 32]>, key: &[u8; 32]) -> Option<u32> {
    friends.iter().find(|(_, k)| *k == key).map(|(n, _)| *n)
}

/// A friendship for this key, if there is none yet, and no longer idle if
/// there is. Friendships are the instance's and shared by every table; a
/// second table with the same player finds the connection already up, which
/// is the whole point of `D-042`.
fn befriend(
    tox: &mut Tox,
    friends: &mut HashMap<u32, [u8; 32]>,
    idle: &mut HashMap<u32, Instant>,
    key: &[u8; 32],
) {
    match friend_number(friends, key) {
        Some(n) => {
            idle.remove(&n);
        }
        None => {
            if let Ok(n) = tox.add_friend(key) {
                friends.insert(n, *key);
            }
        }
    }
}

/// `D-019`, by the line an invitation went over (`patches/0033`): where this
/// client is the admin, every member that came in through the friend with
/// this Tox key is removed from the group; elsewhere it is impossible rather
/// than refused. (Before `S1-DV` the kick looked the Tox key up as an
/// application key, and found nothing.)
fn unseat(tox: &mut Tox, t: &TableState, key: &[u8; 32]) {
    let Some(g) = t.group else {
        return;
    };
    // `D-045`: the roster's word. Every member sees the seat leave the roster
    // (the founder's signed list), so every member drops it for good; the
    // founder's kick counts because every member allowed it on that word.
    let host = matches!(t.setup.role, Role::Host);
    for gk in keys_of_seat(t, None, Some(key)) {
        let _ = tox.allow_kick(g, &gk);
        if host {
            if let Some(p) = t.peer_keys.iter().find(|(_, k)| **k == gk).map(|(p, _)| *p) {
                let _ = tox.kick(g, p);
            }
        }
        if tox.peer_drop(g, &gk, true) {
            t.trouble.removed.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// `D-045`: every group key a seat speaks from -- taught for its application
/// key, or held by a member that came in over its line.
fn keys_of_seat(t: &TableState, app_key: Option<&[u8; 32]>, tox_key: Option<&[u8; 32]>) -> Vec<[u8; 32]> {
    let mut keys: Vec<[u8; 32]> = Vec::new();
    if let Some(app) = app_key {
        keys.extend(t.known_as.iter().filter(|(_, a)| *a == app).map(|(gk, _)| *gk));
    }
    if let Some(line) = tox_key {
        for (peer, l) in t.peer_lines.iter() {
            if l == line {
                if let Some(gk) = t.peer_keys.get(peer) {
                    keys.push(*gk);
                }
            }
        }
    }
    keys.sort_unstable();
    keys.dedup();
    keys
}

/// `D-051`: this client's name in the group is its member binding -- its
/// application key and its signature over the group and its own member key --
/// set the moment the group exists, before any member can read a name.
fn name_self(tox: &mut Tox, key: &ed25519_dalek::SigningKey, g: u32) -> bool {
    let (Ok(chat), Some(me)) = (tox.chat_id(g), tox.self_key(g)) else {
        return false;
    };
    let name = crate::table::membership::binding_for(key, &chat, &me);
    tox.set_self_name(g, &name)
}

/// `D-051`: which seat a member is, from the binding in its name, checked
/// against the group's own key for it and the table's seats.
///
/// Placed at a seat, the binding is the pairing (`known_as`), and a claim
/// read off carried traffic no longer decides it. Cut off at once: a name
/// that is a binding made for another member or group; a binding of a seat
/// barred here; a binding of this client's own seat, which only this client
/// holds; a binding of a key the ratified roster does not seat. Left to wait:
/// no binding yet, for `NAME_GRACE` at the sweep; a key the forming roster
/// does not name yet, until the roster is ratified.
fn place_member(tox: &mut Tox, t: &mut TableState, g: u32, peer: u32, key: [u8; 32]) {
    let Some(mine) = t.setup.binder.as_ref().map(|k| *k.verifying_key().as_bytes()) else {
        return;
    };
    if t.cut.contains(&key) {
        return;
    }
    let (Some(name), Ok(chat)) = (tox.peer_name(g, peer), tox.chat_id(g)) else {
        return;
    };
    match crate::table::membership::bound_to(&name, &chat, &key) {
        Some(app) => {
            if t.bound.insert(key, app).is_some_and(|was| was != app) {
                cut_off(tox, t, g, key, Cut::NotItsBinding);
                return;
            }
            if t.barred.contains(&app) {
                cut_off(tox, t, g, key, Cut::Barred);
                return;
            }
            // `S1-EE`'s rule, by binding: another member is never this client's
            // own seat -- an old entry of this client's own dead process, or a
            // second client on the same identity.
            if app == mine {
                cut_off(tox, t, g, key, Cut::NotASeat);
                return;
            }
            // No seats said yet: nothing to judge a binding against, so it
            // waits -- a table whose node has not said its roster cuts nobody
            // off for not being on it.
            if t.seat_apps.is_empty() {
                t.unplaced.entry(key).or_insert_with(Instant::now);
                return;
            }
            if t.seat_apps.contains(&app) {
                t.unplaced.remove(&key);
                t.known_as.insert(key, app);
                // `S1-KJ`: a seat here before, under a key it never had here --
                // a new process of that seat (`S1-DU`), or an entry its player
                // brought in.
                if a_new_entry_of_a_seat_seen(&mut t.placed_seats, &mut t.placed_keys, app, key) {
                    if let Ok(mut n) = t.trouble.new_entries.lock() {
                        n.push(app);
                    }
                }
                if let Ok(mut known) = t.trouble.known.lock() {
                    known.insert(app);
                }
                if t.confirmed.contains(&peer) {
                    if let Ok(mut present) = t.trouble.present.lock() {
                        present.insert(app);
                    }
                }
                return;
            }
            if t.seats_fixed {
                cut_off(tox, t, g, key, Cut::NotASeat);
            } else {
                t.unplaced.entry(key).or_insert_with(Instant::now);
            }
        }
        None if crate::table::membership::looks_like_a_binding(&name) => {
            cut_off(tox, t, g, key, Cut::NotItsBinding);
        }
        None => {
            // A member bound before and named otherwise now is not ours.
            if t.bound.contains_key(&key) {
                cut_off(tox, t, g, key, Cut::NotItsBinding);
            } else {
                t.unplaced.entry(key).or_insert_with(Instant::now);
            }
        }
    }
}

/// `D-051`: cut a member off from this client's view of the group for good:
/// allowed to be kicked, kicked where this client is the founder, dropped,
/// its key refused for the group's life. A flooder's seat is barred with it,
/// so every other entry the seat holds goes now and any it takes later goes
/// the moment its binding is read. Said to the node.
fn cut_off(tox: &mut Tox, t: &mut TableState, g: u32, key: [u8; 32], why: Cut) {
    if !t.cut.insert(key) {
        return;
    }
    let app = t.bound.get(&key).copied();
    // A flooder's line, where it came in over one of this client's invitations:
    // the founder offers the group to that seat no more.
    if matches!(why, Cut::Flood(_) | Cut::Barred) {
        let line = t
            .peer_keys
            .iter()
            .find(|(_, k)| **k == key)
            .and_then(|(p, _)| t.peer_lines.get(p))
            .copied();
        if let Some(line) = line {
            t.barred_lines.insert(line);
        }
    }
    let _ = tox.allow_kick(g, &key);
    if matches!(t.setup.role, Role::Host) {
        if let Some(p) = t.peer_keys.iter().find(|(_, k)| **k == key).map(|(p, _)| *p) {
            let _ = tox.kick(g, p);
            // `S1-ED`: the library's kick deletes without the exit callback.
            member_gone(tox, t, g, p, Some(key), false);
        }
    }
    if tox.peer_drop(g, &key, true) {
        t.trouble.removed.fetch_add(1, Ordering::Relaxed);
    }
    t.unplaced.remove(&key);
    t.meters.remove(&key);
    let flood = matches!(why, Cut::Flood(_));
    if let Ok(mut said) = t.trouble.cut_off.lock() {
        said.push(CutOff {
            app_key: app,
            member_key: key,
            why,
        });
    }
    if let (true, Some(app)) = (flood, app) {
        bar_seat(tox, t, g, app);
    }
}

/// `D-051`: bar a seat from this group here, and cut off every entry it holds.
fn bar_seat(tox: &mut Tox, t: &mut TableState, g: u32, app: [u8; 32]) {
    if !t.barred.insert(app) {
        return;
    }
    let entries: Vec<[u8; 32]> = t
        .bound
        .iter()
        .filter(|(k, a)| **a == app && !t.cut.contains(*k))
        .map(|(k, _)| *k)
        .collect();
    for key in entries {
        cut_off(tox, t, g, key, Cut::Barred);
    }
}

/// `D-051`: count noise against a member and cut it off if that makes it a
/// flooder.
fn score_noise(tox: &mut Tox, t: &mut TableState, g: u32, key: [u8; 32], points: u32) {
    if t.setup.binder.is_none() || t.cut.contains(&key) {
        return;
    }
    let now = millis();
    let mk = metered_as(t, &key);
    let meter = t.meters.entry(mk).or_default();
    meter.noise(now, points);
    if let Some(flood) = meter.over(now) {
        cut_off(tox, t, g, key, Cut::Flood(flood));
    }
}

/// `S1-KJ`: whether `key`, placed at `app`'s seat now, is a new entry of a
/// seat that had another here before. Records both either way; a key placed
/// again (every sweep reads an unplaced name again) is nothing.
fn a_new_entry_of_a_seat_seen(
    placed_seats: &mut std::collections::HashSet<[u8; 32]>,
    placed_keys: &mut std::collections::HashSet<[u8; 32]>,
    app: [u8; 32],
    key: [u8; 32],
) -> bool {
    if !placed_keys.insert(key) {
        return false;
    }
    !placed_seats.insert(app)
}

/// `S1-KI`: whose meter a member's traffic is counted on: its seat's -- the key
/// its binding names -- once the binding is read, its own key before that.
///
/// Every entry a seat binds is a member with a key of its own, and metered by
/// member key a rogue's allowance was `D-051`'s limits times the entries it
/// kept in the group. Metered by seat, a seat's entries share one allowance:
/// an honest seat holds one entry, or two for the moments of `S1-DB` while its
/// dead process's entry goes -- and a dead process sends nothing.
fn metered_as(t: &TableState, key: &[u8; 32]) -> [u8; 32] {
    if count_by_member() {
        return *key;
    }
    meter_key(&t.bound, key)
}

/// `S1-KI`: `metered_as` on the bindings alone.
fn meter_key(bound: &HashMap<[u8; 32], [u8; 32]>, key: &[u8; 32]) -> [u8; 32] {
    bound.get(key).copied().unwrap_or(*key)
}

fn by_group(tables: &mut HashMap<TableId, TableState>, g: u32) -> Option<&mut TableState> {
    tables.values_mut().find(|t| t.group == Some(g))
}

/// Close one table: leave its group, and mark the friends no other open table
/// needs as idle, for the sweep to drop after `FRIEND_LINGER`. What the table
/// still held was said before this (see `closing`).
fn close_table(
    tox: &mut Tox,
    tables: &mut HashMap<TableId, TableState>,
    id: TableId,
    friends: &HashMap<u32, [u8; 32]>,
    idle: &mut HashMap<u32, Instant>,
) {
    let Some(t) = tables.remove(&id) else {
        return;
    };
    if let Some(g) = t.group {
        let _ = tox.leave(g);
    }
    let mut keys: Vec<[u8; 32]> = t.roster.clone();
    if let Role::Joiner { founder, .. } = &t.setup.role {
        keys.push(*founder);
    }
    // `S1-KL`: and a seat removed for good, which the roster no longer names --
    // its friendship idles out with the others no table needs (`D-047`).
    keys.extend(t.barred_lines.iter().copied());
    keys.sort_unstable();
    keys.dedup();
    for key in keys {
        if tables.values().any(|o| o.needs(&key)) {
            continue;
        }
        if let Some(n) = friend_number(friends, &key) {
            // `S1-DV`: a table that never got into its group has nothing to
            // linger for; its friendships go at the next sweep, so the others
            // see this client's line drop within the library's friend timeout
            // rather than two minutes later.
            let since = if t.self_joined || t.had_members {
                Instant::now()
            } else {
                Instant::now().checked_sub(FRIEND_LINGER).unwrap_or_else(Instant::now)
            };
            idle.entry(n).or_insert(since);
        }
    }
}

/// The sweep, per table: invitations owed, the counters the node reads, the
/// membership and friend-link sets the felt is drawn from, and a joiner's
/// second try at a join that never finished.
fn sweep_table(
    tox: &mut Tox,
    t: &mut TableState,
    friends: &HashMap<u32, [u8; 32]>,
    connected: &std::collections::HashSet<u32>,
    self_connection: u64,
) {
    t.reassembler.sweep(millis());
    // `D-051`: every member not at a seat is read again, and one still not at
    // a seat past its grace is no seat of this table.
    if let (Some(g), true) = (t.group, t.setup.binder.is_some()) {
        let members: Vec<(u32, [u8; 32])> = t
            .peer_keys
            .iter()
            .filter(|(p, k)| {
                t.confirmed.contains(p)
                    && !t.cut.contains(*k)
                    && !t.bound.get(*k).is_some_and(|a| t.seat_apps.contains(a))
            })
            .map(|(p, k)| (*p, *k))
            .collect();
        for (p, k) in members {
            place_member(tox, t, g, p, k);
            if t.cut.contains(&k) {
                continue;
            }
            let Some(since) = t.unplaced.get(&k).copied() else {
                continue;
            };
            // A member bound to a key the forming roster does not name yet waits
            // for the roster: `place_member` cuts it the moment the roster is
            // ratified without it, and never before -- a joiner's roster can lag
            // the founder's invitation by as long as the mesh takes (`S1-DP`).
            if t.bound.contains_key(&k) {
                continue;
            }
            if since.elapsed() >= NAME_GRACE {
                cut_off(tox, t, g, k, Cut::Nameless);
            }
        }
    }
    // `D-051`: named with this seat's binding, again if the first try found
    // no key or no chat id to sign.
    if let (Some(g), Some(key)) = (t.group, t.setup.binder.as_ref()) {
        if t.named != Some(g) && name_self(tox, key, g) {
            t.named = Some(g);
        }
    }
    #[cfg(feature = "fault-harness")]
    if t.peak != t.peak_said {
        t.peak_said = t.peak;
        println!(
            "fault-harness: the busiest member of this table's group sent at most {} packets and {} bytes in {} s, {} packets and {} bytes in {} s (D-051)",
            t.peak.0,
            t.peak.1,
            crate::table::membership::FLOOD_SHORT_S,
            t.peak.2,
            t.peak.3,
            crate::table::membership::FLOOD_LONG_S
        );
    }
    // **Offer the group again to whoever is not in it.** See `REINVITE_EVERY`:
    // `invited` says what this client has done, and a peer that restarted
    // needs asking again even though it does. Only while the group is short;
    // the invitations themselves go one at a time from the loop.
    if matches!(t.setup.role, Role::Host) && t.last_reinvite.elapsed() >= REINVITE_EVERY {
        // `S1-EG`: CONFIRMED members against the roster, this client among
        // them -- the library's count holds unconfirmed entries too, and a
        // dropped seat's old key comes back as one every forty seconds by the
        // library's own reconnection, reaped after thirty (run180223-3: the
        // group looked whole from 105 s to 175 s and the seat that needed the
        // offer got none until its next outage).
        // `S1-FE`: short of an OTHER seat -- the roster leaves this client out.
        // `S1-KI`: by seat, however many entries one seat holds.
        let short = t.group.is_some() && group_short(seats_here(t), t.roster.len());
        if short {
            // `S1-EG`: only the seats NOT in the group are asked again. Clearing
            // the record wholesale invited the confirmed members too, each such
            // invitation swallowed at its end and each costing the seat that
            // needed one an `INVITE_GAP` (run180223-3: back on its line at 90 s,
            // the seat took the founder's invitation at 210 s).
            let in_group: std::collections::HashSet<[u8; 32]> = t
                .peer_lines
                .iter()
                .filter(|(p, _)| t.confirmed.contains(p))
                .map(|(_, l)| *l)
                .collect();
            t.invited.retain(|f| friends.get(f).is_some_and(|k| in_group.contains(k)));
            t.last_invite = None;
        }
        t.last_reinvite = Instant::now();
    }
    // Whether the group now holds every other seat. **Counted, not matched**:
    // `tox_group_peer_get_public_key` gives a peer's group key, not the friend
    // key the roster holds, so no scan can say which seat a member is.
    // `S1-KI`: **the members for an arrival, the seats for a whole group.** The
    // count of members is what says somebody arrived -- a seat back under a
    // fresh key raises it while its old entry is still in (`S1-DX` answers
    // that with the node's list and ratification, `run162035-3`). Whether the
    // group holds every seat is `seats_here`: by member, the count was sound
    // only while nobody but the founder brought members in, and any member of
    // a private group can invite -- a rogue's own extra entries, each bound to
    // its seat, read as the seats still missing.
    // `S1-DZ`: CONFIRMED members -- never the library's peer count, which
    // holds unconfirmed entries: an invitation half-way through its handshake,
    // a dropped seat's old key re-added by the library's own reconnection. The
    // founder dealt hand #1 at 17 s to a seat still handshaking (run192753-3),
    // which never caught up.
    //
    // `S1-FE`: **the other seats, as `roster` counts them.** `S1-DZ` counted
    // this client in as well (`1 +`) against a roster that leaves it out, so
    // every reading was one seat generous: a heads-up group read complete with
    // nobody else in it, a larger one with a seat still missing, and the sweeps
    // that offer the group again only while it is short never fired for one
    // missing seat -- the founder's `S1-EG` sweep and `S1-FB`'s `offer_again`
    // alike. The node reads this number as other seats too (*not one of N
    // other seats*, *held N of M other seats*).
    let (seen, seats) = match t.group {
        Some(_) => (t.confirmed.len(), seats_here(t)),
        None => (0, 0),
    };
    // The friend connections that are up among the ones THIS table needs.
    let mine_up = connected
        .iter()
        .filter(|n| friends.get(n).is_some_and(|k| t.needs(k)))
        .count();
    t.trouble.self_connection.store(self_connection, Ordering::Relaxed);
    t.trouble.friends_up.store(mine_up as u64, Ordering::Relaxed);
    t.trouble.in_group.store(seen as u64, Ordering::Relaxed);
    t.trouble.seats_in_group.store(seats as u64, Ordering::Relaxed);
    // `D-035`, `D-041`: which seats are confirmed members right now, by
    // APPLICATION key through the bridge `known_as` holds.
    // `D-049`: this seat's own word on sitting out is the group's status of
    // it: AWAY while it sits out, NONE while it plays. Set whenever the
    // library's copy differs -- a fresh copy of the group starts at NONE --
    // and said again every `AWAY_RESAY`, so a member whose connection to this
    // one was down at the broadcast has it within that, whatever the library's
    // own peer exchange did.
    if let Some(g) = t.group {
        let due = t.away_said_at.is_none_or(|at| at.elapsed() >= AWAY_RESAY);
        if (due || tox.self_away(g) != Some(t.away)) && tox.set_self_away(g, t.away) {
            t.away_said_at = Some(Instant::now());
        }
    }
    if let Ok(mut present) = t.trouble.present.lock() {
        present.clear();
        let mut quiet: HashMap<[u8; 32], u64> = HashMap::new();
        // `D-049`: each seat's status as its most recently heard entry has it.
        let mut away: HashMap<[u8; 32], (u64, bool)> = HashMap::new();
        if let Some(g) = t.group {
            for (group_key, app_key) in t.known_as.iter() {
                let member = peer_of(tox, t, g, group_key);
                if let Some(p) = member.filter(|p| t.confirmed.contains(p)) {
                    present.insert(*app_key);
                    // `S1-DT`: and how long the group has heard nothing from it.
                    let entry_quiet = tox.peer_quiet_secs(g, group_key);
                    if let Some(q) = entry_quiet {
                        // `S1-DU`: a seat is as quiet as its most recent entry.
                        let q = quiet.get(app_key).map_or(q, |cur| (*cur).min(q));
                        quiet.insert(*app_key, q);
                    }
                    let q = entry_quiet.unwrap_or(u64::MAX);
                    if away.get(app_key).is_none_or(|(cur, _)| q < *cur) {
                        away.insert(*app_key, (q, tox.peer_away(g, p).unwrap_or(false)));
                    }
                }
            }
        }
        if let Ok(mut q) = t.trouble.quiet.lock() {
            *q = quiet;
        }
        if let Ok(mut a) = t.trouble.away.lock() {
            *a = away.into_iter().filter(|(_, (_, on))| *on).map(|(k, _)| k).collect();
        }
    }
    // `S1-DV`: the same by the Tox key of the friend each member came in
    // through -- taught by nobody, known from the join.
    if let Ok(mut present) = t.trouble.present_lines.lock() {
        present.clear();
        let mut quiet: HashMap<[u8; 32], u64> = HashMap::new();
        let mut away: HashMap<[u8; 32], (u64, bool)> = HashMap::new();
        if let Some(g) = t.group {
            for (peer, line) in t.peer_lines.iter() {
                if !t.confirmed.contains(peer) {
                    continue;
                }
                present.insert(*line);
                let entry_quiet = t.peer_keys.get(peer).and_then(|k| tox.peer_quiet_secs(g, k));
                if let Some(q) = entry_quiet {
                    let q = quiet.get(line).map_or(q, |cur| (*cur).min(q));
                    quiet.insert(*line, q);
                }
                let q = entry_quiet.unwrap_or(u64::MAX);
                if away.get(line).is_none_or(|(cur, _)| q < *cur) {
                    away.insert(*line, (q, tox.peer_away(g, *peer).unwrap_or(false)));
                }
            }
        }
        if let Ok(mut q) = t.trouble.quiet_lines.lock() {
            *q = quiet;
        }
        if let Ok(mut a) = t.trouble.away_lines.lock() {
            *a = away.into_iter().filter(|(_, (_, on))| *on).map(|(k, _)| k).collect();
        }
    }
    // `D-041`: and the friend connections that are up, by Tox key.
    if let Ok(mut up) = t.trouble.friends_on.lock() {
        up.clear();
        for n in connected.iter() {
            if let Some(k) = friends.get(n) {
                up.insert(*k);
            }
        }
    }
    // `D-037`: while the founder's friendship is up and it is not a confirmed
    // member, a member offers the group again.
    //
    // `S1-FE`: **every `REINVITE_EVERY`, not once a friendship.** An offer made
    // while the founder's own copy still held the others is swallowed by the
    // library (`patches/0035` delivers only into a copy held empty), and the
    // next one waited for the friendship to go down and come up again -- which
    // a founder back on its line does not do. And absent by the line its entry
    // came in over, while the group is short: this read `entries_of` with the
    // founder's TOX key against APPLICATION keys, found nothing ever, and so
    // offered while the founder sat in the group. A founder out of the roster
    // for good (`D-047`) is offered nothing.
    // `S1-KP` (`D-103`, `REFUTE_S1KP_v2_groups` V5): not from a copy held empty
    // while the founder is away by the node's reading -- a founder apart from the
    // table whose own copy emptied keeps the newest member offer and takes it
    // `FOUNDER_YIELDS_AFTER` on, and a lone copy's offer would take it out of the
    // table's reach with that member; the copies holding seats offer it.
    let lone_and_away = t.founder_away_by_hands && held_empty(t);
    if let (Role::Joiner { founder, .. }, Some(g), false) = (&t.setup.role, t.group, lone_and_away) {
        if t.self_joined && t.roster.contains(founder) && !t.barred_lines.contains(founder) {
            if let Some(n) = friend_number(friends, founder) {
                let here = t.peer_lines.iter().any(|(p, l)| l == founder && t.confirmed.contains(p));
                let absent = !here && group_short(seats_here(t), t.roster.len());
                if !absent {
                    t.founder_offered = None;
                } else if connected.contains(&n)
                    && t.founder_offered.is_none_or(|at| at.elapsed() >= REINVITE_EVERY)
                    && tox.invite(g, n).is_ok()
                {
                    t.founder_offered = Some(Instant::now());
                    if !t.invited.contains(&n) {
                        t.invited.push(n);
                    }
                    t.trouble.invites_sent.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
    // `S1-KO`: a member whose founder is out of the table for good takes the
    // founder's part in the group: it offers the group to a seat of the game
    // missing from it, over the line that seat's join declared, every
    // `REINVITE_EVERY` while the seat is missing -- a seat back from a restart has
    // no founder to ask (`D-037`), and a member whose copy emptied takes no
    // invitation but a member's. By seat, through the bindings: a member that
    // came in on another member's invitation is on no line this client knows.
    // One seat a sweep, as the founder's invitations go one at a time
    // (`invite_pending`); only a seat still in the game (`in_game_line`), and from
    // a lone copy only at a game of two (`offers_its_copy`).
    // How long this copy has held nobody else, and who of a higher line has
    // offered this lone copy the group since it last held somebody -- forgotten
    // once its offers pause.
    if held_empty(t) {
        t.empty_since.get_or_insert_with(Instant::now);
        t.waiting_on.retain(|_, (_, last)| last.elapsed() < OFFER_FORGOTTEN_AFTER);
    } else {
        t.empty_since = None;
        t.waiting_on.clear();
    }
    // `S1-KP` (`D-103`): and so for a founder away from the table's hands by the
    // node's reading, which bars nothing.
    if let (true, Some(g)) = (founder_away(t), t.group) {
        if let (true, Some(mine)) = (t.self_joined && t.settled, t.setup.binder.as_ref().map(|k| k.verifying_key().to_bytes())) {
            // `S1-KP` (`REFUTE_S1KP_v2_groups` V1, fix A): a seat whose entries here
            // are all quiet counts as missing -- one back from a restart is offered
            // from this copy before its dead entry times out.
            let present = seats_heard(tox, g, t);
            let offers = member_offers(t, &mine, friends, connected);
            // Not again to a member this lone copy has answered once: it waits on
            // that member's next offer (`LONE_YIELDS_AFTER`). Of the seats due, the
            // one offered longest ago -- round the missing seats in turn: the first
            // due in seat order took every sweep from a third seat for as long as
            // two before it stayed missing. `S1-KP`: never the founder away by the
            // node's reading -- `S1-FE` offers it the group already.
            let due = missing_lines(&t.seat_lines, &mine, &t.in_game, &present, &t.barred, &t.barred_lines, &t.roster)
                .into_iter()
                .filter(|line| offers && !t.waiting_on.contains_key(line) && !founder_line_away(t, line))
                .filter_map(|line| friend_number(friends, &line).map(|n| (line, n)))
                .filter(|(line, n)| {
                    connected.contains(n) && t.seat_offered.get(line).is_none_or(|at| at.elapsed() >= REINVITE_EVERY)
                })
                .min_by_key(|(line, _)| t.seat_offered.get(line).copied());
            if let Some((line, n)) = due {
                if tox.invite(g, n).is_ok() {
                    t.seat_offered.insert(line, Instant::now());
                    t.trouble.invites_sent.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
    t.trouble
        .confirmed_peers
        .store(t.confirmed.len() as u64, Ordering::Relaxed);
    t.trouble.founder_link.store(
        match &t.setup.role {
            Role::Host => 3,
            Role::Joiner { founder, .. } if !founder_away(t) => friend_number(friends, founder)
                .map(|n| tox.friend_connection(n).max(0) as u64)
                .unwrap_or(0),
            // `D-037`: no founder to reach; the best member link stands in.
            // `S1-KO`: so at a table whose founder is out for good.
            Role::Joiner { .. } | Role::Back { .. } => t
                .roster
                .iter()
                .filter_map(|k| friend_number(friends, k))
                .map(|n| tox.friend_connection(n).max(0) as u64)
                .max()
                .unwrap_or(0),
        },
        Ordering::Relaxed,
    );
    t.trouble.want_in_group.store(t.roster.len() as u64, Ordering::Relaxed);
    // **An empty roster is not a complete group**: `seen >= roster.len()` is
    // vacuously true at zero, and the roster reaches this thread one turn
    // behind the node loop that sets it. Measured, `split001316-2`: hand 1
    // opened into a group the joiner did not enter for another thirty seconds.
    // `S1-DZ`, `S1-EG`: complete when every other seat is a CONFIRMED member;
    // the library's count holds unconfirmed entries. `S1-FE`: every OTHER
    // seat -- see `seen`.
    t.trouble.complete.store(
        t.group.is_some() && group_complete(seats_here(t), t.roster.len()),
        Ordering::Relaxed,
    );
    // `S1-KH`: a group a client of this build made keeps nobody out; one that
    // does was changed by its founder since -- the only member whose word on
    // the group's shared state the library takes. Read once this copy has
    // confirmed a member, by when the founder's state has come with it; before
    // that the library holds its defaults, which lock nothing.
    if let (Some(g), true) = (t.group, t.settled) {
        if let Some(lock) = tox.group_lock(g).filter(|l| l.locked()) {
            if let Ok(mut seen) = t.trouble.group_lock.lock() {
                if seen.is_none() {
                    *seen = Some(lock);
                }
            }
        }
    }

    // `S1-DB`: what a seat's dead process left in the group goes, once the seat
    // is back under a fresh key of its own binding. **After the counts above
    // are out, never before them**: the node says its list and its ratification
    // again when the group's count rises (`S1-DX`), which is how a seat back
    // from a restart learns the session from every member. Dropped first, the
    // old entry and the fresh one never stood in one count, no member saw
    // anybody arrive, and the seat that was back sat at *ratified 2 of 3* for
    // the rest of the run (`run162035-3`).
    if let (Some(g), true) = (t.group, t.setup.binder.is_some()) {
        drop_stale_twins(tox, t, g);
        // `S1-KI`: and what a seat holds past two living entries.
        cut_surplus(tox, t, g);
    }

    // **A join that never finished, given up and started again** (`S1-AA`
    // shape (i)). Past `JOIN_GRACE` the chat is destroyed -- the one lever
    // that clears toxcore's gate -- and the next invitation tries again, at
    // most `MAX_REJOINS` times; the budget is fresh whenever this client
    // becomes invitable again, so an outage does not spend it.
    let can_be_invited = self_connection > 0 && mine_up > 0;
    if can_be_invited && !t.was_reachable {
        t.rejoins = 0;
        t.trouble.rejoins.store(0, Ordering::Relaxed);
    }
    t.was_reachable = can_be_invited;
    // `S1-KO`: and since when, for a lone copy's patience (`member_offers`).
    if can_be_invited {
        t.reachable_since.get_or_insert_with(Instant::now);
    } else {
        t.reachable_since = None;
    }
    // `S1-FE`: and a founder taken back into its group by a member's invitation,
    // whose join can stall like anybody's.
    if !t.self_joined
        && t.group.is_some()
        && can_be_invited
        && (matches!(t.setup.role, Role::Joiner { .. } | Role::Back { .. })
            || (matches!(t.setup.role, Role::Host) && t.own_chat.is_some()))
    {
        if let Some(since) = t.accepted_at {
            if since.elapsed() >= JOIN_GRACE && t.rejoins < MAX_REJOINS {
                if let Some(g) = t.group.take() {
                    let _ = tox.leave(g);
                }
                t.accepted_at = None;
                t.settled = false;
                t.confirmed.clear();
                t.rejoins += 1;
                t.trouble.rejoins.store(t.rejoins as u64, Ordering::Relaxed);
            }
        }
    }
}

/// The driver: one instance, every table, until the client ends.
fn run(mut tox: Tox, control: sync_mpsc::Receiver<Ctl>, nodes: Vec<crate::tox::nodes::Node>) {
    // `S1-FB`: when the instance went offline, and when it was last pointed at
    // the bootstrap nodes again. See `REBOOTSTRAP_AFTER`.
    let mut offline_since: Option<Instant> = None;
    let mut last_rebootstrap = Instant::now();
    let mut rebootstraps = 0u32;
    // Tox friend number -> that friend's public key. The instance's, shared by
    // every table: a friendship outlives the table it was made for as long as
    // any open table needs it, and `FRIEND_LINGER` past that.
    let mut friends: HashMap<u32, [u8; 32]> = HashMap::new();
    // Friends no open table needs, and since when.
    let mut idle: HashMap<u32, Instant> = HashMap::new();
    // **This client's own key, so it can never be added to a roster.** See
    // `Setup::roster`: the caller tells the driver about every seat, its own
    // included, and a roster that counted it would wait for a seat that can
    // never arrive -- measured, every joiner at a six-seat table reporting
    // *held 4 of 6 other seats* and running to the sixty-second fallback.
    let me: [u8; 32] = tox.address()[..32].try_into().unwrap_or([0u8; 32]);
    // Which friends toxcore currently reports as up. An invitation is a
    // condition, not an event (see `invite_pending`), and this is the set the
    // condition is read from; reconciled every sweep, since events say what
    // changed and never what is.
    let mut connected: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut tables: HashMap<TableId, TableState> = HashMap::new();
    let mut last_sweep = Instant::now();
    let mut stopping = false;
    // `S1-EB`: invitations taken one turn after the emptied copy of their
    // group was left, once the library has let go of the chat.
    let mut invites_to_take: Vec<(u32, Vec<u8>)> = Vec::new();
    let mut invites_deferred: Vec<(u32, Vec<u8>)> = Vec::new();
    #[cfg(feature = "fault-harness")]
    let started = Instant::now();
    #[cfg(feature = "fault-harness")]
    let mut line_cut = false;
    // fault-harness, `D-051`: the flood and the stranger.
    #[cfg(feature = "fault-harness")]
    let mut flood_sent: u64 = 0;
    #[cfg(feature = "fault-harness")]
    let mut flood_said = false;
    #[cfg(feature = "fault-harness")]
    let mut junk_state: u64 = 0x9E37_79B9_7F4A_7C15 ^ u64::from(std::process::id());
    #[cfg(feature = "fault-harness")]
    let mut strangers: Vec<Stranger> = Vec::new();
    #[cfg(feature = "fault-harness")]
    let mut stranger_tried = false;

    loop {
        // --- what the client asked for -------------------------------------
        while let Ok(ctl) = control.try_recv() {
            match ctl {
                Ctl::Stop => {
                    stopping = true;
                    for t in tables.values_mut() {
                        t.closing.get_or_insert(FLUSH_TURNS);
                    }
                }
                Ctl::Open {
                    id,
                    setup,
                    out,
                    inbox,
                    chat,
                    trouble,
                } => {
                    let roster: Vec<[u8; 32]> =
                        setup.roster.iter().copied().filter(|k| *k != me).collect();
                    for key in &roster {
                        befriend(&mut tox, &mut friends, &mut idle, key);
                    }
                    if let Role::Joiner { founder, .. } = &setup.role {
                        if *founder != me {
                            befriend(&mut tox, &mut friends, &mut idle, founder);
                        }
                    }
                    let group = match &setup.role {
                        Role::Host => tox.new_group(&setup.group_name, &setup.self_name).ok(),
                        Role::Joiner { .. } | Role::Back { .. } => None,
                    };
                    // `D-051`: named with this seat's binding before anybody joins.
                    let named = match (group, setup.binder.as_ref()) {
                        (Some(g), Some(key)) => name_self(&mut tox, key, g).then_some(g),
                        _ => None,
                    };
                    // Said as soon as there is something to say: the founder's
                    // advertisement cannot name the group until this arrives.
                    announce(&tox, group, &chat);
                    tables.insert(
                        id,
                        TableState {
                            setup,
                            roster,
                            known_as: HashMap::new(),
                            peer_keys: HashMap::new(),
                            peer_lines: HashMap::new(),
                            had_members: false,
                            settled: false,
                            group,
                            away: false,
                            away_said_at: None,
                            named,
                            seat_apps: Vec::new(),
                            seats_fixed: false,
                            placed_seats: std::collections::HashSet::new(),
                            placed_keys: std::collections::HashSet::new(),
                            bound: HashMap::new(),
                            unplaced: HashMap::new(),
                            barred: std::collections::HashSet::new(),
                            barred_lines: std::collections::HashSet::new(),
                            meters: HashMap::new(),
                            cut: std::collections::HashSet::new(),
                            #[cfg(feature = "fault-harness")]
                            peak: (0, 0, 0, 0),
                            #[cfg(feature = "fault-harness")]
                            peak_said: (0, 0, 0, 0),
                            invited: Vec::new(),
                            confirmed: std::collections::HashSet::new(),
                            accepted_at: None,
                            self_joined: false,
                            rejoins: 0,
                            stalled_once: false,
                            was_reachable: false,
                            last_reinvite: Instant::now(),
                            pending: Vec::new(),
                            next_id: 0,
                            reassembler: Reassembler::new(fragment::TOX_PACKET),
                            closing: None,
                            last_invite: None,
                            offer_again: Vec::new(),
                            held_offer: None,
                            own_chat: None,
                            founder_offered: None,
                            seat_lines: Vec::new(),
                            out_lines: std::collections::HashSet::new(),
                            in_game: Vec::new(),
                            seat_offered: HashMap::new(),
                            in_game_from_hand: false,
                            in_game_present: Vec::new(),
                            empty_since: None,
                            reachable_since: None,
                            waiting_on: HashMap::new(),
                            founder_away_by_hands: false,
                            founder_outside: false,
                            out,
                            inbox,
                            chat,
                            trouble,
                        },
                    );
                }
                Ctl::For {
                    id,
                    command: Command::Leave,
                } => {
                    // **Said before it is left, not dropped.** What is still
                    // in `pending` when a table closes is the LAST message it
                    // produced -- the `HAND_COMPLETE` that ends the hand.
                    // Measured: a hand played to the river over Tox, the seat
                    // that finished first left, and the other sat at the
                    // settlement stage until its deadline. Bounded, because
                    // leaving must not become waiting: `FLUSH_TURNS` turns of
                    // the loop below, and the other tables keep their turns.
                    if let Some(t) = tables.get_mut(&id) {
                        t.closing.get_or_insert(FLUSH_TURNS);
                    }
                }
                Ctl::For { id, command } => {
                    let Some(t) = tables.get_mut(&id) else {
                        continue;
                    };
                    // `S1-KO`: the lines the table's word puts out for good now.
                    let mut out_now: Vec<[u8; 32]> = Vec::new();
                    match command {
                        Command::Nudge { app_key, seat } => {
                            // `patches/0011`: ask this seat for the message a
                            // stage is waiting on -- one small lossy packet,
                            // throttled by toxcore, harmless to a seat that
                            // never sent it. The reverse of `known_as`, not
                            // `peer_for`: both entry points take a public key,
                            // and a scan of `0..PEER_SCAN` per nudge took the
                            // Tox lock up to 576 times a tick (measured: the
                            // table dropped from 44 hands in 900 s to 18).
                            if let Some(g) = t.group {
                                // `S1-DU`: the seat's freshest entry among the
                                // confirmed ones `peer_keys` remembers (no scan),
                                // or failing that any key taught for it.
                                let live = entries_of(
                                    &app_key,
                                    t.peer_keys
                                        .iter()
                                        .filter(|(p, _)| t.confirmed.contains(p))
                                        .map(|(p, k)| (*p, *k)),
                                    &t.known_as,
                                );
                                let group_key = freshest(&live, |gk| tox.peer_quiet_secs(g, gk)).or_else(|| {
                                    t.known_as.iter().find(|(_, a)| **a == app_key).map(|(gk, _)| *gk)
                                });
                                if let Some(group_key) = group_key {
                                    tox.request_missing(g, &group_key);
                                    let bit = 1u32 << u32::from(seat.min(31));
                                    if tox.recv_pending(g, &group_key) > 0 {
                                        t.trouble.mid_delivery.fetch_or(bit, Ordering::Relaxed);
                                    } else {
                                        t.trouble.mid_delivery.fetch_and(!bit, Ordering::Relaxed);
                                    }
                                }
                            }
                        }
                        // This client. Not a friend of itself and not a peer
                        // of itself; see `me` above.
                        Command::Seated(key) if key == me => {}
                        // `S1-KL`: a seat removed for good is seated here no more.
                        Command::Seated(key) if t.barred_lines.contains(&key) => {}
                        Command::Seated(key) => {
                            if !t.roster.contains(&key) {
                                t.roster.push(key);
                            }
                            befriend(&mut tox, &mut friends, &mut idle, &key);
                        }
                        // What signed traffic has taught this client about who
                        // is who in the group: the only authenticated bridge
                        // between the two key spaces (`S1-I`).
                        // `D-051`: a member whose name binds it decides its own
                        // pairing; a claim read off carried traffic -- which
                        // anybody can carry -- does not move it.
                        Command::KnownAs { group_key, .. } if t.bound.contains_key(&group_key) => {}
                        Command::KnownAs { group_key, app_key } => {
                            // `S1-DW`: a claim to a seat that is here and speaking
                            // is refused -- a member's first word could be another
                            // seat's signed message said again, and the pairing
                            // would make that member's exit read as the seat's. A
                            // seat back under a fresh key (S1-DU) passes: its old
                            // entry has been quiet.
                            const CLAIM_QUIET_S: u64 = 20;
                            let refused = t.group.is_some_and(|g| {
                                let entries = t.known_as.iter().map(|(gk, a)| {
                                    let confirmed =
                                        t.peer_keys.iter().any(|(p, k)| k == gk && t.confirmed.contains(p));
                                    (*gk, *a, confirmed, tox.peer_quiet_secs(g, gk))
                                });
                                claim_refused(&group_key, &app_key, entries, CLAIM_QUIET_S)
                            });
                            if refused {
                                t.trouble.claims_refused.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                            t.known_as.insert(group_key, app_key);
                            if let Ok(mut known) = t.trouble.known.lock() {
                                known.insert(app_key);
                            }
                            // `S1-DS`: present at once if the group holds it now, not
                            // at the next sweep -- the friend link no longer stands
                            // in for a taught seat, and five seconds of *not on the
                            // line* at every deal would be its price.
                            if let Some(g) = t.group {
                                let member = peer_of(&tox, t, g, &group_key);
                                if member.is_some_and(|p| t.confirmed.contains(&p)) {
                                    if let Ok(mut present) = t.trouble.present.lock() {
                                        present.insert(app_key);
                                    }
                                }
                            }
                        }
                        Command::Unseated(key) => {
                            t.roster.retain(|k| *k != key);
                            unseat(&mut tox, t, &key);
                        }
                        Command::Roster(keys) => {
                            // `D-044`: the roster as the formation holds it now. A
                            // seat it no longer names leaves this table's roster
                            // here, so the group gate wants the seats that remain
                            // -- a joiner's driver had never been told a seat was
                            // gone, and waited for it (`run130909-3`: *2 wanted*
                            // ninety seconds after the founder said two).
                            // `D-045`: only a roster that names this client is the
                            // table's word; one that does not -- an empty one,
                            // `run140845-3`, where a joiner reported it and would
                            // have dropped its founder for good -- changes nothing.
                            if !keys.contains(&me) {
                                continue;
                            }
                            let gone: Vec<[u8; 32]> =
                                t.roster.iter().filter(|k| !keys.contains(k)).copied().collect();
                            for key in gone {
                                t.roster.retain(|k| *k != key);
                                unseat(&mut tox, t, &key);
                            }
                            for key in keys {
                                if key == me {
                                    continue;
                                }
                                // `S1-KL`: nor a seat the table's word removed for
                                // good (`D-047`). The roster names it still -- its
                                // chips left the table, its seat did not -- and is
                                // said again at every change: each saying took it
                                // back in, and a founder put out for good was
                                // offered the group by every member every
                                // `REINVITE_EVERY`, came in under a fresh key and
                                // was cut off again, for as long as the table ran.
                                if t.barred_lines.contains(&key) {
                                    continue;
                                }
                                if !t.roster.contains(&key) {
                                    t.roster.push(key);
                                }
                                befriend(&mut tox, &mut friends, &mut idle, &key);
                            }
                        }
                        Command::Remove { app_key, tox_key, for_good } => {
                            if let Some(g) = t.group {
                                let keys = keys_of_seat(t, app_key.as_ref(), tox_key.as_ref());
                                let host = matches!(t.setup.role, Role::Host);
                                for gk in keys {
                                    let _ = tox.allow_kick(g, &gk);
                                    if host {
                                        let peer = t.peer_keys.iter().find(|(_, k)| **k == gk).map(|(p, _)| *p);
                                        if let Some(p) = peer {
                                            let _ = tox.kick(g, p);
                                            // `S1-ED`: the library's kick deletes without the exit
                                            // callback; the bookkeeping is done here.
                                            member_gone(&tox, t, g, p, Some(gk), false);
                                        }
                                    }
                                    if tox.peer_drop(g, &gk, for_good) {
                                        t.trouble.removed.fetch_add(1, Ordering::Relaxed);
                                    }
                                }
                            }
                            // `D-047`: for good is for good -- off this table's roster
                            // too, so nobody here offers the group to it again (a fresh
                            // key needs an invitation) and its friendship idles out with
                            // the others no table needs. `S1-KL`: and its line barred for
                            // the table's life, so no roster said again takes it back,
                            // and whether or not this client holds a group this moment:
                            // a word that came while it held none was lost.
                            if for_good {
                                if let Some(k) = tox_key {
                                    t.roster.retain(|r| *r != k);
                                    t.barred_lines.insert(k);
                                    // `S1-KO`: by the table's word, which alone tells
                                    // a member its founder is out.
                                    t.out_lines.insert(k);
                                    out_now.push(k);
                                }
                                // `D-051`: and barred by its application key, so a
                                // fresh member key bound to it is cut off on sight.
                                if let Some(a) = app_key {
                                    match t.group {
                                        Some(g) => bar_seat(&mut tox, t, g, a),
                                        None => {
                                            t.barred.insert(a);
                                        }
                                    }
                                }
                            }
                        }
                        Command::InGame { apps, present, from_hand } => {
                            t.in_game = apps;
                            t.in_game_present = present;
                            t.in_game_from_hand = from_hand;
                        }
                        Command::FounderAway { away, outside } => {
                            t.founder_away_by_hands = away;
                            t.founder_outside = outside || away;
                        }
                        Command::Seats { apps, lines, fixed } => {
                            t.seat_apps = apps;
                            t.seat_lines = lines;
                            t.seats_fixed = fixed;
                            // Every member not placed yet is read again against the
                            // seats as they now stand.
                            if let Some(g) = t.group {
                                let members: Vec<(u32, [u8; 32])> = t
                                    .peer_keys
                                    .iter()
                                    .filter(|(p, k)| t.confirmed.contains(p) && t.known_as.get(*k).is_none_or(|a| !t.seat_apps.contains(a)))
                                    .map(|(p, k)| (*p, *k))
                                    .collect();
                                for (p, k) in members {
                                    place_member(&mut tox, t, g, p, k);
                                }
                            }
                        }
                        Command::Noise { member_key, points } => {
                            if let Some(g) = t.group {
                                score_noise(&mut tox, t, g, member_key, points);
                            }
                        }
                        Command::LockGroup => {
                            #[cfg(feature = "fault-harness")]
                            if let Some(g) = t.group {
                                let took = tox.lock_group(g);
                                println!(
                                    "fault-harness: the founder locked the group: peer limit {}, password {}, public {} (S1-KH)",
                                    took[0], took[1], took[2]
                                );
                            }
                        }
                        Command::KickWithoutWord(key) => {
                            if let Some(g) = t.group {
                                let peers: Vec<u32> =
                                    t.peer_lines.iter().filter(|(_, l)| **l == key).map(|(p, _)| *p).collect();
                                for p in peers {
                                    let _ = tox.kick(g, p);
                                }
                            }
                        }
                        // `S1-KL`: a seat removed for good asking back is no seat
                        // back: nothing is offered it.
                        Command::Rejoined(key) if t.barred_lines.contains(&key) => {}
                        Command::Rejoined(key) if key != me => {
                            // A seat back after a restart needs the group
                            // offered afresh; a friendship that is down is
                            // remade so the invitation has a link to ride --
                            // toxcore keeps trying the address the peer had
                            // before it restarted for over two and a half
                            // minutes otherwise (see `Tox::forget_friend`).
                            if offers_the_group(&t.setup.role, t.group.is_some(), t.self_joined, t.settled) {
                                if let Some(n) = friend_number(&friends, &key) {
                                    t.invited.retain(|f| *f != n);
                                    if !connected.contains(&n) {
                                        let _ = tox.forget_friend(n);
                                        friends.remove(&n);
                                        connected.remove(&n);
                                        idle.remove(&n);
                                        if let Ok(fresh) = tox.add_friend(&key) {
                                            friends.insert(fresh, key);
                                        }
                                    }
                                }
                                t.last_invite = None;
                                invite_pending(&mut tox, t, &friends, &connected);
                            }
                        }
                        Command::Rejoined(_) => {}
                        Command::Away(on) => {
                            // Said now -- the sweep is five seconds apart, and a
                            // seat at the far end waited nine for the word
                            // (run155209-3) -- and again every `AWAY_RESAY`
                            // after (see `sweep_table`).
                            if t.away != on {
                                t.away = on;
                                t.away_said_at = None;
                                if let Some(g) = t.group {
                                    if tox.set_self_away(g, on) {
                                        t.away_said_at = Some(Instant::now());
                                    }
                                }
                            }
                        }
                        Command::Leave => {}
                    }
                    // `S1-KO`: and its friendship goes at once where no open table
                    // needs it, not when the table closes: a founder put out for good
                    // stayed a friend of every seat that had befriended it -- on its
                    // line, its address in view -- for as long as the table ran, and a
                    // seat back from a restart befriends its founder as it opens.
                    for line in out_now {
                        if tables.values().any(|o| o.needs(&line)) {
                            continue;
                        }
                        if let Some(n) = friend_number(&friends, &line) {
                            let _ = tox.forget_friend(n);
                            friends.remove(&n);
                            connected.remove(&n);
                            idle.remove(&n);
                        }
                    }
                }
            }
        }

        // `patches/0035`: the harness's outage, applied on its edges.
        #[cfg(feature = "fault-harness")]
        {
            let now = started.elapsed();
            let scheduled = offline_in(offline_window(), now);
            let asked = CUT_UNTIL.lock().ok().and_then(|u| *u).is_some_and(|u| Instant::now() < u);
            let cut = scheduled || asked;
            if cut != line_cut {
                tox.cut_line(cut);
                line_cut = cut;
                println!(
                    "fault-harness: the internet {} at {} s, as P2P_POKER_OFFLINE_AT asked",
                    if cut { "goes away" } else { "is back" },
                    now.as_secs()
                );
            }
        }
        // --- one turn of toxcore's own loop --------------------------------
        // `S1-KP` (`REFUTE_S1KP_v1_groups` G7): the tables that left an emptied
        // copy in this turn -- every later invitation for one of them waits a turn.
        let mut left_this_turn: std::collections::HashSet<TableId> = std::collections::HashSet::new();
        for e in tox.iterate() {
            match e {
                Event::FriendConnection { friend, status } if status != 0 => {
                    connected.insert(friend);
                    for t in tables.values_mut() {
                        if t.closing.is_some() {
                            continue;
                        }
                        // Up. The founder invites every seat the roster names
                        // once its friend connection is up -- from the loop
                        // below, one at a time (`invite_pending`).
                        // `D-037`: a member offers the group to the founder
                        // whose friendship has just come up -- a founder that
                        // restarted has the same key and no group. One still
                        // in the group refuses the offer.
                        if let (Role::Joiner { founder, .. }, Some(g)) = (&t.setup.role, t.group) {
                            // `S1-KL`: never a founder the table's word removed for good.
                            // `S1-KP` (`REFUTE_S1KP_v2_groups` V5): nor from a copy held empty
                            // while the founder is away by the node's reading.
                            let lone_and_away = t.founder_away_by_hands && held_empty(t);
                            if t.self_joined
                                && friends.get(&friend) == Some(founder)
                                && !t.barred_lines.contains(founder)
                                && !lone_and_away
                            {
                                t.invited.retain(|f| *f != friend);
                                if tox.invite(g, friend).is_ok() {
                                    t.invited.push(friend);
                                    // `S1-FE`: the sweep's next offer counts from this one.
                                    t.founder_offered = Some(Instant::now());
                                    t.trouble.invites_sent.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                        // `S1-KO`: and once the founder is out for good, a seat of the
                        // game whose friendship has just come up -- a seat back from a
                        // restart has the same key and no group; one still in the group
                        // refuses the offer. Only a seat still in the game, no more often
                        // than the sweep's `REINVITE_EVERY` -- a link that flaps would
                        // otherwise fill the library's ring of invitations -- and only
                        // while this member offers its copy at all (`member_offers`).
                        // `S1-KP` (`D-103`): and for a founder away by the node's reading
                        // -- never to that founder itself (`S1-FE` offers it, just above).
                        if let (true, Some(g)) = (founder_away(t), t.group) {
                            let line = friends
                                .get(&friend)
                                .copied()
                                .filter(|k| in_game_line(t, k) && !t.waiting_on.contains_key(k) && !founder_line_away(t, k));
                            let mine = t.setup.binder.as_ref().map(|k| k.verifying_key().to_bytes());
                            if let (true, Some(k), Some(mine)) = (t.self_joined && t.settled, line, mine) {
                                let due = t.seat_offered.get(&k).is_none_or(|at| at.elapsed() >= REINVITE_EVERY);
                                let offers = due && member_offers(t, &mine, &friends, &connected);
                                if offers && tox.invite(g, friend).is_ok() {
                                    // The sweep's next offer to it counts from this one.
                                    t.seat_offered.insert(k, Instant::now());
                                    t.trouble.invites_sent.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                }
                Event::FriendConnection { friend, .. } => {
                    // Down. The invitation is forgotten so that a peer which
                    // reconnects is invited again.
                    connected.remove(&friend);
                    for t in tables.values_mut() {
                        t.invited.retain(|f| *f != friend);
                    }
                }
                Event::GroupInvite { friend, invite } => {
                    // `S1-EB`: a table whose group this client still holds with
                    // nobody else left in it -- every other member timed out of
                    // this view, as this client did out of theirs -- leaves that
                    // copy now and takes the invitation at the next turn, once
                    // the library has let go of the chat: a confirmation read
                    // while both copies are held would find the old one. The
                    // library delivers such an invitation since `patches/0035`;
                    // before it, the founder's fresh offer was swallowed and a
                    // seat whose old address no longer answered had no way back.
                    let from = friends.get(&friend).copied();
                    let mut left_one = false;
                    for (id, t) in tables.iter_mut() {
                        // `S1-EK`: and settled once -- a copy whose founder is a round
                        // trip from confirmed is not an emptied one; leaving it said
                        // goodbye, the founder freed the seat before the first hand,
                        // and the window closed while joining.
                        if !held_empty(t) {
                            continue;
                        }
                        if !invitation_fits(t, from) {
                            continue;
                        }
                        // `S1-FE`: a founder keeps a member's invitation into its own
                        // group -- the newest bytes, the first one's time -- and takes
                        // it only if its copy still holds nobody else
                        // `FOUNDER_YIELDS_AFTER` later (the driver's turn, below).
                        if matches!(t.setup.role, Role::Host) {
                            let ours = t.group.and_then(|g| tox.chat_id(g).ok());
                            if ours.is_some_and(|c| invite.get(..c.len()) == Some(&c[..])) {
                                let since = t.held_offer.as_ref().map_or_else(Instant::now, |(_, _, at)| *at);
                                if t.held_offer.is_none() {
                                    println!(
                                        "the founder's copy of the table's group holds nobody else and a member offers the group back; taken in {} s unless a member comes back to this copy first (S1-FE)",
                                        FOUNDER_YIELDS_AFTER.as_secs()
                                    );
                                }
                                t.held_offer = Some((friend, invite.clone(), since));
                            }
                            continue;
                        }
                        // `S1-KH`: and only for an invitation into the group
                        // this table's advertisement named. An invitation says
                        // which group it is for, and one into another -- the
                        // founder's other table, where this seat sits too --
                        // left a copy that would have come back by itself.
                        if !invite_names(&invite, wanted_chat(t)) {
                            continue;
                        }
                        // `S1-KO`: a member whose founder is out for good, offered the
                        // group by another seat of the game while its own copy holds
                        // nobody else. From a lower line it goes at once: that copy is
                        // never left for this one's offer. From a higher line, by order
                        // (`LONE_YIELDS_AFTER`): the first such offer is answered with
                        // one offer of this copy and not taken -- a lone copy of the
                        // higher line is left at once for it and stops offering itself
                        // -- and an offer that member makes `LONE_YIELDS_AFTER` after
                        // that one is taken: its copy did not leave. Two copies each
                        // left for the other would be no copy at all. Nothing is kept for
                        // later -- an offer is taken as it comes, and a kept one could
                        // outlive the copy it was made from.
                        // `S1-KP` (`D-103`): and so while the founder is away by the node's
                        // reading, the founder's own line ordered above every member's
                        // (`REFUTE_S1KP_v1_groups` G3): its first offer is not taken, a later
                        // one `LONE_YIELDS_AFTER` on is -- a founder apart from the table no
                        // longer takes a lone member at once.
                        if let (true, Some(k), Some(g)) =
                            (founder_away(t), from.filter(|k| *k > me || founder_line_away(t, k)), t.group)
                        {
                            let now = Instant::now();
                            match t.waiting_on.get_mut(&k) {
                                Some((answered, last)) => {
                                    *last = now;
                                    if answered.elapsed() < LONE_YIELDS_AFTER {
                                        continue;
                                    }
                                }
                                None => {
                                    // `S1-KP`: answered with this copy only by a seat its own
                                    // hand counts in the game (`REFUTE_S1KP_v1_groups` G6: one
                                    // back from a restart holds none yet), and never to the
                                    // founder away by the node's reading -- whose kept offer
                                    // would take this lone copy apart from the table
                                    // (`REFUTE_S1KP_v2_groups` V5). Recorded all the same: the
                                    // other copy's own offer is taken `LONE_YIELDS_AFTER` on.
                                    if t.in_game_from_hand && !founder_line_away(t, &k) && tox.invite(g, friend).is_ok() {
                                        t.seat_offered.insert(k, now);
                                        t.trouble.invites_sent.fetch_add(1, Ordering::Relaxed);
                                    }
                                    t.waiting_on.insert(k, (now, now));
                                    continue;
                                }
                            }
                        }
                        if let Some(g) = t.group.take() {
                            let _ = tox.leave(g);
                            t.self_joined = false;
                            t.settled = false;
                            t.accepted_at = None;
                            t.peer_keys.clear();
                            t.peer_lines.clear();
                            t.invited.clear();
                            t.trouble.left_empty.fetch_add(1, Ordering::Relaxed);
                            println!(
                                "the table's group held nobody else here and it is offered again: left the old copy, taking the invitation (S1-EB)"
                            );
                            left_one = true;
                            left_this_turn.insert(*id);
                        }
                    }
                    // `S1-KP` (`D-103`, `REFUTE_S1KP_v1_groups` G7): and so for a second
                    // invitation in this same turn for a table that left its copy in it --
                    // found with no group, it was taken at once, while the library still
                    // held the copy just left.
                    let deferred_table = tables
                        .iter()
                        .any(|(id, t)| left_this_turn.contains(id) && t.group.is_none() && invitation_fits(t, from));
                    if left_one || deferred_table {
                        invites_deferred.push((friend, invite));
                    } else {
                        take_invitation(&mut tox, &mut tables, &friends, friend, &invite);
                    }
                }
                Event::GroupSelfJoin { group: g } => {
                    // **The join actually finished.** Until this fires, holding
                    // a group number means nothing.
                    if let Some(t) = by_group(&mut tables, g) {
                        t.self_joined = true;
                        t.accepted_at = None;
                    }
                }
                Event::GroupJoinFail { group: g, reason } => {
                    // toxcore gave up on its own. Leave, so the chat id stops
                    // blocking the next invitation, and let the sweep retry.
                    if let Some(t) = by_group(&mut tables, g) {
                        let _ = tox.leave(g);
                        t.group = None;
                        t.accepted_at = None;
                        t.self_joined = false;
                        t.settled = false;
                        t.confirmed.clear();
                        t.trouble.join_fails.fetch_add(1, Ordering::Relaxed);
                        let _ = reason;
                    }
                }
                // `D-049`: a member's word on sitting out, taken at once -- the
                // sweep reads it again within five seconds either way. A status
                // comes from the member that sent it, so it is that seat's
                // freshest word.
                Event::GroupPeerStatus { group: g, peer, away } => {
                    if let Some(t) = by_group(&mut tables, g) {
                        let app_key = t.peer_keys.get(&peer).and_then(|k| t.known_as.get(k)).copied();
                        if let (Some(k), Ok(mut set)) = (app_key, t.trouble.away.lock()) {
                            if away {
                                set.insert(k);
                            } else {
                                set.remove(&k);
                            }
                        }
                        if let (Some(line), Ok(mut set)) = (t.peer_lines.get(&peer).copied(), t.trouble.away_lines.lock()) {
                            if away {
                                set.insert(line);
                            } else {
                                set.remove(&line);
                            }
                        }
                    }
                }
                Event::GroupPeerJoin { group: g, peer } => {
                    let key = tox.peer_key(g, peer).ok();
                    // `S1-DV`: the friend this member came in through, if any.
                    let line = key
                        .and_then(|k| tox.peer_friend_number(g, &k))
                        .and_then(|n| friends.get(&n).copied());
                    if let Some(t) = by_group(&mut tables, g) {
                        t.confirmed.insert(peer);
                        t.had_members = true;
                        t.settled = true;
                        if let Some(k) = key {
                            t.peer_keys.insert(peer, k);
                        }
                        if let Some(l) = line {
                            t.peer_lines.insert(peer, l);
                            if let Ok(mut present) = t.trouble.present_lines.lock() {
                                present.insert(l);
                            }
                        }
                        // `D-051`: which seat it is, by the binding in its name.
                        if let Some(k) = key {
                            place_member(&mut tox, t, g, peer, k);
                        }
                    }
                }
                // `D-051`: a name changed. A seat's client names itself once,
                // before anybody can read it; any later name is read again.
                Event::GroupPeerName { group: g, peer } => {
                    let key = tox.peer_key(g, peer).ok();
                    if let (Some(t), Some(k)) = (by_group(&mut tables, g), key) {
                        if t.confirmed.contains(&peer) {
                            place_member(&mut tox, t, g, peer, k);
                        }
                    }
                }
                // `D-051`: traffic no client of ours sends.
                Event::GroupStray { group: g, peer, .. } => {
                    let key = tox.peer_key(g, peer).ok();
                    if let (Some(t), Some(k)) = (by_group(&mut tables, g), key) {
                        score_noise(&mut tox, t, g, k, crate::table::membership::NOISE_STRAY);
                    }
                }
                Event::GroupPeerExit {
                    group: g,
                    peer,
                    key,
                    quit,
                } => {
                    if let Some(t) = by_group(&mut tables, g) {
                        member_gone(&tox, t, g, peer, key, quit);
                    }
                }
                Event::GroupPacket { group: g, peer, data } => {
                    // Reassembled here, so nothing above this module ever sees
                    // a fragment.
                    if let Some(t) = by_group(&mut tables, g) {
                        let now = millis();
                        // `D-051`: counted against the member before anything is
                        // spent on it; a member cut off here is not read at all.
                        let key = t.peer_keys.get(&peer).copied().or_else(|| tox.peer_key(g, peer).ok());
                        if let Some(k) = key {
                            if t.cut.contains(&k) {
                                continue;
                            }
                            // `S1-KI`: on its seat's meter, however many entries
                            // the seat holds.
                            let mk = metered_as(t, &k);
                            let meter = t.meters.entry(mk).or_default();
                            meter.packet(now, data.len());
                            let over = meter.over(now);
                            #[cfg(feature = "fault-harness")]
                            {
                                let (p10, b10) = meter.within(now, crate::table::membership::FLOOD_SHORT_S);
                                let (p60, b60) = meter.within(now, crate::table::membership::FLOOD_LONG_S);
                                let peak = &mut t.peak;
                                peak.0 = peak.0.max(p10);
                                peak.1 = peak.1.max(b10);
                                peak.2 = peak.2.max(p60);
                                peak.3 = peak.3.max(b60);
                            }
                            if let (Some(flood), true) = (over, t.setup.binder.is_some()) {
                                cut_off(&mut tox, t, g, k, Cut::Flood(flood));
                                continue;
                            }
                        }
                        match t.reassembler.accept(&peer, &data, now) {
                            Ok(Some(message)) => {
                                // The sender's GROUP key, advisory: the
                                // signature inside the message is the only
                                // thing that says who spoke. Carried out so
                                // the node loop can pair it with the signing
                                // key (`S1-I`).
                                let claimed = key;
                                if let Some(k) = claimed {
                                    t.peer_keys.insert(peer, k);
                                }
                                // `D-051`: while the inbox is three quarters full,
                                // a member sending far more than anybody this
                                // second waits its turn, so no one member fills
                                // it for the rest. An honest seat's burst is a
                                // re-said stage, well under `INBOX_LOUD` packets
                                // in a second and not twice everybody else's.
                                let max = t.inbox.max_capacity();
                                if t.inbox.capacity() < max / 4 {
                                    if let Some(k) = claimed {
                                        // `S1-KI`: by seat, as metered.
                                        let k = metered_as(t, &k);
                                        let mine = t.meters.get(&k).map_or(0, |m| m.this_second(now));
                                        let others = t
                                            .meters
                                            .iter()
                                            .filter(|(o, _)| **o != k)
                                            .map(|(_, m)| m.this_second(now))
                                            .max()
                                            .unwrap_or(0);
                                        if mine >= INBOX_LOUD && mine > others.saturating_mul(2) {
                                            t.trouble.inbox_yielded.fetch_add(1, Ordering::Relaxed);
                                            continue;
                                        }
                                    }
                                }
                                let item = FromTable {
                                    claimed,
                                    bytes: message,
                                };
                                if t.inbox.try_send(item).is_err() {
                                    // The client is not draining. Dropping is
                                    // right: blocking here would stop
                                    // `tox_iterate`. Counted, because it was
                                    // silent.
                                    t.trouble.inbox_dropped.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                            Ok(None) => {}
                            // `D-051`: a fragment no client of this build cuts.
                            Err(_) => {
                                if let Some(k) = key {
                                    score_noise(&mut tox, t, g, k, crate::table::membership::NOISE_BAD_FRAGMENT);
                                }
                            }
                        }
                    }
                }
                // `D-045`: this client was removed by the table's word. The group
                // it held is left, so an invitation can bring it back (D-031); a
                // group held would refuse one.
                Event::GroupModeration {
                    group: g,
                    target_is_self: true,
                    kick: true,
                } => {
                    if let Some(t) = by_group(&mut tables, g) {
                        let _ = tox.leave(g);
                        t.group = None;
                        t.self_joined = false;
                        t.settled = false;
                        t.confirmed.clear();
                        t.peer_keys.clear();
                        t.peer_lines.clear();
                        t.known_as.clear();
                        t.invited.clear();
                        t.trouble.kicked_out.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Event::GroupModeration { .. } => {}
                Event::FriendRequestIgnored => {}
            }
        }

        // `S1-EB`: the invitations deferred a turn ago, their old copies gone
        // with the turn of toxcore's loop just taken.
        for (friend, invite) in std::mem::take(&mut invites_to_take) {
            take_invitation(&mut tox, &mut tables, &friends, friend, &invite);
        }
        invites_to_take.append(&mut invites_deferred);

        // --- fault-harness, `D-051`: the flood and the stranger ---------------
        #[cfg(feature = "fault-harness")]
        {
            let now = started.elapsed();
            if let Some(at) = no_invites_at() {
                if now >= at && !NO_INVITES.swap(true, Ordering::Relaxed) {
                    if tables.values().any(|t| !matches!(t.setup.role, Role::Joiner { .. })) {
                        println!(
                            "fault-harness: this client offers the table's group to nobody from {} s, as P2P_POKER_NO_INVITES_AT asked (S1-KH)",
                            at.as_secs()
                        );
                    }
                }
            }
            if let Some((at, rate, kind)) = flood_plan() {
                if let Some(since) = now.checked_sub(at) {
                    if !flood_said {
                        flood_said = true;
                        println!(
                            "fault-harness: this client floods every table's group from {} s, {rate} packets a second of {kind:?}, as P2P_POKER_FLOOD_AT asked",
                            at.as_secs()
                        );
                    }
                    let due = (since.as_millis() as u64).saturating_mul(rate) / 1_000;
                    let groups: Vec<u32> = tables.values().filter_map(|t| t.group).collect();
                    for _ in 0..due.saturating_sub(flood_sent).min(2_000) {
                        for g in &groups {
                            let _ = flood_one(&mut tox, *g, kind, &mut junk_state, 0x8000_0000 | (flood_sent as u32));
                        }
                        flood_sent += 1;
                    }
                }
            }
            if let Some((at, flood, copy)) = stranger_plan() {
                let puppet = puppets().is_some();
                if now >= at && !stranger_tried {
                    stranger_tried = true;
                    for _ in 0..puppets().unwrap_or(1) {
                        match (Tox::new(), tox.local_node()) {
                            (Ok(mut st), Some((port, dht))) => {
                                let _ = st.bootstrap("127.0.0.1", port, &dht);
                                let here: [u8; 32] = tox.address()[..32].try_into().unwrap_or([0u8; 32]);
                                let there: [u8; 32] = st.address()[..32].try_into().unwrap_or([0u8; 32]);
                                let _ = st.add_friend(&here);
                                match tox.add_friend(&there) {
                                    Ok(n) => {
                                        println!(
                                            "fault-harness: a stranger is made at {} s and befriended, as P2P_POKER_STRANGER_AT asked",
                                            now.as_secs()
                                        );
                                        strangers.push(Stranger {
                                            tox: st,
                                            friend_here: n,
                                            invited: None,
                                            group: None,
                                            joined: false,
                                            flood_sent: 0,
                                            flood_since: None,
                                        });
                                    }
                                    Err(e) => println!("fault-harness: the stranger could not be befriended: {e}"),
                                }
                            }
                            _ => println!("fault-harness: no stranger could be made"),
                        }
                    }
                }
                for s in strangers.iter_mut() {
                    for e in s.tox.iterate() {
                        match e {
                            Event::GroupInvite { friend, invite } if s.group.is_none() => {
                                match s.tox.accept_invite(friend, &invite, "stranger") {
                                    Ok(g2) => {
                                        s.group = Some(g2);
                                        // `S1-KI`: this client's own binding for the
                                        // stranger's member key -- an entry of its seat.
                                        if puppet {
                                            let binder = tables.values().find_map(|t| t.setup.binder.clone());
                                            if let (Some(key), Ok(chat), Some(me)) =
                                                (binder, s.tox.chat_id(g2), s.tox.self_key(g2))
                                            {
                                                let name = crate::table::membership::binding_for(&key, &chat, &me);
                                                let _ = s.tox.set_self_name(g2, &name);
                                            }
                                        }
                                        if copy {
                                            // A seat's binding, as this client reads it.
                                            let name = tables.values().find_map(|t| {
                                                let g = t.group?;
                                                t.bound.keys().find_map(|k| {
                                                    let p = t.peer_keys.iter().find(|(_, kk)| *kk == k).map(|(p, _)| *p)?;
                                                    tox.peer_name(g, p)
                                                })
                                            });
                                            if let Some(n) = name {
                                                let _ = s.tox.set_self_name(g2, &n);
                                            }
                                        }
                                        println!(
                                            "fault-harness: the stranger took the invitation{}",
                                            if puppet {
                                                ", named with this client's own binding: an entry of its seat (S1-KI)"
                                            } else if copy {
                                                ", named with a copy of a seat's binding"
                                            } else {
                                                ""
                                            }
                                        );
                                    }
                                    Err(e) => println!("fault-harness: the stranger could not take the invitation: {e}"),
                                }
                            }
                            Event::GroupSelfJoin { .. } if !s.joined => {
                                s.joined = true;
                                s.flood_since = Some(Instant::now());
                                println!("fault-harness: the stranger is in the table's group");
                            }
                            _ => {}
                        }
                    }
                    // Again every five seconds until the stranger takes one: an
                    // invitation sent while this client's own join was settling
                    // went nowhere (`S1-KI`'s first beds, one puppet of three in).
                    let due = s.group.is_none() && s.invited.is_none_or(|at| at.elapsed() >= Duration::from_secs(5));
                    if due && tox.friend_connection(s.friend_here) > 0 {
                        if let Some(g) = tables.values().find_map(|t| t.group) {
                            if tox.invite(g, s.friend_here).is_ok() {
                                s.invited = Some(Instant::now());
                                println!("fault-harness: this client invited the stranger into its table's group");
                            }
                        }
                    }
                    if let (true, true, Some(g2), Some(since)) = (flood, s.joined, s.group, s.flood_since) {
                        let rate = flood_rate();
                        let due = (since.elapsed().as_millis() as u64).saturating_mul(rate) / 1_000;
                        for _ in 0..due.saturating_sub(s.flood_sent).min(2_000) {
                            let _ = flood_one(&mut s.tox, g2, FloodKind::Noise, &mut junk_state, 0);
                            s.flood_sent += 1;
                        }
                    }
                }
            }
        }

        // --- the founder's invitations, one at a time -------------------------
        // `S1-FR`: the founder's, and a founder back once it is in the group.
        for t in tables.values_mut() {
            if offers_the_group(&t.setup.role, t.group.is_some(), t.self_joined, t.settled) && t.closing.is_none() {
                // `S1-FB`: a member the group has just reported gone is no longer
                // one this founder "has invited" -- that record was of an
                // invitation to a membership that has ended. Forgotten here, the
                // very next call below offers the group again at once if the
                // seat's friend link is up, and on its up-edge if not. The
                // one-at-a-time gap is reset with it, so the offer is not held
                // behind an earlier one. Only while the group is short, as the
                // sweep's (`S1-EG`): a group holding every seat has nobody to
                // offer it to.
                if !t.offer_again.is_empty() {
                    let gone = std::mem::take(&mut t.offer_again);
                    if t.group.is_some() && group_short(seats_here(t), t.roster.len()) {
                        t.invited
                            .retain(|f| friends.get(f).is_none_or(|k| !gone.contains(k)));
                        t.last_invite = None;
                    }
                }
                // `S1-FE`: a member's invitation kept while this founder's copy
                // held nobody else. A member back in the copy means the founder's
                // own offer was taken, and the kept one is dropped; still nobody
                // `FOUNDER_YIELDS_AFTER` on, the copy is left and the invitation
                // taken like a joiner's (`S1-EB`) -- the members' group is the one
                // the table plays in.
                if let Some((friend, invite, since)) = t.held_offer.take() {
                    let emptied = held_empty(t);
                    if emptied && since.elapsed() >= FOUNDER_YIELDS_AFTER {
                        if let Some(g) = t.group.take() {
                            t.own_chat = tox.chat_id(g).ok();
                            let _ = tox.leave(g);
                            t.self_joined = false;
                            t.settled = false;
                            t.accepted_at = None;
                            t.peer_keys.clear();
                            t.peer_lines.clear();
                            t.invited.clear();
                            t.last_invite = None;
                            t.trouble.left_empty.fetch_add(1, Ordering::Relaxed);
                            println!(
                                "the founder's copy of the table's group held nobody else for {} s: left it, taking a member's invitation back into the table's group (S1-FE)",
                                since.elapsed().as_secs()
                            );
                            invites_deferred.push((friend, invite));
                        }
                    } else if emptied {
                        t.held_offer = Some((friend, invite, since));
                    }
                }
                invite_pending(&mut tox, t, &friends, &connected);
            } else {
                // A member of somebody else's group has nobody to invite.
                t.offer_again.clear();
            }
        }

        // --- and what each table wants said ---------------------------------
        for t in tables.values_mut() {
            while let Ok(message) = t.out.try_recv() {
                t.pending.push(message);
            }
            if let Some(g) = t.group {
                flush(&mut tox, g, &mut t.pending, &mut t.next_id, &t.trouble);
            }
        }
        // Tables that are closing go once what they held is said, or once
        // their turns are spent.
        let done: Vec<TableId> = tables
            .iter()
            .filter(|(_, t)| t.closing.is_some_and(|left| left == 0 || t.pending.is_empty()))
            .map(|(id, _)| *id)
            .collect();
        for id in done {
            close_table(&mut tox, &mut tables, id, &friends, &mut idle);
        }
        for t in tables.values_mut() {
            if let Some(left) = t.closing.as_mut() {
                *left = left.saturating_sub(1);
            }
        }
        if stopping && tables.is_empty() {
            break;
        }

        if last_sweep.elapsed() >= SWEEP_EVERY {
            // **Asked of toxcore rather than remembered.** `connected` is built
            // from events, which say what has changed and never what is.
            for (n, _) in friends.iter() {
                if tox.friend_connection(*n) > 0 {
                    connected.insert(*n);
                } else {
                    connected.remove(n);
                }
            }
            let self_connection = tox.connection().max(0) as u64;
            // `S1-FB`: **an instance that has been offline a while is pointed at
            // its bootstrap nodes again**, every `REBOOTSTRAP_EVERY`, until it is
            // back. It was bootstrapped once, when it started, and after that it
            // relied on toxcore's own reconnection alone -- which keeps pinging the
            // nodes of its close list, and after an outage long enough for every
            // one of them to time out has nobody left to ping. The owner's own
            // client read *self offline* eight minutes after an outage; whether
            // its line was back all that while, its log cannot say, and this
            // costs a packet per node for as long as the instance is offline.
            if self_connection == 0 {
                let since = *offline_since.get_or_insert_with(Instant::now);
                if !nodes.is_empty()
                    && since.elapsed() >= REBOOTSTRAP_AFTER
                    && last_rebootstrap.elapsed() >= REBOOTSTRAP_EVERY
                {
                    for n in &nodes {
                        let _ = tox.bootstrap(&n.host, n.udp_port, &n.key);
                        if let Some(port) = crate::tox::nodes::best_tcp_port(&n.tcp_ports) {
                            let _ = tox.add_tcp_relay(&n.host, port, &n.key);
                        }
                    }
                    last_rebootstrap = Instant::now();
                    rebootstraps += 1;
                    if rebootstraps == 1 {
                        println!(
                            "the Tox instance has been offline for {} s: bootstrapping it again from {} stored node(s), every {} s until it is back (S1-FB)",
                            since.elapsed().as_secs(),
                            nodes.len(),
                            REBOOTSTRAP_EVERY.as_secs()
                        );
                    }
                }
            } else {
                if let Some(since) = offline_since.take() {
                    if rebootstraps > 0 {
                        println!(
                            "the Tox instance is back after about {} s offline, bootstrapped again {rebootstraps} time(s) (S1-FB)",
                            since.elapsed().as_secs()
                        );
                    }
                }
                rebootstraps = 0;
            }
            for t in tables.values_mut() {
                if t.closing.is_none() {
                    sweep_table(&mut tox, t, &friends, &connected, self_connection);
                }
            }
            // `D-042`: the friends of a closed table go once no open table has
            // needed them for `FRIEND_LINGER`.
            let stale: Vec<u32> = idle
                .iter()
                .filter(|(n, since)| {
                    since.elapsed() >= FRIEND_LINGER
                        && !friends
                            .get(n)
                            .is_some_and(|k| tables.values().any(|t| t.needs(k)))
                })
                .map(|(n, _)| *n)
                .collect();
            for n in stale {
                let _ = tox.forget_friend(n);
                friends.remove(&n);
                connected.remove(&n);
                idle.remove(&n);
            }
            last_sweep = Instant::now();
        }

        std::thread::sleep(tox.interval().min(MAX_TICK));
    }

    // One more turn so the last part message actually goes out before the
    // instance is dropped. Leaving without it is leaving silently, and the
    // other seats then wait out a deadline for somebody who has gone.
    tox.iterate();
}

/// How many turns a leaving driver spends trying to send what it still holds.
///
/// Leaving must not become waiting, so it is a fixed number rather than a wait
/// for an empty queue: at `MAX_TICK` this is at most a second and a half.
const FLUSH_TURNS: usize = 30;

/// How long the founder waits for an invited seat to become a confirmed
/// member before it invites the next one regardless.
pub const INVITE_GAP: Duration = Duration::from_secs(3);

/// Invite the next seat that is connected, on the roster and not in the group
/// yet -- **one at a time**, the next once the last is a confirmed member or
/// `INVITE_GAP` after (`D-042`).
///
/// **An invitation is a condition, not an event.** It used to be sent only
/// from the `FriendConnection` up-edge, so a refusal from
/// `tox_group_invite_friend` was never retried and a seated player could sit
/// outside the table until the network happened to hiccup. Called every turn
/// of the loop now, which is what makes it certain rather than likely; cheap,
/// since it does nothing once every friend has been invited.
///
/// **And one at a time, because two seats invited in the same instant do not
/// find each other.** A joiner learns the members from one sync response, at
/// its join, and opens a handshake to each of them; a seat that is not yet a
/// confirmed member when that response is built is not in it. Two seats
/// invited together each sync before the other is confirmed, so neither holds
/// the other and neither opens the handshake -- they meet only when the
/// founder's next ping carries a peer count they do not have (every
/// `GC_PING_TIMEOUT`) and their own sync limit has passed. Measured,
/// `run212040-3`: a warm instance invited both seats of a second table in
/// one sweep, each saw *group 1 seen/1 confirmed/2 wanted* for 18 s, and the
/// table's first hand came 25 s after the join, where a first table -- whose
/// friend links came up a second apart -- dealt 5 s after it. A seat invited
/// after the last is confirmed gets the last in its own sync.
/// A member is gone from this view, on purpose, by timing out, or by the
/// table's word: the driver's maps, the felt's presence sets and the
/// `gone` queues the node reads are brought up to date. Called from the
/// library's exit callback, and by the founder's own kick (`S1-ED`): the
/// library deletes a kicked member without that callback, and a dead peer
/// number left in `peer_lines` kept the seat *on the line* by its line at
/// the founder's felt until the seat itself came back (run161903-3).
fn member_gone(tox: &Tox, t: &mut TableState, g: u32, peer: u32, key: Option<[u8; 32]>, quit: bool) {
    t.confirmed.remove(&peer);
    // `D-051`: the peer id is given to the next member that joins; its first
    // message must not meet a partial this one left.
    t.reassembler.forget(&peer);
    let remembered = t.peer_keys.remove(&peer);
    let line = t.peer_lines.remove(&peer);
    if let Some(k) = key.or(remembered) {
        t.unplaced.remove(&k);
    }
    // `D-035`: a seat this driver knows by its group key is
    // reported gone, on purpose or by timing out. `S1-DS`: by
    // the key remembered for the peer when the callback
    // could not read one.
    let key = key.or(remembered);
    // `S1-FB`: the seat still here by another entry it is known by.
    let mut seat_still = false;
    if let Some(app) = key.and_then(|k| t.known_as.get(&k).copied()) {
        // `S1-DU`: a seat back under a fresh key is still here
        // by that entry -- the one that timed out is gone, the
        // seat is not. A quit is the live process saying so,
        // and any other entry of that seat is the stale one.
        // The library has dropped the leaving peer before
        // this, so the scan holds the others.
        let still = !quit
            && entries_of(&app, scan_pairs(tox, t, g), &t.known_as)
                .iter()
                .any(|(p, _)| t.confirmed.contains(p));
        seat_still = still;
        if !still {
            if let Ok(mut present) = t.trouble.present.lock() {
                present.remove(&app);
            }
            if let Ok(mut gone) = t.trouble.gone.lock() {
                gone.push((app, quit));
            }
        }
    }
    // `S1-DV`: and by the line it came in over, taught or not.
    if let Some(l) = line {
        let still = !quit
            && t.peer_lines.iter().any(|(p, k)| *k == l && t.confirmed.contains(p));
        if !still {
            if let Ok(mut present) = t.trouble.present_lines.lock() {
                present.remove(&l);
            }
            if let Ok(mut gone) = t.trouble.gone_lines.lock() {
                gone.push((l, quit));
            }
            // `S1-FB`: and it is to be offered the group again the moment it
            // can take it, rather than when the next sweep happens to forget
            // that it was invited once already -- unless its seat is here by
            // a fresh entry. A restarted client's old entry times out a minute
            // after the kill, long after the new one is in, and its line is
            // not among the new entry's: that offer went to a member already in
            // the group (`run160249-2`, the third invitation at 120 s).
            if !seat_still && !t.offer_again.contains(&l) {
                t.offer_again.push(l);
            }
        }
    }
}

/// `S1-EB`, `S1-FE`: this client holds the table's group with nobody else
/// confirmed in it -- a copy every other member timed out of -- after members
/// had been in it (`S1-EK`), and is not leaving it.
///
/// **A founder's copy is joined from its creation.** The library says a self
/// join only when a sync response arrives with `time_connected` still zero, and
/// `gc_group_add` sets that at creation, so a founder never hears one and its
/// `self_joined` stays false: the first build of `S1-FE` read every founder's
/// copy as never joined and kept no member's invitation at all (`fe180653-3`).
/// A founder that has left its copy for a member's invitation (`own_chat`) is a
/// joiner again, and its join is read like one.
fn held_empty(t: &TableState) -> bool {
    let joined = t.self_joined || (matches!(t.setup.role, Role::Host) && t.own_chat.is_none());
    t.group.is_some() && joined && t.settled && t.confirmed.is_empty() && t.closing.is_none()
}

/// Whether an invitation from this friend could be for this table: a
/// joiner's founder, or for a founder coming back any member of its roster
/// (`D-037`); and only a table whose advertisement named a group, since an
/// invitation says nothing about which group it is for (see `Role`).
fn invitation_fits(t: &TableState, from: Option<[u8; 32]>) -> bool {
    // `S1-KL`: nothing is taken from a seat this table put out for good (`D-047`)
    // or cut off for flooding (`D-051`) -- not into the group it was put out of.
    if from.is_some_and(|k| t.barred_lines.contains(&k)) {
        return false;
    }
    match (&t.setup.role, from) {
        // `S1-KO`: and once the founder is out for good, any seat still in the
        // game -- the members take the founder's part in the group. `S1-KP`
        // (`D-103`): and while it is away from the table's hands by the node's
        // reading; or, at a copy held empty, once the founder was outside them at
        // the last hand this client held (`REFUTE_S1KP_v1_groups` G4) -- and only
        // from a seat this client's hand, or its record, counts as playing (G1).
        // `REFUTE_S1KP_impl_groups` I1: and with no copy at all -- the copy held
        // empty is left for the invitation one turn, and taken without it the next.
        (Role::Joiner { founder, chat_id: Some(_) }, Some(k)) => {
            k == *founder
                || ((founder_away(t) || (t.founder_outside && (held_empty(t) || t.group.is_none())))
                    && in_game_line(t, &k)
                    && counted_line(t, &k))
        }
        (Role::Back { chat_id: Some(_) }, Some(k)) => t.roster.contains(&k),
        // `S1-FE`: a founder, from a member of its roster -- only ever taken
        // into the group it created (`wanted_chat`), and only once its own copy
        // has held nobody else for `FOUNDER_YIELDS_AFTER`.
        (Role::Host, Some(k)) => t.roster.contains(&k),
        _ => false,
    }
}

/// `S1-KH`: whether an invitation is into the group `chat` names -- an
/// invitation's first bytes are the group's id.
fn invite_names(invite: &[u8], chat: Option<[u8; 32]>) -> bool {
    chat.is_some_and(|c| invite.get(..c.len()) == Some(&c[..]))
}

/// The group an invitation is for, if a table of this client wants one.
fn wanted_chat(t: &TableState) -> Option<[u8; 32]> {
    match &t.setup.role {
        Role::Joiner { chat_id, .. } | Role::Back { chat_id } => *chat_id,
        // `S1-FE`: the group this founder left to be taken back into it.
        Role::Host => t.own_chat,
    }
}

/// Take an invitation for whichever table it fits: one still without a
/// group, whose advertisement named one, and whose role names this friend.
/// It is accepted, the group's id read back and compared, and a mismatch is
/// left at once.
fn take_invitation(
    tox: &mut Tox,
    tables: &mut HashMap<TableId, TableState>,
    friends: &HashMap<u32, [u8; 32]>,
    friend: u32,
    invite: &[u8],
) {
    let from = friends.get(&friend).copied();
    let candidates: Vec<(TableId, [u8; 32])> = tables
        .iter()
        .filter(|(_, t)| t.group.is_none() && t.closing.is_none() && invitation_fits(t, from))
        .filter_map(|(id, t)| wanted_chat(t).map(|want| (*id, want)))
        .collect();
    let Some((first, _)) = candidates.first().copied() else {
        return;
    };
    let self_name = tables[&first].setup.self_name.clone();
    let Ok(joined) = tox.accept_invite(friend, invite, &self_name) else {
        return;
    };
    let got = tox.chat_id(joined).ok();
    let target = candidates
        .iter()
        .find(|(_, want)| Some(*want) == got)
        .map(|(id, _)| *id);
    match target.and_then(|id| tables.get_mut(&id)) {
        Some(t) => {
            t.group = Some(joined);
            t.accepted_at = Some(Instant::now());
            t.self_joined = false;
            // `D-051`: named with this seat's binding before the first handshake.
            t.named = None;
            if let Some(key) = t.setup.binder.as_ref() {
                if name_self(tox, key, joined) {
                    t.named = Some(joined);
                }
            }
            announce(tox, t.group, &t.chat);
            // **The harness's stall, once per process.** Sleeping here
            // starves the handshake past toxcore's twelve-second reaper
            // without touching the code under test.
            let stall = stall_join_secs();
            if stall > 0 && !t.stalled_once {
                t.stalled_once = true;
                std::thread::sleep(Duration::from_secs(stall));
            }
        }
        None => {
            let _ = tox.leave(joined);
        }
    }
}

fn invite_pending(
    tox: &mut Tox,
    t: &mut TableState,
    friends: &HashMap<u32, [u8; 32]>,
    connected: &std::collections::HashSet<u32>,
) {
    let Some(g) = t.group else { return };
    if founder_stopped_offering() {
        return;
    }
    if let Some((at, confirmed_then)) = t.last_invite {
        if seats_here(t) <= confirmed_then && at.elapsed() < INVITE_GAP {
            return;
        }
    }
    // `D-051`: never a seat this client cut off for flooding the group.
    let wanted: Vec<[u8; 32]> = t.roster.iter().copied().filter(|k| !t.barred_lines.contains(k)).collect();
    let Some(friend) = pending_invites(connected, friends, &wanted, &t.invited)
        .into_iter()
        .next()
    else {
        return;
    };
    if tox.invite(g, friend).is_ok() {
        t.trouble.invites_sent.fetch_add(1, Ordering::Relaxed);
        t.invited.push(friend);
        t.last_invite = Some((Instant::now(), seats_here(t)));
    } else {
        // Counted rather than logged: a refusal here is ordinary while the
        // group is settling, and the number is only interesting if it does
        // not stop growing.
        t.trouble.invites_refused.fetch_add(1, Ordering::Relaxed);
    }
}

/// `S1-KI`: how many OTHER seats the table's group holds -- counted by seat, not
/// by member.
///
/// A member's binding is its seat's signature over that member's key, and a
/// seat's player can sign as many as it likes: every group key a rogue binds
/// to its own seat is a confirmed member of its own, and `confirmed.len()`
/// read each as a seat. The group read whole while an honest seat was still
/// out of it -- hand 1 opened without it, and the offers of the group again
/// that only a short group makes (`S1-EG`, `S1-FB`, `D-037`) never went to it.
/// So a member counts as the seat its binding names, placed at this table,
/// and a seat counts once however many entries it holds. A member not placed
/// yet -- its name not read, the roster not said -- counts for nothing until
/// it is, and one never placed is cut off within `NAME_GRACE`. Without a
/// binder (the transport's own tests) there is no seat to count by, and every
/// confirmed member counts.
fn seats_here(t: &TableState) -> usize {
    if t.setup.binder.is_none() || count_by_member() {
        return t.confirmed.len();
    }
    seats_among(&t.confirmed, &t.peer_keys, &t.cut, &t.bound, &t.seat_apps)
}

/// `S1-KI`: `seats_here` on the maps alone: the seats of the table that the
/// confirmed members not cut off here are bound to, each once.
fn seats_among(
    confirmed: &std::collections::HashSet<u32>,
    peer_keys: &HashMap<u32, [u8; 32]>,
    cut: &std::collections::HashSet<[u8; 32]>,
    bound: &HashMap<[u8; 32], [u8; 32]>,
    seat_apps: &[[u8; 32]],
) -> usize {
    seats_present(confirmed, peer_keys, cut, bound, seat_apps).len()
}

/// `S1-KO`: which seats of the table the confirmed members not cut off here
/// are bound to, by application key.
fn seats_present(
    confirmed: &std::collections::HashSet<u32>,
    peer_keys: &HashMap<u32, [u8; 32]>,
    cut: &std::collections::HashSet<[u8; 32]>,
    bound: &HashMap<[u8; 32], [u8; 32]>,
    seat_apps: &[[u8; 32]],
) -> std::collections::HashSet<[u8; 32]> {
    peer_keys
        .iter()
        .filter(|(p, k)| confirmed.contains(*p) && !cut.contains(*k))
        .filter_map(|(_, k)| bound.get(k))
        .filter(|app| seat_apps.contains(*app))
        .copied()
        .collect()
}

/// `S1-KO`: whether this member's founder is out of the table for good -- put
/// out by the table's word (`D-047`, `D-051`, `D-084`), its line in
/// `out_lines`; a founder this client's own meter cut off is not -- so that the
/// members take the founder's part in the group: they offer it to a seat of the
/// game missing from it, and take such a seat's invitation into it.
fn founder_out(t: &TableState) -> bool {
    matches!(&t.setup.role, Role::Joiner { founder, .. } if t.out_lines.contains(founder))
}

/// `S1-KP` (`D-103`): whether this member's founder is away -- out for good by
/// the table's word (`founder_out`), or away from the table's hands by the
/// node's reading (`Command::FounderAway`): certified out of them with chips
/// for three hands or three minutes, or gone by its own signed word, until a
/// hand has it back. Either way the members take its part in the group; only
/// the first bars it.
fn founder_away(t: &TableState) -> bool {
    founder_out(t) || (matches!(&t.setup.role, Role::Joiner { .. }) && t.founder_away_by_hands)
}

/// `S1-KP` (`D-103`): whether `line` is this member's founder's while the founder
/// is away by the node's reading -- a line the members' own offers leave to
/// `S1-FE`'s (`REFUTE_S1KP_v2_groups` V6), `heads_up` reads without, and a copy
/// held empty orders above every member's (`REFUTE_S1KP_v1_groups` G3).
fn founder_line_away(t: &TableState, line: &[u8; 32]) -> bool {
    t.founder_away_by_hands && matches!(&t.setup.role, Role::Joiner { founder, .. } if founder == line)
}

/// `S1-KP` (`D-103`): the founder's application key while it is away by the
/// node's reading, read through the seats' lines.
fn founder_app_away(t: &TableState) -> Option<[u8; 32]> {
    match &t.setup.role {
        Role::Joiner { founder, .. } if t.founder_away_by_hands => {
            t.seat_lines.iter().find(|(_, l)| l == founder).map(|(a, _)| *a)
        }
        _ => None,
    }
}

/// `S1-KP` (`D-103`, `REFUTE_S1KP_v1_groups` G1): whether `line` is the line of a
/// seat this client's own hand -- or, back from a restart, its record -- counts
/// as playing (`in_game_present`). A member's invitation is taken from no other:
/// a copy cut off from the table, whose seats this client's last hand had
/// certified out, offers itself all the same.
fn counted_line(t: &TableState, line: &[u8; 32]) -> bool {
    t.seat_lines.iter().any(|(app, l)| l == line && t.in_game_present.contains(app))
}

/// `S1-KP` (`D-103`, `REFUTE_S1KP_v2_groups` V1, fix A): `seats_present`, less
/// the seats whose every confirmed entry here has been quiet for
/// `QUIET_MISSING_AFTER` -- a seat back from a restart, whose dead entry the
/// library times out only after 58 s, is offered the group from the copy the
/// table plays in at once. An offer to a seat that still holds a copy is
/// swallowed by that copy.
fn seats_heard(tox: &Tox, g: u32, t: &TableState) -> std::collections::HashSet<[u8; 32]> {
    t.peer_keys
        .iter()
        .filter(|(p, k)| t.confirmed.contains(*p) && !t.cut.contains(*k))
        .filter(|(_, k)| tox.peer_quiet_secs(g, k).is_none_or(|q| q < QUIET_MISSING_AFTER.as_secs()))
        .filter_map(|(_, k)| t.bound.get(k))
        .filter(|app| t.seat_apps.contains(*app))
        .copied()
        .collect()
}

/// `S1-KO`: whether `line` is the line of a seat still in the game -- the line
/// its join declared, of a seat this client's own hand counts in the game
/// (`in_game`), barred here neither by key nor by line, and on this table's
/// roster. The only lines a member whose founder is out offers the group to or
/// takes an invitation from: a seat put out while this client was away, which
/// its driver never barred, has no chips in any hand after it.
fn in_game_line(t: &TableState, line: &[u8; 32]) -> bool {
    !t.barred_lines.contains(line)
        && t.roster.contains(line)
        && t.seat_lines.iter().any(|(app, l)| l == line && t.in_game.contains(app) && !t.barred.contains(app))
}

/// `S1-KO`: whether the game is down to two seats, this client's own one of
/// them -- a client whose seat is out of the game (busted, staying to watch) is
/// at no game of two.
fn heads_up(in_game: &[[u8; 32]], mine: &[u8; 32]) -> bool {
    in_game.len() == 2 && in_game.contains(mine)
}

/// `S1-KO`: whether a member whose founder is out for good offers its copy of
/// the group to a seat of the game missing from it.
///
/// **A copy that holds another seat offers itself always.** No copy that holds
/// another seat is left for a member's invitation (and `patches/0035` delivers
/// none into it), so a seat that takes the offer lands where the others play.
///
/// **A copy that holds nobody else (`lone`) offers itself at once at a game of
/// two seats** (`heads_up`): the other seat is the only one to meet. **At a
/// larger game, only once it is `quiet_long`** -- alone and on the line for
/// `LONE_OFFERS_AFTER` with no other member's offer: a copy the other seats
/// play in reaches a missing seat sooner than that, and a lone copy that
/// offered itself first could be taken by a seat back from a restart and hold
/// it apart from them. Two lone copies that do offer each other meet in the
/// copy of the lower line (the `GroupInvite` arm, `LONE_YIELDS_AFTER`).
fn offers_its_copy(lone: bool, heads_up: bool, quiet_long: bool) -> bool {
    !lone || heads_up || quiet_long
}

/// `S1-KO`: whether this member, its founder out for good, offers its copy of
/// the group just now -- only while its own hand (not its session record) says
/// which seats are in the game and counts its own seat among them -- by
/// `offers_its_copy`.
fn member_offers(
    t: &TableState,
    mine: &[u8; 32],
    friends: &HashMap<u32, [u8; 32]>,
    connected: &std::collections::HashSet<u32>,
) -> bool {
    if !t.in_game_from_hand || !t.in_game.contains(mine) {
        return false;
    }
    let quiet_since = t.empty_since.zip(t.reachable_since).map(|(alone, on_line)| alone.max(on_line));
    // And every seat its hand counts as playing within reach: a copy the others
    // play in may be held among the seats it cannot reach yet -- two seats behind
    // one router back on the line first would otherwise meet in a copy of their
    // own and stay apart from it. A seat away (certified out of the hand) is no
    // such seat.
    let all_reached = t.in_game_present.iter().filter(|app| *app != mine).all(|app| {
        t.seat_lines
            .iter()
            .filter(|(a, _)| a == app)
            .any(|(_, line)| friend_number(friends, line).is_some_and(|n| connected.contains(&n)))
    });
    let quiet_long = t.waiting_on.is_empty()
        && quiet_since.is_some_and(|at| {
            at.elapsed() >= LONE_OFFERS_AFTER && (all_reached || at.elapsed() >= LONE_OFFERS_ANYWAY_AFTER)
        });
    // `S1-KP` (`D-103`, `REFUTE_S1KP_v1_groups` G5): a founder away from the hands
    // by the node's reading holds chips as a dead seat, and no game of two
    // counts it -- its copy is not one of the two to meet.
    let founder = founder_app_away(t);
    let in_game: Vec<[u8; 32]> = t.in_game.iter().copied().filter(|a| Some(*a) != founder).collect();
    offers_its_copy(held_empty(t), heads_up(&in_game, mine), quiet_long)
}

/// `S1-KO`: the lines to offer the group to -- of each seat still in the game
/// (`in_game`), this client's own excepted, that no confirmed member here is
/// bound to: by the line its join declared, still on this table's roster, and
/// never a seat or a line barred here. In the order the seats are listed.
fn missing_lines(
    seat_lines: &[([u8; 32], [u8; 32])],
    mine: &[u8; 32],
    in_game: &[[u8; 32]],
    present: &std::collections::HashSet<[u8; 32]>,
    barred: &std::collections::HashSet<[u8; 32]>,
    barred_lines: &std::collections::HashSet<[u8; 32]>,
    roster: &[[u8; 32]],
) -> Vec<[u8; 32]> {
    let mut out: Vec<[u8; 32]> = Vec::new();
    for (app, line) in seat_lines {
        if app == mine || !in_game.contains(app) || present.contains(app) || barred.contains(app) {
            continue;
        }
        if barred_lines.contains(line) || !roster.contains(line) || out.contains(line) {
            continue;
        }
        out.push(*line);
    }
    out
}

/// `S1-FE`: whether the table's group holds every other seat. `confirmed`
/// counts the OTHER seats in it (`seats_here`); `others` the roster's seats but
/// this client's own, which is how `TableState::roster` holds them. An empty
/// roster is never complete: the roster reaches the driver a turn behind the
/// node.
fn group_complete(confirmed: usize, others: usize) -> bool {
    others > 0 && confirmed >= others
}

/// `S1-FE`: whether another seat is missing from the table's group -- the
/// condition every offer of the group again is gated on.
fn group_short(confirmed: usize, others: usize) -> bool {
    confirmed < others
}

/// `S1-FR`: whether this client offers the table's group to the seats missing
/// from it. The founder always has; the founder back after a restart (`D-037`)
/// does too once it holds a copy that has confirmed a member -- the members'
/// copy it was taken into, and by then the only copy there may be. Before this
/// a founder back offered nobody anything, and a seat that restarted after it
/// waited to be invited by nobody: the owner's test (2026-09-15), *0 of 1 in*
/// for as long as both clients ran, the founder's copy holding the group alone
/// with the seat's friendship up. A member offers only its founder (`D-037`).
fn offers_the_group(role: &Role, in_group: bool, self_joined: bool, settled: bool) -> bool {
    match role {
        Role::Host => true,
        Role::Back { .. } => in_group && self_joined && settled,
        Role::Joiner { .. } => false,
    }
}

/// Who is owed an invitation: connected, on the roster, not invited yet.
///
/// Split out from [`invite_pending`] because it is the half that had the defect
/// and the half that can be tested without a Tox instance. Sorted so a caller
/// invites in a stable order — nothing depends on it, and a `HashSet`'s order
/// changing between runs is the kind of thing that makes a flake look like a
/// protocol problem.
fn pending_invites(
    connected: &std::collections::HashSet<u32>,
    friends: &HashMap<u32, [u8; 32]>,
    roster: &[[u8; 32]],
    invited: &[u32],
) -> Vec<u32> {
    // `D-042`: this table's roster and nobody else's -- the friendships are
    // the instance's, shared by every table this client sits at.
    let mut out: Vec<u32> = connected
        .iter()
        .copied()
        .filter(|f| friends.get(f).is_some_and(|k| roster.contains(k)) && !invited.contains(f))
        .collect();
    out.sort_unstable();
    out
}

/// Send what is queued, one message at a time.
///
/// **One at a time, and only while its fragments are being accepted.**
/// `tox_group_send_custom_packet` refuses when the group has no peers or its
/// send queue is full, and pushing the next message on top of a refusal would
/// interleave two half-sent ones — the reassembler at the far end would then be
/// holding two part-built messages from one sender, which it bounds, and the
/// older of them would be the one dropped.
fn flush(
    tox: &mut Tox,
    group: u32,
    pending: &mut Vec<Vec<u8>>,
    next_id: &mut u32,
    trouble: &Trouble,
) {
    trouble
        .waiting
        .store(pending.len() as u64, Ordering::Relaxed);
    while let Some(message) = pending.first() {
        let Ok(parts) = fragment::split(message, *next_id, fragment::TOX_PACKET) else {
            // Longer than the protocol builds. Dropped rather than retried for
            // ever, because it will not get shorter.
            pending.remove(0);
            continue;
        };
        let mut sent_all = true;
        for part in &parts {
            if let Err(e) = tox.send(group, part) {
                trouble.refused.fetch_add(1, Ordering::Relaxed);
                // Which refusal, not merely that there was one.
                let code = match e {
                    crate::tox::Failed::Api { error, .. } => error as usize,
                    _ => 0,
                };
                if let Some(c) = trouble.refused_why.get(code.min(5)) {
                    c.fetch_add(1, Ordering::Relaxed);
                }
                sent_all = false;
                break;
            }
            trouble.sent.fetch_add(1, Ordering::Relaxed);
        }
        if !sent_all {
            // **The whole message stays, and its fragments go again from the
            // start.** Half a message at the far end is a reassembly that never
            // completes and is swept; sending the rest under a new id would be
            // two half-messages instead of one. The duplicate fragments the far
            // end already has cost it a comparison each.
            trouble
                .waiting
                .store(pending.len() as u64, Ordering::Relaxed);
            return;
        }
        *next_id = next_id.wrapping_add(1);
        pending.remove(0);
    }
    trouble
        .waiting
        .store(pending.len() as u64, Ordering::Relaxed);
}

/// Publish the group's id to anybody waiting for it, once and only once.
fn announce(
    tox: &Tox,
    group: Option<u32>,
    chat: &tokio::sync::watch::Sender<Option<[u8; 32]>>,
) {
    if chat.borrow().is_some() {
        return;
    }
    if let Some(id) = group.and_then(|g| tox.chat_id(g).ok()) {
        // A closed channel means every waiter has gone, which is ordinary at
        // shutdown and is not worth reporting.
        let _ = chat.send(Some(id));
    }
}

/// The group entries a seat speaks from, as (peer number, group key) pairs.
///
/// **Two key spaces, and comparing across them was `S1-I`.** This used to test
/// `tox_group_peer_get_public_key` against the roster's key directly, and
/// `tox.h:3823` says that value is the peer's *group* public key — *"permanently
/// tied to a particular peer … the only way to reliably identify the same peer
/// across client restarts"* — a per-group identity, not the long-term friend key
/// the roster holds. The test could never be true, so `tox.kick` was never
/// reached and D-019's removal never once happened.
///
/// The bridge is `known_as`, built from **verified** traffic: the node loop
/// checks a message's signature, learns which application key signed it, and
/// pairs that with the group key the transport reported. Nothing here is
/// trusted — a wrong pairing can only fail to find a peer, and the roster
/// remains the only authority on who is seated.
///
/// **All of them, not the first (`S1-DU`).** One as a rule; two while the entry
/// a client that died left behind lingers beside the fresh one its restart
/// made: the library keeps a confirmed member for 58 s after its last packet,
/// and a restarted instance enters the group under a new key pair, since it
/// boots from its secret key alone and carries no group state. A reader that
/// took the first entry let the stale one answer for the live seat: read quiet
/// for 58 s, nudged for a message it could not send, and reported gone when the
/// library dropped it. Each reader chooses by its own evidence instead -- the
/// freshest entry for a nudge (`freshest`), any confirmed one for presence,
/// every one for a kick.
fn entries_of(
    app_key: &[u8; 32],
    pairs: impl IntoIterator<Item = (u32, [u8; 32])>,
    known_as: &HashMap<[u8; 32], [u8; 32]>,
) -> Vec<(u32, [u8; 32])> {
    pairs
        .into_iter()
        .filter(|(_, group_key)| known_as.get(group_key) == Some(app_key))
        .collect()
}

/// `S1-DB`: how long a seat's older entry has to have been silent -- beside a
/// fresh one that is a confirmed member under its own binding and speaks -- to
/// be read as a dead process's. The library has a living member say something
/// at least every twelve seconds (its ping), so fifteen is past anything a
/// living entry shows, and a quarter of the 58 s the library itself waits.
pub const STALE_TWIN_AFTER_S: u64 = 15;

/// One group key a seat is or was known by, as the sweep reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TwinEntry {
    key: [u8; 32],
    /// Paired by the binding in its own name (`D-051`), which only the holder
    /// of the seat's key can make -- not by a claim read off carried traffic.
    bound: bool,
    /// The group holds an entry for it right now.
    present: bool,
    confirmed: bool,
    /// Seconds since the group last heard from it.
    quiet: Option<u64>,
}

/// `S1-DB`: of one seat's group keys, the ones a dead process left behind.
///
/// A client that died and was started again enters the group under a new key
/// pair (`S1-DU`), and the library keeps the old entry for 58 s and then goes
/// on trying the dead key for ever -- ten handshakes, a minute's pause, ten
/// more (`run155832-2`: the old key tried again at 212 s and at 302 s of a run
/// whose seat had been back since 183 s). The library cannot know the two keys
/// are one player; this client can, by the binding each carries.
///
/// **The proof is the fresh entry, and nothing else is.** A key is stale only
/// beside another key of the same seat that is a confirmed member, **bound by
/// its own name**, and heard from within `STALE_TWIN_AFTER_S`: a binding is
/// the seat's signature over that member key, so only the seat's player can
/// have started the process behind it, and a profile runs one client. Beside
/// such a key, an older one is stale when it has been silent that long, or
/// when the library has already dropped it and would try it again.
///
/// **What this never does:** judge a seat that has one key -- an outage is not
/// a death, and the same process comes back under the same key; drop an entry
/// that still speaks -- two living clients on one identity are `D-068`'s
/// business and not this rule's; or act on a claim read off carried traffic,
/// which anybody can carry (`S1-DW`).
fn stale_twins(entries: &[TwinEntry]) -> Vec<[u8; 32]> {
    let fresh = entries
        .iter()
        .filter(|e| e.bound && e.confirmed && e.quiet.is_some_and(|q| q < STALE_TWIN_AFTER_S))
        .min_by_key(|e| e.quiet.unwrap_or(u64::MAX));
    let Some(fresh) = fresh else {
        return Vec::new();
    };
    entries
        .iter()
        .filter(|e| e.key != fresh.key)
        .filter(|e| match (e.present, e.confirmed, e.quiet) {
            // Gone from the group already: the key the library would try again.
            (false, _, _) => true,
            (true, true, Some(q)) => q >= STALE_TWIN_AFTER_S,
            // Still shaking hands, or a member with no reading: not judged.
            _ => false,
        })
        .map(|e| e.key)
        .collect()
}

/// `S1-DB`: drop what a seat's dead process left in the group, and forbid the
/// library its return. The **entry** goes and the **seat** stays: it is here
/// by its fresh entry, so nothing is said to the felt or the node about
/// anybody leaving -- which is why this does not go through `member_gone`.
fn drop_stale_twins(tox: &mut Tox, t: &mut TableState, g: u32) {
    // Only a seat known by two keys is looked at, which is no seat at all on
    // an ordinary sweep.
    let mut keys_of: HashMap<[u8; 32], Vec<[u8; 32]>> = HashMap::new();
    for (group_key, app) in t.known_as.iter() {
        keys_of.entry(*app).or_default().push(*group_key);
    }
    keys_of.retain(|_, keys| keys.len() > 1);
    if keys_of.is_empty() {
        return;
    }
    let pairs = scan_pairs(tox, t, g);
    for (app, keys) in keys_of {
        let entries: Vec<TwinEntry> = keys
            .iter()
            .map(|key| {
                let peer = pairs.iter().find(|(_, k)| k == key).map(|(p, _)| *p);
                TwinEntry {
                    key: *key,
                    bound: t.bound.get(key) == Some(&app),
                    present: peer.is_some(),
                    confirmed: peer.is_some_and(|p| t.confirmed.contains(&p)),
                    quiet: tox.peer_quiet_secs(g, key),
                }
            })
            .collect();
        for key in stale_twins(&entries) {
            let peer = pairs.iter().find(|(_, k)| *k == key).map(|(p, _)| *p);
            // For good: the process that held this key is gone and its secret
            // with it, so whatever brings the key back is not the seat.
            let was_member = tox.peer_drop(g, &key, true);
            if let Some(p) = peer {
                t.confirmed.remove(&p);
                // The peer id is given to the next member that joins.
                t.reassembler.forget(&p);
                t.peer_keys.remove(&p);
                t.peer_lines.remove(&p);
            }
            t.known_as.remove(&key);
            t.bound.remove(&key);
            t.unplaced.remove(&key);
            t.meters.remove(&key);
            t.trouble.stale_dropped.fetch_add(1, Ordering::Relaxed);
            #[cfg(feature = "fault-harness")]
            println!(
                "fault-harness: a seat is back under a fresh key, so the entry its dead process left is {} (S1-DB)",
                if was_member { "dropped from the table's group and not to come back" } else { "not to be tried again" }
            );
            #[cfg(not(feature = "fault-harness"))]
            let _ = was_member;
        }
    }
}

/// `S1-KI`: how many entries of one seat the group may hold at once: the two of
/// `S1-DB`, a dead process's and its fresh one's, until the first goes.
pub const ENTRIES_A_SEAT: usize = 2;

/// `S1-KI`: of one seat's entries, the ones past the `ENTRIES_A_SEAT` the group
/// has heard from most recently.
///
/// **Only a confirmed entry with a reading is judged**, and every such entry
/// but the freshest two goes. For an honest seat this is never its living
/// process: a profile runs one client (`S1-FV`), so the seat's processes came
/// one after another, the living one joined after every other died, and it
/// has been heard since -- every dead entry has been silent longer. A member
/// still shaking hands, or one the group has heard nothing from, is not judged
/// (`stale_twins` says the same). Ties go by key, so the choice is the same
/// on every sweep.
fn surplus_entries(entries: &[TwinEntry]) -> Vec<[u8; 32]> {
    let mut heard: Vec<(u64, [u8; 32])> = entries
        .iter()
        .filter(|e| e.present && e.confirmed)
        .filter_map(|e| e.quiet.map(|q| (q, e.key)))
        .collect();
    heard.sort_unstable();
    heard.into_iter().skip(ENTRIES_A_SEAT).map(|(_, key)| key).collect()
}

/// `S1-KI`: cut off every entry a seat holds past `ENTRIES_A_SEAT`.
///
/// A seat's player can bind as many group keys to its seat as it likes and
/// bring each into the group itself -- any member of a private group can
/// invite. Counted by seat (`seats_here`) and metered by seat (`metered_as`)
/// they are no seat and no allowance; this is what keeps them from filling the
/// group: at the library's hundred members every join is refused, and a seat
/// that dropped could not come back. The seat is not barred -- the entries go,
/// not the player -- and where this client is the founder they are kicked from
/// the group for everybody.
fn cut_surplus(tox: &mut Tox, t: &mut TableState, g: u32) {
    if count_by_member() {
        return;
    }
    let mut keys_of: HashMap<[u8; 32], Vec<[u8; 32]>> = HashMap::new();
    for (p, k) in t.peer_keys.iter() {
        if !t.confirmed.contains(p) || t.cut.contains(k) {
            continue;
        }
        if let Some(app) = t.bound.get(k) {
            keys_of.entry(*app).or_default().push(*k);
        }
    }
    // One key a seat entry: the library can hold a key at two peer numbers
    // for a moment (a dead entry's key coming back, `S1-DB`).
    for keys in keys_of.values_mut() {
        keys.sort_unstable();
        keys.dedup();
    }
    keys_of.retain(|_, keys| keys.len() > ENTRIES_A_SEAT);
    for keys in keys_of.into_values() {
        let entries: Vec<TwinEntry> = keys
            .iter()
            .map(|key| TwinEntry {
                key: *key,
                bound: true,
                present: true,
                confirmed: true,
                quiet: tox.peer_quiet_secs(g, key),
            })
            .collect();
        for key in surplus_entries(&entries) {
            cut_off(tox, t, g, key, Cut::Surplus);
        }
    }
}

/// `S1-DW`: whether a claim that `group_key` speaks for `app_key` is refused:
/// another group key holds that application key, is a confirmed member, and
/// has spoken within `quiet_limit` seconds. A seat back under a fresh key
/// (S1-DU) passes, its old entry having been quiet; a member saying another
/// seat's message again as its own first word does not, that seat being here
/// and speaking. No member can make another read as gone by it.
fn claim_refused(
    group_key: &[u8; 32],
    app_key: &[u8; 32],
    entries: impl IntoIterator<Item = ([u8; 32], [u8; 32], bool, Option<u64>)>,
    quiet_limit: u64,
) -> bool {
    entries.into_iter().any(|(gk, a, confirmed, quiet)| {
        a == *app_key && gk != *group_key && confirmed && quiet.is_some_and(|q| q < quiet_limit)
    })
}

/// The group's members as (peer number, group key) pairs, read from the
/// library. Peer ids are small and dense and a table is at most ten seats, so
/// a scan beats keeping a second map in step with joins and parts -- except
/// per nudge, where `TableState::peer_keys` is the cheap copy (see `Nudge`).
///
/// `S1-KI`: **and every member this client has seen past the scan**, each checked
/// against the library. Ids are handed out lowest first, so a group a rogue
/// filled with entries of its own put later members at 64 and above, where
/// the scan stopped: a seat there read as not in the group at all.
fn scan_pairs(tox: &Tox, t: &TableState, group: u32) -> Vec<(u32, [u8; 32])> {
    let mut pairs: Vec<(u32, [u8; 32])> = (0..Tox::PEER_SCAN)
        .filter_map(|p| tox.peer_key(group, p).ok().map(|k| (p, k)))
        .collect();
    pairs.extend(
        t.peer_keys
            .iter()
            .filter(|(p, k)| **p >= Tox::PEER_SCAN && tox.peer_key(group, **p).ok().as_ref() == Some(*k))
            .map(|(p, k)| (*p, *k)),
    );
    pairs
}

/// `S1-KI`: the peer number the group holds this group key at, if any -- a scan,
/// then the members seen past it (see `scan_pairs`).
fn peer_of(tox: &Tox, t: &TableState, group: u32, group_key: &[u8; 32]) -> Option<u32> {
    (0..Tox::PEER_SCAN)
        .find(|p| tox.peer_key(group, *p).ok().as_ref() == Some(group_key))
        .or_else(|| {
            t.peer_keys
                .iter()
                .find(|(p, k)| *k == group_key && tox.peer_key(group, **p).ok().as_ref() == Some(group_key))
                .map(|(p, _)| *p)
        })
}

/// `S1-DU`: of a seat's entries, the group key the group heard from most
/// recently. An entry with no reading (`None`) counts as never heard from, so
/// one that has spoken wins over one that has not.
fn freshest(
    entries: &[(u32, [u8; 32])],
    quiet_of: impl Fn(&[u8; 32]) -> Option<u64>,
) -> Option<[u8; 32]> {
    entries
        .iter()
        .map(|(_, group_key)| *group_key)
        .min_by_key(|group_key| quiet_of(group_key).unwrap_or(u64::MAX))
}

/// Milliseconds for the reassembler's timers and the flood meters.
///
/// **Monotonic** (`S1-JB`): it only ever measures a difference between two
/// readings taken here, and nothing in the protocol depends on it --
/// `PROTOCOL.md` §8.2's rule about deadlines being local. On the wall clock, as
/// it was, a system time set ahead swept every message half received, and one
/// set back kept a flood meter's minute counting for as long as it was set back.
fn millis() -> u64 {
    crate::clock::mono_ms()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The harness's outage window, which the table's line and -- since
    /// `S1-IB`/`S1-ID`'s re-test -- the lobby's transport are both cut by: from
    /// its start, for its length, once or again every so often, and never
    /// without a window.
    #[test]
    fn the_outage_window_covers_its_span_once_or_every_period() {
        let s = Duration::from_secs;
        let once = Some((s(100), s(30), s(0)));
        assert!(!offline_in(once, s(99)), "before it");
        assert!(offline_in(once, s(100)) && offline_in(once, s(129)), "through it");
        assert!(!offline_in(once, s(130)) && !offline_in(once, s(500)), "and once only");
        let every = Some((s(100), s(30), s(200)));
        assert!(offline_in(every, s(310)) && !offline_in(every, s(340)), "again every period, for as long");
        assert!(!offline_in(None, s(110)), "no window, no outage");
    }

    /// `S1-KL`: a seat the table's word removed for good (`D-047`) never comes
    /// back into the table's group from here, founder or not: its line is barred
    /// for the table's life whether or not a group is held, no roster said again
    /// takes it back, nobody offers it the group -- a member its founder, the
    /// founder its seats -- and no invitation of its is taken. Measured
    /// (2026-09-30, the owner's table of three): a founder put out after its
    /// fourth absence was offered the group by both other seats every
    /// `REINVITE_EVERY`, came in under a fresh key and was cut off again, eleven
    /// times in fifteen minutes.
    #[test]
    fn a_seat_removed_for_good_is_never_offered_the_group_again() {
        let src = include_str!("table.rs");
        let code = &src[..src.find("\n#[cfg(test)]\nmod tests {").expect("the tests")];
        let code = code.split_whitespace().collect::<Vec<_>>().join(" ");
        let remove = code.find("Command::Remove { app_key, tox_key, for_good } => {").expect("the word's removal");
        let barred = remove
            + code[remove..]
                .find("if for_good { if let Some(k) = tox_key { t.roster.retain(|r| *r != k); t.barred_lines.insert(k);")
                .expect("the line barred");
        let group = remove + code[remove..].find("if let Some(g) = t.group {").expect("the group's entries");
        let group_end = group + code[group..].find("// `D-047`: for good is for good").expect("its end");
        assert!(barred > group_end, "barred outside the group's own block: whether or not a group is held");
        assert!(
            code.contains("match t.group { Some(g) => bar_seat(&mut tox, t, g, a), None => { t.barred.insert(a); } }"),
            "and its application key barred all the same"
        );
        let roster = code.find("Command::Roster(keys) => {").expect("the roster said again");
        let skip = roster + code[roster..].find("if t.barred_lines.contains(&key) { continue; }").expect("a barred line skipped");
        let push = roster + code[roster..].find("t.roster.push(key);").expect("the roster taken");
        assert!(skip < push, "before it is taken back in");
        assert!(code.contains("Command::Seated(key) if t.barred_lines.contains(&key) => {}"), "not seated again");
        assert!(code.contains("Command::Rejoined(key) if t.barred_lines.contains(&key) => {}"), "not offered the group on asking back");
        assert!(
            code.contains("if t.self_joined && t.roster.contains(founder) && !t.barred_lines.contains(founder) {"),
            "a member's offer to its founder, on the sweep"
        );
        assert!(
            code.contains("if t.self_joined && friends.get(&friend) == Some(founder) && !t.barred_lines.contains(founder) && !lone_and_away {"),
            "and on the founder's friendship coming up"
        );
        assert!(
            code.contains("let wanted: Vec<[u8; 32]> = t.roster.iter().copied().filter(|k| !t.barred_lines.contains(k)).collect();"),
            "the founder's own invitations"
        );
        let fits = code.find("fn invitation_fits(t: &TableState, from: Option<[u8; 32]>) -> bool {").expect("an invitation's fit");
        let refused = fits
            + code[fits..]
                .find("if from.is_some_and(|k| t.barred_lines.contains(&k)) { return false; }")
                .expect("an invitation from a barred line refused");
        let by_role = fits + code[fits..].find("match (&t.setup.role, from) {").expect("the roles' fits");
        assert!(refused < by_role, "and no invitation of its taken, whatever the role");
        // Its friendship idles out when the table closes, and no table needs it
        // as a founder any more.
        assert!(
            code.contains("|| matches!(&self.setup.role, Role::Joiner { founder, .. } if founder == key && !self.barred_lines.contains(key))"),
            "a founder removed for good is needed by nobody"
        );
        assert!(code.contains("keys.extend(t.barred_lines.iter().copied());"), "idled with the table's others");
        // The node names a seat's line only when no other seat names it: a line
        // is declared, and a rogue declaring an honest seat's would have it barred.
        let run_src = include_str!("../net/run.rs");
        let run_code = &run_src[..run_src.find("\n#[cfg(test)]\nmod tests {").expect("the tests")];
        let run_code = run_code.split_whitespace().collect::<Vec<_>>().join(" ");
        // `S1-KR`: the fourth, a seat a cheat certificate put out.
        assert_eq!(run_code.matches(".map(|e| (e.app_public_key, own_line(f, e.seat)))").count(), 4, "every removal by its own line");
        assert_eq!(run_code.matches(".map(|e| (e.app_public_key, e.tox_key))").count(), 0, "and never by a declared line as such");
        assert!(
            run_code.contains("let shared = seats.iter().any(|e| e.seat != seat && e.tox_key == Some(line)); (!shared).then_some(line)"),
            "a line another seat names is nobody's"
        );
    }

    /// `S1-KO`: once a member's founder is out of the table for good -- by the
    /// table's word, not a cut of this client's own -- the members take the
    /// founder's part in the group. A seat of the game missing from it is offered
    /// it over its own line, by seat through the bindings: never this client's
    /// own seat, a seat no longer in the game, a seat or a line barred here, a
    /// line off the roster. A copy that holds another seat offers itself always;
    /// a lone copy at once at a game of two, at a larger one only once alone, on
    /// the line and offered nothing for `LONE_OFFERS_AFTER`; and only a member
    /// whose own hand counts its seat in the game. Any seat of the game's
    /// invitation fits; a lone copy is left at once for a lower line's, and a
    /// higher line's first offer is answered with one offer of the lone copy and
    /// taken only when it comes again `LONE_YIELDS_AFTER` after that answer;
    /// nothing is kept for later.
    #[test]
    fn a_member_whose_founder_is_out_offers_the_group_to_a_missing_seat() {
        let app = |n: u8| [n; 32];
        let line = |n: u8| [100 + n; 32];
        // Seat 0 the founder, out for good; this client seat 1; seats 2..4.
        let mut lines: Vec<([u8; 32], [u8; 32])> = (0..=4).map(|n| (app(n), line(n))).collect();
        let roster: Vec<[u8; 32]> = [2u8, 3, 4].iter().map(|n| line(*n)).collect();
        let in_game: Vec<[u8; 32]> = (1..=4).map(app).collect();
        let barred: std::collections::HashSet<[u8; 32]> = [app(0)].into_iter().collect();
        let barred_lines: std::collections::HashSet<[u8; 32]> = [line(0)].into_iter().collect();
        let present: std::collections::HashSet<[u8; 32]> = [app(2)].into_iter().collect();
        assert_eq!(
            missing_lines(&lines, &app(1), &in_game, &present, &barred, &barred_lines, &roster),
            vec![line(3), line(4)],
            "the seats of the game missing from the group, by their lines, in seat order"
        );
        // A seat no longer in the game -- put out while this client was away, its
        // line never barred here -- is offered nothing.
        let without_four: Vec<[u8; 32]> = (1..=3).map(app).collect();
        assert_eq!(
            missing_lines(&lines, &app(1), &without_four, &present, &barred, &barred_lines, &roster),
            vec![line(3)]
        );
        // A seat declaring the founder's line, one declaring a line off the
        // roster, and a line declared twice: none offered, or offered once.
        lines.push((app(5), line(0)));
        lines.push((app(6), [9u8; 32]));
        lines.push((app(7), line(3)));
        let all_in: Vec<[u8; 32]> = (1..=7).map(app).collect();
        assert_eq!(
            missing_lines(&lines, &app(1), &all_in, &present, &barred, &barred_lines, &roster),
            vec![line(3), line(4)]
        );
        let everybody: std::collections::HashSet<[u8; 32]> = (0..=7).map(app).collect();
        assert!(
            missing_lines(&lines, &app(1), &all_in, &everybody, &barred, &barred_lines, &roster).is_empty(),
            "nobody missing"
        );
        // A copy holding another seat offers itself always; a lone copy at once at
        // a game of two, at a larger game only once alone and quiet long enough.
        assert!(offers_its_copy(false, false, false) && offers_its_copy(false, true, true));
        assert!(offers_its_copy(true, true, false), "a lone copy at a game of two: the other is the only seat to meet");
        assert!(!offers_its_copy(true, false, false), "a lone copy at a larger game waits for the others' offer");
        assert!(offers_its_copy(true, false, true), "and offers itself once none has come");
        assert!(heads_up(&[app(1), app(2)], &app(1)) && !heads_up(&in_game, &app(1)));
        assert!(!heads_up(&[app(2), app(3)], &app(1)), "a seat out of the game, watching, is at no game of two");

        let src = include_str!("table.rs");
        let code = &src[..src.find("\n#[cfg(test)]\nmod tests {").expect("the tests")];
        let code = code.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            code.contains("matches!(&t.setup.role, Role::Joiner { founder, .. } if t.out_lines.contains(founder))"),
            "a founder out by the table's word"
        );
        assert_eq!(code.matches("t.out_lines.insert(").count(), 1, "and by nothing else");
        // `S1-KP` (`D-103`): or away from the hands by the node's reading -- and from
        // a seat counted as playing only.
        assert!(
            code.contains("Role::Joiner { founder, chat_id: Some(_) }, Some(k)) => { k == *founder || ((founder_away(t) || (t.founder_outside && (held_empty(t) || t.group.is_none()))) && in_game_line(t, &k) && counted_line(t, &k)) }"),
            "a seat of the game's invitation, once the founder is out or away"
        );
        let sweep = code.find("fn sweep_table(").expect("the sweep");
        let offers = sweep + code[sweep..].find("if let (true, Some(g)) = (founder_away(t), t.group) {").expect("a member's offer");
        assert!(code[offers..].contains("let offers = member_offers(t, &mine, friends, connected);"), "by the rule");
        assert!(
            code[offers..]
                .contains("let due = missing_lines(&t.seat_lines, &mine, &t.in_game, &present, &t.barred, &t.barred_lines, &t.roster)"),
            "to a missing seat of the game, by its line"
        );
        assert!(
            code.contains("let line = friends .get(&friend) .copied() .filter(|k| in_game_line(t, k) && !t.waiting_on.contains_key(k) && !founder_line_away(t, k));"),
            "and on a seat of the game's friendship coming up -- not one this lone copy waits on, nor the founder away"
        );
        assert!(code.contains("let offers = due && member_offers(t, &mine, &friends, &connected);"), "throttled, by the same rule");
        assert!(
            code.contains(".filter(|line| offers && !t.waiting_on.contains_key(line) && !founder_line_away(t, line))"),
            "the sweep, the same"
        );
        // A member offers only by its own hand, its own seat in the game, and a
        // lone copy at a larger game only once alone, on the line, unoffered for
        // `LONE_OFFERS_AFTER` and within reach of every seat its hand counts as
        // playing.
        let rule = code.find("fn member_offers(").expect("the rule");
        let body = &code[rule..rule + code[rule..].find("offers_its_copy(held_empty(t), heads_up(&in_game, mine), quiet_long)").expect("its end")];
        assert!(body.contains("if !t.in_game_from_hand || !t.in_game.contains(mine) { return false; }"));
        assert!(body.contains("let all_reached = t.in_game_present.iter().filter(|app| *app != mine).all(|app| {"));
        assert!(body.contains(
            "let quiet_long = t.waiting_on.is_empty() && quiet_since.is_some_and(|at| { at.elapsed() >= LONE_OFFERS_AFTER && (all_reached || at.elapsed() >= LONE_OFFERS_ANYWAY_AFTER) });"
        ));
        // A higher line's later offer is taken only past two friend timeouts.
        assert!(LONE_YIELDS_AFTER > Duration::from_secs(64) && OFFER_FORGOTTEN_AFTER > Duration::from_secs(32));
        // And a member's offers to one missing seat, one seat a sweep in turn, come
        // round before they are forgotten at a full table, and before a lone copy
        // that has had none offers itself.
        assert!(OFFER_FORGOTTEN_AFTER > SWEEP_EVERY * 10 && LONE_OFFERS_AFTER > SWEEP_EVERY * 10);
        assert!(
            code.contains(".min_by_key(|(line, _)| t.seat_offered.get(line).copied());"),
            "the seat offered longest ago, not the first in seat order"
        );
        // A lone copy leaves at once for a lower line; a higher line's first offer
        // it answers with one of its own and does not take, and takes the one that
        // comes `LONE_YIELDS_AFTER` after its answer. Nothing is kept for later.
        let invite = code.find("Event::GroupInvite { friend, invite } => {").expect("an invitation");
        let named = invite + code[invite..].find("if !invite_names(&invite, wanted_chat(t)) { continue; }").expect("its group");
        let higher = invite
            + code[invite..]
                .find("if let (true, Some(k), Some(g)) = (founder_away(t), from.filter(|k| *k > me || founder_line_away(t, k)), t.group) {")
                .expect("a higher line's offer -- the founder away ordered above all");
        let waited = higher
            + code[higher..]
                .find("Some((answered, last)) => { *last = now; if answered.elapsed() < LONE_YIELDS_AFTER { continue; } }")
                .expect("taken only after this copy's own answer");
        let answered = higher
            + code[higher..]
                .find("if t.in_game_from_hand && !founder_line_away(t, &k) && tox.invite(g, friend).is_ok() {")
                .expect("the first answered with one offer of this copy -- by a seat in the game, never to the founder away");
        let left = invite + code[invite..].find("if let Some(g) = t.group.take() {").expect("the copy left");
        assert!(named < higher && waited < left && answered < left, "before any copy is left for it");
        assert_eq!(code.matches("t.held_offer = Some(").count(), 2, "an offer kept by a founder alone (S1-FE)");
        // Who offered a lone copy is forgotten once its offers pause, or the copy
        // holds somebody again.
        assert!(code.contains("t.waiting_on.retain(|_, (_, last)| last.elapsed() < OFFER_FORGOTTEN_AFTER);"));
        assert!(code.contains("} else { t.empty_since = None; t.waiting_on.clear(); }"));
    }

    /// `S1-KP` (`D-103`): a founder away from the table's hands by the node's
    /// reading -- certified out of them with chips for three hands or three
    /// minutes, or gone by its own word -- is a founder whose part the members
    /// take, as one out for good, and it is barred by nothing: no line of its is
    /// barred, its invitation is still taken (ordered above every member's at a
    /// copy held empty), and `S1-FE` still offers it the group from a copy holding
    /// seats. A seat whose entries here are all quiet counts as missing, so the
    /// table's copy reaches one back from a restart before its dead entry times
    /// out; and a second invitation in one turn for a table that left its copy
    /// waits a turn, as the first did.
    #[test]
    fn a_founder_away_from_the_hands_has_its_part_taken_and_is_barred_by_nothing() {
        let src = include_str!("table.rs");
        let code = &src[..src.find("\n#[cfg(test)]\nmod tests {").expect("the tests")];
        let code = code.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(code.contains(
            "fn founder_away(t: &TableState) -> bool { founder_out(t) || (matches!(&t.setup.role, Role::Joiner { .. }) && t.founder_away_by_hands) }"
        ));
        assert!(code.contains("Command::FounderAway { away, outside } => { t.founder_away_by_hands = away; t.founder_outside = outside || away; }"));
        assert_eq!(code.matches("t.founder_away_by_hands = ").count(), 1, "set by the node's word alone");
        assert!(!code.contains("t.out_lines.insert(founder") && !code.contains("barred_lines.insert(founder"), "nothing barred by it");
        // The members' sweep counts quiet seats missing, and leaves the founder to S1-FE.
        let sweep = code.find("fn sweep_table(").expect("the sweep");
        assert!(code[sweep..].contains("let present = seats_heard(tox, g, t);"));
        assert!(code.contains(".filter(|(_, k)| tox.peer_quiet_secs(g, k).is_none_or(|q| q < QUIET_MISSING_AFTER.as_secs()))"));
        assert!(QUIET_MISSING_AFTER > AWAY_RESAY && QUIET_MISSING_AFTER < Duration::from_secs(58), "past a live member's gap, inside the library's timeout");
        // A copy held empty offers the founder away nothing, answers it nothing.
        assert_eq!(code.matches("let lone_and_away = t.founder_away_by_hands && held_empty(t);").count(), 2, "the sweep's offer and the up-edge's");
        assert!(code.contains("if let (Role::Joiner { founder, .. }, Some(g), false) = (&t.setup.role, t.group, lone_and_away) {"));
        // No game of two counts the founder away.
        assert!(code.contains("let founder = founder_app_away(t); let in_game: Vec<[u8; 32]> = t.in_game.iter().copied().filter(|a| Some(*a) != founder).collect();"));
        // A second invitation in one turn waits a turn.
        assert!(code.contains("left_one = true; left_this_turn.insert(*id);"));
        assert!(code.contains("if left_one || deferred_table { invites_deferred.push((friend, invite)); } else { take_invitation(&mut tox, &mut tables, &friends, friend, &invite); }"));
    }

    /// `S1-KL`: the node's own count of this client's returns belongs to one
    /// table and one game, noted on the tick and at the boundary.
    #[test]
    fn this_clients_own_returns_are_counted_at_one_table_and_one_game() {
        let src = include_str!("../net/run.rs");
        let code = &src[..src.find("\n#[cfg(test)]\nmod tests {").expect("the tests")];
        let code = code.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(code.matches("note_own_returns(t);").count(), 2, "on the tick and at the boundary");
        let note = code.find("fn note_own_returns(t: &mut TableRun) {").expect("the count");
        let reset = note
            + code[note..]
                .find("let of = t.table.as_ref().map(|f| (f.table_id(), f.session())); if of != t.own_returns_of { t.own_returns.clear(); t.own_returns_of = of; }")
                .expect("started again at another table or game");
        let hands = note + code[note..].find("for h in [t.previous.as_ref(), t.hand.as_ref()].into_iter().flatten() {").expect("the hands held");
        assert!(reset < hands, "before any hand is read");
    }

    /// `S1-FR`: the founder offers the group always; the founder back after a
    /// restart once it is in a copy that has confirmed a member; a member never
    /// offers it to anybody but its founder (which is not this question).
    #[test]
    fn a_founder_back_in_the_group_offers_it_and_a_member_does_not() {
        let back = Role::Back { chat_id: Some([1u8; 32]) };
        let member = Role::Joiner { founder: [2u8; 32], chat_id: Some([1u8; 32]) };
        assert!(offers_the_group(&Role::Host, false, false, false));
        assert!(offers_the_group(&back, true, true, true), "back in a settled copy: it offers");
        assert!(!offers_the_group(&back, false, false, false), "not in the group yet: it waits to be offered it");
        assert!(!offers_the_group(&back, true, true, false), "a copy still settling");
        assert!(!offers_the_group(&back, true, false, true), "a join not finished");
        assert!(!offers_the_group(&member, true, true, true));
    }

    /// `S1-KH`: a copy held empty is left for an invitation into the group
    /// its table's advertisement named, and for no other.
    #[test]
    fn an_invitation_names_the_group_it_is_into() {
        let chat = [7u8; 32];
        let mut invite = chat.to_vec();
        invite.extend_from_slice(&[1, 2, 3]);
        assert!(invite_names(&invite, Some(chat)));
        assert!(!invite_names(&invite, Some([8u8; 32])), "another group");
        assert!(!invite_names(&invite[..10], Some(chat)), "too short to name one");
        assert!(!invite_names(&invite, None), "no group advertised");
    }

    /// `S1-FE`: the group's counts are of OTHER seats, as the roster the driver
    /// holds is. The formula before this counted this client in as well, so a
    /// heads-up group with nobody else in it read complete and never short, and
    /// a three-seat group with one seat missing the same.
    #[test]
    fn a_group_is_complete_with_every_other_seat_and_short_without_one() {
        // Heads-up: one other seat.
        assert!(!group_complete(0, 1), "nobody else in the group is not a complete heads-up group");
        assert!(group_short(0, 1), "and it is short, so the group is offered again");
        assert!(group_complete(1, 1));
        assert!(!group_short(1, 1));
        // Three seats: two others, one of them missing.
        assert!(!group_complete(1, 2), "a seat still missing");
        assert!(group_short(1, 2));
        assert!(group_complete(2, 2));
        assert!(!group_short(2, 2));
        // The roster a turn behind the node: never complete, nothing to offer.
        assert!(!group_complete(0, 0));
        assert!(!group_short(0, 0));
    }

    /// An invitation is owed by a **condition**, not by an event.
    ///
    /// The defect this pins: the founder invited a seat only from the
    /// `FriendConnection` up-edge, and `invited` is appended to only when
    /// `tox_group_invite_friend` succeeds. A refusal therefore lost the seat
    /// until the connection dropped and came back — and measured once at three
    /// seats, that took **a hundred seconds**, during which the seat opened
    /// hands on a genesis nobody else held, played none of them, and still
    /// finished by printing `TABLE FORMED seats=3`.
    ///
    /// Four cases, and the third is the one that was wrong.
    #[test]
    fn an_invitation_is_owed_by_a_condition_and_not_by_an_edge() {
        let friends: HashMap<u32, [u8; 32]> = [(1u32, [1u8; 32]), (2, [2u8; 32]), (3, [3u8; 32])]
            .into_iter()
            .collect();
        let set = |xs: &[u32]| xs.iter().copied().collect::<std::collections::HashSet<u32>>();
        let everyone: Vec<[u8; 32]> = friends.values().copied().collect();

        // Connected, on the roster, not yet invited: owed.
        assert_eq!(pending_invites(&set(&[1, 2]), &friends, &everyone, &[]), vec![1, 2]);

        // Already invited: not owed again. A second invitation is not harmful,
        // but sending one every five seconds for the life of a table is.
        assert_eq!(pending_invites(&set(&[1, 2]), &friends, &everyone, &[1]), vec![2]);

        // **The case the edge got wrong.** The friend is connected and the
        // invitation was refused, so it is not in `invited` — and there will be
        // no second up-edge. The sweep must still owe it.
        assert_eq!(pending_invites(&set(&[3]), &friends, &everyone, &[1, 2]), vec![3]);

        // Connected but not a seat at this table: never owed. The founder
        // invites the roster, not everyone toxcore has a friendship with.
        assert_eq!(pending_invites(&set(&[9]), &friends, &everyone, &[]), Vec::<u32>::new());

        // Not connected: nothing to invite over, which is the case the edge
        // handled correctly and this must not change.
        assert_eq!(pending_invites(&set(&[]), &friends, &everyone, &[]), Vec::<u32>::new());

        // `D-042`: a friend of the instance that is not on THIS table's
        // roster is another table's, and never owed this table's group.
        assert_eq!(
            pending_invites(&set(&[1, 2, 3]), &friends, &[[2u8; 32]], &[]),
            vec![2]
        );
    }

    /// `S1-DB`: beside a fresh entry that is bound, confirmed and speaking, an
    /// older key of the same seat is a dead process's once it has been silent
    /// for `STALE_TWIN_AFTER_S`, or once the library has dropped it; and nothing
    /// else is ever judged.
    #[test]
    fn a_seats_dead_entry_goes_only_beside_a_fresh_one_that_proves_it() {
        let e = |key: u8, bound, present, confirmed, quiet| TwinEntry { key: [key; 32], bound, present, confirmed, quiet };
        let fresh = e(2, true, true, true, Some(1));
        // The return twenty seconds after the death: the old entry goes.
        assert_eq!(stale_twins(&[e(1, true, true, true, Some(20)), fresh]), vec![[1u8; 32]]);
        // A quick restart: the old entry has not been silent long enough yet.
        assert!(stale_twins(&[e(1, true, true, true, Some(5)), fresh]).is_empty());
        // The library dropped the old one already: forbidden to come back.
        assert_eq!(stale_twins(&[e(1, true, false, false, None), fresh]), vec![[1u8; 32]]);
        // Two living clients on one identity both speak: neither is judged.
        assert!(stale_twins(&[e(1, true, true, true, Some(3)), fresh]).is_empty());
        // One key, silent for a minute: an outage, not a death.
        assert!(stale_twins(&[e(1, true, true, true, Some(60))]).is_empty());
        // A fresh key paired by a claim read off carried traffic proves nothing.
        assert!(stale_twins(&[e(1, true, true, true, Some(40)), e(2, false, true, true, Some(1))]).is_empty());
        // Nor does a bound one that is not a confirmed member, or is silent itself.
        assert!(stale_twins(&[e(1, true, true, true, Some(40)), e(2, true, true, false, Some(1))]).is_empty());
        assert!(stale_twins(&[e(1, true, true, true, Some(40)), e(2, true, true, true, Some(30))]).is_empty());
        // An entry still shaking hands is never the stale one.
        assert!(stale_twins(&[e(3, true, true, false, None), fresh]).is_empty());
        // Of two that speak, the freshest is the proof and the other is left.
        assert!(stale_twins(&[e(1, true, true, true, Some(9)), fresh]).is_empty());
    }

    /// `S1-KI`: the group is counted by seat. A rogue's own entries, each bound
    /// to its seat by its own binding, are one seat -- the group holds two of
    /// three other seats however many of them it brought in, and stays short
    /// until the missing seat is there. A member not placed at a seat, one
    /// cut off here, and one still shaking hands count for nothing.
    #[test]
    fn a_group_is_counted_by_seat_however_many_entries_a_seat_holds() {
        let (rogue, honest, missing, stranger) = ([1u8; 32], [2u8; 32], [3u8; 32], [9u8; 32]);
        let seat_apps = [rogue, honest, missing, [7u8; 32]];
        let member = |n: u8| [100 + n; 32];
        let mut peer_keys: HashMap<u32, [u8; 32]> = HashMap::new();
        let mut bound: HashMap<[u8; 32], [u8; 32]> = HashMap::new();
        let mut confirmed: std::collections::HashSet<u32> = std::collections::HashSet::new();
        // The rogue and four entries of its own, the honest seat, a stranger
        // bound to a key the table does not seat, and a member not named yet.
        for n in 0..5u8 {
            peer_keys.insert(u32::from(n), member(n));
            bound.insert(member(n), rogue);
            confirmed.insert(u32::from(n));
        }
        peer_keys.insert(5, member(5));
        bound.insert(member(5), honest);
        confirmed.insert(5);
        peer_keys.insert(6, member(6));
        bound.insert(member(6), stranger);
        confirmed.insert(6);
        peer_keys.insert(7, member(7));
        confirmed.insert(7);
        let cut = std::collections::HashSet::new();
        let seats = seats_among(&confirmed, &peer_keys, &cut, &bound, &seat_apps);
        assert_eq!(seats, 2, "the rogue once, the honest seat once: eight members");
        assert!(group_short(seats, 3), "the group is short of the missing seat, and offered to it again");
        assert!(!group_complete(seats, 3), "so hand 1 does not open without it");
        // Counted by member, as before: complete while a seat is missing.
        assert!(group_complete(confirmed.len(), 3), "the defect: eight members read as three seats");
        // The missing seat arrives; one of its members still shaking hands
        // (not confirmed) is nothing, a confirmed one is the seat.
        peer_keys.insert(8, member(8));
        bound.insert(member(8), missing);
        assert_eq!(seats_among(&confirmed, &peer_keys, &cut, &bound, &seat_apps), 2);
        confirmed.insert(8);
        let seats = seats_among(&confirmed, &peer_keys, &cut, &bound, &seat_apps);
        assert_eq!(seats, 3);
        assert!(group_complete(seats, 3) && !group_short(seats, 3));
        // A seat whose only entry is cut off here is not in the group here.
        let cut: std::collections::HashSet<[u8; 32]> = [member(5)].into_iter().collect();
        assert_eq!(seats_among(&confirmed, &peer_keys, &cut, &bound, &seat_apps), 2);
    }

    /// `S1-KJ`: a seat seen under a key it never had here is a new entry of
    /// it -- its first key is not, and a key placed again is not.
    #[test]
    fn a_seat_under_a_key_it_never_had_here_is_a_new_entry() {
        let mut seats = std::collections::HashSet::new();
        let mut keys = std::collections::HashSet::new();
        let (seat, other) = ([1u8; 32], [2u8; 32]);
        assert!(!a_new_entry_of_a_seat_seen(&mut seats, &mut keys, seat, [11u8; 32]), "its first entry");
        assert!(!a_new_entry_of_a_seat_seen(&mut seats, &mut keys, seat, [11u8; 32]), "placed again");
        assert!(!a_new_entry_of_a_seat_seen(&mut seats, &mut keys, other, [21u8; 32]), "another seat's first");
        assert!(a_new_entry_of_a_seat_seen(&mut seats, &mut keys, seat, [12u8; 32]), "back under a new key");
        assert!(!a_new_entry_of_a_seat_seen(&mut seats, &mut keys, seat, [12u8; 32]), "said once");
    }

    /// `S1-KI`: a seat's entries share one allowance. Three entries of one seat,
    /// each well inside `D-051`'s ten-second limit on its own, are a flood
    /// together; before the binding is read a member is metered by its own
    /// key, and a member of another seat keeps its own allowance.
    #[test]
    fn a_seats_entries_share_one_flood_allowance() {
        use crate::table::membership::{Meter, FLOOD_SHORT_PACKETS};
        let (seat, other) = ([1u8; 32], [2u8; 32]);
        let entries = [[11u8; 32], [12u8; 32], [13u8; 32]];
        let bound: HashMap<[u8; 32], [u8; 32]> =
            entries.iter().map(|e| (*e, seat)).chain([([21u8; 32], other)]).collect();
        let each = FLOOD_SHORT_PACKETS / 2;
        let mut by_seat: HashMap<[u8; 32], Meter> = HashMap::new();
        let mut by_member: HashMap<[u8; 32], Meter> = HashMap::new();
        let now = 1_000_000u64;
        // Five seconds of it, in the order it arrives.
        for i in 0..each {
            let t = now + i * 5_000 / each;
            for e in entries.iter() {
                by_seat.entry(meter_key(&bound, e)).or_default().packet(t, 100);
                by_member.entry(*e).or_default().packet(t, 100);
            }
            by_seat.entry(meter_key(&bound, &[21u8; 32])).or_default().packet(t, 100);
        }
        let at = now + 5_000;
        assert!(by_seat[&seat].over(at).is_some(), "three entries, one seat: one allowance, spent");
        assert!(by_seat[&other].over(at).is_none(), "the other seat keeps its own");
        assert!(by_member.values().all(|m| m.over(at).is_none()), "the defect: by member, nobody floods");
        assert_eq!(meter_key(&bound, &[99u8; 32]), [99u8; 32], "unbound: its own key");
    }

    /// `S1-KI`: of a seat's confirmed entries the group has heard from, every one
    /// but the two it heard from most recently goes -- never an honest seat's
    /// living process, which joined after its dead ones died and has been
    /// heard since; never an entry with no reading or not confirmed.
    #[test]
    fn a_seat_keeps_its_two_freshest_entries_and_no_more() {
        let e = |key: u8, confirmed, quiet| TwinEntry { key: [key; 32], bound: true, present: true, confirmed, quiet };
        // A rogue's entries, all living: the two freshest stay.
        let mut cut = surplus_entries(&[e(1, true, Some(4)), e(2, true, Some(1)), e(3, true, Some(9)), e(4, true, Some(2))]);
        cut.sort_unstable();
        assert_eq!(cut, vec![[1u8; 32], [3u8; 32]]);
        // Two entries -- S1-DB's pair -- are never judged here.
        assert!(surplus_entries(&[e(1, true, Some(14)), e(2, true, Some(0))]).is_empty());
        // Two quick restarts: two dead entries and the living one, which the
        // group heard last. The oldest dead one goes, the living one stays.
        assert_eq!(surplus_entries(&[e(1, true, Some(12)), e(2, true, Some(6)), e(3, true, Some(1))]), vec![[1u8; 32]]);
        // An entry with no reading, or still shaking hands, is not judged and
        // is not counted against the others.
        assert!(surplus_entries(&[e(1, true, Some(3)), e(2, true, Some(1)), e(3, true, None)]).is_empty());
        assert!(surplus_entries(&[e(1, true, Some(3)), e(2, true, Some(1)), e(3, false, Some(0))]).is_empty());
        // Ties go by key: the same choice on every sweep.
        assert_eq!(surplus_entries(&[e(3, true, Some(1)), e(1, true, Some(1)), e(2, true, Some(1))]), vec![[3u8; 32]]);
    }

    /// `S1-DU`: a seat's entries are all of them, and each reader chooses by
    /// evidence. A client that died leaves a confirmed entry behind for 58 s;
    /// its restart enters the group under a fresh key, and once that key is
    /// taught the seat has two. Before this the sweep's quiet was whichever
    /// entry the map iterated last, a nudge went to whichever `find` met
    /// first, and the stale entry's timeout was reported as the seat leaving
    /// -- a seat that was back and dealing.
    #[test]
    fn a_seat_back_under_a_fresh_key_is_read_by_its_freshest_entry() {
        let seat = [7u8; 32];
        let stale = [1u8; 32];
        let fresh = [2u8; 32];
        let other = [3u8; 32];
        let known_as: HashMap<[u8; 32], [u8; 32]> =
            [(stale, seat), (fresh, seat), (other, [9u8; 32])].into_iter().collect();
        let pairs = vec![(1u32, stale), (2u32, fresh), (3u32, other)];

        // Both entries are the seat's; the other seat's is not.
        let mine = entries_of(&seat, pairs.clone(), &known_as);
        assert_eq!(mine.len(), 2);
        assert!(mine.contains(&(1, stale)) && mine.contains(&(2, fresh)));
        assert_eq!(entries_of(&[5u8; 32], pairs.clone(), &known_as), Vec::new());

        // The freshest speaks: the stale entry is 40 s quiet, the fresh one 1 s.
        let quiet = |gk: &[u8; 32]| match *gk {
            k if k == stale => Some(40),
            k if k == fresh => Some(1),
            _ => None,
        };
        assert_eq!(freshest(&mine, quiet), Some(fresh));
        // An entry with no reading counts as never heard from.
        assert_eq!(freshest(&mine, |gk| (*gk == stale).then_some(40)), Some(stale));
        assert_eq!(freshest(&[], quiet), None);

        // After the stale entry's timeout the seat is still held by the fresh
        // one, so it is not reported gone ...
        let confirmed: std::collections::HashSet<u32> = [2u32, 3].into_iter().collect();
        let after: Vec<(u32, [u8; 32])> = pairs.iter().copied().filter(|(p, _)| *p != 1).collect();
        assert!(entries_of(&seat, after, &known_as).iter().any(|(p, _)| confirmed.contains(p)));
        // ... and with no entry of its left, it is.
        assert!(!entries_of(&seat, vec![(3, other)], &known_as).iter().any(|(p, _)| confirmed.contains(p)));
    }

    /// `S1-DW`: a claim to a seat that is here and speaking is refused; a seat
    /// back under a fresh key is not. The owner's question: can a member say
    /// another has left? Only by first being taken for it, and this is the
    /// gate.
    #[test]
    fn a_claim_to_a_seat_that_is_here_and_speaking_is_refused() {
        let seat = [7u8; 32];
        let held_by = [1u8; 32];
        let claimant = [2u8; 32];
        // Held by a confirmed member that spoke a second ago: refused.
        assert!(claim_refused(&claimant, &seat, [(held_by, seat, true, Some(1))], 20));
        // The holder has been quiet forty seconds (a client that died; S1-DU's
        // return under a fresh key): the claim stands.
        assert!(!claim_refused(&claimant, &seat, [(held_by, seat, true, Some(40))], 20));
        // The holder is not a confirmed member now: stands.
        assert!(!claim_refused(&claimant, &seat, [(held_by, seat, false, Some(1))], 20));
        // The holder's silence is unknown to the library: stands.
        assert!(!claim_refused(&claimant, &seat, [(held_by, seat, true, None)], 20));
        // The same key taught again for the same seat: not a claim against anybody.
        assert!(!claim_refused(&held_by, &seat, [(held_by, seat, true, Some(1))], 20));
        // Another seat's entry says nothing about this one.
        assert!(!claim_refused(&claimant, &seat, [(held_by, [9u8; 32], true, Some(1))], 20));
    }

    /// **The whole stack, between two real Tox instances**: a nine-kilobyte
    /// message — the size of a `SHUFFLE_STEP` — handed to `broadcast` at one
    /// end and taken whole out of `next` at the other, having crossed as seven
    /// fragments over a group nobody searched a DHT for.
    ///
    /// `#[ignore]` because it needs a network and tens of seconds:
    ///
    /// ```text
    /// cargo test --features tox -- --ignored a_whole_message_crosses
    /// ```
    #[tokio::test]
    #[ignore = "needs a network and tens of seconds"]
    async fn a_whole_message_crosses_the_group_in_one_piece() {
        let mut host_tox = Tox::new().expect("a host instance");
        let mut join_tox = Tox::new().expect("a joining instance");
        let host_key: [u8; 32] = host_tox.address()[..32].try_into().unwrap();
        let join_key: [u8; 32] = join_tox.address()[..32].try_into().unwrap();

        // Every node twice: the DHT over UDP and the relay list on every TCP
        // port. Without the relays two peers behind one router never meet.
        for t in [&mut host_tox, &mut join_tox] {
            for n in crate::tox::nodes::bundled() {
                let _ = t.bootstrap(&n.host, n.udp_port, &n.key);
                for p in &n.tcp_ports {
                    let _ = t.add_tcp_relay(&n.host, *p, &n.key);
                }
            }
        }

        let mut host = spawn(
            host_tox,
            Setup {
                role: Role::Host,
                group_name: "TwoNet".into(),
                self_name: "host".into(),
                roster: vec![join_key],
                binder: None,
            },
        );

        // **The advertisement's job, done by hand.** In the client the chat id
        // goes into `TableAd::on_tox` and reaches the joiner through the lobby;
        // here it goes straight across, because what is under test is the Tox
        // side and not the lobby.
        let chat_id = tokio::time::timeout(Duration::from_secs(10), host.wait_for_chat_id())
            .await
            .expect("the group is created locally and at once")
            .expect("and it has an id");

        let mut join = spawn(
            join_tox,
            Setup {
                role: Role::Joiner {
                    founder: host_key,
                    chat_id: Some(chat_id),
                },
                group_name: "TwoNet".into(),
                self_name: "player".into(),
                roster: vec![host_key],
                binder: None,
            },
        );

        // A `SHUFFLE_STEP`-sized message: seven fragments at Tox's packet.
        let message: Vec<u8> = (0..9_000).map(|i| (i % 251) as u8).collect();
        // Several fragments -- how many follows the packet size, and is not
        // what this test is about (it was a stale 7 against today's 19).
        assert!(
            fragment::split(&message, 0, fragment::TOX_PACKET).unwrap().len() > 1,
            "a nine-kilobyte message is more than one packet"
        );

        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        let mut got: Option<Vec<u8>> = None;
        while std::time::Instant::now() < deadline {
            // Sent every turn: the joiner is in the group before the founder
            // knows it, so the first few go to nobody.
            let _ = host.broadcast(&message).await;
            match tokio::time::timeout(Duration::from_millis(250), join.next()).await {
                Ok(Some(item)) => {
                    got = Some(item.bytes);
                    break;
                }
                Ok(None) => break,
                Err(_) => {}
            }
        }

        assert_eq!(
            got.as_deref(),
            Some(message.as_slice()),
            "nine kilobytes arrived whole, in the order it was cut"
        );
    }

    /// **An invitation into a group the advertisement did not name is left.**
    ///
    /// The reason `tox_chat_id` is published at all. An invitation says nothing
    /// about which group it is for, so without the comparison a founder could
    /// put the table on a group nobody advertised — and every other client
    /// would be watching a different one, which is what a founder colluding
    /// with one player would arrange.
    ///
    /// The joiner here is told to expect a chat id that is not the host's. It
    /// accepts the invitation, because accepting is the only way Tox lets it
    /// learn which group the invitation was for, reads the id back, and leaves.
    /// Nothing then arrives, and `chat_id()` stays `None` — the driver never
    /// took the group as its own.
    #[tokio::test]
    #[ignore = "needs a network and tens of seconds"]
    async fn an_invitation_to_the_wrong_group_is_left() {
        let mut host_tox = Tox::new().expect("a host instance");
        let mut join_tox = Tox::new().expect("a joining instance");
        let host_key: [u8; 32] = host_tox.address()[..32].try_into().unwrap();
        let join_key: [u8; 32] = join_tox.address()[..32].try_into().unwrap();
        for t in [&mut host_tox, &mut join_tox] {
            for n in crate::tox::nodes::bundled() {
                let _ = t.bootstrap(&n.host, n.udp_port, &n.key);
                for p in &n.tcp_ports {
                    let _ = t.add_tcp_relay(&n.host, *p, &n.key);
                }
            }
        }

        let mut host = spawn(
            host_tox,
            Setup {
                role: Role::Host,
                group_name: "TwoNet".into(),
                self_name: "host".into(),
                roster: vec![join_key],
                binder: None,
            },
        );
        let real = tokio::time::timeout(Duration::from_secs(10), host.wait_for_chat_id())
            .await
            .expect("the group exists at once")
            .expect("and has an id");

        // Every byte flipped: a chat id that is certainly not this group's, and
        // certainly not a value anybody could have arrived at by accident.
        let mut wrong = real;
        for b in &mut wrong {
            *b ^= 0xff;
        }

        let mut join = spawn(
            join_tox,
            Setup {
                role: Role::Joiner {
                    founder: host_key,
                    chat_id: Some(wrong),
                },
                group_name: "TwoNet".into(),
                self_name: "player".into(),
                roster: vec![host_key],
                binder: None,
            },
        );

        let message = vec![0xA5u8; 4_000];
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while std::time::Instant::now() < deadline {
            let _ = host.broadcast(&message).await;
            if let Ok(got) =
                tokio::time::timeout(Duration::from_millis(250), join.next()).await
            {
                panic!("a message arrived over a group nobody advertised: {got:?}");
            }
        }
        assert_eq!(
            join.chat_id(),
            None,
            "the joiner never took the group as its own"
        );
    }

    /// A message the protocol could not have built is refused rather than
    /// handed to a fragmenter that would refuse it later and quieter.
    #[tokio::test]
    async fn a_message_over_the_cap_is_refused_at_the_transport() {
        let tox = Tox::new().expect("an instance");
        let mut t = spawn(
            tox,
            Setup {
                role: Role::Host,
                group_name: "T".into(),
                self_name: "h".into(),
                roster: Vec::new(),
                binder: None,
            },
        );
        assert_eq!(t.cap(), fragment::MAX_MESSAGE);
        let too_big = vec![0u8; fragment::MAX_MESSAGE + 1];
        assert_eq!(
            t.broadcast(&too_big).await,
            Err(TransportError::TooLarge {
                bytes: too_big.len(),
                cap: fragment::MAX_MESSAGE,
            })
        );
        t.leave().await;
    }
}
