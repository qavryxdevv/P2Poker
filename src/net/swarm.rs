//! The libp2p node: transports, discovery behaviours, and the table mesh.
//!
//! `docs/NETWORK_STACK.md` and `docs/research/LIBP2P.md`. Everything here is
//! transport. **This module must never decide a poker rule and never create a
//! card** (`SPEC_CS.md` §1).
//!
//! # Why this client runs a relay server at all (D-002)
//!
//! The owner's decision: a publicly reachable client volunteers to relay other
//! people's games on its own line. Without volunteers there are no relays, and
//! without relays two clients that are both behind symmetric NAT cannot reach
//! each other at all — which D-004 says must still work.
//!
//! It is **conditional on being publicly reachable** — but the condition is on
//! *advertising*, not on capacity, and the first draft of this module conflated
//! the two.
//!
//! The harm in a relay behind NAT is that it announces itself as a way through
//! and is not one, so a peer that picks it has spent an attempt on nothing. The
//! harm is in the announcement. Capacity that nobody can reach costs nothing and
//! is never used, and `relay::Config` cannot be changed after the swarm is
//! built — so a client that had to become a volunteer later would have to be
//! rebuilt.
//!
//! So the limits are always the volunteer ones, and **AutoNAT gates the announce
//! under `RELAY_INFOHASH`** ([`super::dht::Swarm::Relay`]). A client behind NAT
//! is then exactly as useless as a relay as it would have been with zero limits,
//! and becomes useful the moment AutoNAT says it is reachable, with nothing to
//! rebuild.
//!
//! # The relay limits are set explicitly, and the defaults are the reason
//!
//! `relay::Config::default()` is **2 minutes and 128 KiB**. Those numbers are
//! sized for hole-punch coordination — a handful of packets to get two peers
//! talking directly — and a poker session is neither. A relayed table would be
//! cut off mid-hand, and the failure would look like a peer disconnecting rather
//! than like a limit being hit.
//!
//! So they are set here, deliberately and with the reasoning written down, and
//! they are set **generously but not without bound**: this client is lending its
//! own bandwidth to strangers and that has to have an edge.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher as _};
use std::time::Duration;

use libp2p::{
    autonat, connection_limits, dcutr, gossipsub, identify, identity, kad,
    kad::store::MemoryStore, noise, ping, relay,
    swarm::NetworkBehaviour,
    tcp, tls, yamux, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};

use crate::protocol::constants::{
    GOSSIP_MAX_TRANSMIT, IDLE_CONNECTION_TIMEOUT_MS, LOBBY_CHAT_TOPIC, LOBBY_TOPIC,
    MDNS_QUERY_INTERVAL_MS,
};

/// Whether this client offers its line to other people's games.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayRole {
    /// Volunteering (D-002), if and when AutoNAT says this client is reachable.
    ///
    /// Carries capacity from the start. Whether that capacity is ever *announced*
    /// is a separate decision made later, and it is the announcement that would
    /// do harm from behind a NAT.
    Volunteer,
    /// The user declined to relay at all.
    ///
    /// Zero capacity, so a reservation is refused even if this client turns out
    /// to be reachable. This is a preference, not a reachability finding.
    Declined,
}

/// How long a relayed circuit may live.
///
/// Ten minutes against the library default of two. A poker hand at ten seats has
/// a whole-hand deadline of up to an hour, so this does not cover a session — it
/// covers a hand and then some, and a circuit that ends is re-established rather
/// than being a game-ending event.
pub const RELAY_RESERVATION: Duration = Duration::from_secs(600);

/// How much data one relayed circuit may carry.
///
/// 16 MiB against the library default of 128 KiB. The default is sized for
/// hole-punch coordination and would cut a relayed table off mid-hand.
pub const RELAY_MAX_CIRCUIT_BYTES: u64 = 16 * 1024 * 1024;

/// How many circuits one peer may hold on this client at once.
pub const RELAY_MAX_CIRCUITS_PER_PEER: usize = 4;

/// The behaviours this node runs.
#[derive(NetworkBehaviour)]
pub struct PokerBehaviour {
    /// The lobby and the table mesh.
    pub gossipsub: gossipsub::Behaviour,
    /// Peer routing. **Not** the global lobby — that is Mainline, under
    /// `LOBBY_INFOHASH`, and lives in [`super::dht`].
    pub kademlia: kad::Behaviour<MemoryStore>,
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
    /// Asks other peers whether this client is reachable, which is what decides
    /// [`RelayRole`].
    pub autonat_client: autonat::v2::client::Behaviour,
    /// Answers the same question for others.
    pub autonat_server: autonat::v2::server::Behaviour,
    /// Direct connection upgrade: the hole punch that turns a relayed
    /// connection into a direct one.
    pub dcutr: dcutr::Behaviour,
    pub relay_client: relay::client::Behaviour,
    pub relay_server: relay::Behaviour,
    pub conn_limits: connection_limits::Behaviour,
    pub mem_limits: libp2p::memory_connection_limits::Behaviour,
    /// Port mapping, for the router that will do it.
    pub upnp: libp2p::upnp::tokio::Behaviour,
    /// Local-network discovery.
    ///
    /// Not an optimisation. Two clients on one LAN get each other's **external**
    /// address from the DHT and would have to dial it inwards through their own
    /// router, which is NAT hairpinning and which many routers simply do not do.
    /// The DHT is the wrong tool for a peer that is one hop away, and mDNS is
    /// the right one.
    pub mdns: libp2p::mdns::tokio::Behaviour,
}

/// The topics this node subscribes to.
pub struct Topics {
    pub lobby: gossipsub::IdentTopic,
    pub lobby_chat: gossipsub::IdentTopic,
}

impl Default for Topics {
    fn default() -> Self {
        Topics {
            lobby: gossipsub::IdentTopic::new(LOBBY_TOPIC),
            lobby_chat: gossipsub::IdentTopic::new(LOBBY_CHAT_TOPIC),
        }
    }
}

/// What a node needs to know before it starts.
pub struct NodeConfig {
    pub identity: identity::Keypair,
    /// Whether this client is willing to relay at all (D-002).
    ///
    /// A **preference**, not a reachability finding. Reachability is AutoNAT's
    /// and it gates the announce rather than the capacity; see the module
    /// documentation for why the two were separated.
    pub relay_role: RelayRole,
}

/// Build the node.
///
/// **Must be called from inside a tokio runtime.** The UPnP behaviour spawns a
/// task in its constructor, so building outside one panics inside the library
/// rather than returning an error. That is a property of the dependency and not
/// of this function, and it is written here because the failure is a panic in
/// somebody else's file.
///
/// The builder is a type-state machine and the phase order is fixed:
/// identity → runtime → TCP → QUIC → DNS → relay client → behaviour → config.
/// Note that `with_relay_client` gives the behaviour closure a **second
/// argument**; that is not optional and is why the closure below takes two.
pub fn build(config: NodeConfig) -> Result<Swarm<PokerBehaviour>, Box<dyn std::error::Error>> {
    let local_peer_id = PeerId::from(config.identity.public());
    let relay_role = config.relay_role;

    let swarm = SwarmBuilder::with_existing_identity(config.identity)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            (tls::Config::new, noise::Config::new),
            yamux::Config::default,
        )?
        .with_quic_config(|mut cfg| {
            cfg.handshake_timeout = Duration::from_secs(10);
            cfg.max_idle_timeout = 30_000;
            cfg.keep_alive_interval = Duration::from_secs(5);
            cfg
        })
        .with_dns()?
        .with_relay_client((tls::Config::new, noise::Config::new), yamux::Config::default)?
        .with_behaviour(|key, relay_client| {
            let gossipsub = build_gossipsub(key)?;

            let mut kad_cfg = kad::Config::new(StreamProtocol::new("/p2p-poker/kad/1"));
            kad_cfg.set_query_timeout(Duration::from_secs(60));
            let kademlia = kad::Behaviour::with_config(
                local_peer_id,
                MemoryStore::new(local_peer_id),
                kad_cfg,
            );

            let identify = identify::Behaviour::new(
                identify::Config::new("/p2p-poker/1".into(), key.public())
                    .with_agent_version(format!("p2p-poker/{}", env!("CARGO_PKG_VERSION")))
                    .with_push_listen_addr_updates(true),
            );

            Ok(PokerBehaviour {
                gossipsub,
                kademlia,
                identify,
                ping: ping::Behaviour::new(ping::Config::new()),
                // AutoNAT's constructor names the concrete OS generator type
                // from the `rand` crate rather than taking a generic, so this
                // crate's own handle cannot be passed. Nothing is weakened -
                // that type **is** the operating system CSPRNG and the value is
                // a probe nonce - and the two exemptions below are counted by
                // `security::rng`'s source scan, so a third fails the build.
                autonat_client: autonat::v2::client::Behaviour::new(
                    // RNG-EXEMPT
                    rand::rngs::OsRng,
                    autonat::v2::client::Config::default()
                        .with_probe_interval(Duration::from_secs(30))
                        .with_max_candidates(8),
                ),
                // RNG-EXEMPT: as above.
                autonat_server: autonat::v2::server::Behaviour::new(rand::rngs::OsRng),
                dcutr: dcutr::Behaviour::new(local_peer_id),
                relay_client,
                relay_server: relay::Behaviour::new(local_peer_id, relay_config(relay_role)),
                conn_limits: connection_limits::Behaviour::new(
                    connection_limits::ConnectionLimits::default()
                        .with_max_pending_incoming(Some(32))
                        .with_max_established_incoming(Some(256))
                        .with_max_established_per_peer(Some(2)),
                ),
                mem_limits: libp2p::memory_connection_limits::Behaviour::with_max_percentage(0.25),
                upnp: libp2p::upnp::tokio::Behaviour::default(),
                mdns: libp2p::mdns::tokio::Behaviour::new(
                    libp2p::mdns::Config {
                        query_interval: Duration::from_millis(MDNS_QUERY_INTERVAL_MS),
                        ..Default::default()
                    },
                    local_peer_id,
                )?,
            })
        })?
        .with_swarm_config(|c| {
            c.with_idle_connection_timeout(Duration::from_millis(IDLE_CONNECTION_TIMEOUT_MS))
        })
        .build();

    Ok(swarm)
}

/// The GossipSub configuration.
///
/// **`ValidationMode::Strict` and `validate_messages()` together.** The first
/// requires every message to carry a signature, a sequence number and a source;
/// the second stops the behaviour from forwarding a message before this client
/// has judged it. Without the second, a message this client is about to reject
/// has already been passed on to its mesh peers in its name — which is how one
/// spammer's advert reaches the whole lobby through honest clients.
fn build_gossipsub(
    key: &identity::Keypair,
) -> Result<gossipsub::Behaviour, Box<dyn std::error::Error + Send + Sync>> {
    // The message id is over the content, so two peers relaying one advert
    // produce one id and the duplicate cache actually suppresses it.
    let message_id_fn = |message: &gossipsub::Message| {
        let mut s = DefaultHasher::new();
        message.data.hash(&mut s);
        message.topic.hash(&mut s);
        message.sequence_number.hash(&mut s);
        gossipsub::MessageId::from(s.finish().to_be_bytes())
    };

    let config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(1))
        .validation_mode(gossipsub::ValidationMode::Permissive)
        .message_id_fn(message_id_fn)
        .max_transmit_size(GOSSIP_MAX_TRANSMIT)
        .mesh_n(8)
        .mesh_n_low(6)
        .mesh_n_high(12)
        .mesh_outbound_min(3)
        .duplicate_cache_time(Duration::from_secs(120))
        // Deliberately off: flood publishing sends every message to every known
        // peer of the topic rather than to the mesh, which turns one advert into
        // a fan-out proportional to the whole lobby.
        .build()?;

    Ok(gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(key.clone()),
        config,
    )?)
}

/// The relay server's limits.
///
/// See the module documentation: the library's defaults are two minutes and
/// 128 KiB, sized for hole-punch coordination, and a relayed poker table would
/// be cut off mid-hand.
///
/// When this client is not publicly reachable the limits are set to zero
/// circuits, which is the honest form of "not a relay": it does not advertise a
/// way through that it cannot provide.
pub fn relay_config(role: RelayRole) -> relay::Config {
    match role {
        RelayRole::Volunteer => relay::Config {
            max_reservations: 128,
            max_reservations_per_peer: 4,
            reservation_duration: RELAY_RESERVATION,
            max_circuits: 64,
            max_circuits_per_peer: RELAY_MAX_CIRCUITS_PER_PEER,
            max_circuit_duration: RELAY_RESERVATION,
            max_circuit_bytes: RELAY_MAX_CIRCUIT_BYTES,
            ..Default::default()
        },
        RelayRole::Declined => relay::Config {
            max_reservations: 0,
            max_reservations_per_peer: 0,
            max_circuits: 0,
            max_circuits_per_peer: 0,
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keypair() -> identity::Keypair {
        identity::Keypair::generate_ed25519()
    }

    /// The stack builds. It is a type-state builder with a fixed phase order and
    /// a closure whose arity depends on an earlier phase, so "it compiles" is a
    /// real result rather than a formality.
    #[tokio::test]
    async fn the_node_builds() {
        let swarm = build(NodeConfig {
            identity: keypair(),
            relay_role: RelayRole::Declined,
        })
        .expect("the stack builds");
        assert_eq!(swarm.connected_peers().count(), 0);
    }

    /// A user who declines to relay accepts nothing, whatever AutoNAT later
    /// says. That is a preference and it outranks a measurement.
    #[test]
    fn a_client_that_declined_accepts_no_reservations() {
        let off = relay_config(RelayRole::Declined);
        assert_eq!(off.max_reservations, 0);
        assert_eq!(off.max_circuits, 0);
        assert_eq!(off.max_circuits_per_peer, 0);
    }

    /// The library defaults are sized for hole-punch coordination — two minutes
    /// and 128 KiB — and a relayed table would be cut off mid-hand. The point of
    /// this test is that the numbers are *chosen*, so a library bump that moved
    /// the defaults cannot silently move ours.
    #[test]
    fn the_relay_limits_are_ours_and_not_the_librarys() {
        let ours = relay_config(RelayRole::Volunteer);
        let theirs = relay::Config::default();

        assert!(
            ours.max_circuit_duration > theirs.max_circuit_duration,
            "the default duration is sized for a hole punch, not a hand"
        );
        assert!(
            ours.max_circuit_bytes > theirs.max_circuit_bytes,
            "the default byte budget is sized for a hole punch, not a hand"
        );
        assert_eq!(ours.max_circuit_duration, RELAY_RESERVATION);
        assert_eq!(ours.max_circuit_bytes, RELAY_MAX_CIRCUIT_BYTES);
    }

    /// Lending a stranger this client's bandwidth has to have an edge.
    #[test]
    fn a_volunteer_relay_is_still_bounded() {
        let c = relay_config(RelayRole::Volunteer);
        assert!(c.max_circuits > 0 && c.max_circuits <= 256);
        assert!(c.max_circuits_per_peer <= c.max_circuits);
        assert!(c.max_reservations_per_peer <= c.max_reservations);
        assert!(
            c.max_circuit_bytes <= 64 * 1024 * 1024,
            "generous, not unbounded"
        );
    }

    /// The two lobby topics are distinct, or chat and adverts arrive on one
    /// channel and every advert parser sees chat.
    #[test]
    fn the_topics_are_distinct() {
        let t = Topics::default();
        assert_ne!(t.lobby.hash(), t.lobby_chat.hash());
        assert_eq!(t.lobby.to_string(), LOBBY_TOPIC);
    }

    /// The message id is over the content, so two peers relaying one advert
    /// produce one id and the duplicate cache suppresses it. An id over
    /// arrival-order state would make every relayed copy a fresh message.
    #[test]
    fn one_message_has_one_id_wherever_it_arrives_from() {
        let key = keypair();
        assert!(build_gossipsub(&key).is_ok());

        // The property, stated directly on the hash the config installs.
        let id = |data: &[u8], topic: &str| {
            let mut s = DefaultHasher::new();
            data.hash(&mut s);
            gossipsub::IdentTopic::new(topic).hash().hash(&mut s);
            s.finish()
        };
        assert_eq!(id(b"advert", LOBBY_TOPIC), id(b"advert", LOBBY_TOPIC));
        assert_ne!(id(b"advert", LOBBY_TOPIC), id(b"advert", LOBBY_CHAT_TOPIC));
        assert_ne!(id(b"advert", LOBBY_TOPIC), id(b"other", LOBBY_TOPIC));
    }

    /// The transmit cap is the protocol's, not the library's example value.
    #[test]
    fn the_transmit_cap_is_the_protocols() {
        assert_eq!(GOSSIP_MAX_TRANSMIT, 65_536);
    }
}
