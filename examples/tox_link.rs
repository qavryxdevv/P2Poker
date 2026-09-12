//! Does a Tox group carry the table's traffic between two machines on two
//! networks? (D-019.)
//!
//! This is the measurement the decision rests on, reduced to the smallest
//! program that can take it. It carries no poker: it makes the group, invites
//! across the boundary, and pushes packets of the size this protocol actually
//! sends until the time runs out.
//!
//! ```text
//! tox_link --seed <64 hex> --print-key
//! tox_link --seed <64 hex> --peer <64 hex> --host --for 300
//! tox_link --seed <64 hex> --peer <64 hex>        --for 300
//! ```
//!
//! # Why both ends are given a key, and why the identity is seeded
//!
//! **A Tox friendship is two-sided.** `tox_friend_add_norequest` on one side
//! alone establishes nothing: the other end has never heard of the caller and
//! will not answer it. The first version of this probe had only the joiner add
//! the host, and the result was a hundred and fifty seconds of silence that
//! read exactly like a network failure — two machines, no friend connection,
//! nothing to distinguish "the far end is unreachable" from "I only did half of
//! the handshake".
//!
//! In the client this cannot happen, because both ends take both keys off the
//! ratified roster. Here there is no roster, so the identity is fixed by
//! `--seed` and each end is told the other's public key with `--peer` before
//! either of them runs. `--print-key` computes a key from a seed and exits, so
//! the script can work both out locally before it starts anything.
//!
//! **What a passing run proves and does not prove.** It proves an invitation
//! crossed a real network boundary and that lossless packets flowed both ways
//! afterwards, with no libp2p relay and no per-circuit byte cap. It does not
//! prove NAT traversal unless the two machines are on different NATs — the same
//! caveat `tools/two-network-ssh.ps1` prints, for the same reason.

#[cfg(not(feature = "tox"))]
fn main() {
    eprintln!("built without --features tox, so there is no Tox to link");
    std::process::exit(2);
}

#[cfg(feature = "tox")]
fn main() {
    use p2p_poker::tox::{Event, Tox};
    use std::time::{Duration, Instant};

    let args: Vec<String> = std::env::args().collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let value = |f: &str| {
        args.iter()
            .position(|a| a == f)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };

    let seconds: u64 = value("--for")
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    let host = has("--host");

    fn hex32(s: &str) -> Option<[u8; 32]> {
        if s.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, pair) in s.as_bytes().chunks(2).enumerate() {
            out[i] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
        }
        Some(out)
    }

    let Some(seed) = value("--seed").as_deref().and_then(hex32) else {
        eprintln!("--seed wants 64 hex characters");
        std::process::exit(2);
    };

    // `--tcp-only` is not a preference: two hosts behind one router on
    // different subnets cannot reach each other over Tox's UDP path at all, so
    // this is the switch that decides whether such a pair can play.
    let tcp_only = has("--tcp-only");
    let made = if tcp_only {
        Tox::tcp_only(&seed)
    } else {
        Tox::with_secret_key(&seed)
    };
    let mut tox = match made {
        Ok(t) => t,
        Err(e) => {
            eprintln!("no tox instance: {e}");
            std::process::exit(1);
        }
    };

    // Work out the identity and stop, so a caller can learn both keys before it
    // starts either end.
    if has("--print-key") {
        let key: String = tox.address()[..32]
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect();
        println!("{key}");
        return;
    }

    let peer = value("--peer").as_deref().and_then(hex32);
    if peer.is_none() {
        eprintln!("--peer wants the other end's 64 hex characters: a Tox friendship is two-sided, and adding one side of it establishes nothing");
        std::process::exit(2);
    }

    // **The node list, kept current by the client rather than baked in.**
    //
    // `nodes::load` is the bundled list plus whatever a previous fetch cached,
    // and `nodes::refresh` updates the cache from `https://nodes.tox.chat/json`
    // when it is a day old. A fetched list is **added** to the bundled one and
    // never replaces it, so a bad day at that host - or a hostile one - can add
    // nodes and cannot take away the ones this binary shipped with.
    //
    // Each node goes into **two** toxcore lists: the DHT over UDP, and the
    // relay list on every TCP port it advertises. The relays are not optional
    // and not a fallback that waits for UDP to fail: a real client holds
    // connections to several of them at all times, because a relay is the only
    // route to a peer that cannot be reached directly. Measured on this bench,
    // without them two machines sharing a public address never connect at all.
    let profile = std::env::temp_dir();
    if p2p_poker::tox::nodes::stale(&profile, 0) {
        match p2p_poker::tox::nodes::refresh(&profile) {
            Ok(n) => println!("node list refreshed: {n} nodes"),
            Err(e) => println!("node list not refreshed ({e}); using what is cached"),
        }
    }
    let node_list = p2p_poker::tox::nodes::load(&profile);
    let mut relays = 0;
    for n in &node_list {
        if let Err(e) = tox.bootstrap(&n.host, n.udp_port, &n.key) {
            println!("bootstrap {} refused: {e}", n.host);
        }
        for port in &n.tcp_ports {
            if tox.add_tcp_relay(&n.host, *port, &n.key).is_ok() {
                relays += 1;
            }
        }
    }
    println!("{} nodes, {relays} relay entries", node_list.len());

    let me = tox.address();
    let key_hex: String = me[..32].iter().map(|b| format!("{b:02X}")).collect();
    println!("my key {key_hex}");
    println!("udp {}", if tcp_only { "off" } else { "on" });

    let mut group: Option<u32> = None;
    let mut invited = false;
    let mut connected_at: Option<Instant> = None;

    // **Both ends add the other.** This is the half the first version of this
    // probe left out, and leaving it out looks exactly like the far machine
    // being unreachable.
    let friend = match tox.add_friend(&peer.expect("checked above")) {
        Ok(f) => {
            println!("added the peer as friend {f}");
            Some(f)
        }
        Err(e) => {
            eprintln!("could not add the peer: {e}");
            std::process::exit(1);
        }
    };

    if host {
        match tox.new_group("TwoNetTox", "host") {
            Ok(g) => {
                let id: String = tox
                    .chat_id(g)
                    .map(|c| c.iter().map(|b| format!("{b:02X}")).collect())
                    .unwrap_or_else(|e| format!("<{e}>"));
                println!("group made, chat id {id}");
                group = Some(g);
            }
            Err(e) => {
                eprintln!("no group: {e}");
                std::process::exit(1);
            }
        }
    } else {
        println!("waiting for an invitation");
    }

    // Nine kilobytes is a `SHUFFLE_STEP`, so the packets this pushes are the
    // fragments of one: the size the transport is actually asked to carry.
    let payload = vec![0xA5u8; p2p_poker::table::fragment::payload_for(1373)];

    let began = Instant::now();
    let deadline = Duration::from_secs(seconds);
    let mut sent = 0u64;
    let mut received = 0u64;
    let mut bytes_in = 0u64;
    let mut said_up = false;
    let mut next_report = Duration::from_secs(15);
    let mut conn = tox.connection();

    while began.elapsed() < deadline {
        for e in tox.iterate() {
            match e {
                Event::FriendConnection { friend: f, status } => {
                    println!("friend {f} connection {status}");
                    if status != 0 {
                        if connected_at.is_none() {
                            connected_at = Some(Instant::now());
                            println!(
                                "friend up after {:.1}s",
                                began.elapsed().as_secs_f32()
                            );
                        }
                        if host && !invited {
                            if let Some(g) = group {
                                match tox.invite(g, f) {
                                    Ok(()) => {
                                        println!("invited friend {f}");
                                        invited = true;
                                    }
                                    Err(e) => println!("invite refused: {e}"),
                                }
                            }
                        }
                    }
                }
                Event::GroupInvite { friend: f, invite } => {
                    if Some(f) == friend {
                        match tox.accept_invite(f, &invite, "player") {
                            Ok(g) => {
                                println!(
                                    "joined the group after {:.1}s",
                                    began.elapsed().as_secs_f32()
                                );
                                group = Some(g);
                            }
                            Err(e) => println!("could not accept: {e}"),
                        }
                    } else {
                        // An invitation from somebody this client did not add
                        // from a roster. Refused by ignoring it, which is the
                        // whole policy.
                        println!("an invitation from an unknown friend {f}, ignored");
                    }
                }
                Event::GroupPacket { peer, data, .. } => {
                    received += 1;
                    bytes_in += data.len() as u64;
                    if !said_up {
                        said_up = true;
                        println!(
                            "FIRST PACKET from peer {peer} after {:.1}s, {} bytes",
                            began.elapsed().as_secs_f32(),
                            data.len()
                        );
                    }
                }
                Event::FriendRequestIgnored => println!("a friend request, ignored"),
                // The two that say whether a join actually finished. This probe
                // exists to time a link, and until they existed it could report a
                // group number as a join.
                Event::GroupSelfJoin { group } => {
                    println!("joined group {group} after {:.1}s", began.elapsed().as_secs_f32())
                }
                // The confirmed set, which peer_count is not.
                Event::GroupPeerJoin { group, peer } => {
                    println!("group {group}: peer {peer} confirmed")
                }
                Event::GroupPeerExit { group, peer, .. } => {
                    println!("group {group}: peer {peer} left")
                }
                Event::GroupJoinFail { group, reason } => {
                    println!("group {group} join failed, reason {reason}")
                }
                Event::GroupModeration { group, target_is_self, kick } => {
                    println!("group {group}: moderation, about me {target_is_self}, a kick {kick}")
                }
            }
        }

        // Both ends push, so the run measures the path in both directions.
        if let Some(g) = group {
            if tox.send(g, &payload).is_ok() {
                sent += 1;
            }
        }

        // **Say whether this instance reached the Tox network at all.**
        // Without it, "no friend connection" and "never bootstrapped" look
        // identical from outside, and the first run of this probe spent three
        // minutes producing a result that could have meant either.
        let now_conn = tox.connection();
        if now_conn != conn {
            println!(
                "self connection {conn} -> {now_conn} at {:.1}s",
                began.elapsed().as_secs_f32()
            );
            conn = now_conn;
        }

        if began.elapsed() > next_report {
            next_report += Duration::from_secs(15);
            println!(
                "at {:.0}s: self {conn}, sent {sent}, received {received} ({bytes_in} bytes)",
                began.elapsed().as_secs_f32()
            );
        }

        std::thread::sleep(tox.interval().min(Duration::from_millis(50)));
    }

    println!("RESULT self {conn} sent {sent} received {received} bytes_in {bytes_in}");
    if conn == 0 {
        println!("RESULT this end never reached the Tox network - bootstrap, not the group");
    }
    if received > 0 {
        println!("RESULT the group carried traffic across the boundary");
    } else {
        println!("RESULT nothing arrived");
    }
}
