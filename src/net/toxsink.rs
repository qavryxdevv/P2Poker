//! Where a table's game traffic goes when it rides Tox (D-019), and a stub of
//! the same shape when this build has no Tox at all.
//!
//! # Why this type exists rather than `#[cfg]` in the node loop
//!
//! `run.rs` is one `select!` inside one loop, and it is the most carefully
//! measured file in this project — the single-exit verdict in its table arm was
//! a day's debugging to arrive at. Scattering `#[cfg(feature = "tox")]` through
//! it would mean two versions of that loop, only one of which any given build
//! compiles, and the untested one would be the one that rots.
//!
//! So the cfg lives here, once, behind an API that is the same either way. A
//! build without the feature gets a sink that is always empty, whose
//! [`next`](crate::net::toxsink::TableSink::next) never resolves, and whose branch in the `select!`
//! is therefore inert. The loop reads identically in both.
//!
//! # What rides it, and what does not
//!
//! **Only the hand.** D-019 is explicit that everything up to the table
//! existing — discovery, the lobby, the join RPC, the roster, the ratification
//! that produces `session_id` — stays on libp2p exactly as it is. The table's
//! own mesh keeps carrying `PLAYER_LIST` and `TABLE_READY`; what moves is the
//! game.

use std::path::Path;

use crate::table::transport::FromTable;

/// How this client stands to the table's group, without naming a Tox type.
///
/// A mirror of `tox::table::Role`, and it exists so that `run.rs` can say what
/// it wants without a `#[cfg]`: a build with no Tox still compiles the word
/// `Role::Host`, it just never reaches a Tox instance with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// The founder. Creates the group and invites every seat.
    Host,
    /// A player, waiting to be invited by the founder the advertisement named.
    Joiner {
        founder: [u8; 32],
        /// The `chat_id` the advertisement named, which the joiner compares the
        /// group it is invited into against.
        chat_id: Option<[u8; 32]>,
    },
}

/// Something the roster decided, for the driver. A mirror of
/// `tox::table::Command` for the same reason as [`Role`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seat {
    /// This Tox key now holds a seat: add it, and invite it if this client is
    /// the founder.
    Took([u8; 32]),
    /// It no longer does: remove it from the group, where this client may.
    Left([u8; 32]),
    /// It is back after a restart and needs the group offered again. See
    /// `tox::table::Command::Rejoined` for why nothing else notices.
    Back([u8; 32]),
    /// A group peer's own key, and the **application** key that was verified to
    /// have signed a message from it.
    ///
    /// The bridge between two key spaces the driver cannot cross on its own: a
    /// group key identifies a peer inside one group, the roster holds a
    /// long-term key, and `tox.h` offers nothing that maps one to the other.
    /// The signature does, and every hand event carries one. `S1-I`.
    KnownAs {
        group_key: [u8; 32],
        app_key: [u8; 32],
    },
}

/// The table's game transport, when there is one.
///
/// Empty until a table forms on Tox, and empty for ever in a build without the
/// feature.
pub struct TableSink {
    #[cfg(feature = "tox")]
    inner: Option<crate::tox::table::ToxTable>,
    /// This client's own Tox public key, once an instance is running.
    ///
    /// Kept so [`start`](Self::start) can answer a second call without building
    /// a second instance. See its own note for what the second call used to do.
    #[cfg(feature = "tox")]
    mine: Option<[u8; 32]>,
    /// How far this client got in reaching the Tox network at all.
    reach: Reach,
}

/// What bootstrapping actually achieved, rather than that it was attempted.
///
/// **Every one of these calls was `let _ =`.** The relay list is the whole of
/// what makes a NATed or UDP-blocked player able to play — the owner's point,
/// and the reason `attohttpc` and `serde_json` are in this tree at all — and
/// nothing recorded whether a single relay was ever added. A run that failed
/// looked exactly like a run where the network was fine and something else was
/// wrong.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reach {
    /// Nodes in the list this client loaded.
    pub nodes: usize,
    /// Of those, how many `tox_bootstrap` accepted (the DHT, over UDP).
    pub booted: usize,
    /// How many relays were **offered**, one per node.
    ///
    /// Not how many connected: `tox_add_tcp_relay` answers `true` whenever the
    /// hostname resolved (`tox.c:1186`), so this is a DNS-success count and it
    /// says so here rather than wearing a name that promises more. What
    /// toxcore then does with them is its own — it keeps
    /// `RECOMMENDED_FRIEND_TCP_CONNECTIONS` = 3, chosen by whichever finishes
    /// its handshake first, and there is no API to influence that.
    pub relays: usize,
    /// Whether the list was re-fetched from the network on this start.
    pub refreshed: bool,
}

impl Default for TableSink {
    fn default() -> Self {
        Self::none()
    }
}

impl TableSink {
    /// How far this client got in reaching the Tox network.
    pub fn reach(&self) -> Reach {
        self.reach
    }

    /// No Tox table. The node publishes to its GossipSub mesh as before.
    pub const fn none() -> Self {
        Self {
            #[cfg(feature = "tox")]
            inner: None,
            #[cfg(feature = "tox")]
            mine: None,
                    reach: Reach {
                nodes: 0,
                booted: 0,
                relays: 0,
                refreshed: false,
            },
        }
    }

    /// Whether a table's traffic is on Tox.
    ///
    /// The one question `run.rs` asks, and it is what decides where
    /// `publish_hand` sends. In a build without the feature it is always false
    /// and the compiler removes the branch.
    pub fn is_on_tox(&self) -> bool {
        #[cfg(feature = "tox")]
        {
            self.inner.is_some()
        }
        #[cfg(not(feature = "tox"))]
        {
            false
        }
    }

    /// Start this table's Tox transport, if this build has one.
    ///
    /// Gives back **this client's own Tox public key**, which is what the
    /// advertisement's `founder_tox_key` and the join request's `tox_key`
    /// carry. `Ok(None)` means the build has no Tox and the table rides the
    /// mesh — not a failure, and the caller says nothing about it.
    ///
    /// The identity is the profile's, not a fresh one: a Tox key that changed
    /// on every start would take the table with it, because both ends add each
    /// other from keys carried in the advertisement and the join request and a
    /// Tox friendship is two-sided.
    #[allow(unused_variables)]
    /// Start the table's Tox driver, or keep the one already running.
    ///
    /// # The second call used to throw the first away
    ///
    /// This built a new `Tox` instance every time and replaced `inner`,
    /// dropping the running driver — its thread, its friendships and its place
    /// in the group — and starting over from a fresh DHT bootstrap.
    ///
    /// **The joiner path calls it on every `JoinTable`**, and a headless client
    /// asks to join every thirty seconds for as long as it is not seated. So a
    /// client that restarted and was answered *already seated* tore down and
    /// rebuilt its own Tox identity every half minute for ever, and was never in
    /// one place long enough for the founder to find it.
    ///
    /// That is what four earlier fixes were aimed at and missed. The founder's
    /// counters said `tox friends up 2` of three for a whole five-minute run
    /// while it sent fifteen invitations to the two it could reach; the peer it
    /// could not reach was resetting itself on a timer.
    ///
    /// A second call now returns the running instance's key. Changing tables
    /// goes through [`clear`](Self::clear) first, which is what `LeaveTable`
    /// already does.
    pub fn start(
        &mut self,
        profile: &Path,
        role: Role,
        group_name: &str,
        self_name: &str,
        roster: Vec<[u8; 32]>,
    ) -> Result<Option<[u8; 32]>, String> {
        #[cfg(feature = "tox")]
        {
            use crate::tox::{table, Tox};

            if self.inner.is_some() {
                return Ok(self.mine);
            }

            let secret = crate::storage::profile::load_or_create_tox_key(profile)
                .map_err(|e| format!("no tox identity: {e}"))?;
            let mut tox =
                Tox::with_secret_key(&secret).map_err(|e| format!("no tox instance: {e}"))?;
            let mine: [u8; 32] = tox.address()[..32]
                .try_into()
                .map_err(|_| "a tox address that is not an address".to_string())?;

            // The node list, refreshed at most daily, and **every node added to
            // both of toxcore's lists**: the DHT over UDP and the relay list on
            // every TCP port it advertises. The relays are not a fallback that
            // waits for UDP to fail - measured, two machines behind one router
            // reach the DHT in nine seconds and never reach each other without
            // them, because the hole punch asks the router to hairpin.
            // **Counted, because every one of these was `let _ =` and the
            // relay fallback therefore had no instrument at all.** When a whole
            // evening of runs died with `tox self tcp, tox friends up 0, invites
            // 0 sent`, nothing anywhere could say whether zero relays had been
            // added or all of them had and the friendships failed anyway — and
            // those are different faults with different fixes. A mechanism whose
            // outcome is invisible is a mechanism that fails silently, which is
            // the same defect this client has now been bitten by four times.
            let refreshed = if crate::tox::nodes::stale(profile, 0) {
                crate::tox::nodes::refresh(profile).is_ok()
            } else {
                false
            };
            let mut nodes = 0usize;
            let mut booted = 0usize;
            let mut relays = 0usize;
            for n in crate::tox::nodes::load(profile) {
                nodes += 1;
                if tox.bootstrap(&n.host, n.udp_port, &n.key).is_ok() {
                    booted += 1;
                }
                // **One port, chosen.** toxcore dedups by the relay's public
                // key, so every port after the first is a silent no-op — 24 of
                // 45 calls, measured — and a node whose first port does not
                // complete its handshake in ten seconds is wiped and never
                // retried on the others. See `nodes::best_tcp_port`.
                if let Some(port) = crate::tox::nodes::best_tcp_port(&n.tcp_ports) {
                    if tox.add_tcp_relay(&n.host, port, &n.key).is_ok() {
                        relays += 1;
                    }
                }
            }
            self.reach = Reach {
                nodes,
                booted,
                relays,
                refreshed,
            };

            let role = match role {
                Role::Host => table::Role::Host,
                Role::Joiner { founder, chat_id } => table::Role::Joiner { founder, chat_id },
            };
            self.inner = Some(table::spawn(
                tox,
                table::Setup {
                    role,
                    group_name: group_name.to_string(),
                    self_name: self_name.to_string(),
                    roster,
                },
            ));
            self.mine = Some(mine);
            Ok(Some(mine))
        }
        #[cfg(not(feature = "tox"))]
        {
            Ok(None)
        }
    }

    /// The group's `chat_id`, once there is a group.
    ///
    /// The founder puts it in the advertisement with `TableAd::on_tox`, and
    /// until it is there nobody can check that the group they were invited into
    /// is the one the table named.
    pub fn chat_id(&self) -> Option<[u8; 32]> {
        #[cfg(feature = "tox")]
        {
            self.inner.as_ref().and_then(|t| t.chat_id())
        }
        #[cfg(not(feature = "tox"))]
        {
            None
        }
    }

    /// Wait a bounded while for the group's `chat_id`.
    ///
    /// The founder needs it before it advertises, and it arrives in
    /// milliseconds — `tox_group_new` is local state — but not before `start`
    /// returns, because the driver creates the group on its own thread.
    ///
    /// **Bounded, and `None` on the timeout.** A table that advertises without
    /// a chat id is a table nobody can check the group of, which is worse than
    /// no table only if the founder pretends otherwise; a founder that hung
    /// here would be a client that stops responding because a socket did.
    #[allow(unused_variables)]
    pub async fn chat_id_ready(&mut self, within: std::time::Duration) -> Option<[u8; 32]> {
        #[cfg(feature = "tox")]
        {
            let t = self.inner.as_mut()?;
            tokio::time::timeout(within, t.wait_for_chat_id())
                .await
                .ok()
                .flatten()
        }
        #[cfg(not(feature = "tox"))]
        {
            None
        }
    }

    /// What the transport is having trouble with: fragments refused, whole
    /// messages waiting, fragments sent. All zeroes when there is no Tox.
    ///
    /// Reported rather than kept, because two stalls in a row were diagnosed
    /// from logs that said nothing: a group send toxcore refuses is the whole
    /// mechanism by which a busy table falls behind, and it was swallowed.
    pub fn trouble(&self) -> (u64, u64, u64) {
        #[cfg(feature = "tox")]
        {
            use std::sync::atomic::Ordering;
            match self.inner.as_ref() {
                Some(t) => {
                    let x = t.trouble();
                    (
                        x.refused.load(Ordering::Relaxed),
                        x.waiting.load(Ordering::Relaxed),
                        x.sent.load(Ordering::Relaxed),
                    )
                }
                None => (0, 0, 0),
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            (0, 0, 0)
        }
    }

    /// Whether the table's Tox group holds every other seat on the roster.
    ///
    /// **`true` when this build has no Tox, and that is the point.** The gate
    /// this answers is *"may hand 1 open?"*, and on a build where the hand does
    /// not ride a group there is nothing to wait for. A gate that answered
    /// `false` there would stop a table that has no reason to be stopped.
    pub fn group_is_complete(&self) -> bool {
        #[cfg(feature = "tox")]
        {
            use std::sync::atomic::Ordering;
            match self.inner.as_ref() {
                Some(t) => t.trouble().complete.load(Ordering::Relaxed),
                None => true,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            true
        }
    }

    /// What the driver last counted in the group, and what it wanted.
    ///
    /// `(0, 0)` on a build with no Tox, where there is no group to count.
    pub fn group_seen(&self) -> (u64, u64) {
        #[cfg(feature = "tox")]
        {
            use std::sync::atomic::Ordering;
            match self.inner.as_ref() {
                Some(t) => (
                    t.trouble().in_group.load(Ordering::Relaxed),
                    t.trouble().want_in_group.load(Ordering::Relaxed),
                ),
                None => (0, 0),
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            (0, 0)
        }
    }

    /// Invitations sent, invitations refused, and friends the driver believes
    /// are connected.
    ///
    /// **Three numbers because zero refusals is ambiguous**: every invitation
    /// went, or none was attempted, and a peer that never rejoins looks the
    /// same under both.
    pub fn invite_counts(&self) -> (u64, u64, u64) {
        #[cfg(feature = "tox")]
        {
            use std::sync::atomic::Ordering;
            match self.inner.as_ref() {
                Some(t) => (
                    t.trouble().invites_sent.load(Ordering::Relaxed),
                    t.trouble().invites_refused.load(Ordering::Relaxed),
                    t.trouble().friends_up.load(Ordering::Relaxed),
                ),
                None => (0, 0, 0),
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            (0, 0, 0)
        }
    }

    /// This client's own connection to the Tox network: `0` none, `1` TCP,
    /// `2` UDP, and `0` in a build without the feature.
    ///
    /// **The first question to ask of a peer nobody can reach.** A client whose
    /// own Tox never reaches the network is unreachable for a reason that has
    /// nothing to do with friendships or invitations, and from the outside the
    /// two look the same.
    pub fn tox_connection(&self) -> u64 {
        #[cfg(feature = "tox")]
        {
            use std::sync::atomic::Ordering;
            match self.inner.as_ref() {
                Some(t) => t.trouble().self_connection.load(Ordering::Relaxed),
                None => 0,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            0
        }
    }

    /// Invitations into the table's group that toxcore refused.
    ///
    /// **Separate from `trouble`, because it is a different condition with a
    /// different remedy.** Those three counters say *the transport is behind*;
    /// this one says *a seat is not in the group*, which shows at the table as
    /// a player who is seated, counted in the roster, and never dealt to. The
    /// two were indistinguishable in a log until a run at three seats put a
    /// seat outside the group for a hundred seconds while it opened hands of
    /// its own that nobody else held.
    pub fn invites_refused(&self) -> u64 {
        #[cfg(feature = "tox")]
        {
            use std::sync::atomic::Ordering;
            match self.inner.as_ref() {
                Some(t) => t.trouble().invites_refused.load(Ordering::Relaxed),
                None => 0,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            0
        }
    }

    /// Tell the driver what the roster decided. Nothing, when there is no Tox.
    #[allow(unused_variables)]
    pub fn tell(&self, seat: Seat) {
        #[cfg(feature = "tox")]
        {
            use crate::tox::table::Command;
            if let Some(t) = self.inner.as_ref() {
                t.tell(match seat {
                    Seat::Took(k) => Command::Seated(k),
                    Seat::Left(k) => Command::Unseated(k),
                    Seat::Back(k) => Command::Rejoined(k),
                    Seat::KnownAs {
                        group_key,
                        app_key,
                    } => Command::KnownAs {
                        group_key,
                        app_key,
                    },
                });
            }
        }
    }

    /// Give it up, leaving the group.
    pub fn clear(&mut self) {
        #[cfg(feature = "tox")]
        {
            // Dropping the handle tells the driver to leave and joins its
            // thread, which flushes whatever it still holds — the last message
            // of a hand is exactly what is in that queue.
            self.inner = None;
            // And the key goes with it, so a later `start` builds afresh rather
            // than answering with the key of an instance that has stopped.
            self.mine = None;
        }
    }

    /// Send one whole protocol message. **Not async, and not blocking.**
    ///
    /// `publish_hand` is called from inside the node loop, from arms that
    /// already hold the swarm, and an `await` there would be an await in the
    /// middle of handling one event. The driver's channel is bounded, so a
    /// caller that outruns it gets `false` rather than a stall — and a message
    /// dropped here is re-sent by the five-second loop that exists because no
    /// transport in this design keeps history.
    ///
    /// `false` also when there is no Tox table, which is how `publish_hand`
    /// falls back to the mesh.
    pub fn try_broadcast(&self, bytes: &[u8]) -> bool {
        #[cfg(feature = "tox")]
        {
            match self.inner.as_ref() {
                Some(t) => t.try_broadcast(bytes),
                None => false,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            let _ = bytes;
            false
        }
    }

    /// The next message from the table's group.
    ///
    /// **Never resolves when there is no Tox table**, which is what makes the
    /// `select!` branch safe to write unconditionally: a branch whose future is
    /// pending contributes nothing, in either build.
    ///
    /// Cancel-safe, because the underlying receiver is: `select!` drops this
    /// future every time another branch wins, and a `next` that lost a message
    /// on being dropped would lose it exactly when the table is busiest.
    pub async fn next(&mut self) -> Option<FromTable> {
        #[cfg(feature = "tox")]
        {
            use crate::table::transport::TableTransport;
            match self.inner.as_mut() {
                Some(t) => t.next().await,
                None => std::future::pending().await,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            std::future::pending().await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty sink says no to everything and never yields, in both builds.
    /// That is what lets `run.rs` carry the branch unconditionally.
    #[tokio::test]
    async fn an_empty_sink_is_inert() {
        let mut sink = TableSink::none();
        assert!(!sink.is_on_tox());
        assert!(!sink.try_broadcast(b"anything"));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), sink.next())
                .await
                .is_err(),
            "an empty sink must never resolve, or the select! branch would spin"
        );
    }

    /// Clearing an empty one is safe, because leaving a table happens on paths
    /// that do not all know whether there was one.
    #[test]
    fn clearing_nothing_is_safe() {
        let mut sink = TableSink::default();
        sink.clear();
        sink.clear();
        assert!(!sink.is_on_tox());
    }
}
