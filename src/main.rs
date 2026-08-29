//! Portable single-executable entry point (`docs/SPEC_CS.md` §22).
//!
//! The profile — identity, settings, history — lives next to the binary so the
//! whole directory can be copied to another machine. Nothing is written to the
//! registry or outside this folder.
//!
//! Running it with no arguments starts a node: it listens, announces under the
//! fixed lobby infohash, discovers peers and dials them. That is D-003's
//! acceptance criterion in its bare form — two copies of this binary, on two
//! networks, finding each other with no server between them.

use std::time::Duration;

use p2p_poker::net::node::NodeEvent;

#[tokio::main]
async fn main() {
    println!("p2p-poker {}", env!("CARGO_PKG_VERSION"));

    // The profile beside the executable, so the peer id is the same client
    // tomorrow. A client that changes identity on restart cannot be reconnected
    // to and appears to the network as an endless stream of strangers.
    let dir = p2p_poker::storage::profile::profile_dir();
    let identity = match p2p_poker::storage::profile::load_or_create_identity(&dir) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("profile at {}: {e}", dir.display());
            return;
        }
    };
    let peer_id = libp2p::PeerId::from(identity.public());
    println!("peer id  {peer_id}");

    // `--host <name>` offers a table; with no argument the node only watches.
    let hosted = std::env::args()
        .nth(1)
        .filter(|a| a == "--host")
        .map(|_| {
            let name = std::env::args().nth(2).unwrap_or_else(|| "Table".into());
            // Seeded from this crate's one randomness source rather than from
            // an ecosystem generator: `ed25519-dalek` 3.0 wants `rand_core`
            // 0.10's trait and our handle speaks `rand` 0.8's, and threading a
            // second generator in to bridge that would be the exact thing
            // `security::rng` exists to prevent.
            let seed = p2p_poker::security::rng::secret_32()
                .expect("the operating system CSPRNG is available");
            let key = ed25519_dalek::SigningKey::from_bytes(&seed);
            println!("hosting  {name}");
            p2p_poker::net::run::Hosted { ad: demo_table(name), key }
        });

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);

    tokio::spawn(async move {
        if let Err(e) = p2p_poker::net::run::run(identity, tx, hosted).await {
            eprintln!("node stopped: {e}");
        }
    });

    // Runs until Ctrl-C. `--for <seconds>` bounds it, which is what a scripted
    // two-machine test wants; a fixed 180 seconds was neither.
    let bounded = std::env::args()
        .position(|a| a == "--for")
        .and_then(|i| std::env::args().nth(i + 1))
        .and_then(|v| v.parse::<u64>().ok());
    if let Some(secs) = bounded {
        println!("running  {secs} s");
    } else {
        println!("running  until Ctrl-C");
    }

    let deadline = async {
        match bounded {
            Some(secs) => tokio::time::sleep(Duration::from_secs(secs)).await,
            None => std::future::pending::<()>().await,
        }
    };
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("done");
                break;
            }
            Some(event) = rx.recv() => match event {
                NodeEvent::Listening(addr) => println!("listen   {addr}"),
                NodeEvent::PeerConnected(p) => println!("connect  {p}"),
                NodeEvent::PeerDisconnected(p) => println!("drop     {p}"),
                NodeEvent::Discovered { hints, dropped } => {
                    println!("discover {hints} usable, {dropped} dropped")
                }
                NodeEvent::TableSeen { key } => {
                    println!("table    {}", hex(&key))
                }
                NodeEvent::TableRefused { reason } => println!("refused  {reason}"),
                NodeEvent::Reachability { public } => {
                    println!("nat      {}", if public { "public" } else { "behind NAT" })
                }
                NodeEvent::Announced { port } => println!("announce udp/{port} in the lobby swarm"),
                NodeEvent::Warning(w) => println!("warn     {w}"),
                NodeEvent::DialFailed { reason } => println!("dial     failed: {reason}"),
                NodeEvent::LocalPeer(p) => println!("lan      {p}"),
                NodeEvent::MeshPeer(p) => println!("mesh     {p}"),
                NodeEvent::Published { bytes } => println!("publish  {bytes} bytes"),
                NodeEvent::Reserved { relay, bytes, seconds, adequate } => println!(
                    "relay    {relay} limit {} bytes / {} s -> {}",
                    bytes.map(|b| b.to_string()).unwrap_or_else(|| "none".into()),
                    seconds.map(|s| s.to_string()).unwrap_or_else(|| "none".into()),
                    if adequate { "usable for a table" } else { "TOO SMALL for a hand" }
                ),
                NodeEvent::NoRelayFound { cycles } => println!(
                    "relay    none found after {cycles} searches - if nobody anywhere is                      publicly reachable there is no game"
                ),
                NodeEvent::HolePunched(p) => println!("punch    {p} is now direct"),
                NodeEvent::StillRelayed(p) => println!("punch    {p} stays relayed"),
            },
            _ = &mut deadline => {
                println!("done");
                break;
            }
        }
    }
}

/// A `CUSTOM` six-seat table, with a deadline derived from its own shape.
///
/// `CUSTOM` and not a name of its own: §7.2 rule 3 admits exactly two preset
/// identifiers, and a name that asserts values nothing checks is how two clients
/// ship different tables under one identity.
fn demo_table(name: String) -> p2p_poker::net::lobby::TableAd {
    use p2p_poker::net::lobby::{BlindSchedule, TableAd, DECK_SUITE_V1};
    use p2p_poker::protocol::constants::hand_deadline_min_ms;

    let (action, grace, crypto, delay) = (20_000u32, 5_000u32, 30_000u32, 7_000u32);
    let seats = 6u8;
    TableAd {
        game: 1,
        mode: 1,
        preset_id: "CUSTOM".into(),
        table_name: name,
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
        ) as u32,
        join_deadline_ms: 120_000,
        hand_delay_ms: delay,
        button_rule: 1,
        odd_chip_rule: 1,
        showdown_policy: 1,
        password_required: false,
        deck_suite: DECK_SUITE_V1.into(),
        founder_app_key: [0u8; 32],
        founder_peer_id: Vec::new(),
        timestamp_unix_ms: 0,
        expires_at_unix_ms: 0,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
