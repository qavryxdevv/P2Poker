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

use p2p_poker::poker::actions::{Action, BettingRound};
use p2p_poker::poker::engine::{
    betting_is_closed, first_to_act, next_to_act, only_one_live, post_blinds, round_complete,
};
use p2p_poker::poker::evaluator::{evaluate_holdem, HandRank};
use p2p_poker::poker::pots::{award, build_pots, total};
use p2p_poker::poker::state::{Card, Chips, SeatIdx, Street};
use p2p_poker::poker::tournament::{HEADS_UP_CUSTOM_2P, RATED_SNG_POKERTH_V1};

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

    let mut round = BettingRound {
        big_blind,
        current_bet: 0,
        last_full_raise: big_blind,
        committed: vec![0; seat_count as usize],
        stack: stacks.to_vec(),
        acted: vec![false; seat_count as usize],
        folded: (0..seat_count).map(|s| !dealt_in[s as usize]).collect(),
    };
    post_blinds(&mut round, sb_seat, bb_seat, small_blind, big_blind);

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
        let mut to_act = first_to_act(street, &round, dealt_in, button, bb_seat, seat_count);
        let mut guard = 0;
        while let Some(seat) = to_act {
            // Everyone folding to one seat ends the hand at once. The survivor
            // is never offered the action - otherwise it could fold too, and
            // the pot would have nobody eligible for it.
            if only_one_live(&round, dealt_in)
                || betting_is_closed(&round, dealt_in)
                || round_complete(&round, dealt_in)
            {
                break;
            }
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

            let action = choices[rng.below(choices.len())];
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

            to_act = next_to_act(&round, dealt_in, seat, seat_count);
        }

        // --- fold-out ----------------------------------------------------
        let still_live: Vec<SeatIdx> = (0..seat_count)
            .filter(|&s| dealt_in[s as usize] && !round.folded[s as usize])
            .collect();
        // --- carry this street's commitments into the hand total ---------
        for (total, this_street) in committed_hand.iter_mut().zip(&round.committed) {
            *total += this_street;
        }
        carried += round.committed.iter().sum::<Chips>();

        if still_live.len() == 1 {
            break;
        }

        if street == Street::River || betting_is_closed(&round, dealt_in) {
            // Remaining board cards are still dealt - they decide the pots -
            // but there is no more action.
            break;
        }

        street = street.next().expect("river was handled above");
        round.current_bet = 0;
        round.last_full_raise = big_blind;
        round.committed.iter_mut().for_each(|c| *c = 0);
        round.acted.iter_mut().for_each(|a| *a = false);
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
