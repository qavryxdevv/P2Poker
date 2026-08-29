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

    // A fresh identity per run for now. The persistent one belongs with the
    // profile directory, and giving it a temporary key is better than giving it
    // a key that looks persistent and is not.
    let identity = libp2p::identity::Keypair::generate_ed25519();
    let peer_id = libp2p::PeerId::from(identity.public());
    println!("peer id  {peer_id}");

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);

    tokio::spawn(async move {
        if let Err(e) = p2p_poker::net::run::run(identity, tx).await {
            eprintln!("node stopped: {e}");
        }
    });

    // Report for a while, then stop. A long-running client is the GUI's job.
    let deadline = tokio::time::sleep(Duration::from_secs(180));
    tokio::pin!(deadline);

    loop {
        tokio::select! {
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
            },
            _ = &mut deadline => {
                println!("done");
                break;
            }
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
