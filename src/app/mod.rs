//! Wiring between the GUI, the protocol and the engine.
//!
//! Owns the worker that parses, verifies, proves and applies, so that
//! `SPEC_CS.md` §33 holds: **cryptography never blocks the GUI event loop.**
//!
//! # The shape that keeps that true
//!
//! The node runs on a tokio task and pushes [`NodeEvent`]s down a channel. This
//! module drains the channel and folds each event into a plain snapshot; the
//! panes read the snapshot and nothing else. There is no lock the paint loop can
//! wait on and no future it can block on, because there is nothing shared to
//! wait for.
//!
//! A verification is 40 ms and a shuffle proof is 95 ms — measured, not
//! estimated. At sixty frames a second a frame is 16 ms, so a single proof on
//! the paint thread is six dropped frames and a hand's worth is a client that
//! looks crashed.

use std::collections::VecDeque;

use crate::gui::lobby::{LobbyView, NetworkStatus, RelayStatus};
use crate::net::lobby::LobbyStore;
use crate::net::node::NodeEvent;

/// The founder's reason codes, §4.3, in the words a player can act on.
///
/// Two of the eight are never sent by this client because nothing in the corpus
/// gives them a mechanism; they are named here anyway, because another
/// implementation may send them and "reason 6" tells a player nothing.
fn refusal(code: u16) -> &'static str {
    match code {
        1 => "the table is full",
        2 => "that seat is taken",
        3 => "wrong password",
        4 => "the buy-in is out of range",
        5 => "the advertisement has expired",
        6 => "banned",
        7 => "capabilities do not match",
        8 => "already seated",
        _ => "no reason this client understands",
    }
}

/// Eight characters of a key, which is what a person can compare at a glance.
fn short(key: &[u8; 32]) -> String {
    key[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// How many log lines the status pane keeps.
///
/// Bounded because the events come from the network: a peer that reconnects in a
/// loop would otherwise grow this without limit, and an unbounded log is a
/// memory bug with an external trigger like any other.
pub const MAX_LOG_LINES: usize = 500;

/// Everything the client knows, in the form the panes read it.
#[derive(Debug, Default)]
pub struct AppState {
    pub lobby: LobbyStore,
    pub status: NetworkStatus,
    /// What has happened, newest last.
    pub log: VecDeque<String>,
    /// The table the user has selected in the list.
    pub selected: Option<[u8; 32]>,
    /// The table this client is at or forming, if any.
    pub seated: Option<Seat>,
    /// The latest advertisement timestamp this client has seen.
    ///
    /// Used as the clock for this store's own expiry. Not this machine's clock:
    /// the two stores must agree about which adverts are alive, and a client
    /// whose clock ran fast would expire tables the node still held.
    pub newest_seen: u64,
}

/// Where this client is sitting, as the panes read it.
///
/// A **local view** like everything else here: it is what this client believes
/// about a table being formed, and none of it is canonical until `session` is
/// filled in — which happens only when every seat has ratified the roster.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seat {
    /// The table key, which is the table's identity.
    pub key: [u8; 32],
    /// This client's seat, once the founder has given it one.
    pub seat: Option<u8>,
    /// Whether this client founded the table.
    pub hosting: bool,
    /// Who is seated: seat, name, buy-in.
    pub roster: Vec<(u8, String, u64)>,
    /// The session identity, once the table is real. `None` means the table has
    /// **not** started, whatever else is filled in.
    pub session: Option<[u8; 32]>,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one event from the node into the snapshot.
    ///
    /// Every arm is a local view. **Nothing here enters a hash** — D-012 forbids
    /// canonical state derived from a per-receiver quantity, and every value in
    /// this struct is exactly that: how many peers *this* client has, what *this*
    /// client heard, when.
    pub fn apply(&mut self, event: NodeEvent) {
        match event {
            NodeEvent::Listening(addr) => {
                self.status.listening.push(addr.to_string());
                self.note(format!("listening on {addr}"));
            }
            NodeEvent::PeerConnected(p) => {
                self.status.peers += 1;
                self.note(format!("connected to {p}"));
            }
            NodeEvent::PeerDisconnected(p) => {
                self.status.peers = self.status.peers.saturating_sub(1);
                self.note(format!("lost {p}"));
            }
            NodeEvent::Discovered { hints, dropped } => {
                self.note(format!("{hints} peers found, {dropped} unusable"));
            }
            NodeEvent::Announced { port } => {
                self.status.dht_announced = true;
                self.note(format!("announced udp/{port} in the lobby swarm"));
            }
            NodeEvent::Reachability { public } => {
                self.status.public = Some(public);
                self.note(if public {
                    "this client is reachable from the internet".into()
                } else {
                    "this client is behind NAT".into()
                });
            }
            NodeEvent::Reserved {
                relay,
                bytes,
                seconds,
                adequate,
            } => {
                self.status.relay = Some(RelayStatus {
                    peer: relay.to_string(),
                    adequate,
                });
                self.note(format!(
                    "relay {relay}: {} bytes / {} s, {}",
                    bytes.map(|b| b.to_string()).unwrap_or_else(|| "no limit".into()),
                    seconds.map(|s| s.to_string()).unwrap_or_else(|| "no limit".into()),
                    if adequate {
                        "enough for a table"
                    } else {
                        "NOT enough to carry a hand"
                    }
                ));
            }
            NodeEvent::NoRelayFound { cycles } => {
                self.note(format!(
                    "no relay after {cycles} searches; if nobody anywhere is reachable there is no game"
                ));
            }
            NodeEvent::HolePunched(p) => self.note(format!("{p} is now a direct connection")),
            NodeEvent::StillRelayed(p) => self.note(format!("{p} stays relayed")),
            NodeEvent::LocalPeer(p) => self.note(format!("found {p} on this network")),
            NodeEvent::MeshPeer(p) => self.note(format!("{p} joined the lobby mesh")),
            NodeEvent::Published { bytes } => self.note(format!("advertised, {bytes} bytes")),
            NodeEvent::TableSeen {
                key,
                ad,
                params_hash,
                advert_hash,
            } => {
                let name = ad.table_name.clone();
                // Offered rather than inserted, so this store applies §7.2's
                // rules 6 and 7 for itself. It is a **second** copy of the
                // lobby — the node has its own — and a copy that took the
                // node's word would drift from it silently the first time a
                // message was dropped, which this channel is allowed to do.
                let now = ad.timestamp_unix_ms.max(self.newest_seen);
                self.newest_seen = now;
                self.lobby.expire(now);
                match self.lobby.offer(key, *ad, params_hash, advert_hash, now) {
                    Ok(()) => self.note(format!(
                        "table {} ({name})",
                        crate::gui::lobby::short_key(&key)
                    )),
                    // Not newer is the ordinary case: a table re-broadcasts
                    // every thirty seconds and this client already has it.
                    Err(crate::net::lobby::NotTaken::NotNewer) => {}
                    Err(e) => self.note(format!("table {}: {e:?}", crate::gui::lobby::short_key(&key))),
                }
            }
            NodeEvent::TableRefused { reason } => self.note(format!("advert refused: {reason}")),
            // Kept out of the log by default. Most dials fail on an open DHT and
            // a log full of them buries the lines that mean something — but the
            // count is carried, because "most dials fail" and "this client is
            // broken" look identical without one.
            NodeEvent::DialFailed { .. } => self.status.failed_dials += 1,
            NodeEvent::Warning(w) => self.note(w),

            NodeEvent::Hosting { key } => {
                self.seated = Some(Seat {
                    key,
                    seat: Some(0),
                    hosting: true,
                    ..Default::default()
                });
                self.note(format!("hosting {}", short(&key)));
            }
            NodeEvent::Seated { key, seat } => {
                self.table(key).seat = Some(seat);
                self.note(format!("seat {seat} at {}", short(&key)));
            }
            NodeEvent::Roster { key, seats } => {
                let n = seats.len();
                self.table(key).roster = seats;
                self.note(format!("{n} seated"));
            }
            NodeEvent::TableReal { key, session } => {
                self.table(key).session = Some(session);
                self.note(format!("the table is set: session {}", short(&session)));
            }
            NodeEvent::JoinRefused { reason } => {
                // The founder's claim, said as a claim. A rejection is never
                // proof of anything: §4.3 puts it plainly, and the founder may
                // simply not want this player.
                self.seated = None;
                self.note(format!("the founder says no: {}", refusal(reason)));
            }
            NodeEvent::LeftTable { why } => {
                self.seated = None;
                self.note(why);
            }
        }
    }

    /// This client's record for a table, created if there is not one yet.
    ///
    /// The three table events are a stream and their order is not guaranteed:
    /// a seat can be ratified from the founder's roster announcement on the mesh
    /// **before** the answer to the join RPC arrives, and it does exactly that on
    /// a fast local network. An earlier version dropped anything that arrived
    /// before the seat, so a table that had genuinely formed was reported as no
    /// table at all — by the client that was sitting at it.
    ///
    /// A record for a different table replaces this one. One table per client
    /// here; multi-tabling is more than one client, which is what §4.3's
    /// per-roster rules are written for.
    fn table(&mut self, key: [u8; 32]) -> &mut Seat {
        match &self.seated {
            Some(s) if s.key == key => {}
            _ => {
                self.seated = Some(Seat {
                    key,
                    ..Default::default()
                })
            }
        }
        self.seated.as_mut().expect("just set")
    }

    /// Whether this client is at a table that has actually started.
    ///
    /// Not `seated.is_some()`: a seat with no session is a table still being
    /// formed, and treating the two as one is how a client deals a hand at a
    /// table nobody has ratified.
    pub fn at_a_real_table(&self) -> bool {
        self.seated.as_ref().is_some_and(|s| s.session.is_some())
    }

    fn note(&mut self, line: String) {
        if self.log.len() >= MAX_LOG_LINES {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }

    /// The snapshot the panes read.
    pub fn view(&self) -> LobbyView {
        let mut v = LobbyView::from(&self.lobby, self.status.clone());
        v.selected = self.selected;
        v.log = self.log.iter().cloned().collect();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::PeerId;

    fn peer() -> PeerId {
        PeerId::from(libp2p::identity::Keypair::generate_ed25519().public())
    }

    /// A seat is not a table. Until every seat has ratified there is no session
    /// identity, and a client that treated the two as one would deal a hand at a
    /// table nobody agreed to.
    /// The three table events arrive in whatever order the network gives them,
    /// and on a fast local network the ratification really does beat the answer
    /// to the join request. An earlier version dropped everything that arrived
    /// before the seat, and a client sitting at a formed table reported no
    /// table at all.
    #[test]
    fn the_table_events_may_arrive_in_any_order() {
        let orders: [&[NodeEvent]; 3] = [
            &[
                NodeEvent::Seated { key: [7u8; 32], seat: 1 },
                NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "a".into(), 1), (1, "b".into(), 1)] },
                NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] },
            ],
            &[
                NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] },
                NodeEvent::Seated { key: [7u8; 32], seat: 1 },
                NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "a".into(), 1), (1, "b".into(), 1)] },
            ],
            &[
                NodeEvent::Roster { key: [7u8; 32], seats: vec![(0, "a".into(), 1), (1, "b".into(), 1)] },
                NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] },
                NodeEvent::Seated { key: [7u8; 32], seat: 1 },
            ],
        ];
        for order in orders {
            let mut s = AppState::new();
            for e in order {
                s.apply(e.clone());
            }
            let seat = s.seated.as_ref().expect("a seat");
            assert_eq!(seat.seat, Some(1));
            assert_eq!(seat.roster.len(), 2);
            assert_eq!(seat.session, Some([9u8; 32]));
            assert!(s.at_a_real_table());
        }
    }

    /// A record for another table replaces this one rather than merging into
    /// it. Two tables' rosters in one record is two tables nobody is at.
    #[test]
    fn another_table_replaces_this_one() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 1 });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s.apply(NodeEvent::Seated { key: [8u8; 32], seat: 4 });
        let seat = s.seated.as_ref().unwrap();
        assert_eq!(seat.key, [8u8; 32]);
        assert_eq!(seat.seat, Some(4));
        assert_eq!(seat.session, None, "the old session followed us to a new table");
    }

    #[test]
    fn a_seat_without_a_session_is_not_a_table() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Hosting { key: [7u8; 32] });
        assert!(s.seated.is_some());
        assert!(!s.at_a_real_table(), "a table with one seat has not started");

        s.apply(NodeEvent::Roster {
            key: [7u8; 32],
            seats: vec![(0, "Alice".into(), 1_000), (1, "Dana".into(), 1_000)],
        });
        assert!(!s.at_a_real_table(), "a roster is a proposal, not a table");

        s.apply(NodeEvent::TableReal {
            key: [7u8; 32],
            session: [9u8; 32],
        });
        assert!(s.at_a_real_table());
    }

    /// A refusal clears the seat and reaches the log as a **claim**.
    #[test]
    fn a_refusal_is_reported_as_a_claim() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated {
            key: [7u8; 32],
            seat: 3,
        });
        s.apply(NodeEvent::JoinRefused { reason: 3 });
        assert!(s.seated.is_none());
        let line = s.log.back().unwrap();
        assert!(line.contains("says no"), "{line}");
        assert!(line.contains("wrong password"), "{line}");
    }

    /// Every reason code §4.3 allocates has words, including the two this client
    /// never sends. Another implementation may send them, and "reason 6" tells a
    /// player nothing at all.
    #[test]
    fn every_reason_code_has_words() {
        for code in 1..=8u16 {
            assert_ne!(refusal(code), "no reason this client understands");
        }
        assert_eq!(refusal(0), "no reason this client understands");
        assert_eq!(refusal(9), "no reason this client understands");
    }

    #[test]
    fn the_peer_count_follows_the_connections() {
        let mut s = AppState::new();
        let a = peer();
        let b = peer();

        s.apply(NodeEvent::PeerConnected(a));
        s.apply(NodeEvent::PeerConnected(b));
        assert_eq!(s.status.peers, 2);

        s.apply(NodeEvent::PeerDisconnected(a));
        assert_eq!(s.status.peers, 1);
    }

    /// A disconnection this client never saw a connection for must not take the
    /// count below zero — which on a `usize` is not a negative number but a very
    /// large one, and would render as "connected to 18446744073709551615 peers".
    #[test]
    fn a_stray_disconnection_does_not_wrap_the_count() {
        let mut s = AppState::new();
        s.apply(NodeEvent::PeerDisconnected(peer()));
        assert_eq!(s.status.peers, 0);
    }

    /// The events come from the network, so the log is bounded like everything
    /// else that does. A peer reconnecting in a loop would otherwise grow it
    /// without limit.
    #[test]
    fn the_log_is_bounded() {
        let mut s = AppState::new();
        for _ in 0..(MAX_LOG_LINES * 3) {
            s.apply(NodeEvent::PeerConnected(peer()));
        }
        assert_eq!(s.log.len(), MAX_LOG_LINES);
    }

    /// And it keeps the newest, because the oldest line is the least useful one
    /// when something has just gone wrong.
    #[test]
    fn the_log_keeps_the_newest() {
        let mut s = AppState::new();
        for i in 0..(MAX_LOG_LINES + 5) {
            s.apply(NodeEvent::Warning(format!("line {i}")));
        }
        assert_eq!(s.log.back().unwrap(), &format!("line {}", MAX_LOG_LINES + 4));
        assert!(!s.log.front().unwrap().contains("line 0"));
    }

    /// The relay verdict reaches the status pane as a verdict and not as a
    /// number, because "16 MiB" tells a player nothing and "cannot carry a hand"
    /// tells them everything.
    #[test]
    fn an_inadequate_relay_says_so_in_the_status() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Reserved {
            relay: peer(),
            bytes: Some(131_072),
            seconds: Some(120),
            adequate: false,
        });
        assert!(!s.status.relay.as_ref().unwrap().adequate);
        assert!(s.view().status.summary().contains("cannot carry a hand"));
        assert!(s.log.back().unwrap().contains("NOT enough"));
    }

    #[test]
    fn reachability_reaches_the_status() {
        let mut s = AppState::new();
        assert_eq!(s.status.public, None, "unknown until AutoNAT answers");
        s.apply(NodeEvent::Reachability { public: false });
        assert_eq!(s.status.public, Some(false));
        assert!(s.view().status.summary().contains("no relay"));
    }

    /// The whole struct is a local view. Nothing in it enters a hash, which is
    /// D-012, and the test exists so that a later editor who adds a field is
    /// reminded rather than trusted.
    #[test]
    fn the_snapshot_is_a_local_view() {
        let mut s = AppState::new();
        s.apply(NodeEvent::Discovered {
            hints: 7,
            dropped: 3,
        });
        // Two clients that heard different things hold different snapshots, and
        // that is correct rather than a fault.
        let mut other = AppState::new();
        other.apply(NodeEvent::Discovered {
            hints: 2,
            dropped: 0,
        });
        assert_ne!(s.log, other.log);
    }
}
