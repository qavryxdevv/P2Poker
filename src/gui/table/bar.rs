//! PokerTH's action bar (`GameActionBar.qml`), in the Green Casino style.
//!
//! Two rows of the raise section and the row of actions, on a black panel at
//! the bottom of the table:
//!
//! 1. the amount -- a field the player can type into, and a slider that moves
//!    in PokerTH's steps;
//! 2. the quick bets *1/3*, *1/2* and *Pot*, the *All-In* button in Green
//!    Casino's red outline, and the playing mode: *Manual*, *Auto Check/Call*,
//!    *Auto Check/Fold*;
//! 3. *Fold*, *Call* or *Check*, and *Raise* or *Bet*, in the style's red, blue
//!    and gold.
//!
//! And PokerTH's behaviour with them: a button pressed before this client's
//! turn preselects its action (a gold ring and dot) and the action is taken
//! when the turn comes; an automatic mode acts by itself; a call whose amount
//! changed under the pointer is held for a second (the *accidental call
//! blocker*); a new turn starts the amount at the minimum.
//!
//! The actions leave as [`TableAction`]s and the engine checks every one
//! before anything is sealed: nothing here decides what is legal.

use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, Rect, Stroke, StrokeKind};

use super::seats::Seats;
use super::style::{self, ButtonLook, Weight};
use super::{raise_default, Facing, PlayMode, Pre, TableAction, TableUi, TableView};
use crate::poker::state::Chips;

/// The bar's height with its margin: the raise section (4 + 26 + 3 + 26 + 2),
/// the action row (54) and the 8 under the panel.
pub const HEIGHT: f32 = 123.0;
const RAISE_ROW_H: f32 = 26.0;
const ACTION_ROW_H: f32 = 54.0;
/// PokerTH's narrowest panel.
pub const MIN_WIDTH: f32 = 380.0;

/// The panel: as wide as the community row, at least `MIN_WIDTH`, centred
/// under the table.
pub fn rect(full: Rect, zone: Rect, l: &Seats) -> Rect {
    let board = l.board();
    let visual = board[4].right() - board[0].left();
    let w = full.width().min(visual.max(MIN_WIDTH));
    Rect::from_min_max(pos2(full.center().x - w / 2.0, zone.bottom()), pos2(full.center().x + w / 2.0, full.bottom() - 8.0))
}

/// `raiseStepFor`: the slider's step for a stack of this size.
pub fn raise_step(maximum: Chips) -> Chips {
    match maximum {
        0..=1_000 => 10,
        1_001..=10_000 => 50,
        10_001..=100_000 => 500,
        _ => 5_000,
    }
}

/// `roundedRaiseAmount`: a slider position as PokerTH rounds it.
pub fn rounded(amount: Chips, maximum: Chips) -> Chips {
    if amount >= maximum {
        return maximum;
    }
    let step = raise_step(maximum);
    amount / step * step
}

/// What the bar knows this frame about the hero and the hand.
struct Now {
    my_turn: bool,
    /// PokerTH's `actionsArmed`: this client's turn, or a hand it is in
    /// where an action may be chosen ahead.
    armed: bool,
    can_check: bool,
    call: Chips,
    raise_available: bool,
    raise_to: Chips,
    preflop: bool,
}

fn now_of(view: &TableView, state: &mut TableUi) -> Now {
    let hero = view.seats.iter().find(|s| s.seat == view.hero);
    let in_hand = view.hand > 0
        && !view.hand_over
        && hero.is_some_and(|h| {
            !h.folded && !h.sitting_out && !h.left && h.stack > 0 && h.cards.iter().any(|c| !matches!(c, Facing::Empty))
        });
    let my_turn = view.can_act;
    let max_bet = view.seats.iter().map(|s| s.bet).max().unwrap_or(0);
    let hero_bet = hero.map(|h| h.bet).unwrap_or(0);
    let call = if my_turn { view.to_call } else { max_bet.saturating_sub(hero_bet) };
    let raise_available = my_turn && view.max_raise >= view.min_raise && view.min_raise > 0;
    let raise_to = if raise_available { raise_default(state, view) } else { 0 };
    Now {
        my_turn,
        armed: my_turn || in_hand,
        can_check: call == 0,
        call,
        raise_available,
        raise_to,
        preflop: view.street == "pre-flop",
    }
}

/// The action a click (or a preselection coming due) takes on this
/// client's turn.
fn fire(which: Pre, view: &TableView, n: &Now) -> Option<TableAction> {
    if !n.my_turn {
        return None;
    }
    Some(match which {
        Pre::Fold => TableAction::Fold,
        Pre::Call if view.to_call == 0 => TableAction::Check,
        Pre::Call => TableAction::Call,
        Pre::Raise if n.raise_available => TableAction::Raise(n.raise_to),
        Pre::Raise => return None,
        Pre::AllIn if view.max_raise > view.to_call && view.max_raise > 0 => TableAction::Raise(view.max_raise),
        Pre::AllIn if view.to_call == 0 => TableAction::Check,
        Pre::AllIn => TableAction::Call,
    })
}

/// `clickAction`: act on this client's turn, otherwise preselect (a second
/// click takes the preselection back); any click leaves an automatic mode.
fn click(which: Pre, view: &TableView, state: &mut TableUi, n: &Now) -> Option<TableAction> {
    state.mode = PlayMode::Manual;
    if n.my_turn {
        state.pre = None;
        let act = fire(which, view, n);
        if act.is_some() {
            state.acted_turn = Some(view.turn_id);
        }
        return act;
    }
    if n.armed {
        if state.pre == Some(which) {
            state.pre = None;
        } else {
            state.pre = Some(which);
            state.pre_call = n.call;
        }
    }
    None
}

/// The turn has come: an automatic mode acts, or the preselection does.
fn due(view: &TableView, state: &mut TableUi, n: &Now) -> Option<TableAction> {
    if !n.my_turn || state.acted_turn == Some(view.turn_id) {
        return None;
    }
    let act = match state.mode {
        PlayMode::AutoCheckCall => Some(if view.to_call == 0 { TableAction::Check } else { TableAction::Call }),
        PlayMode::AutoCheckFold => Some(if view.to_call == 0 { TableAction::Check } else { TableAction::Fold }),
        PlayMode::Manual => match state.pre.take() {
            // PokerTH's *Check / Fold*: check when it is free.
            Some(Pre::Fold) if view.to_call == 0 => Some(TableAction::Check),
            Some(Pre::Fold) => Some(TableAction::Fold),
            // A preselected call is for the call it saw, and no other.
            Some(Pre::Call) if view.to_call == state.pre_call => fire(Pre::Call, view, n),
            Some(Pre::Call) => None,
            Some(other) => fire(other, view, n),
            None => None,
        },
    };
    if act.is_some() {
        state.acted_turn = Some(view.turn_id);
    }
    act
}

/// Draw the bar into `rect` and say what was pressed.
pub fn draw(ui: &mut egui::Ui, rect: Rect, view: &TableView, state: &mut TableUi, time: f64) -> Option<TableAction> {
    let mut action = None;
    let n = now_of(view, state);
    // A preselection belongs to the street it was made on.
    if !n.armed {
        state.pre = None;
    }
    if let Some(a) = due(view, state, &n) {
        action = Some(a);
    }

    let p = ui.painter().clone();
    style::shadow(&p, rect, 10.0, 2.0, 12.0, Color32::from_black_alpha(120));
    p.rect_filled(rect, 10.0, Color32::from_rgba_unmultiplied(0, 0, 0, 209));

    let x0 = rect.left() + 8.0;
    let w = rect.width() - 16.0;
    let row1 = Rect::from_min_size(pos2(x0, rect.top() + 4.0), vec2(w, RAISE_ROW_H));
    let row2 = Rect::from_min_size(pos2(x0, row1.bottom() + 3.0), vec2(w, RAISE_ROW_H));
    let actions = Rect::from_min_size(pos2(x0, row2.bottom() + 2.0 + 5.0), vec2(w, ACTION_ROW_H - 10.0));

    // Row one: the amount and the slider.
    let field = Rect::from_min_size(row1.min, vec2(78.0, RAISE_ROW_H));
    p.rect_filled(field, 5.0, if n.raise_available { Color32::from_rgb(0x1A, 0x2A, 0x1A) } else { Color32::from_rgb(0x17, 0x17, 0x17) });
    p.rect_stroke(
        field,
        5.0,
        Stroke::new(1.0, if n.raise_available { Color32::from_rgb(0x4C, 0xAF, 0x50) } else { Color32::from_rgb(0x3A, 0x3A, 0x3A) }),
        StrokeKind::Inside,
    );
    if !state.raise_editing {
        state.raise_text = if n.raise_available { n.raise_to.to_string() } else { String::new() };
    }
    let edit = ui.put(
        field.shrink2(vec2(6.0, 3.0)),
        egui::TextEdit::singleline(&mut state.raise_text)
            .frame(egui::Frame::NONE)
            .font(style::font(13.0, Weight::Bold))
            .text_color(if n.raise_available { style::WHITE } else { Color32::from_gray(0x8A) })
            .horizontal_align(Align::Center)
            .vertical_align(Align::Center)
            .char_limit(9)
            .interactive(n.raise_available),
    );
    if edit.has_focus() {
        state.raise_editing = true;
        state.raise_text.retain(|c| c.is_ascii_digit());
        if let Ok(v) = state.raise_text.parse::<Chips>() {
            if n.raise_available {
                state.raise = v.clamp(view.min_raise, view.max_raise.max(view.min_raise));
            }
        }
    }
    if edit.lost_focus() {
        state.raise_editing = false;
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) && n.raise_available {
            let fresh = now_of(view, state);
            if let Some(a) = click(Pre::Raise, view, state, &fresh) {
                action = Some(a);
            }
        }
    }

    let slider = Rect::from_min_max(pos2(field.right() + 6.0, row1.top()), row1.right_bottom());
    let slide = ui.interact(slider, ui.id().with("raise-slider"), if n.raise_available { egui::Sense::click_and_drag() } else { egui::Sense::hover() });
    let track = Rect::from_center_size(slider.center(), vec2(slider.width() - 18.0, 4.0));
    let span = view.max_raise.saturating_sub(view.min_raise);
    if n.raise_available && (slide.dragged() || slide.clicked()) {
        if let Some(pointer) = slide.interact_pointer_pos() {
            let t = ((pointer.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0);
            let value = view.min_raise + (span as f64 * t as f64).round() as Chips;
            state.raise = rounded(value, view.max_raise).clamp(view.min_raise, view.max_raise.max(view.min_raise));
            state.raise_editing = false;
        }
    }
    let position = if n.raise_available && span > 0 {
        (state.raise.saturating_sub(view.min_raise)) as f32 / span as f32
    } else {
        0.0
    };
    let dim = if n.raise_available { 1.0 } else { 0.45 };
    p.rect_filled(track, 2.0, style::faded(Color32::from_gray(0x33), dim));
    p.rect_filled(Rect::from_min_size(track.min, vec2(track.width() * position, track.height())), 2.0, style::faded(Color32::from_rgb(0x4C, 0xAF, 0x50), dim));
    let knob = pos2(track.left() + track.width() * position, track.center().y);
    p.circle_filled(knob, 9.0, style::faded(if slide.dragged() { Color32::from_rgb(0x80, 0xFF, 0x80) } else { Color32::from_rgb(0x4C, 0xAF, 0x50) }, dim));
    p.circle_stroke(knob, 9.0, Stroke::new(1.0, style::faded(Color32::from_rgb(0x2A, 0x7A, 0x2A), dim)));

    // Row two: the quick bets, All-In, the playing mode.
    let total_pot: Chips = view.pot + view.seats.iter().map(|s| s.bet).sum::<Chips>();
    let mut x = row2.left();
    for (label, part) in [("1/3", 1.0 / 3.0), ("1/2", 0.5), ("Pot", 1.0)] {
        let cell = Rect::from_min_size(pos2(x, row2.top()), vec2(38.0, RAISE_ROW_H));
        let r = ui.interact(cell, ui.id().with(("quick", label)), if n.raise_available { egui::Sense::click() } else { egui::Sense::hover() });
        let fill = if !n.raise_available {
            Color32::from_gray(0x20)
        } else if r.is_pointer_button_down_on() {
            Color32::from_rgb(0x2E, 0x7D, 0x32)
        } else if r.hovered() {
            Color32::from_rgb(0x38, 0x8E, 0x3C)
        } else {
            Color32::from_rgb(0x1B, 0x5E, 0x20)
        };
        p.rect_filled(cell, 5.0, fill);
        p.rect_stroke(cell, 5.0, Stroke::new(1.0, if n.raise_available { Color32::from_rgb(0x4C, 0xAF, 0x50) } else { Color32::from_gray(0x3A) }), StrokeKind::Inside);
        style::text(&p, cell.center(), Align2::CENTER_CENTER, label, 11.0, Weight::Bold, if n.raise_available { style::WHITE } else { Color32::from_gray(0x8A) });
        if r.clicked() && n.raise_available {
            let want = (total_pot as f64 * part).round() as Chips;
            state.raise = want.clamp(view.min_raise, view.max_raise.max(view.min_raise));
            state.raise_editing = false;
        }
        x += 38.0 + 4.0;
    }
    let all_in = Rect::from_min_size(pos2(x, row2.top()), vec2(52.0, RAISE_ROW_H));
    let r = ui.interact(all_in, ui.id().with("all-in"), if n.armed { egui::Sense::click() } else { egui::Sense::hover() });
    style::action_button(
        &p,
        all_in,
        ButtonLook::AllIn,
        "All-In",
        12.0,
        if n.armed { 1.0 } else { 0.4 },
        r.hovered() && n.armed,
        r.is_pointer_button_down_on() && n.armed,
        false,
        n.armed && state.pre == Some(Pre::AllIn),
    );
    if r.clicked() && n.armed {
        if let Some(a) = click(Pre::AllIn, view, state, &n) {
            action = Some(a);
        }
    }

    let combo = Rect::from_min_size(pos2(row2.right() - 132.0, row2.top()), vec2(132.0, RAISE_ROW_H));
    ui.scope_builder(egui::UiBuilder::new().max_rect(combo), |ui| {
        let v = ui.visuals_mut();
        let auto = state.mode != PlayMode::Manual;
        let fill = if auto { Color32::from_rgb(0x3A, 0x2E, 0x10) } else { Color32::from_gray(0x22) };
        let edge = if auto { style::COLOR_ACCENT } else { Color32::from_gray(0x3A) };
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.weak_bg_fill = fill;
            w.bg_fill = fill;
            w.bg_stroke = Stroke::new(1.0, edge);
            w.corner_radius = 5.into();
            w.fg_stroke = Stroke::new(1.0, style::WHITE);
        }
        ui.spacing_mut().interact_size.y = RAISE_ROW_H;
        ui.spacing_mut().button_padding = vec2(8.0, 3.0);
        egui::ComboBox::from_id_salt("pokerth-play-mode")
            .width(132.0 - 10.0)
            .selected_text(egui::RichText::new(state.mode.label()).font(style::font(11.0, Weight::Medium)).color(style::WHITE))
            .show_ui(ui, |ui| {
                for mode in [PlayMode::Manual, PlayMode::AutoCheckCall, PlayMode::AutoCheckFold] {
                    if ui.selectable_label(state.mode == mode, mode.label()).clicked() {
                        state.mode = mode;
                        state.pre = None;
                    }
                }
            });
    });

    // Row three: Fold, Call or Check, Raise or Bet.
    let call_text = if !n.armed {
        "Call".to_owned()
    } else if n.can_check {
        "Check".to_owned()
    } else {
        format!("Call\n${}", n.call)
    };
    // The accidental call blocker: a call whose words change while the bar is
    // armed is held for a second.
    if n.armed {
        match state.armed_since {
            None => {
                state.armed_since = Some(time);
                state.call_label = call_text.clone();
            }
            Some(since) if state.call_label != call_text => {
                if time - since > 0.5 {
                    state.call_blocked_until = time + 1.0;
                }
                state.call_label = call_text.clone();
            }
            Some(_) => {}
        }
    } else {
        state.armed_since = None;
    }
    let blocked = time < state.call_blocked_until;
    if blocked {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64((state.call_blocked_until - time).max(0.05)));
    }

    let fold_text = if n.armed && !n.my_turn && n.can_check { "Check /\nFold".to_owned() } else { "Fold".to_owned() };
    let word = if !n.preflop && n.can_check { "Bet" } else { "Raise" };
    let raise_text = if !n.armed {
        "Raise".to_owned()
    } else if n.raise_available {
        format!("{word}\n${}", n.raise_to)
    } else {
        word.to_owned()
    };
    let cell_w = (actions.width() - 16.0) / 3.0;
    let buttons = [
        (Pre::Fold, ButtonLook::Fold, fold_text, n.armed, false),
        (Pre::Call, ButtonLook::Call, call_text, n.armed && !blocked, false),
        (Pre::Raise, ButtonLook::Raise, raise_text, n.armed && n.raise_available, true),
    ];
    for (i, (which, look, label, armed, highlight)) in buttons.into_iter().enumerate() {
        let cell = Rect::from_min_size(pos2(actions.left() + i as f32 * (cell_w + 8.0), actions.top()), vec2(cell_w, actions.height()));
        let r = ui.interact(cell, ui.id().with(("action", i)), if armed { egui::Sense::click() } else { egui::Sense::hover() });
        let pre = armed && state.pre == Some(which);
        let opacity = if !armed { 0.4 } else if n.my_turn || pre { 1.0 } else { 0.72 };
        style::action_button(&p, cell, look, &label, 15.0, opacity, r.hovered() && armed, r.is_pointer_button_down_on() && armed, highlight && armed, pre);
        if r.clicked() && armed {
            if let Some(a) = click(which, view, state, &n) {
                action = Some(a);
            }
        }
    }
    action
}

/// The bar fits the smallest window the client allows, and leaves the table
/// most of its height: checked when the crate compiles.
const _BAR_FITS_THE_SMALLEST_WINDOW: () = {
    assert!(MIN_WIDTH <= super::MIN_WINDOW[0]);
    assert!(HEIGHT + super::APP_BAR_H + super::STATUS_BAR_H <= 0.40 * super::MIN_WINDOW[1]);
};

#[cfg(test)]
mod tests {
    use super::super::SeatView;
    use super::*;

    fn turn(to_call: Chips, min: Chips, max: Chips, id: u64) -> TableView {
        TableView {
            hand: 3,
            street: "flop".into(),
            can_act: true,
            to_call,
            min_raise: min,
            max_raise: max,
            turn_id: id,
            seats: vec![SeatView {
                seat: 0,
                stack: max,
                cards: [Facing::Down, Facing::Down],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    /// PokerTH's slider steps and its rounding, which never rounds the whole
    /// stack away.
    #[test]
    fn the_slider_moves_in_pokerths_steps() {
        assert_eq!(raise_step(800), 10);
        assert_eq!(raise_step(5_000), 50);
        assert_eq!(raise_step(50_000), 500);
        assert_eq!(raise_step(500_000), 5_000);
        assert_eq!(rounded(1_234, 5_000), 1_200);
        assert_eq!(rounded(5_000, 5_000), 5_000);
    }

    /// Auto Check/Call calls anything, Auto Check/Fold folds to a bet and
    /// checks when it is free; each acts once per turn.
    #[test]
    fn the_automatic_modes_act_once_per_turn() {
        let mut state = TableUi { mode: PlayMode::AutoCheckCall, ..Default::default() };
        let v = turn(40, 80, 1_000, 7);
        let n = now_of(&v, &mut state);
        assert_eq!(due(&v, &mut state, &n), Some(TableAction::Call));
        assert_eq!(due(&v, &mut state, &n), None, "once per turn");

        let mut state = TableUi { mode: PlayMode::AutoCheckFold, ..Default::default() };
        let v = turn(40, 80, 1_000, 8);
        let n = now_of(&v, &mut state);
        assert_eq!(due(&v, &mut state, &n), Some(TableAction::Fold));
        let v = turn(0, 20, 1_000, 9);
        let n = now_of(&v, &mut state);
        assert_eq!(due(&v, &mut state, &n), Some(TableAction::Check), "a free check is taken");
    }

    /// A preselected *Check / Fold* checks when it can; a preselected call is
    /// only for the call it saw.
    #[test]
    fn a_preselection_is_taken_when_the_turn_comes() {
        let mut state = TableUi { pre: Some(Pre::Fold), ..Default::default() };
        let v = turn(0, 20, 1_000, 3);
        let n = now_of(&v, &mut state);
        assert_eq!(due(&v, &mut state, &n), Some(TableAction::Check));

        let mut state = TableUi { pre: Some(Pre::Call), pre_call: 40, ..Default::default() };
        let v = turn(120, 240, 1_000, 4);
        let n = now_of(&v, &mut state);
        assert_eq!(due(&v, &mut state, &n), None, "the call changed: nothing is called");
        assert_eq!(state.pre, None);
    }

    /// All-In is a raise of everything when that is more than the call, and a
    /// call when it is not.
    #[test]
    fn all_in_is_everything() {
        let mut state = TableUi::default();
        let v = turn(40, 80, 1_000, 5);
        let n = now_of(&v, &mut state);
        assert_eq!(fire(Pre::AllIn, &v, &n), Some(TableAction::Raise(1_000)));
        let v = TableView { max_raise: 0, min_raise: 0, ..turn(900, 0, 0, 6) };
        let n = now_of(&v, &mut state);
        assert_eq!(fire(Pre::AllIn, &v, &n), Some(TableAction::Call));
    }

    /// Any click leaves an automatic mode, as PokerTH's does.
    #[test]
    fn a_click_leaves_the_automatic_mode() {
        let mut state = TableUi { mode: PlayMode::AutoCheckFold, ..Default::default() };
        let v = turn(40, 80, 1_000, 11);
        let n = now_of(&v, &mut state);
        assert_eq!(click(Pre::Call, &v, &mut state, &n), Some(TableAction::Call));
        assert_eq!(state.mode, PlayMode::Manual);
    }
}
