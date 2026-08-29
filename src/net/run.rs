//! The running node: the async loop that makes D-003 true.
//!
//! Everything else in `net` is a decision made in isolation and tested in
//! isolation. This is the part that has to be run to be believed, so it is kept
//! small and its pieces live elsewhere: the address filter is
//! [`dht`](super::dht)'s, the admission rules are [`lobby`](super::lobby)'s, and
//! the per-node state is [`node`](super::node)'s. What is here is the order
//! things happen in.
//!
//! # The order, and why it is this one
//!
//! 1. **Listen first.** The Mainline announce carries a port, and announcing a
//!    port nothing is bound to publishes an address that cannot be dialled — to
//!    a hundred strangers, every ten minutes.
//! 2. **Announce, then ask.** BEP 5's token comes from a recent `get_peers` to
//!    the same node, so the crate does the round trip itself; what matters here
//!    is that this node is findable before it starts expecting to find others.
//! 3. **Dial what is new.** The DHT returns the same peers every cycle, and
//!    re-dialling all of them every ten minutes is a connection storm a node
//!    would be inflicting on itself.
//! 4. **Gossip.** A peer that completed a handshake joins the mesh, and the
//!    adverts arrive there.
//!
//! # What failure looks like, and why most of it is not failure
//!
//! Most dials fail. The lobby infohash is shared with whatever else announces
//! under it, the addresses are stale by up to ten minutes, and a client behind
//! symmetric NAT cannot be reached directly at all. None of that is an error
//! condition — D-003 is satisfied by the dials that succeed, and the ones that
//! do not are the ordinary weather of an open DHT.

use std::time::Duration;

// Two `StreamExt` traits are in play - `futures_lite`'s for the DHT stream and
// libp2p's for the swarm - so both are named rather than glob-imported. A `next`
// that resolved to the wrong one compiles until it does not.
use futures_lite::StreamExt as DhtStreamExt;
use libp2p::{futures::StreamExt as SwarmStreamExt, gossipsub, identity, swarm::SwarmEvent, Multiaddr};
use mainline::{Dht, Id};
use tokio::sync::mpsc;

use super::dht::{self, PeerHints, Swarm as DhtSwarm, REANNOUNCE_INTERVAL};
use super::node::{quic_dial_addr, worth_parsing, NodeEvent, NodeState, REBROADCAST};
use super::swarm::{self, NodeConfig, PokerBehaviourEvent, RelayRole, Topics};
use crate::protocol::constants::LOBBY_MSG_MAX;

/// How long to wait for the DHT to bootstrap before giving up on this cycle.
const BOOTSTRAP_PATIENCE: Duration = Duration::from_secs(20);

/// How often to ask whether an announce is due.
///
/// Not the same thing as how often to announce. The check is cheap and the
/// announce is rare, and collapsing the two into one interval is what made the
/// first version silent for its first ten minutes.
const ANNOUNCE_CHECK: Duration = Duration::from_secs(15);

/// Where this node listens.
///
/// Port 0 on both, so the OS chooses and nothing collides with a torrent client
/// or another instance. QUIC is what a discovered address means; TCP is there
/// for peers that cannot do UDP at all.
fn listen_addrs() -> Vec<Multiaddr> {
    vec![
        "/ip4/0.0.0.0/udp/0/quic-v1".parse().expect("a literal"),
        "/ip4/0.0.0.0/tcp/0".parse().expect("a literal"),
    ]
}

/// The QUIC port out of a listen address, if it has one.
///
/// This is what gets announced to Mainline, so it is read from what the swarm
/// actually bound rather than from what was asked for.
pub fn quic_port(addr: &Multiaddr) -> Option<u16> {
    use libp2p::multiaddr::Protocol;
    let mut udp = None;
    let mut is_quic = false;
    for p in addr.iter() {
        match p {
            Protocol::Udp(port) => udp = Some(port),
            Protocol::QuicV1 => is_quic = true,
            _ => {}
        }
    }
    if is_quic {
        udp
    } else {
        None
    }
}

/// Run a node until the caller drops the receiver.
///
/// Events go out on the channel rather than to a logger, so the GUI and a
/// headless run see the same stream.
pub async fn run(
    identity: identity::Keypair,
    events: mpsc::Sender<NodeEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut swarm = swarm::build(NodeConfig {
        identity,
        // Closed until AutoNAT says otherwise (D-002). A user who is wrong about
        // their own NAT would otherwise advertise a way through that is not one.
        relay_role: RelayRole::None,
    })?;

    let topics = Topics::default();
    swarm.behaviour_mut().gossipsub.subscribe(&topics.lobby)?;
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&topics.lobby_chat)?;

    for addr in listen_addrs() {
        swarm.listen_on(addr)?;
    }

    let mut state = NodeState::new();
    let mut hints = PeerHints::new();

    // The DHT socket. Port 0, never 6881 - see `dht`.
    let dht = Dht::builder()
        .port(0)
        .request_timeout(Duration::from_millis(2_500))
        .build()?
        .as_async();
    let lobby_hash = Id::from_bytes(DhtSwarm::Lobby.infohash())?;

    let mut announced_port: Option<u16> = None;
    // Checked often, acted on rarely. The listen address arrives a moment after
    // the swarm starts, so a timer whose period **is** the re-announce interval
    // misses its first tick and then says nothing for ten minutes — which is
    // what the first run of this binary did, silently.
    let mut announce_timer = tokio::time::interval(ANNOUNCE_CHECK);
    let mut last_announce: Option<tokio::time::Instant> = None;
    let mut discover_timer = tokio::time::interval(Duration::from_secs(60));
    let mut housekeeping = tokio::time::interval(REBROADCAST);

    loop {
        tokio::select! {
            event = SwarmStreamExt::select_next_some(&mut swarm) => {
                match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        // Announce only a port something is actually bound to.
                        if announced_port.is_none() {
                            announced_port = quic_port(&address);
                        }
                        let _ = events.send(NodeEvent::Listening(address)).await;
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        let _ = events.send(NodeEvent::PeerConnected(peer_id)).await;
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } => {
                        let _ = events.send(NodeEvent::PeerDisconnected(peer_id)).await;
                    }
                    SwarmEvent::OutgoingConnectionError { error, .. } => {
                        let _ = events
                            .send(NodeEvent::DialFailed { reason: error.to_string() })
                            .await;
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Mdns(
                        libp2p::mdns::Event::Discovered(found),
                    )) => {
                        for (peer, addr) in found {
                            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer);
                            let _ = swarm.dial(addr);
                            let _ = events.send(NodeEvent::LocalPeer(peer)).await;
                        }
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                        gossipsub::Event::Message { message, .. },
                    )) if !worth_parsing(&message, LOBBY_MSG_MAX) => {
                        // Size first: it is free, the cap is the protocol's, and
                        // a message over it cannot be a conforming one — so
                        // there is nothing to gain by looking inside.
                        let _ = events
                            .send(NodeEvent::TableRefused {
                                reason: format!(
                                    "over the lobby cap: {} bytes",
                                    message.data.len()
                                ),
                            })
                            .await;
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                        gossipsub::Event::Message { .. },
                    )) => {
                        // The seam. Decoding, `verify_strict` and §7.2's
                        // admission rules go here, in that order, and each of
                        // them already exists and is tested — what is missing is
                        // the envelope parser that turns bytes into the advert
                        // `lobby::admit` takes.
                    }
                    _ => {}
                }
            }

            _ = announce_timer.tick() => {
                // Two preconditions, and the first fires at t = 0 without them:
                // a port something is actually bound to, and a bootstrapped
                // routing table. Announcing before either is a store request
                // with nowhere to send it, which is what the first run of this
                // binary reported.
                let due = match last_announce {
                    None => true,
                    Some(at) => at.elapsed() >= REANNOUNCE_INTERVAL,
                };
                if let (Some(port), true, true) =
                    (announced_port, due, bootstrapped(&dht).await)
                {
                    {
                        match dht.announce_peer(lobby_hash, Some(port)).await {
                            Ok(_) => {
                                last_announce = Some(tokio::time::Instant::now());
                                let _ = events.send(NodeEvent::Announced { port }).await;
                            }
                            Err(e) => {
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "DHT announce failed: {e}"
                                    )))
                                    .await;
                            }
                        }
                    }
                }
            }

            _ = discover_timer.tick() => {
                if bootstrapped(&dht).await {
                    let mut stream = dht.get_peers(lobby_hash);
                    while let Some(batch) = DhtStreamExt::next(&mut stream).await {
                        hints.absorb(batch.as_ref());
                    }
                    let (unusable, full) = hints.dropped();
                    let _ = events
                        .send(NodeEvent::Discovered {
                            hints: hints.len(),
                            dropped: unusable + full,
                        })
                        .await;

                    for addr in state.fresh_dials(hints.peers()) {
                        // A failed dial is the ordinary case, not an error.
                        let _ = swarm.dial(quic_dial_addr(addr));
                    }
                }
            }

            _ = housekeeping.tick() => {
                state.tick(super::node::now_unix_ms());
            }
        }
    }
}

/// Whether the DHT has a routing table yet, with a bound on the wait.
///
/// Both the announce and the discovery need this, and they needed it separately
/// — which is how the announce came to fire at `t = 0` with nowhere to send a
/// store request.
async fn bootstrapped(dht: &mainline::async_dht::AsyncDht) -> bool {
    tokio::time::timeout(BOOTSTRAP_PATIENCE, dht.bootstrapped())
        .await
        .unwrap_or(false)
}

/// Bootstrap addresses for the DHT, filtered so none can panic the crate.
///
/// The public routers, as `mainline` ships them, minus anything IPv6 — which is
/// a crash and not a preference. See [`dht::usable_bootstrap`].
pub fn bootstrap_addresses() -> Vec<std::net::SocketAddrV4> {
    use std::net::ToSocketAddrs;
    let hosts = [
        "router.bittorrent.com:6881",
        "dht.transmissionbt.com:6881",
        "router.utorrent.com:6881",
    ];
    let mut resolved = Vec::new();
    for h in hosts {
        if let Ok(addrs) = h.to_socket_addrs() {
            resolved.extend(addrs);
        }
    }
    dht::usable_bootstrap(&resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The announce carries a port, and announcing one nothing is bound to
    /// publishes an unreachable address to a hundred strangers every ten
    /// minutes. So the port is read from what the swarm actually bound.
    #[test]
    fn only_a_quic_listen_address_yields_a_port() {
        let quic: Multiaddr = "/ip4/192.168.1.5/udp/54321/quic-v1".parse().unwrap();
        assert_eq!(quic_port(&quic), Some(54321));

        let tcp: Multiaddr = "/ip4/192.168.1.5/tcp/54321".parse().unwrap();
        assert_eq!(quic_port(&tcp), None, "a TCP port is not the QUIC port");

        let bare_udp: Multiaddr = "/ip4/192.168.1.5/udp/54321".parse().unwrap();
        assert_eq!(quic_port(&bare_udp), None, "and UDP alone is not QUIC");
    }

    /// Port 0 on both transports: the OS chooses, so nothing collides with
    /// another instance or with a torrent client.
    #[test]
    fn the_node_binds_ports_the_os_chooses() {
        let addrs = listen_addrs();
        assert_eq!(addrs.len(), 2);
        for a in &addrs {
            let s = a.to_string();
            assert!(
                s.contains("/udp/0/") || s.ends_with("/tcp/0"),
                "{s} does not bind port 0"
            );
            assert!(!s.contains("6881"), "never the BitTorrent default port");
        }
    }

    /// The bootstrap list is resolved and then filtered, because a v6 address
    /// reaching `mainline` is a panic in somebody else's file.
    ///
    /// It does not assert that resolution succeeded: a machine with no DNS is
    /// not a broken build, and the empty list is handled by the crate's own
    /// defaults.
    #[test]
    fn no_bootstrap_address_can_panic_the_crate() {
        for addr in bootstrap_addresses() {
            assert!(dht::worth_trying(addr).is_ok());
            assert!(!addr.ip().is_loopback());
        }
    }

    /// The infohash is 20 bytes and the crate takes it as one.
    #[test]
    fn the_lobby_infohash_is_a_valid_id() {
        assert!(Id::from_bytes(DhtSwarm::Lobby.infohash()).is_ok());
        assert!(Id::from_bytes(DhtSwarm::Relay.infohash()).is_ok());
    }
}
