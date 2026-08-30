//! Three peers, real cryptography, one hand played to showdown.
//!
//! Every other test in the tree exercises one layer. This one exercises the
//! **seam**: the deck is shuffled by three shufflers who each verify the others,
//! the cards are opened from real `n`-of-`n` share sets, and what comes out is
//! fed to the betting engine and the pot builder.
//!
//! The three peers are three independent [`Dealing`] states, not one shared one.
//! That is the point: a defect where a peer sees a card it should not, or where
//! two peers see different cards, is invisible to a simulation that keeps one
//! copy of the state.

use std::collections::BTreeSet;
use std::sync::Arc;

use p2p_poker::mental_poker::backend::{
    DeckParams, HandDeck, HandSecret, VerifiedKey, DECK,
};
use p2p_poker::mental_poker::deck::{CardIndex, DeckIndexMap};
use p2p_poker::mental_poker::protocol::{CtxFields, DeckCtx, ProofPosition};
use p2p_poker::mental_poker::reveal::{NotEntitled, RevealStage};
use p2p_poker::mental_poker::shuffle::{ChainParams, ShuffleChain};
use p2p_poker::poker::actions::{Action, BettingRound};
use p2p_poker::poker::engine::{first_to_act, next_to_act, post_blinds, round_complete};
use p2p_poker::poker::evaluator::evaluate_holdem;
use p2p_poker::poker::pots::{award, build_pots, total};
use p2p_poker::poker::state::{Card, Chips, SeatIdx, Street};
use p2p_poker::table::dealing::{Deal, Dealing, FinalDeck, Identity, NotOpenable, Refused, Share};

const SEATS: usize = 3;
const BUTTON: SeatIdx = 0;

fn reveal_ctx(tag: u8) -> DeckCtx {
    DeckCtx::build(&CtxFields {
        protocol_version: 1,
        table_id: [1u8; 32],
        session_id: [2u8; 32],
        hand_id: 1,
        sequence: 1_000,
        position: ProofPosition::NotAShuffleStep,
        sender_public_key: [tag; 32],
    })
}

struct Table {
    hand: HandDeck,
    keys: Vec<Option<VerifiedKey>>,
    secrets: Vec<HandSecret>,
    deck: FinalDeck,
    map: DeckIndexMap,
}

/// Set up a real table: three keys, a three-link shuffle chain, a final deck.
fn set_up() -> Table {
    let params = DeckParams::new();
    let key_ctx = reveal_ctx(0);

    let mut secrets = Vec::new();
    let mut seated: Vec<VerifiedKey> = Vec::new();
    for _ in 0..SEATS {
        let (sk, pk, proof) = params.keygen(&key_ctx);
        seated.push(
            HandDeck::verify_key(pk, &proof, &seated, &key_ctx).expect("an honest key verifies"),
        );
        secrets.push(sk);
    }

    let hand = HandDeck::new(Arc::clone(&params), &seated);

    let mut chain = ShuffleChain::open(
        ChainParams {
            protocol_version: 1,
            table_id: [1u8; 32],
            session_id: [2u8; 32],
            hand_id: 1,
        },
        (0..SEATS as u8).collect(),
        (0..SEATS as u8).map(|s| [s; 32]).collect(),
    )
    .unwrap();

    for round in 0..SEATS as u8 {
        let ctx = chain.next_ctx(round as u64).expect("the chain is open");
        let (deck, proof) = hand
            .shuffle(chain.last_verified(), &ctx)
            .expect("a peer can shuffle a deck it verified");
        chain
            .accept_step(&hand, round, deck, &proof, round as u64)
            .expect("an honest link verifies at every other seat too");
    }

    let deck = chain.finish().expect("every seat shuffled");
    let map = DeckIndexMap::build(&[true; SEATS], BUTTON, DECK).unwrap();

    Table {
        hand,
        keys: seated.into_iter().map(Some).collect(),
        secrets,
        deck,
        map,
    }
}

impl Table {
    fn deal(&self) -> Deal<'_> {
        Deal {
            hand: &self.hand,
            keys: &self.keys,
            deck: &self.deck,
        }
    }

    fn identity(&self, seat: SeatIdx) -> Identity<'_> {
        Identity {
            seat,
            key: self.keys[seat as usize].as_ref().unwrap(),
            secret: &self.secrets[seat as usize],
        }
    }
}

/// Give every peer the shares for one index, obeying the addressing rules.
///
/// Returns the seats that were sent nothing, which for a hole card is everybody
/// but its owner.
fn exchange(t: &Table, peers: &mut [Dealing], index: CardIndex, ctx: &DeckCtx) {
    let deal = t.deal();
    for from in 0..SEATS as u8 {
        // Its own share goes into its own set and onto no wire.
        let (token, proof) = peers[from as usize]
            .own_share(&deal, &t.identity(from), index, ctx)
            .expect("a peer can always compute its own share");

        // And the rest is broadcast, because every legal share is
        // (`PROTOCOL.md` §4.6). There is no addressee to compute.
        let recipients: Vec<SeatIdx> = match peers[from as usize].due(index, from) {
            Ok(()) => (0..SEATS as u8).filter(|&s| s != from).collect(),
            Err(_) => vec![],
        };

        let share = Share {
            from,
            index,
            token,
            proof: &proof,
        };
        for to in recipients {
            peers[to as usize]
                .accept(&deal, to, &share, ctx)
                .expect("an honest share, correctly addressed");
        }
    }
}

/// The whole thing.
#[test]
fn three_peers_deal_and_play_one_hand() {
    let t = set_up();
    let ctx = reveal_ctx(9);

    let mut peers: Vec<Dealing> = (0..SEATS)
        .map(|_| Dealing::new(t.map.clone(), (0..SEATS as u8).collect()))
        .collect();

    // ---- hole cards --------------------------------------------------------
    for seat in 0..SEATS as u8 {
        for index in t.map.hole_cards(seat).unwrap() {
            exchange(&t, &mut peers, index, &ctx);
        }
    }

    let mut hole = Vec::new();
    for seat in 0..SEATS as u8 {
        for index in t.map.hole_cards(seat).unwrap() {
            // The owner has every share and opens the card.
            let card = peers[seat as usize]
                .open(&t.deal(), index)
                .expect("the owner holds n of n shares");
            hole.push((seat, card));

            // Nobody else does, and the shortfall is **exactly one share** —
            // the owner's. Every other share was broadcast and every peer kept
            // it (`PROTOCOL.md` §4.6), which is what makes a showdown one
            // message. The card stays private because `n - 1` of `n` opens
            // nothing at all, which is §3.4's counting argument and the only
            // thing holding it up.
            for other in (0..SEATS as u8).filter(|&s| s != seat) {
                match peers[other as usize].open(&t.deal(), index) {
                    Err(NotOpenable::Waiting { outstanding }) => assert_eq!(
                        outstanding,
                        vec![seat],
                        "seat {other} is short exactly seat {seat}'s own share"
                    ),
                    got => panic!("seat {other} should not open another hand: {got:?}"),
                }
            }
        }
    }

    for seat in 0..SEATS as u8 {
        assert!(
            peers[seat as usize].hole_cards(seat).is_some(),
            "a seat sees its own hand"
        );
        for other in (0..SEATS as u8).filter(|&s| s != seat) {
            assert!(
                peers[seat as usize].hole_cards(other).is_none(),
                "and no other"
            );
        }
    }

    // ---- the board, one street at a time -----------------------------------
    let streets = [
        (Street::Flop, t.map.flop().to_vec()),
        (Street::Turn, vec![t.map.turn()]),
        (Street::River, vec![t.map.river()]),
    ];

    let mut board: Vec<Card> = Vec::new();
    for (street, indices) in streets {
        // Before the street, the share is not even due.
        for index in &indices {
            let (token, proof) = t
                .hand
                .token(
                    &t.secrets[0],
                    t.keys[0].as_ref().unwrap(),
                    &t.deck,
                    *index,
                    &ctx,
                )
                .unwrap();
            let share = Share {
                from: 0,
                index: *index,
                token,
                proof: &proof,
            };
            let refused = peers[1].accept(&t.deal(), 1, &share, &ctx);
            assert!(
                matches!(refused, Err(Refused::NotDue(_))),
                "a board share before its street is not due: {refused:?}"
            );
        }

        for p in peers.iter_mut() {
            assert!(p.advance(RevealStage::Betting(street)));
        }
        for index in &indices {
            exchange(&t, &mut peers, *index, &ctx);
        }

        // Every peer opens it, and every peer sees the same card.
        for index in &indices {
            let mut seen = BTreeSet::new();
            for p in peers.iter_mut() {
                seen.insert(p.open(&t.deal(), *index).expect("the board is public"));
            }
            assert_eq!(seen.len(), 1, "three peers, one card");
            board.push(*seen.iter().next().unwrap());
        }
    }

    for p in &peers {
        assert_eq!(p.board(), board, "and one board");
    }
    assert_eq!(board.len(), 5);

    // ---- a deck is a deck --------------------------------------------------
    let mut all: BTreeSet<Card> = board.iter().copied().collect();
    for (_, c) in &hole {
        all.insert(*c);
    }
    assert_eq!(
        all.len(),
        2 * SEATS + 5,
        "eleven cards were dealt and eleven distinct cards came out"
    );

    // ---- and now it is just poker ------------------------------------------
    let start: Chips = 1_000;
    let (sb, bb) = (10, 20);
    let mut round = BettingRound {
        big_blind: bb,
        current_bet: 0,
        last_full_raise: bb,
        committed: vec![0; SEATS],
        stack: vec![start; SEATS],
        acted: vec![false; SEATS],
        folded: vec![false; SEATS],
    };
    let dealt_in = [true; SEATS];
    post_blinds(&mut round, 1, 2, sb, bb);

    let mut to_act = first_to_act(
        Street::PreFlop,
        &round,
        &dealt_in,
        BUTTON,
        2,
        SEATS as u8,
    );
    let mut guard = 0;
    while let Some(seat) = to_act {
        if round_complete(&round, &dealt_in) {
            break;
        }
        guard += 1;
        assert!(guard < 50, "the betting did not terminate");
        let legal = round.legal(seat).unwrap();
        let action = if legal.can_call {
            Action::Call
        } else {
            Action::Check
        };
        round.apply(seat, action).unwrap();
        to_act = next_to_act(&round, &dealt_in, seat, SEATS as u8);
    }

    // Everybody called the big blind, so everybody is in for 20.
    assert!(round.committed.iter().all(|&c| c == bb));

    let mut rank = vec![None; SEATS];
    for seat in 0..SEATS {
        let cards = peers[seat].hole_cards(seat as u8).unwrap();
        let b: [Card; 5] = board.clone().try_into().unwrap();
        rank[seat] = Some(evaluate_holdem(cards, &b));
    }

    let (pots, refunds) = build_pots(&round.committed, &round.folded);
    assert_eq!(total(&pots, &refunds), (bb as Chips) * SEATS as Chips);

    let mut stacks = round.stack.clone();
    for r in &refunds {
        stacks[r.seat as usize] += r.amount;
    }
    let mut winners = Vec::new();
    for pot in &pots {
        for (seat, amount) in award(pot, &rank, BUTTON, SEATS as u8) {
            stacks[seat as usize] += amount;
            winners.push(seat);
        }
    }

    assert_eq!(
        stacks.iter().sum::<Chips>(),
        start * SEATS as Chips,
        "no chip was created or destroyed between the deck and the pot"
    );
    assert!(!winners.is_empty(), "somebody won the hand");

    // The winner really does hold the best hand of the three.
    let best = rank.iter().flatten().max().copied().unwrap();
    for &w in &winners {
        assert_eq!(
            rank[w as usize].unwrap(),
            best,
            "the pot went to a hand that is not the best one"
        );
    }
}

/// Every peer holds every share but the owner's, and still cannot read the
/// card. This is `PROTOCOL.md` §3.4's counting argument, executed.
///
/// The previous version of this test asserted the opposite — that a share of
/// seat 0's card reaching seat 2 was misdirected and refused. That was the
/// point-to-point model §4.6 withdrew, and holding to it would have broken the
/// showdown: `SHOWDOWN_REVEAL` is one message precisely because the other `m-1`
/// shares are already on every peer's transcript.
///
/// What is asserted instead is the property that actually protects the card:
/// `m-1` shares open nothing, and the `m`-th is the owner's and never travels.
#[test]
fn every_peer_holds_all_but_one_share_and_still_cannot_read_the_card() {
    let t = set_up();
    let ctx = reveal_ctx(9);
    let mut bystander = Dealing::new(t.map.clone(), (0..SEATS as u8).collect());

    let victim = 0u8;
    let index = t.map.hole_cards(victim).unwrap()[1];

    // Every seat but the owner hands its share to a seat that is not the owner
    // either, and every one of them is taken.
    for from in (0..SEATS as u8).filter(|&s| s != victim) {
        let (token, proof) = t
            .hand
            .token(
                &t.secrets[from as usize],
                t.keys[from as usize].as_ref().unwrap(),
                &t.deck,
                index,
                &ctx,
            )
            .unwrap();
        let share = Share {
            from,
            index,
            token,
            proof: &proof,
        };
        bystander
            .accept(&t.deal(), 2, &share, &ctx)
            .expect("every seat's share but the owner's is broadcast and kept");
    }

    // One short, and the one it is short of is the owner's.
    assert_eq!(
        bystander.outstanding(index).unwrap(),
        vec![victim],
        "exactly the owner's share is missing, and it is missing from everybody"
    );
    assert!(
        bystander.open(&t.deal(), index).is_err(),
        "m-1 of m shares must open nothing at all"
    );

    // And the owner may not publish it before showdown. That one rule is the
    // whole of hole-card privacy.
    assert_eq!(
        bystander.due(index, victim),
        Err(NotEntitled::OwnHoleCard { seat: victim })
    );
}

/// One share per seat per card. A second is a protocol violation and not an
/// update, because a set that could be overwritten would have contents that
/// depend on arrival order — and two peers would then open different cards from
/// one deck.
#[test]
fn a_seat_cannot_send_two_shares_for_one_card() {
    let t = set_up();
    let ctx = reveal_ctx(9);
    let mut me = Dealing::new(t.map.clone(), (0..SEATS as u8).collect());
    me.advance(RevealStage::Betting(Street::Flop));

    let index = t.map.flop()[0];
    let (token, proof) = t
        .hand
        .token(&t.secrets[1], t.keys[1].as_ref().unwrap(), &t.deck, index, &ctx)
        .unwrap();

    let share = Share {
        from: 1,
        index,
        token,
        proof: &proof,
    };
    me.accept(&t.deal(), 0, &share, &ctx)
        .expect("the first is fine");
    let again = me.accept(&t.deal(), 0, &share, &ctx);
    assert!(
        matches!(again, Err(Refused::NotWanted(_))),
        "the second is a violation: {again:?}"
    );
}

/// A card cannot be opened from a short set, and the shortfall is reported as
/// waiting rather than as a fault. C-10's soundness fault is reserved for a
/// **complete** verified set that fails, and confusing the two would raise a
/// non-resumable, unattributable error every time a packet was slow.
#[test]
fn a_missing_share_is_waiting_and_never_a_fault() {
    let t = set_up();
    let ctx = reveal_ctx(9);
    let mut me = Dealing::new(t.map.clone(), (0..SEATS as u8).collect());
    me.advance(RevealStage::Betting(Street::Flop));

    let index = t.map.flop()[0];
    for from in 0..(SEATS as u8 - 1) {
        let (token, proof) = t
            .hand
            .token(
                &t.secrets[from as usize],
                t.keys[from as usize].as_ref().unwrap(),
                &t.deck,
                index,
                &ctx,
            )
            .unwrap();
        let share = Share {
            from,
            index,
            token,
            proof: &proof,
        };
        me.accept(&t.deal(), 2, &share, &ctx).unwrap();
    }

    match me.open(&t.deal(), index) {
        Err(NotOpenable::Waiting { outstanding }) => {
            assert_eq!(outstanding, vec![SEATS as u8 - 1])
        }
        other => panic!("a short set must be `Waiting`, not {other:?}"),
    }
}
