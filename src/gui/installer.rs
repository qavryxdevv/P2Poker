//! `D-073`: the installer's window -- one question, the work, and one page
//! that says what was really done and where it is.
//!
//! The same split as the lobby's: **what is said is decided by plain
//! functions** ([`offer_words`], [`done_lines`]) that a test reads, and the
//! drawing below puts their answers on the screen. The work itself -- hashing,
//! copying, the shell's COM -- never runs on the paint thread: the window asks,
//! and is told.
//!
//! The window never touches the profile and never starts the node. That is
//! what makes the owner's rule hold by construction -- *after OK the
//! application closes, so that two instances are not running*: this process was
//! never a client, and the installed copy is started only as this one ends.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, RichText, Stroke};

use super::table::style;
use super::theme;
use crate::install::copy::{hex, Moved};
use crate::install::shell::{self, Choices, ProfileStep, Report, Step};
use crate::install::{self, Installed, Offer, Portable, Relation};

/// Whether *Start P2Poker now* is ticked when the last page opens. One
/// constant, because it is one decision: on, the player is in the lobby a
/// click after installing; off, the first start is from the new shortcut.
pub const START_NOW_BY_DEFAULT: bool = true;

/// The window's inner size, in points.
pub const WINDOW_SIZE: [f32; 2] = [680.0, 660.0];

/// What `main` does once the window has closed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Outcome {
    /// Nothing more: the player closed the window, or pressed OK with *start
    /// now* unticked.
    #[default]
    Quit,
    /// Start this same file again where it stands, as a portable client.
    RunHere,
    /// Start the installed copy.
    Start(PathBuf),
}

enum Page {
    Looking,
    /// This machine would not say where its folders are.
    NoPlaces,
    Offer(Offer),
    Working,
    Done(Report),
    Failed { words: String },
}

pub struct InstallerApp {
    page: Page,
    ticks: Choices,
    start_now: bool,
    source: PathBuf,
    surveyed: Receiver<Option<Offer>>,
    finished: Option<Receiver<Result<Report, String>>>,
    outcome: Arc<Mutex<Outcome>>,
}

impl InstallerApp {
    pub fn new(ctx: &egui::Context, source: PathBuf, outcome: Arc<Mutex<Outcome>>) -> InstallerApp {
        let mut app = InstallerApp {
            page: Page::Looking,
            ticks: Choices { desktop: true, start_menu: true, move_profile: true },
            start_now: START_NOW_BY_DEFAULT,
            source,
            surveyed: channel().1,
            finished: None,
            outcome,
        };
        app.look(ctx);
        app
    }

    /// Read what stands at the install location, off the paint thread: with a
    /// program installed that is two hashes of forty megabytes each.
    fn look(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let (source, ctx) = (self.source.clone(), ctx.clone());
        std::thread::spawn(move || {
            let offer = shell::places().map(|places| {
                let missing = shell::desktop_shortcut_missing(&places);
                install::survey(&source, places, missing)
            });
            let _ = tx.send(offer);
            ctx.request_repaint();
        });
        self.surveyed = rx;
        self.page = Page::Looking;
    }

    fn work(&mut self, ctx: &egui::Context, offer: Offer) {
        let (tx, rx) = channel();
        let (choices, ctx, job) = (self.ticks, ctx.clone(), offer.clone());
        std::thread::spawn(move || {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64);
            let _ = tx.send(shell::install(&job, choices, now).map_err(|e| e.to_string()));
            ctx.request_repaint();
        });
        self.finished = Some(rx);
        self.page = Page::Working;
    }

    fn leave(&mut self, ctx: &egui::Context, outcome: Outcome) {
        if let Ok(mut slot) = self.outcome.lock() {
            *slot = outcome;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl eframe::App for InstallerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Ok(offer) = self.surveyed.try_recv() {
            self.page = offer.map_or(Page::NoPlaces, Page::Offer);
        }
        if let Some(done) = self.finished.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.finished = None;
            self.page = match done {
                Ok(report) => Page::Done(report),
                Err(words) => Page::Failed { words },
            };
        }

        // A close asked for in the second the copy takes is held until it is
        // made: the work is one program and one rename, and a window that went
        // away in the middle of it would leave a partial for the next run to
        // wait two minutes behind.
        if matches!(self.page, Page::Working) && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }

        ui.painter().rect_filled(ui.max_rect(), 0.0, theme::WINDOW);
        band(ui);
        let action = egui::Frame::new()
            .inner_margin(egui::Margin { left: 26, right: 26, top: 16, bottom: 14 })
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match &self.page {
                        Page::Looking => waiting(ui, "Looking at this computer\u{2026}"),
                        Page::Working => waiting(ui, "Copying the program and checking the copy\u{2026}"),
                        Page::NoPlaces => no_places(ui),
                        Page::Offer(offer) => offer_page(ui, offer, &mut self.ticks),
                        Page::Done(report) => done_page(ui, report, &self.source, &mut self.start_now),
                        Page::Failed { words } => failed_page(ui, words),
                    })
                    .inner
            })
            .inner;

        match action {
            Action::None => {}
            Action::Quit => self.leave(&ctx, Outcome::Quit),
            Action::RunHere => self.leave(&ctx, Outcome::RunHere),
            Action::Again => self.look(&ctx),
            Action::Show(path) => shell::show_in_explorer(&path),
            Action::Go => {
                if let Page::Offer(offer) = &self.page {
                    let offer = offer.clone();
                    match primary(&offer, self.ticks) {
                        Primary::Work => self.work(&ctx, offer),
                        Primary::Start => self.leave(&ctx, Outcome::Start(offer.places.installed_exe())),
                    }
                }
            }
            Action::Finish => {
                if let Page::Done(report) = &self.page {
                    let outcome = if self.start_now { Outcome::Start(report.program.clone()) } else { Outcome::Quit };
                    self.leave(&ctx, outcome);
                }
            }
        }
    }
}

enum Action {
    None,
    /// The gold button of the offer.
    Go,
    RunHere,
    Quit,
    /// Look at the machine again: *Try again*.
    Again,
    Show(PathBuf),
    /// OK on the last page.
    Finish,
}

/// What the offer's gold button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primary {
    /// Copy, move, make shortcuts -- whichever of them there is to do.
    Work,
    /// Nothing to do but start the copy that is there.
    Start,
}

/// **The gold button never replaces a newer program, and never shows a page of
/// work for no work.** With the same build or a newer one installed there is
/// nothing to copy, so it is *work* only when a missing desktop shortcut is to
/// be put back -- which `shell::install` does without touching the program.
pub fn primary(offer: &Offer, ticks: Choices) -> Primary {
    // **A profile still to be moved is work, whatever else is settled.** This
    // is the state a failed move leaves behind -- the program in place, the
    // player still beside the old file -- and a gold button that merely
    // *started* the installed copy there would make a new player of it, after
    // which the move could never be made.
    if offer.profile_can_move && ticks.move_profile {
        return Primary::Work;
    }
    match &offer.installed {
        Installed::Nothing => Primary::Work,
        Installed::OtherBuild { relation, .. } if *relation != Relation::Newer => Primary::Work,
        _ if offer.desktop_shortcut_missing && ticks.desktop => Primary::Work,
        _ => Primary::Start,
    }
}

/// The offer in words: the heading, the sentence under it, the two places, the
/// gold button.
///
/// **The places are fields and not part of the sentence.** A user's path is
/// long and wraps in the middle of a name; inside a sentence it buried the
/// sentence (the first photograph of this page). Each gets a well of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferWords {
    pub heading: String,
    pub body: String,
    /// The folder this file was started from.
    pub from: PathBuf,
    /// The folder the program is, or will be, installed in.
    pub home: PathBuf,
    pub button: &'static str,
    /// The checkboxes about shortcuts are shown.
    pub shortcut_boxes: bool,
}

pub fn offer_words(offer: &Offer) -> OfferWords {
    let home = offer.places.install_dir();
    let from = offer.source.parent().map(Path::to_path_buf).unwrap_or_default();
    let (heading, body, button, shortcut_boxes) = match &offer.installed {
        Installed::Nothing => (
            "Give P2Poker a home on this computer?",
            "It can install itself, so that it stays put and is one click away: the program is copied to your user \
             folder and started from there."
                .to_owned(),
            "Install",
            true,
        ),
        Installed::SameBuild if offer.profile_can_move => (
            "Finish installing P2Poker",
            "The program is installed already, but your player profile is still beside this file. It can move now, \
             so that the installed copy is you and not a new player."
                .to_owned(),
            "Finish installing",
            true,
        ),
        Installed::SameBuild => (
            "P2Poker is already installed",
            "This same build is installed on this computer. Start that one: your player profile is there.".to_owned(),
            "Start P2Poker",
            false,
        ),
        Installed::OtherBuild { version, relation: Relation::Newer } => (
            "A newer P2Poker is already installed",
            format!(
                "Installed: version {}. This file is version {}, which is older, so nothing is replaced.",
                version.as_deref().unwrap_or("unknown"),
                offer.this_version
            ),
            "Start the installed P2Poker",
            false,
        ),
        Installed::OtherBuild { version, relation } => (
            "Update the installed P2Poker?",
            match (version, relation) {
                (Some(v), Relation::SameVersion) => format!(
                    "Another build of version {v} is installed. This file is the one you just started, so it takes \
                     its place. Your player profile stays as it is."
                ),
                (Some(v), _) => format!(
                    "Version {v} is installed. This file is version {}. Your player profile stays as it is.",
                    offer.this_version
                ),
                (None, _) => format!(
                    "An earlier build is installed. This file is version {}. Your player profile stays as it is.",
                    offer.this_version
                ),
            },
            "Update",
            true,
        ),
    };
    OfferWords { heading: heading.into(), body, from, home, button, shortcut_boxes }
}

/// Why *run without installing* is not on offer, when it is not.
pub fn portable_words(portable: Portable) -> Option<&'static str> {
    match portable {
        Portable::Fine => None,
        Portable::Temporary => Some(
            "This file is running from a temporary folder \u{2014} from inside an archive, most likely. A player \
             profile kept here would be deleted with it, so running without installing is not offered.",
        ),
        Portable::ReadOnly => {
            Some("The folder this file is in cannot be written to, so there is nowhere here to keep a player profile.")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Good,
    Plain,
    Warn,
}

/// **The last page, as it really went.** One line for every step that was
/// asked for, in the step's own outcome -- a shortcut the system refused is
/// said to have been refused, with the likeliest reason, and is never folded
/// into a cheerful *installed*.
pub fn done_lines(report: &Report, source: &Path) -> Vec<(Tone, String)> {
    let mut lines = Vec::new();
    let name = |p: &Path| p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    match &report.copied {
        Some(c) => lines.push((
            Tone::Plain,
            format!(
                "The copy was checked against this file: {} bytes, SHA-256 {}\u{2026}",
                thousands(c.bytes),
                &hex(&c.sha256)[..16]
            ),
        )),
        None => lines.push((Tone::Plain, "The program that was there is this same build and was left as it is.".into())),
    }
    match &report.profile {
        ProfileStep::NotAsked => {}
        ProfileStep::Moved(Moved::Renamed) => {
            lines.push((Tone::Good, "Your player profile moved with it: you are the same player, album and all.".into()));
        }
        ProfileStep::Moved(Moved::Copied { kept_as }) => {
            let aside = kept_as.file_name().is_some_and(|n| n.to_string_lossy().starts_with("profile.moved-"));
            lines.push((Tone::Good, "Your player profile was copied over and every file of it checked.".into()));
            lines.push(if aside {
                (Tone::Plain, format!("The original was set aside as {} \u{2014} delete it when you like.", kept_as.display()))
            } else {
                (
                    Tone::Warn,
                    format!(
                        "The original could not be set aside and is still at {}. Do not start the old copy again, \
                         or one player will be sitting in two places.",
                        kept_as.display()
                    ),
                )
            });
        }
    }
    for (step, place) in [(&report.desktop, "on your desktop"), (&report.start_menu, "in the Start menu")] {
        match step {
            Step::NotAsked => {}
            Step::Made(lnk) => lines.push((Tone::Good, format!("The shortcut \u{201c}{}\u{201d} is {place}.", name(lnk)))),
            Step::AlreadyThere(lnk) => {
                lines.push((Tone::Plain, format!("The shortcut \u{201c}{}\u{201d} was {place} already.", name(lnk))));
            }
            Step::NoFolder => lines.push((Tone::Warn, format!("This computer did not say where \u{201c}{place}\u{201d} is, so no shortcut went there."))),
            Step::Failed(why) => lines.push((
                Tone::Warn,
                format!(
                    "No shortcut could be put {place}: {why}. Windows\u{2019} ransomware protection (Controlled folder \
                     access) stops programs it does not know from writing there. The program is installed all the same."
                ),
            )),
        }
    }
    if !install::same_place(source, &report.program) {
        lines.push((Tone::Plain, format!("The file you started, {}, is no longer needed on this computer.", source.display())));
    }
    lines
}

fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push('\u{a0}');
        }
        out.push(c);
    }
    out
}

/// The felt band with the mark, as the lobby wears it.
fn band(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 74.0), egui::Sense::hover());
    let p = ui.painter();
    style::gradient_rect(p, rect, 0.0, &[(0.0, theme::FELT_MID), (1.0, theme::FELT_EDGE)]);
    p.hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, theme::GOLD_EDGE));
    let mark = egui::Rect::from_center_size(egui::pos2(rect.left() + 52.0, rect.center().y), egui::vec2(44.0, 44.0));
    p.circle_filled(mark.center(), 22.0, Color32::from_black_alpha(150));
    p.circle_stroke(mark.center(), 21.0, Stroke::new(1.5, theme::GOLD_EDGE));
    p.text(mark.center(), egui::Align2::CENTER_CENTER, "\u{2660}", egui::FontId::proportional(26.0), theme::GOLD_ACTION);
    let left = mark.right() + 16.0;
    style::text(p, egui::pos2(left, rect.center().y - 11.0), egui::Align2::LEFT_CENTER, "P2Poker", 25.0, style::Weight::Bold, theme::TEXT);
    style::text(
        p,
        egui::pos2(left, rect.center().y + 14.0),
        egui::Align2::LEFT_CENTER,
        "Decentralised poker. No house, no server, every card proven.",
        14.0,
        style::Weight::Regular,
        style::mix(theme::TEXT, theme::FELT_LETTERING, 0.35),
    );
}

fn waiting(ui: &mut egui::Ui, words: &str) -> Action {
    ui.add_space(90.0);
    ui.vertical_centered(|ui| {
        ui.add(egui::Spinner::new().size(34.0).color(theme::GOLD_ACTION));
        ui.add_space(14.0);
        ui.label(RichText::new(words).color(theme::TEXT_DIM).size(16.0));
    });
    Action::None
}

fn heading(ui: &mut egui::Ui, words: &str, colour: Color32) {
    ui.label(RichText::new(words).color(colour).size(22.0).strong());
    ui.add_space(6.0);
}

fn para(ui: &mut egui::Ui, words: &str, colour: Color32) {
    ui.add(egui::Label::new(RichText::new(words).color(colour).size(15.0)).wrap());
}

/// A path, in the field's own well so that it reads as a place and wraps where
/// a user's long name makes it.
fn place(ui: &mut egui::Ui, label: &str, path: &Path) -> bool {
    let mut show = false;
    egui::Frame::new()
        .fill(theme::FIELD)
        .stroke(Stroke::new(1.0, theme::LINE))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).color(theme::TEXT_DIM).size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if path.exists() && ui.small_button("Show in folder").clicked() {
                        show = true;
                    }
                });
            });
            ui.add(egui::Label::new(RichText::new(path.display().to_string()).color(theme::TEXT).size(14.5).monospace()).wrap());
        });
    show
}

fn offer_page(ui: &mut egui::Ui, offer: &Offer, ticks: &mut Choices) -> Action {
    let words = offer_words(offer);
    let mut action = Action::None;
    heading(ui, &words.heading, theme::TEXT);
    para(ui, &words.body, theme::TEXT_DIM);
    ui.add_space(10.0);
    if place(ui, "You started it from", &words.from) {
        action = Action::Show(offer.source.clone());
    }
    ui.add_space(6.0);
    let label = if offer.installed == Installed::Nothing { "It will be installed in" } else { "It is installed in" };
    if place(ui, label, &words.home) {
        action = Action::Show(words.home.clone());
    }
    ui.add_space(10.0);

    let boxes = words.shortcut_boxes || offer.desktop_shortcut_missing;
    if boxes {
        let again = if words.shortcut_boxes { "" } else { " again" };
        ui.checkbox(&mut ticks.desktop, RichText::new(format!("Put a shortcut on the desktop{again}")).size(15.0));
        if words.shortcut_boxes {
            ui.checkbox(&mut ticks.start_menu, RichText::new("Put a shortcut in the Start menu").size(15.0));
        }
    }
    if offer.profile_can_move {
        ui.checkbox(
            &mut ticks.move_profile,
            RichText::new("Move my player profile there too \u{2014} identity, album and results").size(15.0),
        );
        if !ticks.move_profile {
            para(ui, "Left unticked, the installed copy starts as a new player and this one stays beside this file.", theme::WARN);
        }
    }
    if offer.two_players {
        ui.add_space(4.0);
        para(
            ui,
            "The installed copy has a player profile of its own, and so does this one. Neither is touched: two \
             profiles are two players, and one is never put over the other.",
            theme::WARN,
        );
    }
    if words.shortcut_boxes {
        ui.add_space(8.0);
        para(
            ui,
            "No administrator rights, nothing in the registry, nothing that starts by itself. To remove it later, \
             delete that folder and the shortcuts \u{2014} your player profile lives in that folder too, so back it up \
             first (Settings, Profile).",
            style::mix(theme::TEXT_DIM, theme::WINDOW, 0.25),
        );
    }

    ui.add_space(16.0);
    ui.horizontal(|ui| {
        // As wide as its words: *Start the installed P2Poker* ran over the edges
        // of a button sized for *Install* -- seen in the photograph, not the diff.
        let wide = style::text_width(ui.painter(), words.button, 18.0, style::Weight::Bold) + 56.0;
        if super::render::gold_button(ui, words.button, egui::vec2(wide.max(200.0), 44.0)).clicked() {
            action = Action::Go;
        }
        ui.add_space(8.0);
        let may = offer.portable == Portable::Fine;
        let label = if offer.installed == Installed::Nothing { "Run without installing" } else { "Run this copy here instead" };
        if ui.add_enabled(may, egui::Button::new(RichText::new(label).size(15.0)).min_size(egui::vec2(0.0, 40.0))).clicked() {
            action = Action::RunHere;
        }
        if ui.add(egui::Button::new(RichText::new("Quit").size(15.0)).min_size(egui::vec2(70.0, 40.0))).clicked() {
            action = Action::Quit;
        }
    });
    ui.add_space(8.0);
    match portable_words(offer.portable) {
        Some(why) => para(ui, why, theme::WARN),
        None if offer.installed == Installed::Nothing => para(
            ui,
            "Run without installing keeps the program and your player profile in the folder this file is in now. \
             Right for a USB stick; fragile in a Downloads folder.",
            style::mix(theme::TEXT_DIM, theme::WINDOW, 0.25),
        ),
        None => {}
    }
    action
}

fn done_page(ui: &mut egui::Ui, report: &Report, source: &Path, start_now: &mut bool) -> Action {
    let mut action = Action::None;
    let title = match &report.copied {
        Some(c) if c.replaced => "P2Poker is updated",
        Some(_) => "P2Poker is installed",
        None => "P2Poker is ready",
    };
    heading(ui, title, theme::OK);
    if place(ui, "The program is here", &report.program) {
        action = Action::Show(report.program.clone());
    }
    ui.add_space(10.0);
    for (tone, words) in done_lines(report, source) {
        let (glyph, colour) = match tone {
            Tone::Good => ("\u{2714}", theme::OK),
            Tone::Plain => ("\u{2022}", theme::TEXT_DIM),
            Tone::Warn => ("!", theme::WARN),
        };
        ui.horizontal_top(|ui| {
            ui.add_sized(egui::vec2(18.0, 20.0), egui::Label::new(RichText::new(glyph).color(colour).size(15.0).strong()));
            let ink = if tone == Tone::Warn { theme::WARN } else { theme::TEXT };
            ui.add(egui::Label::new(RichText::new(words).color(ink).size(15.0)).wrap());
        });
        ui.add_space(3.0);
    }
    ui.add_space(12.0);
    ui.checkbox(start_now, RichText::new("Start P2Poker now").size(15.0));
    ui.add_space(8.0);
    if super::render::gold_button(ui, "OK", egui::vec2(160.0, 44.0)).clicked() {
        action = Action::Finish;
    }
    ui.add_space(6.0);
    para(ui, "This window closes. From now on, start P2Poker from the shortcut.", style::mix(theme::TEXT_DIM, theme::WINDOW, 0.25));
    action
}

fn failed_page(ui: &mut egui::Ui, words: &str) -> Action {
    let mut action = Action::None;
    heading(ui, "P2Poker was not installed", theme::WARN);
    para(ui, words, theme::TEXT);
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        if super::render::gold_button(ui, "Try again", egui::vec2(180.0, 44.0)).clicked() {
            action = Action::Again;
        }
        ui.add_space(8.0);
        if ui.add(egui::Button::new(RichText::new("Quit").size(15.0)).min_size(egui::vec2(70.0, 40.0))).clicked() {
            action = Action::Quit;
        }
    });
    action
}

fn no_places(ui: &mut egui::Ui) -> Action {
    let mut action = Action::None;
    heading(ui, "P2Poker cannot install itself here", theme::WARN);
    para(
        ui,
        "This computer did not say where this user\u{2019}s programs folder is. P2Poker still runs from the folder \
         it is in: nothing else is needed.",
        theme::TEXT,
    );
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        if super::render::gold_button(ui, "Run it here", egui::vec2(200.0, 44.0)).clicked() {
            action = Action::RunHere;
        }
        ui.add_space(8.0);
        if ui.add(egui::Button::new(RichText::new("Quit").size(15.0)).min_size(egui::vec2(70.0, 40.0))).clicked() {
            action = Action::Quit;
        }
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::copy::Copied;
    use crate::install::Places;

    fn offer(installed: Installed) -> Offer {
        Offer {
            source: PathBuf::from(r"C:\Users\someone\Downloads\p2p-poker.exe"),
            places: Places::under(Path::new(r"C:\R")),
            installed,
            this_version: "0.1.0".into(),
            portable: Portable::Fine,
            profile_can_move: false,
            two_players: false,
            desktop_shortcut_missing: true,
        }
    }

    fn ticks(desktop: bool) -> Choices {
        Choices { desktop, start_menu: true, move_profile: true }
    }

    /// **`D-073`: the gold button never replaces a newer program.** With a
    /// newer build installed it starts that build; the only work it will ever
    /// do there is put a missing shortcut back, which copies nothing.
    #[test]
    fn the_gold_button_never_downgrades() {
        let newer = offer(Installed::OtherBuild { version: Some("9.0.0".into()), relation: Relation::Newer });
        assert_eq!(offer_words(&newer).button, "Start the installed P2Poker");
        assert!(offer_words(&newer).body.contains("nothing is replaced"));
        assert_eq!(primary(&newer, ticks(false)), Primary::Start);
        let settled = Offer { desktop_shortcut_missing: false, ..newer.clone() };
        assert_eq!(primary(&settled, ticks(true)), Primary::Start, "nothing to do: no page of work for no work");

        for relation in [Relation::Older, Relation::SameVersion] {
            let other = offer(Installed::OtherBuild { version: Some("0.1.0".into()), relation });
            assert_eq!(offer_words(&other).button, "Update");
            assert_eq!(primary(&other, ticks(false)), Primary::Work);
        }
        assert_eq!(offer_words(&offer(Installed::Nothing)).button, "Install");
        assert_eq!(primary(&offer(Installed::Nothing), ticks(false)), Primary::Work);
        assert_eq!(offer_words(&offer(Installed::SameBuild)).button, "Start P2Poker");
    }

    /// **A failed move leaves the program in place and the player beside the old
    /// file, and the next look must not mistake that for *installed*.** The gold
    /// button that only *started* the installed copy there would make a new
    /// player of it, and the move could never be made afterwards.
    #[test]
    fn a_profile_still_to_be_moved_is_never_skipped_over() {
        let half = Offer { profile_can_move: true, desktop_shortcut_missing: false, ..offer(Installed::SameBuild) };
        assert_eq!(primary(&half, ticks(false)), Primary::Work, "the player was left behind");
        let words = offer_words(&half);
        assert_eq!(words.button, "Finish installing");
        assert!(words.body.contains("still beside this file"), "{}", words.body);
        // Unticked on purpose, it is the player's own choice and is obeyed.
        let declined = Choices { desktop: false, start_menu: false, move_profile: false };
        assert_eq!(primary(&half, declined), Primary::Start);
        // And with a newer program there, the profile still moves -- the program is not touched.
        let newer = Offer { profile_can_move: true, ..offer(Installed::OtherBuild { version: Some("9.0.0".into()), relation: Relation::Newer }) };
        assert_eq!(primary(&newer, ticks(false)), Primary::Work);
    }

    /// The first page names both places in full: where the file was started
    /// from and where the copy will go. A player who is told nothing about
    /// where a program puts itself has no reason to trust it.
    #[test]
    fn the_offer_names_where_it_was_started_and_where_it_will_go() {
        for installed in [Installed::Nothing, Installed::SameBuild] {
            let words = offer_words(&offer(installed));
            assert_eq!(words.from, Path::new(r"C:\Users\someone\Downloads"));
            assert_eq!(words.home, Path::new(r"C:\R\Programs\P2Poker"));
            // And not inside the sentence, where a long path buries it.
            assert!(!words.body.contains(r"C:\"), "{}", words.body);
        }
        assert!(offer_words(&offer(Installed::Nothing)).shortcut_boxes);
    }

    #[test]
    fn a_folder_that_cannot_keep_a_profile_is_not_offered_as_a_home() {
        assert_eq!(portable_words(Portable::Fine), None);
        assert!(portable_words(Portable::Temporary).is_some_and(|w| w.contains("archive")));
        assert!(portable_words(Portable::ReadOnly).is_some_and(|w| w.contains("cannot be written")));
    }

    fn report(desktop: Step, start_menu: Step) -> Report {
        Report {
            program: PathBuf::from(r"C:\R\Programs\P2Poker\p2p-poker.exe"),
            copied: Some(Copied { sha256: [0xAB; 32], bytes: 45_078_528, replaced: false }),
            profile: ProfileStep::NotAsked,
            desktop,
            start_menu,
        }
    }

    /// **The last page says what happened, step by step -- the owner's two
    /// sentences first: the shortcut is on the desktop, and where the copy is.**
    #[test]
    fn the_last_page_says_where_the_shortcut_and_the_copy_are() {
        let source = Path::new(r"C:\Users\someone\Downloads\p2p-poker.exe");
        let made = report(Step::Made(r"C:\R\Desktop\P2Poker.lnk".into()), Step::Made(r"C:\R\StartMenu\P2Poker.lnk".into()));
        let lines = done_lines(&made, source);
        let all: String = lines.iter().map(|(_, w)| w.as_str()).collect::<Vec<_>>().join("\n");
        assert!(all.contains("The shortcut \u{201c}P2Poker\u{201d} is on your desktop."), "{all}");
        assert!(all.contains("in the Start menu"), "{all}");
        assert!(all.contains("45\u{a0}078\u{a0}528 bytes") && all.contains("abababababababab"), "{all}");
        assert!(all.contains(r"C:\Users\someone\Downloads\p2p-poker.exe, is no longer needed"), "{all}");
        assert!(lines.iter().all(|(tone, _)| *tone != Tone::Warn), "a clean run warns of nothing");
    }

    /// A refused shortcut is said to have been refused. The break this must
    /// catch is the cheerful page: *installed*, with the one thing the player
    /// was going to click not there.
    #[test]
    fn a_refused_shortcut_is_said_to_have_been_refused() {
        let source = Path::new(r"C:\Users\someone\Downloads\p2p-poker.exe");
        let refused = report(Step::Failed("Access is denied. (0x80070005)".into()), Step::Made(r"C:\R\StartMenu\P2Poker (2).lnk".into()));
        let lines = done_lines(&refused, source);
        let warn: Vec<&String> = lines.iter().filter(|(t, _)| *t == Tone::Warn).map(|(_, w)| w).collect();
        assert_eq!(warn.len(), 1);
        assert!(warn[0].contains("No shortcut could be put on your desktop") && warn[0].contains("Access is denied"));
        assert!(warn[0].contains("installed all the same"));
        // The name actually taken is the name said.
        assert!(lines.iter().any(|(_, w)| w.contains("\u{201c}P2Poker (2)\u{201d} is in the Start menu")));
        // And a box left unticked says nothing at all.
        let quiet = done_lines(&report(Step::NotAsked, Step::NotAsked), source);
        assert!(!quiet.iter().any(|(_, w)| w.contains("hortcut")), "{quiet:?}");
    }

    /// A profile whose original could not be set aside is a warning, in words
    /// that say what not to do.
    #[test]
    fn a_profile_left_in_two_places_is_a_warning() {
        let source = Path::new(r"E:\stick\p2p-poker.exe");
        let mut r = report(Step::NotAsked, Step::NotAsked);
        r.profile = ProfileStep::Moved(Moved::Copied { kept_as: r"E:\stick\profile.moved-17".into() });
        assert!(done_lines(&r, source).iter().all(|(t, _)| *t != Tone::Warn));
        r.profile = ProfileStep::Moved(Moved::Copied { kept_as: r"E:\stick\profile".into() });
        let lines = done_lines(&r, source);
        assert!(lines.iter().any(|(t, w)| *t == Tone::Warn && w.contains("two places")), "{lines:?}");
    }

    #[test]
    fn a_big_number_is_grouped_in_threes() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1\u{a0}000");
        assert_eq!(thousands(45_078_528), "45\u{a0}078\u{a0}528");
    }
}
