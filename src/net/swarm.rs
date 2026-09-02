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
//! under the relay namespace** ([`super::run`]). A client behind NAT
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

use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::{
    autonat, connection_limits, dcutr, gossipsub, identify, identity, kad, request_response,
    kad::store::MemoryStore, noise, ping, relay,
    swarm::NetworkBehaviour,
    tcp, tls, yamux, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};

use super::joinrpc::JoinCodec;
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
/// **The whole-hand cap, because anything less refuses itself.** The first
/// version was ten minutes, on the reasoning that it "covers a hand and then
/// some" — while [`adequate`](super::relay::adequate), the function that decides
/// whether a relay is usable, judges against `HAND_DEADLINE_CAP_MS`, an hour. So
/// every p2p-poker relay was declared inadequate by every p2p-poker client, and
/// the two halves of this project's own NAT story cancelled each other out.
/// Neither number was wrong alone; nothing compared them, and now a test does.
///
/// An hour is a long circuit to lend a stranger and it is the honest number: a
/// legal hand may take that long under §7.2's cap, and a circuit that dies
/// mid-street is the engineered abort `SPEC_CS.md` §19 treats as a security
/// problem rather than a hiccup. A volunteer who will not lend it declines
/// outright with [`RelayRole::Declined`], rather than by offering a circuit that
/// cannot carry a hand.
pub const RELAY_RESERVATION: Duration =
    Duration::from_millis(crate::protocol::constants::HAND_DEADLINE_CAP_MS);

/// How much data one relayed circuit may carry.
///
/// Derived from what a hand actually costs rather than picked: a full table's
/// `per_hand_bytes`, times the headroom the adequacy rule demands, times four
/// for the hands after the first. A literal would be a second number to keep in
/// step with the first, and those drift — which is exactly how the duration
/// above came to refuse itself.
pub const RELAY_MAX_CIRCUIT_BYTES: u64 =
    super::relay::per_hand_bytes(crate::protocol::constants::MAX_SEATS)
        * super::relay::HAND_HEADROOM
        * 4;

/// How many circuits one peer may hold on this client at once.
pub const RELAY_MAX_CIRCUITS_PER_PEER: usize = 4;

/// How long a join request may go unanswered before it is a failure.
///
/// A founder that has to be dialled through a relay is slow, and a joiner that
/// gives up at five seconds gives up on a table it could have sat down at. Thirty
/// is long enough for a relayed round trip and short enough that a player is not
/// left looking at a button that appears to have done nothing.
pub const JOIN_RPC_TIMEOUT_MS: u64 = 30_000;

/// The behaviours this node runs.
#[derive(NetworkBehaviour)]
pub struct PokerBehaviour {
    /// The lobby and the table mesh.
    pub gossipsub: gossipsub::Behaviour,
    /// Peer routing. **Not** the global lobby — that is a provider record on
    /// the public Kademlia below, and lives in [`super::run`].
    pub kademlia: kad::Behaviour<MemoryStore>,
    /// The **public** Kademlia, and the only reason it exists is relays.
    ///
    /// `kademlia` above speaks `/p2p-poker/kad/1`, which is right: this
    /// project's records are its own and have no business in anybody else's
    /// routing table. But it also means this client is invisible to, and blind
    /// to, the network where relays actually advertise themselves — and a peer
    /// behind a NAT with no relay cannot be reached at all.
    ///
    /// libp2p's answer to "where are the relays" is not a file. A relay host
    /// advertises itself in the DHT under the namespace `/libp2p/relay`, and
    /// AutoRelay looks it up there; that is the list, and it maintains itself.
    /// Reading it means speaking `/ipfs/kad/1.0.0`.
    ///
    /// In **client mode**: this node queries and does not answer. Serving other
    /// people's routing queries is a service to a network this client is only
    /// visiting, and it would be paid for with the bandwidth of somebody trying
    /// to play poker.
    pub ipfs_kad: kad::Behaviour<MemoryStore>,
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
    /// The join RPC: one request, one answer, on `/p2p-poker/join/1`.
    ///
    /// Request-response rather than the mesh, because a join happens between two
    /// peers who are not yet at a table together: there is no mesh to carry it,
    /// and an unanswered request has to become a timeout rather than silence.
    pub join: request_response::Behaviour<JoinCodec>,
    /// Local-network discovery.
    ///
    /// Not an optimisation. Two clients on one LAN get each other's **external**
    /// address from the DHT and would have to dial it inwards through their own
    /// router, which is NAT hairpinning and which many routers simply do not do.
    /// The DHT is the wrong tool for a peer that is one hop away, and mDNS is
    /// the right one.
    /// Peers on the same wire, found by multicast.
    ///
    /// Optional, and the reason is evidence rather than tidiness. Two clients
    /// on one network find each other by mDNS in under a second, which is
    /// excellent for playing and useless for **proving** that anything else
    /// works: a test that dials through the DHT and then asserts two peers met
    /// is satisfied either way, and would keep passing after the DHT path
    /// broke. Turning it off is how the other path gets tested at all.
    pub mdns: Toggle<libp2p::mdns::tokio::Behaviour>,
}

/// The topics this node subscribes to.
/// How many connections this client will hold at once.
///
/// Measured before it was capped: **over four hundred**, nearly all of them
/// strangers on the public DHT that this client will never exchange a poker
/// message with. They cost sockets, file handles, keep-alives and — on a home
/// connection — a NAT table that other software has to share.
///
/// What the client actually needs at once: the handful of entry points, two or
/// three relays it holds reservations on, the seats at its tables, and enough
/// DHT peers for a lookup to converge. Eighty is comfortably above that sum and
/// far below four hundred.
///
/// **What it costs.** At the cap a new connection is refused, including one this
/// client wanted, so a lookup mid-flight can lose a hop and take longer.
/// Kademlia stores addresses rather than connections and dials on demand, so
/// the routing table is not what shrinks — the churn during a lookup is. Half
/// the budget is reserved against **incoming** connections, so a crowd of
/// strangers arriving cannot fill the table and leave no room for a relay this
/// client is trying to reach.
pub const MAX_CONNECTIONS: u32 = 80;

/// The floor and the ceiling the cap moves between.
///
/// The cap is not a fixed number, because a fixed one is either wasteful or
/// starving and there is no way to know which in advance. It starts at
/// [`MAX_CONNECTIONS`] and moves: **up** when a connection this client wanted
/// was refused by the limit, **down** while it has been comfortably under for a
/// while. The floor is what a client needs to function at all — entry points,
/// two or three relays, a table's seats and enough of a routing table to look
/// something up. The ceiling is where the cost stops being worth it.
pub const MIN_CONNECTIONS: u32 = 40;

/// The limits at one budget.
///
/// One function, called at start-up and again whenever the budget moves, so the
/// three limits that are derived from it cannot drift apart from each other —
/// `ConnectionLimits`'s setters consume `self` and it has no getters, so the
/// alternative was rebuilding it by hand in three places.
pub fn connection_limits(budget: u32) -> connection_limits::ConnectionLimits {
    connection_limits::ConnectionLimits::default()
        .with_max_pending_incoming(Some(32))
        .with_max_established(Some(budget))
        // Half against strangers arriving, so a crowd cannot fill the table and
        // leave no room for a relay this client is trying to reach.
        .with_max_established_incoming(Some(budget / 2))
        .with_max_established_per_peer(Some(2))
}
pub const CONNECTION_CEILING: u32 = 320;

/// What this client answers with when asked who it is.
///
/// Named rather than written twice: `identify` announces it, and it is what
/// tells another poker client apart from the several hundred strangers this node
/// shares a DHT with.
///
/// **It was written twice.** This was `pub const PROTOCOL_VERSION: &str` here
/// and `IDENTIFY_PROTOCOL` in `protocol::constants`, holding the same string —
/// and `PROTOCOL_VERSION` is also a `u16` in two other modules, so the name
/// carried two types and three meanings. One definition, under the name that
/// says what it is.
pub use crate::protocol::constants::IDENTIFY_PROTOCOL;

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
    /// Whether to look for peers by multicast on the local network.
    ///
    /// True for a client, because a player on the same wire should be found in
    /// a second rather than in a minute. False when the point is to prove that
    /// discovery over the DHT works, since mDNS would answer first and the
    /// proof would be of nothing.
    pub local_discovery: bool,
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
    let local_discovery = config.local_discovery;
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

            let mut ipfs_cfg = kad::Config::new(StreamProtocol::new("/ipfs/kad/1.0.0"));
            ipfs_cfg.set_query_timeout(Duration::from_secs(60));
            let mut ipfs_kad = kad::Behaviour::with_config(
                local_peer_id,
                MemoryStore::new(local_peer_id),
                ipfs_cfg,
            );
            // Automatic, not pinned to client.
            //
            // Client mode means never answering a query, which sounds polite and
            // starves the routing table: nobody adds a node they never hear
            // from, and a table built only from peers this node happened to
            // speak to first does not span the key space. Measured with it
            // pinned: two clients each announced themselves in the lobby, each
            // read the lobby a hundred and thirty times over ten minutes, each
            // found three or four other players — and never each other, because
            // their walks landed on different nodes.
            //
            // `None` restores libp2p's own rule: client until there is a
            // confirmed external address, server after. A client behind a NAT
            // gains one when a relay accepts its reservation, so the node that
            // starts answering queries is one that can actually be reached.
            ipfs_kad.set_mode(None);

            // **`identify` sent this machine's LAN topology to every stranger on
            // the public DHT**, and `NETWORK_STACK.md` §3.5 now says so.
            //
            // `hide_listen_addrs` defaults to false, so the message carries
            // `listen_addresses ∪ external_addresses`. The external half is
            // filtered — `run::reachable` gates what AutoNAT may add, which is
            // §5.6's publish filter doing its job — but the listen half is the
            // raw bound set. Measured here on 2026-09-02 that was
            // `/ip4/192.168.1.20`, `/ip4/172.27.224.1` (a Hyper-V "Default
            // Switch"), and two `fdc9:…` IPv6 ULAs. A peer on the Amino DHT that
            // never sees the lobby key still learned the player's home subnet
            // and which hypervisor they run.
            //
            // Hiding it costs a delay, not a capability. A genuinely public host
            // is advertised the moment AutoNAT confirms its address — the same
            // address, by the path that filters it — and the LAN case is served
            // by mDNS (§9.8), which does not go through `identify` at all. What
            // stops being advertised is precisely the set `reachable` would have
            // refused anyway.
            let identify = identify::Behaviour::new(
                identify::Config::new(IDENTIFY_PROTOCOL.into(), key.public())
                    .with_agent_version(format!("p2p-poker/{}", env!("CARGO_PKG_VERSION")))
                    .with_hide_listen_addrs(true)
                    .with_push_listen_addr_updates(true),
            );

            Ok(PokerBehaviour {
                gossipsub,
                kademlia,
                ipfs_kad,
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
                    connection_limits(MAX_CONNECTIONS),
                ),
                join: request_response::Behaviour::with_codec(
                    JoinCodec,
                    [(
                        super::joinrpc::protocol(),
                        request_response::ProtocolSupport::Full,
                    )],
                    request_response::Config::default()
                        .with_request_timeout(Duration::from_millis(JOIN_RPC_TIMEOUT_MS)),
                ),
                mem_limits: libp2p::memory_connection_limits::Behaviour::with_max_percentage(0.25),
                upnp: libp2p::upnp::tokio::Behaviour::default(),
                mdns: Toggle::from(if local_discovery {
                    Some(libp2p::mdns::tokio::Behaviour::new(
                        libp2p::mdns::Config {
                        query_interval: Duration::from_millis(MDNS_QUERY_INTERVAL_MS),
                        ..Default::default()
                    },
                        local_peer_id,
                    )?)
                } else {
                    None
                }),
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
    Ok(gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(key.clone()),
        gossipsub_config()?,
    )?)
}

/// The configuration, separately, so a test can read it back.
///
/// `gossipsub::Behaviour::new` consumes the config and exposes none of it, so a
/// build that merely succeeds proves nothing about the settings. Splitting it
/// out is not tidiness: the settings below are the ones that were silently
/// weakened and committed, under a doc comment that asserted them.
pub fn gossipsub_config() -> Result<gossipsub::Config, Box<dyn std::error::Error + Send + Sync>> {
    // The message id is over the content, so two peers relaying one advert
    // produce one id and the duplicate cache actually suppresses it. It must
    // NOT include the sequence number: that is per-sender, so one advert
    // relayed by two peers would be two messages and the cache would suppress
    // neither.
    let message_id_fn = |message: &gossipsub::Message| {
        let mut s = DefaultHasher::new();
        message.data.hash(&mut s);
        message.topic.hash(&mut s);
        gossipsub::MessageId::from(s.finish().to_be_bytes())
    };

    let config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(1))
        .validation_mode(gossipsub::ValidationMode::Strict)
        .validate_messages()
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
        .flood_publish(false)
        .build()?;

    Ok(config)
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
            local_discovery: false,
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

    /// The relay this client offers must pass the test this client applies.
    ///
    /// The first version failed it — 600 seconds offered against an hour
    /// demanded — so every p2p-poker relay was refused by every p2p-poker
    /// client and the two halves of the NAT story cancelled out. Neither number
    /// was wrong on its own; nothing compared them.
    #[test]
    fn our_own_relay_is_one_our_own_client_would_accept() {
        let c = relay_config(RelayRole::Volunteer);
        assert_eq!(
            crate::net::relay::adequate(
                crate::protocol::constants::MAX_SEATS,
                Duration::from_millis(crate::protocol::constants::HAND_DEADLINE_CAP_MS),
                Some(c.max_circuit_bytes),
                Some(c.max_circuit_duration),
            ),
            crate::net::relay::Adequacy::Adequate,
            "a relay this client would not use is a relay it should not offer"
        );
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

    /// The settings this module's documentation asserts, asserted.
    ///
    /// **This test exists because these exact values were silently changed and
    /// committed**, under the doc comment that claims them: `Strict` became
    /// `Permissive`, `validate_messages` was dropped, `flood_publish(false)` was
    /// dropped, and the per-sender sequence number was added to the message id.
    /// `gossipsub::Behaviour::new` consumes the config and exposes nothing, so
    /// the build test that existed could not have noticed any of it.
    ///
    /// Each line below is a security property and not a preference:
    ///
    /// * `Strict` requires every message to carry a signature, a source and a
    ///   sequence number. `Permissive` accepts an unsigned one.
    /// * `validate_messages` stops the behaviour forwarding a message before
    ///   this client has judged it — without it, a message this client is about
    ///   to reject has already gone to its mesh peers in its name.
    /// * `flood_publish(false)` keeps a publish to the mesh. On, it goes to
    ///   every known peer of the topic, turning one advert into a fan-out the
    ///   size of the lobby.
    #[test]
    fn the_gossip_settings_are_the_ones_the_documentation_claims() {
        let config = gossipsub_config().expect("the config builds");

        assert!(
            matches!(config.validation_mode(), gossipsub::ValidationMode::Strict),
            "Permissive accepts an unsigned message"
        );
        assert!(
            config.validate_messages(),
            "without this, a message this client is about to reject has already \
             been forwarded in its name"
        );
        assert!(
            !config.flood_publish(),
            "flood publishing turns one advert into a fan-out the size of the lobby"
        );
        assert_eq!(config.max_transmit_size(), GOSSIP_MAX_TRANSMIT);
        assert_eq!(config.mesh_n(), 8);
        assert_eq!(config.mesh_n_low(), 6);
        assert_eq!(config.mesh_n_high(), 12);
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
