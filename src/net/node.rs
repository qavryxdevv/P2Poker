//! The node's event loop: listen, announce, discover, dial, gossip.
//!
//! This is where [`swarm`](super::swarm)'s behaviours, [`dht`](super::dht)'s
//! global discovery and [`lobby`](super::lobby)'s admission rules are driven.
//! It is transport plumbing and decides no poker rule.
//!
//! # The two-step that D-003 asks for
//!
//! The acceptance criterion is that a client anywhere on the internet sees the
//! open tables, and it takes both discovery layers to get there:
//!
//! 1. **Mainline** answers *where to try*. Announce under the fixed
//!    `LOBBY_INFOHASH`, ask for peers, and get back addresses — of poker clients
//!    and of anything else that happens to share the swarm.
//! 2. **libp2p** settles who is actually there. A dial either completes a Noise
//!    or TLS handshake with a peer that speaks `/p2p-poker/1`, or it does not,
//!    and only then does the peer join the GossipSub mesh where adverts live.
//!
//! Most dials in step 2 fail, and that is the expected case rather than an
//! error: the DHT list is a hint, and D-003 is satisfied by the ones that
//! succeed.
//!
//! # Why the loop owns the stores
//!
//! [`LobbyStore`] and [`RateLimiter`] are per-node state that only this loop
//! writes. Handing them out behind a lock would buy nothing — every write is on
//! this task — and would make the ordering of two writes a question somebody
//! could get wrong.

use std::collections::HashSet;
use std::net::SocketAddrV4;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use libp2p::{gossipsub, multiaddr::Protocol, Multiaddr, PeerId};

use super::lobby::{LobbyStore, RateLimiter};
use crate::protocol::constants::AD_REBROADCAST_MS;

/// What the loop reports upwards, for the GUI and the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeEvent {
    /// A transport address this node is reachable on.
    Listening(Multiaddr),
    /// A peer completed a handshake and speaks this protocol.
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    /// The DHT returned addresses. Most will not be poker clients.
    Discovered { hints: usize, dropped: usize },
    /// An advert was accepted into the lobby.
    TableSeen { key: [u8; 32] },
    /// An advert was refused, with the reason as text for the log.
    TableRefused { reason: String },
    /// AutoNAT decided.
    Reachability { public: bool },
    /// Something in the transport went wrong and the node carried on.
    ///
    /// Separate from [`TableRefused`](NodeEvent::TableRefused), which is a
    /// finding about somebody else's advert. Reporting a failed DHT announce as
    /// a refused table would have told a user that a peer misbehaved when in
    /// fact this node had not finished starting up.
    Warning(String),
    /// The announce under the lobby infohash succeeded.
    Announced { port: u16 },
    /// A dial did not complete.
    ///
    /// Reported rather than discarded, because *most dials fail* is a true
    /// statement about an open DHT and an indistinguishable statement about a
    /// node that is quietly broken. Without this line the two look identical in
    /// the log — which is how the first run of this binary looked like it was
    /// working.
    DialFailed { reason: String },
    /// A peer was found on the local network.
    LocalPeer(PeerId),
    /// A peer subscribed to a topic this node also holds.
    ///
    /// Worth its own line: until one does, `publish` returns
    /// `NoPeersSubscribedToTopic` and a table this node is offering reaches
    /// nobody. That is the ordinary state at start-up and is indistinguishable,
    /// in a log without this event, from an advert that is silently broken.
    MeshPeer(PeerId),
    /// This node's own advert went out.
    Published { bytes: usize },
    /// A relay accepted a reservation, and what it will carry.
    ///
    /// The limits are **read**, not assumed: the protocol reports the server's
    /// real ones back precisely so the decision can be made before committing,
    /// and `NAT_AND_DISCOVERY.md` says in so many words to use them.
    Reserved {
        relay: PeerId,
        bytes: Option<u64>,
        seconds: Option<u64>,
        adequate: bool,
    },
    /// A relayed connection was upgraded to a direct one by hole punching.
    HolePunched(PeerId),
    /// Hole punching gave up. The connection stays relayed, which is a normal
    /// steady state and not an error — upstream allows three attempts and then
    /// stops.
    StillRelayed(PeerId),
}

/// Turn a discovered `SocketAddrV4` into something the swarm can dial.
///
/// QUIC only for a discovered peer. The Mainline announce carries **one** port
/// and this node announces its QUIC port, so a discovered address means QUIC and
/// nothing else — constructing a TCP dial from it would be a guess, and a guess
/// that fails looks exactly like a peer being offline.
pub fn quic_dial_addr(addr: SocketAddrV4) -> Multiaddr {
    Multiaddr::empty()
        .with(Protocol::Ip4(*addr.ip()))
        .with(Protocol::Udp(addr.port()))
        .with(Protocol::QuicV1)
}

/// How often to re-publish this node's own adverts.
///
/// Half the advert TTL, so one lost re-broadcast does not expire a table. That
/// relationship is asserted at compile time in `protocol::constants` rather than
/// left to two numbers that happen to agree today.
pub const REBROADCAST: Duration = Duration::from_millis(AD_REBROADCAST_MS);

/// Milliseconds since the Unix epoch, or `0` if the clock is before it.
///
/// A local view and never canonical state (D-012): two honest peers may read
/// different values, so nothing derived from this may enter a hash.
pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The per-node state the loop owns.
pub struct NodeState {
    pub lobby: LobbyStore,
    pub limits: RateLimiter,
    /// Peers already dialled this session, so a repeated DHT batch does not
    /// re-dial the whole list every ten minutes.
    dialled: HashSet<SocketAddrV4>,
    /// Whether AutoNAT has confirmed this node is reachable, which is what D-002
    /// gates relay volunteering on.
    public: bool,
}

impl Default for NodeState {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeState {
    pub fn new() -> Self {
        NodeState {
            lobby: LobbyStore::new(),
            limits: RateLimiter::new(),
            dialled: HashSet::new(),
            public: false,
        }
    }

    /// Which of a discovered batch are worth dialling now.
    ///
    /// Already-dialled addresses are dropped, because the DHT returns the same
    /// peers every cycle and re-dialling all of them every ten minutes would be
    /// a self-inflicted connection storm.
    pub fn fresh_dials(&mut self, batch: &[SocketAddrV4]) -> Vec<SocketAddrV4> {
        batch
            .iter()
            .copied()
            .filter(|a| self.dialled.insert(*a))
            .collect()
    }

    pub fn set_public(&mut self, public: bool) {
        self.public = public;
    }

    pub fn is_public(&self) -> bool {
        self.public
    }

    /// Periodic housekeeping: expire adverts, forget idle rate windows.
    ///
    /// Returns how many tables were dropped.
    pub fn tick(&mut self, now_ms: u64) -> usize {
        self.limits.sweep(now_ms);
        self.lobby.expire(now_ms)
    }
}

/// Whether a GossipSub message is one this node should even parse.
///
/// Size first, because it is free and the cap is the protocol's. A message over
/// the cap cannot be a conforming one, so there is nothing to gain by looking
/// inside it.
pub fn worth_parsing(message: &gossipsub::Message, cap: usize) -> bool {
    message.data.len() <= cap
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::constants::{AD_TTL_MS, LOBBY_MSG_MAX};
    use std::net::Ipv4Addr;

    fn v4(a: [u8; 4], port: u16) -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::new(a[0], a[1], a[2], a[3]), port)
    }

    /// A discovered address means QUIC, because that is the port this node
    /// announces. Building a TCP dial from it would be a guess, and a failed
    /// guess is indistinguishable from a peer being offline.
    #[test]
    fn a_discovered_address_dials_quic() {
        let addr = quic_dial_addr(v4([203, 0, 113, 5], 4001));
        assert_eq!(
            addr.to_string(),
            "/ip4/203.0.113.5/udp/4001/quic-v1",
            "one address family, one transport, no guessing"
        );
    }

    /// The DHT returns the same peers every cycle. Re-dialling all of them every
    /// ten minutes would be a connection storm this node inflicted on itself.
    #[test]
    fn a_peer_is_dialled_once_per_session() {
        let mut state = NodeState::new();
        let batch = vec![v4([1, 1, 1, 1], 4001), v4([2, 2, 2, 2], 4001)];

        assert_eq!(state.fresh_dials(&batch).len(), 2);
        assert_eq!(state.fresh_dials(&batch).len(), 0, "the same batch again");

        let mut wider = batch.clone();
        wider.push(v4([3, 3, 3, 3], 4001));
        assert_eq!(
            state.fresh_dials(&wider),
            vec![v4([3, 3, 3, 3], 4001)],
            "and only what is new"
        );
    }

    /// The size gate is free and the cap is the protocol's, so it runs before
    /// anything looks inside the message.
    #[test]
    fn an_oversized_message_is_not_parsed() {
        let msg = |len: usize| gossipsub::Message {
            source: None,
            data: vec![0u8; len],
            sequence_number: None,
            topic: gossipsub::IdentTopic::new("t").hash(),
        };
        assert!(worth_parsing(&msg(LOBBY_MSG_MAX), LOBBY_MSG_MAX));
        assert!(!worth_parsing(&msg(LOBBY_MSG_MAX + 1), LOBBY_MSG_MAX));
        assert!(worth_parsing(&msg(0), LOBBY_MSG_MAX));
    }

    /// One lost re-broadcast must not expire a table, which is why the interval
    /// is half the TTL and not merely shorter than it.
    #[test]
    fn a_lost_rebroadcast_does_not_expire_a_table() {
        assert!(
            REBROADCAST.as_millis() as u64 * 2 <= AD_TTL_MS,
            "two intervals must still fit inside the TTL"
        );
    }

    /// Housekeeping is one call, so no caller can do half of it.
    #[test]
    fn the_tick_expires_and_sweeps_together() {
        let mut state = NodeState::new();
        for i in 0..10u32 {
            let mut peer = [0u8; 32];
            peer[..4].copy_from_slice(&i.to_be_bytes());
            state.limits.admit_ad(peer, [9u8; 32], 0);
        }
        assert_eq!(state.limits.tracked().0, 10);

        state.tick(200_000);
        assert_eq!(state.limits.tracked(), (0, 0), "idle windows forgotten");
    }

    /// D-002's gate. It starts closed and is opened by measurement, never by a
    /// setting: a user who is wrong about their own NAT would otherwise
    /// advertise a way through that is not one.
    #[test]
    fn reachability_starts_closed() {
        let mut state = NodeState::new();
        assert!(!state.is_public(), "assume nothing until AutoNAT answers");
        state.set_public(true);
        assert!(state.is_public());
    }

    /// The clock is a local view and never canonical state (D-012). This test
    /// exists to hold the shape of that claim: the function returns a plain
    /// number that nothing hashes.
    #[test]
    fn the_clock_is_a_local_view() {
        let a = now_unix_ms();
        let b = now_unix_ms();
        assert!(b >= a);
        assert!(a > 1_600_000_000_000, "and it is a real wall clock");
    }
}
