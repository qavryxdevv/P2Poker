//! The drawing, which is a thin layer over the decisions in
//! [`lobby`](super::lobby).
//!
//! Nothing here decides anything. Every question a widget asks — is this table
//! joinable, which rows survive the filter, what an empty list means, what the
//! worst true thing about the connection is — is answered by a tested function
//! next door, and this file puts the answer on the screen.
//!
//! # The layout is the Python client's, and so is the way it is built
//!
//! Header, three columns, network strip: tables on the left, chat and the player
//! list in the middle, table information and the client log on the right.
//!
//! The columns are **panels** (`SidePanel::show_inside`,
//! `CentralPanel::show_inside`) rather than hand-computed fractions of the
//! available width, and that is what makes them follow the window when it is
//! resized. The first version divided `available_width()` by hand and drifted
//! out of the window the moment anything changed size.
//!
//! # Text sizes are set, not inherited
//!
//! egui's default body is 12.5 pixels, which on a high-resolution screen is what
//! *illegible* meant. The sizes here are the Python client's stylesheet: a
//! 14-pixel body, 19 for the title, 15 for a group heading, 12 for the things
//! the eye should skip.
//!
//! # The one rule that shows up as code rather than prose
//!
//! `SPEC_CS.md` §22 closes with *never display a cryptographically unverified
//! card as valid*. The lobby draws no cards at all, which is the simplest way to
//! obey it; the table window's obligation is real and is that window's.

use eframe::egui::{self, Color32, FontFamily, FontId, RichText, Stroke, TextStyle};

use super::lobby::{empty_explanation, visible, Filter, LobbyView, TableState, PASSWORD_WARNING};
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
    OpenTableWindow,
    LeaveTable,
}

/// The parts of the pane the user types into.
///
/// Separate from [`LobbyView`], which is a snapshot of what the node knows: a
/// search box is what the *user* is doing, and it has to survive the snapshot
/// being replaced several times a second.
#[derive(Debug, Clone, Default)]
pub struct LobbyUi {
    pub search: String,
    pub filter: Filter,
}

/// Install the palette and the text sizes.
pub fn install(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = theme::WINDOW;
    visuals.window_fill = theme::PANEL;
    visuals.faint_bg_color = theme::FIELD_ALT;
    visuals.extreme_bg_color = theme::FIELD;
    visuals.override_text_color = Some(theme::TEXT);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, theme::LINE);
    visuals.widgets.inactive.bg_fill = theme::PANEL_LIGHT;
    visuals.widgets.inactive.weak_bg_fill = theme::PANEL_LIGHT;
    visuals.widgets.hovered.bg_fill = theme::SELECTED;
    visuals.widgets.hovered.weak_bg_fill = theme::SELECTED;
    visuals.widgets.active.bg_fill = theme::SELECTED;
    visuals.selection.bg_fill = theme::SELECTED;
    visuals.selection.stroke = Stroke::new(1.0, theme::ACCENT);
    ctx.set_visuals(visuals);

    // egui 0.36 keeps one style per theme, so both get the same one — a client
    // whose text changed size with the system theme would be two clients.
    for theme_kind in [egui::Theme::Dark, egui::Theme::Light] {
        let mut style = (*ctx.style_of(theme_kind)).clone();
        style.text_styles = [
            (TextStyle::Heading, FontId::new(19.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = 26.0;
        ctx.set_style_of(theme_kind, style);
    }
}

fn frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(Stroke::new(1.0, theme::LINE))
        .corner_radius(8.0)
        .inner_margin(10.0)
}

/// A titled group inside a column, the way the Python client frames each one.
fn group<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.label(RichText::new(title).color(theme::TEXT_DIM).size(15.0).strong());
    ui.add_space(3.0);
    let r = add(ui);
    ui.add_space(8.0);
    r
}

/// Draw the lobby, and say what was pressed.
pub fn lobby(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi) -> LobbyAction {
    let mut action = LobbyAction::None;

    egui::Panel::top("header")
        .frame(egui::Frame::new().inner_margin(egui::Margin {
            left: 4,
            right: 4,
            top: 2,
            bottom: 8,
        }))
        .show(ui, |ui| header(ui, view));

    egui::Panel::bottom("network")
        .frame(frame())
        .show(ui, |ui| network_strip(ui, view));

    egui::Panel::left("tables")
        .resizable(true)
        .default_size(560.0)
        .min_size(380.0)
        .frame(frame())
        .show(ui, |ui| {
            action = tables_column(ui, view, state);
        });

    egui::Panel::right("info")
        .resizable(true)
        .default_size(280.0)
        .min_size(200.0)
        .frame(frame())
        .show(ui, |ui| info_column(ui, view));

    egui::CentralPanel::default()
        .frame(frame())
        .show(ui, |ui| people_column(ui, view));

    action
}

fn header(ui: &mut egui::Ui, view: &LobbyView) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Decentralised poker lobby")
                .color(theme::TEXT)
                .size(19.0)
                .strong(),
        );
        ui.add_space(14.0);
        let open = view.tables.iter().filter(|r| r.state.joinable()).count();
        ui.label(
            RichText::new(format!(
                "{} tables, {} open, {} peers",
                view.tables.len(),
                open,
                view.status.peers
            ))
            .color(theme::TEXT_DIM)
            .size(15.0),
        );
    });
}

fn tables_column(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi) -> LobbyAction {
    let mut action = LobbyAction::None;

    // Which room this is a list *of*. An empty list means two different things —
    // "quiet here" and "nowhere at all" — and a player cannot pick a reaction
    // without knowing which, so it is stated whether the list is empty or not.
    ui.label(
        RichText::new("Lobby: public")
            .color(theme::TEXT_DIM)
            .size(15.0)
            .strong(),
    );
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        let width = ui.available_width();
        ui.add(
            egui::TextEdit::singleline(&mut state.search)
                .hint_text("search tables or hosts…")
                .desired_width(width * 0.60),
        );
        egui::ComboBox::from_id_salt("filter")
            .selected_text(state.filter.label())
            .width(width * 0.30)
            .show_ui(ui, |ui| {
                for f in Filter::ALL {
                    ui.selectable_value(&mut state.filter, f, f.label());
                }
            });
    });
    ui.add_space(6.0);

    let rows = visible(&view.tables, &state.search, state.filter);

    // The buttons are placed from the bottom first, so the list takes what is
    // left. Laid out the other way round, a long list pushes them off the
    // window — which is where they went in the first version.
    egui::Panel::bottom("table-buttons")
        .frame(egui::Frame::new().inner_margin(egui::Margin {
            left: 0,
            right: 0,
            top: 8,
            bottom: 0,
        }))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Create table")
                                .color(Color32::from_rgb(6, 18, 29))
                                .strong(),
                        )
                        .fill(theme::ACCENT),
                    )
                    .clicked()
                {
                    action = LobbyAction::CreateTable;
                }

                let can_join = view.can_join();
                let join = ui.add_enabled(can_join, egui::Button::new("Join table"));
                if join.clicked() {
                    if let Some(row) = view.selected_row() {
                        action = LobbyAction::Join(row.key);
                    }
                }
                if !can_join {
                    // A grey button with no explanation is indistinguishable
                    // from a broken client, so the reason is one hover away.
                    let why = view
                        .selected_row()
                        .and_then(|r| r.state.why_not())
                        .unwrap_or("Select a table first.");
                    join.on_hover_text(why);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Leave table").clicked() {
                        action = LobbyAction::LeaveTable;
                    }
                    if ui.button("Show table window").clicked() {
                        action = LobbyAction::OpenTableWindow;
                    }
                });
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            egui::ScrollArea::both()
                .id_salt("tables")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("table-list")
                        .num_columns(8)
                        .striped(true)
                        .spacing([12.0, 5.0])
                        .min_col_width(40.0)
                        .show(ui, |ui| {
                            for h in [
                                "Table", "Players", "Type", "Stack", "Blinds", "Timing", "Host",
                                "State",
                            ] {
                                ui.label(
                                    RichText::new(h).color(theme::TEXT_DIM).size(12.0).strong(),
                                );
                            }
                            ui.end_row();

                            for row in &rows {
                                let selected = view.selected == Some(row.key);
                                let mut name = RichText::new(&row.name).color(theme::TEXT);
                                if selected {
                                    name = name.strong();
                                }
                                if ui.selectable_label(selected, name).clicked() {
                                    action = LobbyAction::Select(row.key);
                                }

                                ui.label(RichText::new(&row.occupancy).color(theme::TEXT));
                                ui.label(RichText::new(&row.game).color(theme::TEXT_DIM));
                                ui.label(RichText::new(&row.stack).color(theme::STACK));
                                ui.label(RichText::new(&row.blinds).color(theme::MONEY));
                                ui.label(RichText::new(&row.timing).color(theme::TEXT_DIM));
                                ui.label(
                                    RichText::new(&row.host).color(theme::TEXT_DIM).monospace(),
                                );

                                let state_label = ui.label(
                                    RichText::new(row.state.label())
                                        .color(state_colour(&row.state)),
                                );
                                if let Some(why) = row.state.why_not() {
                                    state_label.on_hover_text(why);
                                }
                                ui.end_row();
                            }
                        });

                    // The explanation for an empty list, and only for an empty
                    // one: which silence this is, and the action that ends it.
                    if let Some(why) =
                        empty_explanation(view.tables.len(), rows.len(), view.status.peers)
                    {
                        ui.add_space(10.0);
                        ui.label(RichText::new(why).color(theme::TEXT_DIM).italics());
                    }
                });
        });

    action
}

fn people_column(ui: &mut egui::Ui, view: &LobbyView) {
    let half = ui.available_height() * 0.55;
    ui.allocate_ui(egui::vec2(ui.available_width(), half), |ui| {
        group(ui, "Lobby chat", |ui| {
            egui::ScrollArea::vertical()
                .id_salt("chat")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &view.chat {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", line.who))
                                    .color(theme::ACCENT)
                                    .strong(),
                            );
                            // Untrusted display data, rendered as data.
                            ui.label(RichText::new(&line.said).color(theme::TEXT));
                        });
                    }
                    if view.chat.is_empty() {
                        ui.label(RichText::new("quiet").color(theme::TEXT_DIM).italics());
                    }
                });
        });
    });

    group(ui, "Players in the lobby", |ui| {
        egui::ScrollArea::vertical()
            .id_salt("players")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for name in &view.seated {
                    ui.label(RichText::new(name).color(theme::TEXT));
                }
                if view.seated.is_empty() {
                    ui.label(RichText::new("nobody yet").color(theme::TEXT_DIM).italics());
                }
            });
    });
}

fn info_column(ui: &mut egui::Ui, view: &LobbyView) {
    let top = ui.available_height() * 0.46;
    ui.allocate_ui(egui::vec2(ui.available_width(), top), |ui| {
        group(ui, "Table information", |ui| {
            egui::ScrollArea::vertical()
                .id_salt("info")
                .auto_shrink([false, false])
                .show(ui, |ui| match view.selected_row() {
                    Some(r) => {
                        field(ui, "Name", &r.name, theme::TEXT);
                        field(
                            ui,
                            "Identity",
                            &super::lobby::short_key(&r.key),
                            theme::TEXT_DIM,
                        );
                        field(ui, "Game", &r.game, theme::TEXT);
                        field(ui, "Blinds", &r.blinds, theme::MONEY);
                        field(ui, "Stack", &r.stack, theme::STACK);
                        field(ui, "Clock", &r.timing, theme::TEXT);
                        field(ui, "Seats", &r.occupancy, theme::TEXT);
                        field(ui, "State", r.state.label(), state_colour(&r.state));
                        if let Some(why) = r.state.why_not() {
                            ui.add_space(4.0);
                            ui.label(RichText::new(why).color(theme::WARN).size(12.0));
                        }
                        if r.password_required {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(PASSWORD_WARNING).color(theme::WARN).size(12.0),
                            );
                        }
                    }
                    None => {
                        ui.label(
                            RichText::new("select a table")
                                .color(theme::TEXT_DIM)
                                .italics(),
                        );
                    }
                });
        });
    });

    group(ui, "Client log", |ui| {
        egui::ScrollArea::vertical()
            .id_salt("log")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &view.log {
                    ui.label(
                        RichText::new(line)
                            .color(theme::TEXT_DIM)
                            .monospace()
                            .size(11.5),
                    );
                }
            });
    });
}

fn field(ui: &mut egui::Ui, label: &str, value: &str, colour: Color32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(theme::TEXT_DIM).size(12.0));
        ui.label(RichText::new(value).color(colour));
    });
}

/// The bottom strip: what the connection is actually doing.
fn network_strip(ui: &mut egui::Ui, view: &LobbyView) {
    ui.horizontal(|ui| {
        let s = &view.status;
        ui.label(RichText::new(s.summary()).color(status_colour(view)).strong());
        ui.add_space(16.0);

        ui.label(
            RichText::new(if s.dht_announced {
                "DHT: announced"
            } else {
                "DHT: not yet"
            })
            .color(if s.dht_announced {
                theme::OK
            } else {
                theme::TEXT_DIM
            })
            .size(12.0),
        );

        ui.label(
            RichText::new(match &s.relay {
                None => "relay: none".to_string(),
                Some(r) if r.adequate => format!("relay: {}", &r.peer[..8.min(r.peer.len())]),
                Some(_) => "relay: too small for a hand".to_string(),
            })
            .color(match &s.relay {
                None => theme::TEXT_DIM,
                Some(r) if r.adequate => theme::OK,
                Some(_) => theme::WARN,
            })
            .size(12.0),
        );

        if s.failed_dials > 0 {
            // Counted rather than listed: most dials fail on an open DHT, and a
            // log of them buries what matters — but with no count at all, "most
            // dials fail" and "this client is broken" look identical.
            ui.label(
                RichText::new(format!("{} dials failed", s.failed_dials))
                    .color(theme::TEXT_DIM)
                    .size(12.0),
            );
        }
    });
}

/// The colour of the status line, which follows the worst true thing.
pub fn status_colour(view: &LobbyView) -> Color32 {
    let s = &view.status;
    if s.relay.as_ref().is_some_and(|r| !r.adequate) {
        theme::WARN
    } else if s.public == Some(false) && s.relay.is_none() {
        theme::DANGER
    } else if s.peers == 0 {
        theme::TEXT_DIM
    } else {
        theme::OK
    }
}

/// The state column's colour, exposed so a test can compare it.
pub const fn state_colour(state: &TableState) -> Color32 {
    match state {
        TableState::Open => theme::OK,
        TableState::Full => theme::TEXT_DIM,
        TableState::ParametersChanged => theme::DANGER,
    }
}

#[cfg(test)]
mod tests {
    use super::super::lobby::{NetworkStatus, RelayStatus};
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
        assert_eq!(state_colour(&TableState::ParametersChanged), theme::DANGER);
        assert_ne!(
            state_colour(&TableState::ParametersChanged),
            state_colour(&TableState::Full)
        );
    }

    /// The status colour follows the worst true thing, in the same order the
    /// text does — so a red line and a reassuring sentence can never appear
    /// together.
    #[test]
    fn the_status_colour_follows_the_status_text() {
        let mut v = LobbyView {
            status: NetworkStatus {
                peers: 4,
                public: Some(false),
                relay: None,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(status_colour(&v), theme::DANGER);
        assert!(v.status.summary().contains("no relay"));

        v.status.relay = Some(RelayStatus {
            peer: "12D3KooWabc".into(),
            adequate: false,
        });
        assert_eq!(status_colour(&v), theme::WARN);
        assert!(v.status.summary().contains("cannot carry a hand"));

        v.status = NetworkStatus {
            peers: 4,
            public: Some(true),
            ..Default::default()
        };
        assert_eq!(status_colour(&v), theme::OK);

        v.status = NetworkStatus::default();
        assert_eq!(status_colour(&v), theme::TEXT_DIM);
    }

    /// Every colour this file puts on a panel is one the palette's legibility
    /// test covers. A colour used here and not there would be a hole in exactly
    /// the check that exists because the client was unreadable.
    #[test]
    fn every_colour_drawn_is_a_palette_colour() {
        for c in [
            theme::TEXT,
            theme::TEXT_DIM,
            theme::ACCENT,
            theme::STACK,
            theme::MONEY,
            theme::OK,
            theme::WARN,
            theme::DANGER,
        ] {
            assert!(
                theme::separation(theme::PANEL, c) >= 150,
                "a colour drawn on a panel is not legible on it"
            );
        }
    }

    /// The text sizes are set rather than inherited. egui's default body is
    /// 12.5 pixels, and on a high-resolution screen that is what *illegible*
    /// meant.
    #[test]
    fn the_text_is_bigger_than_the_default() {
        let ctx = egui::Context::default();
        install(&ctx);
        let style = ctx.style_of(egui::Theme::Dark);
        assert!(
            style.text_styles[&TextStyle::Body].size >= 14.0,
            "the body text is back to the size that was called illegible"
        );
        assert!(style.text_styles[&TextStyle::Heading].size >= 18.0);
        assert!(
            style.text_styles[&TextStyle::Small].size >= 12.0,
            "even the small text has a floor"
        );
    }

    /// Both themes carry the same sizes: a client whose text changed size with
    /// the system theme would be two different clients.
    #[test]
    fn both_themes_get_the_same_text() {
        let ctx = egui::Context::default();
        install(&ctx);
        let dark = ctx.style_of(egui::Theme::Dark);
        let light = ctx.style_of(egui::Theme::Light);
        for s in [TextStyle::Body, TextStyle::Heading, TextStyle::Small] {
            assert_eq!(dark.text_styles[&s].size, light.text_styles[&s].size);
        }
    }
}
