//! What the table window draws, built from the snapshot.
//!
//! `S1-CS`. This used to live in the binary, beside the window, where nothing
//! could test it -- and the window showed a stack frozen at the buy-in, no bet
//! in front of anybody, a pot only on the hero's own turn, and the last game's
//! cards after the player had left the table. Every claim the window makes is
//! made here, from [`AppState`], and every one of them has a test.
//!
//! Nothing here decides anything about the game: the engine's state reaches
//! this file through the node's events, and the view is a reading of it.
//! `SPEC_CS.md` §22's rule stands as before -- a card is face-up only through
//! [`Facing::up`] with a verdict, and the verdict is that the card was opened
//! here from a complete set of verified shares.

use crate::gui::table::{Facing, Link, SeatView, TableChatLine, TableView, Win};
use crate::poker::strength::{category_name, Horizon};

use super::{AppState, HandInProgress};

/// `D-052`: what this seat won of a hand that has ended, or `None`.
///
/// Read from the settlement's own pots -- the first is the main pot, each one
/// after it a side pot, and a pot with more than one winner was split. A hand
/// that ended with no pots named (an abort restores every stack, and a window
/// that heard only the end has nothing) falls back to the chips that moved,
/// which is what the badge said before there were pots to name.
fn win_at(h: &HandInProgress, seat: u8) -> Option<Win> {
    if !h.over {
        return None;
    }
    let mut win = Win {
        one_pot: h.pots.len() <= 1,
        ..Win::default()
    };
    for (i, pot) in h.pots.iter().enumerate() {
        if !pot.winners.contains(&seat) {
            continue;
        }
        if i == 0 {
            win.main = true;
        } else {
            win.side = true;
        }
        win.split |= pot.winners.len() > 1;
    }
    if win.main || win.side {
        return Some(win);
    }
    // No pot names this seat: it won only if chips moved to it.
    let moved = h.won.get(usize::from(seat)).copied().unwrap_or(0) > 0;
    moved.then_some(Win { one_pot: true, ..Win::default() })
}

impl AppState {
    /// The table, as the window should draw it now.
    ///
    /// The **real** roster while this client has a seat, so a player watches
    /// the others arrive; the sample only when there is no table at all and the
    /// player asked for a look.
    pub fn table_view(&self) -> TableView {
        let Some(seat) = self.seated.as_ref() else {
            return TableView::sample();
        };

        // From the node, which knows them, and not from this client's own
        // lobby -- a founder's table is not in its own lobby until the network
        // has taken the advertisement, and a table window showing invented
        // defaults for those thirty seconds is worse than one showing nothing.
        let name = if seat.name.is_empty() {
            format!("table {}", crate::gui::lobby::short_key(&seat.key))
        } else {
            seat.name.clone()
        };
        let blinds = format!("{} / {}", seat.small_blind, seat.big_blind);
        let max_seats = seat.seats.max(1);
        let needed = seat.needed;
        let hand = self.hand.as_ref();
        let hero = seat.seat;

        let seats = seat
            .roster
            .iter()
            .map(|(n, who, buyin)| {
                let i = usize::from(*n);
                SeatView {
                    seat: *n,
                    name: who.clone(),
                    stack: self.stack_of(*n, *buyin),
                    bet: hand.and_then(|h| h.bets.get(i).copied()).unwrap_or(0),
                    cards: hole_cards(hand, *n, hero),
                    folded: hand.and_then(|h| h.folded.get(i).copied()).unwrap_or(false),
                    // `S1-DG`: a roster seat the running hand does not deal in
                    // sits it out -- certified out, or busted -- and the felt
                    // says so rather than drawing it like any other seat.
                    sitting_out: hand.is_some_and(|h| !h.dealt_in.contains(n)),
                    clock: (self.turn_seat == Some(*n) && !hand.is_some_and(|h| h.over)).then(|| {
                        clock_fraction(
                            self.turn_since.map(|t| t.elapsed().as_millis() as u64).unwrap_or(0),
                            seat.action_ms,
                        )
                    }),
                    won: hand.and_then(|h| h.won.get(i).copied()).unwrap_or(0),
                    // `D-052`: and which pot it was, for the word that blinks.
                    win: hand.and_then(|h| win_at(h, *n)),
                    muted: self.muted.contains(n),
                    left: self.gone.contains(n),
                    // `D-058`: on its way back, said at its seat and not over the hand.
                    coming_back: self.waits.get(n).is_some_and(|w| w.returning()),
                    link: self.links.get(n).map(|(rtt, group, quiet, at)| Link {
                        rtt_ms: *rtt,
                        stale: at.elapsed().as_millis() as u64 > LINK_STALE_MS,
                        group: *group,
                        quiet_s: *quiet,
                        // `D-049`: sits out by the group's word, and only while
                        // the group holds the seat.
                        away: *group && self.away.contains(n),
                    }),
                    shown_hand: shown_name(hand, *n, hero),
                    act: hand.and_then(|h| h.acted.get(i).copied().flatten()),
                    key: seat.keys.get(n).copied(),
                    rating: seat.keys.get(n).map(|k| self.notes.about(k).0).unwrap_or(0),
                    note: seat.keys.get(n).map(|k| self.notes.about(k).1.to_owned()).unwrap_or_default(),
                }
            })
            .collect::<Vec<_>>();

        let seated = seats.len();
        let turn = hand.and_then(|h| h.turn);
        let (min_raise, max_raise) = turn
            .filter(|t| t.can_bet || t.can_raise)
            .map(|t| (t.min_raise_to, t.max_raise_to))
            .unwrap_or((0, 0));
        // The engine's pot after every action, and the turn's own figure
        // while the first `TableState` of a hand has not arrived.
        let pot = match hand.map(|h| h.pot) {
            Some(p) if p > 0 => p,
            _ => turn.map(|t| t.pot).unwrap_or(0),
        };
        let (hero_hand, improve, improve_total, improve_by) = match &self.strength {
            Some(s) => (
                Some(s.name.clone()),
                s.improve
                    .iter()
                    .map(|(c, p)| (category_name(*c).to_string(), *p))
                    .collect(),
                s.improve_total,
                s.horizon.map(|h| match h {
                    Horizon::ByTheFlop => "by the flop",
                    Horizon::ByTheRiver => "by the river",
                }),
            ),
            None => (None, Vec::new(), 0.0, None),
        };

        TableView {
            name,
            blinds,
            hand: hand.map(|h| h.hand_id).unwrap_or(0),
            street: street_label(hand, seat.session.is_some()),
            pot,
            board: board_cards(hand),
            seats,
            hero: hero.unwrap_or(0),
            button: hand.map(|h| h.button).unwrap_or(0),
            to_act: hand.and_then(|h| if h.turn.is_some() { hero } else { h.waiting_on }),
            max_seats,
            hero_hand,
            can_act: turn.is_some(),
            to_call: turn.map(|t| t.to_call).unwrap_or(0),
            min_raise,
            max_raise,
            preview: false,
            note: Some(note(
                hand,
                seat.session.is_some(),
                &self.waiting_for,
                seated,
                needed,
                // The carrier's own `want` grows as it invites; the roster's
                // other seats are what the player counts, so the larger.
                seat.heard.zip(seat.group_want).map(|(seen, want)| {
                    (seen, want.max(u16::try_from(seat.roster.len().saturating_sub(1)).unwrap_or(u16::MAX)))
                }),
            )),
            improve,
            improve_total,
            improve_by,
            turn_id: self.turns,
            hand_over: hand.map(|h| h.over).unwrap_or(false),
            // `S1-EL`: while this client's own line is gone by the library's
            // verdict, the question about the others is not asked -- the cause is
            // this line, and *Line down* says so. The episode runs on underneath,
            // so the question comes the moment the line is back and they are not.
            opponent_gone_s: if self.tox_line_gone() { None } else { self.opponent_gone_for_s() },
            opponent_out: self.opponent_out,
            opponent_slow: self.opponent_gone.as_ref().is_some_and(|g| g.slow),
            opponent_left: self.opponent_left,
            out_for_good: self.out_for_good.clone(),
            out_flooded: self.out_flooded,
            lost: self.lost.clone(),
            unsafe_note: self.unsafe_note.clone(),
            // `S1-IX`: the seats by the names the window shows.
            stopped: self.stopped.as_ref().map(|(s, serial)| crate::gui::table::StoppedView {
                hand_id: s.hand_id,
                seats: s.seats.iter().map(|x| self.seat_name(*x)).collect(),
                ended: s.ended.clone(),
                serial: *serial,
            }),
            // `S1-EL`: and the group's softer *the line may be down* yields to the
            // question when that stands, which says the same with the choice.
            line: match self.line_message() {
                Some(_) if !self.tox_line_gone() && self.opponent_gone_for_s().is_some() => None,
                other => other,
            },
            absent: self.absent_seats(),
            rejoin: self.rejoin_view(),
            waits: self.wait_views(),
            opponent_alone: self.opponent_gone.as_ref().is_some_and(|g| g.alone),
            chat: self
                .table_chat
                .iter()
                .filter(|l| !self.muted.contains(&l.seat))
                .map(|l| TableChatLine {
                    seat: l.seat,
                    who: l.who.clone(),
                    said: l.said.clone(),
                })
                .collect(),
            game_no: seat.game_no,
            log: self.table_log.iter().cloned().collect(),
            winning_hand: winning_hand(hand, hero, self.strength.as_ref()),
            hero_sitting_out: self.sitting_out,
            // `D-050`: the time left to show the waiting hand.
            show_cards_in_ms: self
                .show_choice
                .filter(|(id, _)| hand.is_some_and(|h| h.hand_id == *id))
                .map(|(_, until)| u64::try_from(until.saturating_duration_since(std::time::Instant::now()).as_millis()).unwrap_or(u64::MAX)),
            // The window about it waits: the deciding hand is looked at first.
            finished: self.finished.map(|(f, at)| crate::gui::table::Finish {
                show_in_ms: f.show_in_ms.saturating_sub(u64::try_from(at.elapsed().as_millis()).unwrap_or(u64::MAX)),
                ..f
            }),
        }
    }

    /// What a seat has behind: the settlement's figure once the hand is over,
    /// the engine's after every action while it runs, the last settlement's
    /// between hands, and the buy-in before the first hand.
    fn stack_of(&self, seat: u8, buyin: u64) -> u64 {
        let i = usize::from(seat);
        if let Some(h) = self.hand.as_ref() {
            if h.over {
                if let Some(s) = h.stacks.get(i) {
                    return *s;
                }
            }
            if let Some(s) = h.stacks_now.get(i) {
                return *s;
            }
        }
        self.last_stacks.get(i).copied().unwrap_or(buyin)
    }
}

/// PokerTH's winning hand under the board: the hand of the seat that took the
/// most at the settlement, in words -- the hero's own reading when the hero
/// won, the shown cards' name for anybody else -- and nothing when the pot
/// went without a showdown.
fn winning_hand(
    hand: Option<&HandInProgress>,
    hero: Option<u8>,
    strength: Option<&crate::poker::strength::Strength>,
) -> Option<String> {
    let h = hand.filter(|h| h.over)?;
    let (seat, _) = h.won.iter().enumerate().filter(|(_, w)| **w > 0).max_by_key(|(_, w)| **w)?;
    let seat = seat as u8;
    h.shown.get(usize::from(seat)).copied().flatten()?;
    if hero == Some(seat) {
        return strength.map(|s| s.name.clone());
    }
    shown_name(Some(h), seat, hero)
}

/// A link reading older than this is shown as stale: two ping intervals
/// and a bit, so one missed ping is not a verdict.
pub const LINK_STALE_MS: u64 = 40_000;

/// How much of a decision budget is left, in `0..=1`; all of it when the
/// table names no budget.
pub fn clock_fraction(elapsed_ms: u64, budget_ms: u64) -> f32 {
    if budget_ms == 0 {
        return 1.0;
    }
    (1.0 - elapsed_ms as f32 / budget_ms as f32).clamp(0.0, 1.0)
}

/// The street, as the header names it.
fn street_label(hand: Option<&HandInProgress>, real: bool) -> String {
    match hand {
        Some(h) if h.over => "hand over".into(),
        Some(h) => match h.street {
            Some(0) => "pre-flop".into(),
            Some(1) => "flop".into(),
            Some(2) => "turn".into(),
            Some(3) => "river".into(),
            _ if h.cards.is_some() => "pre-flop".into(),
            _ if h.deck_ready => "deck sealed".into(),
            _ => "shuffling".into(),
        },
        None if real => "ready".into(),
        None => "waiting".into(),
    }
}

/// The one sentence about where the table stands.
///
/// While the chain runs, whose turn it is says more than the button does: it
/// is the one thing on this screen that can be late, and a player who knows
/// which seat everybody is waiting for knows whether the wait is theirs to fix.
fn note(
    hand: Option<&HandInProgress>,
    real: bool,
    waiting_for: &[u8],
    seated: usize,
    needed: u8,
    group: Option<(u16, u16)>,
) -> String {
    match (hand, real) {
        (Some(h), _) if h.over => format!("hand #{} is over", h.hand_id),
        (Some(h), _) if h.cards.is_some() => format!(
            "hand #{} — your cards are dealt; the button is at seat {}",
            h.hand_id, h.button
        ),
        // `S1-DG`: a hand this client follows without a seat in it -- back
        // from a restart and not yet dealt in, or certified out -- is said as
        // that, not as a deck being sealed while the others play the river.
        (Some(h), _) if h.street.is_some() => format!(
            "hand #{} — you are not dealt in this hand; the button is at seat {}",
            h.hand_id, h.button
        ),
        (Some(h), _) => match (h.shuffling, h.deck_ready) {
            (_, true) => format!(
                "hand #{} — the deck is shuffled and sealed; the button is at seat {}",
                h.hand_id, h.button
            ),
            (Some(s), _) => format!("hand #{} — seat {s} is shuffling the deck", h.hand_id),
            (None, false) => format!("hand #{} — preparing the deck", h.hand_id),
        },
        (None, true) if !waiting_for.is_empty() => {
            let who: Vec<String> = waiting_for.iter().map(|s| s.to_string()).collect();
            format!("waiting for seat {} to open the hand", who.join(", "))
        }
        // `S1-CS`, the owner's word: between the roster and the first hand
        // the players are joining the table's group, and the felt says so
        // with the count, because that wait is the one a player cannot see.
        (None, true) if group.is_some_and(|(seen, want)| want > 0 && seen < want) => {
            let (seen, want) = group.unwrap_or((0, 0));
            format!(
                "the table is set; the players are joining its group ({} of {} in) — the first hand deals when everybody is",
                seen, want
            )
        }
        (None, true) => "everybody has ratified the roster; the table is set".into(),
        (None, false) => format!("waiting for players — {seated} of {needed}"),
    }
}

/// The five board slots.
///
/// A card is face-up only where one has actually been opened -- three after
/// the flop, four after the turn, five after the river -- and `Facing::Empty`
/// for the rest. There is no back on the board: an unopened board card is not
/// a card somebody is holding, it is a card that does not exist yet.
fn board_cards(hand: Option<&HandInProgress>) -> [Facing; 5] {
    let mut out = [Facing::Empty; 5];
    let Some(h) = hand else {
        return out;
    };
    for (slot, index) in out.iter_mut().zip(h.board.iter()) {
        if let Ok(card) = crate::poker::state::Card::from_index(*index) {
            *slot = Facing::up(card, true);
        }
    }
    out
}

/// `S1-DO`: the hand a seat showed at the showdown, in words -- as the
/// hero's own panel would say it -- for every seat but the hero, from the
/// cards it showed and the board. `None` for a seat that showed nothing:
/// a folded or mucked hand was never opened, so there is nothing to name.
fn shown_name(hand: Option<&HandInProgress>, seat: u8, hero: Option<u8>) -> Option<String> {
    use crate::poker::state::Card;
    let h = hand?;
    if hero == Some(seat) {
        return None;
    }
    let pair = h.shown.get(usize::from(seat)).copied().flatten()?;
    let hole = [Card::from_index(pair[0]).ok()?, Card::from_index(pair[1]).ok()?];
    let board: Vec<Card> = h.board.iter().filter_map(|i| Card::from_index(*i).ok()).collect();
    Some(crate::poker::strength::describe(hole, &board))
}

/// What to draw in one seat's two card slots.
///
/// Face-up only for this client's own seat, and only from cards it opened
/// itself: `Facing::up` takes a verdict and the verdict here is that the cards
/// came out of a complete set of verified shares. Every other seat gets backs
/// once the deal is done -- those cards exist, this client holds `m-1` shares
/// for each of them, and it is one share short of every one of them by design.
/// Before the deal there is no card anywhere, and §22 is kept by there being
/// nothing to draw rather than by a check.
fn hole_cards(hand: Option<&HandInProgress>, seat: u8, hero: Option<u8>) -> [Facing; 2] {
    let Some(h) = hand else {
        return [Facing::Empty, Facing::Empty];
    };
    if !h.holding.contains(&seat) {
        return [Facing::Empty, Facing::Empty];
    }
    let face_up = |pair: [u8; 2]| {
        pair.map(|index| match crate::poker::state::Card::from_index(index) {
            Ok(card) => Facing::up(card, true),
            // A byte outside the deck cannot come from a card this client
            // opened, so this is unreachable -- and it draws a back rather
            // than panicking, because a covered card is a correct thing to
            // draw and a crashed table is not.
            Err(_) => Facing::Down,
        })
    };

    // A seat that showed at the showdown is face-up for everybody, and stays
    // that way while D-020's hold runs. This is what the hold is *for*: a
    // player who cannot see what beat them learns nothing from the hand.
    //
    // Only the seats that actually showed. A folded or mucked hand is not
    // drawn here, and could not be: its owner never published the share that
    // would open it, so no peer holds the cards to draw.
    if let Some(shown) = h.shown.get(usize::from(seat)).copied().flatten() {
        return face_up(shown);
    }
    match (h.cards, hero) {
        (Some(cards), Some(me)) if me == seat => face_up(cards),
        _ => [Facing::Down, Facing::Down],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::node::NodeEvent;
    use crate::poker::state::{Card, Rank, Suit};

    const KEY: [u8; 32] = [7u8; 32];

    fn card(r: Rank, s: Suit) -> u8 {
        Card::new(r, s).index()
    }

    /// `S1-FH`: a player out of the tournament watching the table sees backs at
    /// the seats a hand deals in and the hands shown at the showdown face up;
    /// its own seat, dealt nothing, stays empty, and a hand not shown stays
    /// covered.
    #[test]
    fn a_player_out_of_the_tournament_sees_the_hands_shown_at_the_showdown() {
        let mut s = seated(2);
        s.apply(NodeEvent::HandBegan { hand_id: 9, button: 0, dealt_in: vec![0, 1], small_blind: 50, big_blind: 100 });
        let cards_at = |s: &AppState, n: u8| s.table_view().seats.iter().find(|x| x.seat == n).map(|x| x.cards).unwrap();
        assert_eq!(cards_at(&s, 0), [Facing::Empty, Facing::Empty], "before the deal, nothing");
        s.apply(NodeEvent::CardsDealt { hand_id: 9, seats: vec![0, 1] });
        assert_eq!(cards_at(&s, 0), [Facing::Down, Facing::Down], "dealt: backs");
        assert_eq!(cards_at(&s, 2), [Facing::Empty, Facing::Empty], "this seat is dealt nothing");
        let shown = [card(Rank::Ace, Suit::Spades), card(Rank::King, Suit::Hearts)];
        s.apply(NodeEvent::HandEnded {
            hand_id: 9,
            stacks: vec![1_100, 900, 0],
            shown: vec![Some(shown), None, None],
            pots: vec![],
            gained: vec![200, 0, 0],
        });
        assert!(
            matches!(cards_at(&s, 0), [Facing::Up(_), Facing::Up(_)]),
            "the hand shown at the showdown, face up: {:?}",
            cards_at(&s, 0)
        );
        assert_eq!(cards_at(&s, 1), [Facing::Down, Facing::Down], "a hand not shown stays covered");
    }

    /// Three seats at a real table, buy-ins of a thousand, this client at
    /// `hero`.
    fn seated(hero: u8) -> AppState {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: KEY, seat: hero });
        s.apply(NodeEvent::Roster {
            key: KEY,
            seats: vec![(0, "Alice".into(), 1_000), (1, "Bob".into(), 1_000), (2, "Carol".into(), 1_000)],
        });
        s.apply(NodeEvent::TableParams {
            key: KEY,
            name: "Riverside".into(),
            seats: 3,
            needed: 3,
            small_blind: 50,
            big_blind: 100,
            action_ms: 30_000,
            tournament: true,
        });
        s.apply(NodeEvent::TableReal { key: KEY, session: [9u8; 32] });
        s
    }

    fn state(hand_id: u64, street: u16, pot: u64, to_act: Option<u8>, stacks: &[u64], bets: &[u64], folded: &[bool]) -> NodeEvent {
        NodeEvent::TableState {
            hand_id,
            street,
            pot,
            to_act,
            stacks: stacks.to_vec(),
            bets: bets.to_vec(),
            folded: folded.to_vec(),
        }
    }

    fn stacks(v: &TableView) -> Vec<u64> {
        v.seats.iter().map(|s| s.stack).collect()
    }

    /// The owner's first item: stacks were frozen at the buy-in. They are the
    /// engine's after every action, the settlement's when the hand is over,
    /// and they survive the boundary into the next hand.
    #[test]
    fn stacks_follow_the_engine_and_survive_the_boundary() {
        let mut s = seated(0);
        assert_eq!(stacks(&s.table_view()), vec![1_000, 1_000, 1_000], "the buy-ins before a hand");
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        assert_eq!(stacks(&s.table_view()), vec![1_000, 1_000, 1_000]);
        s.apply(state(1, 0, 150, Some(0), &[1_000, 950, 900], &[0, 50, 100], &[false; 3]));
        assert_eq!(stacks(&s.table_view()), vec![1_000, 950, 900], "the engine's figures");
        s.apply(NodeEvent::HandEnded { hand_id: 1, stacks: vec![1_150, 950, 900], shown: vec![None; 3], pots: Vec::new(), gained: Vec::new() });
        assert_eq!(stacks(&s.table_view()), vec![1_150, 950, 900], "the settlement's");
        s.apply(NodeEvent::HandBegan { hand_id: 2, button: 1, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        assert_eq!(stacks(&s.table_view()), vec![1_150, 950, 900], "and they carry into the next hand");
    }

    /// The second and third items: a seat's bet is chips in front of it and
    /// the pot is on the table whoever is to act, not only on the hero's turn.
    #[test]
    fn bets_and_the_pot_are_shown_whoever_is_to_act() {
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(state(1, 0, 150, Some(2), &[1_000, 950, 900], &[0, 50, 100], &[false; 3]));
        s.apply(NodeEvent::NotYourTurn { hand_id: 1, seat: Some(2), elapsed_ms: 0 });
        let v = s.table_view();
        assert!(!v.can_act);
        assert_eq!(v.pot, 150, "the pot is there when it is somebody else's turn");
        assert_eq!(v.seats.iter().map(|x| x.bet).collect::<Vec<_>>(), vec![0, 50, 100]);
        assert_eq!(v.to_act, Some(2));
        assert_eq!(v.street, "pre-flop");
        assert!(!v.hand_over);
    }

    /// A folded seat is marked, and a seat's winnings are what the settlement
    /// gave it over what it had -- the figure the chips fly with.
    #[test]
    fn folds_and_winnings_reach_the_seats() {
        let mut s = seated(1);
        s.apply(NodeEvent::HandBegan { hand_id: 3, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(state(3, 1, 300, Some(1), &[900, 900, 900], &[0, 0, 0], &[false, false, true]));
        let v = s.table_view();
        assert!(v.seats[2].folded && !v.seats[1].folded);
        assert_eq!(v.street, "flop");
        s.apply(NodeEvent::HandEnded { hand_id: 3, stacks: vec![900, 1_200, 900], shown: vec![None; 3], pots: Vec::new(), gained: Vec::new() });
        let v = s.table_view();
        assert!(v.hand_over);
        assert_eq!(v.seats.iter().map(|x| x.won).collect::<Vec<_>>(), vec![0, 300, 0]);
        assert_eq!(v.street, "hand over");
        assert_eq!(v.seats.iter().map(|x| x.bet).collect::<Vec<_>>(), vec![0, 0, 0], "nothing is in front of anybody once the pot is paid");
    }

    /// `S1-DO`: a seat that showed at the showdown has its hand named on the
    /// felt, the way the hero's panel names the hero's; the hero's own seat
    /// and a seat that showed nothing have no name.
    #[test]
    fn a_shown_hand_is_named_for_every_seat_but_the_hero() {
        use crate::poker::state::{Card, Rank, Suit};
        let idx = |r: Rank, s: Suit| {
            let want = Card::new(r, s);
            (0..52u8).find(|i| Card::from_index(*i).ok() == Some(want)).expect("a card has an index")
        };
        let mut s = seated(1);
        s.apply(NodeEvent::HandBegan { hand_id: 3, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::CardsDealt { hand_id: 3, seats: vec![0, 1, 2] });
        s.apply(state(3, 3, 300, None, &[900, 900, 900], &[0, 0, 0], &[false; 3]));
        let board = vec![
            idx(Rank::King, Suit::Spades),
            idx(Rank::King, Suit::Hearts),
            idx(Rank::Five, Suit::Diamonds),
            idx(Rank::Nine, Suit::Clubs),
            idx(Rank::Two, Suit::Spades),
        ];
        s.apply(NodeEvent::Board { hand_id: 3, cards: board.clone() });
        s.apply(NodeEvent::HandEnded {
            hand_id: 3,
            stacks: vec![1_200, 900, 600],
            shown: vec![
                Some([idx(Rank::Ace, Suit::Spades), idx(Rank::King, Suit::Diamonds)]),
                Some([idx(Rank::Seven, Suit::Clubs), idx(Rank::Eight, Suit::Clubs)]),
                None,
            ],
            pots: Vec::new(),
            gained: Vec::new(),
        });
        let v = s.table_view();
        let named = v.seats[0].shown_hand.clone().expect("seat 0 showed, so its hand is named");
        let cards: Vec<Card> = board.iter().map(|i| Card::from_index(*i).unwrap()).collect();
        assert_eq!(
            named,
            crate::poker::strength::describe(
                [Card::new(Rank::Ace, Suit::Spades), Card::new(Rank::King, Suit::Diamonds)],
                &cards
            ),
            "named as the hero's panel would name it"
        );
        assert!(named.to_lowercase().contains("king"), "{named}");
        assert!(v.seats[1].shown_hand.is_none(), "the hero's own hand is on the panel, not the felt");
        assert!(v.seats[2].shown_hand.is_none(), "a seat that showed nothing has nothing to name");
        assert!(
            v.seats[0].cards.iter().all(|f| matches!(f, crate::gui::table::Facing::Up(_))),
            "and its cards are face up"
        );
    }

    /// `S1-DG`: a hand ended without a settlement restores every stack to
    /// the hand's start, and the felt shows those -- not the buy-ins, which
    /// is what an empty stack list from the engine used to mean here.
    #[test]
    fn a_hand_ended_without_a_settlement_shows_the_stacks_it_started_with() {
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::HandEnded { hand_id: 1, stacks: vec![1_100, 900, 1_000], shown: vec![None, None, None], pots: Vec::new(), gained: Vec::new() });
        s.apply(NodeEvent::HandBegan { hand_id: 2, button: 1, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::TableState {
            hand_id: 2,
            street: 0,
            pot: 150,
            to_act: Some(0),
            stacks: vec![1_100, 850, 900],
            bets: vec![0, 50, 100],
            folded: vec![false, false, false],
        });
        assert_eq!(s.table_view().seats.iter().map(|x| x.stack).collect::<Vec<_>>(), vec![1_100, 850, 900]);
        // The deadline ends it: no settlement, no stacks reported.
        s.apply(NodeEvent::HandEnded { hand_id: 2, stacks: vec![], shown: vec![None, None, None], pots: Vec::new(), gained: Vec::new() });
        let v = s.table_view();
        assert!(v.hand_over);
        assert_eq!(v.seats.iter().map(|x| x.stack).collect::<Vec<_>>(), vec![1_100, 900, 1_000], "the stacks the hand started with");
        assert_eq!(v.seats.iter().map(|x| x.won).collect::<Vec<_>>(), vec![0, 0, 0], "nobody won anything");
        assert_eq!(v.seats.iter().map(|x| x.bet).collect::<Vec<_>>(), vec![0, 0, 0]);
    }

    /// `S1-DG`: a roster seat the running hand does not deal in sits it out,
    /// and the felt says so; a hero outside the hand is told so, not shown a
    /// deck being sealed while the others play. Before this every seat drew
    /// like any other and the note read the deck's state.
    #[test]
    fn a_seat_not_dealt_in_sits_out_and_a_hero_outside_the_hand_is_told() {
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 3, button: 0, dealt_in: vec![0, 1], small_blind: 10, big_blind: 20 });
        let v = s.table_view();
        assert!(v.seats.iter().any(|x| x.seat == 2 && x.sitting_out), "seat 2 sits this hand out");
        assert!(v.seats.iter().filter(|x| x.seat != 2).all(|x| !x.sitting_out));

        let mut b = seated(2);
        b.apply(NodeEvent::HandBegan { hand_id: 3, button: 0, dealt_in: vec![0, 1], small_blind: 10, big_blind: 20 });
        b.apply(NodeEvent::TableState {
            hand_id: 3,
            street: 1,
            pot: 200,
            to_act: Some(1),
            stacks: vec![900, 900, 1_000],
            bets: vec![0, 0, 0],
            folded: vec![false, false, false],
        });
        let v = b.table_view();
        assert_eq!(v.street, "flop");
        assert!(v.note.as_deref().is_some_and(|n| n.contains("not dealt in")), "{:?}", v.note);
        assert!(v.seats.iter().any(|x| x.seat == 2 && x.sitting_out), "the hero itself sits out");
    }

    /// The hero's hand is named on the left, with the odds of improving over
    /// the cards still to come.
    #[test]
    fn the_hero_sees_the_hand_named_and_its_odds() {
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        assert_eq!(s.table_view().hero_hand, None, "no cards, no name");
        s.apply(NodeEvent::CardsDealt { hand_id: 1, seats: vec![0, 1, 2] });
        s.apply(NodeEvent::HoleCards { hand_id: 1, cards: [card(Rank::Ace, Suit::Spades), card(Rank::King, Suit::Spades)] });
        let v = s.table_view();
        assert_eq!(v.hero_hand.as_deref(), Some("ace-king suited"));
        assert_eq!(v.improve_by, Some("by the flop"));
        assert!(v.improve_total > 0.3 && v.improve_total < 0.7, "{}", v.improve_total);
        s.apply(NodeEvent::Board {
            hand_id: 1,
            cards: vec![card(Rank::Queen, Suit::Spades), card(Rank::Seven, Suit::Spades), card(Rank::Two, Suit::Diamonds)],
        });
        let v = s.table_view();
        assert_eq!(v.hero_hand.as_deref(), Some("ace high"));
        assert_eq!(v.improve_by, Some("by the river"));
        let flush = v.improve.iter().find(|(n, _)| n == "a flush").map(|(_, p)| *p).unwrap();
        assert!((flush - 377.0 / 1081.0).abs() < 1e-4);
        // The others' cards are backs, the hero's are faces, the board is up.
        assert!(matches!(v.seats[0].cards[0], Facing::Up(_)));
        assert!(matches!(v.seats[1].cards[0], Facing::Down));
        assert!(matches!(v.board[0], Facing::Up(_)) && matches!(v.board[3], Facing::Empty));
    }

    /// The sixth item: the window reopened showed the last game. Leaving the
    /// table leaves no cards, no pot and no hand name behind, and a new seat
    /// starts from the roster alone.
    #[test]
    fn a_left_table_leaves_no_cards_behind() {
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::CardsDealt { hand_id: 1, seats: vec![0, 1, 2] });
        s.apply(NodeEvent::HoleCards { hand_id: 1, cards: [card(Rank::Ace, Suit::Spades), card(Rank::King, Suit::Spades)] });
        s.apply(state(1, 0, 150, Some(0), &[1_000, 950, 900], &[0, 50, 100], &[false; 3]));
        assert!(s.table_view().hero_hand.is_some());

        s.apply(NodeEvent::LeftTable { why: "left the table".into() });
        assert!(s.table_view().preview, "with no table the window shows the sample, and says so");

        let mut again = seated(2);
        again.last_stacks = s.last_stacks.clone();
        let v = again.table_view();
        assert!(!v.preview);
        assert!(v.hero_hand.is_none());
        assert_eq!(v.pot, 0);
        assert!(v.board.iter().all(|f| matches!(f, Facing::Empty)));
        assert!(v.seats.iter().all(|x| x.cards.iter().all(|c| matches!(c, Facing::Empty)) && x.bet == 0));
        assert_eq!(stacks(&v), vec![1_000, 1_000, 1_000], "the new table's buy-ins, not the old table's stacks");
    }

    /// The seventh item needs to know when a turn is a new one: every
    /// `YourTurn` is numbered, so the raise control can start again from the
    /// minimum.
    #[test]
    fn each_turn_is_numbered_for_the_raise_control() {
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        let turn = |hand_id| NodeEvent::YourTurn {
            hand_id,
            street: 0,
            to_call: 100,
            pot: 150,
            can_check: false,
            can_call: true,
            can_bet: false,
            can_raise: true,
            min_raise_to: 200,
            max_raise_to: 1_000,
            elapsed_ms: 0,
        };
        assert_eq!(s.table_view().turn_id, 0);
        s.apply(turn(1));
        let v = s.table_view();
        assert_eq!(v.turn_id, 1);
        assert!(v.can_act);
        assert_eq!((v.min_raise, v.max_raise, v.to_call), (200, 1_000, 100));
        s.apply(NodeEvent::NotYourTurn { hand_id: 1, seat: Some(1), elapsed_ms: 0 });
        assert_eq!(s.table_view().turn_id, 1, "somebody else's turn is not a new turn of ours");
        s.apply(turn(1));
        assert_eq!(s.table_view().turn_id, 2);
    }

    /// The sentence about the table is always there for the header; the felt
    /// gets it only while nothing is on the felt (`gui::table::felt_note`).
    #[test]
    fn the_note_says_where_the_table_stands() {
        let mut s = seated(0);
        assert!(s.table_view().note.unwrap().contains("the table is set"));
        s.apply(NodeEvent::HandWaiting { hand_id: 1, seats: vec![2] });
        assert!(s.table_view().note.unwrap().contains("waiting for seat 2"));
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        assert!(s.table_view().note.unwrap().contains("preparing the deck"));
        s.apply(NodeEvent::HandEnded { hand_id: 1, stacks: vec![1_000; 3], shown: vec![None; 3], pots: Vec::new(), gained: Vec::new() });
        assert!(s.table_view().note.unwrap().contains("is over"));
    }

    /// `S1-CS`: the seat to act carries a clock, full at the moment the turn
    /// arrived and nobody else's; the fraction itself is pure arithmetic.
    #[test]
    fn the_seat_to_act_carries_the_clock() {
        assert_eq!(clock_fraction(0, 30_000), 1.0);
        assert_eq!(clock_fraction(15_000, 30_000), 0.5);
        assert_eq!(clock_fraction(40_000, 30_000), 0.0);
        assert_eq!(clock_fraction(5, 0), 1.0, "no budget is no clock to run out");

        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(state(1, 0, 30, Some(2), &[1_000; 3], &[0, 10, 20], &[false; 3]));
        let v = s.table_view();
        let clocks: Vec<Option<f32>> = v.seats.iter().map(|x| x.clock).collect();
        assert!(clocks[2].is_some_and(|c| c > 0.95), "{clocks:?}");
        assert!(clocks[0].is_none() && clocks[1].is_none(), "{clocks:?}");
        s.apply(NodeEvent::HandEnded { hand_id: 1, stacks: vec![1_000; 3], shown: vec![None; 3], pots: Vec::new(), gained: Vec::new() });
        assert!(s.table_view().seats.iter().all(|x| x.clock.is_none()), "no clock once the hand is over");
    }

    /// `S1-CS`: the chat reaches the window under the seats that spoke, a
    /// muted seat is not heard until it is unmuted, and a seat's link is
    /// shown as the last ping said.
    #[test]
    fn sitting_out_and_the_groups_word_reach_the_window() {
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        assert!(!s.table_view().hero_sitting_out);
        s.apply(NodeEvent::SittingOut { on: true });
        assert!(s.table_view().hero_sitting_out, "the bar offers the way back");
        s.apply(NodeEvent::SittingOut { on: false });
        assert!(!s.table_view().hero_sitting_out);

        let away_of = |s: &AppState, seat: u8| {
            s.table_view().seats.iter().find(|x| x.seat == seat).and_then(|x| x.link).is_some_and(|l| l.away)
        };
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(0), away: true });
        assert!(away_of(&s, 1), "seat 1 sits out by the group's word");
        assert!(s.table_log.iter().any(|l| l.text.ends_with("sits out")));
        // Off the line: not said to sit out, and nothing of it is kept.
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: false, quiet_s: Some(25), away: true });
        assert!(!away_of(&s, 1), "a seat off the line is not shown sitting out");
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        assert!(!away_of(&s, 1), "back on the line and playing: no ghost of the old word");
        assert!(
            !s.table_log.iter().any(|l| l.text.ends_with("is back")),
            "dropping off the line is not coming back"
        );
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(0), away: true });
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: None, group: true, quiet_s: Some(0), away: false });
        assert!(!away_of(&s, 1), "the word taken back is taken back");
        assert!(s.table_log.iter().any(|l| l.text.ends_with("is back")));
        s.apply(NodeEvent::LeftTable { why: "left the table".into() });
        assert!(s.away.is_empty() && !s.sitting_out, "nothing of it outlives the table");
    }

    /// `S1-FL`: out of chips, the window says the place the node decided at the
    /// boundary -- shared or not -- ten seconds after the deciding hand, once;
    /// the other seats' places go to the table's log.
    #[test]
    fn a_busted_player_is_told_the_place() {
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 2, button: 1, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::HandEnded { hand_id: 2, stacks: vec![0, 3_000, 0], shown: vec![None; 3], pots: Vec::new(), gained: Vec::new() });
        assert_eq!(s.table_view().finished, None, "the node's word decides, not the stacks");
        s.apply(NodeEvent::Finished { hand_id: 2, seat: 2, place: 2, tied: false, players_left: 1, over: true });
        assert_eq!(s.table_view().finished, None, "another seat's place");
        assert!(s.table_log.iter().any(|l| l.text == "Carol finished in 2nd place"), "{:?}", s.table_log);
        s.apply(NodeEvent::Finished { hand_id: 2, seat: 0, place: 3, tied: false, players_left: 1, over: true });
        let b = s.table_view().finished.expect("the window says the place");
        assert_eq!((b.place, b.tied, b.players_left), (3, false, 1));
        assert!(b.show_in_ms > 9_000, "the deciding hand is looked at first: {} ms", b.show_in_ms);
        assert!(s.table_log.iter().any(|l| l.text == "Alice finished in 3rd place"));
        assert!(s.log.iter().any(|l| l.contains("finished in 3rd place")));

        // A place shared: busted in one hand with the same chips at its start.
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 2, button: 1, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::HandEnded { hand_id: 2, stacks: vec![0, 3_000, 0], shown: vec![None; 3], pots: Vec::new(), gained: Vec::new() });
        s.apply(NodeEvent::Finished { hand_id: 2, seat: 0, place: 2, tied: true, players_left: 1, over: true });
        let b = s.table_view().finished.expect("out");
        assert!(b.tied && b.place == 2);
        assert!(s.table_log.iter().any(|l| l.text == "Alice finished tied for 2nd place"), "{:?}", s.table_log);

        // Out while the others play on: the seats still in, said -- and soon,
        // before the next hand is under way (the owner, 2026-09-15).
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::HandEnded { hand_id: 1, stacks: vec![0, 1_900, 1_100], shown: vec![None; 3], pots: Vec::new(), gained: Vec::new() });
        s.apply(NodeEvent::Finished { hand_id: 1, seat: 0, place: 3, tied: false, players_left: 2, over: false });
        let b = s.table_view().finished.expect("out");
        assert_eq!((b.place, b.players_left), (3, 2));
        assert!(
            b.show_in_ms <= crate::gui::table::BUST_WINDOW_DELAY_MS && b.show_in_ms > 2_000,
            "a few seconds, not ten: {} ms",
            b.show_in_ms
        );
    }

    /// `S1-FL`: the winner is congratulated by the node's word -- once the hand
    /// that won it has been looked at -- and the log says who won the game when
    /// the showdown's own line did not.
    #[test]
    fn the_winner_is_congratulated() {
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 2, button: 1, dealt_in: vec![0, 1], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::HandEnded { hand_id: 2, stacks: vec![3_000, 0, 0], shown: vec![None; 3], pots: Vec::new(), gained: Vec::new() });
        assert_eq!(s.table_view().finished, None, "not before the node's word");
        s.apply(NodeEvent::Finished { hand_id: 2, seat: 0, place: 1, tied: false, players_left: 1, over: true });
        let f = s.table_view().finished.expect("the winner is told");
        assert_eq!((f.place, f.players_left), (1, 1));
        assert!(f.show_in_ms > 9_000, "ten seconds to look at the winning hand: {} ms", f.show_in_ms);
        assert!(s.log.iter().any(|n| n.contains("won the tournament")), "{:?}", s.log);
        assert_eq!(s.table_log.iter().filter(|l| l.text.ends_with("wins game 1!")).count(), 1, "said once: {:?}", s.table_log);
    }

    /// `D-050`: the showdown's wait for this player reaches the window, and
    /// goes with the word or with the next hand.
    #[test]
    fn the_showdown_choice_reaches_the_window() {
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 3, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        assert_eq!(s.table_view().show_cards_in_ms, None);
        s.apply(NodeEvent::ShowdownChoice { hand_id: 3, open_ms: Some(7_000) });
        let left = s.table_view().show_cards_in_ms.expect("the button is offered");
        assert!(left > 6_000 && left <= 7_000, "{left}");
        s.apply(NodeEvent::ShowdownChoice { hand_id: 3, open_ms: None });
        assert_eq!(s.table_view().show_cards_in_ms, None, "the word is said");
        s.apply(NodeEvent::ShowdownChoice { hand_id: 3, open_ms: Some(7_000) });
        s.apply(NodeEvent::HandBegan { hand_id: 4, button: 1, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        assert_eq!(s.table_view().show_cards_in_ms, None, "a new hand waits for nothing of the last");
    }

    /// `D-052`: the winner's badge says which pot it won. One pot is
    /// PokerTH's bare *Winner*; with side pots the word names the pot, and a
    /// pot with two winners is a split. Nobody who won nothing has a word.
    #[test]
    fn the_winner_of_each_pot_is_named_at_its_seat() {
        use crate::net::node::PotEnd;
        let word = |s: &AppState, seat: usize| {
            s.table_view().seats[seat].win.map(|w| w.word())
        };

        // One pot, one winner: the hand, whether it was shown or everybody folded.
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::HandEnded {
            hand_id: 1,
            stacks: vec![1_200, 900, 900],
            shown: vec![None; 3],
            pots: vec![PotEnd { size: 300, winners: vec![0] }],
            gained: Vec::new(),
        });
        assert_eq!(word(&s, 0), Some("Winner"));
        assert_eq!(word(&s, 1), None, "a seat that won nothing says nothing");

        // One pot, two winners: a split.
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::HandEnded {
            hand_id: 1,
            stacks: vec![1_050, 1_050, 900],
            shown: vec![None; 3],
            pots: vec![PotEnd { size: 300, winners: vec![0, 1] }],
            gained: Vec::new(),
        });
        assert_eq!(word(&s, 0), Some("Split pot"));
        assert_eq!(word(&s, 1), Some("Split pot"));

        // A main pot and a side pot, to different seats -- and the seat that
        // takes both is told so.
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::HandEnded {
            hand_id: 1,
            stacks: vec![1_400, 800, 800],
            shown: vec![None; 3],
            pots: vec![
                PotEnd { size: 300, winners: vec![1] },
                PotEnd { size: 200, winners: vec![0] },
            ],
            gained: Vec::new(),
        });
        assert_eq!(word(&s, 1), Some("Main pot"));
        assert_eq!(word(&s, 0), Some("Side pot"));

        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::HandEnded {
            hand_id: 1,
            stacks: vec![1_600, 700, 700],
            shown: vec![None; 3],
            pots: vec![
                PotEnd { size: 300, winners: vec![0, 2] },
                PotEnd { size: 200, winners: vec![0] },
            ],
            gained: Vec::new(),
        });
        assert_eq!(word(&s, 0), Some("Main + side"), "both pots");
        assert_eq!(word(&s, 2), Some("Split main"), "and its share of the main");
    }

    /// `S1-ER`: the winner is named from the settlement's own figures, and a
    /// state reported **after** the settlement cannot blank it.
    ///
    /// This is the shape that was live: the node's own `report_hand` sends the
    /// table's state and then, in the same call, `HandEnded` -- and once the
    /// hand was over that state already held the settled stacks. The window
    /// read what each seat took as the difference between the two, so it came
    /// out zero at every seat: nobody marked a winner, no chips flew and the
    /// table's log said nothing. The tests above never saw it because they
    /// applied `HandEnded` to a window that had been told no state at all.
    #[test]
    fn the_winner_survives_a_state_reported_after_the_settlement() {
        use crate::net::node::PotEnd;
        // Seat 1 takes a 300 pot: 900 behind before the settlement, 1 200 after.
        let ended = |gained: Vec<u64>| NodeEvent::HandEnded {
            hand_id: 1,
            stacks: vec![900, 1_200, 900],
            shown: vec![None; 3],
            pots: vec![PotEnd { size: 300, winners: vec![1] }],
            gained,
        };
        let running = state(1, 3, 300, None, &[900, 900, 900], &[0, 0, 0], &[false; 3]);
        // What the node reported after the settlement: the settled stacks.
        let settled = state(1, 3, 0, None, &[900, 1_200, 900], &[0, 0, 0], &[false; 3]);

        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(running.clone());
        s.apply(settled.clone());
        s.apply(ended(vec![0, 300, 0]));
        let v = s.table_view();
        assert_eq!(v.seats[1].won, 300, "the engine's own figure, not a difference");
        assert_eq!(v.seats[1].win.map(|w| w.word()), Some("Winner"));
        assert_eq!(v.seats[0].won, 0, "and nobody else took anything");
        assert!(
            s.table_log.iter().any(|l| l.text.contains("wins $300")),
            "the table's log says what was won: {:?}",
            s.table_log.iter().map(|l| l.text.clone()).collect::<Vec<_>>()
        );

        // And the node no longer sends that state at all, which is the fix at
        // its root: the difference is then the right one on its own.
        let mut s = seated(0);
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        s.apply(running);
        s.apply(ended(Vec::new()));
        assert_eq!(s.table_view().seats[1].won, 300, "the last running state is the baseline");
    }

    /// `D-051`: the table not safe reaches the window with a serial the
    /// window's *Stay* closes -- a second word from the node is a second
    /// question -- and goes when the node says the table is safe again; out
    /// for flooding is said as that, not as a fourth absence.
    #[test]
    fn the_table_not_safe_and_out_for_flooding_reach_the_window() {
        let mut s = seated(0);
        assert_eq!(s.table_view().unsafe_note, None);
        s.apply(NodeEvent::SeatFlooded { seat: 2 });
        assert!(
            s.table_view().log.iter().any(|l| l.text.contains("flooded the table's connection")),
            "the table's log says who flooded it"
        );
        s.apply(NodeEvent::TableUnsafe { why: Some("2 of the 4 players flooded".into()) });
        let (why, first) = s.table_view().unsafe_note.expect("said");
        assert!(why.contains("flooded"));
        s.apply(NodeEvent::TableUnsafe { why: Some("2 of the 4 players flooded".into()) });
        let (_, second) = s.table_view().unsafe_note.expect("said again");
        assert!(second > first, "asked again");
        s.apply(NodeEvent::TableUnsafe { why: None });
        assert_eq!(s.table_view().unsafe_note, None, "safe again");
        s.apply(NodeEvent::OutForGood { key: [0; 32], why: "out for flooding".into(), flooded: true });
        let v = s.table_view();
        assert!(v.out_for_good.is_some() && v.out_flooded);
    }

    /// `S1-IX`, section 6.4: a table stopped on a disagreement about a hand's
    /// result reaches the window by the names of the seats that differed, with
    /// a serial the window's *Wait* closes; the game ended on it is said as
    /// that, plainly and blaming nobody; and a table that deals again takes the
    /// word down. A table left takes it with it.
    #[test]
    fn a_table_stopped_on_a_disagreement_and_the_game_ended_on_it_reach_the_window() {
        use crate::net::node::TableStop;
        let stop = |ended: Option<&str>| NodeEvent::TableStopped {
            stop: Some(TableStop { hand_id: 7, seats: vec![1, 2], ended: ended.map(str::to_string) }),
        };
        let mut s = seated(0);
        assert_eq!(s.table_view().stopped, None);
        s.apply(stop(None));
        let v = s.table_view().stopped.expect("said");
        assert_eq!((v.hand_id, v.seats.clone(), v.ended.clone()), (7, vec!["Bob".to_string(), "Carol".to_string()], None));
        assert!(
            s.table_view().log.iter().any(|l| l.text.contains("Bob, Carol's client finished hand #7 with a different result")),
            "the table's log says why no further hand is dealt"
        );
        s.apply(NodeEvent::TableStopped { stop: None });
        assert_eq!(s.table_view().stopped, None, "the table deals again");
        assert!(s.table_view().log.iter().any(|l| l.text == "The table deals again."));

        s.apply(stop(None));
        let first = s.table_view().stopped.expect("stopped again").serial;
        s.apply(stop(Some("the seats compared their results again and they still differ")));
        let v = s.table_view().stopped.expect("ended");
        assert!(v.serial > first, "the end is a new word, not the question the player waited through");
        assert_eq!(v.ended.as_deref(), Some("the seats compared their results again and they still differ"));
        let line = s.table_view().log.last().expect("logged").text.clone();
        assert!(line.contains("could not agree on how hand #7 ended") && line.contains("No winner is named"), "{line}");
        assert!(!line.contains("Bob") && !line.contains("Carol"), "the end names nobody: {line}");

        s.apply(NodeEvent::LeftTable { why: "left the table".into() });
        assert!(s.stopped.is_none(), "nothing of it outlives the table");
    }

    #[test]
    fn the_chat_the_mutes_and_the_links_reach_the_window() {
        let mut s = seated(0);
        s.apply(NodeEvent::TableSaid { seat: 1, nickname: "Bob".into(), text: "hi".into() });
        s.apply(NodeEvent::TableSaid { seat: 2, nickname: "Carol".into(), text: "hello".into() });
        s.apply(NodeEvent::SeatLink { seat: 1, rtt_ms: Some(80), group: false, quiet_s: None, away: false });
        s.apply(NodeEvent::SeatLink { seat: 2, rtt_ms: None, group: false, quiet_s: None, away: false });
        let v = s.table_view();
        assert_eq!(v.chat.len(), 2);
        assert_eq!(v.chat[0].seat, 1);
        assert!(v.chat[1].who.contains("Carol"));
        assert_eq!(v.seats[1].link, Some(Link { rtt_ms: Some(80), stale: false, group: false, quiet_s: None, away: false }));
        assert_eq!(v.seats[2].link, Some(Link { rtt_ms: None, stale: false, group: false, quiet_s: None, away: false }));
        assert_eq!(v.seats[0].link, None, "nobody pings themselves");

        s.muted.insert(1);
        let v = s.table_view();
        assert!(v.seats[1].muted);
        assert_eq!(v.chat.len(), 1, "a muted seat is not heard");
        assert_eq!(v.chat[0].seat, 2);
        s.muted.remove(&1);
        assert_eq!(s.table_view().chat.len(), 2, "and is heard again once unmuted, history included");
    }

    /// `S1-CS`: between the roster and the first hand the felt says the
    /// players are joining the table's group, with the count, and stops
    /// saying it once everybody is in.
    #[test]
    fn the_felt_says_the_players_are_joining_the_group() {
        let mut s = seated(0);
        s.apply(NodeEvent::Carrier { seen: 0, want: 2 });
        let n = s.table_view().note.unwrap();
        assert!(n.contains("joining its group") && n.contains("0 of 2"), "{n}");
        s.apply(NodeEvent::Carrier { seen: 1, want: 2 });
        assert!(s.table_view().note.unwrap().contains("1 of 2"));
        s.apply(NodeEvent::Carrier { seen: 2, want: 2 });
        let n = s.table_view().note.unwrap();
        assert!(!n.contains("joining") && n.contains("the table is set"), "{n}");
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2], small_blind: 10, big_blind: 20 });
        assert!(!s.table_view().note.unwrap().contains("joining"), "a hand on the felt says the hand");
    }

    /// `D-043`: the node's `AtTable` says which slot what follows is about;
    /// the active slot's state is the fields, another slot's is kept aside
    /// and swapped in for its events; the window turns between them.
    #[test]
    fn two_tables_keep_their_own_state_and_the_window_turns_between_them() {
        let mut s = seated(1);
        assert_eq!(s.seated.as_ref().map(|x| x.key), Some([7u8; 32]));
        // The node turns to a second slot and seats this client there.
        s.apply(NodeEvent::AtTable { slot: 1, key: None });
        s.apply(NodeEvent::Seated { key: [9u8; 32], seat: 0 });
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1], small_blind: 10, big_blind: 20 });
        s.apply(NodeEvent::YourTurn {
            hand_id: 1,
            street: 0,
            to_call: 100,
            pot: 150,
            can_check: false,
            can_call: true,
            can_bet: false,
            can_raise: true,
            min_raise_to: 200,
            max_raise_to: 1_000,
            elapsed_ms: 0,
        });
        // The window still shows the first table, untouched.
        assert_eq!(s.seated.as_ref().map(|x| x.key), Some([7u8; 32]));
        assert!(s.hand.is_none(), "the second table's hand is not the first's");
        let slots = s.slots();
        assert_eq!(slots.len(), 2, "{slots:?}");
        assert!(slots[0].active && !slots[1].active);
        assert!(slots[1].turn && !slots[0].turn, "it is this client's turn at the second table");
        // The felt of the second slot can be looked at without turning.
        assert_eq!(s.table_view_of(1).hand, 1);
        assert_eq!(s.seated.as_ref().map(|x| x.key), Some([7u8; 32]), "looking did not turn");
        // The window turns: the second table's state is the fields now.
        s.switch_to(1);
        assert_eq!(s.active_slot, 1);
        assert_eq!(s.seated.as_ref().map(|x| x.key), Some([9u8; 32]));
        assert!(s.hand.as_ref().is_some_and(|h| h.hand_id == 1));
        // And events for the first slot land on its state aside.
        s.apply(NodeEvent::AtTable { slot: 0, key: Some([7u8; 32]) });
        s.apply(NodeEvent::LeftTable { why: "left".into() });
        assert_eq!(s.seated.as_ref().map(|x| x.key), Some([9u8; 32]), "leaving the first did not touch the second");
        assert_eq!(s.slots().len(), 1, "the first table is gone from the list");
    }
    /// `S1-FA`: a seat the table removed keeps its chips on the felt, and they
    /// must not be counted as a living opponent.
    ///
    /// Found in the owner's own game, 2026-09-14 (`table 486376d1`, hand #57).
    /// Seat 0's connection dropped, the table certified it out and went on
    /// heads-up between seats 1 and 2; seat 2 then busted. The node asked the
    /// engine over the **roster**, correctly found no hand #58 and left the
    /// table's group ten seconds later. The window asked a different question
    /// -- how many seats anywhere hold chips -- counted the removed seat's
    /// 10 600 among them, decided two players were still in and said nothing.
    /// So the winner was never told they had won: the felt showed *Hand over*
    /// until the group went, and *Line down: nobody at the table can be
    /// reached* afterwards, about a line this client had hung up itself.
    ///
    /// The same miscount put the loser one place too low: busting heads-up at
    /// that table was announced as **third**.
    #[test]
    fn a_removed_seats_chips_do_not_keep_the_tournament_open() {
        use crate::net::node::PotEnd;
        // Three sat down; the table certified seat 0 out, so hands are dealt
        // between 1 and 2 while seat 0's chips stay where they were. `S1-FL`:
        // the node decides the tournament is over and says the places -- the
        // showdown's own line named no winner, with two seats holding chips.
        let mut s = seated(1);
        s.apply(NodeEvent::HandBegan { hand_id: 57, button: 1, dealt_in: vec![1, 2], small_blind: 1_600, big_blind: 3_200 });
        s.apply(NodeEvent::TableState { hand_id: 57, street: 0, pot: 0, to_act: None, stacks: vec![10_600, 17_200, 2_200], bets: vec![0; 3], folded: vec![false; 3] });
        s.apply(NodeEvent::HandEnded {
            hand_id: 57,
            stacks: vec![10_600, 19_400, 0],
            shown: vec![None; 3],
            pots: vec![PotEnd { size: 4_400, winners: vec![1] }],
            gained: vec![0, 2_200, 0],
        });
        assert!(!s.table_log.iter().any(|l| l.text.contains("wins game")), "two seats hold chips: the showdown names no winner");
        s.apply(NodeEvent::Finished { hand_id: 57, seat: 1, place: 1, tied: false, players_left: 1, over: true });
        s.apply(NodeEvent::Finished { hand_id: 57, seat: 2, place: 2, tied: false, players_left: 1, over: true });
        s.apply(NodeEvent::Finished { hand_id: 57, seat: 0, place: 3, tied: false, players_left: 1, over: true });

        let v = s.table_view();
        let finish = v.finished.expect("the tournament ended and the window was told");
        assert_eq!(finish.place, 1, "the last seat of the roster holding chips has won it");
        assert_eq!(finish.players_left, 1, "and the removed seat is not a player left in it");
        assert!(
            s.log.iter().any(|n| n.contains("won the tournament")),
            "the winner is congratulated: {:?}",
            s.log
        );
        assert!(s.table_log.iter().any(|l| l.text == "Bob wins game 1!"), "{:?}", s.table_log);
        assert!(s.table_log.iter().any(|l| l.text == "Carol finished in 2nd place"));
        assert!(s.table_log.iter().any(|l| l.text == "Alice finished in 3rd place"));
        // And the line is not reported down: this client's own group goes ten
        // seconds after the last hand, by `D-042`, which is not a fault.
        assert_eq!(v.unsafe_note, None);
        assert!(
            !v.note.as_deref().unwrap_or("").contains("line may be down"),
            "a tournament that ended is not a network that failed: {:?}",
            v.note
        );
    }

}
