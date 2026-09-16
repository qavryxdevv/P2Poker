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
    /// `D-037`: the founder, back after a restart. Its group is gone with the
    /// old process; it takes an invitation from any roster member into the
    /// group the advertisement named, and nothing else.
    Back {
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
    /// `D-044`: the roster as the formation holds it now, by Tox key. The
    /// driver takes the seats it did not have and drops the ones no longer
    /// named, so its group gate wants the seats that remain.
    Roster(Vec<[u8; 32]>),
    /// `D-045`: the table's word removed a seat; see `Command::Remove`.
    Remove {
        app_key: Option<[u8; 32]>,
        tox_key: Option<[u8; 32]>,
        for_good: bool,
    },
    /// fault-harness: a founder's kick without the word (`D-045`).
    KickWithoutWord([u8; 32]),
    /// It is back after a restart and needs the group offered again. See
    /// `tox::table::Command::Rejoined` for why nothing else notices.
    Back([u8; 32]),
    /// `D-049`: this client's seat sits out (`true`) or plays again.
    Away(bool),
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
    /// `D-051`: the application keys of the seats, and whether the roster is
    /// ratified. A member whose binding names none of them is no seat.
    Seats { apps: Vec<[u8; 32]>, fixed: bool },
    /// `D-051`: a whole message from this group member was not a signed
    /// event, or did not verify under the key inside it.
    ///
    /// `D-054`: `points` is what it is worth against the member's meter, so a
    /// refusal that is merely an eager speaker (a spent chat budget) is not
    /// counted as the forgery it is not.
    Noise { member_key: [u8; 32], points: u32 },
}

/// `D-051`: a member the carrier cut off from this table's group, as the node
/// reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutOff {
    /// The application key the member's binding named, when it had one.
    pub app_key: Option<[u8; 32]>,
    pub member_key: [u8; 32],
    /// What it flooded the group with, when that is why.
    pub flood: Option<String>,
    /// Why, in words.
    pub why: String,
}

/// The table's game transport, when there is one.
///
/// Empty until a table forms on Tox, and empty for ever in a build without the
/// feature.
pub struct TableSink {
    /// `D-042`: the client's one Tox instance, started with the client and
    /// kept for its life; every table is a group on it. `D-043`: shared by
    /// every table's sink, so a second table rides the same instance.
    #[cfg(feature = "tox")]
    driver: Option<std::sync::Arc<crate::tox::table::Driver>>,
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
    /// Whether this table **asked** for a Tox carrier, whatever came of it.
    ///
    /// **`inner.is_none()` cannot tell the two apart and the difference is the
    /// whole hand.** A table with no Tox key in its advert never wanted one; a
    /// table whose `start` failed — no identity, no instance, a profile
    /// directory that would not open — wanted one and has none. The second
    /// client cannot send a hand at all, because D-019's second amendment left
    /// the hand no other channel, and it must therefore not open one.
    ///
    /// Set before the fallible steps in [`start`](Self::start) and cleared by
    /// [`clear`](Self::clear), so it says *this table's traffic belongs on a
    /// group* rather than *a group exists*.
    wants_tox: bool,
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
            driver: None,
            #[cfg(feature = "tox")]
            inner: None,
            #[cfg(feature = "tox")]
            mine: None,
            wants_tox: false,
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
    /// Whether this table asked for a Tox carrier, whether or not it got one.
    ///
    /// The pair with [`is_on_tox`](Self::is_on_tox): `wants_tox && !is_on_tox()`
    /// is a client that owes its table a channel it does not have.
    pub fn wants_tox(&self) -> bool {
        self.wants_tox
    }

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
    ///
    /// `D-042`: the instance is started here, before any table exists, and
    /// bootstrapped into the Tox DHT -- so a table opened later finds the DHT
    /// warm and its friends' links up. Called by the node at launch; `start`
    /// calls it too when a table comes first. Idempotent.
    #[allow(unused_variables)]
    pub fn boot(&mut self, profile: &Path) -> Result<Option<[u8; 32]>, String> {
        #[cfg(feature = "tox")]
        {
            use crate::tox::{table, Tox};

            if self.driver.is_some() {
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
            // one TCP port each. The relays are not a fallback that waits for
            // UDP to fail -- measured, two machines behind one router reach the
            // DHT in nine seconds and never reach each other without them.
            // **Counted**, because every one of these was `let _ =` once and a
            // whole evening of runs died with nothing to say whether a relay
            // had been added at all (`S1-U`).
            let refreshed = if crate::tox::nodes::stale(profile, 0) {
                crate::tox::nodes::refresh(profile).is_ok()
            } else {
                false
            };
            let mut nodes = 0usize;
            let mut booted = 0usize;
            let mut relays = 0usize;
            // `S1-FB`: kept, so the driver can offer them again while offline.
            let list: Vec<crate::tox::nodes::Node> = crate::tox::nodes::load(profile).into_iter().collect();
            for n in list.iter().cloned() {
                nodes += 1;
                if tox.bootstrap(&n.host, n.udp_port, &n.key).is_ok() {
                    booted += 1;
                }
                // **One port, chosen.** toxcore dedups by the relay's public
                // key, so every port after the first is a silent no-op, and a
                // node whose first port does not complete its handshake in ten
                // seconds is wiped and never retried on the others. See
                // `nodes::best_tcp_port`.
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
            self.driver = Some(std::sync::Arc::new(table::Driver::start_with_nodes(tox, list)));
            self.mine = Some(mine);
            Ok(Some(mine))
        }
        #[cfg(not(feature = "tox"))]
        {
            Ok(None)
        }
    }

    /// `D-043`: a sink for another table on the same instance -- the driver,
    /// this client's key and the reach shared, no table yet.
    pub fn share(&self) -> TableSink {
        TableSink {
            #[cfg(feature = "tox")]
            driver: self.driver.clone(),
            #[cfg(feature = "tox")]
            inner: None,
            #[cfg(feature = "tox")]
            mine: self.mine,
            reach: self.reach,
            wants_tox: false,
        }
    }

    /// Open this client's table on the instance: a group of its own, created
    /// here for a founder and awaited from an invitation otherwise.
    ///
    /// # The second call used to throw the first away
    ///
    /// This built a new `Tox` instance every time and replaced `inner`,
    /// dropping the running driver -- its thread, its friendships and its
    /// place in the group -- and starting over from a fresh DHT bootstrap.
    /// **The joiner path calls it on every `JoinTable`**, and a headless client
    /// asks to join every thirty seconds for as long as it is not seated, so a
    /// client that restarted and was answered *already seated* tore down and
    /// rebuilt its own Tox identity every half minute for ever. A second call
    /// answers with the running table. Changing tables goes through
    /// [`clear`](Self::clear) first, which is what `LeaveTable` does.
    pub fn start(
        &mut self,
        profile: &Path,
        role: Role,
        group_name: &str,
        self_name: &str,
        roster: Vec<[u8; 32]>,
        binder: Option<ed25519_dalek::SigningKey>,
    ) -> Result<Option<[u8; 32]>, String> {
        #[cfg(feature = "tox")]
        {
            use crate::tox::table;

            // Said before anything that can fail: the claim is that this table's
            // traffic belongs on a group, not that one was built.
            self.wants_tox = true;
            if self.inner.is_some() {
                return Ok(self.mine);
            }
            if self.driver.is_none() {
                self.boot(profile)?;
            }
            let Some(driver) = self.driver.as_ref() else {
                return Err("no tox instance".into());
            };
            let role = match role {
                Role::Host => table::Role::Host,
                Role::Joiner { founder, chat_id } => table::Role::Joiner { founder, chat_id },
                Role::Back { chat_id } => table::Role::Back { chat_id },
            };
            self.inner = Some(driver.open(table::Setup {
                role,
                group_name: group_name.to_string(),
                self_name: self_name.to_string(),
                roster,
                binder,
            }));
            Ok(self.mine)
        }
        #[cfg(not(feature = "tox"))]
        {
            let _ = (profile, role, group_name, self_name, roster, binder);
            Ok(None)
        }
    }

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
                // **`true` here is only honest when the table never wanted a
                // group.** The comment above justifies exactly one state — a
                // build with no Tox — and this arm is reached in a build that
                // has it, by a table whose `start` failed after its advert had
                // already named a group. That client is not on a group the hand
                // rides, and answering *complete* let it open a hand it could
                // not send anywhere.
                None => !self.wants_tox,
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

    /// Hand events received and thrown away because the node loop was not
    /// draining. Any non-zero value is a seat diverging from the table.
    /// `D-035`: seats whose client left the table's group since the last
    /// call, each with whether it quit on purpose.
    pub fn take_gone(&self) -> Vec<([u8; 32], bool)> {
        #[cfg(feature = "tox")]
        {
            match self.inner.as_ref() {
                Some(t) => t
                    .trouble()
                    .gone
                    .lock()
                    .map(|mut g| std::mem::take(&mut *g))
                    .unwrap_or_default(),
                None => Vec::new(),
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            Vec::new()
        }
    }

    /// `D-035`: whether a seat's client is a confirmed member of the table's
    /// group right now -- the reading the window's link indicator is made of
    /// once the hand rides the group.
    pub fn in_group(&self, app_key: &[u8; 32]) -> bool {
        #[cfg(feature = "tox")]
        {
            match self.inner.as_ref() {
                Some(t) => t
                    .trouble()
                    .present
                    .lock()
                    .map(|p| p.contains(app_key))
                    .unwrap_or(false),
                None => false,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            let _ = app_key;
            false
        }
    }

    /// `D-041`: whether the friend connection to this Tox key is up right
    /// now -- on the line, whether or not it has spoken in the group yet.
    /// `S1-DS`: whether the group has taught this table who this application
    /// key is -- a seat that has spoken there. Such a seat not in the group
    /// now is gone, whatever its friend link says.
    pub fn known(&self, app_key: &[u8; 32]) -> bool {
        #[cfg(feature = "tox")]
        {
            match self.inner.as_ref() {
                Some(t) => t.trouble().known.lock().map(|k| k.contains(app_key)).unwrap_or(false),
                None => false,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            let _ = app_key;
            false
        }
    }

    /// `D-049`: whether the table's group says this seat sits out -- its
    /// status as its most recently heard entry holds it. `false` when it is
    /// not a present member: a seat off the line is not said to sit out.
    pub fn away(&self, app_key: &[u8; 32]) -> bool {
        #[cfg(feature = "tox")]
        {
            match self.inner.as_ref() {
                Some(t) => t.trouble().away.lock().map(|a| a.contains(app_key)).unwrap_or(false),
                None => false,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            let _ = app_key;
            false
        }
    }

    /// `D-049`: the same by the Tox key of the line the member came in over.
    pub fn away_line(&self, tox_key: &[u8; 32]) -> bool {
        #[cfg(feature = "tox")]
        {
            match self.inner.as_ref() {
                Some(t) => t.trouble().away_lines.lock().map(|a| a.contains(tox_key)).unwrap_or(false),
                None => false,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            let _ = tox_key;
            false
        }
    }

    /// `S1-DT`: how many seconds ago the group last heard from this seat,
    /// `None` when it is not a present member.
    pub fn quiet_secs(&self, app_key: &[u8; 32]) -> Option<u64> {
        #[cfg(feature = "tox")]
        {
            match self.inner.as_ref() {
                Some(t) => t.trouble().quiet.lock().ok().and_then(|q| q.get(app_key).copied()),
                None => None,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            let _ = app_key;
            None
        }
    }

    /// `S1-DV`: seats whose client left the table's group since the last
    /// call, by the TOX key their invitation came over (`patches/0033`),
    /// each with whether it quit on purpose -- said before the group has
    /// taught any application key, so before the first hand.
    pub fn take_gone_lines(&self) -> Vec<([u8; 32], bool)> {
        #[cfg(feature = "tox")]
        {
            match self.inner.as_ref() {
                Some(t) => t
                    .trouble()
                    .gone_lines
                    .lock()
                    .map(|mut g| std::mem::take(&mut *g))
                    .unwrap_or_default(),
                None => Vec::new(),
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            Vec::new()
        }
    }

    /// `S1-DV`: whether the friend with this Tox key is a confirmed member
    /// of the table's group right now, by the line its invitation came over.
    pub fn in_group_line(&self, tox_key: &[u8; 32]) -> bool {
        #[cfg(feature = "tox")]
        {
            match self.inner.as_ref() {
                Some(t) => t
                    .trouble()
                    .present_lines
                    .lock()
                    .map(|p| p.contains(tox_key))
                    .unwrap_or(false),
                None => false,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            let _ = tox_key;
            false
        }
    }

    /// `S1-DV`: how many seconds ago the group last heard from the member
    /// that came in over this Tox key, `None` when it is not a present member.
    pub fn quiet_line(&self, tox_key: &[u8; 32]) -> Option<u64> {
        #[cfg(feature = "tox")]
        {
            match self.inner.as_ref() {
                Some(t) => t.trouble().quiet_lines.lock().ok().and_then(|q| q.get(tox_key).copied()),
                None => None,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            let _ = tox_key;
            None
        }
    }

    pub fn friend_up(&self, tox_key: &[u8; 32]) -> bool {
        #[cfg(feature = "tox")]
        {
            match self.inner.as_ref() {
                Some(t) => t
                    .trouble()
                    .friends_on
                    .lock()
                    .map(|p| p.contains(tox_key))
                    .unwrap_or(false),
                None => false,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            let _ = tox_key;
            false
        }
    }

    pub fn inbox_dropped(&self) -> u64 {
        #[cfg(feature = "tox")]
        {
            use std::sync::atomic::Ordering;
            match self.inner.as_ref() {
                Some(t) => t.trouble().inbox_dropped.load(Ordering::Relaxed),
                None => 0,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            0
        }
    }

    /// `S1-DW`: claims refused by the table's group -- a member's signed
    /// traffic saying it is a seat that another confirmed member holds and
    /// has spoken from lately. Any non-zero value is a member saying another
    /// seat's message again as its own first word.
    pub fn claims_refused(&self) -> u64 {
        #[cfg(feature = "tox")]
        {
            use std::sync::atomic::Ordering;
            match self.inner.as_ref() {
                Some(t) => t.trouble().claims_refused.load(Ordering::Relaxed),
                None => 0,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            0
        }
    }

    /// `D-045`: members this client dropped from the table's group by the
    /// table's word.
    pub fn removed(&self) -> u64 {
        #[cfg(feature = "tox")]
        {
            use std::sync::atomic::Ordering;
            match self.inner.as_ref() {
                Some(t) => t.trouble().removed.load(Ordering::Relaxed),
                None => 0,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            0
        }
    }

    /// `D-045`: the times this client was itself removed from the table's
    /// group by the table's word.
    pub fn kicked_out(&self) -> u64 {
        #[cfg(feature = "tox")]
        {
            use std::sync::atomic::Ordering;
            match self.inner.as_ref() {
                Some(t) => t.trouble().kicked_out.load(Ordering::Relaxed),
                None => 0,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            0
        }
    }

    /// **Why** the refusals happened, by `Tox_Err_Group_Send_Custom_Packet`:
    /// index 1 group-not-found, 2 too-long, 3 empty, 4 disconnected,
    /// 5 fail-send. Index 0 is unused.
    ///
    /// A bare count of refusals cannot be acted on. Code 4 is decided inside
    /// `tox_group_send_custom_packet` before any peer is consulted -- this
    /// client's own group connection is down -- and code 5 comes from the peer
    /// loop, which is where `patches/0003` lives. The remedies have nothing in
    /// common.
    pub fn refused_why(&self) -> [u64; 6] {
        #[cfg(feature = "tox")]
        {
            use std::sync::atomic::Ordering;
            match self.inner.as_ref() {
                Some(t) => {
                    let w = &t.trouble().refused_why;
                    let mut out = [0u64; 6];
                    for (i, slot) in out.iter_mut().enumerate() {
                        *slot = w[i].load(Ordering::Relaxed);
                    }
                    out
                }
                None => [0; 6],
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            [0; 6]
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

    /// **Stalled joins this client gave up and started again, and joins
    /// toxcore itself abandoned.**
    ///
    /// Zero on a healthy run, and the two are different things: the first is
    /// this client noticing that `self_join` never fired, the second is the
    /// library saying so. `S1-AA` shape (i) — a seat that holds a group number
    /// nine steps short of being in the group, whose inviter entry is reaped
    /// after twelve seconds with no callback and no log, and for which
    /// libtoxcore has no path back.
    pub fn join_trouble(&self) -> (u64, u64, u64, u64) {
        #[cfg(feature = "tox")]
        {
            use std::sync::atomic::Ordering;
            match self.inner.as_ref() {
                Some(t) => (
                    t.trouble().rejoins.load(Ordering::Relaxed),
                    t.trouble().join_fails.load(Ordering::Relaxed),
                    t.trouble().confirmed_peers.load(Ordering::Relaxed),
                    t.trouble().founder_link.load(Ordering::Relaxed),
                ),
                None => (0, 0, 0, 3),
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            (0, 0, 0, 3)
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
                    Seat::Roster(keys) => Command::Roster(keys),
                    Seat::Remove { app_key, tox_key, for_good } => Command::Remove { app_key, tox_key, for_good },
                    Seat::KickWithoutWord(k) => Command::KickWithoutWord(k),
                    Seat::Back(k) => Command::Rejoined(k),
                    Seat::Away(on) => Command::Away(on),
                    Seat::KnownAs {
                        group_key,
                        app_key,
                    } => Command::KnownAs {
                        group_key,
                        app_key,
                    },
                    Seat::Seats { apps, fixed } => Command::Seats { apps, fixed },
                    Seat::Noise { member_key, points } => Command::Noise { member_key, points },
                });
            }
        }
    }

    /// `D-051`: the members the carrier cut off since the node last asked.
    pub fn take_cut_offs(&self) -> Vec<CutOff> {
        #[cfg(feature = "tox")]
        {
            use crate::tox::table::Cut;
            let Some(t) = self.inner.as_ref() else {
                return Vec::new();
            };
            let Ok(mut said) = t.trouble().cut_off.lock() else {
                return Vec::new();
            };
            std::mem::take(&mut *said)
                .into_iter()
                .map(|c| CutOff {
                    app_key: c.app_key,
                    member_key: c.member_key,
                    flood: match &c.why {
                        Cut::Flood(f) => Some(f.to_string()),
                        _ => None,
                    },
                    why: c.why.to_string(),
                })
                .collect()
        }
        #[cfg(not(feature = "tox"))]
        {
            Vec::new()
        }
    }

    /// `D-051`: messages held back from a member that was filling the inbox.
    pub fn inbox_yielded(&self) -> u64 {
        #[cfg(feature = "tox")]
        {
            use std::sync::atomic::Ordering;
            match self.inner.as_ref() {
                Some(t) => t.trouble().inbox_yielded.load(Ordering::Relaxed),
                None => 0,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            0
        }
    }

    /// **Ask a seat for the message a stage is waiting on (`patches/0011`).**
    ///
    /// A no-op without a Tox carrier, and a no-op at the driver when the seat
    /// is not a group peer yet or has nothing outstanding. Safe to call every
    /// tick: toxcore throttles the request it turns into to one per second per
    /// connection.
    #[allow(unused_variables)]
    pub fn nudge(&self, app_key: [u8; 32], seat: u8) {
        #[cfg(feature = "tox")]
        {
            if let Some(t) = self.inner.as_ref() {
                t.tell(crate::tox::table::Command::Nudge { app_key, seat });
            }
        }
    }

    /// **Which seats the carrier is still delivering from, one bit per seat.**
    ///
    /// Refreshed by the driver whenever `nudge` asks about a seat, which the
    /// stall tick does for exactly the seats a stage is waiting on. Zero
    /// without a Tox carrier, which makes the vote behave as it always did.
    pub fn mid_delivery(&self) -> u32 {
        #[cfg(feature = "tox")]
        {
            use std::sync::atomic::Ordering;
            match self.inner.as_ref() {
                Some(t) => t.trouble().mid_delivery.load(Ordering::Relaxed),
                None => 0,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            0
        }
    }

    /// Give the table up: what it still holds is said, its group is left,
    /// and the friends no other open table needs go after `FRIEND_LINGER`
    /// (`D-042`). The instance and this client's key stay for the next
    /// table.
    pub fn clear(&mut self) {
        #[cfg(feature = "tox")]
        {
            self.inner = None;
            self.wants_tox = false;
        }
    }

    /// `S1-GO`: give the table up as [`TableSink::clear`] does, but stay in the
    /// table's group for `linger` first. A member that got this client's later
    /// messages and missed an earlier one asks this client for it, and a client
    /// gone from the group answers nobody: two players left a table of ten the
    /// moment it dealt, their openings of hand #1 reached some seats and not
    /// others, and the seats split between two stages of the hand
    /// (`churn193840-10`). Nothing reads the table's inbox meanwhile; the driver
    /// drops what does not fit.
    pub fn clear_lingering(&mut self, linger: std::time::Duration) {
        #[cfg(feature = "tox")]
        {
            if let Some(table) = self.inner.take() {
                tokio::spawn(async move {
                    tokio::time::sleep(linger).await;
                    drop(table);
                });
            }
            self.wants_tox = false;
        }
        #[cfg(not(feature = "tox"))]
        {
            let _ = linger;
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
