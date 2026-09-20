//! Two nodes, one process: a table **formed over a real connection**.
//!
//! `tests/two_nodes.rs` proved an advert crosses the mesh. This proves the next
//! thing, which is the one `NEXT.md` has had at number two for weeks: a joiner
//! asks, a founder answers, a roster is proposed, both seats ratify it, and the
//! two clients arrive at the **same `session_id`** — the value every subsequent
//! hand's genesis contains.
//!
//! Everything here travels: the join RPC over `/p2p-poker/join/1` on a QUIC
//! connection, and `PLAYER_LIST` and `TABLE_READY` over the table's own
//! GossipSub topic. Nothing is handed between the two `Formation` values in
//! memory.
//!
//! # What this does not cover
//!
//! NAT, again, and for the same reason as everywhere else: two instances on one
//! machine are the hardest case for it rather than the easiest. This dials
//! loopback directly and says so.

use std::time::Duration;

use libp2p::futures::StreamExt;
use libp2p::{gossipsub, identity, request_response, swarm::SwarmEvent, Multiaddr, Swarm};

use ed25519_dalek::SigningKey;
use p2p_poker::net::formation::{Failed, Formation, Send as Emit};
use p2p_poker::net::joinrpc;
use p2p_poker::net::joinwire;
use p2p_poker::net::lobby::{BlindSchedule, Mode, TableAd, DECK_SUITE_V1};
use p2p_poker::net::swarm::{self, NodeConfig, ShapedBehaviour, PokerBehaviourEvent, RelayRole};
use p2p_poker::protocol::constants::hand_deadline_min_ms;
use p2p_poker::protocol::messages::SignedEvent;
use p2p_poker::protocol::serialization::from_canonical;
use p2p_poker::protocol::transcript::event_hash;

const NOW: u64 = 1_700_000_000_000;
const PATIENCE: Duration = Duration::from_secs(60);

fn node() -> Swarm<ShapedBehaviour> {
    swarm::build(NodeConfig {
        identity: identity::Keypair::generate_ed25519(),
        // Off. These dial a loopback address and then assert two peers met;
        // with multicast on, a client running elsewhere on this machine could
        // satisfy that assertion and the test would pass for the wrong reason.
        local_discovery: false,
        relay_role: RelayRole::Volunteer,
        relay_admits: Default::default(),
    })
    .expect("the stack builds")
}

fn ad(founder_app: [u8; 32], founder_peer: Vec<u8>) -> TableAd {
    let (action, grace, crypto, delay) = (20_000u32, 5_000u32, 30_000u32, 7_000u32);
    let seats = 6u8;
    TableAd {
        game: 1,
        mode: Mode::CashPlayMoney.code(),
        preset_id: "CUSTOM".into(),
        table_name: "Riverside".into(),
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
        founder_app_key: founder_app,
        founder_peer_id: founder_peer,
        timestamp_unix_ms: NOW,
        expires_at_unix_ms: NOW + 90_000,
        founder_tox_key: None,
        tox_chat_id: None,
    }
}

/// Whether a frame is a `PLAYER_LIST`, so the test routes it the way a client
/// does: by what it decodes as, not by what it hoped it would be.
fn is_list(bytes: &[u8]) -> bool {
    joinwire::receive_player_list(bytes).is_ok()
}

#[tokio::test]
async fn two_clients_form_a_table_over_a_real_connection() {
    let (session_a, session_b) = tokio::time::timeout(PATIENCE, run())
        .await
        .expect("formation did not complete; a hang here is a message that never arrived");

    assert_eq!(
        session_a, session_b,
        "the two seats computed different session identities, so every hand's \
         genesis would differ and nothing either of them signed would verify at \
         the other"
    );
}

async fn run() -> ([u8; 32], [u8; 32]) {
    let mut host = node();
    let mut guest = node();

    let host_peer = *host.local_peer_id();
    let guest_peer = *guest.local_peer_id();

    host.listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse::<Multiaddr>().unwrap())
        .expect("a literal address");
    let host_addr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = host.select_next_some().await {
            break address;
        }
    };
    guest.dial(host_addr).expect("a dialable address");

    // The two application keys, which are not the two network identities.
    let host_app = SigningKey::from_bytes(&[11u8; 32]);
    let guest_app = SigningKey::from_bytes(&[22u8; 32]);
    let table_key = SigningKey::from_bytes(&[33u8; 32]);

    let advert = ad(host_app.verifying_key().to_bytes(), host_peer.to_bytes());
    let event = p2p_poker::net::advert::publish(&advert, &table_key).expect("the advert publishes");
    let advert_hash = {
        let signed: SignedEvent = from_canonical(&event, 8_192).unwrap();
        event_hash(&signed.body)
    };
    let table_id = table_key.verifying_key().to_bytes();
    let topic = joinrpc::table_topic(&table_id);

    let mut founder = Formation::found(
        host_app,
        table_key,
        advert.clone(),
        event,
        advert_hash,
        None,
        host_peer.to_bytes(),
        "host".into(),
        1_000,
    )
    .expect("the founder seats itself");

    let (mut joiner, request) = Formation::join(
        guest_app,
        advert,
        advert_hash,
        table_id,
        guest_peer.to_bytes(),
        "guest".into(),
        1_000,
        None,
        None,
        [7u8; 32],
        NOW,
        None,
    )
    .expect("the request builds");

    host.behaviour_mut().gossipsub.subscribe(&topic).unwrap();
    guest.behaviour_mut().gossipsub.subscribe(&topic).unwrap();

    let mut asked = false;
    let mut connected = false;
    // The table's own topic needs a mesh before anything published on it goes
    // anywhere. Publishing into an empty mesh is not an error and not a
    // delivery, so the request waits for a peer on the topic rather than for a
    // connection.
    let mut mesh = false;

    loop {
        if let (Some(a), Some(b)) = (founder.session(), joiner.session()) {
            return (a, b);
        }

        tokio::select! {
            event = host.select_next_some() => match event {
                SwarmEvent::ConnectionEstablished { .. } => connected = true,
                SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                    gossipsub::Event::Subscribed { topic: t, .. },
                )) if t == topic.hash() => mesh = true,
                SwarmEvent::Behaviour(PokerBehaviourEvent::Join(
                    request_response::Event::Message {
                        peer,
                        message: request_response::Message::Request { request, channel, .. },
                        ..
                    },
                )) => {
                    // What the transport authenticated, which is what §4.3
                    // compares the request's own claim against.
                    let out = founder
                        .on_join_request(&request, &peer.to_bytes(), false, NOW)
                        .expect("an honest request is seated");
                    let mut channel = Some(channel);
                    for send in out {
                        match send {
                            Emit::Reply(bytes) => {
                                if let Some(c) = channel.take() {
                                    host.behaviour_mut().join.send_response(c, bytes).unwrap();
                                }
                            }
                            Emit::Broadcast(bytes) => {
                                host.behaviour_mut()
                                    .gossipsub
                                    .publish(topic.clone(), bytes)
                                    .expect("the table topic has a peer on it");
                            }
                        }
                    }
                }
                // A founder hears its own proposal back from nobody, so
                // anything arriving on the table topic is the other seat's
                // ratification.
                SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                    gossipsub::Event::Message { message, .. },
                )) if !is_list(&message.data) => {
                    let _ = founder.on_table_ready(&message.data);
                }
                _ => {}
            },

            event = guest.select_next_some() => match event {
                SwarmEvent::ConnectionEstablished { .. } => connected = true,
                SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                    gossipsub::Event::Subscribed { topic: t, .. },
                )) if t == topic.hash() => mesh = true,
                SwarmEvent::Behaviour(PokerBehaviourEvent::Join(
                    request_response::Event::Message {
                        message: request_response::Message::Response { response, .. },
                        ..
                    },
                )) => {
                    match joiner.on_join_answer(&response, NOW) {
                        Ok(_) => {}
                        Err(Failed::Refused { reason, .. }) => {
                            panic!("the founder refused an honest join: reason {reason}")
                        }
                        Err(e) => panic!("the acceptance did not hold: {e:?}"),
                    }
                }
                SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                    gossipsub::Event::Message { message, .. },
                )) => {
                    if is_list(&message.data) {
                        for send in joiner
                            .on_player_list(&message.data, NOW)
                            .expect("the founder's list holds")
                        {
                            if let Emit::Broadcast(bytes) = send {
                                guest
                                    .behaviour_mut()
                                    .gossipsub
                                    .publish(topic.clone(), bytes)
                                    .expect("the table topic has a peer on it");
                            }
                        }
                    } else {
                        let _ = joiner.on_table_ready(&message.data);
                    }
                }
                _ => {}
            },
        }

        if connected && mesh && !asked {
            asked = true;
            guest
                .behaviour_mut()
                .join
                .send_request(&host_peer, request.clone());
        }
    }
}

/// The founder's own seat is in the roster it proposes, and the joiner's is the
/// one the founder gave it — not the one it asked for and not seat zero.
#[tokio::test]
async fn the_two_seats_are_the_ones_the_founder_assigned() {
    // Formed in memory rather than over the wire: this is a question about the
    // roster, and the wire is covered above.
    let host_app = SigningKey::from_bytes(&[11u8; 32]);
    let guest_app = SigningKey::from_bytes(&[22u8; 32]);
    let table_key = SigningKey::from_bytes(&[33u8; 32]);

    let advert = ad(host_app.verifying_key().to_bytes(), vec![1u8; 38]);
    let event = p2p_poker::net::advert::publish(&advert, &table_key).unwrap();
    let advert_hash = {
        let signed: SignedEvent = from_canonical(&event, 8_192).unwrap();
        event_hash(&signed.body)
    };

    let mut founder = Formation::found(
        host_app,
        table_key.clone(),
        advert.clone(),
        event,
        advert_hash,
        None,
        vec![1u8; 38],
        "host".into(),
        1_000,
    )
    .unwrap();

    let (mut joiner, request) = Formation::join(
        guest_app.clone(),
        advert,
        advert_hash,
        table_key.verifying_key().to_bytes(),
        vec![2u8; 38],
        "guest".into(),
        1_000,
        None,
        None,
        [7u8; 32],
        NOW,
        None,
    )
    .unwrap();

    for send in founder
        .on_join_request(&request, &[2u8; 38], false, NOW)
        .unwrap()
    {
        match send {
            Emit::Reply(bytes) => {
                joiner.on_join_answer(&bytes, NOW).unwrap();
            }
            Emit::Broadcast(bytes) => {
                if is_list(&bytes) {
                    joiner.on_player_list(&bytes, NOW).unwrap();
                }
            }
        }
    }

    assert_eq!(founder.my_seat(), Some(0), "the founder takes seat zero");
    assert_eq!(joiner.my_seat(), Some(1), "the joiner takes the next one");
    assert_eq!(founder.roster().len(), 2);
    assert_eq!(
        founder.roster().seat_of(&guest_app.verifying_key().to_bytes()),
        Some(1)
    );
}
