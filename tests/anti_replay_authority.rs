//! There is **one** anti-replay authority in this client, and it is the running
//! code. This file is the guard on that sentence.
//!
//! `PROTOCOL.md` §5.2.1 defined an eight-tuple slot key, and `src/protocol/`
//! carried it — `slot.rs`, `antireplay.rs`, and behind them `staleness.rs`,
//! `checkpoint.rs` and `seats.rs` — fully written, fully tested, and called by
//! nothing. It was deleted rather than wired in, for a reason this file keeps
//! executable: **applied to this tree the key convicts honest peers.** A
//! `TABLE_READY` is chained at `hand_id = 0, sequence = 0` for every seat at
//! every `list_serial` (`net/joinwire.rs` `seal_ready`), and an honest client
//! re-ratifies on every serial change (`net/formation.rs` `adopt`, which resets
//! `sent_ready`). The fields that distinguish those emissions — `list_serial`
//! and `roster_hash` — live in the payload, which §5.2.1 deliberately excludes.
//! So an ordinary five-seat formation puts several distinct honest bodies in
//! one slot, and the predicate over that key calls each of them equivocation.
//!
//! Each test below says, in its own doc comment, how to make it fail.

use ed25519_dalek::SigningKey;
use p2p_poker::net::advert;
use p2p_poker::net::chained;
use p2p_poker::net::formation::{Formation, Send as Emit};
use p2p_poker::net::joinwire;
use p2p_poker::net::lobby::{BlindSchedule, Mode, TableAd, DECK_SUITE_V1};
use p2p_poker::protocol::constants::hand_deadline_min_ms;
use p2p_poker::protocol::messages::{EventBody, EventType, SignedEvent};
use p2p_poker::protocol::serialization::from_canonical;
use p2p_poker::protocol::transcript::event_hash;
use p2p_poker::table::hand::{Failed, Hand, Opening, Send, GRACE_HANDS};
use std::collections::HashMap;

const NOW: u64 = 1_700_000_000_000;
const CAP: usize = 64 * 1024;

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

// ---------------------------------------------------------------------------
// 1. One authority, and it is not a module nothing calls.
// ---------------------------------------------------------------------------

/// The deleted modules stay deleted, and nothing imports them.
///
/// This project has twice shipped two clients that behaved differently under
/// one identity. A second, unrun opinion about which events are duplicates is
/// that hazard in miniature: it disagrees with the running checks (it refuses
/// `event_class` 1 and 2 outright, and this tree emits both — `net/run.rs`
/// `vote_on_timeouts`, `table/hand.rs` `say_at`), and it disagrees with them
/// silently, because nothing executes it.
///
/// **To make this fail:** restore any of the five files under `src/protocol/`,
/// or add a `use` of one, or introduce a second module that indexes accepted
/// events by a key of its own.
#[test]
fn the_anti_replay_authority_is_not_duplicated() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    // The three that carried a rule about which events to REJECT. Every one
    // of them passed its own tests while disagreeing with the running code,
    // and that is the shape this guard is against.
    //
    // `checkpoint.rs` and `seats.rs` are deliberately not in this list. They
    // are a draft of §6.2's checkpoint records and the seat set beneath it:
    // unwired, but they refuse nothing, so wiring them in would add a stage
    // this client does not have rather than overrule a check it runs.
    for gone in ["slot.rs", "antireplay.rs", "staleness.rs"] {
        let p = root.join("src/protocol").join(gone);
        assert!(
            !p.exists(),
            "src/protocol/{gone} is back. It was deleted because the key it \
             implements convicts honest peers on this tree — see \
             `an_honest_formation_puts_several_bodies_in_one_slot_key` below. \
             If it is being restored, that test must be made to pass first."
        );
    }

    // And nothing refers to them, in src or in tests.
    let mut offenders = Vec::new();
    for dir in ["src", "tests"] {
        walk(&root.join(dir), &mut |path: &std::path::Path| {
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                return;
            }
            // This file names them on purpose.
            if path.file_name().and_then(|f| f.to_str()) == Some("anti_replay_authority.rs") {
                return;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                return;
            };
            // The three deleted modules, and the type that was their entry
            // point. `protocol::checkpoint` and `protocol::seats` are NOT here:
            // they may refer to each other, because a draft of §6.2 is allowed
            // to be internally complete. What they may not do is reach the live
            // code, and that is the next check.
            for needle in [
                "protocol::slot",
                "protocol::antireplay",
                "protocol::staleness",
                "AntiReplay",
            ] {
                if text.contains(needle) {
                    offenders.push(format!("{} mentions {needle}", path.display()));
                }
            }
        });
    }
    assert!(
        offenders.is_empty(),
        "a second anti-replay authority is being wired in:\n{}",
        offenders.join("\n")
    );
}

fn walk(dir: &std::path::Path, f: &mut impl FnMut(&std::path::Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, f);
        } else {
            f(&p);
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Why the key was deleted rather than wired in.
// ---------------------------------------------------------------------------

/// **The reason for the deletion, executable.**
///
/// An attack-free five-seat formation. Several honest `TABLE_READY` bodies from
/// one seat land in one §5.2.1 slot — same `protocol_version`, `table_id`,
/// `hand_id = 0`, `sequence = 0`, `sender_public_key`, `event_class = 0`,
/// `event_type`, and no subject — with different `event_hash` values, because
/// `list_serial` and `roster_hash` are payload fields and the key excludes the
/// payload by design. A predicate over that key labels each repeat an
/// equivocation by a peer that did exactly what `formation.rs` requires of it.
///
/// `PROTOCOL.md` §4.11 row 14 rated `TABLE_READY` "one per seat, setup chain /
/// — / Clean". That row is wrong against this tree, and it is the sixth
/// recurrence of the defect §5.2.1 says five review passes were spent on.
///
/// **To make this fail:** give each ratification its own `sequence` (a real fix
/// for the message, per §5.2.1's own closing rule that a colliding honest
/// emission is a defect in the message), or stop re-ratifying on a serial
/// change. Either would make the key safe here — and either is a wire change
/// that has to be decided on its own merits, not smuggled in with a store.
#[test]
fn an_honest_formation_puts_several_bodies_in_one_slot_key() {
    let mut t = Table::new();
    for seed in 1..=4u8 {
        t.add(seed);
    }

    // The table forms. Nobody misbehaved.
    let settled = t.founder.session().expect("the founder settles");
    for x in &t.seats {
        assert_eq!(x.session(), Some(settled), "every seat agrees");
    }

    // Now group every broadcast ratification by the §5.2.1 key, rebuilt here
    // field for field so this test does not depend on the deleted module.
    let mut cells: HashMap<SlotKey, Vec<[u8; 32]>> = HashMap::new();
    for bytes in &t.seen_ready {
        let signed: SignedEvent = from_canonical(bytes, CAP).unwrap();
        let env: EventBody = from_canonical(&signed.body, CAP).unwrap();
        assert_eq!(env.chain_scope, 1, "TABLE_READY is chained, so it has a slot");
        assert_eq!(env.hand_id, 0);
        assert_eq!(env.sequence, 0, "every ratification is pinned to stage 0");
        cells
            .entry(SlotKey::of(&env))
            .or_default()
            .push(event_hash(&signed.body));
    }

    let mut convicted = 0usize;
    for hashes in cells.values() {
        let mut distinct = hashes.clone();
        distinct.sort();
        distinct.dedup();
        // Every distinct body after the first is what the predicate calls
        // evidence of equivocation.
        convicted += distinct.len().saturating_sub(1);
    }

    assert!(
        convicted > 0,
        "expected honest peers to collide in the §5.2.1 key on this tree; \
         if this is now 0, TABLE_READY's placement changed and the key may be \
         safe here — re-open the wire-in decision rather than deleting this test"
    );
    println!(
        "{} honest ratifications, {} slot keys, {convicted} honest bodies the \
         §5.2.1 predicate would call equivocation",
        t.seen_ready.len(),
        cells.len()
    );
}

/// The eight-tuple of `PROTOCOL.md` §5.2.1, rebuilt locally.
///
/// Deliberately not imported from a shared module: the corpus records that
/// "a test written against a copy is how three of the five recurrences survived
/// review". This copy exists to be *compared against the specification text by
/// a reader*, and it indexes nothing at runtime.
#[derive(PartialEq, Eq, Hash)]
struct SlotKey {
    protocol_version: u16,
    table_id: [u8; 32],
    hand_id: u64,
    sequence: u64,
    sender_public_key: [u8; 32],
    event_class: u8,
    event_type: u16,
    // subject(E) is () for event_class 0, which is every type in this test.
}

impl SlotKey {
    fn of(e: &EventBody) -> SlotKey {
        SlotKey {
            protocol_version: e.protocol_version,
            table_id: e.table_id,
            hand_id: e.hand_id,
            sequence: e.sequence,
            sender_public_key: e.sender_public_key,
            event_class: e.event_class,
            event_type: e.event_type,
        }
    }
}

// ---------------------------------------------------------------------------
// 3. The checks that actually carry the property.
// ---------------------------------------------------------------------------

/// **Duplicate suppression and equivocation naming at a collective stage** is
/// `table/stage.rs` `Collective::hear`, not a slot store.
///
/// One seat seals the same payload twice with two `emitted_at_unix_ms` values.
/// Both verify. The receiver keeps the first and names the second.
///
/// **To make this fail:** change `Collective::hear`'s `Some(first)` arm to
/// overwrite instead of returning `Heard::Equivocation` — which is what
/// `Formation::take_ratification` and `Hand::take_vote` already do, and which
/// is why neither of those detects a double signer.
#[test]
fn a_double_signer_at_a_collective_stage_is_still_named() {
    let (mut v, _) = Hand::open(opening3(0), &key(10), NOW, 30_000).unwrap();
    let (_, from_2) = Hand::open(opening3(2), &key(12), NOW, 30_000).unwrap();

    let a = broadcast(&from_2[0]).to_vec();
    let signed: SignedEvent = from_canonical(&a, CAP).unwrap();
    let env: EventBody = from_canonical(&signed.body, CAP).unwrap();
    let at = chained::Slot {
        table_id: env.table_id,
        hand_id: env.hand_id,
        sequence: env.sequence,
        previous_event_hash: env.previous_event_hash,
    };
    // The identical payload, one millisecond later.
    let b = chained::seal(
        EventType::HandInit,
        &at,
        &Raw(env.payload.clone()),
        &key(12),
        NOW + 1,
        30_000,
        CAP,
    )
    .unwrap();

    assert_ne!(a, b);
    assert!(chained::open(&a, CAP, EventType::HandInit, &at).is_ok());
    assert!(
        chained::open(&b, CAP, EventType::HandInit, &at).is_ok(),
        "both are validly signed into one slot: nothing in the envelope \
         constrains emitted_at_unix_ms"
    );

    v.on_event(&a, &key(10), NOW).expect("the first is taken");
    let second = v.on_event(&b, &key(10), NOW);
    assert!(
        matches!(second, Err(Failed::Equivocation { seat: 2 })),
        "the second body from one seat at one stage must be named, got {second:?}"
    );
}

/// **The principal replay bound for every ordinary chained event** is the stage
/// cursor in `Hand::on_event` — `sequence < self.slot.sequence` — not a store.
///
/// **To make this fail:** delete that guard in `src/table/hand.rs`. The replay
/// then reaches `dispatch`, where the phase handler answers it.
#[test]
fn a_stage_the_hand_has_left_still_swallows_every_copy() {
    let (mut v, _) = Hand::open(opening3(0), &key(10), NOW, 30_000).unwrap();
    let (_, from_1) = Hand::open(opening3(1), &key(11), NOW, 30_000).unwrap();
    let (_, from_2) = Hand::open(opening3(2), &key(12), NOW, 30_000).unwrap();

    let one = broadcast(&from_1[0]).to_vec();
    v.on_event(&one, &key(10), NOW).unwrap();
    v.on_event(broadcast(&from_2[0]), &key(10), NOW).unwrap();
    assert_eq!(v.slot().sequence, 1, "stage 0 is behind it");

    let before = v.slot();
    let again = v.on_event(&one, &key(10), NOW);
    assert_eq!(
        again,
        Ok(Vec::new()),
        "a copy of a stage already left is dropped without a word"
    );
    assert_eq!(v.slot(), before, "and moves nothing");
}

/// **Unchained traffic is bounded per type, never by a slot** — and the
/// eight-tuple could not have covered it if it had been wired in, because
/// `slot()` refuses a `chain_scope == 0` event by construction.
///
/// **To make this fail:** give any of these types `chain_scope = 1`.
#[test]
fn the_lobby_and_join_types_occupy_no_slot_at_all() {
    for t in [
        EventType::LobbyTableAd,
        EventType::PlayerList,
        EventType::JoinRequest,
        EventType::JoinAccept,
        EventType::LobbyChat,
        EventType::Dispute,
    ] {
        assert_eq!(
            t.chain_scope(),
            0,
            "{t} is unchained, so its anti-replay is per-type and nothing a \
             slot key indexes"
        );
    }
}

// ---------------------------------------------------------------------------
// scaffolding
// ---------------------------------------------------------------------------

fn broadcast(s: &Send) -> &[u8] {
    let Send::Broadcast(b) = s;
    b
}

/// Re-encodes to exactly the bytes it was given, so a payload can be re-sealed
/// without being understood.
struct Raw(Vec<u8>);
impl minicbor::Encode<()> for Raw {
    fn encode<W: minicbor::encode::Write>(
        &self,
        e: &mut minicbor::Encoder<W>,
        _: &mut (),
    ) -> Result<(), minicbor::encode::Error<W::Error>> {
        e.writer_mut()
            .write_all(&self.0)
            .map_err(minicbor::encode::Error::write)?;
        Ok(())
    }
}

fn opening3(my_seat: u8) -> Opening {
    Opening {
        table_id: [1; 32],
        hand_id: 1,
        session_id: [2; 32],
        roster_hash: [3; 32],
        genesis: [4; 32],
        required: vec![0, 1, 2],
        seats: vec![
            (0, key(10).verifying_key().to_bytes(), 10_000),
            (1, key(11).verifying_key().to_bytes(), 10_000),
            (2, key(12).verifying_key().to_bytes(), 10_000),
        ],
        max_players: 3,
        small_blind: 50,
        big_blind: 100,
        level: 1,
        my_seat,
        crypto_step_timeout_ms: 30_000,
        action_timeout_ms: 20_000,
        action_grace_ms: 5_000,
        hand_delay_ms: 7_000,
        time_bank_ms: 0,
        hand_deadline_ms: 600_000,
        grace: vec![GRACE_HANDS; 3],
        present_run: vec![0; 3],
        button: None,
    }
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

struct Table {
    founder: Formation,
    seats: Vec<Formation>,
    ad: TableAd,
    advert_hash: [u8; 32],
    table_id: [u8; 32],
    /// Every `TABLE_READY` ever broadcast on this table's topic, in order.
    seen_ready: Vec<Vec<u8>>,
}

impl Table {
    fn new() -> Table {
        let table_key = key(100);
        let founder_app = key(0);
        let a = ad(founder_app.verifying_key().to_bytes(), vec![0u8; 38]);
        let event = advert::publish(&a, &table_key).unwrap();
        let (_, advert_hash) = advert::verify_echoed(&event).unwrap();
        let table_id = table_key.verifying_key().to_bytes();
        let founder = Formation::found(
            founder_app,
            table_key,
            a.clone(),
            event,
            advert_hash,
            None,
            vec![0u8; 38],
            "F".into(),
            1_000,
        )
        .unwrap();
        Table {
            founder,
            seats: vec![],
            ad: a,
            advert_hash,
            table_id,
            seen_ready: vec![],
        }
    }

    fn add(&mut self, seed: u8) {
        let (mut joiner, request) = Formation::join(
            key(seed),
            self.ad.clone(),
            self.advert_hash,
            self.table_id,
            vec![seed; 38],
            format!("P{seed}"),
            1_000,
            None,
            None,
            [seed; 32],
            NOW,
            None,
        )
        .unwrap();

        let out = self
            .founder
            .on_join_request(&request, &[seed; 38], NOW)
            .unwrap();
        let (mut accept, mut list, mut founder_ready) = (None, None, None);
        for s in out {
            match s {
                Emit::Reply(b) => accept = Some(b),
                Emit::Broadcast(b) => {
                    if joinwire::receive_player_list(&b).is_ok() {
                        list = Some(b);
                    } else {
                        founder_ready = Some(b);
                    }
                }
            }
        }
        let accept = accept.expect("an acceptance");
        let list = list.expect("a player list");
        joiner.on_join_answer(&accept, NOW).unwrap();

        let mut readies: Vec<Vec<u8>> = vec![];
        if let Some(b) = founder_ready {
            readies.push(b);
        }
        for s in self.seats.iter_mut() {
            for e in s.on_player_list(&list, NOW).unwrap() {
                if let Emit::Broadcast(b) = e {
                    readies.push(b);
                }
            }
        }
        for e in joiner.on_player_list(&list, NOW).unwrap() {
            if let Emit::Broadcast(b) = e {
                readies.push(b);
            }
        }
        self.seats.push(joiner);

        for r in &readies {
            let _ = self.founder.on_table_ready(r);
            for s in self.seats.iter_mut() {
                let _ = s.on_table_ready(r);
            }
        }
        self.seen_ready.extend(readies);
    }
}


/// **Nothing is held, and nothing is forwarded, that this client cannot
/// verify.**
///
/// `Hand::on_event` routes on `chained::peek`, which checks no signature and no
/// key — so `Failed::NotYet` is reachable by unsigned junk with a matching type
/// byte and a sequence one ahead. The node answered that by storing the bytes
/// and telling GossipSub to **Accept**, which forwards a forgery in this
/// client's own name and evicts a genuine early event from a queue of
/// sixty-four. It needs no key, no seat and no table membership, which made it
/// the cheapest attack in the set.
///
/// **To make this fail:** take the `open_in_hand` check out of `Hand::hold`, or
/// have it return `Holding::Kept` unconditionally.
#[test]
fn unsigned_junk_is_neither_held_nor_forwarded() {
    use p2p_poker::table::hand::Holding;

    let (mut h, _) = Hand::open(opening3(0), &key(10), NOW, 30_000).expect("the hand opens");
    assert_eq!(h.held(), 0);

    let mut refused = 0usize;
    for n in 0..80u8 {
        let junk = vec![n; 64 + usize::from(n)];
        if h.hold(junk) != Holding::Kept {
            refused += 1;
        }
    }
    assert_eq!(refused, 80, "junk was kept");
    assert_eq!(h.held(), 0, "the queue took bytes it could not verify");
}


/// **Formation is a collective stage and answers a double signer like one.**
///
/// `session_id` is computed from the ratification `event_hash`es and goes into
/// every subsequent hand's genesis. The map they were collected in used
/// `insert`, so a seat that ratified twice at one `list_serial` overwrote its
/// own first copy, and which one a peer held was decided by arrival order —
/// two honest peers, two orders, two tables that can never speak to each other.
///
/// The second copy needs no forged signature. `event_hash` covers the envelope,
/// so one seat re-sealing the same body a millisecond later produces a
/// different hash and `admit_ready` has nothing to object to. This test does
/// exactly that, by replaying a ratification the table already accepted through
/// a fresh seal.
///
/// **To make this fail:** put `self.ratified.insert(...)` back in
/// `Formation::take_ratification` in place of the first-copy-wins match.
#[test]
fn a_seat_that_ratifies_twice_is_named_rather_than_letting_the_last_one_win() {
    use p2p_poker::net::formation::Failed as FormFailed;

    let mut t = Table::new();
    for seed in 1..=2u8 {
        t.add(seed);
    }
    let settled = t.founder.session().expect("the table forms");

    // One seat's own ratification, re-sealed a millisecond later. Same body,
    // same serial, same roster — a different envelope, so a different hash.
    let (body, _, first_hash) = joinwire::receive_table_ready(
        t.seen_ready.last().expect("somebody ratified"),
        &t.table_id,
        &t.founder.genesis(),
    )
    .expect("the ratification this table already took");
    let seat_key = key(2);
    let again = joinwire::publish_table_ready(
        &body,
        &t.table_id,
        &t.founder.genesis(),
        &seat_key,
        NOW + 1,
        t.ad.join_deadline_ms,
    )
    .expect("an honest seat can seal the same body again");
    let (_, _, second_hash) = joinwire::receive_table_ready(
        &again,
        &t.table_id,
        &t.founder.genesis(),
    )
    .expect("and it verifies");
    assert_ne!(
        first_hash, second_hash,
        "the two copies must differ, or this test proves nothing"
    );

    let outcome = t.founder.on_table_ready(&again);
    assert!(
        matches!(outcome, Err(FormFailed::RatifiedTwice { .. })),
        "the second ratification was taken instead of named: {outcome:?}"
    );
    assert_eq!(
        t.founder.session(),
        Some(settled),
        "the session identity moved under a second ratification, and it is in \
         every later genesis"
    );
}


/// **A ratification that can never become valid is not kept.**
///
/// A `TABLE_READY` that does not fit the roster this client holds is held
/// rather than refused, because on a mesh the ratification and the list it
/// ratifies race and the loser is usually the list. But one naming a serial
/// this client has already passed will never fit: `admit_ready` compares
/// against the held serial and `adopt` only moves forward.
///
/// Keeping it wasted a slot in a queue of ten and pushed a genuine early
/// ratification out of it — and the bytes are a real seat's real signed event,
/// so a bystander who had merely watched an earlier round could refill the
/// queue from its own recording and hold the table up without a key of its own.
///
/// **To make this fail:** delete the `stale` guard from
/// `Formation::on_table_ready`.
#[test]
fn a_ratification_for_a_serial_already_passed_is_not_held() {
    let mut t = Table::new();
    t.add(1);
    // Every ratification broadcast so far names the serial the table had then.
    let recorded = t.seen_ready.clone();
    assert!(!recorded.is_empty(), "somebody ratified at the first serial");

    // The roster moves on, which raises the serial and clears what was held.
    t.add(2);
    let before = t.founder.held_early();

    // A bystander replays everything it heard at the earlier serial.
    for _ in 0..4 {
        for bytes in &recorded {
            let _ = t.founder.on_table_ready(bytes);
        }
    }
    assert_eq!(
        t.founder.held_early(),
        before,
        "ratifications for a serial the table has passed were kept, and the \
         queue holds ten"
    );
}
