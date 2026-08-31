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
use std::sync::mpsc as sync_mpsc;
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
    /// Leave the group and stop.
    Leave,
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
    /// Kept so a caller can wait for the thread to finish on shutdown, and so
    /// that dropping the handle does not orphan it silently.
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ToxTable {
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

/// Start the driver on its own thread.
///
/// `tox` is moved onto that thread and stays there. The returned handle is the
/// only way to reach it, and dropping the handle stops it.
pub fn spawn(tox: Tox, setup: Setup) -> ToxTable {
    let (out_tx, out_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let (in_tx, in_rx) = tokio::sync::mpsc::channel::<FromTable>(256);
    let (ctl_tx, ctl_rx) = sync_mpsc::channel::<Command>();
    let (chat_tx, chat_rx) = tokio::sync::watch::channel::<Option<[u8; 32]>>(None);

    let thread = std::thread::Builder::new()
        .name("tox-table".into())
        .spawn(move || run(tox, setup, out_rx, in_tx, ctl_rx, chat_tx))
        .expect("a thread for the table's transport");

    ToxTable {
        out: out_tx,
        inbox: in_rx,
        control: ctl_tx,
        chat: chat_rx,
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
) {
    // Tox friend number -> that friend's public key, so an invitation can be
    // matched against the roster rather than accepted from whoever sends one.
    let mut friends: HashMap<u32, [u8; 32]> = HashMap::new();
    let mut roster: Vec<[u8; 32]> = setup.roster.clone();
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
    let mut reassembler: Reassembler<u32> = Reassembler::new(fragment::TOX_PACKET);
    let mut next_id: u32 = 0;
    let mut last_sweep = Instant::now();
    let mut pending: Vec<Vec<u8>> = Vec::new();

    loop {
        // --- what the client asked for -------------------------------------
        let mut stop = false;
        while let Ok(cmd) = control.try_recv() {
            match cmd {
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
                Command::Leave => stop = true,
            }
        }
        if stop {
            break;
        }

        // --- one turn of toxcore's own loop --------------------------------
        for e in tox.iterate() {
            match e {
                Event::FriendConnection { friend, status } if status != 0 => {
                    // The founder invites every seat the roster names, once its
                    // friend connection is up. Before that there is nothing to
                    // invite: an invitation is carried over the friendship.
                    if matches!(setup.role, Role::Host) {
                        if let (Some(g), false) = (group, invited.contains(&friend)) {
                            if friends.contains_key(&friend) && tox.invite(g, friend).is_ok() {
                                invited.push(friend);
                            }
                        }
                    }
                }
                Event::FriendConnection { friend, .. } => {
                    // Down. The invitation is forgotten so that a peer which
                    // reconnects is invited again — a group membership does not
                    // survive a client restart, and a peer that came back
                    // without one would sit outside the table for ever.
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
            // One message at a time, and only while its fragments are being
            // accepted. `tox_group_send_custom_packet` refuses when the group
            // has no peers or its send queue is full, and pushing the next
            // message on top of a refusal would interleave two half-sent ones.
            while let Some(message) = pending.first() {
                let id = next_id;
                let Ok(parts) = fragment::split(message, id, fragment::TOX_PACKET) else {
                    // Longer than the protocol builds. Dropped rather than
                    // retried for ever, because it will not get shorter.
                    pending.remove(0);
                    continue;
                };
                let mut sent_all = true;
                for part in &parts {
                    if tox.send(g, part).is_err() {
                        sent_all = false;
                        break;
                    }
                }
                if !sent_all {
                    break;
                }
                next_id = next_id.wrapping_add(1);
                pending.remove(0);
            }
        }

        if last_sweep.elapsed() >= SWEEP_EVERY {
            reassembler.sweep(millis());
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
    (0..64u32).find(|p| tox.peer_key(group, *p).as_ref() == Ok(key))
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
