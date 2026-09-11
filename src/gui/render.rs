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
//! *illegible* meant. Fourteen was the next answer and it was still called too
//! small. What is here now is a 16.5-pixel body, 24 for the title, 17 for a
//! group heading and 14 for the things the eye is meant to skip — the smallest
//! text in the client is larger than the largest text the first version had for
//! a row of the table list.
//!
//! The floors are asserted at the bottom of this file. A size is one careless
//! line away from drifting back down, and this has already happened twice.
//!
//! # The one rule that shows up as code rather than prose
//!
//! `SPEC_CS.md` §22 closes with *never display a cryptographically unverified
//! card as valid*. The lobby draws no cards at all, which is the simplest way to
//! obey it; the table window's obligation is real and is that window's.

use eframe::egui::{self, Color32, FontFamily, FontId, RichText, Stroke, TextStyle};

use super::lobby::{empty_explanation, visible, Filter, LobbyView, TableState, PASSWORD_WARNING};
use crate::net::lobby::TableKind;
use crate::protocol::constants::{
    RATED_BLIND_EVERY_N_HANDS, RATED_SEATS, RATED_SMALL_BLIND, RATED_START_STACK,
};
use crate::storage::settings::Settings;
use super::theme;

/// What the user did this frame.
///
/// Returned rather than acted on, so the paint loop never touches the network
/// and §33's rule survives contact with a button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LobbyAction {
    None,
    /// Say this in the lobby.
    Say(String),
    Select([u8; 32]),
    /// Keep these settings.
    Save(Settings),
    /// Found a table with these settings.
    Create(NewTable),
    /// Sit down at a table this client has seen advertised.
    Sit {
        key: [u8; 32],
        buyin: u64,
        password: Option<Vec<u8>>,
    },
    OpenTableWindow,
    LeaveTable,
    /// `S1-CR`: rejoin the unfinished game on record, buying in with the
    /// stack the record holds (the founder answers *already seated* and the
    /// figure is not read).
    Resume { key: [u8; 32], stack: u64 },
    /// `S1-CR`: forget the unfinished game on record.
    Forget,
    /// `S1-CS`: try the join in progress again.
    RetryJoin,
    /// `S1-CS`: give the join in progress up.
    CancelJoin,
}

/// What the create dialog collects.
///
/// Everything else about a table is fixed by this version: the blinds, the
/// clock, the button rule. A field the client offers and the protocol does not
/// check is a field two clients can disagree about, so only the ones §7.2 admits
/// under `CUSTOM` are asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTable {
    /// Which game. A Sit-and-Go settles every field below it but `seats`.
    pub kind: TableKind,
    pub name: String,
    pub seats: u8,
    pub min_players: u8,
    pub buyin: u64,
    pub password: String,
}

impl Default for NewTable {
    /// The rated Sit-and-Go, because it is the one two strangers can agree on.
    ///
    /// A custom table is a set of numbers one founder chose; a rated one is the
    /// same game for everybody, and every client derives the same
    /// `table_params_hash` from the name alone. In a lobby of people who have
    /// never spoken, that is the difference between a table anyone will sit down
    /// at and a table they have to read first.
    fn default() -> Self {
        NewTable {
            kind: TableKind::SitAndGo,
            name: "New table".into(),
            // Ten, so the table a player founds without touching anything is the
            // rated one — the only table two strangers can agree on from its
            // name alone.
            seats: RATED_SEATS,
            min_players: 2,
            buyin: 1_000,
            password: String::new(),
        }
    }
}

/// What the sit-down dialog collects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitDown {
    pub key: [u8; 32],
    pub buyin: u64,
    pub password: String,
    /// Whether the table said it wants one. Shown, not enforced: the founder
    /// decides, and a client that hid the field would make a joinable table look
    /// unjoinable.
    pub wants_password: bool,
}

/// Which dialog is open, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    Create(NewTable),
    Sit(SitDown),
    /// The player's own name and how big the text is.
    Settings(Settings),
}

/// The parts of the pane the user types into.
///
/// Separate from [`LobbyView`], which is a snapshot of what the node knows: a
/// search box is what the *user* is doing, and it has to survive the snapshot
/// being replaced several times a second.
#[derive(Debug, Clone)]
pub struct LobbyUi {
    pub search: String,
    /// What the player is typing into the lobby chat, not yet sent.
    pub draft: String,
    pub filter: Filter,
    /// The dialog the player has open, if any.
    pub dialog: Option<Dialog>,
    /// What the player has chosen, as the dialogs read it.
    pub settings: Settings,
}

impl LobbyUi {
    /// The pane's own state, for a player whose settings have been loaded.
    ///
    /// No `Default`: the settings default to a name derived from **this
    /// player's key**, and a `Default::default()` would have to invent one — a
    /// name that is nobody's is worse than no default at all.
    pub fn new(settings: Settings) -> Self {
        LobbyUi {
            search: String::new(),
            draft: String::new(),
            filter: Filter::default(),
            dialog: None,
            settings,
        }
    }
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
            (TextStyle::Heading, FontId::new(24.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(16.5, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(16.5, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(15.0, FontFamily::Monospace)),
        ]
        .into();
        // Air. A dense list of rows is harder to read than a small one, and the
        // complaint that started this was about reading, not about size alone.
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(16.0, 9.0);
        style.spacing.interact_size.y = 32.0;
        style.spacing.scroll.bar_width = 11.0;
        style.visuals.widgets.inactive.corner_radius = 7.into();
        style.visuals.widgets.hovered.corner_radius = 7.into();
        style.visuals.widgets.active.corner_radius = 7.into();
        style.visuals.widgets.noninteractive.corner_radius = 7.into();
        ctx.set_style_of(theme_kind, style);
    }
}

/// A scrolling pane whose bar is always there, rather than fading in when it is
/// needed.
///
/// Not a decision about taste. `VisibleWhenNeeded` fades the bar with
/// `animate_bool_responsive`, the fade changes the width left for the content,
/// rewrapped content is a different height, and a different height wants a
/// different answer about whether a bar was needed at all. The animation never
/// settles — and an animation in flight asks egui for another frame, for ever.
///
/// On a machine with a graphics card nobody would ever find this. On the
/// software renderer a frame costs about half a second of processor time, and
/// this alone was half of what an idle client burned: 78% of a core against
/// 39%, measured. Reserving the width once ends the argument.
fn scroller(area: egui::ScrollArea) -> egui::ScrollArea {
    area.scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
}

/// A column, drawn as a card floating on the window rather than as a region of
/// it. The outer margin is what makes the three columns read as three things;
/// without it they share edges and the eye sees one grey field, which is what
/// the version this replaced looked like.
fn frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(Stroke::new(1.0, theme::LINE))
        .corner_radius(10.0)
        .inner_margin(13.0)
        .outer_margin(egui::Margin {
            left: 5,
            right: 5,
            top: 4,
            bottom: 5,
        })
}

/// A titled group inside a column, the way the Python client frames each one.
fn group<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.label(RichText::new(title).color(theme::TEXT).size(17.0).strong());
    ui.add_space(4.0);
    // A rule rather than a box: the column already has a border, and a second
    // one inside it is a frame around a frame.
    let line = ui.available_rect_before_wrap();
    ui.painter().hline(
        line.left()..=line.right(),
        line.top(),
        Stroke::new(1.0, theme::LINE),
    );
    ui.add_space(7.0);
    let r = add(ui);
    ui.add_space(14.0);
    r
}

/// Draw the lobby, and say what was pressed.
pub fn lobby(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi) -> LobbyAction {
    let mut action = LobbyAction::None;

    // `S1-CR`: an unfinished game on record is asked about before anything
    // else, in the middle of the window; the table window asks too.
    if let Some(u) = view.unfinished.as_ref() {
        if let Some(what) = unfinished_window(ui.ctx(), u) {
            action = what;
        }
    }
    // `S1-CS`: the small window that says a join is in progress, with a
    // clock on it, and says why when it ends badly.
    if let Some(j) = view.joining.as_ref() {
        if let Some(what) = joining_window(ui.ctx(), j) {
            action = what;
        }
    }
    // The chat box is in the middle column and the columns are drawn inside
    // closures, so what it produced is carried out here rather than assigned
    // through a borrow the closure does not have.
    let mut said: Option<String> = None;

    egui::Panel::top("header")
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .stroke(Stroke::new(1.0, theme::LINE))
                .corner_radius(10.0)
                .inner_margin(egui::Margin {
                    left: 16,
                    right: 16,
                    top: 11,
                    bottom: 11,
                })
                .outer_margin(egui::Margin {
                    left: 5,
                    right: 5,
                    top: 5,
                    bottom: 1,
                }),
        )
        .show(ui, |ui| {
            if header(ui, view) {
                state.dialog = Some(Dialog::Settings(state.settings.clone()));
            }
        });

    egui::Panel::bottom("network")
        .frame(frame())
        .show(ui, |ui| network_strip(ui, view));

    // Proportions rather than pixel counts: 560 and 280 are answers to one
    // window size only, and on a wide one they left the middle column too narrow
    // to fit the words "Players in the lobby" on a single line.
    //
    // The tables column gets over half, because it carries eight columns of its
    // own and the first proportion that fixed the middle column cut "Timing" and
    // "State" off the right of it.
    let across = ui.available_width();
    egui::Panel::left("tables")
        .resizable(true)
        .default_size((across * 0.55).clamp(430.0, 1_000.0))
        .min_size(360.0)
        .frame(frame())
        .show(ui, |ui| {
            action = tables_column(ui, view, state);
        });

    egui::Panel::right("info")
        .resizable(true)
        .default_size((across * 0.22).clamp(215.0, 430.0))
        .min_size(200.0)
        .frame(frame())
        .show(ui, |ui| info_column(ui, view));

    egui::CentralPanel::default()
        .frame(frame())
        .show(ui, |ui| {
            // The middle column can produce an action now, so its result is
            // taken rather than dropped. `CentralPanel::show` hands back what
            // the closure returns.
            if let LobbyAction::Say(text) = people_column(ui, view, state) {
                said = Some(text);
            }
        });

    // A line typed into the chat box beats nothing else that happened this
    // frame: the other actions come from buttons, and a button and the Enter
    // key cannot both have been pressed in one pass.
    if let Some(text) = said {
        action = LobbyAction::Say(text);
    }

    if let Some(what) = dialog(ui, state) {
        action = what;
    }

    action
}

/// `S1-CR`: the question about an unfinished game, drawn in whichever window
/// the player is looking at -- the lobby's and the table's both. The two
/// answers are the two things the node can do with the record.
pub fn unfinished_window(ctx: &egui::Context, u: &crate::app::Unfinished) -> Option<LobbyAction> {
    let mut action = None;
    egui::Window::new(RichText::new("Unfinished game").size(19.0).strong())
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(format!(
                "You left a game unfinished at {} -- seat {}, stack {}, last at hand #{}.",
                u.table_name, u.seat, u.stack, u.hand_id
            ));
            ui.label("Rejoin it? The table deals you back in at the next hand boundary.");
            ui.horizontal(|ui| {
                if ui.button("Rejoin").clicked() {
                    action = Some(LobbyAction::Resume { key: u.key, stack: u.stack });
                }
                if ui.button("Forget it").clicked() {
                    action = Some(LobbyAction::Forget);
                }
            });
        });
    action
}

/// `S1-CS`: connecting to a table -- a spinner and the seconds so far while
/// the join is open, the reason and two buttons once it has failed. The
/// window closes by itself when the seat comes: the view then carries no
/// join, and nothing is drawn.
pub fn joining_window(ctx: &egui::Context, j: &super::lobby::JoiningView) -> Option<LobbyAction> {
    let mut action = None;
    egui::Window::new(RichText::new("Connecting").size(19.0).strong())
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            match &j.failed {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("connecting to {}… {} s", j.name, j.elapsed_s));
                    });
                    // A frame every few hundred milliseconds keeps the spinner
                    // turning and the seconds honest.
                    ctx.request_repaint_after(std::time::Duration::from_millis(250));
                    if ui.button("Cancel").clicked() {
                        action = Some(LobbyAction::CancelJoin);
                    }
                }
                Some(why) => {
                    ui.label(RichText::new(format!("could not join {}", j.name)).color(theme::DANGER).strong());
                    ui.label(RichText::new(why).color(theme::TEXT));
                    ui.horizontal(|ui| {
                        if ui.button("Try again").clicked() {
                            action = Some(LobbyAction::RetryJoin);
                        }
                        if ui.button("Cancel").clicked() {
                            action = Some(LobbyAction::CancelJoin);
                        }
                    });
                }
            }
        });
    action
}

/// The create and sit-down dialogs, which are the only two places this client
/// asks a player for anything.
fn dialog(ui: &mut egui::Ui, state: &mut LobbyUi) -> Option<LobbyAction> {
    let mut action = None;
    let mut close = false;
    let mut open = state.dialog.take()?;

    let title = match &open {
        Dialog::Create(_) => "Create a table",
        Dialog::Sit(_) => "Sit down",
        Dialog::Settings(_) => "Settings",
    };

    egui::Window::new(RichText::new(title).size(19.0).strong())
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .stroke(Stroke::new(1.0, theme::LINE))
                .corner_radius(12.0)
                .inner_margin(18.0),
        )
        .show(ui.ctx(), |ui| {
            ui.set_min_width(340.0);
            match &mut open {
                Dialog::Create(f) => {
                    field_row(ui, "Name", |ui| {
                        ui.add(egui::TextEdit::singleline(&mut f.name).char_limit(32));
                    });
                    field_row(ui, "Game", |ui| {
                        ui.selectable_value(&mut f.kind, TableKind::SitAndGo, "Sit & Go");
                        ui.selectable_value(&mut f.kind, TableKind::Cash, "Cash game");
                    });

                    if f.kind == TableKind::SitAndGo {
                        field_row(ui, "Seats", |ui| {
                            ui.add(egui::Slider::new(&mut f.seats, 2..=RATED_SEATS));
                        });
                        // The numbers, stated rather than offered. A preset is a
                        // claim about values: every client derives the same
                        // parameters from the name, which is what lets two
                        // people who have never spoken agree on the game before
                        // either sits down. Offering a field here would be
                        // offering a way to make a table nobody could join.
                        preset_row(ui, "Stack", &RATED_START_STACK.to_string(), theme::STACK);
                        preset_row(
                            ui,
                            "Blinds",
                            &format!("{} / {}", RATED_SMALL_BLIND, RATED_SMALL_BLIND * 2),
                            theme::MONEY,
                        );
                        preset_row(
                            ui,
                            "Blinds double",
                            &format!("every {RATED_BLIND_EVERY_N_HANDS} hands"),
                            theme::TEXT,
                        );
                        preset_row(
                            ui,
                            "Starts",
                            &format!("when all {} seats are in", f.seats),
                            theme::TEXT,
                        );
                        ui.add_space(4.0);
                        // Which of the two it will be, and why that matters.
                        // Ten seats is the one configuration two strangers can
                        // agree on from the name; anything else is the same
                        // game with its numbers spelled out in the advert.
                        if f.seats == RATED_SEATS {
                            ui.label(
                                RichText::new(
                                    "Rated: every client computes this exact table from its \
                                     name alone, so two players who have never spoken agree \
                                     on the game before either sits down.",
                                )
                                .color(theme::OK)
                                .size(14.0),
                            );
                        } else {
                            ui.label(
                                RichText::new(format!(
                                    "The same game with {} seats. Only the ten-seat one is \
                                     rated, so this table carries its numbers in the \
                                     advertisement instead of in its name.",
                                    f.seats
                                ))
                                .color(theme::TEXT_DIM)
                                .size(14.0),
                            );
                        }
                    } else {
                        field_row(ui, "Seats", |ui| {
                            ui.add(egui::Slider::new(&mut f.seats, 2..=10));
                        });
                        field_row(ui, "Start with", |ui| {
                            ui.add(egui::Slider::new(&mut f.min_players, 2..=f.seats.max(2)));
                        });
                        field_row(ui, "Buy-in", |ui| {
                            ui.add(egui::DragValue::new(&mut f.buyin).range(200..=2_000));
                        });
                        field_row(ui, "Password", |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut f.password)
                                    .password(true)
                                    .hint_text("optional"),
                            );
                        });
                        if !f.password.is_empty() {
                            // §4.3 requires this to be said: the proof is per
                            // join and does not replay, but a weak table
                            // password is guessable offline by anyone who sees
                            // one proof.
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(super::lobby::PASSWORD_WARNING)
                                    .color(theme::WARN)
                                    .size(14.0),
                            );
                        }
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Create")
                                        .color(Color32::from_rgb(4, 16, 26))
                                        .strong(),
                                )
                                .fill(theme::ACCENT)
                                .min_size(egui::vec2(120.0, 34.0)),
                            )
                            .clicked()
                        {
                            action = Some(LobbyAction::Create(f.clone()));
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::Settings(f) => {
                    field_row(ui, "Name", |ui| {
                        // Bounded by characters here and by bytes where it is
                        // saved. This one is only to stop a player typing a
                        // paragraph; §4.3's real limit is 32 **bytes** and
                        // `Settings::repair` is what enforces it.
                        ui.add(
                            egui::TextEdit::singleline(&mut f.nickname)
                                .char_limit(super::super::storage::settings::NAME_MAX),
                        );
                    });
                    ui.label(
                        RichText::new(
                            "Shown to other players. It is never how you are identified — \
                             two people may pick the same one.",
                        )
                        .color(theme::TEXT_DIM)
                        .size(14.0),
                    );
                    ui.add_space(10.0);

                    field_row(ui, "Text size", |ui| {
                        ui.add(
                            egui::Slider::new(
                                &mut f.text_percent,
                                super::super::storage::settings::SCALE_MIN
                                    ..=super::super::storage::settings::SCALE_MAX,
                            )
                            .suffix(" %"),
                        );
                    });
                    ui.label(
                        RichText::new("Applies to both windows, at once.")
                            .color(theme::TEXT_DIM)
                            .size(14.0),
                    );

                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Save")
                                        .color(Color32::from_rgb(4, 16, 26))
                                        .strong(),
                                )
                                .fill(theme::ACCENT)
                                .min_size(egui::vec2(120.0, 34.0)),
                            )
                            .clicked()
                        {
                            action = Some(LobbyAction::Save(f.clone()));
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::Sit(f) => {
                    field_row(ui, "Buy-in", |ui| {
                        ui.add(egui::DragValue::new(&mut f.buyin));
                    });
                    if f.wants_password {
                        field_row(ui, "Password", |ui| {
                            ui.add(egui::TextEdit::singleline(&mut f.password).password(true));
                        });
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(super::lobby::PASSWORD_WARNING)
                                .color(theme::WARN)
                                .size(14.0),
                        );
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Sit down")
                                        .color(Color32::from_rgb(4, 16, 26))
                                        .strong(),
                                )
                                .fill(theme::ACCENT)
                                .min_size(egui::vec2(120.0, 34.0)),
                            )
                            .clicked()
                        {
                            action = Some(LobbyAction::Sit {
                                key: f.key,
                                buyin: f.buyin,
                                password: if f.password.is_empty() {
                                    None
                                } else {
                                    Some(f.password.as_bytes().to_vec())
                                },
                            });
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
            }
        });

    if !close {
        state.dialog = Some(open);
    }
    action
}

/// A value the preset settles, shown rather than offered.
fn preset_row(ui: &mut egui::Ui, label: &str, value: &str, colour: Color32) {
    ui.horizontal(|ui| {
        ui.add_sized(
            egui::vec2(96.0, 24.0),
            egui::Label::new(RichText::new(label).color(theme::TEXT_DIM)),
        );
        ui.label(RichText::new(value).color(colour).strong());
    });
    ui.add_space(6.0);
}

/// A labelled row in a dialog, so the labels line up without a grid.
fn field_row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized(
            egui::vec2(96.0, 24.0),
            egui::Label::new(RichText::new(label).color(theme::TEXT_DIM)),
        );
        add(ui);
    });
    ui.add_space(6.0);
}

fn header(ui: &mut egui::Ui, view: &LobbyView) -> bool {
    let mut opened = false;
    ui.horizontal(|ui| {
        // A felt-green stripe: the one place the table's colour appears in the
        // lobby, and what makes the window read as a poker client rather than
        // as a file manager.
        let (rect, _) = ui.allocate_exact_size(egui::vec2(5.0, 30.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.5, theme::SELECTED);
        ui.add_space(6.0);

        ui.label(
            RichText::new("Decentralised poker")
                .color(theme::TEXT)
                .size(24.0)
                .strong(),
        );
        ui.add_space(16.0);
        let open = view.tables.iter().filter(|r| r.state.joinable()).count();
        for (n, what, colour) in [
            (view.tables.len(), "tables", theme::TEXT_DIM),
            (open, "open", theme::OK),
            (view.status.peers, "peers", theme::ACCENT),
            // Beside it, not instead of it: "462 peers" is true about the
            // network and says nothing about who is here to play.
            (view.status.lobby_peers, "in lobby", theme::OK),
        ] {
            pill(ui, &format!("{n} {what}"), colour);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Settings").clicked() {
                opened = true;
            }
            ui.label(
                RichText::new(&view.me)
                    .color(theme::TEXT)
                    .size(16.0)
                    .strong(),
            );
        });
    });
    opened
}

/// A counter, in a rounded chip.
///
/// Three numbers inside one sentence are three numbers nobody reads; three
/// chips are three things, and the eye finds the one it wants.
fn pill(ui: &mut egui::Ui, text: &str, colour: Color32) {
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_string(), egui::FontId::proportional(15.0), colour);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 22.0, 27.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 13.0, theme::PANEL_LIGHT);
    ui.painter()
        .galley(rect.center() - galley.size() * 0.5, galley, colour);
}

fn tables_column(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi) -> LobbyAction {
    let mut action = LobbyAction::None;

    // Which room this is a list *of*. An empty list means two different things —
    // "quiet here" and "nowhere at all" — and a player cannot pick a reaction
    // without knowing which, so it is stated whether the list is empty or not.
    ui.label(
        RichText::new("Lobby: public")
            .color(theme::TEXT_DIM)
            .size(17.0)
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
            // Two rows, not one.
            //
            // The first version put the two main buttons on the left of a line
            // and the two secondary ones on the right of the same line, and
            // when the column was narrowed they were drawn on top of each
            // other: two opposed layouts in one row have no idea the other
            // exists. Two rows always fit, and they say which pair matters.
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Create table")
                                .color(Color32::from_rgb(4, 16, 26))
                                .strong(),
                        )
                        .fill(theme::ACCENT)
                        .min_size(egui::vec2(140.0, 34.0)),
                    )
                    .clicked()
                {
                    state.dialog = Some(Dialog::Create(NewTable::default()));
                }

                let can_join = view.can_join();
                let join = ui.add_enabled(
                    can_join,
                    egui::Button::new(RichText::new("Join table").strong())
                        .min_size(egui::vec2(140.0, 34.0)),
                );
                if join.clicked() {
                    if let Some(row) = view.selected_row() {
                        state.dialog = Some(Dialog::Sit(SitDown {
                            key: row.key,
                            buyin: row.default_buyin,
                            password: String::new(),
                            wants_password: row.password_required,
                        }));
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
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Show table window").clicked() {
                    action = LobbyAction::OpenTableWindow;
                }
                if ui.button("Leave table").clicked() {
                    action = LobbyAction::LeaveTable;
                }
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            scroller(egui::ScrollArea::both())
                .id_salt("tables")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("table-list")
                        .num_columns(8)
                        .striped(true)
                        .spacing([10.0, 6.0])
                        .min_col_width(32.0)
                        .show(ui, |ui| {
                            for h in [
                                "Table", "Players", "Type", "Stack", "Blinds", "Timing", "Host",
                                "State",
                            ] {
                                let r = ui.label(
                                    RichText::new(h.to_uppercase())
                                        .color(theme::TEXT_DIM)
                                        .size(13.0)
                                        .strong(),
                                );
                                ui.painter().hline(
                                    r.rect.left()..=r.rect.right(),
                                    r.rect.bottom() + 4.0,
                                    Stroke::new(1.0, theme::LINE),
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

fn people_column(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi) -> LobbyAction {
    let mut action = LobbyAction::None;
    let half = ui.available_height() * 0.55;
    ui.allocate_ui(egui::vec2(ui.available_width(), half), |ui| {
        group(ui, "Lobby chat", |ui| {
            // The box first, so the reserved height is taken off the top and
            // the scrolling history fills whatever is left. The other order
            // gives the history all of it and pushes the box off the pane.
            let line_height = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
            let history = (ui.available_height() - line_height).max(0.0);
            ui.allocate_ui(egui::vec2(ui.available_width(), history), |ui| {
            scroller(egui::ScrollArea::vertical())
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

            // Enter sends and keeps the cursor where it was, because a chat box
            // that loses focus after every line is a chat box that is used once.
            let box_id = ui.id().with("chat-draft");
            let field = ui.add(
                egui::TextEdit::singleline(&mut state.draft)
                    .id(box_id)
                    .hint_text("say something…")
                    .desired_width(f32::INFINITY),
            );
            let sent = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if sent && !state.draft.trim().is_empty() {
                action = LobbyAction::Say(std::mem::take(&mut state.draft));
                ui.memory_mut(|m| m.request_focus(box_id));
            }
        });
    });

    group(ui, "Players in the lobby", |ui| {
        scroller(egui::ScrollArea::vertical())
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

    action
}

fn info_column(ui: &mut egui::Ui, view: &LobbyView) {
    let top = ui.available_height() * 0.46;
    ui.allocate_ui(egui::vec2(ui.available_width(), top), |ui| {
        group(ui, "Table information", |ui| {
            scroller(egui::ScrollArea::vertical())
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
                            ui.label(RichText::new(why).color(theme::WARN).size(14.0));
                        }
                        if r.password_required {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(PASSWORD_WARNING).color(theme::WARN).size(14.0),
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
        scroller(egui::ScrollArea::vertical())
            .id_salt("log")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &view.log {
                    ui.label(
                        RichText::new(line)
                            .color(theme::TEXT_DIM)
                            .monospace()
                            .size(13.5),
                    );
                }
            });
    });
}

fn field(ui: &mut egui::Ui, label: &str, value: &str, colour: Color32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(theme::TEXT_DIM).size(14.0));
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
            .size(14.0),
        );

        ui.label(
            // A relay is a relay. Whether its budget carries a hand is a
            // question about a table, and it is asked in `net::relay` when
            // somebody sits down — not on a strip that reads "too small for a
            // hand" at a player who is not in one, permanently, because every
            // public relay reports the library's defaults.
            RichText::new(match &s.relay {
                None => "relay: none".to_string(),
                Some(r) => format!("relay: {}", &r.peer[..8.min(r.peer.len())]),
            })
            .color(match &s.relay {
                None => theme::TEXT_DIM,
                Some(_) => theme::OK,
            })
            .size(14.0),
        );

        if let Some(how) = s.port_mapped {
            ui.label(
                RichText::new(format!("{how}: port open"))
                    .color(theme::OK)
                    .size(14.0),
            );
        }

        if s.failed_dials > 0 {
            // Counted rather than listed: most dials fail on an open DHT, and a
            // log of them buries what matters — but with no count at all, "most
            // dials fail" and "this client is broken" look identical.
            ui.label(
                RichText::new(format!("{} dials failed", s.failed_dials))
                    .color(theme::TEXT_DIM)
                    .size(14.0),
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
        // Still amber — a relay whose budget cannot carry a hand is worth a
        // colour, even though it is no longer worth the whole sentence.
        assert_eq!(status_colour(&v), theme::WARN);
        assert!(!v.status.summary().contains("cannot carry a hand"));

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
            style.text_styles[&TextStyle::Body].size >= 16.0,
            "the body text is back down to a size that was called illegible"
        );
        assert!(style.text_styles[&TextStyle::Heading].size >= 22.0);
        assert!(
            style.text_styles[&TextStyle::Small].size >= 14.0,
            "even the smallest text in the client has a floor, and it is the size the body text used to be"
        );
        assert!(
            style.spacing.interact_size.y >= 30.0,
            "a row too short to click comfortably reads as cramped however large the letters in it are"
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
