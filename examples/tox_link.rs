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

    // **The public node list, with each node's own TCP relay ports.**
    //
    // Fetched from `https://nodes.tox.chat/json` on 2026-08-31 and filtered to
    // those reporting both UDP and TCP up. Baked in rather than fetched at run
    // time, because a measurement that depends on a web service is a
    // measurement that changes when the web service does - and because the
    // whole point of these runs is to compare one against another.
    //
    // Each node is added **twice**: to the DHT list over UDP, and to the relay
    // list on every TCP port it advertises. That is what a real client does -
    // qTox holds connections to several relays at all times, even with UDP
    // working, because a relay is the fallback for a peer that cannot be
    // reached directly and a NAT is exactly that case. The first version of
    // this probe added no relays at all and hardcoded three nodes on one port.
    const NODES: &[(&str, u16, &str, &[u16])] = &[
    ("144.217.167.73", 33445, "7E5668E0EE09E19F320AD47902419331FFEE147BB3606769CFBE921A2A2FD34C", &[33445, 3389]),
    ("205.185.115.131", 53, "3091C6BEB2A993F1C6300C16549FABA67098FF3D62C6D253828B531470B53D68", &[3389, 33445, 443, 53]),
    ("3.0.24.15", 33445, "E20ABCF38CDBFFD7D04B29C956B33F7B27A3BB7AF0618101617B036E4AEA402D", &[33445]),
    ("139.162.110.188", 33445, "F76A11284547163889DDC89A7738CF271797BF5E5E220643E97AD3C7E7903D55", &[443, 33445, 3389]),
    ("144.172.88.203", 33445, "2016A0F2797EE3A8B004BA623F11AAFC8146F1B8F45107232A1A1AECCE856674", &[33445, 443]),
    ("172.104.215.182", 33445, "DA2BD927E01CD05EBCC2574EBE5BEBB10FF59AE0B2105A7D1E2B40E49BB20239", &[443, 3389, 33445]),
    ("188.214.122.30", 33445, "2A9F7A620581D5D1B09B004624559211C5ED3D1D712E8066ACDB0896A7335705", &[3389, 33445]),
    ("43.198.227.166", 33445, "AD13AB0D434BCE6C83FE2649237183964AE3341D0AFB3BE1694B18505E4E135E", &[3389, 33445]),
    ("95.181.230.108", 33445, "B5FFECB4E4C26409EBB88DB35793E7B39BFA3BA12AC04C096950CB842E3E130A", &[3389, 33445]),
    ("188.245.84.166", 33445, "96B66D300BA2B59B98FC42DB1325E7092388F0379593E680ABDBEA03B9C9CE03", &[443, 3389, 33445]),
    ];

    let mut relays = 0;
    for (host, udp_port, key_hex, tcp_ports) in NODES {
        let mut key = [0u8; 32];
        let ok = key_hex.as_bytes().chunks(2).enumerate().all(|(i, pair)| {
            match (std::str::from_utf8(pair).ok(), i < 32) {
                (Some(s), true) => match u8::from_str_radix(s, 16) {
                    Ok(b) => {
                        key[i] = b;
                        true
                    }
                    Err(_) => false,
                },
                _ => false,
            }
        });
        if !ok {
            println!("node {host} has an unreadable key");
            continue;
        }
        if let Err(e) = tox.bootstrap(host, *udp_port, &key) {
            println!("bootstrap {host} refused: {e}");
        }
        for port in *tcp_ports {
            if tox.add_tcp_relay(host, *port, &key).is_ok() {
                relays += 1;
            }
        }
    }
    println!("{} nodes, {relays} relay entries", NODES.len());

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
