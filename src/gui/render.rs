//! The drawing, which is a thin layer over the decisions in
//! [`lobby`](super::lobby).
//!
//! Nothing here decides anything. Every question a widget asks — is this table
//! joinable, what does the state column say, what is the worst true thing about
//! the connection — is answered by a tested function next door, and this file
//! puts the answer on the screen.
//!
//! # The one rule that shows up as code rather than prose
//!
//! `SPEC_CS.md` §22 closes with *never display a cryptographically unverified
//! card as valid*. The lobby draws no cards at all, which is the easiest way to
//! obey it; the table window's obligation is real and is that window's.

use eframe::egui::{self, Color32, RichText};

use super::lobby::{LobbyView, TableState, PASSWORD_WARNING};
use super::theme;

/// What the user did this frame.
///
/// Returned rather than acted on, so the paint loop never touches the network
/// and §33's rule survives contact with a button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LobbyAction {
    None,
    Select([u8; 32]),
    Join([u8; 32]),
    CreateTable,
}

/// Draw the lobby, and say what was pressed.
pub fn lobby(ui: &mut egui::Ui, view: &LobbyView) -> LobbyAction {
    let mut action = LobbyAction::None;

    // ---- network status ----------------------------------------------------
    ui.horizontal(|ui| {
        let s = &view.status;
        let colour = if s.relay.as_ref().is_some_and(|r| !r.adequate) {
            theme::WARNING
        } else if s.peers == 0 {
            theme::LOBBY_DIM
        } else {
            theme::GOOD
        };
        ui.label(RichText::new(s.summary()).color(colour).strong());

        if s.failed_dials > 0 {
            // Counted rather than listed: most dials fail on an open DHT, and a
            // log of them buries what matters — but with no count at all, "most
            // dials fail" and "this client is broken" look identical.
            ui.label(
                RichText::new(format!("({} dials failed)", s.failed_dials))
                    .color(theme::LOBBY_DIM)
                    .small(),
            );
        }
    });
    ui.separator();

    // ---- the table list ----------------------------------------------------
    ui.heading(RichText::new("Tables").color(theme::LOBBY_TEXT));

    egui::ScrollArea::vertical()
        .id_salt("tables")
        .max_height(260.0)
        .show(ui, |ui| {
            egui::Grid::new("table-list")
                .num_columns(6)
                .striped(true)
                .show(ui, |ui| {
                    for h in ["Table", "Game", "Blinds", "Seats", "State", ""] {
                        ui.label(RichText::new(h).color(theme::LOBBY_DIM).small());
                    }
                    ui.end_row();

                    for row in &view.tables {
                        let selected = view.selected == Some(row.key);

                        let mut name = RichText::new(&row.name).color(theme::LOBBY_TEXT);
                        if selected {
                            name = name.strong();
                        }
                        if ui.selectable_label(selected, name).clicked() {
                            action = LobbyAction::Select(row.key);
                        }

                        ui.label(RichText::new(&row.game).color(theme::LOBBY_TEXT));
                        ui.label(RichText::new(&row.blinds).color(theme::LOBBY_TEXT));
                        ui.label(RichText::new(&row.occupancy).color(theme::LOBBY_TEXT));

                        let state_colour = match row.state {
                            TableState::Open => theme::GOOD,
                            TableState::Full => theme::LOBBY_DIM,
                            TableState::ParametersChanged => theme::REFUSED,
                        };
                        let state = ui.label(
                            RichText::new(row.state.label()).color(state_colour),
                        );
                        // The refusal explains itself on hover, because a state
                        // word alone tells a player nothing about whether their
                        // client is at fault.
                        if let Some(why) = row.state.why_not() {
                            state.on_hover_text(why);
                        }

                        if row.password_required {
                            ui.label(RichText::new("password").color(theme::WARNING).small())
                                .on_hover_text(PASSWORD_WARNING);
                        } else {
                            ui.label("");
                        }
                        ui.end_row();
                    }

                    if view.tables.is_empty() {
                        ui.label(
                            RichText::new("no tables yet").color(theme::LOBBY_DIM).italics(),
                        );
                        ui.end_row();
                    }
                });
        });

    ui.separator();

    // ---- join and create ---------------------------------------------------
    ui.horizontal(|ui| {
        let can_join = view.can_join();
        let join = ui.add_enabled(can_join, egui::Button::new("Join"));
        if join.clicked() {
            if let Some(row) = view.selected_row() {
                action = LobbyAction::Join(row.key);
            }
        }
        // A grey button with no explanation is indistinguishable from a broken
        // client, so the reason is always one hover away.
        if !can_join {
            let why = view
                .selected_row()
                .and_then(|r| r.state.why_not())
                .unwrap_or("Select a table first.");
            join.on_hover_text(why);
        }

        if ui.button("Create table").clicked() {
            action = LobbyAction::CreateTable;
        }
    });

    ui.separator();

    // ---- players and chat --------------------------------------------------
    ui.columns(2, |cols| {
        cols[0].heading(RichText::new("Players").color(theme::LOBBY_TEXT));
        egui::ScrollArea::vertical()
            .id_salt("players")
            .max_height(160.0)
            .show(&mut cols[0], |ui| {
                for name in &view.seated {
                    // A display name, and never an identifier. Rendered as data.
                    ui.label(RichText::new(name).color(theme::LOBBY_TEXT));
                }
                if view.seated.is_empty() {
                    ui.label(RichText::new("nobody yet").color(theme::LOBBY_DIM).italics());
                }
            });

        cols[1].heading(RichText::new("Chat").color(theme::LOBBY_TEXT));
        egui::ScrollArea::vertical()
            .id_salt("chat")
            .max_height(160.0)
            .stick_to_bottom(true)
            .show(&mut cols[1], |ui| {
                for line in &view.chat {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("{}:", line.who)).color(theme::LOBBY_DIM));
                        ui.label(RichText::new(&line.said).color(theme::LOBBY_TEXT));
                    });
                }
            });
    });

    action
}

/// The event log, which is what a player sends when they say it does not work.
pub fn log(ui: &mut egui::Ui, lines: impl Iterator<Item = impl AsRef<str>>) {
    egui::ScrollArea::vertical()
        .id_salt("log")
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for line in lines {
                ui.label(
                    RichText::new(line.as_ref())
                        .color(theme::LOBBY_DIM)
                        .monospace()
                        .small(),
                );
            }
        });
}

/// The dark surface both panes sit on.
pub fn install(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = theme::LOBBY_BG;
    visuals.window_fill = theme::LOBBY_BG;
    visuals.faint_bg_color = theme::LOBBY_ROW_ALT;
    visuals.extreme_bg_color = theme::LOBBY_ROW;
    visuals.override_text_color = Some(theme::LOBBY_TEXT);
    ctx.set_visuals(visuals);
}

/// A colour a test can compare, so the state column's meaning is not only in the
/// paint code.
pub const fn state_colour(state: &TableState) -> Color32 {
    match state {
        TableState::Open => theme::GOOD,
        TableState::Full => theme::LOBBY_DIM,
        TableState::ParametersChanged => theme::REFUSED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The colour says it before the word does, and the three states are three
    /// colours.
    #[test]
    fn the_state_colours_are_distinct() {
        let open = state_colour(&TableState::Open);
        let full = state_colour(&TableState::Full);
        let changed = state_colour(&TableState::ParametersChanged);
        assert_ne!(open, full);
        assert_ne!(open, changed);
        assert_ne!(full, changed);
    }

    /// A table this client refuses to join is drawn in the refusal colour and
    /// not merely dimmed, because dim reads as "busy" and this is not busy.
    #[test]
    fn a_refused_table_is_not_merely_dimmed() {
        assert_eq!(
            state_colour(&TableState::ParametersChanged),
            theme::REFUSED
        );
        assert_ne!(
            state_colour(&TableState::ParametersChanged),
            state_colour(&TableState::Full)
        );
    }
}
