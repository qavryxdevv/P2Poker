//! **A real hand of poker over a Tox group, between two machines.**
//!
//! This is the measurement D-019 is actually for. `tox_link` proved the group
//! carries bytes; this proves it carries *the game* — `HAND_INIT`, the deck
//! chain, `DEAL_PRIVATE`, the betting, the showdown and `HAND_COMPLETE`, over
//! the same transport, with the same driver, fragmenting every message that
//! does not fit in a 1373-byte packet.
//!
//! ```text
//! tox_hand --seed <64 hex> --peer <64 hex> --host [--hands 3] [--for 300]
//! tox_hand --seed <64 hex> --peer <64 hex>        [--hands 3] [--for 300]
//! ```
//!
//! `--print-key` computes an identity from a seed and exits, so a script can
//! learn both keys before starting either end. `tools/two-network-tox.ps1`'s
//! `-Hand` switch does exactly that.
//!
//! # What it is and is not
//!
//! It is the hand driver on a Tox transport and **nothing else**: no lobby, no
//! join RPC, no ratification. The two ends are given the same `Opening` from the
//! same constants, which is what the formation would otherwise have agreed —
//! and it is fair to hand it over, because what is under test is whether the
//! transport carries a hand, not whether two clients can agree to start one.
//! Everything after that is the real protocol, verified at both ends.
//!
//! Both seats play the same way — check when they can, call when they cannot —
//! because the point is to move every stage of a hand across the wire, not to
//! play well.

#[cfg(not(feature = "tox"))]
fn main() {
    eprintln!("built without --features tox, so there is no Tox to play over");
    std::process::exit(2);
}

#[cfg(feature = "tox")]
#[tokio::main]
async fn main() {
    use p2p_poker::poker::actions::Action;
    use p2p_poker::table::hand::{Hand, Opening, Send as HandSend};
    use p2p_poker::table::transport::TableTransport;
    use p2p_poker::tox::table::{spawn, Role, Setup};
    use p2p_poker::tox::Tox;
    use std::time::{Duration, Instant};

    let args: Vec<String> = std::env::args().collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let value = |f: &str| {
        args.iter()
            .position(|a| a == f)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let number = |f: &str, d: u64| value(f).and_then(|v| v.parse().ok()).unwrap_or(d);

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
    let mut tox = match Tox::with_secret_key(&seed) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("no tox instance: {e}");
            std::process::exit(1);
        }
    };
    if has("--print-key") {
        let k: String = tox.address()[..32]
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect();
        println!("{k}");
        return;
    }
    let Some(peer) = value("--peer").as_deref().and_then(hex32) else {
        eprintln!("--peer wants the other end's 64 hex characters");
        std::process::exit(2);
    };

    let host = has("--host");
    let seconds = number("--for", 300);
    let want_hands = number("--hands", 3) as usize;

    // The node list, kept current and added to **both** toxcore lists. Without
    // the relays two machines behind one router never meet, measured.
    let profile = std::env::temp_dir();
    if p2p_poker::tox::nodes::stale(&profile, 0) {
        match p2p_poker::tox::nodes::refresh(&profile) {
            Ok(n) => println!("node list refreshed: {n} nodes"),
            Err(e) => println!("node list not refreshed ({e}); using what is cached"),
        }
    }
    for n in p2p_poker::tox::nodes::load(&profile) {
        let _ = tox.bootstrap(&n.host, n.udp_port, &n.key);
        for p in &n.tcp_ports {
            let _ = tox.add_tcp_relay(&n.host, *p, &n.key);
        }
    }

    // --- the two seats, and the opening they would have agreed --------------
    //
    // Seat 0 hosts, seat 1 joins. The signing keys are derived from the same
    // seeds as the Tox identities so that both ends can build the same roster
    // without a lobby: seat 0's application key is the host's seed, seat 1's is
    // the joiner's, and each end knows both.
    let my_seat: u8 = if host { 0 } else { 1 };
    let (seat0_seed, seat1_seed) = if host { (seed, peer_seed()) } else { (peer_seed(), seed) };
    // The peer's *signing* key cannot be derived from its Tox key, so both
    // seeds are fixed by the script and this end knows the other's. That is a
    // shortcut a lobby would not need and is why this program is a measurement
    // and not a client.
    fn peer_seed() -> [u8; 32] {
        std::env::var("TOX_HAND_PEER_SEED")
            .ok()
            .and_then(|v| {
                let b: Vec<u8> = v
                    .as_bytes()
                    .chunks(2)
                    .filter_map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok())
                    .collect();
                <[u8; 32]>::try_from(b).ok()
            })
            .unwrap_or([0xFE; 32])
    }
    let sk0 = ed25519_dalek::SigningKey::from_bytes(&seat0_seed);
    let sk1 = ed25519_dalek::SigningKey::from_bytes(&seat1_seed);
    let my_key = if host { sk0.clone() } else { sk1.clone() };

    let opening = |hand_id: u64, genesis: [u8; 32]| Opening {
        table_id: [0x11; 32],
        hand_id,
        session_id: [0x22; 32],
        roster_hash: [0x33; 32],
        genesis,
        required: vec![0, 1],
        readmitted: Vec::new(),
        every_n_hands: 11,
        first_small_blind: 50,
        small_blind_cap: 50_000,
        seats: vec![
            (0, sk0.verifying_key().to_bytes(), 10_000),
            (1, sk1.verifying_key().to_bytes(), 10_000),
        ],
        max_players: 2,
        small_blind: 50,
        big_blind: 100,
        level: 1,
        my_seat,
        crypto_step_timeout_ms: 30_000,
        action_timeout_ms: 20_000,
        action_grace_ms: 5_000,
        hand_delay_ms: 2_000,
        time_bank_ms: 0,
        hand_deadline_ms: 600_000,
        grace: vec![2, 2],
        present_run: vec![0, 0],
        returns: vec![0, 0],
        out: Vec::new(),
        button: None,
    };

    // --- the transport ------------------------------------------------------
    let mut table = if host {
        spawn(
            tox,
            Setup {
                role: Role::Host,
                group_name: "ToxHand".into(),
                self_name: "seat0".into(),
                roster: vec![peer],
                binder: None,
            },
        )
    } else {
        // The joiner needs the chat id the advertisement would have carried.
        // Here it is derived the only way it can be without a lobby: the host
        // prints it and the script passes it in.
        let chat = value("--chat").as_deref().and_then(hex32);
        spawn(
            tox,
            Setup {
                role: Role::Joiner {
                    founder: peer,
                    chat_id: chat,
                },
                group_name: "ToxHand".into(),
                self_name: "seat1".into(),
                roster: vec![peer],
                binder: None,
            },
        )
    };

    if host {
        if let Some(id) = table.wait_for_chat_id().await {
            let hex: String = id.iter().map(|b| format!("{b:02X}")).collect();
            println!("chat id {hex}");
        }
    }

    // --- play ---------------------------------------------------------------
    let began = Instant::now();
    let deadline = Duration::from_secs(seconds);
    let now = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    };

    let mut genesis = [0x44u8; 32];
    let mut played = 0usize;
    let mut hand_id = 1u64;

    'hands: while played < want_hands && began.elapsed() < deadline {
        let (mut h, opening_sends) = match Hand::open(opening(hand_id, genesis), &my_key, now(), 30_000) {
            Ok(v) => v,
            Err(e) => {
                println!("could not open hand {hand_id}: {e}");
                break;
            }
        };
        println!("hand {hand_id} opened, seat {my_seat}");
        // **Everything this seat says is kept and re-sent.** A Tox group keeps
        // no history, exactly as GossipSub keeps none: a message published
        // before the other seat was in the group was published to nobody, and
        // nothing re-delivers it. `run.rs` has the same five-second loop over
        // the same buffer for the same reason, and D-019 does not remove the
        // need for it - it changes which transport has no history.
        //
        // Measured without it: the joiner's `HAND_INIT` went out while the group
        // was empty, the joiner then heard the host's and considered stage 0
        // complete, and stopped re-sending. The host held one event it could not
        // place and waited out the hand. Both ends were healthy and the hand was
        // dead.
        let mut said_bytes: Vec<Vec<u8>> = opening_sends
            .into_iter()
            .map(|HandSend::Broadcast(b)| b)
            .collect();
        for b in &said_bytes {
            let _ = table.broadcast(b).await;
        }

        let mut last_push = Instant::now();
        let mut last_say = Instant::now();
        let mut heard = 0u64;
        let mut said = 0u64;
        loop {
            // **Say what the transport is doing.** The first run of this
            // printed one refusal in two minutes and nothing else, which is
            // unattributable: a joiner that never entered the group, a group
            // with no peers to send to, and a hand that will not progress all
            // look identical from outside. The same lesson `tox_link` taught.
            if last_say.elapsed() > Duration::from_secs(10) {
                last_say = Instant::now();
                let g = table
                    .chat_id()
                    .map(|c| c[..4].iter().map(|b| format!("{b:02x}")).collect::<String>())
                    .unwrap_or_else(|| "-".into());
                println!(
                    "  at {:.0}s group {g}, said {said}, heard {heard}, dealt {}, stage {}",
                    began.elapsed().as_secs_f32(),
                    h.dealt(),
                    h.slot().sequence
                );
            }
            if began.elapsed() >= deadline {
                println!("out of time in hand {hand_id}");
                break 'hands;
            }

            // Anything that arrived.
            match tokio::time::timeout(Duration::from_millis(200), table.next()).await {
                Ok(Some(item)) => {
                    heard += 1;
                    match h.on_event(&item.bytes, &my_key, now()) {
                    Ok(sends) => {
                        let (mut more, _) = h.replay_early(&my_key, now());
                        let mut all = sends;
                        all.append(&mut more);
                        for HandSend::Broadcast(b) in all {
                            said += 1;
                            let _ = table.broadcast(&b).await;
                            said_bytes.push(b);
                        }
                    }
                    Err(e) => {
                        // Held rather than refused: the hand keeps what it
                        // cannot place yet and replays it when it can. Said
                        // once, because a hand of 2m+4 stages hears events out
                        // of order as a matter of course.
                        let text = format!("{e}");
                        if !text.contains("has not reached") {
                            println!("refused: {e}");
                        }
                    }
                    }
                }
                Ok(None) => {
                    println!("the transport closed");
                    break 'hands;
                }
                Err(_) => {}
            }

            // This seat's turn, if it is.
            if let Some(turn) = h.turn().filter(|t| t.mine) {
                let action = if turn.legal.can_check {
                    Action::Check
                } else {
                    Action::Call
                };
                match h.act(action, &my_key, now()) {
                    Ok(sends) => {
                        println!("  acted {action:?} on {:?}", turn.street);
                        for HandSend::Broadcast(b) in sends {
                            said += 1;
                            let _ = table.broadcast(&b).await;
                            said_bytes.push(b);
                        }
                    }
                    Err(e) => println!("  could not act: {e}"),
                }
            }

            // **The opening is re-sent while nothing has come back.** The other
            // end may not have been in the group when it first went out — the
            // founder learns a peer has joined after the peer does — and a
            // group keeps no history, so the first `HAND_INIT` is often sent to
            // nobody. The stored bytes are re-broadcast rather than re-sealed:
            // a second signature over a second timestamp would be a second
            // event at the same slot, which is an equivocation against this
            // client by its own hand.
            if last_push.elapsed() > Duration::from_secs(5) {
                last_push = Instant::now();
                for b in &said_bytes {
                    let _ = table.broadcast(b).await;
                }
            }

            if let Some(next) = h.next_hand() {
                played += 1;
                println!(
                    "HAND {hand_id} DONE after {:.1}s ({} of {want_hands})",
                    began.elapsed().as_secs_f32(),
                    played
                );
                genesis = next.genesis;
                hand_id = next.hand_id;
                break;
            }
            if h.aborted().is_some() {
                println!("hand {hand_id} aborted: {:?}", h.aborted());
                break 'hands;
            }
        }
    }

    println!("RESULT hands {played} of {want_hands} in {:.1}s", began.elapsed().as_secs_f32());
    if played > 0 {
        println!("RESULT a hand of poker played over a Tox group");
    } else {
        println!("RESULT no hand completed");
    }
    table.leave().await;
}
