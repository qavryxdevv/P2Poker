//! Global discovery over BitTorrent Mainline DHT, `mainline = "=8.0.0"`.
//!
//! Only `announce_peer` and `get_peers`, under one fixed [`LOBBY_INFOHASH`].
//! **Game state never travels here** (`SPEC_CS.md` §1, §2, §3), and there is
//! nothing in this module that could carry it: the only thing that goes out is
//! *this client exists at this address*, and the only thing that comes back is a
//! list of addresses.
//!
//! # What this layer is, and what it is not
//!
//! It is **step one of two** and the distinction matters, because conflating
//! them is how a later editor would come to put game state on the DHT.
//!
//! * *This module* answers **where to try**. A peer list is a hint about
//!   addresses, never a claim about who is there — identity is settled by the
//!   libp2p handshake, and that is D-003.
//! * *[`super::lobby`]* answers **what tables are open**, over GossipSub, from
//!   signed advertisements, after a connection exists.
//!
//! So a peer that the DHT returns and that turns out to be a stranger with no
//! poker client is not an error. It is the expected case for most of them.
//!
//! # Two facts about the crate that will crash the client if forgotten
//!
//! **`mainline` 8.0.0 panics on IPv6.** `unimplemented!` in `common/id.rs:93`
//! and `rpc/socket.rs:54`. Any v6 address that reaches it takes the process
//! down, so [`usable_bootstrap`] filters them out before they ever do. This is
//! not a preference about address families; it is a crash.
//!
//! **The port must not be 6881.** That is the default BitTorrent port, and a
//! client bound to it looks like a torrent client to every ISP and every
//! traffic classifier that has ever been written. Port 0 lets the OS choose.
//!
//! # What the fixed infohash costs, said plainly
//!
//! A fixed public `LOBBY_INFOHASH` publishes each player's IP address to roughly
//! a hundred arbitrary internet hosts per announce cycle. Phase 0 watched a
//! stranger re-announce under a freshly generated random infohash within
//! twenty-four minutes — DHT crawling observed, not hypothesised.
//!
//! That is the price of D-003's acceptance criterion, which is that every client
//! anywhere on the internet sees the open tables. It is a discovery-layer
//! property and no lobby-protocol rule changes it, so nobody should come away
//! thinking the lobby is private.

use std::net::{SocketAddr, SocketAddrV4};
use std::time::Duration;

use crate::protocol::constants::{LOBBY_INFOHASH, REANNOUNCE_INTERVAL_MS, RELAY_INFOHASH};

/// How often to re-announce.
///
/// BEP 5 defines **no** TTL and no interval. What it does pin is the token
/// window: the secret rotates every 5 minutes and a token is accepted up to 10
/// minutes old. Ten minutes sits inside that window and inside the 15-minute
/// good-node window, so an announce is refreshed before either could have
/// lapsed.
///
/// `mainline`'s own storage side is an LRU and not a TTL — a record is evicted
/// when it is least recently used, never on a clock — so re-announcing is what
/// keeps a client from being pushed out by busier infohashes rather than what
/// keeps it from expiring.
pub const REANNOUNCE_INTERVAL: Duration = Duration::from_millis(REANNOUNCE_INTERVAL_MS);

/// The largest peer list this client will hold from discovery.
///
/// The DHT is an open network and the response is whatever strangers say, so the
/// list is capped. It is a hint about where to try; a longer hint is not a better
/// one, and an unbounded one is a memory bug with an external trigger.
pub const MAX_DISCOVERED_PEERS: usize = 512;

/// The port never to bind.
const BITTORRENT_DEFAULT_PORT: u16 = 6881;

/// Why an address from the DHT is not worth trying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unusable {
    /// `0.0.0.0`, `255.255.255.255`, or a multicast address. Not a host.
    NotAHost,
    /// Port 0. Nothing listens there.
    NoPort,
    /// Loopback, from a peer that is not us. It would only ever reach ourselves.
    Loopback,
}

/// Whether an address the DHT returned is worth a connection attempt.
///
/// Deliberately **not** a filter on private ranges: a table on a LAN is a real
/// table, and two clients behind one router discovering each other through the
/// DHT is a case D-004 requires to work. What is filtered is what could not work
/// for anybody.
pub fn worth_trying(addr: SocketAddrV4) -> Result<(), Unusable> {
    let ip = *addr.ip();
    if addr.port() == 0 {
        return Err(Unusable::NoPort);
    }
    if ip.is_unspecified() || ip.is_broadcast() || ip.is_multicast() {
        return Err(Unusable::NotAHost);
    }
    if ip.is_loopback() {
        return Err(Unusable::Loopback);
    }
    Ok(())
}

/// Keep only the bootstrap addresses `mainline` can be handed.
///
/// **A v6 address here panics the process**, so this is a crash filter and not a
/// policy. It returns the addresses that survive rather than erroring, because a
/// bootstrap list with some unusable entries is still a usable bootstrap list
/// and refusing the whole thing would turn a cosmetic misconfiguration into a
/// client that cannot start.
pub fn usable_bootstrap(addrs: &[SocketAddr]) -> Vec<SocketAddrV4> {
    addrs
        .iter()
        .filter_map(|a| match a {
            SocketAddr::V4(v4) => Some(*v4),
            SocketAddr::V6(_) => None,
        })
        .filter(|v4| worth_trying(*v4).is_ok())
        .collect()
}

/// Whether a port may be bound for the DHT socket.
///
/// 0 is right — the OS chooses — and 6881 is wrong for a reason that has nothing
/// to do with correctness: it is the default BitTorrent port, and binding it
/// makes this client look like a torrent client to every traffic classifier in
/// the path.
pub fn port_is_acceptable(port: u16) -> bool {
    port != BITTORRENT_DEFAULT_PORT
}

/// The peers discovery has turned up.
///
/// Ordered and deduplicated, so two clients that received the same batches in
/// different orders hold the same list — which matters only for making the thing
/// testable and comparable, not for the protocol.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerHints {
    peers: Vec<SocketAddrV4>,
    /// How many addresses were dropped since the last clear, and why.
    dropped_unusable: usize,
    dropped_full: usize,
}

impl PeerHints {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take a batch from `get_peers`.
    ///
    /// Returns how many were added. Everything unusable is dropped and counted,
    /// because a discovery layer that silently discarded most of what it
    /// received would look identical to one that was working.
    pub fn absorb(&mut self, batch: &[SocketAddrV4]) -> usize {
        let before = self.peers.len();
        for &addr in batch {
            if worth_trying(addr).is_err() {
                self.dropped_unusable += 1;
                continue;
            }
            if self.peers.len() >= MAX_DISCOVERED_PEERS {
                self.dropped_full += 1;
                continue;
            }
            if let Err(at) = self.peers.binary_search(&addr) {
                self.peers.insert(at, addr);
            }
        }
        self.peers.len() - before
    }

    pub fn peers(&self) -> &[SocketAddrV4] {
        &self.peers
    }

    pub fn len(&self) -> usize {
        self.peers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    /// What was thrown away, for the log line that tells a user why nothing is
    /// happening.
    pub fn dropped(&self) -> (usize, usize) {
        (self.dropped_unusable, self.dropped_full)
    }

    pub fn clear(&mut self) {
        self.peers.clear();
        self.dropped_unusable = 0;
        self.dropped_full = 0;
    }
}

/// The two infohashes this client uses.
///
/// Named rather than passed around loose, so no call site can announce players
/// under the relay hash or the other way about. They are deliberately different
/// values: a relay volunteer is not a player looking for a table, and mixing
/// them would put every relay in every client's connection queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Swarm {
    /// Where clients looking for tables announce.
    Lobby,
    /// Where publicly reachable clients volunteering as relays announce (D-002).
    Relay,
}

impl Swarm {
    pub const fn infohash(self) -> [u8; 20] {
        match self {
            Swarm::Lobby => LOBBY_INFOHASH,
            Swarm::Relay => RELAY_INFOHASH,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn v4(a: [u8; 4], port: u16) -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::new(a[0], a[1], a[2], a[3]), port)
    }

    /// A crash filter, not a policy. `mainline` 8.0.0 has `unimplemented!` on
    /// two IPv6 paths, so a v6 address that reaches it takes the process down.
    #[test]
    fn no_ipv6_address_survives_the_bootstrap_filter() {
        let addrs = vec![
            SocketAddr::from((Ipv4Addr::new(67, 215, 246, 10), 6881)),
            SocketAddr::new(Ipv6Addr::LOCALHOST.into(), 6881),
            SocketAddr::new(
                "2001:4860:4860::8888".parse::<Ipv6Addr>().unwrap().into(),
                6881,
            ),
            SocketAddr::from((Ipv4Addr::new(87, 98, 162, 88), 6881)),
        ];
        let kept = usable_bootstrap(&addrs);
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().all(|a| !a.ip().is_loopback()));
    }

    /// A partly unusable bootstrap list is still a usable bootstrap list. Making
    /// this an error would turn a cosmetic misconfiguration into a client that
    /// cannot start at all.
    #[test]
    fn a_partly_bad_bootstrap_list_still_starts_the_client() {
        let addrs = vec![
            SocketAddr::new(Ipv6Addr::LOCALHOST.into(), 6881),
            SocketAddr::from((Ipv4Addr::new(0, 0, 0, 0), 6881)),
            SocketAddr::from((Ipv4Addr::new(67, 215, 246, 10), 6881)),
        ];
        assert_eq!(usable_bootstrap(&addrs).len(), 1);
        assert!(usable_bootstrap(&[]).is_empty());
    }

    /// What is filtered is what could not work for anybody. Private ranges are
    /// **not** filtered: a table on a LAN is a real table, and two clients behind
    /// one router finding each other is a case D-004 requires to work.
    #[test]
    fn a_private_address_is_worth_trying() {
        assert_eq!(worth_trying(v4([192, 168, 1, 20], 4001)), Ok(()));
        assert_eq!(worth_trying(v4([10, 0, 0, 7], 4001)), Ok(()));
        assert_eq!(worth_trying(v4([100, 64, 0, 1], 4001)), Ok(()), "CGNAT");
    }

    #[test]
    fn an_address_nobody_could_reach_is_dropped() {
        assert_eq!(worth_trying(v4([1, 2, 3, 4], 0)), Err(Unusable::NoPort));
        assert_eq!(
            worth_trying(v4([0, 0, 0, 0], 4001)),
            Err(Unusable::NotAHost)
        );
        assert_eq!(
            worth_trying(v4([255, 255, 255, 255], 4001)),
            Err(Unusable::NotAHost)
        );
        assert_eq!(
            worth_trying(v4([224, 0, 0, 1], 4001)),
            Err(Unusable::NotAHost)
        );
        assert_eq!(
            worth_trying(v4([127, 0, 0, 1], 4001)),
            Err(Unusable::Loopback)
        );
    }

    /// Never 6881: a client bound there looks like a torrent client to every
    /// traffic classifier in the path.
    #[test]
    fn the_bittorrent_default_port_is_refused() {
        assert!(!port_is_acceptable(6881));
        assert!(port_is_acceptable(0), "0 lets the OS choose, which is right");
        assert!(port_is_acceptable(4001));
    }

    /// The DHT is an open network and the response is whatever strangers say, so
    /// the list is bounded. An unbounded one is a memory bug with an external
    /// trigger.
    #[test]
    fn the_peer_list_is_capped_against_a_hostile_response() {
        let mut hints = PeerHints::new();
        let flood: Vec<SocketAddrV4> = (0..2_000u32)
            .map(|i| v4(i.to_be_bytes(), 4001))
            .filter(|a| worth_trying(*a).is_ok())
            .collect();

        hints.absorb(&flood);
        assert_eq!(hints.len(), MAX_DISCOVERED_PEERS);
        let (_, full) = hints.dropped();
        assert!(full > 0, "and it says how much it threw away");
    }

    /// A discovery layer that silently discarded most of what it received would
    /// look identical to one that was working, so it counts.
    #[test]
    fn what_was_thrown_away_is_counted() {
        let mut hints = PeerHints::new();
        let added = hints.absorb(&[
            v4([1, 2, 3, 4], 4001),
            v4([0, 0, 0, 0], 4001),
            v4([5, 6, 7, 8], 0),
            v4([127, 0, 0, 1], 4001),
        ]);
        assert_eq!(added, 1);
        assert_eq!(hints.dropped(), (3, 0));
    }

    /// Batches arrive in whatever order the network produces them, so the list
    /// is sorted and deduplicated: two clients given the same peers hold the
    /// same list.
    #[test]
    fn the_list_is_a_set_and_not_an_arrival_log() {
        let mut a = PeerHints::new();
        a.absorb(&[v4([9, 9, 9, 9], 1), v4([1, 1, 1, 1], 1)]);
        a.absorb(&[v4([5, 5, 5, 5], 1), v4([1, 1, 1, 1], 1)]);

        let mut b = PeerHints::new();
        b.absorb(&[v4([1, 1, 1, 1], 1), v4([5, 5, 5, 5], 1)]);
        b.absorb(&[v4([1, 1, 1, 1], 1), v4([9, 9, 9, 9], 1)]);

        assert_eq!(a.peers(), b.peers());
        assert_eq!(a.len(), 3, "the repeat is not a fourth peer");
    }

    /// The same address on two ports is two peers: NAT puts several clients
    /// behind one IP, which is the ordinary case rather than the exception.
    #[test]
    fn two_ports_behind_one_address_are_two_peers() {
        let mut hints = PeerHints::new();
        hints.absorb(&[v4([203, 0, 113, 5], 4001), v4([203, 0, 113, 5], 4002)]);
        assert_eq!(hints.len(), 2);
    }

    /// The two swarms are separate, and mixing them would put every relay in
    /// every client's connection queue.
    #[test]
    fn the_lobby_and_the_relay_swarms_are_different() {
        assert_ne!(Swarm::Lobby.infohash(), Swarm::Relay.infohash());
        assert_eq!(Swarm::Lobby.infohash(), LOBBY_INFOHASH);
        assert_eq!(Swarm::Relay.infohash(), RELAY_INFOHASH);
    }

    /// Ten minutes sits inside BEP 5's token window (5 to 10 minutes) and inside
    /// the 15-minute good-node window, so an announce is refreshed before either
    /// could have lapsed.
    #[test]
    fn the_reannounce_interval_is_inside_the_token_window() {
        assert_eq!(REANNOUNCE_INTERVAL, Duration::from_secs(600));
        assert!(REANNOUNCE_INTERVAL <= Duration::from_secs(600), "the token window");
        assert!(REANNOUNCE_INTERVAL < Duration::from_secs(900), "the good-node window");
    }
}
