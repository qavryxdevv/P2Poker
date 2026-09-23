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
    /// `D-068`: open the album's window.
    OpenAlbum,
    /// `D-068`: the summary after a game has been looked at.
    RewardsSeen,
    /// `D-068`: the sentence about the rewards file has been read.
    RewardsNoticeSeen,
    /// `D-081`: the sentence about a clock that is out has been read.
    ClockNoticeSeen,
    /// `D-068`: make a backup of the profile under this password.
    MakeBackup(String),
    /// `D-068`: open this backup with this password and say what it holds.
    CheckBackup { path: String, password: String },
    /// `D-068`: put the backup just opened in the profile's place at the next start.
    StageRestore,
    /// `D-068`: show the folder the backups are in.
    ShowBackups,
    /// `D-071`: open the donation page, `DONATION_URL`, in the browser.
    OpenDonationPage,
    /// `D-072`: open the bug reports page, `BUG_REPORT_URL`, in the browser.
    OpenBugReports,
    /// `D-075`: ask GitHub whether a newer version is out -- the About page's
    /// button. The one other question is the window's own as it opens (`D-077`).
    CheckForUpdate,
    /// `D-075`: open the releases page, `RELEASES_URL`, in the browser.
    OpenReleases,
    /// `D-077`: hand the newer release's program to the browser to download.
    DownloadUpdate,
    /// `D-077`: open the newer release's own page: what is new in it.
    UpdateNotes,
    /// `D-077`: close the client, to start the new version.
    QuitForUpdate,
    /// `D-073`: show the folder this program runs from.
    ShowProgramFolder,
    /// `D-073`: a portable copy asks to be installed: this client closes and
    /// starts itself again as the installer.
    InstallOnThisComputer,
}

/// `D-071`: the page the strip's *Support the project* button opens: the
/// donation addresses, in the project's own repository.
///
/// **A constant of the build, and nothing else.** No setting, no file and no
/// word from another player reaches this string: an address a donor is sent to
/// is money, and one that the network could change is one a stranger could
/// change. The addresses themselves are not in the binary either -- they are
/// on that page, which `tools/donation-page.py` writes and whose history the
/// repository keeps, so an address changes without a release and never
/// without a commit. `tests/donation_page.rs` holds this link to that file.
pub const DONATION_URL: &str = "https://github.com/qavryxdevv/P2Poker/blob/master/DONATE.md";

/// `D-072`: where a player reports a bug -- the repository's issue tracker, with
/// the form already open.
///
/// A constant of the build for the same reason `DONATION_URL` is: an address the
/// client sends a player to is not something the network may choose. It carries
/// no query beyond the template, so nothing of this machine or this player goes
/// out in it; what goes in the report is what the player types.
pub const BUG_REPORT_URL: &str = "https://github.com/qavryxdevv/P2Poker/issues/new";

/// `D-075`: where a newer version is downloaded from -- the repository's
/// releases, newest on top. A constant of the build like the two above, and for
/// the same reason: **what GitHub answers to the version check is read for
/// numbers and never for an address**, so the page a player is sent to cannot
/// be chosen by whoever answers. `D-077`: a release's own page and its program
/// are this address with the release's tag in the path, and nothing else of
/// the answer (`app::update::download_url`).
pub const RELEASES_URL: &str = "https://github.com/qavryxdevv/P2Poker/releases";

/// `D-075`: the version check, as the About page holds it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UpdateUi {
    /// Not asked yet in this session.
    #[default]
    Idle,
    /// `D-077`: the question the window asks as it opens. The lobby waits for
    /// its answer (`lobby::gate`).
    Opening,
    /// The About page's button was pressed and GitHub has not answered.
    Checking,
    Done(Result<crate::app::update::Verdict, String>),
}

/// `D-072`: what the About page calls this build. The number comes from
/// `Cargo.toml` and the word from here, so a release that is no longer a beta is
/// one line, not a search.
pub const RELEASE_STAGE: &str = "beta";

/// `D-073`: where this copy of the client lives, as the About page says it.
///
/// Worked out once at start-up -- it asks the system where this user's folders
/// are -- and only `busy` changes while the client runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeView {
    /// The folder the program runs from.
    pub folder: std::path::PathBuf,
    /// `D-078`: the folder the player's profile is in -- beside the program on
    /// Windows and in a portable copy, the user's data folder in a Linux package.
    pub profile: std::path::PathBuf,
    /// It is the installed copy, in the user's programs folder.
    pub installed: bool,
    /// `D-080`: this copy came from the Microsoft Store -- it runs from a
    /// package, so the Store installs it, updates it and removes it, and this
    /// client neither installs nor downloads anything.
    pub store: bool,
    /// This build can install itself (Windows, and not from the Store).
    pub can_install: bool,
    /// A table is open or a search is running: installing restarts the client,
    /// and nothing restarts a client over a game.
    pub busy: bool,
}

/// `D-068`: the settings' Profile page: what is being typed, and what the
/// client answered. The work itself -- a second of key stretching, and a file
/// -- is never done on the paint thread: the page asks, and is told.
#[derive(Debug, Clone, Default)]
pub struct BackupUi {
    pub password: String,
    pub repeat: String,
    /// A backup or a check is being made.
    pub busy: bool,
    /// The last backup made, as words.
    pub made: Option<String>,
    pub error: Option<String>,
    /// The backups in the profile's folder, newest first: `(name, path)`.
    pub found: Vec<(String, String)>,
    pub restore_path: String,
    pub restore_password: String,
    /// What the backup just opened holds, in words.
    pub checked: Option<String>,
    /// A restore is staged: it takes the profile's place at the next start.
    pub staged: bool,
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
    /// `D-068`: the settings' Profile page.
    pub backup: BackupUi,
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
    /// `D-073`: where this copy lives. `None` until `main` has worked it out,
    /// and in a preview, where there is no copy to speak of.
    pub home: Option<HomeView>,
    /// `D-075`: what the version check said, if it was asked.
    pub update: UpdateUi,
    /// `D-077`: the download of a newer release was asked for, and whether the
    /// system's browser took the address.
    pub download_handed: Option<bool>,
    /// `D-081`: the sentence about a clock that is out, once the version check
    /// has read the time GitHub's answer carried. `None` while the clock is
    /// close enough, and once the player has read it.
    pub clock_notice: Option<String>,
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
    /// `D-068`: the profile's backup and restore.
    Profile,
    /// `D-072`: what this build is, and where a bug goes.
    About,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 6] = [
        SettingsTab::General,
        SettingsTab::Sound,
        SettingsTab::Table,
        SettingsTab::Network,
        SettingsTab::Profile,
        SettingsTab::About,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SettingsTab::General => "General",
            SettingsTab::Sound => "Sound",
            SettingsTab::Table => "Table",
            SettingsTab::Network => "Network",
            SettingsTab::Profile => "Profile",
            SettingsTab::About => "About",
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
            home: None,
            update: UpdateUi::default(),
            download_handed: None,
            clock_notice: None,
            settings,
            settings_tab: SettingsTab::default(),
            backup: BackupUi::default(),
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
    // `D-067`: the word that the search found a game, for a moment.
    if let Some(f) = view.found.as_ref() {
        found_toast(ui.ctx(), f);
    }
    // `D-068`: a card or a level earned, for a moment, in this window only.
    if let Some(r) = view.reveal.as_ref() {
        // Under the word about a table found, while that is showing: the two
        // are never drawn over each other.
        reveal_toast(ui.ctx(), r, if view.found.is_some() { 200.0 } else { 110.0 });
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
    // `D-068`: the profile is running on another device as well.
    if view.profile_elsewhere {
        egui::Panel::top("profile-elsewhere").frame(egui::Frame::NONE).show(ui, profile_elsewhere_band);
    }

    // `D-071`: the strip's one button that leaves the client, where no other
    // button was pressed in the same pass.
    let mut donate = false;
    egui::Panel::bottom("network")
        .frame(frame())
        .show(ui, |ui| donate = network_strip(ui, view, state));

    // Proportions rather than pixel counts: 560 and 280 are answers to one
    // window size only, and on a wide one they left the middle column too narrow
    // to fit the words "Players in the lobby" on a single line. The tables get
    // over half: the way into a game is there.
    let across = ui.available_width();
    // `D-068`: a button on the card about the player, where no other button
    // was pressed in the same pass.
    let mut from_card: Option<LobbyAction> = None;
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
                .show(ui, |ui| {
                    if let Some(a) = info_column(ui, view, state) {
                        from_card = Some(a);
                    }
                });
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
    if let Some(a) = from_card.filter(|_| action == LobbyAction::None) {
        action = a;
    }
    if donate && action == LobbyAction::None {
        action = LobbyAction::OpenDonationPage;
    }
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

    // `D-077`: the lobby shut to this version, or waiting for the answer that
    // says whether it is -- the gate's own window over everything, and its
    // buttons the only thing that reaches the client.
    let gate = super::lobby::gate(&state.update);
    if gate != super::lobby::Gate::Open {
        let from_gate = gate_modal(ui.ctx(), &gate, state);
        action = super::lobby::through_the_gate(&gate, action, from_gate);
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

/// `D-064`, restyled under `D-067`: the search's window -- a modal over the
/// lobby in the table's style. The clock, the seats of the table closest to
/// starting drawn as they fill, the estimate, who else is looking, the seats
/// held, the node's phase and its one warning, a line on what happens next,
/// and the one quiet button, which cancels on the click. Nothing here is red:
/// a search is not a fault, and the cancel is a choice, not an alarm.
///
/// Repainted four times a second while it is up: the spinner turns, the clock
/// counts, and the node's word arrives once a second on its own.
pub fn search_modal(ctx: &egui::Context, s: &super::lobby::SearchView) -> Option<LobbyAction> {
    let mut action = None;
    egui::Modal::new(egui::Id::new("search-modal"))
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .stroke(Stroke::new(1.0, theme::GOLD_EDGE))
                .corner_radius(12.0)
                .inner_margin(22.0),
        )
        .show(ctx, |ui| {
            // Never wider or taller than the window: at 200 % text in a small
            // window the whole of it is 450 points across and 300 down, so the
            // width follows the window and the body scrolls, with the button
            // kept under it.
            let room = ctx.content_rect();
            ui.set_width((room.width() - 60.0).clamp(240.0, 470.0));
            let body_height = (room.height() - 150.0).max(120.0);
            scroller(egui::ScrollArea::vertical())
                .id_salt("search-body")
                .max_height(body_height)
                .auto_shrink([false, true])
                .show(ui, |ui| search_body(ui, s));
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("Cancel search").color(theme::TEXT))
                            .fill(theme::PANEL_LIGHT)
                            .min_size(egui::vec2(200.0, 40.0)),
                    )
                    .clicked()
                {
                    action = Some(LobbyAction::CancelSearch);
                }
            });
            super::table::paint_again(ctx, std::time::Duration::from_millis(250));
        });
    action
}

/// What the search's window says, above its one button.
fn search_body(ui: &mut egui::Ui, s: &super::lobby::SearchView) {
    use super::lobby::{clock, closest_table, eta_words, held_words, needed_now, queue_words, reservation_words};
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new().size(26.0).color(theme::GOLD_ACTION));
        ui.add_space(6.0);
        ui.label(RichText::new("Finding you a game").color(theme::TEXT).size(21.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(clock(s.elapsed_s))
                    .color(theme::GOLD_ACTION)
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
    ui.add_space(12.0);

    let r = s.report.as_ref();
    // The seats of the table closest to starting, drawn as they fill: what
    // the player is waiting for, shown as the thing itself rather than as a
    // bar. `S1-IE`: the rings are the players its founder needs **now**, so
    // they go one by one as the founder comes down -- ten rings from the first
    // second to the last said nothing of that.
    ui.horizontal(|ui| match closest_table(r) {
        Some(x) => {
            dots(ui, egui::Id::new(("search-dots", s.id)), x.players, needed_now(x), theme::GOLD_ACTION, 16.0);
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(
                    RichText::new(format!("{} at {}", reservation_words(x), x.name))
                        .color(theme::TEXT)
                        .size(15.0),
                )
                .wrap(),
            );
        }
        None => {
            dots(ui, egui::Id::new(("search-dots", s.id)), 0, 6, theme::GOLD_ACTION, 16.0);
            ui.add_space(6.0);
            ui.label(
                RichText::new(if r.is_some() { "looking for tables" } else { "starting" })
                    .color(theme::TEXT_DIM)
                    .size(15.0),
            );
        }
    });
    ui.add_space(12.0);
    stat_row(ui, "Estimated wait", &eta_words(r.and_then(|r| r.eta_s), s.typical_s), theme::TEXT);
    stat_row(ui, "Also looking", &queue_words(r), theme::TEXT);
    // Games at once matter only when more than one was asked for, or one has
    // started.
    if let Some(r) = r.filter(|r| r.limit > 1 || r.running > 0) {
        stat_row(ui, "Games", &format!("{} of {} started", r.running, r.limit), theme::STACK);
    }
    ui.add_space(8.0);

    // The seats held: each table with its seats drawn and its word.
    let held = r.map_or(0, |r| r.reservations.len());
    ui.add(
        egui::Label::new(
            RichText::new(held_words(held))
                .color(if held > 0 { theme::OK } else { theme::TEXT_DIM })
                .size(14.0),
        )
        .wrap(),
    );
    if let Some(r) = r {
        for (i, x) in r.reservations.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0);
                dots(ui, egui::Id::new(("held-dots", s.id, i)), x.players, needed_now(x), theme::GOLD_EDGE, 10.0);
                ui.label(RichText::new(&x.name).color(theme::TEXT).size(14.0).strong());
                ui.label(RichText::new(reservation_words(x)).color(theme::TEXT_DIM).size(14.0));
            });
            // `S1-HV`: what the founder waits for, so a forming table never
            // reads as frozen.
            if let Some(n) = x.note.as_ref() {
                ui.add(egui::Label::new(RichText::new(format!("        {n}")).color(theme::WARN).size(13.0)).wrap());
            }
        }
        ui.add_space(6.0);
        ui.label(RichText::new(&r.phase).color(theme::TEXT_DIM).size(13.0));
        if let Some(w) = r.warning.as_ref() {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, theme::WARN);
                ui.add(egui::Label::new(RichText::new(w).color(theme::WARN).size(14.0)).wrap());
            });
        }
    }
    ui.add_space(10.0);
    ui.add(
        egui::Label::new(
            RichText::new("You are seated the moment a table starts; its window opens by itself.")
                .color(theme::TEXT_DIM)
                .size(13.0),
        )
        .wrap(),
    );
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

/// `D-067`: the word that the search found a game -- a card in the table's
/// felt at the top of the lobby, faded in and out over `FOUND_TOAST_MS`, with
/// nothing to press: the table's window has opened by itself. Said once and
/// then not again; no blink, no second helping.
fn found_toast(ctx: &egui::Context, f: &super::lobby::FoundView) {
    use super::lobby::FOUND_TOAST_MS;
    let age = f.age_ms as f32;
    let alpha = (age / 300.0)
        .min((FOUND_TOAST_MS as f32 - age) / 600.0)
        .clamp(0.0, 1.0);
    egui::Area::new(egui::Id::new("found-toast"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 110.0))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(theme::FELT_MID.gamma_multiply(alpha))
                .stroke(Stroke::new(1.0, theme::GOLD_EDGE.gamma_multiply(alpha)))
                .corner_radius(12.0)
                .inner_margin(egui::Margin::symmetric(18, 12))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                        let p = ui.painter();
                        p.circle_filled(rect.center(), 17.0, Color32::from_black_alpha((150.0 * alpha) as u8));
                        p.circle_stroke(rect.center(), 16.0, Stroke::new(1.5, theme::GOLD_EDGE.gamma_multiply(alpha)));
                        p.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "\u{2660}",
                            FontId::proportional(20.0),
                            theme::GOLD_ACTION.gamma_multiply(alpha),
                        );
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.label(
                                RichText::new("Table found")
                                    .color(theme::GOLD_ACTION.gamma_multiply(alpha))
                                    .size(18.0)
                                    .strong(),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "{} \u{2014} your seat is taken; the table's window is open.",
                                    f.table
                                ))
                                .color(theme::TEXT.gamma_multiply(alpha))
                                .size(14.0),
                            );
                        });
                    });
                });
        });
    super::table::paint_again(ctx, std::time::Duration::from_millis(80));
}

/// `D-072`: what this build is, and the one button that leaves the client from
/// here -- the bug report.
///
/// **A beta says so where a player looks for it.** The word is
/// [`RELEASE_STAGE`] and the number is the crate's own, so the two cannot drift
/// apart and a release that stops being a beta is one line.
///
/// The button opens the repository's issue form in the browser (`D-071`'s
/// `open_in_browser`, which takes a plain `https` address and nothing else).
/// **It sends nothing**: no log, no profile, no address -- a report is what the
/// player types, and a client that posted its own state somewhere would be
/// making that decision for them.
fn about_page(ui: &mut egui::Ui, home: Option<&HomeView>, update: &UpdateUi) -> Option<LobbyAction> {
    let mut action = None;
    ui.label(
        RichText::new(format!("P2Poker {} {}", env!("CARGO_PKG_VERSION"), RELEASE_STAGE))
            .color(theme::TEXT)
            .size(19.0)
            .strong(),
    );
    ui.add_space(2.0);
    ui.label(
        RichText::new(
            "A beta: it is played and it is not finished. Tables, hands and the album are real, \
             and a build may still change what it does between versions.",
        )
        .color(theme::TEXT_DIM)
        .size(14.0),
    );
    ui.add_space(10.0);
    ui.label(RichText::new("No house. No server. No rake. No ads.").color(theme::OK).size(14.0));
    ui.label(
        RichText::new("Play money only. Nothing here is a wager and nothing is cashed out.")
            .color(theme::TEXT_DIM)
            .size(14.0),
    );
    ui.add_space(12.0);
    if let Some(a) = update_section(ui, update) {
        action = Some(a);
    }
    ui.add_space(14.0);
    ui.label(RichText::new("Found something wrong?").color(theme::TEXT).size(15.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "The report opens in your browser and this client sends nothing with it. What helps \
             most: what you did, what happened, and what you expected instead.",
        )
        .color(theme::TEXT_DIM)
        .size(14.0),
    );
    ui.add_space(8.0);
    if ui
        .add(
            egui::Button::new(RichText::new("Report a bug").color(theme::INK_ON_GOLD).strong())
                .fill(theme::GOLD_ACTION)
                .min_size(egui::vec2(150.0, 32.0)),
        )
        .on_hover_text("Opens the project's issue tracker on GitHub in your browser.")
        .clicked()
    {
        action = Some(LobbyAction::OpenBugReports);
    }
    if let Some(home) = home {
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);
        if let Some(a) = home_section(ui, home) {
            action = Some(a);
        }
    }
    action
}

/// `D-075`: what the About page says after the version check, and in which
/// tone. `None` while nothing was asked.
pub fn update_words(update: &UpdateUi) -> Option<(String, Color32)> {
    use crate::app::update::Verdict;
    Some(match update {
        UpdateUi::Idle => return None,
        UpdateUi::Opening | UpdateUi::Checking => ("Asking GitHub\u{2026}".to_owned(), theme::TEXT_DIM),
        UpdateUi::Done(Ok(Verdict::Newest { latest })) => {
            (format!("This is the newest version. The newest release is {latest}."), theme::OK)
        }
        UpdateUi::Done(Ok(Verdict::Newer { latest, .. })) => (
            format!(
                "Version {latest} is out, and this one can no longer play. Download it from the releases page and \
                 start it: it offers to update this installation, and your player profile stays as it is."
            ),
            theme::GOLD_ACTION,
        ),
        UpdateUi::Done(Ok(Verdict::NoRelease)) => ("No release has been published yet.".to_owned(), theme::TEXT_DIM),
        UpdateUi::Done(Err(why)) => (format!("{why}. The releases page says what the newest version is."), theme::WARN),
    })
}

/// `D-075`: the version check. One button, what it said, and what it costs in
/// privacy -- in a sentence, where the player presses it.
fn update_section(ui: &mut egui::Ui, update: &UpdateUi) -> Option<LobbyAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        let asking = matches!(update, UpdateUi::Checking | UpdateUi::Opening);
        if ui.add_enabled(!asking, egui::Button::new("Check for a new version")).clicked() {
            action = Some(LobbyAction::CheckForUpdate);
        }
        let offer_page = matches!(update, UpdateUi::Done(Ok(crate::app::update::Verdict::Newer { .. })) | UpdateUi::Done(Err(_)));
        if offer_page && ui.button("Open the releases page").clicked() {
            action = Some(LobbyAction::OpenReleases);
        }
    });
    if let Some((words, colour)) = update_words(update) {
        ui.add_space(4.0);
        ui.label(RichText::new(words).color(colour).size(14.0));
    }
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "Asks GitHub for its list of releases when P2Poker opens and when you press the button. GitHub sees the \
             address the question comes from, as it would if you opened the page; nothing about you or this client \
             is sent. P2Poker downloads nothing itself: a new version is downloaded by your browser.",
        )
        .color(style::mix(theme::TEXT_DIM, theme::PANEL, 0.3))
        .size(13.0),
    );
    action
}

/// `D-077`: why the lobby waits a moment as the window opens.
const GATE_ASKING: &str = "P2Poker asks GitHub once, as it opens, whether a newer version is out: this beta's \
                           protocol still changes from one version to the next, and an older client cannot play \
                           with a newer one.";

/// `D-077`: the window over a lobby that is not open to this version. While the
/// window's opening question is out, a spinner and why it is asked; once a
/// newer release is known, which one it is, its download, and what to do next,
/// for where this copy lives.
///
/// A modal and not a dialog: nothing under it can be clicked, and it has no
/// close box -- Escape and a click beside it are not read. What leaves it is
/// its own buttons, and closing the client.
fn gate_modal(ctx: &egui::Context, gate: &super::lobby::Gate, state: &LobbyUi) -> Option<LobbyAction> {
    use super::lobby::Gate;
    let mut action = None;
    egui::Modal::new(egui::Id::new("update-gate"))
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .stroke(Stroke::new(1.0, theme::GOLD_EDGE))
                .corner_radius(12.0)
                .inner_margin(22.0),
        )
        .show(ctx, |ui| {
            let room = ctx.content_rect();
            ui.set_width((room.width() - 60.0).clamp(260.0, 520.0));
            match gate {
                Gate::Open => {}
                Gate::Asking => {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(26.0).color(theme::GOLD_ACTION));
                        ui.add_space(6.0);
                        ui.label(RichText::new("Checking for a new version").color(theme::TEXT).size(21.0).strong());
                    });
                    ui.add_space(8.0);
                    ui.add(egui::Label::new(RichText::new(GATE_ASKING).color(theme::TEXT_DIM).size(15.0)).wrap());
                    super::table::paint_again(ctx, std::time::Duration::from_millis(250));
                }
                Gate::Closed { latest, tag } => {
                    // `D-080`: a copy from the Store updates through the Store,
                    // so no address is shown to copy into a browser.
                    let store = state.home.as_ref().is_some_and(|h| h.store);
                    let words = super::lobby::closed_words(
                        latest,
                        &crate::app::update::compared_version(),
                        state.home.as_ref(),
                        super::lobby::System::THIS,
                    );
                    let body_height = (room.height() - 160.0).max(140.0);
                    scroller(egui::ScrollArea::vertical())
                        .id_salt("update-gate-body")
                        .max_height(body_height)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.label(RichText::new(&words.heading).color(theme::GOLD_ACTION).size(22.0).strong());
                            ui.add_space(6.0);
                            ui.add(egui::Label::new(RichText::new(&words.body).color(theme::TEXT).size(15.5)).wrap());
                            ui.add_space(12.0);
                            ui.horizontal_wrapped(|ui| {
                                let wide = style::text_width(ui.painter(), &words.download, 18.0, style::Weight::Bold) + 56.0;
                                if gold_button(ui, &words.download, egui::vec2(wide.max(200.0), 44.0)).clicked() {
                                    action = Some(LobbyAction::DownloadUpdate);
                                }
                                let notes = egui::Button::new(RichText::new("What is new").size(15.0))
                                    .min_size(egui::vec2(0.0, 40.0));
                                if ui.add(notes).clicked() {
                                    action = Some(LobbyAction::UpdateNotes);
                                }
                            });
                            if let Some(handed) = state.download_handed {
                                ui.add_space(6.0);
                                let tone = if handed { theme::OK } else { theme::WARN };
                                let words = super::lobby::handed_words(handed, super::lobby::System::THIS, store);
                                ui.label(RichText::new(words).color(tone).size(14.5));
                            }
                            ui.add_space(10.0);
                            ui.add(egui::Label::new(RichText::new(&words.next).color(theme::TEXT_DIM).size(14.5)).wrap());
                            if let Some(folder) = words.folder.as_ref() {
                                ui.add_space(6.0);
                                egui::Frame::new()
                                    .fill(theme::FIELD)
                                    .stroke(Stroke::new(1.0, theme::LINE))
                                    .corner_radius(8.0)
                                    .inner_margin(egui::Margin::symmetric(12, 8))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            let label = if words.by_hand { "This copy is in" } else { "Installed in" };
                                            ui.label(RichText::new(label).color(theme::TEXT_DIM).size(13.0));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                if ui.small_button("Show in folder").clicked() {
                                                    action = Some(LobbyAction::ShowProgramFolder);
                                                }
                                            });
                                        });
                                        let path = RichText::new(folder.display().to_string()).color(theme::TEXT).size(14.0);
                                        ui.add(egui::Label::new(path.monospace()).wrap());
                                    });
                            }
                            if let Some(url) =
                                crate::app::update::download_for(tag, super::lobby::System::THIS).filter(|_| !store)
                            {
                                ui.add_space(8.0);
                                let quiet = style::mix(theme::TEXT_DIM, theme::PANEL, 0.2);
                                ui.add(egui::Label::new(RichText::new(url).color(quiet).size(12.5).monospace()).wrap().selectable(true));
                            }
                        });
                    ui.add_space(14.0);
                    ui.vertical_centered(|ui| {
                        let close = egui::Button::new(RichText::new("Close P2Poker").color(theme::TEXT))
                            .fill(theme::PANEL_LIGHT)
                            .min_size(egui::vec2(200.0, 40.0));
                        if ui.add(close).clicked() {
                            action = Some(LobbyAction::QuitForUpdate);
                        }
                    });
                }
            }
        });
    action
}

/// `D-073`: what the About page says about where this copy lives. `D-078`: a
/// Linux package keeps the program where it was installed and the profile in
/// the user's data folder, and says both. `D-080`: a copy from the Microsoft
/// Store is installed, updated and removed by the Store, and says that first,
/// because none of the three is this client's to offer.
pub fn home_words(home: &HomeView, system: super::lobby::System) -> String {
    if home.store {
        return format!(
            "P2Poker came from the Microsoft Store, which keeps it up to date and removes it if you uninstall it. \
             Your player profile is in {}, and removing P2Poker takes it with them: to stay the same player, make a \
             backup first (the Profile tab).",
            home.profile.display()
        );
    }
    if system == super::lobby::System::Linux && !home.profile.starts_with(&home.folder) {
        return format!(
            "P2Poker runs from {}, and your player profile is in {}. To be the same player on another computer, \
             make a backup (the Profile tab) and restore it there.",
            home.folder.display(),
            home.profile.display()
        );
    }
    if home.installed {
        format!(
            "Installed in {}. To remove P2Poker, delete that folder and the shortcuts. Your player profile is in \
             that folder too, so make a backup first (the Profile tab) if you want to stay the same player.",
            home.folder.display()
        )
    } else {
        format!(
            "This copy is portable: it runs from {}, and your player profile is kept beside it. Copy the whole \
             folder to another computer and you are the same player there.",
            home.folder.display()
        )
    }
}

fn home_section(ui: &mut egui::Ui, home: &HomeView) -> Option<LobbyAction> {
    let mut action = None;
    ui.label(RichText::new("Where this program is").color(theme::TEXT).size(15.0).strong());
    ui.add_space(4.0);
    ui.label(RichText::new(home_words(home, super::lobby::System::THIS)).color(theme::TEXT_DIM).size(14.0));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Show in folder").clicked() {
            action = Some(LobbyAction::ShowProgramFolder);
        }
        if !home.installed && home.can_install {
            let install = ui
                .add_enabled(!home.busy, egui::Button::new("Install on this computer\u{2026}"))
                .on_hover_text(
                    "Copies the program to your user folder and puts a shortcut on the desktop. P2Poker restarts to do it.",
                )
                .on_disabled_hover_text("Leave your tables and stop the search first: installing restarts P2Poker.");
            if install.clicked() {
                action = Some(LobbyAction::InstallOnThisComputer);
            }
        }
    });
    action
}

/// `D-068`: the settings' Profile page -- a backup of the profile under a
/// password, and a profile restored from one. Both only ask: the work is the
/// client's, off the paint thread.
fn profile_page(ui: &mut egui::Ui, b: &mut BackupUi) -> Option<LobbyAction> {
    use crate::storage::backup::PASSWORD_MIN;
    let mut action = None;
    let dim = |text: &str| RichText::new(text).color(theme::TEXT_DIM).size(14.0);
    ui.label(RichText::new("Back up this profile").color(theme::TEXT).size(16.0).strong());
    ui.add(
        egui::Label::new(dim(
            "One file with your player key, settings, record, notes and rewards. Carry it to another \
             computer and restore it there: nothing in it belongs to this machine.",
        ))
        .wrap(),
    );
    ui.add_space(6.0);
    field_row(ui, "Password", |ui| {
        ui.add(egui::TextEdit::singleline(&mut b.password).password(true));
    });
    field_row(ui, "Again", |ui| {
        ui.add(egui::TextEdit::singleline(&mut b.repeat).password(true));
    });
    let long_enough = b.password.chars().count() >= PASSWORD_MIN;
    let same = b.password == b.repeat;
    let hint = if !long_enough {
        format!("A backup is always under a password of at least {PASSWORD_MIN} characters.")
    } else if !same {
        "The two passwords differ.".to_string()
    } else {
        "A forgotten password is a lost backup: nothing opens it without.".to_string()
    };
    ui.add(egui::Label::new(dim(&hint)).wrap());
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(long_enough && same && !b.busy, egui::Button::new("Create backup")).clicked() {
            action = Some(LobbyAction::MakeBackup(b.password.clone()));
        }
        if ui.button("Show backups folder").clicked() {
            action = Some(LobbyAction::ShowBackups);
        }
    });
    if let Some(made) = b.made.as_ref() {
        ui.add(egui::Label::new(RichText::new(made).color(theme::OK).size(14.0)).wrap());
    }

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(6.0);
    ui.label(RichText::new("Restore a profile from a backup").color(theme::TEXT).size(16.0).strong());
    ui.add(
        egui::Label::new(dim(
            "Pick a backup from the folder, type its path, or drop the file on this window. It replaces this \
             profile's player key, settings, record, notes and rewards at the next start; what it replaces is \
             kept in the backups folder. Use a profile on one device at a time.",
        ))
        .wrap(),
    );
    ui.add_space(6.0);
    // A backup file dropped on the window fills the path in.
    let dropped = ui.ctx().input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
    if let Some(path) = dropped {
        b.restore_path = path.display().to_string();
        b.checked = None;
    }
    if !b.found.is_empty() {
        egui::ComboBox::from_id_salt("backups-found")
            .selected_text(
                b.found.iter().find(|(_, p)| *p == b.restore_path).map_or("Backups in the folder\u{2026}", |(n, _)| n.as_str()),
            )
            .width(ui.available_width().min(380.0))
            .show_ui(ui, |ui| {
                for (name, path) in &b.found {
                    if ui.selectable_label(*path == b.restore_path, name).clicked() {
                        b.restore_path = path.clone();
                        b.checked = None;
                    }
                }
            });
    }
    field_row(ui, "File", |ui| {
        if ui.add(egui::TextEdit::singleline(&mut b.restore_path)).changed() {
            b.checked = None;
        }
    });
    field_row(ui, "Password", |ui| {
        if ui.add(egui::TextEdit::singleline(&mut b.restore_password).password(true)).changed() {
            b.checked = None;
        }
    });
    ui.add_space(4.0);
    if b.staged {
        ui.add(
            egui::Label::new(
                RichText::new("Ready. Close p2p-poker and start it again: the restored profile takes its place then.")
                    .color(theme::OK)
                    .size(14.0),
            )
            .wrap(),
        );
    } else if let Some(words) = b.checked.as_ref() {
        ui.add(egui::Label::new(RichText::new(words).color(theme::TEXT).size(14.0)).wrap());
        if ui.add_enabled(!b.busy, egui::Button::new("Restore at the next start")).clicked() {
            action = Some(LobbyAction::StageRestore);
        }
    } else {
        let ready = !b.restore_path.trim().is_empty() && !b.restore_password.is_empty() && !b.busy;
        if ui.add_enabled(ready, egui::Button::new("Open backup")).clicked() {
            action = Some(LobbyAction::CheckBackup { path: b.restore_path.trim().to_string(), password: b.restore_password.clone() });
        }
    }
    if b.busy {
        ui.label(dim("Working\u{2026}"));
    }
    if let Some(e) = b.error.as_ref() {
        ui.add(egui::Label::new(RichText::new(e).color(theme::WARN).size(14.0)).wrap());
    }
    action
}

/// `D-068`: the notice that this profile runs somewhere else as well -- a band
/// under the header, not a window over the lobby: the games it shows go on.
fn profile_elsewhere_band(ui: &mut egui::Ui) {
    egui::Frame::new()
        .fill(theme::FIELD)
        .stroke(Stroke::new(1.0, theme::WARN))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .outer_margin(egui::Margin { left: 5, right: 5, top: 4, bottom: 0 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(super::lobby::PROFILE_ELSEWHERE).color(theme::WARN).size(14.5)).wrap());
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
                    // `D-072`: no Game row. The cash game is deactivated in this
                    // client, so a chooser with one choice in it would be a
                    // question with one answer -- and the branch below stays
                    // whole rather than being deleted, because the kind is still
                    // a field of the protocol and of `NewTable`.
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
                                ui.add_space(10.0);
                                // `D-068`: autonomy -- the whole of it can be put away.
                                let mut shown = f.show_rewards();
                                if ui.checkbox(&mut shown, "Show rewards and quests").changed() {
                                    f.rewards = Some(shown);
                                }
                                ui.label(
                                    RichText::new(
                                        "Levels, cards, quests and table manners, kept on this computer only and \
                                         shown to nobody else. Turned off, nothing of them is shown and your \
                                         progress is still counted quietly.",
                                    )
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
                                    // `D-069`: this client's own, under PokerTH's four.
                                    let mut music = sound.search_music.unwrap_or(true);
                                    if ui
                                        .checkbox(&mut music, "Music while searching for a game")
                                        .on_hover_text("Plays quietly while Find a game is looking, and fades out when a table is found")
                                        .changed()
                                    {
                                        sound.search_music = Some(music);
                                    }
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
                            SettingsTab::Profile => {
                                if let Some(a) = profile_page(ui, &mut state.backup) {
                                    action = Some(a);
                                }
                            }
                            SettingsTab::About => {
                                if let Some(a) = about_page(ui, state.home.as_ref(), &state.update) {
                                    action = Some(a);
                                }
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
                // `S1-IR`: **`min_scrolled_height`, or the dialog eats its own
                // buttons.** A scroll area inside a window that sizes itself to
                // its content is a loop -- the window asks the area how tall it
                // is, the area asks the window how much room there is -- and it
                // settles wherever it first lands. Measured: `body_height` was
                // 650 and the viewport settled at about 365, so *Create* sat
                // below the fold of a window with 300 points of room to spare,
                // and only the wheel found it. Naming the height a scrolling
                // viewport must have ends the loop: the area is the content's
                // height when it fits, and the room it was given when it does
                // not. The settings page had already met this and says so above
                // its own `min_scrolled_height`. The bar is always drawn beside
                // it, as everywhere else in this file, so a body that really is
                // too tall says so before the wheel is turned.
                scroller(egui::ScrollArea::vertical())
                    .id_salt("dialog-body")
                    .max_height(body_height)
                    .min_scrolled_height(body_height)
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
pub(crate) fn gold_button(ui: &mut egui::Ui, label: &str, size: egui::Vec2) -> egui::Response {
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
fn seat_dots(ui: &mut egui::Ui, row: &TableRow, needed: Option<u8>) {
    let colour = if row.state.joinable() { theme::GOLD_ACTION } else { theme::TEXT_DIM };
    // `S1-IE`: at a search table this client holds a seat at, the rings are
    // the players its founder needs now -- from the search's own report -- and
    // the table's size is in the words beside them.
    let total = needed.unwrap_or(row.seats).max(row.seated);
    dots(ui, egui::Id::new(("seats", row.key)), row.seated, total, colour, 10.0).on_hover_text(match needed {
        Some(n) => format!("{} of the {n} players needed to start now; a table of {}", row.seated, row.seats),
        None => format!("{} of {} seats taken", row.seated, row.seats),
    });
    ui.label(
        RichText::new(format!("{}/{}", row.seated, total))
            .color(theme::TEXT_DIM)
            .size(13.0),
    );
}

/// Seats drawn, `d` points across: a disc per player, a ring per empty seat.
/// A seat taken since the last frame fills over a third of a second. The
/// ring of an empty seat is a half-strength dim, which shows on the list's
/// field and on a selected row's green alike (the line colour vanished on
/// the green).
fn dots(ui: &mut egui::Ui, id: egui::Id, filled: u8, total: u8, colour: Color32, d: f32) -> egui::Response {
    let total = total.clamp(1, 10);
    let gap = d * 0.3;
    let width = f32::from(total) * (d + gap) - gap;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, d + 6.0), egui::Sense::hover());
    let shown = ui.ctx().animate_value_with_time(id, f32::from(filled.min(total)), 0.35);
    let p = ui.painter();
    for i in 0..total {
        let c = egui::pos2(rect.left() + d / 2.0 + f32::from(i) * (d + gap), rect.center().y);
        let fill = (shown - f32::from(i)).clamp(0.0, 1.0);
        if fill > 0.0 {
            p.circle_filled(c, d / 2.0 * (0.6 + 0.4 * fill), colour.gamma_multiply(0.5 + 0.5 * fill));
        }
        p.circle_stroke(
            c,
            d / 2.0 - 0.5,
            Stroke::new(1.0, if fill > 0.0 { colour } else { theme::TEXT_DIM.gamma_multiply(0.55) }),
        );
    }
    resp
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
                                    let needed = super::lobby::row_needed(view, &row.key);
                                    seat_dots(ui, row, needed);
                                    let tone = match row.state {
                                        TableState::Open if needed.is_some_and(|n| row.seated >= n) => Tone::Ok,
                                        TableState::Open if needed.is_some() || row.seated < row.needed => Tone::Warn,
                                        TableState::Open => Tone::Ok,
                                        TableState::Full => Tone::Dim,
                                        TableState::ParametersChanged => Tone::Danger,
                                    };
                                    let words = match needed {
                                        Some(n) if row.state.joinable() => {
                                            super::lobby::status_words_needed(row.seated, n)
                                        }
                                        _ => status_words(row),
                                    };
                                    let status = ui.label(RichText::new(words).color(tone_colour(tone)).size(14.0));
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
fn you_card(ui: &mut egui::Ui, view: &LobbyView) -> Option<LobbyAction> {
    let mut action = None;
    // `D-068`: the end of a game first -- what it came to, truthfully, and
    // what is nearest now; then the card itself.
    if let Some(words) = view.rewards_notice {
        if notice_band(ui, words) {
            action = Some(LobbyAction::RewardsNoticeSeen);
        }
    }
    // `D-081`: a clock far enough out to leave the player looking in an hour
    // nobody else is in. Said in the same band and dismissed the same way; it
    // comes back at the next start while the clock is still wrong.
    if let Some(words) = view.clock_notice.as_deref() {
        if notice_band(ui, words) {
            action = Some(LobbyAction::ClockNoticeSeen);
        }
    }
    if let Some(s) = view.rewards.as_ref().and_then(|r| r.summary.as_ref()) {
        if game_summary(ui, s, view.rewards.as_ref().and_then(|r| r.goal.as_ref())) {
            action = Some(LobbyAction::RewardsSeen);
        }
        ui.add_space(10.0);
    }
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
            if let Some(r) = view.rewards.as_ref() {
                if you_rewards(ui, r) {
                    action = Some(LobbyAction::OpenAlbum);
                }
            }
        });
    action
}

/// `D-068`: the rewards on the card about the player -- the level's chip with
/// its bar, the meter, today's quests, the nearest goal, and the way into the
/// album. Returns whether the album was asked for.
fn you_rewards(ui: &mut egui::Ui, r: &super::rewards::YouRewards) -> bool {
    use super::album;
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
        album::chip(ui.painter(), rect.center(), 20.0, &r.level);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.label(RichText::new(format!("Level {}", r.level.level)).color(theme::TEXT).size(16.0).strong());
            let (bar, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(60.0), 8.0), egui::Sense::hover());
            album::bar(ui.painter(), bar, r.level.into, r.level.span, theme::GOLD_EDGE);
            ui.label(
                RichText::new(format!("{} / {} XP", r.level.into, r.level.span)).color(theme::TEXT_DIM).size(12.0),
            );
        });
    });
    ui.add_space(6.0);
    // The way into the album, high on the card: never under the fold.
    let label = format!("Album \u{00b7} {} of 52 cards", r.cards_have);
    let open = ui
        .add(egui::Button::new(RichText::new(label).size(14.0)).min_size(egui::vec2(ui.available_width(), 30.0)))
        .clicked();
    // The card the player chose to show, as its picture and its name.
    if let Some(card) = r.showcase {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(74.0, 60.0), egui::Sense::hover());
            album::picture(&ui.painter_at(rect), rect, card.art, card.suit);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new("On show").color(theme::TEXT_DIM).size(12.5));
                ui.add(egui::Label::new(RichText::new(format!("{} {}", card.label(), card.title)).color(theme::TEXT).size(14.0)).wrap());
            });
        });
    }
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("Table manners {}", r.manners)).color(album::meter_colour(r.manners)).size(13.5));
        ui.label(RichText::new(format!("\u{00b7} {}", r.season.rank)).color(theme::TEXT_DIM).size(13.5));
        let (stars, _) = ui.allocate_exact_size(egui::vec2(44.0, 14.0), egui::Sense::hover());
        for i in 0..3 {
            album::star(
                ui.painter(),
                egui::pos2(stars.left() + 7.0 + i as f32 * 15.0, stars.center().y),
                6.0,
                i < r.season.lit,
                theme::GOLD_ACTION,
            );
        }
    });
    if r.manners < 100 {
        ui.add(egui::Label::new(RichText::new(r.manners_words).color(theme::TEXT_DIM).size(12.5)).wrap());
    }
    ui.add_space(8.0);
    ui.label(RichText::new("Today").color(theme::TEXT_DIM).size(13.0));
    for q in &r.daily {
        let colour = if q.done { theme::OK } else { theme::TEXT };
        let tail = if q.done { "done".to_string() } else { format!("{}/{}", q.have, q.need) };
        words_and_figure(ui, RichText::new(&q.text).color(colour).size(13.5), &tail);
    }
    if r.weekly_to_pick {
        ui.label(RichText::new("This week's challenge is yours to pick, in the album.").color(theme::GOLD_EDGE).size(13.0));
    } else if let Some(w) = r.weekly.as_ref() {
        let tail = if w.done { "done".to_string() } else { format!("{}/{}", w.have, w.need) };
        words_and_figure(ui, RichText::new(format!("Week: {}", w.text)).color(theme::TEXT).size(13.5), &tail);
    }
    if let Some(g) = r.goal.as_ref() {
        ui.add_space(8.0);
        ui.label(RichText::new("Nearest card").color(theme::TEXT_DIM).size(13.0));
        ui.add(egui::Label::new(RichText::new(&g.label).color(theme::TEXT).size(14.5).strong()).wrap());
        let (bar, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 7.0), egui::Sense::hover());
        album::bar(ui.painter(), bar, g.have, g.need, theme::ACCENT);
        let left = if g.left.is_empty() { format!("{} / {}", g.have, g.need) } else { g.left.clone() };
        ui.label(RichText::new(left).color(theme::TEXT_DIM).size(12.5));
    }
    open
}

/// `D-068`: what the last game came to -- the peak and the end of it, said
/// truthfully: the experience with its reasons, the cards, what is nearest.
/// A game that was left says why and the way back, in the same calm colours.
/// Returns whether it was acknowledged.
fn game_summary(ui: &mut egui::Ui, s: &super::rewards::SummaryView, next: Option<&super::rewards::GoalLine>) -> bool {
    let mut seen = false;
    let edge = if s.left { theme::WARN } else { theme::GOLD_EDGE };
    egui::Frame::new()
        .fill(theme::FELT_EDGE)
        .stroke(Stroke::new(1.0, edge))
        .corner_radius(10.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let colour = if s.left { theme::TEXT } else { theme::GOLD_ACTION };
            ui.add(egui::Label::new(RichText::new(&s.headline).color(colour).size(15.5).strong()).wrap());
            ui.add_space(6.0);
            // The figures in a column of their own, right-aligned, and every
            // reason wrapping under its own first word -- not under the figure
            // (the owner's screenshot, 2026-09-19). A new card is one of these
            // lines already, so it is not said a second time below them.
            ui.spacing_mut().item_spacing.y = 3.0;
            let figures: Vec<String> = s.lines.iter().map(|(_, xp)| format!("+{xp}")).collect();
            let column = figures
                .iter()
                .map(|f| ui.painter().layout_no_wrap(f.clone(), FontId::proportional(13.0), theme::GOLD_EDGE).size().x)
                .fold(0.0_f32, f32::max)
                .ceil();
            for ((why, _), figure) in s.lines.iter().zip(&figures) {
                let new_card = why.starts_with("New card");
                let colour = if new_card { theme::TEXT } else { theme::ON_FELT_DIM };
                figure_and_words(ui, column, figure, RichText::new(why).color(colour).size(13.0));
            }
            // A card earned with no experience line of its own is still said.
            for card in s.cards.iter().filter(|c| !s.lines.iter().any(|(why, _)| why.contains(c.title))) {
                figure_and_words(
                    ui,
                    column,
                    "",
                    RichText::new(format!("New card: {} {}", card.label(), card.title)).color(theme::TEXT).size(13.0),
                );
            }
            ui.add_space(4.0);
            if let Some(level) = s.level_up {
                ui.label(RichText::new(format!("Level {level} reached")).color(theme::GOLD_ACTION).size(14.0).strong());
            }
            if !s.stars.is_empty() {
                ui.label(RichText::new(&s.stars).color(theme::ON_FELT_DIM).size(13.0));
            }
            if let Some(g) = next {
                let left = if g.left.is_empty() { format!("{} / {}", g.have, g.need) } else { g.left.clone() };
                ui.add(
                    egui::Label::new(RichText::new(format!("Nearest: {} \u{2014} {left}", g.label)).color(theme::TEXT).size(13.0))
                        .wrap(),
                );
            }
            ui.add_space(4.0);
            if ui.add(egui::Button::new(RichText::new("OK").size(13.0))).clicked() {
                seen = true;
            }
        });
    seen
}

/// One neutral sentence with an *OK* under it: `D-068`'s about the rewards
/// file, `D-081`'s about a clock that is out. Nothing here blinks and nothing
/// here is a warning sign; it is read once and pressed away.
fn notice_band(ui: &mut egui::Ui, words: &str) -> bool {
    let mut seen = false;
    egui::Frame::new().fill(theme::FIELD).stroke(Stroke::new(1.0, theme::LINE)).corner_radius(10.0).inner_margin(10.0).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(words).color(theme::TEXT_DIM).size(13.0)).wrap());
            if ui.add(egui::Button::new(RichText::new("OK").size(13.0))).clicked() {
                seen = true;
            }
        },
    );
    ui.add_space(8.0);
    seen
}

/// `D-068`: a card or a level earned, shown for a moment at the top of the
/// lobby -- and only there, and only while no hand is being played: the table's
/// window is never drawn over. Nothing to press, nothing that blinks.
fn reveal_toast(ctx: &egui::Context, r: &super::lobby::RevealView, from_top: f32) {
    let age = r.age_ms as f32;
    let alpha = (age / 300.0).min((r.stays_ms as f32 - age) / 600.0).clamp(0.0, 1.0);
    let picture = egui::vec2(84.0, 68.0);
    let room = ctx.content_rect().width();
    egui::Area::new(egui::Id::new("reveal-toast"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, from_top))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_opacity(alpha);
            // **The words' width is worked out here and never left to the
            // area.** An area remembers the size of what it showed last and
            // hands that to the next thing it shows, and a label that wraps
            // never asks for more: so after a short *Level 5*, a card's name
            // was set one letter to a line in the little width that was left
            // beside its picture (the owner's screenshot, 2026-09-19).
            let widest = [(r.title.as_str(), 18.0), (r.text.as_str(), 14.0)]
                .iter()
                .map(|(words, size)| {
                    ui.painter().layout_no_wrap((*words).to_string(), FontId::proportional(*size), theme::TEXT).size().x
                })
                .fold(0.0_f32, f32::max);
            let beside = if r.card.is_some() { picture.x + 12.0 } else { 0.0 };
            let words = (widest + 4.0).clamp(120.0, (room - beside - 90.0).clamp(160.0, 440.0)).ceil();
            // Several cards are one reveal and their names take a second line:
            // the words stay level with the middle of the picture, by the lines
            // they will really take -- the ones written, and any the width adds.
            let text_lines: f32 = r
                .text
                .lines()
                .map(|line| {
                    let wide = ui.painter().layout_no_wrap(line.to_string(), FontId::proportional(14.0), theme::TEXT).size().x;
                    (wide / words).ceil().max(1.0)
                })
                .sum::<f32>()
                .clamp(1.0, 4.0);
            let words_high = 24.0 + 20.0 * text_lines;
            egui::Frame::new()
                .fill(theme::FELT_MID)
                .stroke(Stroke::new(1.0, theme::GOLD_EDGE))
                .corner_radius(12.0)
                .inner_margin(egui::Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        if let Some(card) = r.card {
                            let (rect, _) = ui.allocate_exact_size(picture, egui::Sense::hover());
                            super::album::picture(&ui.painter_at(rect), rect, card.art, card.suit);
                            ui.add_space(4.0);
                        }
                        ui.allocate_ui_with_layout(
                            egui::vec2(words, 0.0),
                            egui::Layout::top_down(egui::Align::LEFT),
                            |ui| {
                                ui.set_min_width(words);
                                ui.set_max_width(words);
                                ui.spacing_mut().item_spacing.y = 2.0;
                                // The words, level with the middle of the picture.
                                if r.card.is_some() {
                                    ui.add_space(((picture.y - words_high) / 2.0).max(0.0));
                                }
                                ui.add(egui::Label::new(RichText::new(&r.title).color(theme::GOLD_ACTION).size(18.0).strong()).wrap());
                                ui.add(egui::Label::new(RichText::new(&r.text).color(theme::TEXT).size(14.0)).wrap());
                            },
                        );
                    });
                });
        });
    super::table::paint_again(ctx, std::time::Duration::from_millis(80));
}

/// `D-068`: a figure in a column of its own, right-aligned, and the words
/// beside it wrapping under their own first word.
fn figure_and_words(ui: &mut egui::Ui, column: f32, figure: &str, words: RichText) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(column, 16.0), egui::Sense::hover());
        ui.painter().text(
            egui::pos2(rect.right(), rect.top()),
            egui::Align2::RIGHT_TOP,
            figure,
            FontId::proportional(13.0),
            theme::GOLD_EDGE,
        );
        let width = ui.available_width().max(40.0);
        ui.allocate_ui_with_layout(egui::vec2(width, 0.0), egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.set_max_width(width);
            ui.add(egui::Label::new(words).wrap());
        });
    });
}

/// `D-068`: words that may wrap, with a short figure kept at the right of
/// their first line; the words wrap under themselves and never under it.
fn words_and_figure(ui: &mut egui::Ui, words: RichText, figure: &str) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let figure_w =
            ui.painter().layout_no_wrap(figure.to_string(), FontId::proportional(12.5), theme::TEXT_DIM).size().x.ceil();
        let width = (ui.available_width() - figure_w - 8.0).max(40.0);
        ui.allocate_ui_with_layout(egui::vec2(width, 0.0), egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            ui.add(egui::Label::new(words).wrap());
        });
        let (rect, _) = ui.allocate_exact_size(egui::vec2(figure_w, 16.0), egui::Sense::hover());
        ui.painter().text(
            egui::pos2(rect.right(), rect.top() + 1.0),
            egui::Align2::RIGHT_TOP,
            figure,
            FontId::proportional(12.5),
            theme::TEXT_DIM,
        );
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
        seat_dots(ui, r, super::lobby::row_needed(view, &r.key));
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

fn info_column(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi) -> Option<LobbyAction> {
    // `D-068`: one scrolling column, as the narrow layouts have: the card
    // about the player is taller with its quests, and at 200 % text nothing of
    // it may fall off the pane.
    let mut action = None;
    scroller(egui::ScrollArea::vertical())
        .id_salt("info")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            action = you_card(ui, view);
            ui.add_space(10.0);
            group(ui, "Table", |ui| match view.selected_row() {
                Some(r) => table_info(ui, r, view, state),
                None => how_it_works(ui),
            });
        });
    action
}

/// The people and the card in one scrolling column, for a window too narrow
/// for three.
fn side_column(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi) -> LobbyAction {
    let mut action = LobbyAction::None;
    scroller(egui::ScrollArea::vertical())
        .id_salt("side")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if let Some(a) = you_card(ui, view) {
                action = a;
            }
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
///
/// `D-071`: and in its middle the one button that leaves the client, for the
/// donation page. Returns whether that was pressed.
fn network_strip(ui: &mut egui::Ui, view: &LobbyView, state: &mut LobbyUi) -> bool {
    let s = &view.status;
    let (word, tone) = s.headline();
    let colour = tone_colour(tone);
    // Three things on one row, and the middle one in the middle of the strip
    // rather than wherever the other two leave off: so the row is laid out by
    // hand. It is allocated first, to lie under what is drawn on it.
    let row = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), DONATE_HEIGHT));
    ui.allocate_rect(row, egui::Sense::hover());

    // The right end first: what it takes decides where the middle may sit.
    let details = ui
        .scope_builder(
            egui::UiBuilder::new()
                .max_rect(row)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
            |ui| {
                let label = if state.diagnostics_open { "Hide details" } else { "Network details" };
                if ui
                    .add(egui::Button::new(RichText::new(label).color(theme::TEXT_DIM).size(14.0)).frame(false))
                    .clicked()
                {
                    state.diagnostics_open = !state.diagnostics_open;
                    ui.ctx().request_repaint();
                }
            },
        )
        .response
        .rect;

    let light = 12.0 + ui.spacing().item_spacing.x;
    let word_width = egui::WidgetText::from(RichText::new(word).strong())
        .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body)
        .size()
        .x;
    let widths = DONATE_FORMS.map(|words| donate_width(ui, words));
    let mut donate = false;
    let mut word_ends = details.left() - STRIP_GAP;
    if let Some((form, x)) = donate_place(
        row.left() + light + word_width + STRIP_GAP,
        details.left() - STRIP_GAP,
        row.center().x,
        &widths,
    ) {
        let rect = egui::Rect::from_center_size(
            egui::pos2(x, row.center().y),
            egui::vec2(widths[form], DONATE_HEIGHT - 2.0),
        );
        donate = donate_button(ui, rect, DONATE_FORMS[form]).clicked();
        word_ends = rect.left() - STRIP_GAP;
    }

    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_max(row.min, egui::pos2(word_ends.max(row.left()), row.bottom())))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 5.0, colour);
            if tone == Tone::Ok {
                ui.painter().circle_stroke(rect.center(), 6.5, Stroke::new(1.0, colour.gamma_multiply(0.4)));
            }
            ui.add(egui::Label::new(RichText::new(word).color(colour).strong()).truncate());
        },
    );
    // The children were laid out inside the row; what follows goes under it.
    ui.advance_cursor_after_rect(row);

    if !state.diagnostics_open {
        return donate;
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
    donate
}

/// `D-071`: the strip's row, and the donation button a hair under it.
const DONATE_HEIGHT: f32 = 30.0;
/// The room kept between the strip's three things.
const STRIP_GAP: f32 = 14.0;
/// The forms the donation button takes as the strip narrows: the words, one
/// word, the heart alone.
const DONATE_FORMS: [&str; 3] = ["Support the project", "Support", ""];
const DONATE_HEART: f32 = 15.0;

/// How wide the button is with these words on it.
fn donate_width(ui: &egui::Ui, words: &str) -> f32 {
    if words.is_empty() {
        return DONATE_HEART + 20.0;
    }
    DONATE_HEART + 8.0 + style::text_width(ui.painter(), words, 14.0, style::Weight::DemiBold) + 30.0
}

/// `D-071`: the form of the donation button a strip has room for -- an index
/// into `widths`, widest first -- and the x of its centre: the strip's own
/// centre while that is free, else as near to it as the neighbours let it
/// come. `left` is where the headline ends and `right` where the details
/// button begins, the gaps counted.
///
/// **The headline is never cut for it.** The word about the line is what the
/// strip is for, and the longest of them -- *no way in from the internet* --
/// is the one a player most needs whole. The button gives way form by form,
/// and on a strip with no room for the heart alone it is not drawn.
fn donate_place(left: f32, right: f32, centre: f32, widths: &[f32]) -> Option<(usize, f32)> {
    let (form, width) = widths.iter().enumerate().find(|(_, w)| right - left >= **w)?;
    Some((form, centre.clamp(left + width / 2.0, right - width / 2.0)))
}

/// `D-071`: the button that opens the donation page. A heart and three words
/// on a pill that is warm where everything around it is cool: the one rose
/// thing in the window, which is what brings the eye to it, and nothing else
/// does.
///
/// **It does not move, blink or count.** A frame on the software rasteriser
/// costs half a second (`D-056`), so an idle lobby paints nothing, and a button
/// that pulsed for attention would be the only thing in the client that did.
/// Nor does it appear at a win, or say what others gave: the lobby asks for
/// nothing back (`result_words`), and this asks once, quietly, in one place.
/// Under the pointer it warms and the heart grows -- the pointer's own frames.
fn donate_button(ui: &mut egui::Ui, rect: egui::Rect, words: &str) -> egui::Response {
    let resp = ui
        .interact(rect, ui.id().with("donate"), egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(
            "No house, no rake, no ads: P2Poker is free, and lives on gifts.\n\
             Opens the donation page on GitHub in your browser (Bitcoin, USDT).",
        );
    let hot = resp.hovered();
    let p = ui.painter();
    let r = rect.height() / 2.0;
    style::glow(p, rect, r, if hot { 14.0 } else { 10.0 }, style::faded(theme::ROSE, if hot { 0.50 } else { 0.22 }));
    let [top, bottom] = donate_fill(hot);
    style::gradient_rect(p, rect, r, &[(0.0, top), (1.0, bottom)]);
    p.rect_stroke(
        rect,
        r,
        Stroke::new(1.0, style::mix(theme::ROSE, theme::GOLD_EDGE, if hot { 0.15 } else { 0.45 })),
        egui::StrokeKind::Inside,
    );
    let size = if hot { DONATE_HEART + 2.0 } else { DONATE_HEART };
    let [ink, words_ink] = donate_inks(hot);
    if words.is_empty() {
        heart(p, rect.center(), size, ink);
        return resp;
    }
    let words_width = style::text_width(p, words, 14.0, style::Weight::DemiBold);
    let left = rect.center().x - (DONATE_HEART + 8.0 + words_width) / 2.0;
    heart(p, egui::pos2(left + DONATE_HEART / 2.0, rect.center().y), size, ink);
    style::text(
        p,
        egui::pos2(left + DONATE_HEART + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        words,
        14.0,
        style::Weight::DemiBold,
        words_ink,
    );
    resp
}

/// The donation button's fill, top and bottom: the panel's cool greys warmed
/// towards the rose, and further under the pointer.
fn donate_fill(hot: bool) -> [Color32; 2] {
    [
        style::mix(theme::PANEL_LIGHT, theme::ROSE, if hot { 0.40 } else { 0.24 }),
        style::mix(theme::PANEL, theme::ROSE, if hot { 0.24 } else { 0.12 }),
    ]
}

/// The heart's colour and the words' on that fill: the rose and the felt's
/// warm cream, both lit under the pointer.
fn donate_inks(hot: bool) -> [Color32; 2] {
    if hot {
        [style::mix(theme::ROSE, Color32::WHITE, 0.18), Color32::WHITE]
    } else {
        [theme::ROSE, theme::ON_FELT_DIM]
    }
}

/// A heart `width` across, centred on `at`: a square stood on its corner with a
/// disc on each of its two upper sides. Drawn as shapes because the fonts'
/// heart is a suit -- the album's, the cards' -- and sits off-centre in its
/// line. One opaque colour, so the three pieces' soft edges vanish into each
/// other where they overlap.
fn heart(p: &egui::Painter, at: egui::Pos2, width: f32, colour: Color32) {
    // With `d` the square's half diagonal and the discs' radius `d / sqrt 2`, the
    // shape is `d (1 + sqrt 2)` across, reaches `d / 2 + d / sqrt 2` above the
    // square's centre and `d` below it: so that centre lies half the difference
    // under the middle of the shape, which is what `at` names.
    let d = width / (1.0 + std::f32::consts::SQRT_2);
    let lobe = d * std::f32::consts::FRAC_1_SQRT_2;
    let top = d / 2.0 + lobe;
    let mid = egui::pos2(at.x, at.y + (top - d) / 2.0);
    p.circle_filled(egui::pos2(mid.x - d / 2.0, mid.y - d / 2.0), lobe, colour);
    p.circle_filled(egui::pos2(mid.x + d / 2.0, mid.y - d / 2.0), lobe, colour);
    p.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(mid.x, mid.y + d),
            egui::pos2(mid.x + d, mid.y),
            egui::pos2(mid.x, mid.y - d),
            egui::pos2(mid.x - d, mid.y),
        ],
        colour,
        Stroke::NONE,
    ));
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
            theme::ROSE,
        ] {
            assert!(
                theme::separation(theme::PANEL, c) >= 150,
                "a colour drawn on a panel is not legible on it"
            );
        }
    }

    /// **`D-075`: the version check says what it found in words a player can
    /// act on, never sends them to an address the network chose, and a failure
    /// is never read as good news.**
    #[test]
    fn the_version_check_says_what_it_found_and_opens_only_its_own_page() {
        use crate::app::update::Verdict;
        assert_eq!(update_words(&UpdateUi::Idle), None, "nothing is said before it is asked");
        let words = |u: UpdateUi| update_words(&u).expect("said").0;
        assert!(words(UpdateUi::Checking).contains("Asking GitHub"));
        let newest = words(UpdateUi::Done(Ok(Verdict::Newest { latest: "0.1.0".into() })));
        assert!(newest.contains("newest version") && newest.contains("0.1.0"), "{newest}");
        assert!(words(UpdateUi::Opening).contains("Asking GitHub"), "the window's own question reads the same");
        let newer = words(UpdateUi::Done(Ok(Verdict::Newer { latest: "0.2.0".into(), tag: "v0.2.0".into() })));
        assert!(newer.contains("Version 0.2.0 is out") && newer.contains("releases page"), "{newer}");
        assert!(newer.contains("player profile stays"), "an update must not read as a new player: {newer}");
        assert!(words(UpdateUi::Done(Ok(Verdict::NoRelease))).contains("No release"));
        // A failure is a warning with a way on, not a verdict.
        let (failed, tone) = update_words(&UpdateUi::Done(Err("GitHub could not be reached: timed out".into()))).unwrap();
        assert!(failed.contains("could not be reached") && failed.contains("releases page"), "{failed}");
        assert_eq!(tone, theme::WARN);
        assert_ne!(update_words(&UpdateUi::Done(Ok(Verdict::Newest { latest: "1".into() }))).unwrap().1, theme::WARN);

        // The page it opens is a constant of the build, in this project's own repository.
        assert_eq!(RELEASES_URL, "https://github.com/qavryxdevv/P2Poker/releases");
        assert!(!RELEASES_URL.contains('?'));
        let repo = |u: &str| u.split('/').take(5).collect::<Vec<_>>().join("/");
        assert_eq!(repo(RELEASES_URL), repo(DONATION_URL));
        assert_eq!(repo(RELEASES_URL), repo(BUG_REPORT_URL));
        assert!(crate::app::update::RELEASES_API.contains("/repos/qavryxdevv/P2Poker/"), "and the question goes to the same one");
    }

    /// **`D-077`: the lobby's last word is the gate's.** Whatever the columns,
    /// the dialogs and the small windows produced in a frame, it passes through
    /// `lobby::through_the_gate` last -- read from the source, because it is a
    /// frame's wiring and not a decision: the decisions are tested next door.
    #[test]
    fn the_lobby_ends_at_the_gate() {
        let code = include_str!("render.rs").replace("\r\n", "\n");
        let start = code.find("pub fn lobby(ui: &mut egui::Ui").expect("the lobby");
        let body = &code[start..start + code[start..].find("\n}\n").expect("its end")];
        let gate = body.find("let gate = super::lobby::gate(&state.update);").expect("the gate is asked");
        let through = body.find("action = super::lobby::through_the_gate(&gate, action, from_gate);").expect("and passed");
        let asked = body.rfind("if let Some(what) = asked {").expect("the small windows");
        assert!(asked < gate && gate < through, "the gate after everything else that sets the action");
        assert!(body[through..].trim_end().ends_with("}\n\n    action"), "and nothing after it but the answer");
    }

    /// **`D-072`: the About page says what this build is, and where a bug
    /// goes.**
    ///
    /// The word and the number are separate on purpose: the number is the
    /// crate's, so it cannot be forgotten at a release, and the word is one
    /// constant, so a build that stops being a beta is one line. The bug
    /// address is `https` and the project's own, like the donation page, and it
    /// carries no query -- nothing of this machine or this player is in it.
    #[test]
    fn the_about_page_says_the_build_and_where_a_bug_goes() {
        assert_eq!(RELEASE_STAGE, "beta", "this build is a beta and says so");
        assert_eq!(env!("CARGO_PKG_VERSION"), "0.1.4", "the number comes from Cargo.toml");
        assert!(BUG_REPORT_URL.starts_with("https://github.com/"), "{BUG_REPORT_URL}");
        assert!(BUG_REPORT_URL.ends_with("/issues/new"), "the issue form, already open");
        assert!(!BUG_REPORT_URL.contains('?'), "no query: the client sends nothing with it");
        // The two addresses this client will open are the same repository.
        let repo = |u: &str| u.split('/').take(5).collect::<Vec<_>>().join("/");
        assert_eq!(repo(BUG_REPORT_URL), repo(DONATION_URL));
        // And About is a page of the settings, last, after the profile.
        assert_eq!(SettingsTab::ALL.len(), 6);
        assert_eq!(SettingsTab::ALL.last(), Some(&SettingsTab::About));
        assert_eq!(SettingsTab::About.label(), "About");
    }

    /// `D-071`: the donation button sits in the middle of the strip while the
    /// middle is free, moves no further from it than its neighbours push it,
    /// and gives way to the headline form by form: the word about the line is
    /// never cut for it.
    #[test]
    fn the_donation_button_keeps_the_middle_and_gives_way_to_the_headline() {
        let widths = [190.0, 100.0, 35.0];
        assert_eq!(donate_place(200.0, 1_000.0, 600.0, &widths), Some((0, 600.0)), "a wide strip: the words, centred");
        assert_eq!(
            donate_place(200.0, 650.0, 600.0, &widths),
            Some((0, 555.0)),
            "the details button near the middle: the words still, against it"
        );
        assert_eq!(
            donate_place(520.0, 700.0, 600.0, &widths),
            Some((1, 600.0)),
            "the long headline: one word, and the headline whole"
        );
        assert_eq!(donate_place(560.0, 600.0, 600.0, &widths), Some((2, 582.5)), "the heart alone, left of the middle");
        assert_eq!(donate_place(300.0, 330.0, 600.0, &widths), None, "no room: not drawn");
        assert_eq!(donate_place(700.0, 650.0, 600.0, &widths), None, "the neighbours overlap: not drawn");
        // Wherever the neighbours stand, the button lies between them.
        for left in (0..1_200).step_by(37) {
            for right in (0..1_200).step_by(41) {
                let (left, right) = (left as f32, right as f32);
                if let Some((form, x)) = donate_place(left, right, 600.0, &widths) {
                    let half = widths[form] / 2.0;
                    assert!(x - half >= left && x + half <= right, "{left}..{right}: form {form} at {x}");
                    assert!(form == 0 || right - left < widths[form - 1], "{left}..{right}: a wider form had room");
                }
            }
        }
    }

    /// `D-071`: the button's words and its heart are legible on its own fill,
    /// which is no palette surface: at rest and under the pointer, at the top
    /// of the gradient and at the bottom.
    #[test]
    fn the_donation_buttons_words_are_legible_on_it() {
        for hot in [false, true] {
            for fill in donate_fill(hot) {
                for ink in donate_inks(hot) {
                    assert!(theme::separation(fill, ink) >= 150, "hot {hot}: {ink:?} on {fill:?}");
                }
            }
        }
        // The forms are what `donate_place` is given, widest first.
        assert!(DONATE_FORMS[0].len() > DONATE_FORMS[1].len() && DONATE_FORMS[2].is_empty());
        // And the page it opens is the project's own, over https.
        assert!(DONATION_URL.starts_with("https://github.com/") && DONATION_URL.ends_with("/DONATE.md"));
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
