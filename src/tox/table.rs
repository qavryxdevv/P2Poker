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

/// fault-harness: the stranger's instance and how far it got.
#[cfg(feature = "fault-harness")]
struct Stranger {
    tox: Tox,
    /// The stranger as this client's friend, and this client as the stranger's.
    friend_here: u32,
    invited: bool,
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
    Seats { apps: Vec<[u8; 32]>, fixed: bool },
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
}

impl std::fmt::Display for Cut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cut::Flood(why) => write!(f, "it flooded the table's group ({why})"),
            Cut::NotASeat => write!(f, "its name binds it to no seat of this table"),
            Cut::NotItsBinding => write!(f, "its name is a binding made for another member"),
            Cut::Nameless => write!(f, "it said no seat within {} s of joining", NAME_GRACE.as_secs()),
            Cut::Barred => write!(f, "it is bound to a seat barred from this table"),
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
            || matches!(&self.setup.role, Role::Joiner { founder, .. } if founder == key)
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
    let meter = t.meters.entry(key).or_default();
    meter.noise(now, points);
    if let Some(flood) = meter.over(now) {
        cut_off(tox, t, g, key, Cut::Flood(flood));
    }
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
        let short = t.group.is_some() && group_short(t.confirmed.len(), t.roster.len());
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
    // key the roster holds, so no scan can say which seat a member is. A count
    // answers the only question the gate asks, and it is sound because the
    // group is PRIVATE and the founder the sole admin.
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
    let seen = match t.group {
        Some(_) => t.confirmed.len(),
        None => 0,
    };
    // The friend connections that are up among the ones THIS table needs.
    let mine_up = connected
        .iter()
        .filter(|n| friends.get(n).is_some_and(|k| t.needs(k)))
        .count();
    t.trouble.self_connection.store(self_connection, Ordering::Relaxed);
    t.trouble.friends_up.store(mine_up as u64, Ordering::Relaxed);
    t.trouble.in_group.store(seen as u64, Ordering::Relaxed);
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
                let member =
                    (0..Tox::PEER_SCAN).find(|p| tox.peer_key(g, *p).ok().as_ref() == Some(group_key));
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
    if let (Role::Joiner { founder, .. }, Some(g)) = (&t.setup.role, t.group) {
        if t.self_joined && t.roster.contains(founder) {
            if let Some(n) = friend_number(friends, founder) {
                let here = t.peer_lines.iter().any(|(p, l)| l == founder && t.confirmed.contains(p));
                let absent = !here && group_short(t.confirmed.len(), t.roster.len());
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
    t.trouble
        .confirmed_peers
        .store(t.confirmed.len() as u64, Ordering::Relaxed);
    t.trouble.founder_link.store(
        match &t.setup.role {
            Role::Host => 3,
            Role::Joiner { founder, .. } => friend_number(friends, founder)
                .map(|n| tox.friend_connection(n).max(0) as u64)
                .unwrap_or(0),
            // `D-037`: no founder to reach; the best member link stands in.
            Role::Back { .. } => t
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
        t.group.is_some() && group_complete(t.confirmed.len(), t.roster.len()),
        Ordering::Relaxed,
    );

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
    let mut stranger: Option<Stranger> = None;
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
                                let member = (0..Tox::PEER_SCAN)
                                    .find(|p| tox.peer_key(g, *p).ok().as_ref() == Some(&group_key));
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
                                // `D-047`: for good is for good -- off this table's roster
                                // too, so the founder never offers the group to it again
                                // (a fresh key needs an invitation) and its friendship
                                // idles out with the others no table needs.
                                if for_good {
                                    if let Some(k) = tox_key {
                                        t.roster.retain(|r| *r != k);
                                    }
                                    // `D-051`: and barred by its application key, so a
                                    // fresh member key bound to it is cut off on sight.
                                    if let Some(a) = app_key {
                                        bar_seat(&mut tox, t, g, a);
                                    }
                                }
                            }
                        }
                        Command::Seats { apps, fixed } => {
                            t.seat_apps = apps;
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
                        Command::KickWithoutWord(key) => {
                            if let Some(g) = t.group {
                                let peers: Vec<u32> =
                                    t.peer_lines.iter().filter(|(_, l)| **l == key).map(|(p, _)| *p).collect();
                                for p in peers {
                                    let _ = tox.kick(g, p);
                                }
                            }
                        }
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
                }
            }
        }

        // `patches/0035`: the harness's outage, applied on its edges.
        #[cfg(feature = "fault-harness")]
        if let Some((at, dur, every)) = offline_window() {
            let now = started.elapsed();
            let cut = match now.checked_sub(at) {
                None => false,
                Some(since) if every.is_zero() => since < dur,
                Some(since) => since.as_secs() % every.as_secs() < dur.as_secs(),
            };
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
                            if t.self_joined && friends.get(&friend) == Some(founder) {
                                t.invited.retain(|f| *f != friend);
                                if tox.invite(g, friend).is_ok() {
                                    t.invited.push(friend);
                                    // `S1-FE`: the sweep's next offer counts from this one.
                                    t.founder_offered = Some(Instant::now());
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
                    for t in tables.values_mut() {
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
                        }
                    }
                    if left_one {
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
                            let meter = t.meters.entry(k).or_default();
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
                if now >= at && !stranger_tried {
                    stranger_tried = true;
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
                                    stranger = Some(Stranger {
                                        tox: st,
                                        friend_here: n,
                                        invited: false,
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
                if let Some(s) = stranger.as_mut() {
                    for e in s.tox.iterate() {
                        match e {
                            Event::GroupInvite { friend, invite } if s.group.is_none() => {
                                match s.tox.accept_invite(friend, &invite, "stranger") {
                                    Ok(g2) => {
                                        s.group = Some(g2);
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
                                            if copy { ", named with a copy of a seat's binding" } else { "" }
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
                    if !s.invited && tox.friend_connection(s.friend_here) > 0 {
                        if let Some(g) = tables.values().find_map(|t| t.group) {
                            if tox.invite(g, s.friend_here).is_ok() {
                                s.invited = true;
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
                    if t.group.is_some() && group_short(t.confirmed.len(), t.roster.len()) {
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
            && entries_of(&app, scan_pairs(tox, g), &t.known_as)
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
    match (&t.setup.role, from) {
        (Role::Joiner { founder, chat_id: Some(_) }, Some(k)) => k == *founder,
        (Role::Back { chat_id: Some(_) }, Some(k)) => t.roster.contains(&k),
        // `S1-FE`: a founder, from a member of its roster -- only ever taken
        // into the group it created (`wanted_chat`), and only once its own copy
        // has held nobody else for `FOUNDER_YIELDS_AFTER`.
        (Role::Host, Some(k)) => t.roster.contains(&k),
        _ => false,
    }
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
    if let Some((at, confirmed_then)) = t.last_invite {
        if t.confirmed.len() <= confirmed_then && at.elapsed() < INVITE_GAP {
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
        t.last_invite = Some((Instant::now(), t.confirmed.len()));
    } else {
        // Counted rather than logged: a refusal here is ordinary while the
        // group is settling, and the number is only interesting if it does
        // not stop growing.
        t.trouble.invites_refused.fetch_add(1, Ordering::Relaxed);
    }
}

/// `S1-FE`: whether the table's group holds every other seat. `confirmed`
/// counts the confirmed OTHER members; `others` the roster's seats but this
/// client's own, which is how `TableState::roster` holds them. An empty roster
/// is never complete: the roster reaches the driver a turn behind the node.
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
fn scan_pairs(tox: &Tox, group: u32) -> Vec<(u32, [u8; 32])> {
    (0..Tox::PEER_SCAN)
        .filter_map(|p| tox.peer_key(group, p).ok().map(|k| (p, k)))
        .collect()
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

/// Milliseconds for the reassembler's timers.
///
/// Wall time, and it only ever measures a difference between two readings taken
/// here — nothing in the protocol depends on it, which is `PROTOCOL.md` §8.2's
/// rule about deadlines being local.
fn millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

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
