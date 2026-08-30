//! The routing table, kept between runs.
//!
//! A Kademlia node with an empty table has to be told where to start, and the
//! only thing compiled into this client is one DNS name. That is enough and it
//! is a dependency: every start goes through machines somebody else operates,
//! and a client that has been running for an hour knows hundreds of peers it
//! then throws away.
//!
//! So the peers this node actually talked to are written down beside the
//! profile and read back on the next start. The compiled entry point stays,
//! because a saved book goes stale and a first run has none — but a client that
//! has run before does not need it.
//!
//! **What is written is public.** A peer id and its listen addresses are what
//! every node on the DHT hands out on request; nothing here is this player's.
//! The file is still inside the profile directory, which is the one place this
//! client writes.

use std::path::{Path, PathBuf};

use libp2p::{Multiaddr, PeerId};

/// How many peers are worth keeping.
///
/// A Kademlia table holds twenty per bucket over 256 buckets and most of them
/// are empty; two hundred entries is more than enough to seed a walk in every
/// direction, and it keeps the file small enough to write on a timer without
/// thinking about it.
pub const MAX_PEERS: usize = 200;

/// How many addresses to keep per peer.
///
/// A public IPFS node advertises nine or more — v4 and v6, TCP, QUIC,
/// WebTransport, WebRTC — and this client can dial two of those forms. Keeping
/// all of them would triple the file for addresses that will never be tried.
pub const MAX_ADDRS_PER_PEER: usize = 4;

/// Where the book lives, inside the profile directory.
pub fn path(profile_dir: &Path) -> PathBuf {
    profile_dir.join("peers.txt")
}

/// Read the book. A missing or damaged file is an empty book, never an error:
/// this is a cache, and a client that refused to start because its cache was
/// corrupt would be worse than one that starts slowly.
pub fn load(profile_dir: &Path) -> Vec<(PeerId, Vec<Multiaddr>)> {
    let Ok(text) = std::fs::read_to_string(path(profile_dir)) else {
        return Vec::new();
    };
    let mut out: Vec<(PeerId, Vec<Multiaddr>)> = Vec::new();
    for line in text.lines().take(MAX_PEERS) {
        let mut parts = line.split_whitespace();
        let Some(Ok(peer)) = parts.next().map(str::parse::<PeerId>) else {
            continue;
        };
        let addrs: Vec<Multiaddr> = parts
            .filter_map(|a| a.parse().ok())
            .take(MAX_ADDRS_PER_PEER)
            .collect();
        if !addrs.is_empty() {
            out.push((peer, addrs));
        }
    }
    out
}

/// Write the book, atomically.
///
/// Temp file, flush, rename — the same rule the profile keys and the settings
/// follow. A half-written cache read on the next start would be a file of
/// garbage lines, and while `load` survives that, writing one is avoidable.
pub fn save(profile_dir: &Path, peers: &[(PeerId, Vec<Multiaddr>)]) -> std::io::Result<()> {
    use std::io::Write as _;

    let mut text = String::new();
    for (peer, addrs) in peers.iter().take(MAX_PEERS) {
        text.push_str(&peer.to_string());
        for a in addrs.iter().take(MAX_ADDRS_PER_PEER) {
            text.push(' ');
            text.push_str(&a.to_string());
        }
        text.push('\n');
    }

    let final_path = path(profile_dir);
    let temp = final_path.with_extension("txt.new");
    {
        let mut f = std::fs::File::create(&temp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&temp, &final_path)
}

/// Whether an address is worth writing down.
///
/// A loopback or private address is this machine's own network and means
/// nothing on the next start — least of all on a different one. A circuit
/// address belongs to a reservation that has already expired by then.
pub fn worth_keeping(addr: &Multiaddr) -> bool {
    use libp2p::multiaddr::Protocol;
    let mut routable = false;
    for p in addr.iter() {
        match p {
            Protocol::P2pCircuit => return false,
            // Documentation ranges are deliberately not excluded. They are
            // not routable either, but this is a cache: an address that does
            // not answer is dialled once and forgotten, which costs nothing,
            // while the exclusion cost this project's own test addresses -
            // every one of them is from RFC 5737, because that is what the
            // privacy sweep replaced real ones with.
            Protocol::Ip4(ip) => {
                routable = !ip.is_loopback()
                    && !ip.is_private()
                    && !ip.is_link_local()
                    && !ip.is_unspecified()
            }
            Protocol::Ip6(ip) => routable = !ip.is_loopback() && !ip.is_unspecified(),
            _ => {}
        }
    }
    routable
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of this test's own.
    ///
    /// Named after the test rather than the process: these run in parallel in
    /// one process, and the first version shared one directory, so two tests
    /// wrote the same file and the one that read it got the other's answer.
    fn dir(who: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("p2p-peerbook-{}-{who}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_missing_book_is_an_empty_book() {
        let d = dir("missing").join("nowhere");
        assert!(load(&d).is_empty());
    }

    #[test]
    fn what_is_written_is_what_comes_back() {
        let d = dir("roundtrip");
        let peer = PeerId::random();
        let addrs: Vec<Multiaddr> = vec![
            "/ip4/203.0.113.4/udp/4001/quic-v1".parse().unwrap(),
            "/ip4/203.0.113.4/tcp/4001".parse().unwrap(),
        ];
        save(&d, &[(peer, addrs.clone())]).unwrap();
        let back = load(&d);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].0, peer);
        assert_eq!(back[0].1, addrs);
    }

    /// A damaged cache must not stop the client, and must not smuggle a
    /// half-parsed line through as a peer.
    #[test]
    fn a_damaged_book_is_survived_line_by_line() {
        let d = dir("damaged");
        let peer = PeerId::random();
        let good = format!("{peer} /ip4/203.0.113.4/tcp/4001");
        std::fs::write(
            path(&d),
            format!("not-a-peer-id /ip4/203.0.113.4/tcp/4001
{good}
Qm

"),
        )
        .unwrap();
        let back = load(&d);
        assert_eq!(back.len(), 1, "only the one good line survives");
        assert_eq!(back[0].0, peer);
    }

    /// An entry with a peer id and no usable address is not an entry: dialling
    /// by peer id alone is what the DHT is for.
    #[test]
    fn a_peer_with_no_addresses_is_not_written_down() {
        let d = dir("noaddrs");
        let peer = PeerId::random();
        std::fs::write(path(&d), format!("{peer}
")).unwrap();
        assert!(load(&d).is_empty());
    }

    /// Addresses that mean something only here, or only for the next few
    /// minutes, are not worth a line in a file read tomorrow.
    #[test]
    fn only_addresses_that_will_mean_something_tomorrow() {
        let keep: Multiaddr = "/ip4/203.0.113.4/udp/4001/quic-v1".parse().unwrap();
        assert!(worth_keeping(&keep));

        for drop in [
            "/ip4/127.0.0.1/tcp/4001",
            "/ip4/192.168.1.21/tcp/4001",
            "/ip4/0.0.0.0/tcp/4001",
        ] {
            let a: Multiaddr = drop.parse().unwrap();
            assert!(!worth_keeping(&a), "{drop}");
        }

        // A circuit belongs to a reservation, and a reservation is minutes old
        // by the time this file is read again.
        let circuit: Multiaddr =
            "/ip4/203.0.113.4/tcp/4001/p2p/12D3KooWLZ18MNzm9xu7GZRQ16c6k3CZLN529CbUqrYaWNqyYko9/p2p-circuit"
                .parse()
                .unwrap();
        assert!(!worth_keeping(&circuit));
    }
}
