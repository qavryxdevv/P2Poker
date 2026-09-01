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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc as sync_mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::table::fragment::{self, Reassembler};
use crate::table::transport::{FromTable, PlayerId, TableTransport, TransportError};
use crate::tox::{Event, Tox};

/// How this client stands to the table's group.
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
        /// a founder colluding with one player would want. So the group is
        /// joined, its id read back, and compared; a mismatch is left again at
        /// once.
        ///
        /// `None` when the advertisement named none, which is an advert from a
        /// build with no Tox. There is then nothing to compare against and
        /// nothing to be invited to, so an invitation is refused outright
        /// rather than accepted on trust.
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
    /// Leave the group and stop.
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
    /// Fragments toxcore would not take. The queue was full or the group had
    /// nobody in it; either way the message stays and is tried again.
    pub refused: AtomicU64,
    /// Whole messages waiting to go out. A number that does not come down is a
    /// transport that has stopped keeping up, which at a table shows as seats
    /// being certified late for saying things they did say.
    pub waiting: AtomicU64,
    /// Fragments handed to toxcore and accepted.
    pub sent: AtomicU64,
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
}

/// The handle the rest of the client holds.
///
/// Implements [`TableTransport`], so nothing above it knows a Tox group is
/// underneath — which is the whole point of `table::transport` existing.
pub struct ToxTable {
    out: tokio::sync::mpsc::Sender<Vec<u8>>,
    inbox: tokio::sync::mpsc::Receiver<FromTable>,
    control: sync_mpsc::Sender<Command>,
    /// The group's `chat_id`, once there is a group.
    ///
    /// The founder **needs** this: `TableAd::on_tox` puts it in the
    /// advertisement, and until it is there nobody can check that the group
    /// they were invited into is the one the table named. It is a `watch`
    /// rather than a return value because the group does not exist when
    /// `spawn` returns — the driver creates it on its own thread — and a
    /// constructor that blocked until it did would block the client's startup
    /// on a socket.
    chat: tokio::sync::watch::Receiver<Option<[u8; 32]>>,
    trouble: Arc<Trouble>,
    /// Kept so a caller can wait for the thread to finish on shutdown, and so
    /// that dropping the handle does not orphan it silently.
    thread: Option<std::thread::JoinHandle<()>>,
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
    /// `false` rather than a stall — and a message dropped here is re-sent by
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
    /// `None` if the driver stopped without ever having one — a founder whose
    /// `tox_group_new` failed, or a joiner that was never invited. A caller
    /// that treated that as "wait for ever" would hang a table on a group that
    /// is not coming.
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
        let _ = self.control.send(c);
    }
}

impl Drop for ToxTable {
    fn drop(&mut self) {
        let _ = self.control.send(Command::Leave);
        if let Some(t) = self.thread.take() {
            // Joined rather than detached: the thread owns a Tox instance and a
            // socket, and a client that exits while one is still running leaves
            // a seat looking occupied to everybody else.
            let _ = t.join();
        }
    }
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
        let _ = self.control.send(Command::Leave);
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

/// Start the driver on its own thread.
///
/// `tox` is moved onto that thread and stays there. The returned handle is the
/// only way to reach it, and dropping the handle stops it.
pub fn spawn(tox: Tox, setup: Setup) -> ToxTable {
    // **Deep enough for a re-send burst plus the event that matters.** The node
    // re-broadcasts everything it has said every five seconds, which is up to
    // sixty-four messages at once, and a fresh action arriving while that
    // backlog is in the channel must not be the one dropped. `try_broadcast`
    // refuses silently when it is full, and refusing a *re-send* is free while
    // refusing a new event costs a seat its deadline.
    let (out_tx, out_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(512);
    let (in_tx, in_rx) = tokio::sync::mpsc::channel::<FromTable>(256);
    let (ctl_tx, ctl_rx) = sync_mpsc::channel::<Command>();
    let (chat_tx, chat_rx) = tokio::sync::watch::channel::<Option<[u8; 32]>>(None);
    let trouble = Arc::new(Trouble::default());
    let theirs = Arc::clone(&trouble);

    let thread = std::thread::Builder::new()
        .name("tox-table".into())
        .spawn(move || run(tox, setup, out_rx, in_tx, ctl_rx, chat_tx, theirs))
        .expect("a thread for the table's transport");

    ToxTable {
        out: out_tx,
        inbox: in_rx,
        control: ctl_tx,
        chat: chat_rx,
        trouble,
        thread: Some(thread),
    }
}

/// The driver loop. Owns the Tox instance for its whole life.
fn run(
    mut tox: Tox,
    setup: Setup,
    mut out: tokio::sync::mpsc::Receiver<Vec<u8>>,
    inbox: tokio::sync::mpsc::Sender<FromTable>,
    control: sync_mpsc::Receiver<Command>,
    chat: tokio::sync::watch::Sender<Option<[u8; 32]>>,
    trouble: Arc<Trouble>,
) {
    // Tox friend number -> that friend's public key, so an invitation can be
    // matched against the roster rather than accepted from whoever sends one.
    let mut friends: HashMap<u32, [u8; 32]> = HashMap::new();
    // **This client's own key, so it can never be added to its own roster.**
    // `Setup::roster` says *this client's excepted*, and the caller does not
    // keep to it: `net::run::seat_on_tox` tells the driver about **every** seat
    // the formation holds, its own included, and `Command::Seated` then pushed
    // it here. The count that comes out is one too many, so the group-complete
    // gate waits for a seat that is this client and can never arrive - measured,
    // every joiner at a six-seat table reporting *held 4 of 6 other seats* and
    // running to the sixty-second fallback, while the founder, whose own key was
    // not in the list, reported *4 of 5*.
    //
    // Guarded here rather than at the caller because this is the one place that
    // owns the list, and a second caller would make the same mistake.
    let me: [u8; 32] = tox.address()[..32].try_into().unwrap_or([0u8; 32]);
    let mut roster: Vec<[u8; 32]> = setup.roster.iter().copied().filter(|k| *k != me).collect();
    for key in &roster {
        if let Ok(n) = tox.add_friend(key) {
            friends.insert(n, *key);
        }
    }

    let mut group: Option<u32> = match &setup.role {
        Role::Host => tox.new_group(&setup.group_name, &setup.self_name).ok(),
        Role::Joiner { .. } => None,
    };
    // Said as soon as there is something to say. The founder's advertisement
    // cannot name the group until this arrives, and nothing else can check the
    // group it was invited into until the advertisement names it.
    announce(&tox, group, &chat);

    let mut invited: Vec<u32> = Vec::new();
    // Which friends toxcore currently reports as up. **Kept because an
    // invitation is a condition, not an event.** It was sent only from the
    // `FriendConnection` up-edge, so an invitation that toxcore refused was
    // never retried: `invited` is only appended to when `invite` succeeds, and
    // the one trigger had already passed. The next up-edge for that friend
    // comes when the connection drops and returns, which at a table means a
    // seated player sits outside the group until the network hiccups.
    //
    // Observed once at three seats: a seat entered the group at **100 s**,
    // having asked to join at 1.5 s, and in between opened its own hands on a
    // genesis nobody else held. It never played a hand and still printed
    // `TABLE FORMED seats=3`.
    let mut connected: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut reassembler: Reassembler<u32> = Reassembler::new(fragment::TOX_PACKET);
    let mut next_id: u32 = 0;
    let mut last_sweep = Instant::now();
    let mut last_reinvite = Instant::now();
    let mut pending: Vec<Vec<u8>> = Vec::new();

    loop {
        // --- what the client asked for -------------------------------------
        let mut stop = false;
        while let Ok(cmd) = control.try_recv() {
            match cmd {
                Command::Seated(key) if key == me => {
                    // This client. Not a friend of itself and not a peer of
                    // itself; see `me` above for what happened when it was.
                }
                Command::Seated(key) => {
                    if !roster.contains(&key) {
                        roster.push(key);
                    }
                    if let Ok(n) = tox.add_friend(&key) {
                        friends.insert(n, key);
                    }
                }
                Command::Unseated(key) => {
                    roster.retain(|k| *k != key);
                    // Removed from the group where this client is the admin.
                    // Elsewhere it is not refused so much as impossible, and
                    // pretending to do it would be a second membership answer
                    // beside the roster's.
                    if matches!(setup.role, Role::Host) {
                        if let Some(g) = group {
                            if let Some(peer) = peer_for(&tox, g, &key) {
                                let _ = tox.kick(g, peer);
                            }
                        }
                    }
                }
                Command::Rejoined(key) if key != me => {
                    if matches!(setup.role, Role::Host) {
                        if let Some(n) = friends.iter().find(|(_, k)| **k == key).map(|(n, _)| *n) {
                            invited.retain(|f| *f != n);
                            // **And if the friendship is down, start it over.**
                            // Forgetting that the peer was invited is not enough
                            // on its own: `invite_pending` sends only to a
                            // connected friend, and toxcore will keep trying the
                            // address the peer had before it restarted for
                            // something over two and a half minutes — see
                            // `Tox::forget_friend` for the constants. Measured,
                            // the founder sat at `tox friends up 2` of three for
                            // a whole five-minute run.
                            //
                            // Only while it is down. A connected friend is
                            // reachable and deleting it would throw away a
                            // working connection to solve a problem it does not
                            // have.
                            if !connected.contains(&n) {
                                let _ = tox.forget_friend(n);
                                friends.remove(&n);
                                connected.remove(&n);
                                invited.retain(|f| *f != n);
                                if let Ok(fresh) = tox.add_friend(&key) {
                                    friends.insert(fresh, key);
                                }
                            }
                        }
                        invite_pending(&mut tox, group, &friends, &connected, &mut invited, &trouble);
                    }
                }
                Command::Rejoined(_) => {}
                Command::Leave => stop = true,
            }
        }
        if stop {
            // **Flushed before leaving, not dropped.** Breaking here discarded
            // whatever was still in `pending`, and what is still in `pending`
            // when a client stops is the *last* message it produced - the
            // `HAND_COMPLETE` that ends the hand. Measured: a hand played to the
            // river over Tox, the seat that finished first left, and the other
            // sat at the settlement stage until its deadline waiting for a
            // message that had been built, queued and thrown away.
            //
            // Bounded, because leaving must not become waiting: a fixed number
            // of turns, and then it goes whatever is left.
            for _ in 0..FLUSH_TURNS {
                if pending.is_empty() {
                    break;
                }
                tox.iterate();
                if let Some(g) = group {
                    flush(&mut tox, g, &mut pending, &mut next_id, &trouble);
                }
                std::thread::sleep(tox.interval().min(MAX_TICK));
            }
            break;
        }

        // --- one turn of toxcore's own loop --------------------------------
        for e in tox.iterate() {
            match e {
                Event::FriendConnection { friend, status } if status != 0 => {
                    // Up. The founder invites every seat the roster names, once
                    // its friend connection is up — before that there is nothing
                    // to invite, because an invitation is carried over the
                    // friendship. Trying here as well as in the sweep only makes
                    // the common case immediate; `invite_pending` is what makes
                    // it certain.
                    connected.insert(friend);
                    if matches!(setup.role, Role::Host) {
                        invite_pending(&mut tox, group, &friends, &connected, &mut invited, &trouble);
                    }
                }
                Event::FriendConnection { friend, .. } => {
                    // Down. The invitation is forgotten so that a peer which
                    // reconnects is invited again — a group membership does not
                    // survive a client restart, and a peer that came back
                    // without one would sit outside the table for ever.
                    connected.remove(&friend);
                    invited.retain(|f| *f != friend);
                }
                Event::GroupInvite { friend, invite } => {
                    // **Only from the founder the advertisement named.** An
                    // invitation is an offer; this client joins the table it
                    // decided to join, and a friend that is on the roster but
                    // is not the founder has no business inviting anybody.
                    let expected = match (&setup.role, friends.get(&friend)) {
                        (Role::Joiner { founder, chat_id }, Some(key)) if key == founder => {
                            *chat_id
                        }
                        _ => None,
                    };
                    // Refused outright when the advertisement named no group:
                    // there is nothing to compare against, and an invitation
                    // accepted on trust is the whole thing the id is published
                    // to prevent.
                    if let (Some(want), None) = (expected, group) {
                        if let Ok(joined) = tox.accept_invite(friend, &invite, &setup.self_name) {
                            // **Read back and compared.** Accepting is the only
                            // way to learn which group the invitation was for;
                            // Tox does not say beforehand. So the check is
                            // after, and a mismatch leaves at once — the table
                            // then ends at its own deadline, which is the right
                            // outcome for a founder that pointed somewhere
                            // nobody advertised.
                            match tox.chat_id(joined) {
                                Ok(id) if id == want => {
                                    group = Some(joined);
                                    announce(&tox, group, &chat);
                                }
                                _ => {
                                    let _ = tox.leave(joined);
                                }
                            }
                        }
                    }
                }
                Event::GroupPacket { peer, data, .. } => {
                    // Reassembled here, so nothing above this module ever sees
                    // a fragment. A refusal costs this sender its part-built
                    // message and nothing else — see `table::fragment` for what
                    // each one means.
                    match reassembler.accept(&peer, &data, millis()) {
                        Ok(Some(message)) => {
                            // `claimed` is None on purpose. A group peer id
                            // resolves to a Tox key, which is not a player's
                            // signing key and is not evidence about one.
                            let item = FromTable {
                                claimed: None,
                                bytes: message,
                            };
                            if inbox.try_send(item).is_err() {
                                // The client is not draining. Dropping is right:
                                // blocking here would stop `tox_iterate`, and a
                                // transport that stalls the network to wait for
                                // its reader is a transport that loses the
                                // connection as well as the message.
                            }
                        }
                        Ok(None) => {}
                        Err(_) => {}
                    }
                }
                Event::FriendRequestIgnored => {}
            }
        }

        // --- and what the client wants said --------------------------------
        while let Ok(message) = out.try_recv() {
            pending.push(message);
        }
        if let Some(g) = group {
            flush(&mut tox, g, &mut pending, &mut next_id, &trouble);
        }

        if last_sweep.elapsed() >= SWEEP_EVERY {
            reassembler.sweep(millis());
            // Every seat that is connected, on the roster and not yet in — see
            // `invite_pending`. Cheap: it does nothing at all once every friend
            // has been invited, which is the state a table spends its life in.
            if matches!(setup.role, Role::Host) {
                invite_pending(&mut tox, group, &friends, &connected, &mut invited, &trouble);
            }
            // **Offer the group again to whoever is not in it.** See
            // `REINVITE_EVERY`: `invited` says what this client has done, and
            // a peer that restarted needs asking again even though it does.
            // Only while the group is short, so a whole table sends nothing.
            if matches!(setup.role, Role::Host)
                && last_reinvite.elapsed() >= REINVITE_EVERY
            {
                let short = match group {
                    Some(g) => tox.peer_count(g) < roster.len(),
                    None => false,
                };
                if short {
                    invited.clear();
                    invite_pending(&mut tox, group, &friends, &connected, &mut invited, &trouble);
                }
                last_reinvite = Instant::now();
            }

            // And whether the group now holds every other seat. `roster` is
            // this client's excepted (see `Setup::roster`), so the answer is a
            // straight comparison. At most ten keys and a scan each, once every
            // five seconds.
            //
            // **Counted, not matched, and the API forces that.**
            // `tox_group_peer_get_public_key` gives a peer's *group* public key
            // (`tox.h:3823`) — a per-group identity, not the friend key the
            // roster holds — so no scan can say *which* seat a group member is.
            // The first version of this compared the two key spaces, always
            // found nothing, and made the gate above it time out every single
            // time: measured, six seats all in the group by 20.7 s and hand 1
            // held until the 60-second fallback fired.
            //
            // A count answers the only question the gate asks. It cannot tell a
            // stranger from a seat, which is sound here because the group is
            // PRIVATE and the founder is the sole admin: the only way in is an
            // invitation the founder sent to a roster key.
            let seen = match group {
                Some(g) => tox.peer_count(g),
                None => 0,
            };
            // **Asked of toxcore rather than remembered.** `connected` is
            // built from `FriendConnection` events, which say what has changed
            // and never what is; a connection whose event was missed is
            // invisible to it for ever. Reconciled here every sweep, so the set
            // the invitations are sent to cannot drift from the one toxcore
            // would deliver them over.
            for (n, _) in friends.iter() {
                if tox.friend_connection(*n) > 0 {
                    connected.insert(*n);
                } else {
                    connected.remove(n);
                }
            }
            trouble
                .self_connection
                .store(tox.connection().max(0) as u64, Ordering::Relaxed);
            trouble.friends_up.store(connected.len() as u64, Ordering::Relaxed);
            trouble.in_group.store(seen as u64, Ordering::Relaxed);
            trouble.want_in_group.store(roster.len() as u64, Ordering::Relaxed);
            trouble
                .complete
                .store(group.is_some() && seen >= roster.len(), Ordering::Relaxed);
            last_sweep = Instant::now();
        }

        std::thread::sleep(tox.interval().min(MAX_TICK));
    }

    if let Some(g) = group {
        let _ = tox.leave(g);
        // One more turn so the part message actually goes out before the
        // instance is dropped. Leaving without it is leaving silently, and the
        // other seats then wait out a deadline for somebody who has gone.
        tox.iterate();
    }
}

/// How many turns a leaving driver spends trying to send what it still holds.
///
/// Leaving must not become waiting, so it is a fixed number rather than a wait
/// for an empty queue: at `MAX_TICK` this is at most a second and a half.
const FLUSH_TURNS: usize = 30;

/// Invite every seat that is connected, on the roster and not in the group yet.
///
/// **An invitation is a condition, not an event, and this is the difference.**
/// It used to be sent only from the `FriendConnection` up-edge. Two things
/// followed. A refusal from `tox_group_invite_friend` was never retried, since
/// `invited` is only appended to when the call succeeds and the trigger had
/// already gone by; and the next up-edge for that friend arrives when the
/// connection drops and returns, so a seated player could sit outside the table
/// until the network happened to hiccup.
///
/// Called from the up-edge as well, so the ordinary case is still immediate.
/// The sweep is what makes it certain rather than likely.
fn invite_pending(
    tox: &mut Tox,
    group: Option<u32>,
    friends: &HashMap<u32, [u8; 32]>,
    connected: &std::collections::HashSet<u32>,
    invited: &mut Vec<u32>,
    trouble: &Trouble,
) {
    let Some(g) = group else { return };
    for friend in pending_invites(connected, friends, invited) {
        if tox.invite(g, friend).is_ok() {
            trouble.invites_sent.fetch_add(1, Ordering::Relaxed);
            invited.push(friend);
        } else {
            // Counted rather than logged: a refusal here is ordinary while the
            // group is settling, and the number is only interesting if it does
            // not stop growing.
            trouble.invites_refused.fetch_add(1, Ordering::Relaxed);
        }
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
    invited: &[u32],
) -> Vec<u32> {
    let mut out: Vec<u32> = connected
        .iter()
        .copied()
        .filter(|f| friends.contains_key(f) && !invited.contains(f))
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
            if tox.send(group, part).is_err() {
                trouble.refused.fetch_add(1, Ordering::Relaxed);
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
fn peer_for(tox: &Tox, group: u32, key: &[u8; 32]) -> Option<u32> {
    // Peer ids are small and dense, and a table is at most ten seats. Scanning
    // is cheaper than keeping a map in step with joins and parts, and a map
    // that drifted would remove the wrong player.
    (0..Tox::PEER_SCAN).find(|p| tox.peer_key(group, *p).as_ref() == Ok(key))
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

        // Connected, on the roster, not yet invited: owed.
        assert_eq!(pending_invites(&set(&[1, 2]), &friends, &[]), vec![1, 2]);

        // Already invited: not owed again. A second invitation is not harmful,
        // but sending one every five seconds for the life of a table is.
        assert_eq!(pending_invites(&set(&[1, 2]), &friends, &[1]), vec![2]);

        // **The case the edge got wrong.** The friend is connected and the
        // invitation was refused, so it is not in `invited` — and there will be
        // no second up-edge. The sweep must still owe it.
        assert_eq!(pending_invites(&set(&[3]), &friends, &[1, 2]), vec![3]);

        // Connected but not a seat at this table: never owed. The founder
        // invites the roster, not everyone toxcore has a friendship with.
        assert_eq!(pending_invites(&set(&[9]), &friends, &[]), Vec::<u32>::new());

        // Not connected: nothing to invite over, which is the case the edge
        // handled correctly and this must not change.
        assert_eq!(pending_invites(&set(&[]), &friends, &[]), Vec::<u32>::new());
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
        assert_eq!(
            fragment::split(&message, 0, fragment::TOX_PACKET).unwrap().len(),
            7
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
