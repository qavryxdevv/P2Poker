//! PokerTH's two side panels over the table (`GameSidePanel.qml`): the table
//! chat on the left (`ChatBox.qml`) and the history on the right
//! (`GameInfoPanel.qml`, its *Log* and *Odds* tabs), each opened and closed by
//! a round button at the top corner of the table, in Green Casino's chat and
//! log colours.
//!
//! The chat is the one the table always had (`S1-CS`: a line to the table's
//! group, the muted seats left out); the odds are the hero's own reading of
//! its hand the old window showed beside the buttons.

use eframe::egui::{self, pos2, text::LayoutJob, vec2, Align2, Color32, Rect, Stroke, StrokeKind, TextFormat};

use super::style::{self, Icon, Weight};
use super::{likeliest, pct, LogKind, LogTab, TableAction, TableUi, TableView};

/// The panels' width: a third of the table, at least 300 (PokerTH's).
fn panel_width(zone: Rect) -> f32 {
    (zone.width() / 3.0).max(300.0).min(zone.width() - 20.0)
}

/// The toggles, the chat and the log.
pub fn draw(ui: &mut egui::Ui, zone: Rect, view: &TableView, state: &mut TableUi) -> Option<TableAction> {
    let mut action = None;
    if state.chat_open {
        state.chat_read = view.chat.len();
    }
    let unread = if state.chat_open { 0 } else { view.chat.len().saturating_sub(state.chat_read) };
    let chat_toggle = style::round_button(
        ui,
        pos2(zone.left() + 8.0 + 17.0, zone.top() + 8.0 + 17.0),
        ui.id().with("chat-toggle"),
        Icon::Chat,
        state.chat_open,
        unread,
        "Chat",
    );
    if chat_toggle.clicked() {
        state.chat_open = !state.chat_open;
    }
    let log_toggle = style::round_button(
        ui,
        pos2(zone.right() - 8.0 - 17.0, zone.top() + 8.0 + 17.0),
        ui.id().with("log-toggle"),
        Icon::Log,
        state.log_open,
        0,
        "Log & odds",
    );
    if log_toggle.clicked() {
        state.log_open = !state.log_open;
    }

    let w = panel_width(zone);
    let top = zone.top() + 50.0;
    let bottom = side_panel_bottom(zone);
    if state.chat_open {
        let rect = Rect::from_min_max(pos2(zone.left() + 10.0, top), pos2(zone.left() + 10.0 + w, bottom));
        if let Some(line) = panel(ui, rect, "chat-panel", |ui| chat(ui, view, state)) {
            action = Some(TableAction::Say(line));
        }
    }
    if state.log_open {
        let rect = Rect::from_min_max(pos2(zone.right() - 10.0 - w, top), pos2(zone.right() - 10.0, bottom));
        panel(ui, rect, "log-panel", |ui| {
            log(ui, view, state);
            None::<String>
        });
    }
    action
}

/// Where the chat and log panels end.
pub fn side_panel_bottom(zone: Rect) -> f32 {
    (zone.bottom() - 10.0).max(zone.top() + 50.0 + 120.0)
}

/// The least height of the chat beside the bar: the line to type in, and its
/// frame.
pub const CORNER_CHAT_MIN_H: f32 = 44.0;

/// The chat in the corner left of the action bar (the owner, 2026-09-13), in
/// the odds' frame on the other side: what was said, as many lines as the
/// corner holds, and the line to say something.
pub fn chat_corner(ui: &egui::Ui, rect: Rect, view: &TableView, state: &mut TableUi) -> Option<TableAction> {
    framed(ui, rect, "chat-corner", 10.0, 8.0, 0.92, |ui| chat(ui, view, state)).map(TableAction::Say)
}

/// A side panel: the rounded, shadowed Green Casino box, over the table.
fn panel<R>(ui: &egui::Ui, rect: Rect, id: &str, add: impl FnOnce(&mut egui::Ui) -> Option<R>) -> Option<R> {
    framed(ui, rect, id, 16.0, 12.0, 0.95, add)
}

/// The Green Casino box over the table, with `radius`, `pad` and the fill's
/// `opacity`.
fn framed<R>(ui: &egui::Ui, rect: Rect, id: &str, radius: f32, pad: f32, opacity: f32, add: impl FnOnce(&mut egui::Ui) -> Option<R>) -> Option<R> {
    let mut out = None;
    egui::Area::new(ui.id().with(id))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .show(ui.ctx(), |ui| {
            ui.set_min_size(rect.size());
            ui.set_max_size(rect.size());
            let p = ui.painter();
            style::shadow(p, rect, radius, 3.0, radius + 2.0, Color32::from_black_alpha(140));
            p.rect_filled(rect, radius, style::faded(style::PANEL_BG, opacity));
            p.rect_stroke(rect, radius, Stroke::new(1.0, style::PANEL_BORDER), StrokeKind::Inside);
            // Clicks on the panel stay in the panel.
            let _ = ui.interact(rect, ui.id().with("eat"), egui::Sense::click());
            let inner = rect.shrink(pad);
            ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                ui.visuals_mut().override_text_color = Some(style::PANEL_TEXT);
                ui.visuals_mut().extreme_bg_color = style::PANEL_SURFACE;
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                out = add(ui);
            });
        });
    out
}

/// `ChatBox`: what was said, newest at the bottom, and a line to say
/// something; Enter or the arrow sends it.
fn chat(ui: &mut egui::Ui, view: &TableView, state: &mut TableUi) -> Option<String> {
    let input_h = (ui.available_height() / 5.0).clamp(24.0, 28.0);
    let history_h = ui.available_height() - input_h - ui.spacing().item_spacing.y;
    let width = ui.available_width();
    if history_h >= 16.0 {
        ui.allocate_ui(vec2(width, history_h), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("table-chat-history")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.set_width(width - 14.0);
                    if view.chat.is_empty() {
                        ui.label(egui::RichText::new("the table is quiet").italics().color(style::PANEL_MUTED).font(style::font(13.0, Weight::Regular)));
                    }
                    for line in &view.chat {
                        let mut job = LayoutJob::default();
                        job.wrap.max_width = width - 14.0;
                        job.append(&format!("{}:", line.who), 0.0, TextFormat::simple(style::font(13.0, Weight::Bold), style::PANEL_TEXT));
                        // Untrusted display data, rendered as data.
                        job.append(&line.said, 6.0, TextFormat::simple(style::font(13.0, Weight::Regular), style::PANEL_TEXT_2));
                        ui.label(job);
                    }
                });
        });
    }
    let mut said = None;
    ui.horizontal(|ui| {
        let send_w = input_h;
        let id = ui.id().with("table-chat-draft");
        let focused = ui.memory(|m| m.has_focus(id));
        let field = ui.add_sized(
            vec2(ui.available_width() - send_w - 6.0, input_h),
            egui::TextEdit::singleline(&mut state.chat_draft)
                .id(id)
                .hint_text(egui::RichText::new("Message …").color(style::PANEL_MUTED))
                .font(style::font(14.0, Weight::Regular))
                .text_color(style::PANEL_TEXT)
                .vertical_align(egui::Align::Center)
                .frame(
                    egui::Frame::new()
                        .fill(style::faded(style::PANEL_SURFACE, 0.6))
                        .stroke(Stroke::new(1.0, if focused { style::PANEL_TEXT_2 } else { style::faded(style::PANEL_MUTED, 0.6) }))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::symmetric(8, 4)),
                ),
        );
        let (send_rect, send) = ui.allocate_exact_size(vec2(send_w, input_h), egui::Sense::click());
        let ready = !state.chat_draft.trim().is_empty();
        let icon = (input_h * 0.64).round();
        style::icon(ui.painter(), Rect::from_center_size(send_rect.center(), vec2(icon, icon)), Icon::Send, if ready { style::SEND } else { style::faded(style::SEND, 0.4) });
        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (entered || send.clicked()) && ready {
            said = Some(std::mem::take(&mut state.chat_draft));
            ui.memory_mut(|m| m.request_focus(id));
        }
    });
    said
}

/// `GameInfoPanel`: the *Log* and *Odds* tabs.
fn log(ui: &mut egui::Ui, view: &TableView, state: &mut TableUi) {
    let width = ui.available_width();
    let (tabs_rect, _) = ui.allocate_exact_size(vec2(width, 20.0), egui::Sense::hover());
    let half = tabs_rect.width() / 2.0;
    for (i, (tab, label)) in [(LogTab::Log, "Log"), (LogTab::Odds, "Odds")].into_iter().enumerate() {
        let cell = Rect::from_min_size(pos2(tabs_rect.left() + i as f32 * half, tabs_rect.top()), vec2(half - 2.0, tabs_rect.height()));
        let r = ui.interact(cell, ui.id().with(("log-tab", i)), egui::Sense::click());
        let active = state.log_tab == tab;
        let p = ui.painter();
        p.rect_filled(cell, 4.0, if active { style::PANEL_SURFACE } else { style::faded(style::PANEL_BORDER, 0.20) });
        style::text(p, cell.center(), Align2::CENTER_CENTER, label, 11.0, if active { Weight::DemiBold } else { Weight::Medium }, if active { style::PANEL_TEXT } else { style::PANEL_MUTED });
        if r.clicked() {
            state.log_tab = tab;
        }
    }
    ui.add_space(4.0);
    match state.log_tab {
        LogTab::Log => {
            egui::ScrollArea::vertical()
                .id_salt("table-log-history")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.set_width(width - 14.0);
                    if view.log.is_empty() {
                        ui.label(egui::RichText::new("no hand yet").italics().color(style::PANEL_MUTED).font(style::font(13.0, Weight::Regular)));
                    }
                    for line in &view.log {
                        let (colour, weight, italics) = match line.kind {
                            LogKind::Normal => (style::PANEL_TEXT, Weight::Regular, false),
                            LogKind::Header => (style::PANEL_TEXT, Weight::Bold, false),
                            LogKind::Board => (style::LOG_BOARD, Weight::Regular, false),
                            LogKind::Winner => (style::LOG_WINNER, Weight::Regular, false),
                            LogKind::SitOut => (style::LOG_BOARD, Weight::Regular, true),
                            LogKind::GameWin => (style::PANEL_TEXT, Weight::Bold, true),
                        };
                        let mut job = LayoutJob::default();
                        job.wrap.max_width = width - 14.0;
                        job.append(&line.text, 0.0, TextFormat { font_id: style::font(14.0, weight), color: colour, italics, ..Default::default() });
                        ui.label(job);
                    }
                });
        }
        LogTab::Odds => {
            let folded = view.seats.iter().any(|s| s.seat == view.hero && s.folded);
            match &view.hero_hand {
                Some(name) => {
                    ui.label(egui::RichText::new(if folded { format!("{name} — folded") } else { name.clone() }).font(style::font(14.0, Weight::DemiBold)).color(style::PANEL_TEXT));
                    match (view.improve_by, view.improve.is_empty()) {
                        (Some(by), false) => {
                            ui.label(egui::RichText::new(format!("improves {by}: {}", pct(view.improve_total))).font(style::font(12.0, Weight::Regular)).color(style::PANEL_TEXT_2));
                            for (label, chance) in likeliest(&view.improve, 10) {
                                let (row, _) = ui.allocate_exact_size(vec2(width - 14.0, 26.0), egui::Sense::hover());
                                let p = ui.painter();
                                p.rect_filled(row, 0.0, style::faded(style::PANEL_BORDER, 0.22));
                                let bar = Rect::from_min_size(row.min, vec2(row.width() * chance.clamp(0.0, 1.0), row.height()));
                                p.rect_filled(bar, 0.0, style::faded(style::COLOR_ACCENT, 0.42));
                                style::text(p, pos2(row.left() + 8.0, row.center().y), Align2::LEFT_CENTER, &label, 12.0, Weight::Regular, style::PANEL_TEXT);
                                style::text(p, pos2(row.right() - 8.0, row.center().y), Align2::RIGHT_CENTER, &pct(chance), 12.0, if chance >= 0.5 { Weight::Bold } else { Weight::Regular }, style::PANEL_TEXT);
                            }
                        }
                        (Some(_), true) => {
                            ui.label(egui::RichText::new("nothing to improve to").color(style::PANEL_MUTED));
                        }
                        (None, _) => {}
                    }
                }
                None => {
                    ui.label(egui::RichText::new("no cards yet").italics().color(style::PANEL_MUTED));
                }
            }
        }
    }
}
