//! TEMPORARY refutation probe (delete after the run).
//!
//! Topology A --- B --- C, A and C never dialled to each other.
//! A publishes on the lobby topic; only B can carry it to C.
//! `report` toggles whether B calls `report_message_validation_result`.

use std::time::Duration;

use libp2p::futures::StreamExt;
use libp2p::{gossipsub, identity, swarm::SwarmEvent, Multiaddr, Swarm};

use p2p_poker::net::swarm::{
    self, NodeConfig, PokerBehaviour, PokerBehaviourEvent, RelayRole, Topics,
};

fn node() -> Swarm<PokerBehaviour> {
    swarm::build(NodeConfig {
        identity: identity::Keypair::generate_ed25519(),
        relay_role: RelayRole::Volunteer,
    })
    .expect("builds")
}

async fn two_hops(report: bool) -> bool {
    let (mut a, mut b, mut c) = (node(), node(), node());
    let topics = Topics::default();
    for s in [&mut a, &mut b, &mut c] {
        s.behaviour_mut().gossipsub.subscribe(&topics.lobby).unwrap();
    }
    let a_id = *a.local_peer_id();
    let b_id = *b.local_peer_id();
    let c_id = *c.local_peer_id();
    println!("A={a_id} B={b_id} C={c_id} (report={report})");

    b.listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse::<Multiaddr>().unwrap())
        .unwrap();
    let b_addr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = b.select_next_some().await {
            break address;
        }
    };
    a.dial(b_addr.clone()).unwrap();
    c.dial(b_addr).unwrap();

    let mut ready = false;
    let mut tick = tokio::time::interval(Duration::from_millis(300));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(25);

    loop {
        if tokio::time::Instant::now() > deadline {
            println!("TIMEOUT (report={report}); C connected to A: {}", c.is_connected(&a_id));
            return false;
        }
        tokio::select! {
            e = a.select_next_some() => {
                if let SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                    gossipsub::Event::Subscribed { .. })) = e { ready = true; }
            }
            e = b.select_next_some() => {
                if let SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                    gossipsub::Event::Message { message_id, propagation_source, .. })) = e
                {
                    println!("B received from {propagation_source} (report={report})");
                    if report {
                        let hit = b.behaviour_mut().gossipsub
                            .report_message_validation_result(
                                &message_id,
                                &propagation_source,
                                gossipsub::MessageAcceptance::Accept,
                            );
                        println!("  B reported Accept -> found in cache: {hit}");
                    }
                }
            }
            e = c.select_next_some() => {
                if let SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                    gossipsub::Event::Message { propagation_source, .. })) = e
                {
                    let direct = c.is_connected(&a_id);
                    let via_b = propagation_source == b_id;
                    let peers: Vec<String> =
                        c.connected_peers().map(|p| p.to_string()).collect();
                    println!("C RECEIVED (report={report}) from {propagation_source};                               via_b={via_b} C-A direct={direct} C peers={peers:?}");
                    assert!(!direct, "topology broke: C ended up connected to A");
                    assert!(via_b, "C did not receive it from B");
                    return true;
                }
            }
            _ = tick.tick(), if ready => {
                let _ = a.behaviour_mut().gossipsub
                    .publish(topics.lobby.clone(), b"x".repeat(64));
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn relay_as_the_code_is_written() {
    let got = two_hops(false).await;
    println!("AS WRITTEN, C received: {got}");
}

#[tokio::test(flavor = "multi_thread")]
async fn relay_with_validation_reported() {
    let got = two_hops(true).await;
    println!("WITH REPORT, C received: {got}");
}
