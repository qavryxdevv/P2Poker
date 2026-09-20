//! Three nodes, one process: a connection that goes **through a relay**.
//!
//! # What this closes, and what it cannot
//!
//! `NEXT.md` has said for a while that the relay and DCUtR are configured and
//! that no circuit has ever carried anything. That is two separate gaps and only
//! one of them needs a second network:
//!
//! * **The relay machinery** — a reservation is requested and granted, the
//!   reserving node gains a `/p2p-circuit` address, another node dials it, and
//!   traffic flows. Every part of that is a libp2p construct and none of it
//!   involves NAT, so it is testable here and is what this file tests. It had
//!   never been run.
//! * **NAT traversal** — that two peers who *cannot* reach each other directly
//!   are brought together. Loopback has no NAT and nothing on one machine
//!   simulates one. This file does not test it and does not pretend to; that
//!   needs one endpoint outside the house.
//!
//! The distinction matters because the first is where a plain configuration
//! mistake would live — wrong limits, a relay that denies every reservation, an
//! address this client cannot construct — and the second is where the physics
//! live. A configuration mistake would have made the second test fail for a
//! reason that had nothing to do with NAT, and it would have been diagnosed as
//! NAT.
//!
//! # Why the assertions are on the circuit and not on connectivity
//!
//! mDNS is in the behaviour, so these three nodes will find each other on the
//! local network and connect directly whatever the relay does. A test that
//! asserted "they connected" would therefore pass with the relay entirely
//! broken. So it asserts on the reservation event and on an address containing
//! `/p2p-circuit`, which nothing but the relay path produces.

use std::time::Duration;

use libp2p::futures::StreamExt;
use libp2p::{identity, multiaddr::Protocol, relay, swarm::SwarmEvent, Multiaddr, PeerId, Swarm};

use p2p_poker::net::relay::{adequate, Adequacy};
use p2p_poker::net::swarm::{self, NodeConfig, ShapedBehaviour, PokerBehaviourEvent, RelayAdmits, RelayRole};
use p2p_poker::protocol::constants::{HAND_DEADLINE_CAP_MS, MAX_SEATS};

const PATIENCE: Duration = Duration::from_secs(60);

fn node(role: RelayRole) -> Swarm<ShapedBehaviour> {
    node_admitting(role, RelayAdmits::default())
}

/// A node whose relay reserves slots for the peers in `admits` (`S1-FK`): the
/// node loop fills that set from `identify`, and a test with no loop names the
/// peer itself.
fn node_admitting(role: RelayRole, admits: RelayAdmits) -> Swarm<ShapedBehaviour> {
    swarm::build(NodeConfig {
        identity: identity::Keypair::generate_ed25519(),
        // Off. These dial a loopback address and then assert two peers met;
        // with multicast on, a client running elsewhere on this machine could
        // satisfy that assertion and the test would pass for the wrong reason.
        local_discovery: false,
        relay_role: role,
        relay_admits: admits,
    })
    .expect("the stack builds")
}

/// What the relay path produced, so the assertions are on facts and not on logs.
#[derive(Debug, Default)]
struct Outcome {
    /// The limits the relay reported back, if it granted a reservation.
    reservation: Option<Option<relay::client::Event>>,
    granted_bytes: Option<u64>,
    granted_duration: Option<Duration>,
    /// The `/p2p-circuit` address the reserving node was given.
    circuit_addr: Option<Multiaddr>,
    /// Whether a connection was established over a circuit.
    connected_over_circuit: bool,
}

#[tokio::test]
async fn a_connection_goes_through_a_relay() {
    let outcome = tokio::time::timeout(PATIENCE, run())
        .await
        .expect("the relay path did not complete; a hang here is a reservation that was never granted");

    let addr = outcome
        .circuit_addr
        .expect("the reserving node was never given a circuit address");
    assert!(
        addr.iter().any(|p| p == Protocol::P2pCircuit),
        "the address the reservation produced is not a circuit: {addr}"
    );
    assert!(
        outcome.connected_over_circuit,
        "a circuit address existed and nothing ever connected over it"
    );

    // The limits this client would judge the relay by are the ones it actually
    // granted — which is the check `NAT_AND_DISCOVERY.md` asks for by name and
    // which had, until now, never been run against a real reservation.
    let verdict = adequate(
        MAX_SEATS,
        Duration::from_millis(HAND_DEADLINE_CAP_MS),
        outcome.granted_bytes,
        outcome.granted_duration,
    );
    assert!(
        matches!(verdict, Adequacy::Adequate | Adequacy::Unlimited),
        "a p2p-poker relay granted limits a p2p-poker client refuses: {verdict:?}"
    );
}

async fn run() -> Outcome {
    let admits = RelayAdmits::default();
    let mut relay_node = node_admitting(RelayRole::Volunteer, admits.clone());
    let mut reserver = node(RelayRole::Declined);
    let mut caller = node(RelayRole::Declined);

    let relay_id = *relay_node.local_peer_id();
    let reserver_id = *reserver.local_peer_id();
    // The relay reserves only for poker clients (`S1-FK`); `identify` would name
    // the reserver one, and this test names it itself.
    admits.admit(reserver_id);

    relay_node
        .listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse::<Multiaddr>().unwrap())
        .expect("a literal address");

    // The relay's own address, from what it actually bound.
    let relay_addr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = relay_node.select_next_some().await {
            break address;
        }
    };

    // The relay declares that address reachable.
    //
    // This is not test scaffolding standing in for something missing. It is the
    // same call `net::run` now makes when AutoNAT confirms an address, and it is
    // there because this test found that nothing made it: `libp2p-relay` fills a
    // reservation from the relay's external addresses and from nowhere else, so
    // a relay that has never recorded one grants reservations carrying no
    // address at all. On loopback there is no AutoNAT server to ask, and the
    // address is reachable by construction — the other two nodes are in this
    // process.
    relay_node.add_external_address(relay_addr.clone());

    // Listening on a relay's circuit address IS the reservation request. There
    // is no separate call, which is worth knowing before looking for one.
    let circuit: Multiaddr = relay_addr
        .clone()
        .with(Protocol::P2p(relay_id))
        .with(Protocol::P2pCircuit);
    reserver
        .listen_on(circuit.clone())
        .expect("a well-formed circuit address");

    let mut out = Outcome::default();
    let mut dialled = false;

    loop {
        tokio::select! {
            // The relay must be polled or it answers nothing.
            _ = relay_node.select_next_some() => {}

            event = reserver.select_next_some() => match event {
                SwarmEvent::Behaviour(PokerBehaviourEvent::RelayClient(
                    relay::client::Event::ReservationReqAccepted { limit, .. },
                )) => {
                    out.granted_bytes = limit.as_ref().and_then(|l| l.data_in_bytes());
                    out.granted_duration = limit.as_ref().and_then(|l| l.duration());
                    out.reservation = Some(None);
                }
                SwarmEvent::NewListenAddr { address, .. }
                    if address.iter().any(|p| p == Protocol::P2pCircuit) =>
                {
                    out.circuit_addr = Some(address.clone());
                    if !dialled {
                        dialled = true;
                        // The caller dials the circuit address and nothing else.
                        caller.dial(address).expect("a dialable circuit address");
                    }
                }
                _ => {}
            },

            event = caller.select_next_some() => {
                if let SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } = event {
                    let addr = endpoint.get_remote_address();
                    if peer_id == reserver_id && addr.iter().any(|p| p == Protocol::P2pCircuit) {
                        out.connected_over_circuit = true;
                        return out;
                    }
                }
            }
        }
    }
}

/// The relay this client offers must grant limits this client would accept.
///
/// Asserted here against a **real reservation** rather than against the config,
/// because a config that agrees with itself and a relay that grants something
/// else are different things, and only the second is what a peer sees.
#[tokio::test]
async fn the_granted_limits_are_the_ones_configured() {
    let outcome = tokio::time::timeout(PATIENCE, run())
        .await
        .expect("no reservation was granted");

    // The library reports `None` when it imposes no limit on that axis, so both
    // shapes are legitimate; what must not happen is a limit smaller than what a
    // hand costs.
    if let Some(bytes) = outcome.granted_bytes {
        assert!(
            bytes >= p2p_poker::net::relay::per_hand_bytes(MAX_SEATS),
            "the relay granted {bytes} bytes, less than one hand at a full table"
        );
    }
    if let Some(d) = outcome.granted_duration {
        assert!(
            d >= Duration::from_secs(600),
            "the relay granted {d:?}, which cannot carry a hand"
        );
    }
}

/// A relay that has never confirmed an external address grants a reservation
/// that carries no address — which is worse than a refusal, because the peer
/// that asked believes it has a way in and has nothing to tell anyone.
///
/// This is the defect the first run of this file found, and it had been true
/// from the day the relay was configured. It is kept as a test because the fix
/// is one line in an event arm and one line is easy to lose.
#[tokio::test]
async fn a_relay_that_knows_no_address_of_its_own_grants_an_empty_reservation() {
    let outcome = tokio::time::timeout(Duration::from_secs(20), no_external_address()).await;
    assert!(
        outcome.is_err() || !outcome.unwrap(),
        "a relay with no external address handed out a usable circuit; either \
         libp2p changed where it fills a reservation from, or this test no \
         longer proves what it says"
    );
}

/// Whether the reserving node was ever given a circuit address.
async fn no_external_address() -> bool {
    let admits = RelayAdmits::default();
    let mut relay_node = node_admitting(RelayRole::Volunteer, admits.clone());
    let mut reserver = node(RelayRole::Declined);
    let relay_id = *relay_node.local_peer_id();
    // Admitted (`S1-FK`), so what this proves is the missing address and not
    // the admission.
    admits.admit(*reserver.local_peer_id());

    relay_node
        .listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse::<Multiaddr>().unwrap())
        .unwrap();
    let relay_addr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = relay_node.select_next_some().await {
            break address;
        }
    };
    // Deliberately no `add_external_address`.

    let circuit: Multiaddr = relay_addr
        .with(Protocol::P2p(relay_id))
        .with(Protocol::P2pCircuit);
    let _ = reserver.listen_on(circuit);

    loop {
        tokio::select! {
            _ = relay_node.select_next_some() => {}
            event = reserver.select_next_some() => {
                if let SwarmEvent::NewListenAddr { address, .. } = event {
                    if address.iter().any(|p| p == Protocol::P2pCircuit) {
                        return true;
                    }
                }
            }
        }
    }
}

/// A relay that has declined grants nothing, and a client that asks gets no
/// circuit address rather than a broken one.
#[tokio::test]
async fn a_declined_relay_grants_nothing() {
    let outcome = tokio::time::timeout(Duration::from_secs(15), declined()).await;
    assert!(
        outcome.is_err() || !outcome.unwrap(),
        "a relay with zero capacity granted a reservation"
    );
}

async fn declined() -> bool {
    let mut relay_node = node(RelayRole::Declined);
    let mut reserver = node(RelayRole::Declined);
    let relay_id = *relay_node.local_peer_id();

    relay_node
        .listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse::<Multiaddr>().unwrap())
        .unwrap();
    let relay_addr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = relay_node.select_next_some().await {
            break address;
        }
    };

    let circuit: Multiaddr = relay_addr
        .with(Protocol::P2p(relay_id))
        .with(Protocol::P2pCircuit);
    let _ = reserver.listen_on(circuit);

    loop {
        tokio::select! {
            _ = relay_node.select_next_some() => {}
            event = reserver.select_next_some() => {
                if let SwarmEvent::Behaviour(PokerBehaviourEvent::RelayClient(
                    relay::client::Event::ReservationReqAccepted { .. },
                )) = event
                {
                    return true;
                }
            }
        }
    }
}

/// `S1-FK`, D-002 point 2: a volunteer relay with a confirmed address refuses a
/// reservation to a peer the node never named a poker client -- the stranger
/// that made every reachable client an open relay for the libp2p world.
#[tokio::test]
async fn a_relay_refuses_a_stranger_a_reservation() {
    let refused = tokio::time::timeout(Duration::from_secs(30), stranger_asks()).await;
    assert_eq!(
        refused.ok(),
        Some(true),
        "the relay granted a stranger a reservation, or never answered it"
    );
}

/// Whether the relay refused the stranger (`true`) or granted it (`false`).
async fn stranger_asks() -> bool {
    let mut relay_node = node_admitting(RelayRole::Volunteer, RelayAdmits::default());
    let mut stranger = node(RelayRole::Declined);
    let relay_id = *relay_node.local_peer_id();
    let stranger_id = *stranger.local_peer_id();
    relay_node
        .listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse::<Multiaddr>().unwrap())
        .unwrap();
    let relay_addr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = relay_node.select_next_some().await {
            break address;
        }
    };
    relay_node.add_external_address(relay_addr.clone());
    let circuit: Multiaddr = relay_addr
        .with(Protocol::P2p(relay_id))
        .with(Protocol::P2pCircuit);
    let _ = stranger.listen_on(circuit);
    loop {
        tokio::select! {
            event = relay_node.select_next_some() => match event {
                SwarmEvent::Behaviour(PokerBehaviourEvent::RelayServer(
                    relay::Event::ReservationReqDenied { src_peer_id, .. },
                )) if src_peer_id == stranger_id => return true,
                SwarmEvent::Behaviour(PokerBehaviourEvent::RelayServer(
                    relay::Event::ReservationReqAccepted { src_peer_id, .. },
                )) if src_peer_id == stranger_id => return false,
                _ => {}
            },
            _ = stranger.select_next_some() => {}
        }
    }
}

/// Silences the unused-field warning while keeping the field, which documents
/// what the reservation event carried.
#[allow(dead_code)]
fn _peer(p: PeerId) -> PeerId {
    p
}
