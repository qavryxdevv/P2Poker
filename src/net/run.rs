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

use std::collections::HashSet;
use std::net::SocketAddrV4;

use super::advert;
use super::dht::{self, PeerHints, Swarm as DhtSwarm, REANNOUNCE_INTERVAL};
use super::relay;
use super::lobby::TableAd;
use super::node::{quic_dial_addr, worth_parsing, NodeEvent, NodeState, REBROADCAST};
use super::swarm::{self, NodeConfig, PokerBehaviourEvent, RelayRole, Topics};
use crate::protocol::constants::{AD_TTL_MS, HAND_DEADLINE_CAP_MS, LOBBY_MSG_MAX, MAX_SEATS};

/// How long to wait for the DHT to bootstrap before giving up on this cycle.
const BOOTSTRAP_PATIENCE: Duration = Duration::from_secs(20);

/// How many relay addresses to remember having asked.
///
/// The set exists so a discovery cycle does not re-ask the same relay every
/// minute, and it is fed from the DHT — so it is bounded like everything else
/// that is. The bound empties it rather than freezing it: see where it is used.
const MAX_ASKED_RELAYS: usize = 512;

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
/// A table this node is offering, if it is offering one.
///
/// The signing key **is** the table's identity, so it is held here and nowhere
/// else: a table whose key lived in two places would be a table two peers could
/// be handed two versions of.
pub struct Hosted {
    pub ad: TableAd,
    pub key: ed25519_dalek::SigningKey,
}

pub async fn run(
    identity: identity::Keypair,
    events: mpsc::Sender<NodeEvent>,
    mut hosted: Option<Hosted>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut swarm = swarm::build(NodeConfig {
        identity,
        // Capacity from the start; the ANNOUNCE is what AutoNAT gates (D-002).
        // `relay::Config` cannot be changed after the swarm is built, and
        // capacity nobody can reach costs nothing.
        relay_role: RelayRole::Volunteer,
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
    let relay_hash = Id::from_bytes(DhtSwarm::Relay.infohash())?;

    let mut announced_port: Option<u16> = None;
    // Relays this node has asked for a reservation from, so a repeated discovery
    // cycle does not ask the same relay again every minute.
    //
    // Bounded, because it is fed from the DHT and the DHT is whatever strangers
    // say. When it fills it is emptied rather than frozen: asking a relay twice
    // costs one message, and never asking a NEW one costs the connection this
    // client needs — so the failure this bound must not have is the one a frozen
    // set would give it.
    let mut asked_relays: HashSet<SocketAddrV4> = HashSet::new();
    let mut have_reservation = false;
    // Counted so that "no relay" is reported as a finding rather than as
    // impatience: three cycles is three minutes of looking.
    let mut relay_searches: u32 = 0;
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
                    SwarmEvent::Behaviour(PokerBehaviourEvent::RelayClient(
                        libp2p::relay::client::Event::ReservationReqAccepted {
                            relay_peer_id,
                            limit,
                            ..
                        },
                    )) => {
                        have_reservation = true;
                        let bytes = limit.as_ref().and_then(|l| l.data_in_bytes());
                        let seconds = limit
                            .as_ref()
                            .and_then(|l| l.duration())
                            .map(|d| d.as_secs());
                        // The decision `NAT_AND_DISCOVERY.md` asks for by name:
                        // compare the relay's own reported limits against what a
                        // hand actually costs, before anything is committed to
                        // this circuit.
                        let verdict = relay::adequate(
                            MAX_SEATS,
                            Duration::from_millis(HAND_DEADLINE_CAP_MS),
                            bytes,
                            limit.as_ref().and_then(|l| l.duration()),
                        );
                        let _ = events
                            .send(NodeEvent::Reserved {
                                relay: relay_peer_id,
                                bytes,
                                seconds,
                                adequate: matches!(
                                    verdict,
                                    relay::Adequacy::Adequate | relay::Adequacy::Unlimited
                                ),
                            })
                            .await;
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Dcutr(ev)) => {
                        match ev.result {
                            Ok(_) => {
                                let _ = events
                                    .send(NodeEvent::HolePunched(ev.remote_peer_id))
                                    .await;
                            }
                            Err(_) => {
                                let _ = events
                                    .send(NodeEvent::StillRelayed(ev.remote_peer_id))
                                    .await;
                            }
                        }
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::AutonatClient(ev)) => {
                        // AutoNAT is the only thing that may decide this. A
                        // setting cannot: a user who is wrong about their own NAT
                        // would advertise a way through that is not one.
                        let public = ev.result.is_ok();

                        // A confirmed address is an **external** address, and
                        // saying so is not bookkeeping.
                        //
                        // `libp2p-relay` fills a reservation from the relay's
                        // external addresses and from nowhere else. A node that
                        // never records one volunteers as a relay, accepts the
                        // reservation, and hands back a list of no addresses —
                        // so the peer that reserved has a circuit it cannot tell
                        // anyone about, and the reservation is worse than a
                        // refusal because it looks like it worked. That is what
                        // `tests/relay_circuit.rs` found on the first run it was
                        // ever given, and it had been true since the relay was
                        // configured.
                        if public {
                            swarm.add_external_address(ev.tested_addr.clone());
                        } else {
                            swarm.remove_external_address(&ev.tested_addr);
                        }

                        if public != state.is_public() {
                            state.set_public(public);
                            let _ = events.send(NodeEvent::Reachability { public }).await;
                        }
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
                        gossipsub::Event::Subscribed { peer_id, .. },
                    )) => {
                        let _ = events.send(NodeEvent::MeshPeer(peer_id)).await;
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                        gossipsub::Event::Message {
                            message,
                            propagation_source,
                            message_id,
                        },
                    )) => {
                        // **Every path out of here must end in a
                        // `report_message_validation_result`.** With
                        // `validate_messages()` set, GossipSub forwards nothing
                        // until the application answers — so a path that returns
                        // without answering makes this client a black hole: it
                        // takes adverts and relays none, invisibly, because
                        // delivery to *itself* still works and a two-node test
                        // still passes.
                        //
                        // The three answers are not interchangeable, and the
                        // distinction is the one the whole protocol turns on.
                        // `Reject` says *this sender is at fault* and costs it
                        // peer score; `Ignore` says *this client will not pass it
                        // on* and blames nobody.
                        let verdict = handle_gossip(
                            &message,
                            propagation_source,
                            &topics,
                            &mut state,
                            &events,
                        )
                        .await;

                        let _ = swarm
                            .behaviour_mut()
                            .gossipsub
                            .report_message_validation_result(
                                &message_id,
                                &propagation_source,
                                verdict,
                            );
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

                                // And, only when AutoNAT says this client can
                                // actually be reached, offer the line to other
                                // people's games (D-002). Announcing this from
                                // behind a NAT is the harm the role separation
                                // exists to avoid.
                                if state.is_public() {
                                    if let Err(e) =
                                        dht.announce_peer(relay_hash, Some(port)).await
                                    {
                                        let _ = events
                                            .send(NodeEvent::Warning(format!(
                                                "relay announce failed: {e}"
                                            )))
                                            .await;
                                    }
                                }
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
                    // Cleared first, and this is not tidiness. `absorb` drops the
                    // INCOMING address once the list holds its cap, so a list
                    // that is never cleared freezes on whatever the first
                    // stranger supplied — for the whole life of the process. The
                    // DHT returns a fresh view every cycle and this takes it;
                    // re-dialling is prevented by `fresh_dials`, which is where
                    // that belongs.
                    hints.clear();
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

                    // A relay is needed when this client cannot be reached
                    // directly — and DCUtR needs one too, since it upgrades an
                    // existing relayed connection and cannot start one. So the
                    // relay swarm is asked whenever there is no reservation yet,
                    // whatever AutoNAT has said so far.
                    if !have_reservation {
                        relay_searches += 1;
                        let mut relays = PeerHints::new();
                        let mut stream = dht.get_peers(relay_hash);
                        while let Some(batch) = DhtStreamExt::next(&mut stream).await {
                            relays.absorb(batch.as_ref());
                        }
                        if relays.is_empty() && relay_searches >= 3 {
                            let _ = events
                                .send(NodeEvent::NoRelayFound {
                                    cycles: relay_searches,
                                })
                                .await;
                        }
                        if asked_relays.len() >= MAX_ASKED_RELAYS {
                            asked_relays.clear();
                        }
                        for addr in relays.peers() {
                            if !asked_relays.insert(*addr) {
                                continue;
                            }
                            // Listening on a relay's circuit address IS the
                            // reservation request; there is no separate call.
                            let circuit = quic_dial_addr(*addr)
                                .with(libp2p::multiaddr::Protocol::P2pCircuit);
                            if let Err(e) = swarm.listen_on(circuit) {
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "no reservation from {addr}: {e}"
                                    )))
                                    .await;
                            }
                        }
                    }
                }
            }

            _ = housekeeping.tick() => {
                let now = super::node::now_unix_ms();
                state.tick(now);

                // Re-broadcast this node's own table. The timestamps move every
                // time, which is what rule 6 compares; the parameters do not,
                // which is what rule 7 compares. A re-broadcast that changed a
                // parameter would mark this node's own table unjoinable at every
                // receiver, so the advert is edited only here and only in time.
                if let Some(h) = hosted.as_mut() {
                    h.ad.timestamp_unix_ms = now;
                    h.ad.expires_at_unix_ms = now + AD_TTL_MS;
                    match advert::publish(&h.ad, &h.key) {
                        Ok(bytes) => {
                            let n = bytes.len();
                            if let Err(e) = swarm
                                .behaviour_mut()
                                .gossipsub
                                .publish(topics.lobby.clone(), bytes)
                            {
                                // No mesh peer yet is the ordinary case at
                                // start-up, not a fault.
                                let _ = events
                                    .send(NodeEvent::Warning(format!("not published: {e}")))
                                    .await;
                            } else {
                                let _ = events.send(NodeEvent::Published { bytes: n }).await;
                            }
                        }
                        Err(e) => {
                            let _ = events
                                .send(NodeEvent::Warning(format!("own advert: {e}")))
                                .await;
                        }
                    }
                }
            }
        }
    }
}

/// Judge one gossip message, and say what GossipSub should do with it.
///
/// Returns the acceptance rather than reporting it, so the answer is given in
/// one place and no branch can forget to give it.
///
/// # Why the topic is checked here, and was not checked at all
///
/// This node subscribes to two topics and the old arm read neither, so a chat
/// message was handed to the advert parser — which rejected it as malformed and
/// blamed the sender for speaking correctly on the other channel. That is
/// `PROTOCOL.md` §4.0 step 6, and it was missing.
async fn handle_gossip(
    message: &gossipsub::Message,
    from: libp2p::PeerId,
    topics: &Topics,
    state: &mut NodeState,
    events: &mpsc::Sender<NodeEvent>,
) -> gossipsub::MessageAcceptance {
    // Size first: free, the cap is the protocol's, and a message over it cannot
    // be a conforming one. `Reject`, because the sender chose the size.
    if !worth_parsing(message, LOBBY_MSG_MAX) {
        let _ = events
            .send(NodeEvent::TableRefused {
                reason: format!("over the lobby cap: {} bytes", message.data.len()),
            })
            .await;
        return gossipsub::MessageAcceptance::Reject;
    }

    if message.topic != topics.lobby.hash() {
        return if message.topic == topics.lobby_chat.hash() {
            // Chat is not parsed yet, and forwarding a message this client has
            // not judged would be asserting something about it.
            gossipsub::MessageAcceptance::Ignore
        } else {
            // A topic this node never subscribed to has no business arriving.
            gossipsub::MessageAcceptance::Reject
        };
    }

    // The sending peer, which is **not** the table key: a peer relaying somebody
    // else's advert is the ordinary case.
    let mut peer = [0u8; 32];
    let bytes = from.to_bytes();
    let take = bytes.len().min(32);
    peer[..take].copy_from_slice(&bytes[..take]);

    let now = super::node::now_unix_ms();
    match advert::receive(&message.data, peer, now, &mut state.limits, &mut state.lobby) {
        Ok(key) => {
            let _ = events.send(NodeEvent::TableSeen { key }).await;
            gossipsub::MessageAcceptance::Accept
        }
        Err(e) => {
            let _ = events
                .send(NodeEvent::TableRefused {
                    reason: format!("{e:?}"),
                })
                .await;
            // The same distinction as everywhere else: only a finding about the
            // **sender** costs it anything. A rate limit is about this client's
            // own budget and a full lobby is about its own memory; neither is
            // evidence the peer did wrong.
            match e {
                advert::NotAccepted::RateLimited | advert::NotAccepted::NotTaken(_) => {
                    gossipsub::MessageAcceptance::Ignore
                }
                _ => gossipsub::MessageAcceptance::Reject,
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
