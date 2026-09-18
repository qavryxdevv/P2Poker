//! The drawing, which is a thin layer over the decisions in
//! [`lobby`](super::lobby).
//!
//! Nothing here decides anything. Every question a widget asks — is this table
//! joinable, which rows survive the filter, what an empty list means, what the
//! worst true thing about the connection is — is answered by a tested function
//! next door, and this file puts the answer on the screen.
//!
//! # The layout is a card room's (`D-067`, 2026-09-18)
//!
//! Header on a band of the table's own felt, then columns, then one strip. The
//! left column is where a game is found: the one large gold button, the format
//! beside it, *Create table* under it, and the tables as rows that show their
//! seats rather than count them. The middle column is the people -- chat and
//! who is here -- and the right column is the player's own card and the table
//! they are looking at. Everything technical about the connection is behind
//! *Network details* on the bottom strip, with the client log; a player reads
//! *Online* and nothing else unless they ask.
//!
//! The columns are **panels** (`SidePanel::show_inside`,
//! `CentralPanel::show_inside`) rather than hand-computed fractions of the
//! available width, and that is what makes them follow the window when it is
//! resized. The first version divided `available_width()` by hand and drifted
//! out of the window the moment anything changed size. **How many columns**
//! follows the width in points ([`columns_for`]): three on a wide window, two
//! on a narrower one, and one -- the tables, with the people a button away --
//! when the text is at 200 % in a small window, where three columns had left
//! the middle one no room and drawn the buttons over the list.
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

use super::lobby::{
    empty_state, format_badge, format_chip, headline_counts, result_praise, result_words, session_words, shape_words,
    sorted, status_words, visible, Filter, LobbyView, Sort, TableRow, TableState, Tone, FAIR_PLAY, HOW_IT_WORKS,
    PASSWORD_WARNING,
};
use super::table::style;
use super::theme;
use crate::net::lobby::TableKind;
use crate::net::matchmaker::{Format, SearchRequest};
use crate::protocol::constants::{
    RATED_BLIND_EVERY_N_HANDS, RATED_SEATS, RATED_SMALL_BLIND, RATED_START_STACK,
};
use crate::storage::settings::Settings;

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
    /// `D-043`: turn to one of this client's tables, by slot.
    Focus(u8),
    /// `S1-FG`: the table asked for is one this client is at: say so.
    AlreadyAt([u8; 32]),
    /// `S1-FG`: the word taken down.
    DismissAlreadyAt,
    /// `S1-FG`: taken down, and the table's window shown.
    ShowTable(u8),
    /// `S1-CR`: rejoin the unfinished game on record. The app state holds
    /// the record's table and stack; the founder answers *already seated*
    /// and the buy-in figure is not read.
    Resume,
    /// `S1-CR`: forget the unfinished game on record -- answered at the
    /// question, or at a rejoin that failed (`S1-DE`).
    Forget,
    /// `S1-CS`: try the join in progress again.
    RetryJoin,
    /// `S1-CS`: give the join in progress up.
    CancelJoin,
    /// `D-064`: find a game automatically, as the dialog asked.
    StartSearch(SearchRequest),
    /// `D-064`: the search's one button: give it up, now.
    CancelSearch,
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

/// `D-064`: what the search dialog collects, opened on what the player chose
/// last time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchForm {
    pub format: Format,
    pub tables: u8,
    pub again: bool,
    /// What past searches took, for the dialog's word on what to expect.
    pub history: crate::storage::settings::SearchSettings,
}

impl SearchForm {
    pub fn from_settings(s: &crate::storage::settings::SearchSettings) -> Self {
        SearchForm {
            format: Format::parse(s.format).unwrap_or(Format::Any),
            tables: s.tables.clamp(1, crate::net::matchmaker::MAX_GAMES),
            again: s.again,
            history: s.clone(),
        }
    }
}

/// Which dialog is open, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    Create(NewTable),
    Sit(SitDown),
    /// The player's own name and how big the text is.
    Settings(Settings),
    /// `D-064`: the automatic search's format and games at once.
    Search(SearchForm),
    /// `D-002`, the owner's ruling (2026-09-18): the first-run word on this
    /// client relaying for other players -- on by default, and turned off here or
    /// in the settings. Shown until the player has answered it once.
    RelayNotice(Settings),
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
    /// The settings dialog's page, kept while the client runs.
    pub settings_tab: SettingsTab,
    /// `D-067`: how the list is ordered.
    pub sort: Sort,
    /// `D-067`: the format the big button searches for, opened on what the
    /// player chose last time.
    pub hero_format: Format,
    /// `D-067`: the network details and the client log, shown on the strip
    /// when asked for.
    pub diagnostics_open: bool,
    /// `D-067`: the note on what *provably fair* means, opened from the header.
    pub fair_open: bool,
    /// `D-067`: on a one-column window, the people and the player's card in
    /// place of the tables.
    pub side_open: bool,
}

/// The settings dialog's pages, one at a time: all of them on one page no longer
/// fitted the window, and the dialog was cut off (the owner, 2026-09-18).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    General,
    Sound,
    Table,
    Network,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 4] = [SettingsTab::General, SettingsTab::Sound, SettingsTab::Table, SettingsTab::Network];

    pub fn label(self) -> &'static str {
        match self {
            SettingsTab::General => "General",
            SettingsTab::Sound => "Sound",
            SettingsTab::Table => "Table",
            SettingsTab::Network => "Network",
        }
    }
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
            // `D-002`: the first run says what relaying is before anything else.
            dialog: settings.relay.is_none().then(|| Dialog::RelayNotice(settings.clone())),
            sort: Sort::default(),
            hero_format: Format::parse(settings.search().format).unwrap_or(Format::Any),
            diagnostics_open: false,
            fair_open: false,
            side_open: false,
            settings,
            settings_tab: SettingsTab::default(),
        }
    }
}

/// How many columns the lobby draws at a width in points.
///
/// Three need about a thousand points: the tables want 430 and the people and
/// the card 215 each, with air between. Two need the tables and one side.
/// Under 660 the tables alone fit, and the side is a button away. At 200 % text
/// a 1 180-pixel window is 590 points wide, which is where three columns used
/// to draw over each other.
pub const fn columns_for(across: f32) -> u8 {
    if across >= 980.0 {
        3
    } else if across >= 660.0 {
        2
    } else {
        1
    }
}

/// The colour a tone is said in.
const fn tone_colour(tone: Tone) -> Color32 {
    match tone {
        Tone::Dim => theme::TEXT_DIM,
        Tone::Ok => theme::OK,
        Tone::Accent => theme::ACCENT,
        Tone::Warn => theme::WARN,
        Tone::Danger => theme::DANGER,
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
    // Both themes. `set_visuals` sets only the theme egui is in when this
    // runs, which is the dark fallback before the first frame; on a system
    // whose apps are set to light, egui then drew with its own light visuals
    // -- white fields, light title bars under light words.
    ctx.set_visuals_of(egui::Theme::Dark, visuals.clone());
    ctx.set_visuals_of(egui::Theme::Light, visuals);

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
    // `S1-DE`: what these two windows answer is kept apart from `action`
    // and applied last. The tables column's result is assigned to `action`
    // below, and it used to be assigned over the answer -- so *Rejoin*,
    // *Forget it*, *Try again* and *Cancel* all returned nothing from here.
    let mut asked: Option<LobbyAction> = None;
    if let Some(u) = view.unfinished.as_ref() {
        asked = unfinished_window(ui.ctx(), u);
    }
    // `S1-CS`: the small window that says a join is in progress, with a
    // clock on it, and says why when it ends badly.
    if let Some(j) = view.joining.as_ref() {
        if let Some(what) = joining_window(ui.ctx(), j) {
            asked = Some(what);
        }
    }
    // `S1-FG`: the table asked for is one this client is at already.
    if let Some(a) = view.already_at.as_ref() {
        if let Some(what) = already_at_window(ui.ctx(), a) {
            asked = Some(what);
        }
    }
    // `D-064`: the search's window, over everything, while a search is on.
    if let Some(s) = view.search.as_ref() {
        if let Some(what) = search_modal(ui.ctx(), s) {
            asked = Some(what);
        }
    }
    // The chat box is in a column drawn inside a closure, so what it produced
    // is carried out here rather than assigned through a borrow the closure
    // does not have.
    let mut said: Option<String> = None;
    // `D-067`: how many columns this width takes.
    let columns = columns_for(ui.available_width());

    egui::Panel::top("header")
        .frame(
            // The table's felt, with its gold keyline: the one band of the
            // room's own colour, so the window reads as a poker client before a
            // word of it is read.
            egui::Frame::new()
                .fill(theme::FELT_EDGE)
                .stroke(Stroke::new(1.0, theme::GOLD_EDGE))
                .corner_radius(10.0)
                .inner_margin(egui::Margin {
                    left: 16,
                    right: 16,
                    top: 10,
                    bottom: 10,
                })
                .outer_margin(egui::Margin {
                    left: 5,
                    right: 5,
                    top: 5,
                    bottom: 1,
                }),
        )
        .show(ui, |ui| {
            let pressed = header(ui, view, state, columns);
            if pressed.settings {
                state.dialog = Some(Dialog::Settings(state.settings.clone()));
            }
            if pressed.fair {
                state.fair_open = !state.fair_open;
            }
            if pressed.side {
                state.side_open = !state.side_open;
            }
        });
    if state.fair_open {
        fair_window(ui.ctx(), state);
    }

    egui::Panel::bottom("network")
        .frame(frame())
        .show(ui, |ui| network_strip(ui, view, state));

    // Proportions rather than pixel counts: 560 and 280 are answers to one
    // window size only, and on a wide one they left the middle column too narrow
    // to fit the words "Players in the lobby" on a single line. The tables get
    // over half: the way into a game is there.
    let across = ui.available_width();
    match columns {
        3 => {
            egui::Panel::left("tables")
                .resizable(true)
                .default_size((across * 0.52).clamp(430.0, 1_000.0))
                .min_size(380.0)
                .frame(frame())
                .show(ui, |ui| {
                    action = tables_column(ui, view, state);
                });
            egui::Panel::right("info")
                .resizable(true)
                .default_size((across * 0.24).clamp(230.0, 430.0))
                .min_size(210.0)
                .frame(frame())
                .show(ui, |ui| info_column(ui, view, state));
            egui::CentralPanel::default().frame(frame()).show(ui, |ui| {
                // The middle column can produce an action, so its result is
                // taken rather than dropped.
                if let LobbyAction::Say(text) = people_column(ui, view, state) {
                    said = Some(text);
                }
            });
        }
        2 => {
            egui::Panel::right("side")
                .resizable(true)
                .default_size((across * 0.36).clamp(240.0, 380.0))
                .min_size(220.0)
                .frame(frame())
                .show(ui, |ui| match side_column(ui, view, state) {
                    LobbyAction::Say(text) => said = Some(text),
                    LobbyAction::None => {}
                    other => action = other,
                });
            egui::CentralPanel::default().frame(frame()).show(ui, |ui| {
                action = tables_column(ui, view, state);
            });
        }
        _ => {
            egui::CentralPanel::default().frame(frame()).show(ui, |ui| {
                if state.side_open {
                    match side_column(ui, view, state) {
                        LobbyAction::Say(text) => said = Some(text),
                        LobbyAction::None => {}
                        other => action = other,
                    }
                } else {
                    action = tables_column(ui, view, state);
                }
            });
        }
    }

    // A line typed into the chat box beats nothing else that happened this
    // frame: the other actions come from buttons, and a button and the Enter
    // key cannot both have been pressed in one pass.
    if let Some(text) = said {
        action = LobbyAction::Say(text);
    }

    if let Some(what) = dialog(ui, state) {
        action = what;
    }
    // `S1-DE`: a button in the question or the connecting window beats the
    // columns; nothing else can have been pressed in the same pass.
    if let Some(what) = asked {
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
                    action = Some(LobbyAction::Resume);
                }
                if ui.button("Forget it").clicked() {
                    action = Some(LobbyAction::Forget);
                }
            });
        });
    action
}

/// `S1-CS`: connecting to a table -- a spinner and the seconds so far while
/// the join is open, the reason and the buttons once it has failed. The
/// window closes by itself when the seat comes: the view then carries no
/// join, and nothing is drawn. `S1-DE`: the rejoin of the game on record is
/// said as such; a failed one offers to try again, to leave it for now and
/// to forget the record; one the node has given up on says why, and its one
/// button closes it.
/// `S1-DR`: the question the table window asks before it closes. `Some(true)`
/// is *leave*, `Some(false)` is *stay*, `None` is no answer yet.
pub fn exit_window(ctx: &egui::Context) -> Option<bool> {
    let mut answer = None;
    egui::Window::new("Leave the game?")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .frame(frame())
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            ui.label(
                RichText::new("Closing the table ends your game here. There is no way back to this seat.")
                    .color(theme::TEXT),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(RichText::new("Leave").color(theme::TEXT)).fill(theme::DANGER))
                    .clicked()
                {
                    answer = Some(true);
                }
                if ui.button("Stay").clicked() {
                    answer = Some(false);
                }
            });
        });
    answer
}

/// `S1-FG`: joining a table this client sits at, or joins, is refused -- said,
/// with the way to that table's window (the owner, 2026-09-15).
pub fn already_at_window(ctx: &egui::Context, a: &super::lobby::AlreadyAtView) -> Option<LobbyAction> {
    let mut action = None;
    egui::Window::new(RichText::new("Already at this table").size(19.0).strong())
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.label(match a.slot {
                Some(_) => format!("You are already sitting at {}.", a.name),
                None => format!("You are already joining {}.", a.name),
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if let Some(slot) = a.slot {
                    if ui.button("Show the table").clicked() {
                        action = Some(LobbyAction::ShowTable(slot));
                    }
                }
                if ui.button("OK").clicked() {
                    action = Some(LobbyAction::DismissAlreadyAt);
                }
            });
        });
    action
}

pub fn joining_window(ctx: &egui::Context, j: &super::lobby::JoiningView) -> Option<LobbyAction> {
    let mut action = None;
    let (title, verb) = if j.rejoin { ("Rejoining", "rejoin") } else { ("Connecting", "join") };
    egui::Window::new(RichText::new(title).size(19.0).strong())
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            match &j.failed {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(if j.rejoin {
                            format!("rejoining {}… {} s", j.name, j.elapsed_s)
                        } else {
                            format!("connecting to {}… {} s", j.name, j.elapsed_s)
                        });
                    });
                    // A frame every few hundred milliseconds keeps the spinner
                    // turning and the seconds honest.
                    crate::gui::table::paint_again(ctx, std::time::Duration::from_millis(250));
                    if ui.button("Cancel").clicked() {
                        action = Some(LobbyAction::CancelJoin);
                    }
                }
                Some(why) if j.gone => {
                    ui.label(RichText::new(format!("could not rejoin {}", j.name)).color(theme::DANGER).strong());
                    // The window's ground is light, so the reason takes the
                    // label's own colour: `theme::TEXT` is the dark panels'
                    // and was unreadable here (`shots-de/r3`).
                    ui.label(why);
                    ui.label("The game is no longer on record.");
                    if ui.button("Close").clicked() {
                        action = Some(LobbyAction::CancelJoin);
                    }
                }
                Some(why) => {
                    ui.label(RichText::new(format!("could not {verb} {}", j.name)).color(theme::DANGER).strong());
                    // The window's ground is light, so the reason takes the
                    // label's own colour: `theme::TEXT` is the dark panels'
                    // and was unreadable here (`shots-de/r3`).
                    ui.label(why);
                    if j.rejoin {
                        ui.label("The game stays on record; the question comes back at the next start.");
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Try again").clicked() {
                            action = Some(LobbyAction::RetryJoin);
                        }
                        if ui.button(if j.rejoin { "Not now" } else { "Cancel" }).clicked() {
                            action = Some(LobbyAction::CancelJoin);
                        }
                        if j.rejoin && ui.button("Forget it").clicked() {
                            action = Some(LobbyAction::Forget);
                        }
                    });
                }
            }
        });
    action
}

/// `D-064`: the search's window -- a modal over the lobby, with the clock, the
/// estimate, the queue, the seats held, the games running, the network's one
/// warning, and the one button, which cancels on the click.
///
/// Repainted four times a second while it is up: the spinner turns, the clock
/// counts, and the node's word arrives once a second on its own.
pub fn search_modal(ctx: &egui::Context, s: &super::lobby::SearchView) -> Option<LobbyAction> {
    use super::lobby::clock;
    let mut action = None;
    egui::Modal::new(egui::Id::new("search-modal"))
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .stroke(Stroke::new(1.0, theme::LINE))
                .corner_radius(12.0)
                .inner_margin(22.0),
        )
        .show(ctx, |ui| {
            ui.set_width(460.0);
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(26.0).color(theme::ACCENT));
                ui.add_space(6.0);
                ui.label(RichText::new("Searching for a game").color(theme::TEXT).size(21.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(clock(s.elapsed_s))
                            .color(theme::ACCENT)
                            .size(24.0)
                            .strong()
                            .monospace(),
                    );
                });
            });
            ui.label(
                RichText::new(format!(
                    "{} \u{00b7} {} game{} at once",
                    s.format,
                    s.games,
                    if s.games == 1 { "" } else { "s" }
                ))
                .color(theme::TEXT_DIM)
                .size(14.0),
            );
            ui.add_space(8.0);
            // The bar animates only while it has nothing to show: egui draws the
            // animation inside the bar, where it sat over the words.
            let (fraction, text) = search_progress(s);
            ui.add(egui::ProgressBar::new(fraction).animate(fraction <= 0.0).fill(theme::OK));
            ui.label(RichText::new(text).color(theme::TEXT_DIM).size(14.0));
            ui.add_space(10.0);

            let r = s.report.as_ref();
            let eta = r
                .and_then(|r| r.eta_s)
                .map(|e| format!("about {}", clock(e)))
                .or_else(|| s.typical_s.map(|t| format!("usually about {}", clock(u64::from(t)))))
                .unwrap_or_else(|| "measuring\u{2026}".to_string());
            stat_row(ui, "Estimated wait", &eta, theme::TEXT);
            let queue = match r {
                // `S1-HK`: the queue's silence before it could be heard is not a zero.
                Some(r) if !r.queue_known => "measuring\u{2026}".to_string(),
                Some(r) => match r.queue_wait_s {
                    Some(w) => format!("{} (waiting {} on average)", r.queue, clock(w)),
                    None => r.queue.to_string(),
                },
                None => "\u{2026}".to_string(),
            };
            stat_row(ui, "Players searching", &queue, theme::TEXT);
            let reserved = r.map_or("\u{2026}".to_string(), |r| {
                format!(
                    "{} table{} (looking at up to {})",
                    r.reservations.len(),
                    if r.reservations.len() == 1 { "" } else { "s" },
                    r.looking_at
                )
            });
            stat_row(ui, "Reserved at", &reserved, theme::OK);
            if let Some(r) = r {
                for x in &r.reservations {
                    ui.label(
                        RichText::new(format!(
                            "    {} \u{2014} {}/{} seated, starts at {}{}{}",
                            x.name,
                            x.players,
                            x.seats,
                            x.capacity,
                            if x.mine { " \u{00b7} yours" } else { "" },
                            if x.armed { " \u{00b7} ready" } else { "" }
                        ))
                        .color(theme::TEXT_DIM)
                        .size(14.0),
                    );
                    // `S1-HV`: what the founder waits for, so a forming table
                    // never reads as frozen.
                    if let Some(n) = x.note.as_ref() {
                        ui.label(RichText::new(format!("        {n}")).color(theme::WARN).size(13.0));
                    }
                }
            }
            let games = r.map_or("\u{2026}".to_string(), |r| format!("{} / {}", r.running, r.limit));
            stat_row(ui, "Games running", &games, theme::STACK);
            if let Some(r) = r {
                ui.add_space(4.0);
                ui.label(RichText::new(&r.phase).color(theme::TEXT_DIM).size(14.0));
                if let Some(w) = r.warning.as_ref() {
                    ui.label(RichText::new(w).color(theme::WARN).strong());
                }
            }
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("CANCEL SEARCH").color(theme::TEXT).size(18.0).strong())
                            .fill(theme::DANGER)
                            .min_size(egui::vec2(320.0, 50.0)),
                    )
                    .clicked()
                {
                    action = Some(LobbyAction::CancelSearch);
                }
            });
            crate::gui::table::paint_again(ctx, std::time::Duration::from_millis(250));
        });
    action
}

/// `D-064`: how far the search is, as a bar: the fullest seat held against the
/// seats its table needs to start; nothing yet while no seat is held.
fn search_progress(s: &super::lobby::SearchView) -> (f32, String) {
    let Some(r) = s.report.as_ref() else {
        return (0.0, "starting".to_string());
    };
    let best = r
        .reservations
        .iter()
        .filter(|x| x.capacity > 0)
        .max_by_key(|x| (u32::from(x.players) * 1_000 / u32::from(x.capacity), x.players));
    match best {
        Some(x) => (
            (f32::from(x.players) / f32::from(x.capacity)).clamp(0.0, 1.0),
            format!("{} of {} seats at the closest table", x.players.min(x.capacity), x.capacity),
        ),
        None => (0.0, "looking for tables".to_string()),
    }
}

/// One line of the search's window: a dim label, then the value.
fn stat_row(ui: &mut egui::Ui, label: &str, value: &str, colour: Color32) {
    ui.horizontal(|ui| {
        ui.add_sized(
            egui::vec2(150.0, 24.0),
            egui::Label::new(RichText::new(label).color(theme::TEXT_DIM)),
        );
        ui.label(RichText::new(value).color(colour).strong());
    });
}

/// The create and sit-down dialogs, which are the only two places this client
/// asks a player for anything.
/// `D-002`: what relaying costs the player, in the terms `NETWORK_STACK.md` §9.6
/// asks for -- the ceilings as numbers -- said by the first-run notice and under
/// the switch in the settings.
fn relay_terms() -> String {
    format!(
        "It uses your connection: at most {} connections at a time, each up to {:.1} MiB and one hour. \
         The hands themselves travel another way; what passes through is the lobby and sitting down \
         at a table. Off, nothing new passes through; what is open finishes. You can change this in \
         Settings at any time.",
        crate::net::swarm::RELAY_MAX_CIRCUITS,
        crate::net::swarm::RELAY_MAX_CIRCUIT_BYTES as f64 / (1024.0 * 1024.0)
    )
}

fn dialog(ui: &mut egui::Ui, state: &mut LobbyUi) -> Option<LobbyAction> {
    let mut action = None;
    let mut close = false;
    let mut open = state.dialog.take()?;

    let title = match &open {
        Dialog::Create(_) => "Create a table",
        Dialog::Sit(_) => "Sit down",
        Dialog::Settings(_) => "Settings",
        Dialog::Search(_) => "Find a game",
        Dialog::RelayNotice(_) => "Relaying for other players",
    };

    // Every dialog scrolls inside the window rather than running past its edge:
    // at a large text size a Sit & Go's *Create a table* is taller than a small
    // window, as the settings were at any size (2026-09-18). The body scrolls,
    // not the window -- a window that scrolls keeps a default height of its own
    // and hid the settings' buttons -- and the settings scroll their own page.
    let own_scroll = matches!(open, Dialog::Settings(_));
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
            let body_height = (ui.ctx().content_rect().height() - 110.0).max(160.0);
            let mut body = |ui: &mut egui::Ui| match &mut open {
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
                Dialog::Search(f) => {
                    ui.label(
                        RichText::new(
                            "The client looks for a Sit & Go for you: it reserves seats at the \
                             tables closest to starting, founds one of its own when nothing is \
                             on offer, lets that one start with the players who came, and gives \
                             every other seat back the moment a game starts.",
                        )
                        .color(theme::TEXT_DIM)
                        .size(14.0),
                    );
                    ui.add_space(8.0);
                    field_row(ui, "Format", |ui| {
                        for format in Format::ALL {
                            ui.selectable_value(&mut f.format, format, format.label());
                        }
                    });
                    field_row(ui, "Games at once", |ui| {
                        ui.add(egui::Slider::new(&mut f.tables, 1..=crate::net::matchmaker::MAX_GAMES));
                    });
                    ui.checkbox(&mut f.again, "Search again when the game ends");
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(match f.history.typical_s(f.format.code()) {
                            Some(t) => format!(
                                "Searches for {} took about {} before.",
                                f.format.label(),
                                super::lobby::clock(u64::from(t))
                            ),
                            None => "How long it takes depends on who else is looking; the window \
                                     shows an estimate as the search learns."
                                .to_string(),
                        })
                        .color(theme::TEXT_DIM)
                        .size(14.0),
                    );
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Start search")
                                        .color(Color32::from_rgb(4, 16, 26))
                                        .strong(),
                                )
                                .fill(theme::OK)
                                .min_size(egui::vec2(140.0, 34.0)),
                            )
                            .clicked()
                        {
                            action = Some(LobbyAction::StartSearch(SearchRequest {
                                id: 0,
                                format: f.format,
                                tables: f.tables,
                                again: f.again,
                            }));
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::Settings(f) => {
                    // The owner, 2026-09-18: one page no longer fitted the
                    // window and the dialog was cut off. Four pages, one at a
                    // time, each in a scrolling area of one height -- so the
                    // dialog keeps its size between pages, and at any text size
                    // the buttons stay on the screen.
                    ui.set_max_width(460.0);
                    ui.horizontal(|ui| {
                        for tab in SettingsTab::ALL {
                            if ui
                                .selectable_value(&mut state.settings_tab, tab, RichText::new(tab.label()).size(15.0))
                                .changed()
                            {
                                // The page's scroll bar is sized from the frame
                                // before; without a second frame a page taller
                                // than its area showed no bar until the mouse
                                // moved.
                                ui.ctx().request_repaint();
                            }
                        }
                    });
                    ui.separator();
                    let page_height = (ui.ctx().content_rect().height() - 240.0).clamp(140.0, 300.0);
                    scroller(egui::ScrollArea::vertical())
                        .id_salt("settings-page")
                        .max_height(page_height)
                        .min_scrolled_height(page_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| match state.settings_tab {
                            SettingsTab::General => {
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
                            }
                            SettingsTab::Sound => {
                                // PokerTH's sound settings (`SoundSettings.qml`): the
                                // master switch, the volume, the four categories.
                                let mut sound = f.sound();
                                ui.checkbox(&mut sound.on, "Enable sound effects");
                                ui.add_enabled_ui(sound.on, |ui| {
                                    field_row(ui, "Volume", |ui| {
                                        ui.add(egui::Slider::new(&mut sound.volume, 1..=10));
                                    });
                                    ui.checkbox(&mut sound.game_actions, "Game actions (check, call, raise ...)");
                                    ui.checkbox(&mut sound.lobby_chat, "Lobby chat notifications");
                                    ui.checkbox(&mut sound.network_game, "Network game notifications");
                                    ui.checkbox(&mut sound.blind_raise, "Blind raise notification");
                                });
                                if sound != f.sound() {
                                    f.sound = Some(sound);
                                }
                            }
                            SettingsTab::Table => {
                                // The owner, 2026-09-13: the odds beside the table's
                                // action bar, shown or hidden here, shown by default.
                                let mut show_odds = f.show_odds();
                                if ui.checkbox(&mut show_odds, "Show the odds beside the action bar").changed() {
                                    f.show_odds = Some(show_odds);
                                }
                                let mut show_chat = f.show_chat();
                                if ui.checkbox(&mut show_chat, "Show the chat beside the action bar").changed() {
                                    f.show_chat = Some(show_chat);
                                }
                                let mut auto_muck = f.auto_muck();
                                if ui
                                    .checkbox(&mut auto_muck, "Auto muck: a hand that may muck is mucked at once")
                                    .on_hover_text("Off: at a showdown your losing hand waits three seconds for Show cards")
                                    .changed()
                                {
                                    f.auto_muck = Some(auto_muck);
                                }
                            }
                            SettingsTab::Network => {
                                // `D-002`, the owner's ruling (2026-09-18): on by
                                // default, turned off here.
                                let mut relay = f.relay();
                                if ui
                                    .checkbox(&mut relay, "Relay connections for other players of this game")
                                    .changed()
                                {
                                    f.relay = Some(relay);
                                }
                                ui.add_space(4.0);
                                ui.label(RichText::new(relay_terms()).color(theme::TEXT_DIM).size(14.0));
                            }
                        });

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
                Dialog::RelayNotice(f) => {
                    ui.set_max_width(460.0);
                    ui.label(
                        RichText::new(
                            "Two players who are both behind a router that lets nothing in cannot reach \
                             each other. A client the internet can reach passes their connection through, \
                             and this one does that for players of this game — never for any other \
                             program's traffic, and only while your line can be reached from the internet.",
                        )
                        .color(theme::TEXT)
                        .size(15.0),
                    );
                    ui.add_space(6.0);
                    ui.label(RichText::new(relay_terms()).color(theme::TEXT_DIM).size(14.0));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Keep it on")
                                        .color(Color32::from_rgb(4, 16, 26))
                                        .strong(),
                                )
                                .fill(theme::ACCENT)
                                .min_size(egui::vec2(120.0, 34.0)),
                            )
                            .clicked()
                        {
                            f.relay = Some(true);
                            action = Some(LobbyAction::Save(f.clone()));
                            close = true;
                        }
                        if ui.button("Turn it off").clicked() {
                            f.relay = Some(false);
                            action = Some(LobbyAction::Save(f.clone()));
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
            };
            if own_scroll {
                body(ui);
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("dialog-body")
                    .max_height(body_height)
                    .auto_shrink([false, true])
                    .show(ui, body);
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

/// What the header's buttons asked for this frame.
struct HeaderPress {
    settings: bool,
    fair: bool,
    side: bool,
}

fn header(ui: &mut egui::Ui, view: &LobbyView, state: &LobbyUi, columns: u8) -> HeaderPress {
    let mut press = HeaderPress { settings: false, fair: false, side: false };
    // The felt: the table's own gradient over the panel's fill, painted first
    // so everything sits on it. `D-048` sampled these off the reference.
    let band = ui.max_rect().expand2(egui::vec2(16.0, 10.0));
    style::gradient_rect(ui.painter(), band, 10.0, &[(0.0, theme::FELT_MID), (1.0, theme::FELT_EDGE)]);
    // **Right to left, the Settings button first** (2026-09-18): laid out left
    // to right, the title and the counters took the width first, and at a large
    // text size in a small window the button went off the right edge -- the one
    // place the text size can be turned back down. Now the button and the player
    // are placed first, and the title and the counters get what is left, cut at
    // its edge rather than drawn over the button.
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.button("Settings").clicked() {
            press.settings = true;
        }
        // The player: the table's round avatar and the name beside it.
        ui.label(RichText::new(&view.me).color(theme::TEXT).size(16.0).strong());
        let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
        style::avatar(ui.painter(), rect, &view.me, true);
        if columns == 1 {
            // One column: the people and the card are a button away.
            let label = if state.side_open { "Tables" } else { "People" };
            if ui.button(label).clicked() {
                press.side = true;
            }
        }
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
            // The mark: a spade in a gold ring.
            let (rect, _) = ui.allocate_exact_size(egui::vec2(38.0, 38.0), egui::Sense::hover());
            let p = ui.painter();
            p.circle_filled(rect.center(), 19.0, Color32::from_black_alpha(150));
            p.circle_stroke(rect.center(), 18.0, Stroke::new(1.5, theme::GOLD_EDGE));
            p.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "\u{2660}",
                FontId::proportional(22.0),
                theme::GOLD_ACTION,
            );
            ui.add_space(4.0);
            // No room for the name beside the buttons -- 200 % in the smallest
            // window -- and the mark stands for it alone; *De…* stood for
            // nothing.
            if ui.available_width() < 200.0 {
                return;
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                // Cut, never wrapped: at 200 % in a small window the title had
                // gone to two lines and the band with it.
                ui.add(
                    egui::Label::new(RichText::new("Decentralised poker").color(theme::TEXT).size(23.0).strong())
                        .truncate(),
                );
                // The claim the room is built on, in the first line a player
                // reads; the four sentences behind it are one click away. Not
                // on a one-column window, where the band has room for the name
                // alone.
                if columns > 1 {
                    ui.add(
                        egui::Label::new(
                            RichText::new("No house. No server. Every card proven.")
                                .color(theme::ON_FELT_DIM)
                                .size(13.0),
                        )
                        .truncate(),
                    );
                }
            });
            ui.add_space(12.0);
            if pill(ui, "provably fair", theme::GOLD_ACTION, true)
                .on_hover_text("What that means, in four sentences.")
                .clicked()
            {
                press.fair = true;
            }
            // `D-067`: what is happening, in a person's words, and never a
            // zero. The DHT's peers are not players and are in the details.
            let open = view.tables.iter().filter(|r| r.state.joinable()).count();
            for (words, tone) in headline_counts(
                view.tables.len(),
                open,
                view.status.lobby_peers,
                usize::try_from(view.searching).unwrap_or(usize::MAX),
            ) {
                pill(ui, &words, tone_colour(tone), false);
            }
        });
    });
    press
}

/// A counter, in a rounded chip on the felt.
///
/// Three numbers inside one sentence are three numbers nobody reads; three
/// chips are three things, and the eye finds the one it wants.
fn pill(ui: &mut egui::Ui, text: &str, colour: Color32, clickable: bool) -> egui::Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), egui::FontId::proportional(14.0), Color32::PLACEHOLDER);
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 22.0, 27.0),
        if clickable { egui::Sense::click() } else { egui::Sense::hover() },
    );
    let fill = if clickable && resp.hovered() {
        Color32::from_black_alpha(60)
    } else {
        Color32::from_black_alpha(120)
    };
    ui.painter().rect_filled(rect, 13.0, fill);
    if clickable {
        ui.painter().rect_stroke(rect, 13.0, Stroke::new(1.0, theme::GOLD_EDGE), egui::StrokeKind::Inside);
    }
    ui.painter().galley(rect.center() - galley.size() * 0.5, galley, colour);
    resp
}

/// `D-067`: what *provably fair* means, opened from the header's chip.
fn fair_window(ctx: &egui::Context, state: &mut LobbyUi) {
    egui::Window::new(RichText::new("Provably fair \u{2014} no house").size(19.0).strong())
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 96.0))
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .stroke(Stroke::new(1.0, theme::GOLD_EDGE))
                .corner_radius(12.0)
                .inner_margin(18.0),
        )
        .show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.label(RichText::new(FAIR_PLAY).color(theme::TEXT).size(15.0));
            ui.add_space(8.0);
            if ui.button("Got it").clicked() {
                state.fair_open = false;
            }
        });
}

/// The one large gold button: the table's gold, lit from the top, the ink
/// dark on it. Everything else on the screen is quieter than this on purpose.
fn gold_button(ui: &mut egui::Ui, label: &str, size: egui::Vec2) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let p = ui.painter();
    let lift = if resp.hovered() { 0.12 } else { 0.0 };
    let top = style::mix(theme::GOLD_ACTION, Color32::WHITE, 0.08 + lift);
    let bottom = style::mix(theme::GOLD_EDGE, Color32::BLACK, 0.04);
    style::shadow(p, rect, 10.0, 3.0, 8.0, Color32::from_black_alpha(100));
    style::gradient_rect(p, rect, 10.0, &[(0.0, top), (1.0, bottom)]);
    p.rect_stroke(
        rect,
        10.0,
        Stroke::new(1.0, style::mix(theme::GOLD_ACTION, Color32::WHITE, 0.35)),
        egui::StrokeKind::Inside,
    );
    style::text(
        p,
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        18.0,
        style::Weight::Bold,
        theme::INK_ON_GOLD,
    );
    resp
}

/// A choice in a row of them: the chosen one in the felt's green with a gold
/// edge, the rest quiet.
fn chip(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(14.0), Color32::PLACEHOLDER);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(galley.size().x + 22.0, 28.0), egui::Sense::click());
    let (fill, ink, edge) = if selected {
        (theme::SELECTED, theme::GOLD_ACTION, theme::GOLD_EDGE)
    } else if resp.hovered() {
        (theme::PANEL_LIGHT, theme::TEXT, theme::LINE)
    } else {
        (theme::PANEL, theme::TEXT_DIM, theme::LINE)
    };
    let p = ui.painter();
    p.rect_filled(rect, 14.0, fill);
    p.rect_stroke(rect, 14.0, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
    p.galley(rect.center() - galley.size() * 0.5, galley, ink);
    resp
}

/// A small tinted word on a row: the game.
fn badge(ui: &mut egui::Ui, text: &str, colour: Color32) -> egui::Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(13.0), colour);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(galley.size().x + 14.0, 22.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 11.0, colour.gamma_multiply(0.16));
    p.rect_stroke(rect, 11.0, Stroke::new(1.0, colour.gamma_multiply(0.5)), egui::StrokeKind::Inside);
    p.galley(rect.center() - galley.size() * 0.5, galley, colour);
    resp
}

/// The seats of a table, drawn: a gold disc per player, a ring per empty
/// seat. A seat taken since the last frame fills over a third of a second
/// (`D-067`: a table filling is a thing to watch, and it is the truth).
fn seat_dots(ui: &mut egui::Ui, row: &TableRow) {
    let seats = row.seats.clamp(1, 10);
    let d = 10.0;
    let gap = 3.0;
    let width = f32::from(seats) * (d + gap) - gap;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 16.0), egui::Sense::hover());
    let shown = ui
        .ctx()
        .animate_value_with_time(egui::Id::new(("seats", row.key)), f32::from(row.seated), 0.35);
    let colour = if row.state.joinable() { theme::GOLD_ACTION } else { theme::TEXT_DIM };
    let p = ui.painter();
    for i in 0..seats {
        let c = egui::pos2(rect.left() + d / 2.0 + f32::from(i) * (d + gap), rect.center().y);
        let fill = (shown - f32::from(i)).clamp(0.0, 1.0);
        if fill > 0.0 {
            p.circle_filled(c, d / 2.0 * (0.6 + 0.4 * fill), colour.gamma_multiply(0.5 + 0.5 * fill));
        }
        // An empty seat's ring in a half-strength dim, which shows on the
        // list's field and on a selected row's green alike; the line colour
        // vanished on the green.
        p.circle_stroke(
            c,
            d / 2.0 - 0.5,
            Stroke::new(1.0, if fill > 0.0 { colour } else { theme::TEXT_DIM.gamma_multiply(0.55) }),
        );
    }
    resp.on_hover_text(format!("{} of {} seats taken", row.seated, row.seats));
    ui.label(
        RichText::new(format!("{}/{}", row.seated, row.seats))
            .color(theme::TEXT_DIM)
            .size(13.0),
    );
}

/// The empty list: an invitation in the middle of the space, with the suits
/// as a quiet mark of what room this is.
fn empty(ui: &mut egui::Ui, e: super::lobby::EmptyState) {
    ui.add_space(28.0);
    let width = ui.available_width().min(460.0);
    let left = ui.max_rect().center().x - width / 2.0;
    let rect = egui::Rect::from_min_size(egui::pos2(left, ui.cursor().top()), egui::vec2(width, 10.0));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
        |ui| {
            ui.label(
                RichText::new("\u{2660}   \u{2665}   \u{2666}   \u{2663}")
                    .color(theme::TEXT_DIM.gamma_multiply(0.45))
                    .size(26.0),
            );
            ui.add_space(6.0);
            if e.connecting {
                ui.add(egui::Spinner::new().size(22.0).color(theme::GOLD_EDGE));
                super::table::paint_again(ui.ctx(), std::time::Duration::from_millis(250));
                ui.add_space(4.0);
            }
            ui.label(RichText::new(e.title).color(theme::TEXT).size(19.0).strong());
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(e.body).color(theme::TEXT_DIM)).wrap());
        },
    );
}

/// `D-067`: the way in, first. The format, the one large button, *Create
/// table* beside it, and an honest line on what to expect.
fn hero(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi) -> LobbyAction {
    let mut action = LobbyAction::None;
    egui::Frame::new()
        .fill(theme::FIELD)
        .stroke(Stroke::new(1.0, theme::LINE))
        .corner_radius(10.0)
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Play now").color(theme::TEXT).size(17.0).strong());
                ui.add_space(6.0);
                for f in Format::ALL {
                    if chip(ui, format_chip(f), state.hero_format == f).clicked() {
                        state.hero_format = f;
                    }
                }
            });
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                // One click: the search starts with the format chosen here and
                // what the player asked for last time about games at once.
                if gold_button(ui, "FIND A GAME", egui::vec2(230.0, 48.0))
                    .on_hover_text(
                        "The client reserves seats at the tables closest to starting and founds \
                         one when nothing is on offer. You are seated the moment a game starts.",
                    )
                    .clicked()
                {
                    let s = state.settings.search();
                    action = LobbyAction::StartSearch(SearchRequest {
                        id: 0,
                        format: state.hero_format,
                        tables: s.tables,
                        again: s.again,
                    });
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new("Create table").color(theme::GOLD_EDGE).strong())
                            .stroke(Stroke::new(1.0, theme::GOLD_EDGE))
                            .fill(theme::PANEL)
                            .min_size(egui::vec2(150.0, 48.0)),
                    )
                    .on_hover_text("Found a table of your own, for friends or for anybody.")
                    .clicked()
                {
                    state.dialog = Some(Dialog::Create(NewTable::default()));
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new("More options\u{2026}").color(theme::TEXT_DIM).size(14.0))
                            .frame(false),
                    )
                    .on_hover_text("Games at once, and searching again when a game ends.")
                    .clicked()
                {
                    state.dialog = Some(Dialog::Search(SearchForm::from_settings(&state.settings.search())));
                }
            });
            ui.add_space(4.0);
            let note = super::lobby::hero_note(
                state.settings.search().typical_s(state.hero_format.code()),
                state.hero_format,
                view.searching,
            );
            ui.add(egui::Label::new(RichText::new(note).color(theme::TEXT_DIM).size(14.0)).wrap());
        });
    action
}

fn tables_column(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi) -> LobbyAction {
    let mut action = LobbyAction::None;

    // `D-043`: every table this client sits at, the active one lit, the
    // one whose turn it is marked; a click turns the table window to it.
    if view.my_tables.len() > 1 {
        egui::Panel::bottom("my-tables")
            .show_separator_line(false)
            .frame(egui::Frame::new().inner_margin(egui::Margin {
                left: 0,
                right: 0,
                top: 6,
                bottom: 0,
            }))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Your tables").color(theme::TEXT_DIM));
                    for s in &view.my_tables {
                        let label = if s.turn { format!("{} \u{25cf} your turn", s.name) } else { s.name.clone() };
                        let button = egui::Button::new(RichText::new(label).color(if s.turn {
                            theme::OK
                        } else {
                            theme::TEXT
                        }))
                        .selected(s.active);
                        if ui.add(button).clicked() {
                            action = LobbyAction::Focus(s.slot);
                        }
                    }
                });
            });
    }
    // `S1-DR`: no buttons for the table here. The table window opens
    // with the seat, and closing it is leaving -- the window asks.

    // The way in first, in a panel of its own, so the list takes what is left
    // and nothing is ever drawn over it -- which is where the buttons went at
    // 200 % text in a small window when they were placed from the bottom.
    // Too short for the way in and the list both -- 200 % text in a small
    // window -- and the column scrolls as one instead, the way in at its top.
    let compact = ui.available_height() < 420.0;
    if compact {
        scroller(egui::ScrollArea::vertical())
            .id_salt("tables-all")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let LobbyAction::StartSearch(req) = hero(ui, view, state) {
                    action = LobbyAction::StartSearch(req);
                }
                ui.add_space(8.0);
                if let Some(what) = table_list(ui, view, state) {
                    action = what;
                }
            });
    } else {
        egui::Panel::top("hero")
            .show_separator_line(false)
            .frame(egui::Frame::new().inner_margin(egui::Margin {
                left: 0,
                right: 0,
                top: 0,
                bottom: 8,
            }))
            .show(ui, |ui| {
                if let LobbyAction::StartSearch(req) = hero(ui, view, state) {
                    action = LobbyAction::StartSearch(req);
                }
            });
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            list_head(ui, state);
            scroller(egui::ScrollArea::vertical())
                .id_salt("tables")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(what) = table_rows_and_empty(ui, view, state) {
                        action = what;
                    }
                });
        });
    }

    action
}

/// The list's head: its name and the search on one line, the filter and the
/// order on the next, each half the width -- two lines that always fit, where
/// one wrapped line was cut at the column's edge in a small window.
fn list_head(ui: &mut egui::Ui, state: &mut LobbyUi) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Tables").color(theme::TEXT).size(17.0).strong());
        ui.add(
            egui::TextEdit::singleline(&mut state.search)
                .hint_text("search tables or hosts\u{2026}")
                .desired_width(ui.available_width()),
        );
    });
    ui.horizontal(|ui| {
        let half = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(90.0);
        egui::ComboBox::from_id_salt("filter")
            .selected_text(state.filter.label())
            .width(half)
            .show_ui(ui, |ui| {
                for f in Filter::ALL {
                    ui.selectable_value(&mut state.filter, f, f.label());
                }
            });
        egui::ComboBox::from_id_salt("sort")
            .selected_text(state.sort.label())
            .width(half)
            .show_ui(ui, |ui| {
                for s in Sort::ALL {
                    ui.selectable_value(&mut state.sort, s, s.label());
                }
            });
    });
    ui.add_space(6.0);
}

/// The head and the list together, for the column that scrolls as one.
fn table_list(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi) -> Option<LobbyAction> {
    list_head(ui, state);
    table_rows_and_empty(ui, view, state)
}

/// The rows that survive the search, the filter and the order -- and, for an
/// empty list, which silence this is and the action that ends it.
fn table_rows_and_empty(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi) -> Option<LobbyAction> {
    let rows = sorted(visible(&view.tables, &state.search, state.filter), state.sort);
    let action = table_rows(ui, view, state, &rows);
    if let Some(e) = empty_state(view.tables.len(), rows.len(), view.status.peers) {
        empty(ui, e);
    }
    action
}

/// The sit-down dialog for a row, opened from its button or a double click.
fn sit_dialog(row: &TableRow) -> Dialog {
    Dialog::Sit(SitDown {
        key: row.key,
        buyin: row.default_buyin,
        password: String::new(),
        wants_password: row.password_required,
    })
}

/// The tables, one card each: the name and the game on the first line with
/// the seats drawn and what the table is doing; the numbers on the second.
/// The whole card selects, a double click sits down, and the selected card
/// carries its own *Join* -- the button is beside the table it joins.
fn table_rows(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi, rows: &[&TableRow]) -> Option<LobbyAction> {
    let mut action = None;
    for row in rows {
        let selected = view.selected == Some(row.key);
        let here = view.here.contains(&row.key);
        let inner = ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt(("table-row", row.key))
                .sense(egui::Sense::click()),
            |ui| {
                let resp = ui.response();
                let fill = if selected {
                    theme::SELECTED
                } else if resp.hovered() {
                    theme::PANEL_LIGHT
                } else {
                    theme::FIELD
                };
                let edge = if selected { Stroke::new(1.0, theme::GOLD_EDGE) } else { Stroke::NONE };
                egui::Frame::new()
                    .fill(fill)
                    .stroke(edge)
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let join_w = if selected && row.state.joinable() && !here { 92.0 } else { 0.0 };
                            let width = (ui.available_width() - join_w).max(120.0);
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, 28.0),
                                egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
                                |ui| {
                                    ui.set_max_width(width);
                                    ui.label(RichText::new(&row.name).color(theme::TEXT).size(16.5).strong());
                                    let (word, tip) = format_badge(row);
                                    badge(ui, word, if row.sit_and_go { theme::GOLD_ACTION } else { theme::OK })
                                        .on_hover_text(tip);
                                    ui.label(RichText::new(shape_words(row.seats)).color(theme::TEXT_DIM).size(14.0));
                                    seat_dots(ui, row);
                                    let tone = match row.state {
                                        TableState::Open if row.seated < row.needed => Tone::Warn,
                                        TableState::Open => Tone::Ok,
                                        TableState::Full => Tone::Dim,
                                        TableState::ParametersChanged => Tone::Danger,
                                    };
                                    let status = ui.label(
                                        RichText::new(status_words(row)).color(tone_colour(tone)).size(14.0),
                                    );
                                    if let Some(why) = row.state.why_not() {
                                        status.on_hover_text(why);
                                    }
                                    if row.password_required {
                                        // A word and not a lock glyph: the fonts here have
                                        // no lock, and a word teaches what a glyph assumes.
                                        ui.label(RichText::new("password").color(theme::WARN).size(14.0))
                                            .on_hover_text("This table asks for a password.");
                                    }
                                    if here {
                                        ui.label(RichText::new("you are here").color(theme::WARN).size(14.0));
                                    }
                                },
                            );
                            if join_w > 0.0 {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new("Join").color(theme::INK_ON_GOLD).strong(),
                                            )
                                            .fill(theme::OK)
                                            .min_size(egui::vec2(72.0, 30.0)),
                                        )
                                        .clicked()
                                    {
                                        state.dialog = Some(sit_dialog(row));
                                    }
                                });
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            ui.label(RichText::new("blinds").color(theme::TEXT_DIM).size(13.0));
                            ui.label(RichText::new(&row.blinds).color(theme::MONEY).size(14.0))
                                .on_hover_text("The small and the big blind: the two forced bets that start every hand.");
                            ui.label(RichText::new("\u{00b7}").color(theme::TEXT_DIM));
                            ui.label(
                                RichText::new(if row.sit_and_go { "stack" } else { "buy-in" })
                                    .color(theme::TEXT_DIM)
                                    .size(13.0),
                            );
                            ui.label(RichText::new(&row.stack).color(theme::STACK).size(14.0));
                            ui.label(RichText::new("\u{00b7}").color(theme::TEXT_DIM));
                            ui.label(
                                RichText::new(format!("{} s to act", row.action_s))
                                    .color(theme::TEXT_DIM)
                                    .size(13.0),
                            )
                            .on_hover_text(format!("The clock a player gets to act: {}.", row.timing));
                            ui.label(RichText::new("\u{00b7}").color(theme::TEXT_DIM));
                            ui.label(
                                RichText::new(format!("host {}", row.host))
                                    .color(theme::TEXT_DIM)
                                    .size(13.0)
                                    .monospace(),
                            );
                        });
                    });
            },
        );
        let resp = inner.response;
        if resp.clicked() {
            action = Some(LobbyAction::Select(row.key));
        }
        if resp.double_clicked() {
            // `S1-FG`: not a table this client is at: the lobby says so
            // instead of opening the dialog.
            if here {
                action = Some(LobbyAction::AlreadyAt(row.key));
            } else if row.state.joinable() {
                state.dialog = Some(sit_dialog(row));
            }
        }
        ui.add_space(4.0);
    }
    action
}

/// The chat's history, scrolling, stuck to the newest line.
fn chat_history(ui: &mut egui::Ui, view: &LobbyView) {
    scroller(egui::ScrollArea::vertical())
        .id_salt("chat")
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for line in &view.chat {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{}:", line.who)).color(theme::ACCENT).strong());
                    // Untrusted display data, rendered as data.
                    ui.label(RichText::new(&line.said).color(theme::TEXT));
                });
            }
            if view.chat.is_empty() {
                ui.label(RichText::new("Quiet for now. Say hello.").color(theme::TEXT_DIM).italics());
            }
        });
}

/// The chat's box. Enter sends and keeps the cursor where it was, because a
/// chat box that loses focus after every line is a chat box that is used once.
fn chat_box(ui: &mut egui::Ui, state: &mut LobbyUi) -> Option<String> {
    let box_id = ui.id().with("chat-draft");
    let field = ui.add(
        egui::TextEdit::singleline(&mut state.draft)
            .id(box_id)
            .hint_text("say something\u{2026}")
            .desired_width(f32::INFINITY),
    );
    let sent = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if sent && !state.draft.trim().is_empty() {
        ui.memory_mut(|m| m.request_focus(box_id));
        return Some(std::mem::take(&mut state.draft));
    }
    None
}

/// Who is in the lobby: the table's avatar and the name, as they said it.
/// No state beside a name -- the lobby knows who is here and not what they
/// are doing, and it says only what it knows.
fn players_list(ui: &mut egui::Ui, view: &LobbyView) {
    for name in &view.seated {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
            style::avatar(ui.painter(), rect, name, true);
            ui.label(RichText::new(name).color(theme::TEXT));
        });
    }
    if view.seated.is_empty() {
        ui.label(RichText::new("Nobody else yet.").color(theme::TEXT_DIM).italics());
    }
}

fn players_heading(view: &LobbyView) -> String {
    if view.seated.is_empty() {
        "Players in the lobby".to_string()
    } else {
        format!("Players in the lobby \u{00b7} {}", view.seated.len())
    }
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
            ui.allocate_ui(egui::vec2(ui.available_width(), history), |ui| chat_history(ui, view));
            if let Some(text) = chat_box(ui, state) {
                action = LobbyAction::Say(text);
            }
        });
    });

    group(ui, &players_heading(view), |ui| {
        scroller(egui::ScrollArea::vertical())
            .id_salt("players")
            .auto_shrink([false, false])
            .show(ui, |ui| players_list(ui, view));
    });

    action
}

/// `D-067`: the player's own card -- the avatar, the name, how long they have
/// been here, and their record from this profile. Nothing on it is a ranking
/// of anybody else; nothing on it asks for anything back.
fn you_card(ui: &mut egui::Ui, view: &LobbyView) {
    egui::Frame::new()
        .fill(theme::FIELD)
        .stroke(Stroke::new(1.0, theme::LINE))
        .corner_radius(10.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(46.0, 46.0), egui::Sense::hover());
                style::avatar(ui.painter(), rect, &view.me, true);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(&view.me).color(theme::TEXT).size(18.0).strong());
                    ui.label(
                        RichText::new(format!("here for {}", session_words(view.session_s)))
                            .color(theme::TEXT_DIM)
                            .size(13.0),
                    );
                });
            });
            ui.add_space(8.0);
            match view.record.as_ref() {
                Some(r) if r.games > 0 => {
                    ui.horizontal_wrapped(|ui| {
                        stat(ui, "Sit & Gos", &r.games.to_string(), theme::TEXT);
                        stat(ui, "Wins", &r.wins.to_string(), theme::GOLD_ACTION);
                        if let Some(b) = r.best_place {
                            stat(ui, "Best", &super::table::ordinal(usize::from(b)), theme::STACK);
                        }
                    });
                    if let Some(last) = r.last.as_ref() {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Last game").color(theme::TEXT_DIM).size(13.0));
                        ui.add(
                            egui::Label::new(
                                RichText::new(result_words(last))
                                    .color(if last.won() { theme::GOLD_ACTION } else { theme::TEXT })
                                    .size(15.0),
                            )
                            .wrap(),
                        );
                        if let Some(p) = result_praise(last) {
                            ui.label(RichText::new(p).color(theme::OK).size(14.0));
                        }
                    }
                }
                _ => {
                    ui.add(
                        egui::Label::new(
                            RichText::new("No games on record yet. Your first Sit & Go is one click away.")
                                .color(theme::TEXT_DIM)
                                .size(14.0),
                        )
                        .wrap(),
                    );
                }
            }
        });
}

/// One number on the card, with its word under it.
fn stat(ui: &mut egui::Ui, label: &str, value: &str, colour: Color32) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.label(RichText::new(value).color(colour).size(20.0).strong());
        ui.label(RichText::new(label).color(theme::TEXT_DIM).size(12.0));
    });
    ui.add_space(10.0);
}

/// The three steps, for the space a table's details take once one is chosen.
fn how_it_works(ui: &mut egui::Ui) {
    ui.label(RichText::new("How it works").color(theme::TEXT).size(15.0).strong());
    ui.add_space(4.0);
    for (i, (head, rest)) in HOW_IT_WORKS.iter().enumerate() {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
            let p = ui.painter();
            p.circle_filled(rect.center(), 12.0, theme::SELECTED);
            p.circle_stroke(rect.center(), 11.5, Stroke::new(1.0, theme::GOLD_EDGE));
            p.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                (i + 1).to_string(),
                FontId::proportional(13.0),
                theme::GOLD_ACTION,
            );
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add(egui::Label::new(RichText::new(*head).color(theme::TEXT).size(14.5).strong()).wrap());
                ui.add(egui::Label::new(RichText::new(*rest).color(theme::TEXT_DIM).size(13.5)).wrap());
            });
        });
        ui.add_space(6.0);
    }
    ui.add_space(4.0);
    ui.label(
        RichText::new("Select a table to see its details here.")
            .color(theme::TEXT_DIM)
            .size(13.0)
            .italics(),
    );
}

/// The chosen table's details, and its own way in.
fn table_info(ui: &mut egui::Ui, r: &TableRow, view: &LobbyView, state: &mut LobbyUi) {
    let here = view.here.contains(&r.key);
    ui.add(egui::Label::new(RichText::new(&r.name).color(theme::TEXT).size(18.0).strong()).wrap());
    ui.horizontal_wrapped(|ui| {
        let (word, tip) = format_badge(r);
        badge(ui, word, if r.sit_and_go { theme::GOLD_ACTION } else { theme::OK }).on_hover_text(tip);
        ui.label(RichText::new(shape_words(r.seats)).color(theme::TEXT_DIM).size(14.0));
        if r.password_required {
            ui.label(RichText::new("password").color(theme::WARN).size(14.0));
        }
    });
    ui.add_space(6.0);
    field(ui, "Blinds", &r.blinds, theme::MONEY);
    field(ui, if r.sit_and_go { "Stack" } else { "Buy-in" }, &r.stack, theme::STACK);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Seats").color(theme::TEXT_DIM).size(14.0));
        seat_dots(ui, r);
    });
    field(ui, "Clock", &r.timing, theme::TEXT);
    field(ui, "Host", &r.host, theme::TEXT_DIM);
    field(ui, "Identity", &super::lobby::short_key(&r.key), theme::TEXT_DIM);
    let tone = match r.state {
        TableState::Open if r.seated < r.needed => Tone::Warn,
        TableState::Open => Tone::Ok,
        TableState::Full => Tone::Dim,
        TableState::ParametersChanged => Tone::Danger,
    };
    field(ui, "State", &status_words(r), tone_colour(tone));
    if let Some(why) = r.state.why_not() {
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(why).color(theme::WARN).size(14.0)).wrap());
    }
    if r.password_required {
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(PASSWORD_WARNING).color(theme::WARN).size(14.0)).wrap());
    }
    ui.add_space(8.0);
    if here {
        ui.label(RichText::new("You are at this table.").color(theme::WARN));
    } else if r.state.joinable() {
        if ui
            .add(
                egui::Button::new(RichText::new("Join this table").color(theme::INK_ON_GOLD).strong())
                    .fill(theme::OK)
                    .min_size(egui::vec2(150.0, 34.0)),
            )
            .clicked()
        {
            state.dialog = Some(sit_dialog(r));
        }
    }
}

fn info_column(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi) {
    you_card(ui, view);
    ui.add_space(10.0);
    group(ui, "Table", |ui| {
        scroller(egui::ScrollArea::vertical())
            .id_salt("info")
            .auto_shrink([false, false])
            .show(ui, |ui| match view.selected_row() {
                Some(r) => table_info(ui, r, view, state),
                None => how_it_works(ui),
            });
    });
}

/// The people and the card in one scrolling column, for a window too narrow
/// for three.
fn side_column(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi) -> LobbyAction {
    let mut action = LobbyAction::None;
    scroller(egui::ScrollArea::vertical())
        .id_salt("side")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            you_card(ui, view);
            ui.add_space(10.0);
            group(ui, "Table", |ui| match view.selected_row() {
                Some(r) => table_info(ui, r, view, state),
                None => how_it_works(ui),
            });
            group(ui, "Lobby chat", |ui| {
                ui.allocate_ui(egui::vec2(ui.available_width(), 180.0), |ui| chat_history(ui, view));
                if let Some(text) = chat_box(ui, state) {
                    action = LobbyAction::Say(text);
                }
            });
            group(ui, &players_heading(view), |ui| players_list(ui, view));
        });
    action
}

fn field(ui: &mut egui::Ui, label: &str, value: &str, colour: Color32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(theme::TEXT_DIM).size(14.0));
        ui.label(RichText::new(value).color(colour));
    });
}

/// The bottom strip: one light and one word a player reads -- and, when they
/// ask, everything the connection is doing, with the client log (`D-067`).
fn network_strip(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi) {
    let s = &view.status;
    ui.horizontal(|ui| {
        let (word, tone) = s.headline();
        let colour = tone_colour(tone);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 5.0, colour);
        if tone == Tone::Ok {
            ui.painter().circle_stroke(rect.center(), 6.5, Stroke::new(1.0, colour.gamma_multiply(0.4)));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if state.diagnostics_open { "Hide details" } else { "Network details" };
            if ui
                .add(egui::Button::new(RichText::new(label).color(theme::TEXT_DIM).size(14.0)).frame(false))
                .clicked()
            {
                state.diagnostics_open = !state.diagnostics_open;
                ui.ctx().request_repaint();
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(RichText::new(word).color(colour).strong()).truncate());
            });
        });
    });

    if !state.diagnostics_open {
        return;
    }
    ui.add_space(4.0);
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(s.summary()).color(status_colour(view)).size(14.0));
        ui.label(
            RichText::new(if s.dht_announced { "DHT: announced" } else { "DHT: not yet" })
                .color(if s.dht_announced { theme::OK } else { theme::TEXT_DIM })
                .size(14.0),
        );
        ui.label(
            // A relay is a relay. Whether its budget carries a hand is a
            // question about a table, and it is asked in `net::relay` when
            // somebody sits down -- not on a strip that reads "too small for a
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
            ui.label(RichText::new(format!("{how}: port open")).color(theme::OK).size(14.0));
        }
        // `D-002` point 3 (`S1-FK`): what this client's own line carries for
        // other players, shown whenever it carries anything.
        let (reserved, circuits) = s.relaying;
        if reserved > 0 || circuits > 0 {
            ui.label(
                RichText::new(format!(
                    "relaying for {reserved} player{}, {circuits} connection{}",
                    if reserved == 1 { "" } else { "s" },
                    if circuits == 1 { "" } else { "s" }
                ))
                .color(theme::TEXT_DIM)
                .size(14.0),
            );
        }
        if s.failed_dials > 0 {
            // Counted rather than listed: most dials fail on an open DHT, and a
            // log of them buries what matters -- but with no count at all, "most
            // dials fail" and "this client is broken" look identical.
            ui.label(
                RichText::new(format!("{} dials failed", s.failed_dials))
                    .color(theme::TEXT_DIM)
                    .size(14.0),
            );
        }
    });
    ui.add_space(4.0);
    ui.label(RichText::new("Client log").color(theme::TEXT_DIM).size(13.0).strong());
    let height = (ui.ctx().content_rect().height() * 0.28).clamp(70.0, 220.0);
    scroller(egui::ScrollArea::vertical())
        .id_salt("log")
        .max_height(height)
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for line in &view.log {
                ui.label(RichText::new(line).color(theme::TEXT_DIM).monospace().size(13.5));
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
            theme::GOLD_ACTION,
            theme::GOLD_EDGE,
        ] {
            assert!(
                theme::separation(theme::PANEL, c) >= 150,
                "a colour drawn on a panel is not legible on it"
            );
        }
    }

    /// `D-067`: the columns follow the width in points. Three on a wide
    /// window; one at 200 % text in a 1 180-pixel window (590 points), where
    /// three had drawn over each other; and every tone has a palette colour.
    #[test]
    fn the_columns_follow_the_width() {
        assert_eq!(columns_for(1_180.0), 3, "the default window at 100 %");
        assert_eq!(columns_for(900.0), 2, "the smallest window at 100 %");
        assert_eq!(columns_for(787.0), 2, "the default window at 150 %");
        assert_eq!(columns_for(590.0), 1, "the default window at 200 %");
        assert_eq!(columns_for(450.0), 1, "the smallest window at 200 %");
        for tone in [Tone::Dim, Tone::Ok, Tone::Accent, Tone::Warn, Tone::Danger] {
            assert!(theme::separation(theme::PANEL, tone_colour(tone)) >= 150);
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

    /// Both themes carry the client's palette: with apps set to light, egui
    /// had drawn the fields white and the window titles light grey.
    #[test]
    fn both_themes_get_the_same_palette() {
        let ctx = egui::Context::default();
        install(&ctx);
        let dark = ctx.style_of(egui::Theme::Dark);
        let light = ctx.style_of(egui::Theme::Light);
        assert_eq!(dark.visuals.extreme_bg_color, theme::FIELD);
        assert_eq!(light.visuals.extreme_bg_color, theme::FIELD);
        assert_eq!(dark.visuals.window_fill, light.visuals.window_fill);
        assert_eq!(dark.visuals.widgets.inactive.weak_bg_fill, light.visuals.widgets.inactive.weak_bg_fill);
        assert_eq!(dark.visuals.widgets.open.weak_bg_fill, light.visuals.widgets.open.weak_bg_fill);
        assert!(light.visuals.dark_mode);
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
