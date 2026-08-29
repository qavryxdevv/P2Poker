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
use libp2p::{
    futures::StreamExt as SwarmStreamExt, gossipsub, identity, request_response, swarm::SwarmEvent,
    Multiaddr, PeerId,
};
use mainline::{Dht, Id};
use tokio::sync::mpsc;

use std::collections::HashSet;
use std::net::SocketAddrV4;

use super::advert;
use super::formation::{Failed, Formation, Send};
use super::joinrpc;
use super::joinwire;
use super::portmap;
use super::dht::{self, PeerHints, Swarm as DhtSwarm, REANNOUNCE_INTERVAL};
use super::relay;
use super::lobby::{LobbyStore, RateLimiter, TableAd, TableKind};
use super::node::{quic_dial_addr, worth_parsing, NodeCommand, NodeEvent, NodeState, REBROADCAST};
use super::swarm::{self, NodeConfig, PokerBehaviourEvent, RelayRole, Topics};
use super::joinwire::DISPLAY_NAME_MAX;
use crate::protocol::constants::{
    AD_TTL_MS, HAND_DEADLINE_CAP_MS, LOBBY_MSG_MAX, MAX_SEATS,
};

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
    app_key: ed25519_dalek::SigningKey,
    events: mpsc::Sender<NodeEvent>,
    mut commands: mpsc::Receiver<NodeCommand>,
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
    let my_peer_bytes = swarm.local_peer_id().to_bytes();
    // This node's own application key, used as the "from peer" when it files its
    // own advertisement. It is charged the same rate limit as anybody else,
    // which at one re-broadcast every thirty seconds is half the allowance.
    let my_app_key = app_key.verifying_key().to_bytes();
    // Until the interface says otherwise, eight characters of this player's own
    // key: stable, theirs, and never confusable with somebody else's.
    let mut nickname = crate::storage::profile::short_name(&my_app_key);

    // The table this client is forming or sitting at, and the mesh it is formed
    // on. One table at a time in this loop; multi-tabling is more than one
    // client, which is what §4.3's per-roster `peer_id` rule already allows and
    // what the lobby's own rules are written for.
    let mut table: Option<Formation> = None;
    let mut table_topic: Option<gossipsub::IdentTopic> = None;

    // Who mDNS has already told us about.
    //
    // It keeps telling us. The query goes out on a timer and every neighbour
    // answers it, so the same peer is "discovered" over and over for as long as
    // it is switched on. Announcing each answer costs a line in the log and,
    // through it, a repaint of the whole window — which on a machine with no
    // graphics driver is half a second of processor time for news that is not
    // news. Dialling each answer costs a connection attempt to a peer already
    // connected.
    //
    // Emptied by `Expired`, so a neighbour that goes away and comes back is
    // announced again. Without that half of it this would be a set that only
    // ever grows and a peer that could never be rediscovered.
    let mut seen_locally: std::collections::HashSet<libp2p::PeerId> =
        std::collections::HashSet::new();

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
    // The router, asked to open the port this client is listening on. PCP and
    // NAT-PMP, beside the UPnP already in the behaviour: a router that speaks
    // one of the three is a player who does not need a relay, and a hand over a
    // relay costs somebody else's bandwidth.
    //
    // Held so that renewing it is possible and so that dropping this task drops
    // the mapping — a client that exits without releasing leaves a hole in a
    // stranger's router until it expires.
    // The router, asked to open the port this client listens on: PCP and
    // NAT-PMP, beside the UPnP already in the behaviour. A router that speaks
    // one of the three is a player who does not need a relay, and a hand over a
    // relay costs somebody else's bandwidth.
    //
    // In **its own task**, because both protocols are UDP to an address that may
    // not answer and asking inside this loop stops the whole node while a router
    // that is not there fails to reply. It ends by itself when the event channel
    // closes, and hands the port back on the way out.

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
                            // And ask the router to open it, once, in its own
                            // task. Started here rather than on a timer because
                            // this is the moment there is something to ask about.
                            if let Some(port) = announced_port {
                                tokio::spawn(portmap::keep_open(port, events.clone()));
                            }
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
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Join(
                        request_response::Event::Message { peer, message, .. },
                    )) => {
                        let now = super::node::now_unix_ms();
                        match message {
                            request_response::Message::Request { request, channel, .. } => {
                                // `peer` is what the **transport** authenticated,
                                // and it is what §4.3 compares the request's own
                                // claim against. The whole of `U17` — one node,
                                // one seat — rests on using this and not the
                                // value in the payload.
                                let authenticated = peer.to_bytes();
                                let Some(f) = table.as_mut() else {
                                    // No table here to join. Saying nothing is
                                    // right: an answer would confirm that this
                                    // client is a founder of something.
                                    continue;
                                };
                                match f.on_join_request(&request, &authenticated, now) {
                                    Ok(sends) => {
                                        // The channel is consumed by the one
                                        // reply and the rest of the sends go to
                                        // the table's topic. An earlier version
                                        // stopped at the reply, which threw away
                                        // the roster announcement and this
                                        // client's own ratification — the joiner
                                        // took its seat and then both sides sat
                                        // waiting for a message that had been
                                        // dropped on the floor here.
                                        let mut channel = Some(channel);
                                        for send in sends {
                                            match send {
                                                Send::Reply(bytes) => {
                                                    if let Some(c) = channel.take() {
                                                        let _ = swarm
                                                            .behaviour_mut()
                                                            .join
                                                            .send_response(c, bytes);
                                                    }
                                                }
                                                Send::Broadcast(bytes) => {
                                                    if let Some(t) = &table_topic {
                                                        let _ = swarm
                                                            .behaviour_mut()
                                                            .gossipsub
                                                            .publish(t.clone(), bytes);
                                                    }
                                                }
                                            }
                                        }
                                        report_roster(&events, f).await;
                                        if let Some(session) = f.session() {
                                            let _ = events.send(NodeEvent::TableReal {
                                                key: f.table_id(),
                                                session,
                                            }).await;
                                        }
                                    }
                                    Err(e) => {
                                        // A structurally invalid request gets no
                                        // answer at all, which is what
                                        // `Formation` returns an error for.
                                        let _ = events.send(NodeEvent::Warning(
                                            format!("a join request was dropped: {e:?}"))).await;
                                    }
                                }
                            }
                            request_response::Message::Response { response, .. } => {
                                let Some(f) = table.as_mut() else { continue };
                                match f.on_join_answer(&response, now) {
                                    Ok(_) => {
                                        if let Some(seat) = f.my_seat() {
                                            let _ = events.send(NodeEvent::Seated {
                                                key: f.table_id(),
                                                seat,
                                            }).await;
                                        }
                                        report_params(&events, f).await;
                                        report_roster(&events, f).await;
                                    }
                                    Err(Failed::Refused { reason, .. }) => {
                                        table = None;
                                        if let Some(t) = table_topic.take() {
                                            let _ = swarm.behaviour_mut().gossipsub.unsubscribe(&t);
                                        }
                                        let _ = events
                                            .send(NodeEvent::JoinRefused { reason })
                                            .await;
                                    }
                                    Err(e) => {
                                        table = None;
                                        let _ = events.send(NodeEvent::LeftTable {
                                            why: format!("the acceptance did not hold: {e:?}"),
                                        }).await;
                                    }
                                }
                            }
                        }
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Join(
                        request_response::Event::OutboundFailure { error, .. },
                    )) => {
                        // A join that goes unanswered has to say so. Silence
                        // here is a player looking at a button that appears to
                        // have done nothing.
                        table = None;
                        if let Some(t) = table_topic.take() {
                            let _ = swarm.behaviour_mut().gossipsub.unsubscribe(&t);
                        }
                        let _ = events.send(NodeEvent::LeftTable {
                            why: format!("the founder did not answer: {error}"),
                        }).await;
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                        gossipsub::Event::Message {
                            message,
                            propagation_source,
                            message_id,
                        },
                    )) if table_topic
                        .as_ref()
                        .is_some_and(|t| message.topic == t.hash()) =>
                    {
                        // The table mesh. Everything on it is either the
                        // founder's proposal or a seat's ratification of one,
                        // and which it is is decided by what it decodes as
                        // rather than by what was hoped for.
                        //
                        // **This arm must answer, exactly like the lobby's.**
                        // With `validate_messages()` set, GossipSub forwards
                        // nothing until the application reports a verdict, and
                        // this arm reported none — so every client was a black
                        // hole for its own table's traffic. Two seats did not
                        // notice, because they are directly meshed and delivery
                        // to a direct peer does not need forwarding; at ten
                        // seats a ratification that has to travel through a
                        // third seat simply never arrives, and the table never
                        // starts with nobody able to say why.
                        let verdict = match table.as_mut() {
                            None => {
                                // No table here to judge it against. `Ignore`
                                // and not `Reject`: nothing about the message is
                                // known to be wrong, this client just cannot
                                // tell, and blaming a sender for that would cost
                                // an honest peer its score.
                                let _ = swarm.behaviour_mut().gossipsub
                                    .report_message_validation_result(
                                        &message_id,
                                        &propagation_source,
                                        gossipsub::MessageAcceptance::Ignore,
                                    );
                                continue;
                            }
                            Some(_) => gossipsub::MessageAcceptance::Accept,
                        };
                        let _ = verdict;
                        let Some(f) = table.as_mut() else { continue };
                        let now = super::node::now_unix_ms();
                        let result = if joinwire::receive_player_list(&message.data).is_ok() {
                            f.on_player_list(&message.data, now)
                        } else {
                            f.on_table_ready(&message.data)
                        };
                        let refused = result.is_err();
                        match result {
                            Ok(sends) => {
                                if let Some(t) = &table_topic {
                                    for send in sends {
                                        if let Send::Broadcast(bytes) = send {
                                            let _ = swarm
                                                .behaviour_mut()
                                                .gossipsub
                                                .publish(t.clone(), bytes);
                                        }
                                    }
                                }
                                report_roster(&events, f).await;
                                if let Some(session) = f.session() {
                                    let _ = events
                                        .send(NodeEvent::TableReal {
                                            key: f.table_id(),
                                            session,
                                        })
                                        .await;
                                }
                            }
                            Err(e) => {
                                let _ = events
                                    .send(NodeEvent::Warning(format!(
                                        "a table message was refused: {e:?}"
                                    )))
                                    .await;
                            }
                        }

                        // A signed message that failed a rule is the sender's
                        // fault and costs it peer score; one this client took is
                        // passed on.
                        let _ = swarm.behaviour_mut().gossipsub
                            .report_message_validation_result(
                                &message_id,
                                &propagation_source,
                                if refused {
                                    gossipsub::MessageAcceptance::Reject
                                } else {
                                    gossipsub::MessageAcceptance::Accept
                                },
                            );
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
                            // The address is taken every time even for a peer
                            // already known: mDNS reports one entry per address,
                            // and a neighbour that gains one — a second adapter,
                            // a new lease — is offering a route this node does
                            // not have yet. It is only the *announcement* that
                            // is once per peer.
                            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer);
                            if seen_locally.insert(peer) {
                                let _ = swarm.dial(addr);
                                let _ = events.send(NodeEvent::LocalPeer(peer)).await;
                            }
                        }
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Mdns(
                        libp2p::mdns::Event::Expired(gone),
                    )) => {
                        // Forgotten, so the next answer from this peer counts as
                        // news again. Nothing else is done: an expired mDNS
                        // record says the advertisement lapsed, not that the
                        // connection is down, and dropping a live connection
                        // because a multicast packet went missing would be the
                        // worst possible reading of it.
                        for (peer, _addr) in gone {
                            seen_locally.remove(&peer);
                        }
                    }
                    SwarmEvent::Behaviour(PokerBehaviourEvent::Gossipsub(
                        gossipsub::Event::Subscribed { peer_id, topic: t },
                    )) => {
                        // Somebody new on this table's topic. GossipSub
                        // delivers to whoever is on a topic when a message is
                        // published and to nobody afterwards, so a roster
                        // announced a moment before they arrived was never sent
                        // to them at all. Say it again.
                        if let (Some(f), Some(mine)) = (table.as_ref(), table_topic.as_ref()) {
                            if t == mine.hash() {
                                for bytes in f.say_again() {
                                    let _ = swarm
                                        .behaviour_mut()
                                        .gossipsub
                                        .publish(mine.clone(), bytes);
                                }
                            }
                        }

                        // The first peer on the **lobby** topic, and this client
                        // is hosting a table nothing has heard yet.
                        //
                        // A client that founds a table before anybody is on the
                        // topic publishes into an empty mesh: not an error, and
                        // not a delivery. The table then waited for the next
                        // housekeeping tick — up to half a minute in which the
                        // founder's own lobby, and everybody else's, showed
                        // nothing. The failure was "no peers subscribed", so the
                        // repair is "publish when one subscribes", and it is the
                        // moment rather than a shorter timer.
                        if t == topics.lobby.hash() {
                            if let Some(f) = table.as_mut().filter(|f| f.is_founder()) {
                                let now = super::node::now_unix_ms();
                                if !state.lobby.tables().any(|l| *l.key == f.table_id()) {
                                    if let Ok(bytes) = f.readvertise(now, AD_TTL_MS) {
                                        if swarm
                                            .behaviour_mut()
                                            .gossipsub
                                            .publish(topics.lobby.clone(), bytes.clone())
                                            .is_ok()
                                        {
                                            show_own_table(
                                                &bytes,
                                                &my_app_key,
                                                now,
                                                &mut state.limits,
                                                &mut state.lobby,
                                                &events,
                                            )
                                            .await;
                                        }
                                    }
                                }
                            }
                        }
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

            Some(command) = commands.recv() => {
                let now = super::node::now_unix_ms();
                match command {
                    NodeCommand::CreateTable {
                        kind, name, seats, min_players, buyin, password,
                    } => {
                        // A fresh key per table, and that freshness is the only
                        // thing making two tables with the same players and the
                        // same rules different games (§4.3's `session_id`).
                        let seed = match crate::security::rng::secret_32() {
                            Ok(s) => s,
                            Err(e) => {
                                let _ = events.send(NodeEvent::Warning(
                                    format!("no randomness for a table key: {e}"))).await;
                                continue;
                            }
                        };
                        let table_key = ed25519_dalek::SigningKey::from_bytes(&seed);
                        let tournament = kind == TableKind::SitAndGo;
                        // A Sit-and-Go's structure is settled, so it ignores the
                        // three fields the dialog collects for a cash table. The
                        // password is dropped rather than carried: a rated table
                        // may not have one, and a Sit-and-Go that people are
                        // waiting to fill is the wrong place for a gate anyway.
                        let (ad, password) = if tournament {
                            (
                                TableAd::sng(
                                    seats,
                                    trim_to_bytes(&name, TABLE_NAME_MAX),
                                    app_key.verifying_key().to_bytes(),
                                    my_peer_bytes.clone(),
                                    now,
                                ),
                                None,
                            )
                        } else {
                            (
                                new_table(
                                    name,
                                    seats,
                                    min_players,
                                    buyin,
                                    password.is_some(),
                                    app_key.verifying_key().to_bytes(),
                                    my_peer_bytes.clone(),
                                    now,
                                ),
                                password,
                            )
                        };
                        // A tournament pays every entrant the same stack, so the
                        // founder's own seat takes the start stack and not what
                        // the dialog offered — `SeatEntry::admissible` admits
                        // exactly one value there.
                        let buyin = if tournament { ad.start_stack } else { buyin };
                        match advert::publish(&ad, &table_key) {
                            Ok(bytes) => {
                                let hash = advert_hash_of(&bytes);
                                match Formation::found(
                                    app_key.clone(),
                                    table_key.clone(),
                                    ad.clone(),
                                    bytes.clone(),
                                    hash,
                                    password,
                                    my_peer_bytes.clone(),
                                    nickname.clone(),
                                    buyin,
                                ) {
                                    Ok(f) => {
                                        let key = f.table_id();
                                        let topic = joinrpc::table_topic(&key);
                                        let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic);
                                        table_topic = Some(topic);
                                        table = Some(f);
                                        let _ = events.send(NodeEvent::Hosting { key }).await;
                                        report_params(&events, table.as_ref().unwrap()).await;
                                        report_roster(&events, table.as_ref().unwrap()).await;
                                        // Published first, shown second: a table
                                        // appears in its founder's own lobby when
                                        // the network has taken it, and not when
                                        // it was signed.
                                        match swarm
                                            .behaviour_mut()
                                            .gossipsub
                                            .publish(topics.lobby.clone(), bytes.clone())
                                        {
                                            Ok(_) => {
                                                show_own_table(
                                                    &bytes,
                                                    &my_app_key,
                                                    now,
                                                    &mut state.limits,
                                                    &mut state.lobby,
                                                    &events,
                                                )
                                                .await;
                                            }
                                            Err(e) => {
                                                // No mesh peer yet is the
                                                // ordinary case at start-up. The
                                                // table is real and will be
                                                // advertised again in thirty
                                                // seconds; it is not in anybody's
                                                // lobby until then, including
                                                // this one.
                                                let _ = events
                                                    .send(NodeEvent::Warning(format!(
                                                        "the table is not on the network yet: {e}"
                                                    )))
                                                    .await;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = events.send(NodeEvent::Warning(
                                            format!("the table does not form: {e:?}"))).await;
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = events.send(NodeEvent::Warning(
                                    format!("the advert does not publish: {e}"))).await;
                            }
                        }
                    }

                    NodeCommand::JoinTable { key, buyin, seat, password } => {
                        let Some(held) = state.lobby.get(&key).cloned() else {
                            let _ = events.send(NodeEvent::Warning(
                                "that table is no longer advertised".into())).await;
                            continue;
                        };
                        // The founder's PeerId comes from the advert, which was
                        // signed by the table key. Dialling anything else would
                        // be taking routing advice from whoever spoke last.
                        let founder = match PeerId::from_bytes(&held.ad.founder_peer_id) {
                            Ok(p) => p,
                            Err(_) => {
                                let _ = events.send(NodeEvent::Warning(
                                    "that table advertises a peer id this client cannot read"
                                        .into())).await;
                                continue;
                            }
                        };
                        let nonce = match crate::security::rng::secret_32() {
                            Ok(n) => n,
                            Err(e) => {
                                let _ = events.send(NodeEvent::Warning(
                                    format!("no randomness for a join nonce: {e}"))).await;
                                continue;
                            }
                        };
                        match Formation::join(
                            app_key.clone(),
                            held.ad.clone(),
                            held.advert_hash,
                            key,
                            my_peer_bytes.clone(),
                            nickname.clone(),
                            buyin,
                            seat,
                            password.as_deref(),
                            nonce,
                            now,
                        ) {
                            Ok((f, request)) => {
                                let topic = joinrpc::table_topic(&key);
                                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic);
                                table_topic = Some(topic);
                                table = Some(f);
                                swarm.behaviour_mut().join.send_request(&founder, request);
                            }
                            Err(e) => {
                                let _ = events.send(NodeEvent::LeftTable {
                                    why: format!("cannot ask to join: {e:?}"),
                                }).await;
                            }
                        }
                    }

                    NodeCommand::SetNickname(name) => {
                        // Bounded here as well as where it is chosen. §4.3's
                        // limit is on what a roster will take, and the roster is
                        // built here — a name that arrived by any other path
                        // must not be able to make a seat entry this client's
                        // own rules would refuse.
                        nickname = trim_to_bytes(&name, DISPLAY_NAME_MAX);
                        if nickname.trim().is_empty() {
                            nickname = crate::storage::profile::short_name(
                                &app_key.verifying_key().to_bytes(),
                            );
                        }
                    }

                    NodeCommand::LeaveTable => {
                        if let Some(t) = table_topic.take() {
                            let _ = swarm.behaviour_mut().gossipsub.unsubscribe(&t);
                        }
                        table = None;
                        let _ = events.send(NodeEvent::LeftTable {
                            why: "left the table".into(),
                        }).await;
                    }
                }
            }

            _ = housekeeping.tick() => {
                let now = super::node::now_unix_ms();
                state.tick(now);

                // Re-broadcast this node's own table.
                //
                // Through the `Formation`, which owns the table key and the
                // advertisement together. It used to be published from a
                // separate copy here, and the founder's own record of what it
                // had signed then went stale: a joiner naming the copy it heard
                // was refused with "the advertisement has expired", which is the
                // defect the first two-instance run found and which no unit test
                // could have, because it needs thirty seconds to appear.
                if let Some(f) = table.as_mut().filter(|f| f.is_founder()) {
                    match f.readvertise(now, AD_TTL_MS) {
                        Ok(bytes) => {
                            let n = bytes.len();
                            match swarm
                                .behaviour_mut()
                                .gossipsub
                                .publish(topics.lobby.clone(), bytes.clone())
                            {
                                Ok(_) => {
                                    let _ = events.send(NodeEvent::Published { bytes: n }).await;
                                    // And into this node's own lobby, so a
                                    // founder sees the table it is sitting at.
                                    show_own_table(
                                        &bytes,
                                        &my_app_key,
                                        now,
                                        &mut state.limits,
                                        &mut state.lobby,
                                        &events,
                                    )
                                    .await;
                                }
                                Err(e) => {
                                    // No mesh peer yet is the ordinary case at
                                    // start-up, not a fault.
                                    let _ = events
                                        .send(NodeEvent::Warning(format!("not published: {e}")))
                                        .await;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = events
                                .send(NodeEvent::Warning(format!("own advert: {e:?}")))
                                .await;
                        }
                    }
                }
            }
        }
    }
}

/// Tell the interface what table this is.
///
/// Sent whenever this client joins one or founds one, because the window that
/// draws it must not have to guess a seat count.
async fn report_params(events: &mpsc::Sender<NodeEvent>, f: &Formation) {
    let ad = f.advert();
    let _ = events
        .send(NodeEvent::TableParams {
            key: f.table_id(),
            name: ad.table_name.clone(),
            seats: ad.max_players,
            needed: ad.min_players_to_start,
            small_blind: ad.small_blind,
            big_blind: ad.big_blind,
        })
        .await;
}

/// Tell the interface who is seated.
///
/// The whole roster every time rather than a difference: a difference is only
/// correct if the receiver never missed one, and this channel is bounded and
/// drops under load by design.
async fn report_roster(events: &mpsc::Sender<NodeEvent>, f: &Formation) {
    let seats = f
        .roster()
        .seats()
        .iter()
        .map(|e| (e.seat, e.display_name.clone(), e.buyin))
        .collect();
    let _ = events
        .send(NodeEvent::Roster {
            key: f.table_id(),
            seats,
        })
        .await;
}

/// The `event_hash` of an advert this client just published.
///
/// Taken from the bytes rather than recomputed from the value, because it is the
/// bytes a joiner will name and the founder will look up.
fn advert_hash_of(bytes: &[u8]) -> [u8; 32] {
    use crate::protocol::messages::SignedEvent;
    match crate::protocol::serialization::from_canonical::<SignedEvent>(bytes, LOBBY_MSG_MAX) {
        Ok(signed) => crate::protocol::transcript::event_hash(&signed.body),
        // Unreachable for bytes this client produced a moment ago; a zero hash
        // simply makes every join request name an advert nobody signed, which
        // fails closed.
        Err(_) => [0u8; 32],
    }
}

/// §7.2's bound on a table name, in **bytes**.
pub const TABLE_NAME_MAX: usize = 64;

/// Cut a string to at most `max` bytes, without cutting a character in half.
///
/// The obvious `chars().take(n)` counts the wrong thing and `&s[..n]` panics on
/// a boundary. This is the only correct shape and it is worth having once:
/// every bound the protocol states on a display field is a bound in bytes,
/// because that is what goes on the wire, while every bound a person has in mind
/// is in characters.
pub fn trim_to_bytes(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}


/// The advertisement for a table this client is founding.
///
/// `CUSTOM` and not a name of its own: §7.2 rule 3 admits exactly two preset
/// identifiers, and a name that asserts values nothing checks is how two clients
/// ship different tables under one identity.
#[allow(clippy::too_many_arguments)]
fn new_table(
    name: String,
    seats: u8,
    min_players: u8,
    buyin: u64,
    password_required: bool,
    founder_app_key: [u8; 32],
    founder_peer_id: Vec<u8>,
    now_ms: u64,
) -> TableAd {
    use super::lobby::{BlindSchedule, DECK_SUITE_V1};
    use crate::protocol::constants::hand_deadline_min_ms;

    let seats = seats.clamp(2, MAX_SEATS);
    let (action, grace, crypto, delay) = (20_000u32, 5_000u32, 30_000u32, 7_000u32);
    TableAd {
        game: 1,
        mode: 1,
        preset_id: "CUSTOM".into(),
        // Trimmed rather than refused: §7.2 caps it, and a table that will not
        // advertise because its name is long is a worse answer than one that
        // advertises under a shorter name.
        //
        // **By bytes, and the first version counted characters.** §7.2's bound
        // is 64 *bytes*; thirty-two characters of anything outside ASCII is up
        // to a hundred and twenty-eight of them, so a Czech or an emoji table
        // name passed this trim and was then refused by this client's own
        // admission rules — the table would have appeared in nobody's lobby,
        // including its founder's.
        table_name: trim_to_bytes(&name, TABLE_NAME_MAX),
        small_blind: 10,
        big_blind: 20,
        ante: 0,
        // The buy-in the founder chose is inside the range it advertises, which
        // is what `SeatEntry::admissible` checks its own seat against.
        min_buyin: buyin.min(200),
        max_buyin: buyin.max(2_000),
        start_stack: 0,
        players: 1,
        max_players: seats,
        min_players_to_start: min_players.clamp(2, seats),
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
        ) as u32,
        join_deadline_ms: 120_000,
        hand_delay_ms: delay,
        button_rule: 1,
        odd_chip_rule: 1,
        showdown_policy: 1,
        password_required,
        deck_suite: DECK_SUITE_V1.into(),
        founder_app_key,
        founder_peer_id,
        timestamp_unix_ms: now_ms,
        expires_at_unix_ms: now_ms + AD_TTL_MS,
    }
}

/// Show this node its **own** table, once the network has actually taken it.
///
/// GossipSub does not deliver a message back to whoever published it, so the
/// founder's own advertisement never reached the founder's own lobby: a player
/// created a table and it appeared in everybody's list except theirs.
///
/// It is fed through `advert::receive` — the same path a stranger's advert takes,
/// with the same rate limiter and the same §7.2 admission — rather than inserted.
/// That is deliberate and it buys a real check: **this client's table appears in
/// this client's lobby only if this client's own advertisement would have been
/// accepted by anybody else.** An advert that its own author refuses is a table
/// nobody will ever see, and this is the cheapest possible place to find that out.
///
/// Called only after `publish` returned `Ok`, which is what the request was: a
/// table shows up once it is on the network, not once it is signed. `publish`
/// fails with `NoPeersSubscribedToTopic` when nothing heard it, and a table that
/// nothing heard is not in a lobby anywhere.
async fn show_own_table(
    bytes: &[u8],
    me: &[u8; 32],
    now: u64,
    limits: &mut RateLimiter,
    store: &mut LobbyStore,
    events: &mpsc::Sender<NodeEvent>,
) {
    match advert::receive(bytes, *me, now, limits, store) {
        Ok(key) => {
            if let Some(held) = store.get(&key) {
                let _ = events
                    .send(NodeEvent::TableSeen {
                        key,
                        ad: Box::new(held.ad.clone()),
                        params_hash: held.params_hash,
                        advert_hash: held.advert_hash,
                    })
                    .await;
            }
        }
        // `NotNewer` is the ordinary case on a re-broadcast this node has
        // already filed, and is not worth a line in anybody's log.
        Err(super::advert::NotAccepted::NotTaken(super::lobby::NotTaken::NotNewer)) => {}
        Err(e) => {
            // This client's own advert, refused by this client's own rules. It
            // is worth saying loudly, because every other client will refuse it
            // too and the table will simply never appear anywhere.
            let _ = events
                .send(NodeEvent::Warning(format!(
                    "this client refuses its own advertisement: {e:?}"
                )))
                .await;
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
            // The record the interface needs to draw a row and to build a
            // join request, taken from this node's own store rather than
            // re-derived — one place decides what was accepted.
            if let Some(held) = state.lobby.get(&key) {
                let _ = events
                    .send(NodeEvent::TableSeen {
                        key,
                        ad: Box::new(held.ad.clone()),
                        params_hash: held.params_hash,
                        advert_hash: held.advert_hash,
                    })
                    .await;
            }
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

    /// A table name is bounded in **bytes**, because that is what goes on the
    /// wire, and the first version counted characters.
    ///
    /// Thirty-two characters of Czech is up to sixty-four bytes and of emoji up
    /// to a hundred and twenty-eight, so a name a person would call short passed
    /// the trim and was then refused by this client's own §7.2 admission — the
    /// table appeared in nobody's lobby, its founder's included, and the only
    /// symptom was a warning about this client's own advertisement.
    #[test]
    fn a_name_is_trimmed_by_bytes_and_stays_text() {
        let cards = "\u{1F0A1}".repeat(40);
        let rs = "\u{159}".repeat(60);
        let xs = "x".repeat(200);
        for source in [
            "Riverside",
            "A table whose name is long enough that it will not fit in the field",
            cards.as_str(),
            rs.as_str(),
            xs.as_str(),
        ] {
            let cut = trim_to_bytes(source, TABLE_NAME_MAX);
            assert!(
                cut.len() <= TABLE_NAME_MAX,
                "{source:?} trimmed to {} bytes",
                cut.len()
            );
            assert!(source.starts_with(&cut), "the trim changed the text");
            // Still text. `is_char_boundary` is the whole point: a byte-wise cut
            // would have produced something that is not.
            assert!(std::str::from_utf8(cut.as_bytes()).is_ok());
        }
        assert_eq!(trim_to_bytes("Riverside", TABLE_NAME_MAX), "Riverside");
    }

    /// And the table this client founds passes this client's **own** admission
    /// rules, whatever name it is given. An advertisement its own author refuses
    /// is a table nobody will ever see.
    #[test]
    fn a_table_this_client_founds_is_one_it_would_accept() {
        const NOW: u64 = 1_700_000_000_000;
        let cards = "\u{1F0A1}".repeat(40);
        for name in [
            "Riverside",
            "A table whose name is very much longer than the sixty-four bytes allowed",
            cards.as_str(),
        ] {
            let ad = new_table(
                name.to_string(),
                6,
                2,
                1_000,
                false,
                [1u8; 32],
                vec![2u8; 38],
                NOW,
            );
            assert_eq!(
                super::super::lobby::admit(&ad, NOW),
                Ok(()),
                "this client would refuse its own table called {name:?}"
            );
        }
    }

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
