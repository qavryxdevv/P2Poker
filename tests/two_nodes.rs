//! Two nodes, one process: the transport path end to end, automatically.
//!
//! Everything the manual two-binary run demonstrated, as a test that runs on
//! every `cargo test`: two independent libp2p stacks with two identities, a real
//! QUIC handshake, a real GossipSub mesh, and a real signed table advert that
//! the receiving side decodes, verifies and admits under §7.2's rules.
//!
//! # What this covers, and what it deliberately cannot
//!
//! **Covers:** the swarm builds, the transports bind, two peers complete a
//! security handshake, the mesh forms, an advert crosses it, and the receiver's
//! whole admission pipeline runs on bytes that actually travelled.
//!
//! **Does not cover, and no single-host test can:** NAT. Two instances on one
//! machine are the *hardest* case for it, not the easiest — both learn the same
//! external address and dialling it inwards is hairpinning, which many routers
//! refuse. The DHT, the relay reservation and DCUtR are exercised by a run
//! across two networks and by nothing else, and that is `NEXT.md`'s first item
//! rather than something this file quietly implies it has done.
//!
//! So this test dials a **loopback address directly**. It is not pretending to
//! be a network test; it is the transport-and-protocol test that was being done
//! by hand, made automatic.

use std::time::Duration;

use libp2p::futures::StreamExt;
use libp2p::{gossipsub, identity, swarm::SwarmEvent, Multiaddr, Swarm};

use p2p_poker::net::advert::{publish, receive};
use p2p_poker::net::lobby::{BlindSchedule, LobbyStore, Mode, RateLimiter, TableAd, DECK_SUITE_V1};
use p2p_poker::net::swarm::{self, NodeConfig, PokerBehaviour, PokerBehaviourEvent, RelayRole, Topics};
use p2p_poker::protocol::constants::hand_deadline_min_ms;

const NOW: u64 = 1_700_000_000_000;

/// The whole test must finish inside this, or it has hung rather than failed.
const PATIENCE: Duration = Duration::from_secs(30);

fn node() -> Swarm<PokerBehaviour> {
    swarm::build(NodeConfig {
        identity: identity::Keypair::generate_ed25519(),
        // The relay server is irrelevant here and its capacity costs nothing.
        // Off. These dial a loopback address and then assert two peers met;
        // with multicast on, a client running elsewhere on this machine could
        // satisfy that assertion and the test would pass for the wrong reason.
        local_discovery: false,
        relay_role: RelayRole::Volunteer,
        relay_admits: Default::default(),
    })
    .expect("the stack builds")
}

fn demo_ad(name: &str) -> TableAd {
    let (action, grace, crypto, delay) = (20_000u32, 5_000u32, 30_000u32, 7_000u32);
    let seats = 6u8;
    TableAd {
        game: 1,
        mode: Mode::CashPlayMoney.code(),
        preset_id: "CUSTOM".into(),
        table_name: name.into(),
        small_blind: 10,
        big_blind: 20,
        ante: 0,
        min_buyin: 200,
        max_buyin: 2_000,
        start_stack: 0,
        players: 1,
        max_players: seats,
        min_players_to_start: 2,
        blind_schedule: BlindSchedule {
            mode: 1,
            every_n_hands: 20,
            first_small_blind: 10,
            small_blind_cap: 1_000,
        },
        action_timeout_ms: action,
        action_grace_ms: grace,
        crypto_step_timeout_ms: crypto,
        hand_deadline_ms: hand_deadline_min_ms(
            seats,
            action as u64,
            grace as u64,
            crypto as u64,
            delay as u64,
            0,
        ) as u32,
        join_deadline_ms: 120_000,
        hand_delay_ms: delay,
        time_bank_ms: 0,
        button_rule: 1,
        odd_chip_rule: 1,
        showdown_policy: 1,
        password_required: false,
        deck_suite: DECK_SUITE_V1.into(),
        founder_app_key: [7u8; 32],
        founder_peer_id: Vec::new(),
        timestamp_unix_ms: NOW,
        expires_at_unix_ms: NOW + 90_000,
        founder_tox_key: None,
        tox_chat_id: None,
    }
}

/// A table is signed by one node, crosses a real mesh, and is admitted by the
/// other.
#[tokio::test]
async fn a_table_crosses_between_two_nodes() {
    let outcome = tokio::time::timeout(PATIENCE, run()).await;
    match outcome {
        Ok(store) => {
            assert_eq!(store.len(), 1, "the watcher holds exactly one table");
            let (_, held) = store.tables().next().map(|l| (l.key, l.held)).unwrap();
            assert_eq!(held.ad.table_name, "Riverside");
            assert!(!held.unjoinable);
        }
        Err(_) => panic!(
            "the two nodes did not exchange a table within {PATIENCE:?}; \
             a hang here is a mesh that never formed, not a rejected advert"
        ),
    }
}

async fn run() -> LobbyStore {
    let mut host = node();
    let mut watcher = node();
    let topics = Topics::default();

    for s in [&mut host, &mut watcher] {
        s.behaviour_mut()
            .gossipsub
            .subscribe(&topics.lobby)
            .expect("a fresh topic");
    }

    // Loopback, and deliberately: see the module note. This is the transport
    // test, not the NAT one.
    host.listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse::<Multiaddr>().unwrap())
        .expect("a literal address");

    // Wait for the host's real address, then dial it.
    let host_addr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = host.select_next_some().await {
            break address;
        }
    };
    watcher.dial(host_addr.clone()).expect("a dialable address");

    let table_key = ed25519_dalek::SigningKey::from_bytes(&[42u8; 32]);
    let mut store = LobbyStore::new();
    let mut limits = RateLimiter::new();

    let mut host_ready = false;
    let mut published = false;
    let mut republish = tokio::time::interval(Duration::from_millis(250));

    loop {
        tokio::select! {
            event = host.select_next_some() => {
                if let SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                    gossipsub::Event::Subscribed { .. },
                )) = event
                {
                    // Until a peer has subscribed, `publish` returns
                    // `NoPeersSubscribedToTopic` - the ordinary state at
                    // start-up, and the reason the manual run's first publish
                    // failed.
                    host_ready = true;
                }
            }

            event = watcher.select_next_some() => {
                if let SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                    gossipsub::Event::Message { message, propagation_source, .. },
                )) = event
                {
                    let mut from = [0u8; 32];
                    let bytes = propagation_source.to_bytes();
                    let take = bytes.len().min(32);
                    from[..take].copy_from_slice(&bytes[..take]);

                    receive(&message.data, from, NOW, &mut limits, &mut store)
                        .expect("an honest advert, over a real mesh");
                    return store;
                }
            }

            _ = republish.tick(), if host_ready => {
                // Re-published until it lands. A mesh that has just formed can
                // still drop the first message, and a test that published once
                // would be timing-dependent for no reason.
                let wire = publish(&demo_ad("Riverside"), &table_key)
                    .expect("the advert encodes");
                let _ = host
                    .behaviour_mut()
                    .gossipsub
                    .publish(topics.lobby.clone(), wire);
                published = true;
            }
        }
        assert!(
            !published || host_ready,
            "publishing before a peer subscribed cannot succeed"
        );
    }
}
