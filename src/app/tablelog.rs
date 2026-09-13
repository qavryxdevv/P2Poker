//! The table's history and its sounds, as PokerTH keeps them.
//!
//! The owner, 2026-09-13: the table window after PokerTH's Green Casino table,
//! with a *Log* panel of the game's actions and every sound of the game's
//! actions and situations. The wording is PokerTH's own
//! (`qmlguiinterface.cpp`, commit `944e8b83`):
//!
//! ```text
//! ## Game: 1 | Hand: 7 ##
//! Alice posts small blind ($10)
//! Bob posts big blind ($20)
//! Alice calls $20.
//! Bob checks.
//! --- Flop --- [A♠, 6♠, 3♠]
//! Alice bets $40.
//! Bob folds.
//! Alice wins $80
//! ```
//!
//! Everything here is a reading of what the node said; nothing decides
//! anything about the game.

use crate::gui::table::{LogKind, LogLine, SeatAct};
use crate::poker::actions::Action;
use crate::poker::state::Card;
use crate::sound::Cue;

use super::AppState;

/// How many lines the log keeps before the oldest go.
pub const MAX_TABLE_LOG: usize = 400;

/// A card as PokerTH's history writes it: the rank and the suit's symbol.
pub fn log_card(card: Card) -> String {
    let rank = match card.rank() as u8 {
        0 => "2",
        1 => "3",
        2 => "4",
        3 => "5",
        4 => "6",
        5 => "7",
        6 => "8",
        7 => "9",
        8 => "10",
        9 => "J",
        10 => "Q",
        11 => "K",
        _ => "A",
    };
    let suit = match card.suit() as u8 {
        0 => '♣',
        1 => '♦',
        2 => '♥',
        _ => '♠',
    };
    format!("{rank}{suit}")
}

impl AppState {
    fn log_table(&mut self, kind: LogKind, text: String) {
        if self.table_log.len() >= MAX_TABLE_LOG {
            self.table_log.pop_front();
        }
        self.table_log.push_back(LogLine { kind, text });
    }

    /// A seat's name as the roster has it, or *Seat N* for one it has none for.
    pub(super) fn seat_name(&self, seat: u8) -> String {
        self.seated
            .as_ref()
            .and_then(|s| s.roster.iter().find(|(n, _, _)| *n == seat))
            .map(|(_, who, _)| who.clone())
            .filter(|w| !w.is_empty())
            .unwrap_or_else(|| format!("Seat {seat}"))
    }

    /// The sounds owed since the last call, in order; the window plays them.
    pub fn take_sound_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.sound_cues)
    }

    /// The hand's header, and PokerTH's *sits out* for every seat of the
    /// roster still in the game that the hand does not deal in.
    pub(super) fn log_hand_began(&mut self, hand_id: u64) {
        let game = self.seated.as_ref().map(|s| s.game_no).unwrap_or(0).max(1);
        self.log_table(LogKind::Header, format!("## Game: {game} | Hand: {hand_id} ##"));
        let out: Vec<u8> = match (self.seated.as_ref(), self.hand.as_ref()) {
            (Some(s), Some(h)) => s
                .roster
                .iter()
                .filter(|(n, _, buyin)| {
                    let chips = h.start_stacks.get(usize::from(*n)).copied().unwrap_or(*buyin);
                    !h.dealt_in.contains(n) && chips > 0 && !self.left_for_good.contains(n) && !self.gone.contains(n)
                })
                .map(|(n, _, _)| *n)
                .collect(),
            _ => Vec::new(),
        };
        for seat in out {
            let name = self.seat_name(seat);
            self.log_table(LogKind::SitOut, format!("{name} sits out"));
        }
    }

    /// The blinds, from the first state of the hand's betting: whatever is in
    /// front of a seat before anybody has acted is a blind. And PokerTH's blind
    /// raise sound when the big blind is higher than it was.
    pub(super) fn log_blinds(&mut self, hand_id: u64) {
        let Some(h) = self.hand.as_ref().filter(|h| h.hand_id == hand_id) else {
            return;
        };
        if h.blinds_said || h.street != Some(0) || h.acted.iter().any(|a| a.is_some()) {
            return;
        }
        let mut posted: Vec<(u8, u64)> = h
            .bets
            .iter()
            .enumerate()
            .filter(|(_, b)| **b > 0)
            .map(|(i, b)| (i as u8, *b))
            .collect();
        if posted.is_empty() {
            return;
        }
        posted.sort_by_key(|(_, b)| *b);
        if let Some(h) = self.hand.as_mut() {
            h.blinds_said = true;
        }
        let big = posted[posted.len() - 1];
        if posted.len() >= 2 {
            let small = posted[0];
            let name = self.seat_name(small.0);
            self.log_table(LogKind::Normal, format!("{name} posts small blind (${})", small.1));
        }
        let name = self.seat_name(big.0);
        self.log_table(LogKind::Normal, format!("{name} posts big blind (${})", big.1));

        if let Some(s) = self.seated.as_mut() {
            let (seen, level) = s.blinds_seen;
            if big.1 > seen {
                if seen > 0 {
                    s.blinds_seen = (big.1, level.saturating_add(1));
                    if let Some(cue) = Cue::blinds_raised(level.saturating_add(1)) {
                        self.sound_cues.push(cue);
                    }
                } else {
                    s.blinds_seen = (big.1, 0);
                }
            }
        }
    }

    /// A street opened: PokerTH's `--- Flop --- [..]` for each street the
    /// board went past, and every seat's word cleared but *fold* and *all in*.
    pub(super) fn log_street(&mut self, hand_id: u64, before: usize) {
        let Some(h) = self.hand.as_mut().filter(|h| h.hand_id == hand_id) else {
            return;
        };
        let now = h.board.len();
        if now <= before {
            return;
        }
        for a in h.acted.iter_mut() {
            if !matches!(a, Some(SeatAct::Fold) | Some(SeatAct::AllIn)) {
                *a = None;
            }
        }
        let cards: Vec<String> = h
            .board
            .iter()
            .filter_map(|i| Card::from_index(*i).ok())
            .map(log_card)
            .collect();
        for (at, round) in [(3usize, "Flop"), (4, "Turn"), (5, "River")] {
            if before < at && at <= now && at <= cards.len() {
                let text = format!("--- {round} --- [{}]", cards[..at].join(", "));
                self.log_table(LogKind::Board, text);
            }
        }
    }

    /// The showdown's cards, the pot's winners and, when every chip ended at
    /// one seat, the game's.
    pub(super) fn log_showdown(&mut self, hand_id: u64) {
        let Some(h) = self.hand.as_ref().filter(|h| h.hand_id == hand_id) else {
            return;
        };
        let board: Vec<Card> = h.board.iter().filter_map(|i| Card::from_index(*i).ok()).collect();
        let mut lines: Vec<(LogKind, String)> = Vec::new();
        for (i, pair) in h.shown.iter().enumerate() {
            let Some([a, b]) = pair else {
                continue;
            };
            if h.folded.get(i).copied().unwrap_or(false) {
                continue;
            }
            let (Ok(ca), Ok(cb)) = (Card::from_index(*a), Card::from_index(*b)) else {
                continue;
            };
            let name = self.seat_name(i as u8);
            let named = crate::poker::strength::describe([ca, cb], &board);
            lines.push((
                LogKind::Normal,
                format!("{name} shows [{}, {}] - \"{named}\"", log_card(ca), log_card(cb)),
            ));
        }
        for (i, won) in h.won.iter().enumerate() {
            if *won > 0 {
                let name = self.seat_name(i as u8);
                lines.push((LogKind::Winner, format!("{name} wins ${won}")));
            }
        }
        if let Some(s) = self.seated.as_ref() {
            let with_chips: Vec<u8> = s
                .roster
                .iter()
                .map(|(n, _, _)| *n)
                .filter(|n| h.stacks.get(usize::from(*n)).copied().unwrap_or(0) > 0)
                .collect();
            if s.roster.len() >= 2 && with_chips.len() == 1 {
                let name = self.seat_name(with_chips[0]);
                lines.push((LogKind::GameWin, format!("{name} wins game {}!", s.game_no.max(1))));
            }
        }
        for (kind, text) in lines {
            self.log_table(kind, text);
        }
    }

    /// A seat acted: the badge beside it, the log's line, the action's sound.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn seat_acted(
        &mut self,
        hand_id: u64,
        seat: u8,
        action: Action,
        put_in: u64,
        total: u64,
        all_in: bool,
        by_table: bool,
    ) {
        let _ = (put_in, by_table);
        let act = if all_in {
            SeatAct::AllIn
        } else {
            match action {
                Action::Fold => SeatAct::Fold,
                Action::Check => SeatAct::Check,
                Action::Call => SeatAct::Call,
                Action::Bet(_) => SeatAct::Bet,
                Action::Raise(_) => SeatAct::Raise,
            }
        };
        if let Some(h) = self.hand.as_mut().filter(|h| h.hand_id == hand_id) {
            let i = usize::from(seat);
            if h.acted.len() <= i {
                h.acted.resize(i + 1, None);
            }
            h.acted[i] = Some(act);
        }
        let name = self.seat_name(seat);
        let text = match act {
            SeatAct::Fold => format!("{name} folds."),
            SeatAct::Check => format!("{name} checks."),
            SeatAct::Call => format!("{name} calls ${total}."),
            SeatAct::Bet | SeatAct::Raise => format!("{name} bets ${total}."),
            SeatAct::AllIn => format!("{name} is all in with ${total}."),
        };
        self.log_table(LogKind::Normal, text);
        self.sound_cues.push(match act {
            SeatAct::Fold => Cue::Fold,
            SeatAct::Check => Cue::Check,
            SeatAct::Call => Cue::Call,
            SeatAct::Bet => Cue::Bet,
            SeatAct::Raise => Cue::Raise,
            SeatAct::AllIn => Cue::AllIn,
        });
    }

    /// PokerTH's turn warning: three seconds before this client's own decision
    /// runs out, while it has not acted, and only on a clock of four seconds or
    /// more. Called every frame; said once per turn.
    pub fn tick_turn_warning(&mut self) {
        let Some(budget) = self.seated.as_ref().map(|s| s.action_ms) else {
            return;
        };
        let mine = self.hand.as_ref().is_some_and(|h| h.turn.is_some() && !h.over);
        if !mine || budget < 4_000 || self.turn_warned == self.turns {
            return;
        }
        let Some(since) = self.turn_since else {
            return;
        };
        if since.elapsed().as_millis() as u64 + 3_000 >= budget {
            self.turn_warned = self.turns;
            self.sound_cues.push(Cue::YourTurn);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::node::NodeEvent;

    fn table(names: &[&str]) -> AppState {
        let mut s = AppState::new();
        s.apply(NodeEvent::Seated { key: [7u8; 32], seat: 0 });
        s.apply(NodeEvent::Roster {
            key: [7u8; 32],
            seats: names.iter().enumerate().map(|(i, n)| (i as u8, n.to_string(), 1_000)).collect(),
        });
        s.apply(NodeEvent::TableReal { key: [7u8; 32], session: [9u8; 32] });
        s
    }

    fn texts(s: &AppState) -> Vec<String> {
        s.table_log.iter().map(|l| l.text.clone()).collect()
    }

    /// PokerTH's history, line for line: the header, the blinds from the first
    /// state, each action in its words, the street with its cards.
    #[test]
    fn a_hand_is_logged_in_pokerths_words() {
        let mut s = table(&["Alice", "Bob"]);
        s.take_sound_cues();
        s.apply(NodeEvent::HandBegan { hand_id: 7, button: 0, dealt_in: vec![0, 1] });
        s.apply(NodeEvent::TableState {
            hand_id: 7,
            street: 0,
            pot: 0,
            to_act: Some(0),
            stacks: vec![990, 980],
            bets: vec![10, 20],
            folded: vec![false, false],
        });
        s.apply(NodeEvent::SeatActed {
            hand_id: 7,
            seat: 0,
            action: Action::Call,
            put_in: 10,
            total: 20,
            all_in: false,
            by_table: false,
        });
        s.apply(NodeEvent::SeatActed {
            hand_id: 7,
            seat: 1,
            action: Action::Check,
            put_in: 0,
            total: 20,
            all_in: false,
            by_table: false,
        });
        s.apply(NodeEvent::Board { hand_id: 7, cards: vec![51, 17, 40] });
        let log = texts(&s);
        assert_eq!(
            log,
            vec![
                "## Game: 1 | Hand: 7 ##".to_string(),
                "Alice posts small blind ($10)".into(),
                "Bob posts big blind ($20)".into(),
                "Alice calls $20.".into(),
                "Bob checks.".into(),
                format!(
                    "--- Flop --- [{}, {}, {}]",
                    log_card(Card::from_index(51).unwrap()),
                    log_card(Card::from_index(17).unwrap()),
                    log_card(Card::from_index(40).unwrap())
                ),
            ]
        );
        assert_eq!(s.take_sound_cues(), vec![Cue::Call, Cue::Check]);
    }

    /// PokerTH's engine: a new street clears every seat's word but *fold* and
    /// *all in*.
    #[test]
    fn a_new_street_keeps_only_fold_and_all_in() {
        let mut s = table(&["A", "B", "C"]);
        s.apply(NodeEvent::HandBegan { hand_id: 1, button: 0, dealt_in: vec![0, 1, 2] });
        for (seat, action, all_in) in [(0u8, Action::Fold, false), (1, Action::Raise(500), true), (2, Action::Call, false)] {
            s.apply(NodeEvent::SeatActed { hand_id: 1, seat, action, put_in: 0, total: 500, all_in, by_table: false });
        }
        assert_eq!(
            s.hand.as_ref().unwrap().acted,
            vec![Some(SeatAct::Fold), Some(SeatAct::AllIn), Some(SeatAct::Call)]
        );
        s.apply(NodeEvent::Board { hand_id: 1, cards: vec![1, 2, 3] });
        assert_eq!(s.hand.as_ref().unwrap().acted, vec![Some(SeatAct::Fold), Some(SeatAct::AllIn), None]);
        assert!(texts(&s).contains(&"B is all in with $500.".to_string()));
    }

    /// A board that opens past several streets at once, an all-in run-out,
    /// says each of them, as PokerTH does.
    #[test]
    fn a_run_out_says_every_street() {
        let mut s = table(&["A", "B"]);
        s.apply(NodeEvent::HandBegan { hand_id: 2, button: 0, dealt_in: vec![0, 1] });
        s.apply(NodeEvent::Board { hand_id: 2, cards: vec![0, 1, 2, 3, 4] });
        let streets: Vec<String> = s
            .table_log
            .iter()
            .filter(|l| l.kind == LogKind::Board)
            .map(|l| l.text.split(" --- ").next().unwrap_or("").to_string())
            .collect();
        assert_eq!(streets, vec!["--- Flop", "--- Turn", "--- River"]);
    }

    /// The blinds going up is PokerTH's blind raise sound, by how many times
    /// they have; the first hand only sets where they start.
    #[test]
    fn the_blinds_going_up_is_heard() {
        let mut s = table(&["A", "B"]);
        for (hand, bb) in [(1u64, 20u64), (2, 20), (3, 40), (4, 60)] {
            s.apply(NodeEvent::HandBegan { hand_id: hand, button: 0, dealt_in: vec![0, 1] });
            s.take_sound_cues();
            s.apply(NodeEvent::TableState {
                hand_id: hand,
                street: 0,
                pot: 0,
                to_act: Some(0),
                stacks: vec![900, 900],
                bets: vec![bb / 2, bb],
                folded: vec![false, false],
            });
            let cues = s.take_sound_cues();
            match hand {
                1 | 2 => assert!(cues.is_empty(), "hand {hand}: {cues:?}"),
                3 | 4 => assert_eq!(cues, vec![Cue::BlindsRaiseLevel1], "hand {hand}"),
                _ => unreachable!(),
            }
        }
    }

    /// The showdown's cards with the hand's name, the winner, and the game's
    /// winner when every chip is at one seat.
    #[test]
    fn the_showdown_and_the_game_are_logged() {
        let mut s = table(&["A", "B"]);
        s.apply(NodeEvent::HandBegan { hand_id: 9, button: 0, dealt_in: vec![0, 1] });
        s.apply(NodeEvent::TableState {
            hand_id: 9,
            street: 3,
            pot: 2_000,
            to_act: None,
            stacks: vec![0, 0],
            bets: vec![0, 0],
            folded: vec![false, false],
        });
        s.apply(NodeEvent::Board { hand_id: 9, cards: vec![12, 25, 38, 0, 14] });
        s.apply(NodeEvent::HandEnded {
            hand_id: 9,
            stacks: vec![2_000, 0],
            shown: vec![Some([51, 11]), Some([1, 2])],
        });
        let log = texts(&s);
        assert!(log.iter().any(|l| l.starts_with("A shows [") && l.contains("] - \"")), "{log:?}");
        assert!(log.contains(&"A wins $2000".to_string()), "{log:?}");
        assert!(log.contains(&"A wins game 1!".to_string()), "{log:?}");
    }

    /// A seat that acted gets its sound whoever it is; the deal is heard
    /// once the hero's cards are.
    #[test]
    fn every_action_and_the_deal_are_heard() {
        let mut s = table(&["A", "B"]);
        s.take_sound_cues();
        s.apply(NodeEvent::HandBegan { hand_id: 3, button: 0, dealt_in: vec![0, 1] });
        s.apply(NodeEvent::HoleCards { hand_id: 3, cards: [0, 1] });
        s.apply(NodeEvent::SeatActed { hand_id: 3, seat: 1, action: Action::Fold, put_in: 0, total: 0, all_in: false, by_table: true });
        assert_eq!(s.take_sound_cues(), vec![Cue::DealTwoCards, Cue::Fold]);
    }
}
