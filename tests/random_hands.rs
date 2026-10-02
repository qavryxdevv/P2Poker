//! Play whole hands with a plain shuffled deck, no cryptography and no network
//! (`docs/SPEC_CS.md` section 24), asserting the invariants of section 26 after
//! every action of every hand.
//!
//! This is the harness that shakes the engine out. It composes the pieces —
//! blinds, action order, legality, round completion, street advance, pot
//! construction, award — and a bug in how they fit together shows up here even
//! when each part passes its own tests.
//!
//! The randomness is a deterministic xorshift seeded from a constant, so a
//! failure reproduces exactly. It is test scaffolding and never touches a key,
//! a card commitment or anything else that matters; the crate's own
//! cryptographic randomness rule is enforced separately over `src/` by
//! `p2p_poker::security::rng`.

use p2p_poker::poker::actions::{Action, BettingRound, Illegal, LegalActions};
use p2p_poker::poker::engine::{
    betting_is_closed, next_street, opening_round, opening_turn, table_action, turn_after,
};
use p2p_poker::poker::evaluator::{evaluate_holdem, HandRank};
use p2p_poker::poker::pots::{award, build_pots, total};
use p2p_poker::poker::state::{Card, Chips, SeatIdx, Street};
use p2p_poker::poker::tournament::{HEADS_UP_CUSTOM_2P, RATED_SNG_POKERTH_V1};

thread_local! {
    /// `S1-KU`: what the engine decided, decision by decision, while a test
    /// asks for it -- the digest the engine is pinned by.
    static DECISIONS: std::cell::RefCell<Option<Vec<u8>>> = const { std::cell::RefCell::new(None) };
}

fn record(bytes: &[u8]) {
    DECISIONS.with(|d| {
        if let Some(log) = d.borrow_mut().as_mut() {
            log.extend_from_slice(bytes);
        }
    });
}

/// `S1-KT`: the engine's verdict on a fixed set of probe actions at a decision,
/// each on a copy of the round -- the hand goes on as the corpus plays it. What
/// the engine refuses, and with which minimum or maximum, is what the cheat band
/// proves a seat by, so it is pinned beside what the engine offers.
fn record_refusals(round: &BettingRound, seat: SeatIdx, legal: &LegalActions) {
    let (lo, hi, bet) = (legal.min_raise_to, legal.max_raise_to, round.current_bet);
    let probes = [
        Action::Fold,
        Action::Check,
        Action::Call,
        Action::Bet(0),
        Action::Bet(lo.saturating_sub(1)),
        Action::Bet(lo),
        Action::Bet(hi),
        Action::Bet(hi + 1),
        Action::Raise(bet),
        Action::Raise(lo.saturating_sub(1)),
        Action::Raise(lo),
        Action::Raise(hi),
        Action::Raise(hi + 1),
    ];
    for probe in probes {
        let mut copy = round.clone();
        let (code, value): (u8, Chips) = match copy.apply(seat, probe) {
            Ok(()) => (0, 0),
            Err(Illegal::NotToAct) => (1, 0),
            Err(Illegal::CheckFacingBet) => (2, 0),
            Err(Illegal::CallNothingOwed) => (3, 0),
            Err(Illegal::BetWhenBetStands) => (4, 0),
            Err(Illegal::RaiseWithNoBet) => (5, 0),
            Err(Illegal::RaiseNotReopened) => (6, 0),
            Err(Illegal::BelowMinimum { minimum }) => (7, minimum),
            Err(Illegal::AboveStack { maximum }) => (8, maximum),
        };
        record(&[code]);
        record(&value.to_le_bytes());
    }
}

/// Deterministic, reproducible, and not used for anything that must be secret.
struct Xorshift(u64);

impl Xorshift {
    fn new(seed: u64) -> Self {
        Xorshift(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
    fn shuffle<T>(&mut self, slice: &mut [T]) {
        for i in (1..slice.len()).rev() {
            slice.swap(i, self.below(i + 1));
        }
    }
}

struct HandOutcome {
    chips_before: Chips,
    chips_after: Chips,
    actions: usize,
}

/// Play one complete hand and return what happened.
///
/// Panics on any engine inconsistency, which is the point: this is a test
/// driver, and a violated invariant must stop the run at the hand that caused
/// it rather than being averaged away.
fn play_hand(
    rng: &mut Xorshift,
    stacks: &mut [Chips],
    dealt_in: &[bool],
    button: SeatIdx,
    small_blind: Chips,
    big_blind: Chips,
) -> HandOutcome {
    let seat_count = stacks.len() as u8;
    let chips_before: Chips = stacks.iter().sum();

    // Heads-up the button is the small blind; otherwise the small blind is the
    // first dealt-in seat after the button.
    let dealt: Vec<SeatIdx> = (0..seat_count).filter(|&s| dealt_in[s as usize]).collect();
    assert!(dealt.len() >= 2, "a hand needs two seats");

    let after = |from: SeatIdx| -> SeatIdx {
        (1..=seat_count)
            .map(|o| (from + o) % seat_count)
            .find(|s| dealt_in[*s as usize])
            .expect("at least one other dealt-in seat")
    };

    let heads_up = dealt.len() == 2;
    let (sb_seat, bb_seat) = if heads_up {
        (button, after(button))
    } else {
        let sb = after(button);
        (sb, after(sb))
    };

    let mut deck: Vec<Card> = (0..52).map(|i| Card::from_index(i).unwrap()).collect();
    rng.shuffle(&mut deck);

    let mut hole: Vec<Option<[Card; 2]>> = vec![None; seat_count as usize];
    let mut next_card = 0usize;
    for &seat in &dealt {
        hole[seat as usize] = Some([deck[next_card], deck[next_card + 1]]);
        next_card += 2;
    }

    // `S1-KT`: the hand's own opening round, which the digest pins.
    let mut round = opening_round(stacks.to_vec(), dealt_in, sb_seat, bb_seat, small_blind, big_blind);

    // Total commitment across every street, which is what pots are built from.
    // Filled in at the end of each street, including pre-flop, so the blinds
    // are counted exactly once.
    let mut committed_hand: Vec<Chips> = vec![0; seat_count as usize];
    // What earlier streets already moved out of `round`.
    let mut carried: Chips = 0;
    let mut actions = 0usize;
    let mut street = Street::PreFlop;

    loop {
        // --- betting on this street -------------------------------------
        // `S1-KT`: the hand's own two rules (`Hand::open_betting`,
        // `Hand::after_action`), so the digest pins what the hand does. A street
        // nobody can bet on is never opened, and everyone folding to one seat
        // ends the street at once: the survivor is never offered the action -
        // otherwise it could fold too, and the pot would have nobody eligible.
        let mut to_act = opening_turn(street, &round, dealt_in, button, sb_seat, bb_seat, seat_count);
        let mut guard = 0;
        while let Some(seat) = to_act {
            guard += 1;
            assert!(guard < 400, "betting on {street:?} did not terminate");

            let legal = round.legal(seat).expect("the seat offered the action can act");
            let mut choices: Vec<Action> = vec![Action::Fold];
            if legal.can_check {
                choices.push(Action::Check);
            }
            if legal.can_call {
                choices.push(Action::Call);
            }
            if legal.can_bet || legal.can_raise {
                let kind = if legal.can_bet { Action::Bet } else { Action::Raise };
                let lo = legal.min_raise_to.min(legal.max_raise_to);
                let hi = legal.max_raise_to;
                choices.push(kind(lo));
                choices.push(kind(hi));
                if hi > lo {
                    choices.push(kind(lo + (rng.below((hi - lo) as usize + 1)) as Chips));
                }
            }

            // Now and then the seat's clock runs out and the table acts for it --
            // `S1-KT`: the hand's own `table_action`, pinned with the rest.
            let action = if rng.below(10) == 0 {
                table_action(&round, seat)
            } else {
                choices[rng.below(choices.len())]
            };
            // What the engine refuses here, pinned as well: the cheat band
            // proves a seat by a refusal (`S1-KT`).
            record_refusals(&round, seat, &legal);
            // Who was offered the turn, what the engine offered, what was taken.
            let (tag, amount) = match action {
                Action::Fold => (0u8, 0),
                Action::Check => (1, 0),
                Action::Call => (2, 0),
                Action::Bet(x) => (3, x),
                Action::Raise(x) => (4, x),
            };
            record(&[
                street as u8,
                seat,
                u8::from(legal.can_fold),
                u8::from(legal.can_check),
                u8::from(legal.can_call),
                u8::from(legal.can_bet),
                u8::from(legal.can_raise),
                tag,
            ]);
            record(&legal.min_raise_to.to_le_bytes());
            record(&legal.max_raise_to.to_le_bytes());
            record(&amount.to_le_bytes());
            round.apply(seat, action).unwrap_or_else(|e| {
                panic!("engine offered {action:?} to seat {seat} and then rejected it: {e:?}")
            });
            actions += 1;

            // Chips must be conserved by every single action. Everything is
            // either still behind, committed on this street, or carried over
            // from an earlier one.
            let live: Chips = round.stack.iter().sum::<Chips>()
                + round.committed.iter().sum::<Chips>()
                + carried;
            assert_eq!(live, chips_before, "chips moved on {action:?} by seat {seat}");

            to_act = turn_after(&round, dealt_in, seat, seat_count);
        }

        // --- fold-out ----------------------------------------------------
        let still_live: Vec<SeatIdx> = (0..seat_count)
            .filter(|&s| dealt_in[s as usize] && !round.folded[s as usize])
            .collect();
        // --- carry this street's commitments into the hand total ---------
        // `S1-KT`: by the hand's own rule, which opens the next street too.
        for (total, this_street) in committed_hand.iter_mut().zip(next_street(&mut round)) {
            *total += this_street;
            carried += this_street;
        }

        if still_live.len() == 1 {
            break;
        }

        if street == Street::River || betting_is_closed(&round, dealt_in) {
            // Remaining board cards are still dealt - they decide the pots -
            // but there is no more action.
            break;
        }

        street = street.next().expect("river was handled above");
    }

    // --- showdown ------------------------------------------------------
    let board: [Card; 5] = [
        deck[next_card],
        deck[next_card + 1],
        deck[next_card + 2],
        deck[next_card + 3],
        deck[next_card + 4],
    ];

    let mut rank: Vec<Option<HandRank>> = vec![None; seat_count as usize];
    for s in 0..seat_count as usize {
        if dealt_in[s] && !round.folded[s] {
            rank[s] = hole[s].map(|h| evaluate_holdem(h, &board));
        }
    }

    let (pots, refunds) = build_pots(&committed_hand, &round.folded);
    assert_eq!(
        total(&pots, &refunds),
        committed_hand.iter().sum::<Chips>(),
        "pots plus refunds must equal what was committed"
    );

    // Everything committed left the stacks; hand back what is won.
    stacks.copy_from_slice(&round.stack);
    for r in &refunds {
        stacks[r.seat as usize] += r.amount;
    }
    for pot in &pots {
        assert!(
            !pot.eligible.is_empty(),
            "a pot with no eligible seat is not legally reachable: pot={pot:?} \n             committed={committed_hand:?} folded={:?} dealt_in={dealt_in:?}",
            round.folded
        );
        for (seat, amount) in award(pot, &rank, button, seat_count) {
            assert!(
                !round.folded[seat as usize],
                "a folded seat won pot {pot:?}"
            );
            stacks[seat as usize] += amount;
        }
    }

    for s in stacks.iter() {
        record(&s.to_le_bytes());
    }
    HandOutcome {
        chips_before,
        chips_after: stacks.iter().sum(),
        actions,
    }
}

fn run_table(seed: u64, hands: usize, seats: usize, start_stack: Chips) -> usize {
    let mut rng = Xorshift::new(seed);
    let mut stacks = vec![start_stack; seats];
    let table_total: Chips = stacks.iter().sum();
    let mut button: SeatIdx = 0;
    let mut played = 0;
    let mut total_actions = 0usize;

    for hand in 1..=hands as u32 {
        let dealt_in: Vec<bool> = stacks.iter().map(|&s| s > 0).collect();
        if dealt_in.iter().filter(|&&d| d).count() < 2 {
            break; // somebody has every chip
        }
        // Move the button to a seat that is still in.
        while !dealt_in[button as usize] {
            button = (button + 1) % seats as u8;
        }

        let preset = if seats == 2 {
            HEADS_UP_CUSTOM_2P
        } else {
            RATED_SNG_POKERTH_V1
        };
        let sb = preset.small_blind(hand);
        let bb = preset.big_blind(hand);

        let out = play_hand(&mut rng, &mut stacks, &dealt_in, button, sb, bb);
        played += 1;

        assert_eq!(
            out.chips_after, out.chips_before,
            "hand {hand} created or destroyed chips (seed {seed})"
        );
        assert_eq!(
            stacks.iter().sum::<Chips>(),
            table_total,
            "the table total drifted at hand {hand} (seed {seed})"
        );
        // A hand with no voluntary action at all is legitimate: if the blinds
        // put every dealt-in seat all-in, the board decides it and nobody ever
        // gets a decision. So actions are counted for the run, not asserted per
        // hand.
        total_actions += out.actions;

        button = (button + 1) % seats as u8;
    }

    assert!(
        played == 0 || total_actions > 0,
        "a whole table ran with no voluntary action (seed {seed})"
    );
    played
}

#[test]
fn heads_up_hands_conserve_chips() {
    let mut played = 0;
    for seed in 1..=4_000u64 {
        played += run_table(seed, 300, 2, 10_000);
    }
    println!("hands played: {played}");
    assert!(played > 10_000, "only {played} hands played; the driver stalled");
}

#[test]
fn six_handed_hands_conserve_chips() {
    let mut played = 0;
    for seed in 100_001..=104_000u64 {
        played += run_table(seed, 300, 6, 10_000);
    }
    println!("hands played: {played}");
    assert!(played > 10_000, "only {played} hands played; the driver stalled");
}

#[test]
fn ten_handed_hands_conserve_chips() {
    let mut played = 0;
    for seed in 200_001..=203_000u64 {
        played += run_table(seed, 300, 10, 10_000);
    }
    println!("hands played: {played}");
    assert!(played > 10_000, "only {played} hands played; the driver stalled");
}

/// Very short stacks force all-ins, side pots and blind-sized decisions - the
/// arithmetic that a comfortable table never reaches.
#[test]
fn tiny_stacks_exercise_the_all_in_paths() {
    let mut played = 0;
    for seed in 300_001..=302_000u64 {
        for seats in [2usize, 3, 5] {
            played += run_table(seed, 60, seats, 260);
        }
    }
    println!("hands played: {played}");
    assert!(played > 10_000, "only {played} hands played; the driver stalled");
}

/// `S1-KT`: every engine digest pinned, by the protocol major it was pinned
/// for -- one per major. A change to what the engine decides is a rule change,
/// and a rule change is a new major (`D-089`): two builds of one major never
/// decide differently, so no seat is judged by a round another build would not
/// make. Edited in place only while its major is unreleased (3: 0.3.0 is not).
const PINNED: &[(u16, &str)] = &[(3, "83021f6e91984aaa1b796cf43427bd5c83bf49b10616b829ac0539f381bc71e8")];

/// `S1-KT`: the functions of `src/table/hand.rs` a betting stage's round and
/// its judgement stand on -- the round built at the deal, the turns, the
/// street's close, a certificate's action, the record of a stage and the judge
/// -- read into `ENGINE_SOURCE` beside the engine's own code. Methods end at
/// their closing brace at four spaces, free functions at the margin.
const HAND_FUNCTIONS: &[&str] = &[
    "    fn read_my_cards(",
    "    fn open_betting(",
    "    fn after_action(",
    "    fn close_round_and_open(",
    "    fn apply_certificate(",
    "    fn note_betting_stage(",
    "    fn judge_action(",
    "    fn judge_opened_action(",
    "fn is_betting_action(",
    "fn claimed_action(",
    "fn action_of(",
    // `G9`: the money path -- what a settlement is derived from, and its judge.
    "    fn settlement(",
    "    fn record_showdown(",
    "    fn close_showdown_if_done(",
    "    fn on_showdown(",
    "    fn show(&mut self",
    "    fn muck(&mut self",
    "    fn muck_allowed(",
    "    fn shows_rather_than_mucks(",
    "    fn hold_until(",
    "    fn speak_at_showdown(",
    "    pub fn show_held(",
    "    fn start_stacks_by_seat(",
    "fn five_card_board(",
    "    fn on_hand_complete(",
    "    fn note_settled_money(",
    "    fn judge_money(",
    "    fn judge_left_settlement(",
    // and what feeds the judge: where the reference is derived or carried, the
    // late road's borrowed body, the routing of a frame at a stage left, the
    // evidence's dispatch and the proof's gate (the refuter of G9 v2).
    "    fn begin_settlement(",
    "    fn settle_mine(",
    "    fn give_up(",
    "    fn on_late_settlement(",
    "    fn judge_left(",
    "    fn judge_evidence(",
    "    fn note_proven(",
    // the live writers of the round a settlement reads (the diff's review).
    "    pub fn act(",
    "    fn apply_action(",
    // `G8`: the band's other judges -- a card share, a deck key, a shuffle
    // frame's bytes and its argument -- the shuffle's live check, the memo they
    // share, and the abort gates that take their evidence.
    "    fn judge_reveal(",
    "    fn judge_key(",
    "    fn judge_shuffle_bytes(",
    "    fn judge_shuffle_argument(",
    "    fn shuffle_context(",
    "    fn on_shuffle_proof(",
    "    fn shuffle_if_mine(",
    "    fn bad_reveal_holds(",
    "    fn bad_shuffle_holds(",
    "fn evidence_memo(",
    // and (the v3 refuter) what the judges and the gate read: the deck's hash,
    // its decoding, the shuffle's evidence, the gate and the abort it serves.
    "fn input_deck_hash(",
    "fn deck_hash(",
    "fn unflatten(",
    "    fn judge_shuffle_evidence(",
    "    fn take_shuffle_abort(",
    "    fn on_hand_abort(",
];

/// `G9`: the money path outside `hand.rs`, by file and header -- a function's
/// end found as for `HAND_FUNCTIONS`.
const MONEY_FUNCTIONS: &[(&str, &str, &str)] = &[
    ("dealing.rs", include_str!("../src/table/dealing.rs"), "    pub fn board(&self) -> Vec<Card> {"),
    ("deck.rs", include_str!("../src/mental_poker/deck.rs"), "    pub fn hole_cards(&self, seat: SeatIdx) -> Option<[CardIndex; 2]> {"),
    ("handwire.rs", include_str!("../src/table/handwire.rs"), "pub struct HandComplete {"),
    ("handwire.rs", include_str!("../src/table/handwire.rs"), "pub struct PotAward {"),
    ("handwire.rs", include_str!("../src/table/handwire.rs"), "pub struct Refund {"),
    // `G8`: the shuffle's frames, whose bodies the band's judges decode.
    ("handwire.rs", include_str!("../src/table/handwire.rs"), "pub struct ShuffleStep {"),
    ("handwire.rs", include_str!("../src/table/handwire.rs"), "pub struct ShuffleProof {"),
    ("handwire.rs", include_str!("../src/table/handwire.rs"), "    pub fn money_bytes(&self) -> Vec<u8> {"),
];

/// `G8`: the lock's blocks of the crates whose code a shuffle verification
/// runs or is generated by -- vendor/ziffle's dependency closure in
/// `Cargo.lock`, walked by name and version (ziffle's Fiat-Shamir is sha2 0.10,
/// not the app's 0.11 -- the v3 refuter), less `UNPINNED` -- by name and
/// version. Any crate a `cargo update` adds to the closure is pinned until it
/// is named here.
fn ziffle_closure() -> Vec<&'static str> {
    // Build-time tools, platform and entropy crates, memory, collection and
    // async utilities: none computes a value the verifier reads, and pinned
    // they moved `ENGINE_SOURCE` on a routine update (the diff's refuter).
    // `zerocopy` with them: the generator it serves is pinned here
    // (`rand_chacha`, `ppv-lite86`) and by `deck_constants`' digests.
    const UNPINNED: &[&str] = &[
        "proc-macro2", "quote", "syn", "unicode-ident", "autocfg", "version_check", "rustversion", "paste", "libc",
        "cpufeatures", "cfg-if", "getrandom", "wasi", "wasip2", "wasm-bindgen", "wasm-bindgen-macro",
        "wasm-bindgen-macro-support", "wasm-bindgen-shared", "js-sys", "r-efi", "wit-bindgen", "critical-section",
        "portable-atomic", "bumpalo", "zeroize", "zeroize_derive", "hashbrown", "ahash", "allocator-api2", "once_cell",
        "fnv", "slab", "memchr", "arrayvec", "either", "libm", "zerocopy", "zerocopy-derive", "futures-channel",
        "futures-core", "futures-io", "futures-macro", "futures-sink", "futures-task", "futures-util",
        "pin-project-lite",
    ];
    let lock = include_str!("../Cargo.lock");
    let field = |block: &'static str, key: &str| -> &'static str {
        block.lines().find_map(|l| l.strip_prefix(key)).map_or("", |r| r.trim_matches('"'))
    };
    let blocks: Vec<&'static str> = lock.split("[[package]]").skip(1).collect();
    let resolve = |dep: &str| -> &'static str {
        let mut words = dep.split(' ');
        let (name, version) = (words.next().unwrap_or(""), words.next());
        let found: Vec<&'static str> = blocks
            .iter()
            .copied()
            .filter(|b| field(b, "name = ") == name && version.is_none_or(|v| field(b, "version = ") == v))
            .collect();
        assert_eq!(found.len(), 1, "Cargo.lock: {dep}");
        found[0]
    };
    let mut seen: std::collections::BTreeMap<(&str, &str), &'static str> = std::collections::BTreeMap::new();
    let mut stack = vec![resolve("ziffle")];
    while let Some(block) = stack.pop() {
        if seen.insert((field(block, "name = "), field(block, "version = ")), block).is_some() {
            continue;
        }
        if let Some((_, rest)) = block.split_once("dependencies = [") {
            let list = rest.split(']').next().unwrap_or("");
            for dep in list.lines().map(|l| l.trim().trim_end_matches(',').trim_matches('"')).filter(|l| !l.is_empty()) {
                stack.push(resolve(dep));
            }
        }
    }
    seen.into_iter().filter(|((name, _), _)| !UNPINNED.contains(name)).map(|(_, b)| b).collect()
}

/// The code `ENGINE_SOURCE` is the digest of: comments and whitespace dropped,
/// carriage returns with them, so a checkout's line endings do not move it.
fn engine_source() -> Vec<u8> {
    fn strip(code: &str, out: &mut Vec<u8>) {
        for line in code.lines() {
            let line = line.split("//").next().unwrap_or("");
            out.extend(line.bytes().filter(|b| !b.is_ascii_whitespace()));
        }
    }
    let mut out = Vec::new();
    for (name, text, must) in [
        ("engine.rs", include_str!("../src/poker/engine.rs"), "pub fn first_to_act("),
        ("actions.rs", include_str!("../src/poker/actions.rs"), "pub fn apply("),
        // `G9`: the pots and the ranking every settlement is derived by.
        ("pots.rs", include_str!("../src/poker/pots.rs"), "pub fn build_pots("),
        ("evaluator.rs", include_str!("../src/poker/evaluator.rs"), "pub fn evaluate_holdem("),
        // `G8`: the deck's code -- the chain, the context, the verifier's checks
        // and its wire -- by which a shuffle, a key and a share are judged.
        ("shuffle.rs", include_str!("../src/mental_poker/shuffle.rs"), "pub fn judge_step<"),
        ("protocol.rs", include_str!("../src/mental_poker/protocol.rs"), "fn verify_shuffle("),
        ("backend.rs", include_str!("../src/mental_poker/backend.rs"), "macro_rules! wire"),
    ] {
        let code = &text[..text.find("\n#[cfg(test)]\nmod tests").unwrap_or_else(|| panic!("{name}: its tests"))];
        assert!(code.contains(must), "{name}: the tripwire reads the code it claims to");
        strip(code, &mut out);
    }
    let hand = include_str!("../src/table/hand.rs");
    for header in HAND_FUNCTIONS {
        let at = hand.find(header).unwrap_or_else(|| panic!("hand.rs: {header}"));
        let end = if header.starts_with(' ') { "\n    }\n" } else { "\n}\n" };
        let body = &hand[at..at + hand[at..].find(end).unwrap_or_else(|| panic!("hand.rs: the end of {header}"))];
        strip(body, &mut out);
    }
    for (name, text, header) in MONEY_FUNCTIONS {
        let at = text.find(header).unwrap_or_else(|| panic!("{name}: {header}"));
        let end = if header.starts_with(' ') { "\n    }\n" } else { "\n}\n" };
        let body = &text[at..at + text[at..].find(end).unwrap_or_else(|| panic!("{name}: the end of {header}"))];
        strip(body, &mut out);
    }
    // `G9`: and the ranking crate's version, which the facade above leans on.
    let cargo = include_str!("../Cargo.toml");
    let line = cargo.lines().find(|l| l.starts_with("rs_poker = ")).expect("Cargo.toml: rs_poker");
    strip(line, &mut out);
    // `G8`: the shuffle argument's verifier, whole -- vendored, and pinned by
    // nothing else (`deck_constants` pins its constants alone) -- and the
    // versions of the crates it and the deck's wire stand on.
    let ziffle = include_str!("../vendor/ziffle/src/lib.rs");
    assert!(ziffle.contains("pub fn verify"), "ziffle: the tripwire reads the verifier");
    strip(ziffle, &mut out);
    let mut crates = 0;
    for line in cargo.lines().filter(|l| l.starts_with("ziffle = ") || l.starts_with("ark-")) {
        strip(line, &mut out);
        crates += 1;
    }
    assert!(crates >= 2, "Cargo.toml: the deck's crates");
    // The verifier's own manifest -- its crates are caret ranges there -- and
    // what the lock fixes for them (`ziffle_closure`).
    strip(include_str!("../vendor/ziffle/Cargo.toml"), &mut out);
    let pinned = ziffle_closure();
    for block in &pinned {
        for l in block.lines().filter(|l| l.starts_with("name = ") || l.starts_with("version = ") || l.starts_with("checksum = ")) {
            strip(l, &mut out);
        }
    }
    for must in ["ziffle", "ark-ff", "ark-ec", "ark-serialize", "sha2", "digest", "rand_chacha", "num-bigint"] {
        let line = format!("name = \"{must}\"");
        assert!(pinned.iter().any(|b| b.lines().any(|l| l == line)), "Cargo.lock: the verifier's {must}");
    }
    out
}

/// `S1-KU`: **the engine is the protocol major's.** Two clients of one major
/// must decide every turn, every legal set and every pot alike: the cheat band
/// judges a betting action against the engine (`S1-KT`), and two engines that
/// differ would prove an honest seat a cheat. So every decision over a seeded
/// corpus -- heads-up, three-, six- and ten-handed, deep and tiny stacks; whose
/// turn by the hand's own two rules, what the engine offered and what it refuses
/// -- is pinned by its digest, `ENGINE_DIGEST`, and a change to what the engine
/// decides there fails here until the constant is set to the new digest.
/// `S1-KT`: every `HAND_INIT` carries `ENGINE_DIGEST`, so two clients whose
/// engines decide differently never complete a hand's stage 0 together -- a
/// split, never a framing. What the corpus does not reach it does not pin: this
/// driver lays out its own positions and deals in every seat with chips, and the
/// positions, the dealt-in set and the stacks are `HAND_INIT`'s own fields,
/// compared one by one. Written out, not recomputed: a test that recomputes what
/// it checks passes whatever the code does.
#[test]
fn the_engine_is_the_protocol_majors() {
    DECISIONS.with(|d| *d.borrow_mut() = Some(Vec::new()));
    // 250 tables of each shape, each played to its end or 300 hands. Three-handed
    // is where a hand folds down to two most often (`S1-KU`).
    let mut played = 0;
    for (first, seats, stack) in [
        (0x5eed_0000u64, 2usize, 10_000),
        (0x5eed_1000, 3, 10_000),
        (0x5eed_2000, 6, 10_000),
        (0x5eed_3000, 10, 10_000),
        (0x5eed_4000, 6, 300),
    ] {
        for seed in first..first + 250 {
            played += run_table(seed, 300, seats, stack);
        }
    }
    let log = DECISIONS.with(|d| d.borrow_mut().take()).expect("recorded");
    assert!(played > 2_000, "only {played} hands: the corpus pins too little");
    let digest = blake3::hash(&log);
    let major = p2p_poker::protocol::constants::PROTOCOL_MAJOR;
    let majors: std::collections::BTreeSet<u16> = PINNED.iter().map(|(m, _)| *m).collect();
    assert_eq!(majors.len(), PINNED.len(), "one digest per major: a new digest is a new major (D-089)");
    let pinned = PINNED
        .iter()
        .find(|(m, _)| *m == major)
        .map(|(_, d)| *d)
        .unwrap_or_else(|| panic!("no digest pinned for protocol major {major}: pin {}", digest.to_hex()));
    assert_eq!(
        digest.to_hex().as_str(),
        pinned,
        "the engine decided something else over the pinned corpus ({} bytes of decisions): a rule change, and a rule change is a new protocol major (D-089), its digest pinned beside the old one",
        log.len()
    );
    assert_eq!(
        *digest.as_bytes(),
        p2p_poker::protocol::constants::ENGINE_DIGEST,
        "ENGINE_DIGEST, which every HAND_INIT carries, is this major's pinned digest"
    );
    let source = blake3::hash(&engine_source());
    assert_eq!(
        *source.as_bytes(),
        p2p_poker::protocol::constants::ENGINE_SOURCE,
        "the code the betting engine and the hand's judge decide by changed ({}; src/poker/engine.rs, actions.rs, pots.rs or evaluator.rs, src/mental_poker/shuffle.rs, protocol.rs or backend.rs before their tests, HAND_FUNCTIONS of hand.rs, MONEY_FUNCTIONS, vendor/ziffle's lib.rs or manifest, the rs_poker, ziffle, ark-* or sha2 lines of Cargo.toml, or the deck's crates in Cargo.lock): if anything it decides changed, that is a new protocol major (D-089) -- the corpus may not reach it; if nothing did, pin ENGINE_SOURCE again. Either way HAND_INIT's engine moves, and builds of the two never share a hand",
        source.to_hex()
    );
}
