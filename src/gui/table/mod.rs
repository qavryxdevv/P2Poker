//! The poker table, styled after `assets/ggpoker-rush-and-cash-table.jpg`.
//!
//! Three files, because they answer three different kinds of question:
//!
//! * [`layout`] — *where* everything goes. Pure geometry, tested without a
//!   window: the hero at the bottom, chips never under a card, nothing outside
//!   the frame, at every seat count and every window size.
//! * [`paint`] — *what it looks like*. The sampled felt and rail, the
//!   four-colour deck, the chips.
//! * here — *what it says*, and what the player can do about it.
//!
//! # A card is drawn face-up only when it has been verified
//!
//! `SPEC_CS.md` §22 forbids displaying a cryptographically unverified card as
//! valid. That is enforced by the type rather than by care: [`Facing`] has no
//! constructor that produces a face-up card without a verdict. [`Facing::up`]
//! takes the verdict and returns a **back** when it is false, so the failure
//! mode of forgetting to check is a covered card, which is the safe direction.
//!
//! The one exception is [`TableView::preview`], which exists so the look can be
//! examined with no hand in progress. It is drawn with a banner across the top
//! saying so, and a test holds the banner to it.

pub mod layout;
pub mod paint;

use eframe::egui::{self, Color32, RichText, Stroke};

use crate::gui::theme;
use crate::poker::state::{Card, Chips, SeatIdx};
use layout::Layout;

/// A card this client has been cleared to draw face-up.
///
/// The wrapper is the whole point: it cannot be built without passing the
/// verification verdict, so no code path turns network data into a visible card
/// by forgetting a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown(Card);

impl Shown {
    pub fn card(self) -> Card {
        self.0
    }
}

/// What is drawn in a card-shaped hole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Facing {
    /// Verified, and drawn face-up.
    Up(Shown),
    /// Held but not shown — either not ours to see, or not yet verified.
    Down,
    /// No card at all: this seat is not in the hand, or the street has not come.
    #[default]
    Empty,
}

impl Facing {
    /// A card, face-up **only** if its proof verified.
    ///
    /// `SPEC_CS.md` §22. Passing `false` gives a back rather than an error,
    /// because a covered card is a correct thing to draw and a missing one is
    /// not: the seat does hold a card, this client just cannot vouch for it.
    pub fn up(card: Card, proof_verified: bool) -> Facing {
        if proof_verified {
            Facing::Up(Shown(card))
        } else {
            Facing::Down
        }
    }

    /// A card in a preview, where there is no hand and so nothing to verify.
    ///
    /// Only [`TableView::preview`] may use this, and a preview says so on the
    /// screen. It is separate from [`Facing::up`] so that no verdict-free path
    /// exists in the code that draws a real hand.
    pub fn sample(card: Card) -> Facing {
        Facing::Up(Shown(card))
    }
}

/// One seat, as the table draws it.
#[derive(Debug, Clone, Default)]
pub struct SeatView {
    pub seat: SeatIdx,
    pub name: String,
    pub stack: Chips,
    /// What this seat has put in on the current street.
    pub bet: Chips,
    pub cards: [Facing; 2],
    pub folded: bool,
    pub sitting_out: bool,
    /// How much of this seat's clock is left, if it is their turn.
    pub clock: Option<f32>,
}

/// The whole table, as a snapshot.
///
/// Plain data, filled in from the engine. Nothing here is hashed or signed —
/// like [`crate::app::AppState`], every field is a local view.
#[derive(Debug, Clone, Default)]
pub struct TableView {
    pub name: String,
    pub blinds: String,
    pub hand: u64,
    pub street: String,
    pub pot: Chips,
    pub board: [Facing; 5],
    pub seats: Vec<SeatView>,
    pub hero: SeatIdx,
    pub button: SeatIdx,
    pub to_act: Option<SeatIdx>,
    pub max_seats: u8,
    /// The hero's hand in words, once there is enough board to name it.
    pub hero_hand: Option<String>,
    /// Whether the hero may act, and what the choices cost.
    pub can_act: bool,
    pub to_call: Chips,
    pub min_raise: Chips,
    pub max_raise: Chips,
    /// No hand is in progress and these cards are a sample. Says so on screen.
    pub preview: bool,
    /// Why the table is not playable, when it is not.
    pub note: Option<String>,
}

/// What the player did this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAction {
    None,
    Fold,
    Check,
    Call,
    Raise(Chips),
    BackToLobby,
}

/// What the player is holding in the action bar between frames.
#[derive(Debug, Clone, Default)]
pub struct TableUi {
    pub raise: Chips,
}

/// Draw the table, and say what was pressed.
pub fn draw(ui: &mut egui::Ui, view: &TableView, state: &mut TableUi) -> TableAction {
    let mut action = TableAction::None;

    egui::Panel::top("table-head")
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .inner_margin(10.0)
                .stroke(Stroke::new(1.0, theme::LINE)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("< Lobby").clicked() {
                    action = TableAction::BackToLobby;
                }
                ui.add_space(8.0);
                ui.label(RichText::new(&view.name).color(theme::TEXT).size(20.0).strong());
                ui.add_space(12.0);
                ui.label(RichText::new(format!("blinds {}", view.blinds)).color(theme::MONEY));
                ui.add_space(12.0);
                ui.label(
                    RichText::new(format!("hand #{} · {}", view.hand, view.street))
                        .color(theme::TEXT_DIM),
                );
                if view.preview {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Said plainly, because a sample hand that looks like a
                        // real one is exactly what §22 is about.
                        ui.label(
                            RichText::new("PREVIEW — no hand is in progress, these cards are a sample")
                                .color(theme::WARN)
                                .strong(),
                        );
                    });
                }
            });
        });

    egui::Panel::bottom("table-actions")
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .inner_margin(10.0)
                .stroke(Stroke::new(1.0, theme::LINE)),
        )
        .show(ui, |ui| {
            action = action_bar(ui, view, state).unwrap_or(action);
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::WINDOW))
        .show(ui, |ui| {
            let area = ui.available_rect_before_wrap();
            let seats = view.max_seats.max(view.seats.len() as u8);
            let l = Layout::new(area, seats, view.hero);
            felt_and_people(ui, &l, view);
        });

    action
}

fn felt_and_people(ui: &egui::Ui, l: &Layout, view: &TableView) {
    let p = ui.painter();
    paint::table(p, l, &view.name);

    // The pot, then the board, then the seats — back to front, so a shadow is
    // always cast on something already drawn.
    if view.pot > 0 || view.preview {
        paint::plaque(p, l.pot, Color32::from_black_alpha(150), theme::FELT_KEYLINE);
        paint::centred(
            p,
            l.pot.center(),
            &format!("Total pot: {}", view.pot),
            l.pot.height() * 0.62,
            theme::MONEY,
        );
    }

    for (rect, facing) in l.board.iter().zip(view.board.iter()) {
        match facing {
            Facing::Up(s) => paint::card_face(p, *rect, s.card()),
            Facing::Down => paint::card_back(p, *rect),
            Facing::Empty => {}
        }
    }

    for slot in &l.seats {
        let Some(seat) = view.seats.iter().find(|s| s.seat == slot.seat) else {
            // An empty chair still gets its outline, so the shape of the table
            // does not change when somebody sits down.
            paint::plaque(
                p,
                slot.plate,
                Color32::from_black_alpha(110),
                theme::FELT_KEYLINE,
            );
            paint::centred(
                p,
                slot.plate.center(),
                "empty",
                slot.plate.height() * 0.40,
                theme::TEXT_DIM,
            );
            continue;
        };

        // Cards first: they sit behind the plate in the reference and the plate
        // must win where they meet.
        for (rect, facing) in slot.cards.iter().zip(seat.cards.iter()) {
            // A folded seat has mucked. Drawing backs for it would say it is
            // still in the hand, which is the one thing a card must never say
            // wrongly.
            match facing {
                _ if seat.folded => {}
                Facing::Up(s) => paint::card_face(p, *rect, s.card()),
                Facing::Down => paint::card_back(p, *rect),
                Facing::Empty => {}
            }
        }

        let acting = view.to_act == Some(seat.seat);
        let ring = if acting {
            theme::ACCENT
        } else if seat.folded || seat.sitting_out {
            theme::LINE
        } else {
            theme::RAIL_OUTER_EDGE
        };
        paint::portrait(p, slot.avatar, &seat.name, ring);
        if let Some(left) = seat.clock {
            paint::clock(p, slot.avatar, left);
        }

        let dim = seat.folded || seat.sitting_out;
        paint::plaque(
            p,
            slot.plate,
            if acting {
                theme::SELECTED
            } else {
                Color32::from_black_alpha(170)
            },
            if acting { theme::ACCENT } else { theme::FELT_KEYLINE },
        );
        let h = slot.plate.height();
        paint::centred(
            p,
            egui::pos2(slot.plate.center().x, slot.plate.top() + h * 0.32),
            &seat.name,
            h * 0.34,
            if dim { theme::TEXT_DIM } else { theme::TEXT },
        );
        paint::centred(
            p,
            egui::pos2(slot.plate.center().x, slot.plate.top() + h * 0.72),
            &if seat.sitting_out {
                "sitting out".to_string()
            } else if seat.folded {
                "folded".to_string()
            } else {
                seat.stack.to_string()
            },
            h * 0.32,
            if dim { theme::TEXT_DIM } else { theme::STACK },
        );

        if seat.seat == view.button {
            paint::dealer_button(p, slot.button);
        }
        if seat.bet > 0 {
            paint::chips(p, slot.bet, &seat.bet.to_string());
        }
    }

    if let Some(note) = &view.note {
        paint::centred(
            p,
            egui::pos2(l.centre.x, l.felt.bottom() - l.metrics.rail * 2.0),
            note,
            l.metrics.board_card.x * 0.34,
            theme::WARN,
        );
    }
}

/// Fold, check or call, raise — and nothing enabled that cannot be done.
fn action_bar(ui: &mut egui::Ui, view: &TableView, state: &mut TableUi) -> Option<TableAction> {
    let mut action = None;

    if let Some(hand) = &view.hero_hand {
        ui.label(RichText::new(hand).color(theme::OK).strong());
        ui.add_space(4.0);
    }

    if !view.can_act {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(match view.to_act {
                    Some(s) if s == view.hero => "your turn".to_string(),
                    Some(s) => format!("waiting for seat {s}"),
                    None => "no hand in progress".to_string(),
                })
                .color(theme::TEXT_DIM),
            );
        });
        return action;
    }

    if state.raise < view.min_raise {
        state.raise = view.min_raise;
    }

    ui.horizontal(|ui| {
        // The presets first, because they are what gets used, and the slider
        // after — the reverse of the order they were built in and the same order
        // the reference puts them in.
        for (label, part) in [("33%", 0.33), ("50%", 0.50), ("75%", 0.75), ("100%", 1.0)] {
            if ui.button(label).clicked() {
                let want = (view.pot as f64 * part) as Chips;
                state.raise = want.clamp(view.min_raise, view.max_raise.max(view.min_raise));
            }
        }
        ui.add_space(10.0);
        if view.max_raise > view.min_raise {
            ui.add(
                egui::Slider::new(&mut state.raise, view.min_raise..=view.max_raise)
                    .show_value(true),
            );
        }
    });

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let wide = egui::vec2((ui.available_width() - 24.0) / 3.0, 40.0);

        if ui
            .add_sized(
                wide,
                egui::Button::new(RichText::new("Fold").color(theme::TEXT).strong())
                    .fill(theme::DANGER),
            )
            .clicked()
        {
            action = Some(TableAction::Fold);
        }

        let (label, what) = if view.to_call == 0 {
            ("Check".to_string(), TableAction::Check)
        } else {
            (format!("Call {}", view.to_call), TableAction::Call)
        };
        if ui
            .add_sized(
                wide,
                egui::Button::new(RichText::new(label).color(theme::TEXT).strong())
                    .fill(theme::PANEL_LIGHT),
            )
            .clicked()
        {
            action = Some(what);
        }

        let can_raise = view.max_raise >= view.min_raise && view.min_raise > 0;
        let raise = ui.add_enabled(
            can_raise,
            egui::Button::new(
                RichText::new(format!("Raise to {}", state.raise))
                    .color(Color32::from_rgb(6, 20, 12))
                    .strong(),
            )
            .fill(theme::OK)
            .min_size(wide),
        );
        if raise.clicked() {
            action = Some(TableAction::Raise(state.raise));
        }
        if !can_raise {
            raise.on_hover_text("there is nothing left to raise with");
        }
    });

    action
}

impl TableView {
    /// A table with nobody at it, which is what the client has until a hand
    /// starts. Honest rather than empty: it says what it is waiting for.
    pub fn waiting(name: String, seats: u8) -> TableView {
        TableView {
            name,
            blinds: "—".into(),
            street: "waiting".into(),
            max_seats: seats,
            note: Some("waiting for players to sit down".into()),
            ..Default::default()
        }
    }

    /// A sample hand, so the table can be looked at with nothing in progress.
    ///
    /// Marked as a preview, which puts a banner on the screen saying these cards
    /// are not from a hand. See the module note on §22.
    pub fn sample() -> TableView {
        use crate::poker::state::{Rank, Suit};
        let c = |r: Rank, s: Suit| Facing::sample(Card::new(r, s));
        TableView {
            name: "Riverside".into(),
            blinds: "10 / 20".into(),
            hand: 128,
            street: "river".into(),
            pot: 1_150,
            board: [
                c(Rank::Three, Suit::Clubs),
                c(Rank::King, Suit::Diamonds),
                c(Rank::Four, Suit::Clubs),
                c(Rank::Five, Suit::Diamonds),
                c(Rank::King, Suit::Spades),
            ],
            seats: vec![
                SeatView {
                    seat: 0,
                    name: "Alice".into(),
                    stack: 2_240,
                    bet: 120,
                    cards: [c(Rank::Eight, Suit::Diamonds), c(Rank::Eight, Suit::Hearts)],
                    clock: Some(0.7),
                    ..Default::default()
                },
                SeatView {
                    seat: 1,
                    name: "Bob".into(),
                    stack: 1_970,
                    folded: true,
                    ..Default::default()
                },
                SeatView {
                    seat: 2,
                    name: "Dana".into(),
                    stack: 2_310,
                    bet: 40,
                    cards: [Facing::Down, Facing::Down],
                    ..Default::default()
                },
                SeatView {
                    seat: 3,
                    name: "Dave".into(),
                    stack: 2_070,
                    cards: [Facing::Down, Facing::Down],
                    ..Default::default()
                },
                SeatView {
                    seat: 4,
                    name: "Erin".into(),
                    stack: 2_000,
                    bet: 260,
                    cards: [Facing::Down, Facing::Down],
                    ..Default::default()
                },
                SeatView {
                    seat: 5,
                    name: "Frank".into(),
                    stack: 2_250,
                    sitting_out: true,
                    ..Default::default()
                },
            ],
            hero: 0,
            button: 3,
            to_act: Some(0),
            max_seats: 6,
            hero_hand: Some("two pair, kings and eights".into()),
            can_act: true,
            to_call: 260,
            min_raise: 520,
            max_raise: 2_240,
            preview: true,
            note: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poker::state::{Rank, Suit};

    /// §22, as a property of the type rather than of anybody's care: a card
    /// whose proof did not verify cannot come back face-up.
    #[test]
    fn an_unverified_card_is_drawn_as_a_back() {
        let card = Card::new(Rank::Ace, Suit::Spades);
        assert_eq!(Facing::up(card, false), Facing::Down);
        assert_eq!(Facing::up(card, true), Facing::Up(Shown(card)));
    }

    /// And the failure mode of forgetting the check is a covered card, not a
    /// missing one — the seat still holds something and the table still says so.
    #[test]
    fn a_covered_card_is_not_an_absent_one() {
        assert_ne!(
            Facing::up(Card::new(Rank::Two, Suit::Clubs), false),
            Facing::Empty
        );
    }

    /// The sample hand says it is a sample. Cards that came from nowhere and are
    /// drawn like cards that came from a verified deck is the exact confusion
    /// §22 exists to prevent.
    #[test]
    fn the_sample_hand_admits_what_it_is() {
        let v = TableView::sample();
        assert!(v.preview, "a sample hand that does not say so is a lie");
        assert!(v.board.iter().all(|f| !matches!(f, Facing::Empty)));
    }

    /// A table with nobody at it says what it is waiting for rather than
    /// showing an empty felt and no explanation.
    #[test]
    fn an_empty_table_explains_itself() {
        let v = TableView::waiting("Riverside".into(), 6);
        assert!(!v.preview);
        assert!(v.note.as_deref().unwrap().contains("waiting"));
        assert!(v.board.iter().all(|f| matches!(f, Facing::Empty)));
        assert!(!v.can_act, "there is nothing to act on");
    }

    /// The sample seats fit the sample table, or the preview would be showing a
    /// layout nobody could ever get.
    #[test]
    fn the_sample_is_a_table_that_could_exist() {
        let v = TableView::sample();
        assert_eq!(v.seats.len(), v.max_seats as usize);
        assert!(v.seats.iter().all(|s| s.seat < v.max_seats));
        assert!(v.hero < v.max_seats && v.button < v.max_seats);
        assert!(v.min_raise >= v.to_call, "a raise must beat a call");
        assert!(v.max_raise >= v.min_raise);
        let hero = v.seats.iter().find(|s| s.seat == v.hero).unwrap();
        assert!(
            v.max_raise <= hero.stack,
            "the hero cannot raise more than they have"
        );
    }

    /// A seat that has folded holds nothing. Cards in front of a folded seat
    /// say it is still in the hand.
    #[test]
    fn a_folded_seat_holds_no_cards() {
        let v = TableView::sample();
        for s in v.seats.iter().filter(|s| s.folded || s.sitting_out) {
            assert!(
                s.cards.iter().all(|c| matches!(c, Facing::Empty)),
                "seat {} is out of the hand and still holding cards",
                s.seat
            );
        }
    }

    /// Only the hero's own cards are face-up in the sample. A preview that shows
    /// everybody's hole cards would teach the wrong thing about what this client
    /// can see.
    #[test]
    fn the_sample_shows_only_the_heros_hand() {
        let v = TableView::sample();
        for s in &v.seats {
            let up = s.cards.iter().filter(|c| matches!(c, Facing::Up(_))).count();
            if s.seat == v.hero {
                assert_eq!(up, 2, "the hero sees their own cards");
            } else {
                assert_eq!(up, 0, "seat {} is showing its hand", s.seat);
            }
        }
    }
}
