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
    /// `D-035`: the roster seats whose client is a confirmed member of the
    /// group right now, by application key -- recomputed every sweep, for
    /// the window's link indicator.
    pub present: std::sync::Mutex<std::collections::HashSet<[u8; 32]>>,
    /// `D-041`: the roster seats whose friend connection is up right now,
    /// by Tox key -- a seat that has not spoken in the group yet is still
    /// on the line by this.
    pub friends_on: std::sync::Mutex<std::collections::HashSet<[u8; 32]>>,
    /// `D-035`: seats whose client left the group since the node last
    /// asked, by application key, each with whether it quit on purpose.
    pub gone: std::sync::Mutex<Vec<([u8; 32], bool)>>,
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
        let mine: [u8; 32] = tox.address()[..32].try_into().unwrap_or([0u8; 32]);
        let (ctl_tx, ctl_rx) = sync_mpsc::channel::<Ctl>();
        let thread = std::thread::Builder::new()
            .name("tox".into())
            .spawn(move || run(tox, ctl_rx))
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
const REINVITE_EVERY: Duration = Duration::from_secs(30);

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
    group: Option<u32>,
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
            idle.entry(n).or_insert_with(Instant::now);
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
    // **Offer the group again to whoever is not in it.** See `REINVITE_EVERY`:
    // `invited` says what this client has done, and a peer that restarted
    // needs asking again even though it does. Only while the group is short;
    // the invitations themselves go one at a time from the loop.
    if matches!(t.setup.role, Role::Host) && t.last_reinvite.elapsed() >= REINVITE_EVERY {
        let short = match t.group {
            Some(g) => tox.peer_count(g) < t.roster.len(),
            None => false,
        };
        if short {
            t.invited.clear();
            t.last_invite = None;
        }
        t.last_reinvite = Instant::now();
    }
    // Whether the group now holds every other seat. **Counted, not matched**:
    // `tox_group_peer_get_public_key` gives a peer's group key, not the friend
    // key the roster holds, so no scan can say which seat a member is. A count
    // answers the only question the gate asks, and it is sound because the
    // group is PRIVATE and the founder the sole admin.
    let seen = match t.group {
        Some(g) => tox.peer_count(g),
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
    if let Ok(mut present) = t.trouble.present.lock() {
        present.clear();
        if let Some(g) = t.group {
            for (group_key, app_key) in t.known_as.iter() {
                let member =
                    (0..Tox::PEER_SCAN).find(|p| tox.peer_key(g, *p).ok().as_ref() == Some(group_key));
                if member.is_some_and(|p| t.confirmed.contains(&p)) {
                    present.insert(*app_key);
                }
            }
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
    // `D-037`: every sweep while the founder's friendship is up and its key is
    // not a confirmed member, a member offers the group again -- bounded by
    // `invited`, which the founder's down-edge clears.
    if let (Role::Joiner { founder, .. }, Some(g)) = (&t.setup.role, t.group) {
        if t.self_joined {
            if let Some(n) = friend_number(friends, founder) {
                let absent =
                    !peer_for(tox, g, founder, &t.known_as).is_some_and(|p| t.confirmed.contains(&p));
                if absent && connected.contains(&n) && !t.invited.contains(&n) {
                    if tox.invite(g, n).is_ok() {
                        t.invited.push(n);
                        t.trouble.invites_sent.fetch_add(1, Ordering::Relaxed);
                    }
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
    t.trouble.complete.store(
        t.group.is_some() && !t.roster.is_empty() && seen >= t.roster.len(),
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
    if !t.self_joined
        && t.group.is_some()
        && can_be_invited
        && matches!(t.setup.role, Role::Joiner { .. } | Role::Back { .. })
    {
        if let Some(since) = t.accepted_at {
            if since.elapsed() >= JOIN_GRACE && t.rejoins < MAX_REJOINS {
                if let Some(g) = t.group.take() {
                    let _ = tox.leave(g);
                }
                t.accepted_at = None;
                t.confirmed.clear();
                t.rejoins += 1;
                t.trouble.rejoins.store(t.rejoins as u64, Ordering::Relaxed);
            }
        }
    }
}

/// The driver: one instance, every table, until the client ends.
fn run(mut tox: Tox, control: sync_mpsc::Receiver<Ctl>) {
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
                    // Said as soon as there is something to say: the founder's
                    // advertisement cannot name the group until this arrives.
                    announce(&tox, group, &chat);
                    tables.insert(
                        id,
                        TableState {
                            setup,
                            roster,
                            known_as: HashMap::new(),
                            group,
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
                                let group_key = t
                                    .known_as
                                    .iter()
                                    .find(|(_, a)| **a == app_key)
                                    .map(|(gk, _)| *gk);
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
                        Command::KnownAs { group_key, app_key } => {
                            t.known_as.insert(group_key, app_key);
                        }
                        Command::Unseated(key) => {
                            t.roster.retain(|k| *k != key);
                            // Removed from the group where this client is the
                            // admin; elsewhere it is impossible rather than
                            // refused.
                            if matches!(t.setup.role, Role::Host) {
                                if let Some(g) = t.group {
                                    if let Some(peer) = peer_for(&tox, g, &key, &t.known_as) {
                                        let _ = tox.kick(g, peer);
                                    }
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
                            if matches!(t.setup.role, Role::Host) {
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
                        Command::Leave => {}
                    }
                }
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
                    // **Which table this invitation could be for**: one still
                    // without a group, whose advertisement named one (an
                    // invitation with nothing to compare against is refused
                    // outright, see `Role`), and whose role names this friend
                    // -- a joiner's founder, or for a founder coming back any
                    // member of its roster (`D-037`). An invitation says
                    // nothing about which group it is for: it is accepted,
                    // the group's id read back and compared, and a mismatch
                    // is left at once.
                    let from = friends.get(&friend).copied();
                    let candidates: Vec<(TableId, [u8; 32])> = tables
                        .iter()
                        .filter_map(|(id, t)| {
                            if t.group.is_some() || t.closing.is_some() {
                                return None;
                            }
                            match (&t.setup.role, from) {
                                (
                                    Role::Joiner {
                                        founder,
                                        chat_id: Some(want),
                                    },
                                    Some(k),
                                ) if k == *founder => Some((*id, *want)),
                                (Role::Back { chat_id: Some(want) }, Some(k))
                                    if t.roster.contains(&k) =>
                                {
                                    Some((*id, *want))
                                }
                                _ => None,
                            }
                        })
                        .collect();
                    let Some((first, _)) = candidates.first().copied() else {
                        continue;
                    };
                    let self_name = tables[&first].setup.self_name.clone();
                    let Ok(joined) = tox.accept_invite(friend, &invite, &self_name) else {
                        continue;
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
                            announce(&tox, t.group, &t.chat);
                            // **The harness's stall, once per process.**
                            // Sleeping here starves the handshake past
                            // toxcore's twelve-second reaper without touching
                            // the code under test.
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
                        t.confirmed.clear();
                        t.trouble.join_fails.fetch_add(1, Ordering::Relaxed);
                        let _ = reason;
                    }
                }
                Event::GroupPeerJoin { group: g, peer } => {
                    if let Some(t) = by_group(&mut tables, g) {
                        t.confirmed.insert(peer);
                    }
                }
                Event::GroupPeerExit {
                    group: g,
                    peer,
                    key,
                    quit,
                } => {
                    if let Some(t) = by_group(&mut tables, g) {
                        t.confirmed.remove(&peer);
                        // `D-035`: a seat this driver knows by its group key is
                        // reported gone, on purpose or by timing out.
                        if let Some(app) = key.and_then(|k| t.known_as.get(&k).copied()) {
                            if let Ok(mut present) = t.trouble.present.lock() {
                                present.remove(&app);
                            }
                            if let Ok(mut gone) = t.trouble.gone.lock() {
                                gone.push((app, quit));
                            }
                        }
                    }
                }
                Event::GroupPacket { group: g, peer, data } => {
                    // Reassembled here, so nothing above this module ever sees
                    // a fragment.
                    if let Some(t) = by_group(&mut tables, g) {
                        match t.reassembler.accept(&peer, &data, millis()) {
                            Ok(Some(message)) => {
                                // The sender's GROUP key, advisory: the
                                // signature inside the message is the only
                                // thing that says who spoke. Carried out so
                                // the node loop can pair it with the signing
                                // key (`S1-I`).
                                let item = FromTable {
                                    claimed: tox.peer_key(g, peer).ok(),
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
                            Err(_) => {}
                        }
                    }
                }
                Event::FriendRequestIgnored => {}
            }
        }

        // --- the founder's invitations, one at a time -------------------------
        for t in tables.values_mut() {
            if matches!(t.setup.role, Role::Host) && t.closing.is_none() {
                invite_pending(&mut tox, t, &friends, &connected);
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
    let Some(friend) = pending_invites(connected, friends, &t.roster, &t.invited)
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

/// Which group peer holds this Tox public key, if any.
/// Which group peer is the seat holding this **application** key.
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
/// Peer ids are small and dense and a table is at most ten seats, so scanning
/// beats keeping a second map in step with joins and parts.
fn peer_for(
    tox: &Tox,
    group: u32,
    app_key: &[u8; 32],
    known_as: &std::collections::HashMap<[u8; 32], [u8; 32]>,
) -> Option<u32> {
    (0..Tox::PEER_SCAN).find(|p| {
        tox.peer_key(group, *p)
            .ok()
            .and_then(|g| known_as.get(&g))
            .is_some_and(|a| a == app_key)
    })
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
