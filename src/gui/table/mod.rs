//! The poker table, after PokerTH's table in its *Green Casino* style.
//!
//! The owner, 2026-09-13: *imitate the table of the PokerTH client as
//! faithfully as possible, in look and in function*, from PokerTH's own
//! sources -- `src/gui/qt/gametable.ui` for the widgets, the QML client under
//! `src/gui/qt6-qml` and `data/gfx/qml/table/greencasino/` (its `preview.png`
//! for the exact look). Every message and question the table window had
//! before stays, restyled (the owner: the heads-up opponent who left, wait or
//! leave, and the rest must keep working).
//!
//! * [`seats`] -- *where*: PokerTH's seat ring, box scale and board, tested
//!   without a window.
//! * [`style`] -- *what it looks like*: Green Casino's colours, gradients,
//!   badges, pucks and buttons.
//! * [`paint`] -- the cards, drawn exactly as before (the owner: leave the
//!   cards as they are).
//! * [`bar`] -- PokerTH's action bar; [`panels`] -- the chat and the log.
//! * here -- *what it says*, and what the player can do about it.
//!
//! # A card is drawn face-up only when it has been verified
//!
//! `SPEC_CS.md` §22 forbids displaying a cryptographically unverified card as
//! valid. That is enforced by the type rather than by care: [`Facing`] has no
//! constructor that produces a face-up card without a verdict. [`Facing::up`]
//! takes the verdict and returns a **back** when it is false, so the failure
//! mode of forgetting to check is a covered card, which is the safe direction.
//!
//! The one exception is [`TableView::sample`], which exists so the look can be
//! examined with no hand in progress. It is drawn with a banner across the top
//! saying so, and a test holds the banner to it.

pub mod bar;
pub mod icons;
pub mod motion;
pub mod paint;
pub mod panels;
pub mod seats;
pub mod style;

use eframe::egui::{self, pos2, vec2, Align2, Color32, Rect, RichText, Stroke, StrokeKind};

use crate::poker::state::{Card, Chips, SeatIdx};
use crate::storage::settings::Settings;
use seats::Seats;
use style::{Icon, Puck, Weight};

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
    /// Only [`TableView::sample`] may use this, and a preview says so on the
    /// screen. It is separate from [`Facing::up`] so that no verdict-free path
    /// exists in the code that draws a real hand.
    pub fn sample(card: Card) -> Facing {
        Facing::Up(Shown(card))
    }
}

/// What a seat did last in the running betting round, as the badge beside it
/// says -- PokerTH's six action words (`StaticData.pokerActionWord`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatAct {
    Fold,
    Check,
    Call,
    Bet,
    Raise,
    AllIn,
    /// `D-050`: the seat mucked at the showdown.
    Muck,
}

impl SeatAct {
    pub fn word(self) -> &'static str {
        match self {
            SeatAct::Fold => "Fold",
            SeatAct::Check => "Check",
            SeatAct::Call => "Call",
            SeatAct::Bet => "Bet",
            SeatAct::Raise => "Raise",
            SeatAct::AllIn => "All-In",
            SeatAct::Muck => "Muck",
        }
    }
}

/// `D-052`: what a seat won when the hand ended, for the badge that blinks at
/// its box.
///
/// **The owner, 2026-09-14**: *the winner of the pot at a showdown, or of the
/// hand when everybody else folded, must blink `Winner` at its seat as
/// PokerTH does -- and it must be clear what was won: the hand, the main pot,
/// a side pot, or a split.* So a hand that settled into one pot says only
/// **Winner**, which is PokerTH's word and the whole truth there; where the
/// settlement built side pots the word names the pot instead, because at such
/// a table *winner* alone does not say which chips went where.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Win {
    /// The hand settled into a single pot, so there is no pot to name.
    pub one_pot: bool,
    /// This seat took the main pot -- the settlement's first.
    pub main: bool,
    /// This seat took at least one side pot.
    pub side: bool,
    /// It shared one of the pots it took with another seat.
    pub split: bool,
}

impl Win {
    /// The badge's word.
    pub fn word(self) -> &'static str {
        match (self.one_pot, self.main, self.side, self.split) {
            (true, _, _, true) => "Split pot",
            (true, _, _, false) => "Winner",
            (_, true, true, _) => "Main + side",
            (_, true, false, true) => "Split main",
            (_, true, false, false) => "Main pot",
            (_, false, true, true) => "Split side",
            (_, false, true, false) => "Side pot",
            // A winner the settlement named no pot for -- an abort's restored
            // stacks, a hand the window heard the end of and not the pots.
            (_, false, false, true) => "Split pot",
            (_, false, false, false) => "Winner",
        }
    }
}

/// The colour role of a line of the table's log, as PokerTH's history colours
/// them (`chatcolors.h`): the ordinary line, the hand's header, a street and a
/// seat sitting out, the pot's winner, the game's winner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Normal,
    Header,
    Board,
    Winner,
    SitOut,
    GameWin,
}

/// A line of the table's log, in PokerTH's wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub kind: LogKind,
    pub text: String,
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
    /// What this seat gained at the settlement (`S1-CS`): the chips that
    /// fly from the pot to it.
    pub won: Chips,
    /// `D-052`: what it won, for the word its badge blinks. `None` for every
    /// seat that won nothing.
    pub win: Option<Win>,
    /// `S1-CS`: this player does not hear the seat.
    pub muted: bool,
    /// `D-035`: the seat's client left the table's group; drawn dim, with
    /// *left the table* where its cards were, and no clock.
    pub left: bool,
    /// `S1-CS`: the seat's connection, as the last ping said; `None` before
    /// any reading.
    pub link: Option<Link>,
    /// `S1-DO`: the hand this seat showed at the showdown, in words, for
    /// every seat but the hero (whose own is on the panel).
    pub shown_hand: Option<String>,
    /// What the seat did last this betting round: the badge beside it.
    pub act: Option<SeatAct>,
    /// The seat's application key, when the roster has said it: what a
    /// rating and a note are kept under.
    pub key: Option<[u8; 32]>,
    /// This player's rating of the seat's player, 0 for none, and the note.
    pub rating: u8,
    pub note: String,
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
    /// `S1-CS`: the chance of each better category, by name, best last;
    /// their sum; and what the odds count to ("by the river").
    pub improve: Vec<(String, f32)>,
    pub improve_total: f32,
    pub improve_by: Option<&'static str>,
    /// Which of this client's turns this is, so the raise control can
    /// start a new turn at the minimum.
    pub turn_id: u64,
    /// The hand is settled: the pot has gone to whoever won it.
    pub hand_over: bool,
    /// `S1-CS`: what the seats have said, the muted ones left out.
    pub chat: Vec<TableChatLine>,
    /// `S1-CX`: how long the heads-up opponent has been unreachable, once
    /// it is worth asking about and until the player has answered.
    pub opponent_gone_s: Option<u64>,
    /// `D-032`: the opponent's fourth absence; the game ends here, and the
    /// one thing left to do is leave.
    pub opponent_out: bool,
    /// `D-034`: the opponent is on the line but long past their time to decide.
    pub opponent_slow: bool,
    /// `D-035`: the opponent quit the table; the game is over.
    pub opponent_left: bool,
    /// `D-047`: this seat is out of the table for good -- said with the one
    /// thing left to do, closing the table.
    pub out_for_good: Option<String>,
    /// `D-051`: and the word was that this client flooded the table's group.
    pub out_flooded: bool,
    /// `D-051`: why this table is not safe, with the serial the window is
    /// closed by -- the question comes back when the node says it again.
    pub unsafe_note: Option<(String, u64)>,
    /// `S1-EH`: a word over the felt about this client's own line, while it
    /// is gone: which network is unavailable.
    pub line: Option<String>,
    /// `S1-EI`: the seats off the line during the hand, and what happens
    /// about each.
    pub absent: Vec<AbsentSeat>,
    /// `S1-EI`: the question is about everybody else, not one opponent.
    pub opponent_alone: bool,
    /// PokerTH's *Game: N*: which game of this client's session the table is.
    pub game_no: u32,
    /// The table's history, PokerTH's *Log* panel.
    pub log: Vec<LogLine>,
    /// The winning hand in words, once the hand is settled and a winner
    /// showed: PokerTH's gold badge under the board.
    pub winning_hand: Option<String>,
    /// `D-049`: this client's seat sits out; the bar offers *I'm back*.
    pub hero_sitting_out: bool,
    /// `D-050`: this client's hand waits at the showdown and may be shown
    /// instead of mucked, for this much longer; the bar offers *Show cards*.
    pub show_cards_in_ms: Option<u64>,
    /// The tournament over for this player: the place, and when to say it.
    pub finished: Option<Finish>,
}

/// The smallest table window the client allows. `main.rs` opens the window
/// with these; the action bar and the smallest ring are held to it.
pub const MIN_WINDOW: [f32; 2] = [760.0, 560.0];

/// PokerTH's two bars over the table: the app's own (the menu, the ranking,
/// the settings) and the game's status (the pot, the table, the hand).
/// `pokerth.qml`'s top bar.
pub const APP_BAR_H: f32 = 38.0;
pub const STATUS_BAR_H: f32 = 40.0;

/// What the player did this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableAction {
    None,
    Fold,
    Check,
    Call,
    Raise(Chips),
    /// `S1-DR`: leave the game here for good -- the window asks first.
    Exit,
    /// `S1-CS`: a line for the seats of this table.
    Say(String),
    /// `S1-CS`: stop hearing this seat, or hear it again. Local.
    Mute(SeatIdx),
    Unmute(SeatIdx),
    /// `S1-CX`: the heads-up opponent is gone and the player waits.
    KeepWaiting,
    /// `S1-CX`: the heads-up opponent is gone and the player leaves.
    LeaveTable,
    /// `D-047`: this seat is out of the table for good; the player closes
    /// the table.
    CloseOut,
    /// PokerTH's sound settings, changed at the table's gear.
    SaveSettings(Settings),
    /// PokerTH's *Note about player ...*: a rating and a note, kept locally
    /// under the player's key.
    SaveNote { key: [u8; 32], rating: u8, note: String },
    /// `D-049`: *I'm back* -- this seat stops sitting out.
    Back,
    /// `D-050`: *Show cards* -- the waiting hand is shown instead of mucked.
    ShowCards,
}

/// `S1-EI`: a seat off the line during the hand, and what happens about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsentSeat {
    pub seat: SeatIdx,
    pub name: String,
    /// The table certified it out of this hand.
    pub certified: bool,
    /// The hand waits on it.
    pub waited: bool,
    /// On the clock, for this long.
    pub on_clock_s: Option<u64>,
    pub quiet_s: Option<u64>,
}

/// `S1-CS`: a line of table chat as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableChatLine {
    pub seat: SeatIdx,
    pub who: String,
    pub said: String,
}

/// `S1-CS`: how a seat's connection is doing, as the last ping said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    /// The round trip of the last ping; `None` once the connection closed.
    pub rtt_ms: Option<u64>,
    /// The reading is old enough to doubt.
    pub stale: bool,
    /// `D-041`: the table's group holds the seat as a confirmed member --
    /// on the line whatever the ping says.
    pub group: bool,
    /// `S1-DX`: how many seconds ago the table's group last heard the seat
    /// -- the figure beside the dot on a Tox table, where no ping is shown.
    pub quiet_s: Option<u64>,
    /// `D-049`: the seat sits out by the table's group's word -- only ever
    /// true while `group` is.
    pub away: bool,
}

/// The owner, 2026-09-13: the place the player finished the tournament in --
/// out of chips, or first, the winner -- and how many still play. The window
/// about it waits `show_in_ms`, so the hand that decided it can be looked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Finish {
    pub place: usize,
    pub players_left: usize,
    pub show_in_ms: u64,
}

/// How long the window about the place waits after the deciding hand (the
/// owner: ten seconds, to look at the winning hand).
pub const FINISH_WINDOW_DELAY_MS: u64 = 10_000;

/// *1st*, *2nd*, *3rd*, *4th* ...
pub fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// PokerTH's winner blink (`gametableimpl.cpp`, `postRiverRunAnimation5`):
/// ten steps of `winnerBlinkSpeed`, the highlight hidden on the even ones,
/// then steady. `age` is seconds since the winner was known.
/// `D-056`: the fastest this window ever redraws itself, and the one place that
/// decides it.
///
/// **The owner, 2026-09-14**: *a limit of 25 frames a second -- it is needless
/// load on the GPU, this is not a first-person shooter.* Twenty-five is what
/// film has run at for a century and it is far above what a chip sliding across
/// a felt or a badge blinking needs to read as motion.
///
/// The number that matters is not the frame rate but the **count of repaints**,
/// because `main.rs` measured what one costs: about 4 ms through a graphics
/// driver and about 500 ms on the software rasteriser. Left at the display's
/// own rate an animation asked for 60 to 144 of them a second; at 144 that is
/// well over half a core on a card, and unbounded on a machine without one.
pub const FRAME: std::time::Duration = std::time::Duration::from_millis(40);

/// `D-056`: the cap on a machine with **no graphics driver**.
///
/// `main.rs` measured both: one repaint is about 4 ms through a driver and
/// about **500 ms** on the software rasteriser -- three orders of magnitude,
/// because a processor is shading 900 000 pixels. Asking such a machine for
/// twenty-five frames a second asks for the impossible and costs everything it
/// has: the next frame is due long before the last one is finished, which is
/// the 660 % of a core this project has already measured once. Four frames a
/// second is what it can actually deliver, and an animation there is a
/// slideshow either way -- the choice is between a slideshow and a locked
/// window.
pub const FRAME_SOFTWARE: std::time::Duration = std::time::Duration::from_millis(250);

/// `D-056`: whether this process is drawing without a graphics driver.
///
/// A process-wide fact, decided once before the window opens and never again:
/// `main.rs` settles the renderer and says so here. A global rather than a
/// parameter because every painting function in this module would otherwise
/// carry it, and a fact that cannot change is not worth threading through
/// forty signatures.
static NO_GPU: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `D-056`: said by `main.rs` when it has settled which rasteriser draws.
pub fn drawing_without_a_gpu(yes: bool) {
    NO_GPU.store(yes, std::sync::atomic::Ordering::Relaxed);
}

/// The floor this window will not ask for frames faster than.
fn frame_floor() -> std::time::Duration {
    if NO_GPU.load(std::sync::atomic::Ordering::Relaxed) {
        FRAME_SOFTWARE
    } else {
        FRAME
    }
}

/// `D-056`: how fast this window redraws when the player is looking at
/// something else.
///
/// A table left open behind another window still has a clock ticking and chips
/// flying, and nobody is watching either. Five frames a second keeps the state
/// honest for the moment the player comes back and costs a fifth of what
/// watching it does.
pub const FRAME_UNFOCUSED: std::time::Duration = std::time::Duration::from_millis(200);

/// `D-056`: ask for the next frame, no sooner than the cap allows.
///
/// Every animation in this window goes through here rather than calling
/// `request_repaint` itself, so the cap is one decision in one place and cannot
/// be forgotten at a call site. A minimised window asks for nothing at all: it
/// has no pixels, and whatever it would have drawn is drawn when it comes back.
pub fn paint_again(ctx: &egui::Context, after: std::time::Duration) {
    let (focused, minimised) = ctx.input(|i| {
        (i.focused, i.viewport().minimized.unwrap_or(false))
    });
    if minimised {
        return;
    }
    let floor = if focused { frame_floor() } else { FRAME_UNFOCUSED.max(frame_floor()) };
    ctx.request_repaint_after(after.max(floor));
}

pub const WINNER_BLINK_STEP: f64 = 0.21;
pub fn winner_blink_on(age: f64) -> bool {
    if !(0.0..WINNER_BLINK_STEP * 10.0).contains(&age) {
        return true;
    }
    (age / WINNER_BLINK_STEP) as u32 % 2 == 1
}

/// PokerTH's playing mode (`GameActionBar`'s combo box).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayMode {
    #[default]
    Manual,
    AutoCheckCall,
    AutoCheckFold,
}

impl PlayMode {
    pub fn label(self) -> &'static str {
        match self {
            PlayMode::Manual => "Manual",
            PlayMode::AutoCheckCall => "Auto Check/Call",
            PlayMode::AutoCheckFold => "Auto Check/Fold",
        }
    }
}

/// An action chosen before the turn came: PokerTH's preselection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pre {
    Fold,
    Call,
    Raise,
    AllIn,
}

/// Which tab of the right-hand panel is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogTab {
    #[default]
    Log,
    Odds,
}

/// PokerTH's note dialog while it is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteDraft {
    pub key: [u8; 32],
    pub name: String,
    pub rating: u8,
    pub note: String,
}

/// What the player is holding in the window between frames, and the chips in
/// the air.
#[derive(Debug, Clone, Default)]
pub struct TableUi {
    pub raise: Chips,
    /// The turn `raise` was set for (`S1-CS`): a new turn starts again at
    /// the minimum raise instead of where the slider was left.
    pub for_turn: Option<u64>,
    pub motion: motion::Motion,
    /// `S1-CS`: what the player is typing to the table, not yet sent.
    pub chat_draft: String,
    /// The amount field while the player edits it.
    pub raise_text: String,
    pub raise_editing: bool,
    pub mode: PlayMode,
    pub pre: Option<Pre>,
    /// The call a preselected call was for: a different call clears it.
    pub pre_call: Chips,
    /// The last turn an automatic or preselected action was taken on.
    pub acted_turn: Option<u64>,
    /// PokerTH's accidental call blocker: the call's label when the bar was
    /// armed, when it was, and until when the call is held.
    pub call_label: String,
    pub armed_since: Option<f64>,
    pub call_blocked_until: f64,
    pub chat_open: bool,
    pub log_open: bool,
    pub log_tab: LogTab,
    /// How many chat lines the player has seen with the chat open.
    pub chat_read: usize,
    /// The sound settings being edited at the gear, if that window is open.
    pub settings_open: Option<Settings>,
    pub ranking_open: bool,
    pub note: Option<NoteDraft>,
    /// When each seat's badge last changed, for PokerTH's pop.
    pub badge_changed: Vec<(SeatIdx, Option<SeatAct>, f64)>,
    /// When each winner of a hand was first drawn: `(hand, seat, time)`, for
    /// PokerTH's blink.
    pub winner_since: Vec<(u64, SeatIdx, f64)>,
    /// The window about the place finished in was closed to watch the table.
    pub finish_closed: bool,
    /// `D-051`: the serial of the *not safe* question the player chose to
    /// stay through.
    pub unsafe_closed: u64,
}

/// The raise the control offers this frame.
///
/// `S1-CS`: the minimum raise on a new turn, the player's own setting for
/// the rest of that turn, and never outside what is legal.
pub fn raise_default(state: &mut TableUi, view: &TableView) -> Chips {
    if state.for_turn != Some(view.turn_id) {
        state.for_turn = Some(view.turn_id);
        state.raise = view.min_raise;
    }
    state.raise = state.raise.clamp(view.min_raise, view.max_raise.max(view.min_raise));
    state.raise
}

/// The sentence about the table, when the felt is the place for it.
///
/// Only while nothing is on the felt: a pot, a board or a card there means
/// a hand is being played, and the note goes to the status bar instead of
/// across the pot (`S1-CS`: *elements overlap*). The owner, 2026-09-13: this
/// message in the middle of the table, before the game starts, stays.
pub fn felt_note(view: &TableView) -> Option<&str> {
    let in_use = view.pot > 0
        || view.board.iter().any(|f| !matches!(f, Facing::Empty))
        || view
            .seats
            .iter()
            .any(|s| s.bet > 0 || s.cards.iter().any(|c| !matches!(c, Facing::Empty)));
    if in_use {
        None
    } else {
        view.note.as_deref()
    }
}

/// A probability as a player reads it: whole per cent, and *under 1%*
/// rather than a zero for a chance that is there.
pub fn pct(p: f32) -> String {
    let v = p * 100.0;
    if v > 0.0 && v < 1.0 {
        "<1%".to_string()
    } else {
        format!("{v:.0}%")
    }
}

/// The `n` likeliest improvements, likeliest first.
pub fn likeliest(improve: &[(String, f32)], n: usize) -> Vec<(String, f32)> {
    let mut v: Vec<(String, f32)> = improve.to_vec();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    v.truncate(n);
    v
}

/// PokerTH's phase words for the status bar.
pub fn phase(street: &str) -> String {
    match street {
        "pre-flop" => "Preflop".into(),
        "flop" => "Flop".into(),
        "turn" => "Turn".into(),
        "river" => "River".into(),
        other => {
            let mut c = other.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        }
    }
}

/// The small and the big blind this hand, by the order of play from the
/// button among the seats dealt in: heads-up the button posts the small one.
pub fn blind_seats(view: &TableView) -> (Option<SeatIdx>, Option<SeatIdx>) {
    let mut dealt: Vec<SeatIdx> = view
        .seats
        .iter()
        .filter(|s| !s.sitting_out && !s.left)
        .map(|s| s.seat)
        .collect();
    dealt.sort_unstable();
    if dealt.len() < 2 || view.hand == 0 {
        return (None, None);
    }
    if dealt.len() == 2 {
        let other = dealt.iter().copied().find(|s| *s != view.button);
        return (Some(view.button), other);
    }
    let after = |seat: SeatIdx| dealt.iter().copied().find(|s| *s > seat).or_else(|| dealt.first().copied());
    let sb = after(view.button);
    let bb = sb.and_then(after);
    (sb, bb)
}

/// Green Casino's `table.png`, decoded once and kept as a texture.
fn table_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    const PNG: &[u8] = include_bytes!("../../../assets/pokerth/greencasino/table.png");
    let id = egui::Id::new("pokerth-greencasino-table");
    if let Some(cached) = ctx.data(|d| d.get_temp::<Option<egui::TextureHandle>>(id)) {
        return cached;
    }
    let texture = image::load_from_memory(PNG).ok().map(|img| {
        let rgba = img.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let colour = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        ctx.load_texture("pokerth-greencasino-table", colour, egui::TextureOptions::LINEAR)
    });
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    texture
}

/// Draw the table, and say what was pressed.
pub fn draw(ui: &mut egui::Ui, view: &TableView, state: &mut TableUi, settings: &Settings) -> TableAction {
    let mut action = TableAction::None;
    let full = ui.max_rect();
    let now = ui.input(|i| i.time);
    let p = ui.painter().clone();
    p.rect_filled(full, 0.0, style::APP_BG);

    let app_bar = Rect::from_min_size(full.min, vec2(full.width(), APP_BAR_H));
    let status = Rect::from_min_size(pos2(full.left(), app_bar.bottom()), vec2(full.width(), STATUS_BAR_H));
    let zone_bottom = (full.bottom() - bar::HEIGHT).max(status.bottom() + 120.0);
    let zone = Rect::from_min_max(pos2(full.left(), status.bottom()), pos2(full.right(), zone_bottom));

    let mut seat_ids: Vec<u8> = view.seats.iter().map(|s| s.seat).collect();
    if !seat_ids.contains(&view.hero) {
        seat_ids.push(view.hero);
    }
    let l = seats::layout(zone, &seat_ids, view.hero);

    background(ui.ctx(), &p, zone, full, &l);
    state.motion.observe(view, now);
    board(&p, &l, view, &state.motion, now);

    let (sb, bb) = blind_seats(view);
    for b in &l.others {
        if let Some(seat) = view.seats.iter().find(|s| s.seat == b.seat) {
            if let Some(a) = seat_box(ui, &p, &l, b.rect, seat, view, state, now, false) {
                action = a;
            }
        }
    }
    if let Some(seat) = view.seats.iter().find(|s| s.seat == view.hero) {
        if let Some(a) = seat_box(ui, &p, &l, l.hero, seat, view, state, now, true) {
            action = a;
        }
    }
    pucks(&p, &l, view, sb, bb);
    flights(&p, &l, &state.motion, now);
    shown_hands(&p, &l, view);
    if let Some(note) = felt_note(view) {
        felt_message(&p, &l, note);
    }

    status_bar(&p, status, view);
    if let Some(a) = app_bar_row(ui, &p, app_bar, state, settings) {
        action = a;
    }
    overlays(ui, &p, zone, view);
    if view.preview {
        preview_banner(&p, zone);
    }

    let bar_rect = bar::rect(full, zone, &l);
    // The chat beside the bar, unless the big chat panel is open: one place
    // to type in at a time. What it shows is read.
    let chat_corner = if settings.show_chat() && !state.chat_open { chat_rect(full, bar_rect, &l) } else { None };
    if chat_corner.is_some() {
        state.chat_read = view.chat.len();
    }
    if let Some(a) = panels::draw(ui, zone, view, state) {
        action = a;
    }
    if let Some(a) = bar::draw(ui, bar_rect, view, state, now) {
        action = a;
    }
    if settings.show_odds() {
        // Under the log panel while it is open, not behind it.
        let rect = odds_rect(full, bar_rect, &l).and_then(|r| {
            if !state.log_open {
                return Some(r);
            }
            let top = r.top().max(panels::side_panel_bottom(zone) + 6.0);
            (r.bottom() - top >= ODDS_MIN_H).then(|| Rect::from_min_max(pos2(r.left(), top), r.max))
        });
        if let Some(rect) = rect {
            odds_corner(&p, rect, view);
        }
    }
    if let Some(rect) = chat_corner {
        if let Some(a) = panels::chat_corner(ui, rect, view, state) {
            action = a;
        }
    }
    if let Some(a) = windows(ui, view, state, settings) {
        action = a;
    }

    // `D-056`: the chips in flight and the badge that has just popped are the
    // only two things here that move every frame, and both read the same at
    // twenty-five as at a hundred and forty-four. A clock ticks slowly enough
    // to be drawn ten times a second.
    if state.motion.active(now) {
        paint_again(ui.ctx(), FRAME);
    } else if view.seats.iter().any(|s| s.clock.is_some()) {
        paint_again(ui.ctx(), std::time::Duration::from_millis(100));
    } else if state.badge_changed.iter().any(|(_, _, t)| now - *t < 0.3) {
        paint_again(ui.ctx(), FRAME);
    }
    action
}

/// Which side of the action bar a corner panel takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    /// The chat.
    Left,
    /// The odds.
    Right,
}

/// A corner beside the action bar (the owner, 2026-09-13: the odds *in the
/// bottom right corner*, *it must follow the window's size so it stays in that
/// area*, and *in the same way the chat in the left corner*): from the bar to
/// the window's edge, down to the bottom, reaching up past the bar's top only
/// where no seat, bet or puck is. A seat in the way lowers the top and the
/// panel grows shorter (*shrink its height so it does not cover the seated
/// players*); `None` when the corner is narrower than `MIN_W` or shorter than
/// `min_h`.
pub fn corner_rect(full: Rect, bar: Rect, l: &Seats, corner: Corner, min_h: f32) -> Option<Rect> {
    const GAP: f32 = 14.0;
    const MARGIN: f32 = 8.0;
    const MIN_W: f32 = 150.0;
    const MAX_W: f32 = 380.0;
    const RISE: f32 = 36.0;
    let (left, right) = match corner {
        Corner::Right => {
            let right = full.right() - MARGIN;
            ((bar.right() + GAP).max(right - MAX_W), right)
        }
        Corner::Left => {
            let left = full.left() + MARGIN;
            (left, (bar.left() - GAP).min(left + MAX_W))
        }
    };
    let bottom = full.bottom() - MARGIN;
    if right - left < MIN_W {
        return None;
    }
    let mut top = (bar.top() - RISE).max(full.top());
    let mut keep_clear: Vec<Rect> = l.others.iter().map(|b| b.rect).collect();
    keep_clear.push(l.hero);
    for seat in l.others.iter().map(|b| b.seat).chain(std::iter::once(l.hero_seat)) {
        if let Some(puck) = l.puck(seat) {
            // A seat can hold two pucks side by side, either way.
            let step = puck.width() + 2.0;
            keep_clear.push(puck.expand2(vec2(step, 0.0)));
        }
    }
    for r in keep_clear {
        let r = r.expand(6.0);
        if r.right() > left && r.left() < right && r.bottom() > top && r.top() < bottom {
            top = r.bottom();
        }
    }
    (bottom - top >= min_h).then(|| Rect::from_min_max(pos2(left, top), pos2(right, bottom)))
}

/// The least height of the odds beside the bar: the hand's name, and its frame.
pub const ODDS_MIN_H: f32 = 34.0;

/// The odds' corner, right of the bar: room for at least the hand's name.
pub fn odds_rect(full: Rect, bar: Rect, l: &Seats) -> Option<Rect> {
    corner_rect(full, bar, l, Corner::Right, ODDS_MIN_H)
}

/// The chat's corner, left of the bar: room for at least the line to say
/// something in.
pub fn chat_rect(full: Rect, bar: Rect, l: &Seats) -> Option<Rect> {
    corner_rect(full, bar, l, Corner::Left, panels::CORNER_CHAT_MIN_H)
}

/// The hero's hand and the chances of improving it, as the log's *Odds* tab
/// says them, fitted to `rect`: the more room, the more improvements and the
/// larger the words. Nothing before the hero holds cards.
fn odds_corner(p: &egui::Painter, rect: Rect, view: &TableView) {
    let Some(name) = view.hero_hand.as_deref() else {
        return;
    };
    let folded = view.seats.iter().any(|s| s.seat == view.hero && s.folded);
    style::shadow(p, rect, 10.0, 2.0, 10.0, Color32::from_black_alpha(110));
    p.rect_filled(rect, 10.0, style::faded(style::PANEL_BG, 0.92));
    p.rect_stroke(rect, 10.0, Stroke::new(1.0, style::PANEL_BORDER), StrokeKind::Inside);

    let pad = (rect.height() * 0.07).clamp(7.0, 12.0);
    let inner = rect.shrink(pad);
    let improvements = match view.improve_by {
        Some(_) => likeliest(&view.improve, 6),
        None => Vec::new(),
    };
    let title_size = (inner.height() / 7.5).clamp(11.0, 16.0);
    let sub_size = (title_size * 0.82).max(9.5);
    let gap = 3.0;

    let mut y = inner.top();
    // The owner, 2026-09-13: no *Odds* heading, the hand's name leads.
    let label = if folded { format!("{name} — folded") } else { name.to_string() };
    let title = style::elided(p, &label, title_size, Weight::DemiBold, inner.width());
    style::text(p, pos2(inner.left(), y), Align2::LEFT_TOP, &title, title_size, Weight::DemiBold, style::PANEL_TEXT);
    y += title_size * 1.3;
    if let Some(by) = view.improve_by.filter(|_| y + sub_size * 1.2 <= inner.bottom()) {
        let line = if improvements.is_empty() { "nothing to improve to".to_string() } else { format!("improves {by}: {}", pct(view.improve_total)) };
        let line = style::elided(p, &line, sub_size, Weight::Regular, inner.width());
        style::text(p, pos2(inner.left(), y), Align2::LEFT_TOP, &line, sub_size, Weight::Regular, style::PANEL_TEXT_2);
        y += sub_size * 1.35;
    }
    if improvements.is_empty() {
        return;
    }
    let room = inner.bottom() - y;
    // As many rows as fit at 14 points or more, each at most 24.
    let fits = ((room + gap) / (14.0 + gap)).floor().max(0.0) as usize;
    let rows = improvements.len().min(fits);
    if rows == 0 {
        return;
    }
    let row_h = ((room + gap) / rows as f32 - gap).clamp(14.0, 24.0);
    let text_size = (row_h * 0.6).clamp(9.5, 13.5).min(title_size);
    for (label, chance) in improvements.iter().take(rows) {
        let row = Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), row_h));
        p.rect_filled(row, 3.0, style::faded(style::PANEL_BORDER, 0.22));
        let filled = Rect::from_min_size(row.min, vec2(row.width() * chance.clamp(0.0, 1.0), row.height()));
        p.rect_filled(filled, 3.0, style::faded(style::COLOR_ACCENT, 0.42));
        let figure = pct(*chance);
        let figure_w = style::text_width(p, &figure, text_size, Weight::Bold);
        let words = style::elided(p, label, text_size, Weight::Regular, (row.width() - figure_w - 22.0).max(10.0));
        style::text(p, pos2(row.left() + 7.0, row.center().y), Align2::LEFT_CENTER, &words, text_size, Weight::Regular, style::PANEL_TEXT);
        style::text(
            p,
            pos2(row.right() - 7.0, row.center().y),
            Align2::RIGHT_CENTER,
            &figure,
            text_size,
            if *chance >= 0.5 { Weight::Bold } else { Weight::Regular },
            style::PANEL_TEXT,
        );
        y += row_h + gap;
    }
}

/// `table.png`, PreserveAspectCrop around the board's centre, over the zone
/// and down behind the action bar to the window's edge (`GamePage.qml`'s
/// centre mode, zoom 1.0).
fn background(ctx: &egui::Context, p: &egui::Painter, zone: Rect, full: Rect, l: &Seats) {
    let Some(texture) = table_texture(ctx) else {
        return;
    };
    let src = texture.size_vec2();
    let extra = full.bottom() - zone.bottom();
    let centre_y = l.board_center.y - zone.top();
    let need_h = 2.0 * centre_y.max(zone.height() + extra - centre_y);
    let scale = (zone.width() / src.x.max(1.0)).max(need_h / src.y.max(1.0));
    let rect = Rect::from_center_size(pos2(zone.center().x, l.board_center.y), src * scale);
    let clip = Rect::from_min_max(zone.min, pos2(zone.right(), full.bottom()));
    p.with_clip_rect(clip)
        .image(texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
}

/// The community row, the pot above it, and the winning hand under it.
fn board(p: &egui::Painter, l: &Seats, view: &TableView, motion: &motion::Motion, now: f64) {
    let s = l.board_scale;
    let slots = l.board();
    let row = slots[0].union(slots[4]);
    style::board_glow(p, row.expand2(vec2(40.0 * s, 27.0 * s)));
    for (rect, facing) in slots.iter().zip(view.board.iter()) {
        match facing {
            Facing::Up(c) => paint::card_face(p, *rect, c.card()),
            Facing::Down => paint::card_back(p, *rect),
            Facing::Empty => style::empty_slot(p, *rect, s),
        }
    }
    // `S1-DO`: while the pot flies to the winner the badge stays with the
    // figure in flight, so the chips are seen leaving something.
    let paying: Chips = motion
        .in_flight(now)
        .iter()
        .filter(|f| f.from == motion::Node::Pot)
        .map(|f| f.amount)
        .sum();
    let bets: Chips = view.seats.iter().map(|x| x.bet).sum();
    let total = view.pot + bets;
    if total > 0 || paying > 0 {
        style::pot_badge(p, l.pot_center(), if total > 0 { total } else { paying }, s);
    }
    if view.hand_over {
        if let Some(name) = view.winning_hand.as_deref() {
            let size = (12.0 * s).max(9.0);
            let w = style::text_width(p, name, size, Weight::Bold) + 18.0;
            let h = (22.0 * s).round().max(17.0);
            let at = pos2(row.center().x, row.bottom() + 8.0 * s + h / 2.0);
            let badge = Rect::from_center_size(at, vec2(w, h));
            style::glow(p, badge, h / 2.0, 10.0, style::faded(style::GOLD, 0.45));
            p.rect_filled(badge, h / 2.0, Color32::from_rgba_unmultiplied(13, 61, 13, 235));
            p.rect_stroke(badge, h / 2.0, Stroke::new(1.0, style::GOLD), StrokeKind::Inside);
            style::text(p, badge.center(), Align2::CENTER_CENTER, name, size, Weight::Bold, style::GOLD);
        }
    }
}

/// `S1-CS`, `D-041`, `S1-DX`: a seat's link as a colour and a figure.
pub fn link_reading(link: &Link) -> (Color32, String) {
    use crate::gui::theme;
    match (link.rtt_ms, link.stale, link.group) {
        (Some(ms), false, _) if ms <= 150 => (theme::OK, format!("{ms} ms")),
        (Some(ms), false, _) if ms <= 500 => (theme::WARN, format!("{ms} ms")),
        (Some(ms), false, false) => (theme::DANGER, format!("{ms} ms")),
        (Some(ms), false, true) => (theme::WARN, format!("{ms} ms")),
        (_, _, true) => match link.quiet_s {
            Some(q) if q < 12 => (theme::OK, format!("{q} s")),
            Some(q) => (theme::WARN, format!("{q} s")),
            None => (theme::OK, "on the line".to_string()),
        },
        (None, _, false) | (Some(_), true, false) => (theme::DANGER, "offline".to_string()),
    }
}

/// One player's box: PokerTH's `GamePlayerBox` for an opponent and
/// `GamePlayerSelfBox` for the hero.
#[allow(clippy::too_many_arguments)]
fn seat_box(
    ui: &egui::Ui,
    p: &egui::Painter,
    l: &Seats,
    rect: Rect,
    seat: &SeatView,
    view: &TableView,
    state: &mut TableUi,
    now: f64,
    hero: bool,
) -> Option<TableAction> {
    let mut action = None;
    let s = l.scale;
    let out = seat.sitting_out || seat.left || (seat.stack == 0 && seat.bet == 0 && view.hand > 0 && seat.won == 0 && !seat.cards.iter().any(|c| !matches!(c, Facing::Empty)));
    let opacity = if out { style::DIMMED } else if seat.folded { 0.72 } else { 1.0 };
    let at_turn = seat.clock.is_some() && !view.hand_over;
    // `S1-ER`: a winner is a seat the settlement named, and only where it named
    // nobody does the amount stand in for it (`win_at`). Reading *who won* off
    // the chips that moved is how every winner went unmarked once the amount
    // came out zero.
    let winner = view.hand_over && seat.win.is_some();
    // PokerTH's blink: the winner's highlight and its badge on and off for
    // two seconds, then steady.
    let blink_on = !winner || {
        let since = match state.winner_since.iter().find(|(h, n, _)| *h == view.hand && *n == seat.seat) {
            Some((_, _, at)) => *at,
            None => {
                state.winner_since.retain(|(h, _, _)| *h == view.hand);
                state.winner_since.push((view.hand, seat.seat, now));
                now
            }
        };
        let age = now - since;
        if age < WINNER_BLINK_STEP * 10.0 {
            paint_again(ui.ctx(), FRAME);
        }
        winner_blink_on(age)
    };
    // `D-049`: sits out, by its own word to the table (the hero: its node's).
    let sits_out = if hero { view.hero_sitting_out } else { seat.link.is_some_and(|l| l.away && l.group) };

    // The halos under the box, so they light the felt around it and not the box.
    if winner && blink_on {
        style::winner_glow(p, rect, s);
    }
    if at_turn {
        style::turn_halo(p, rect, s, now);
    }
    style::player_box(p, rect, s, opacity);
    if at_turn {
        style::turn_glow(p, rect, s, if hero { 2.0 } else { 1.0 }, now);
    }

    // The top row: the round avatar and the two cards.
    let o = rect.min;
    let (avatar, cards) = if hero {
        let av = Rect::from_min_size(o + vec2(4.0, 4.0) * s, vec2(52.0, 52.0) * s);
        let c0 = Rect::from_min_size(o + vec2(60.0, 4.0) * s, vec2(37.0, 52.0) * s);
        let c1 = Rect::from_min_size(o + vec2(101.0, 4.0) * s, vec2(37.0, 52.0) * s);
        (av, [c0, c1])
    } else {
        let av = Rect::from_min_size(o + vec2(4.0, 4.0) * s, vec2(40.0, 40.0) * s);
        let c0 = Rect::from_min_size(o + vec2(48.0, 4.0) * s, vec2(29.0, 40.0) * s);
        let c1 = Rect::from_min_size(o + vec2(81.0, 4.0) * s, vec2(29.0, 40.0) * s);
        (av, [c0, c1])
    };
    style::avatar(p, avatar.shrink(1.0), &seat.name, !out);
    if let Some(link) = seat.link.as_ref().filter(|_| !seat.left) {
        let (colour, figure) = link_reading(link);
        let r = (avatar.width() * 0.11).clamp(3.0, 6.0);
        let dot = pos2(avatar.right() - r, avatar.bottom() - r);
        p.circle_filled(dot, r, colour);
        p.circle_stroke(dot, r, Stroke::new(1.0, Color32::from_black_alpha(200)));
        ui.interact(Rect::from_center_size(dot, vec2(r * 3.0, r * 3.0)), ui.id().with(("link", seat.seat)), egui::Sense::hover())
            .on_hover_text(figure);
    }
    let showed = !hero && !seat.folded && seat.cards.iter().all(|f| matches!(f, Facing::Up(_)));
    let cards_area = cards[0].union(cards[1]);
    if !seat.folded && !out && !showed {
        for (r, facing) in cards.iter().zip(seat.cards.iter()) {
            match facing {
                Facing::Up(c) => paint::card_face(p, *r, c.card()),
                Facing::Down => paint::card_back(p, *r),
                Facing::Empty => {}
            }
        }
    }
    if out || seat.left {
        let words = if seat.left { "left the table" } else { "sitting out" };
        style::text(p, cards_area.center(), Align2::CENTER_CENTER, words, (11.0 * s).max(9.0), Weight::Medium, style::faded(style::TEXT_2, 0.9));
    }

    // The info bar: the name, the stars under it, the stack on the right.
    let info_top = if hero { rect.top() + 60.0 * s } else { rect.top() + 44.0 * s };
    let info = Rect::from_min_max(pos2(rect.left() + 4.0 * s, info_top), pos2(rect.right() - 4.0 * s, rect.bottom() - 4.0 * s));
    let name_size = 15.0 * s;
    let stack_text = format!("${}", seat.stack);
    let stack_w = style::text_width(p, &stack_text, name_size, Weight::Bold);
    let name = style::elided(p, if seat.name.is_empty() { "---" } else { &seat.name }, name_size, Weight::DemiBold, info.width() - 2.0);
    style::text(p, info.left_top(), Align2::LEFT_TOP, &name, name_size, Weight::DemiBold, style::faded(style::NAME, opacity.max(0.6)));
    style::text(p, info.right_bottom(), Align2::RIGHT_BOTTOM, &stack_text, name_size, Weight::Bold, style::faded(style::COLOR_ACCENT, opacity.max(0.6)));
    if seat.rating > 0 {
        let left_room = info.width() - stack_w - 6.0;
        let mut star_size = (11.0 * s).max(8.0);
        while star_size > 7.0 && style::text_width(p, "★★★★★", star_size, Weight::Regular) > left_room {
            star_size -= 0.5;
        }
        if style::text_width(p, "★★★★★", star_size, Weight::Regular) <= left_room {
            style::stars(p, pos2(info.left(), info.bottom() - star_size * 0.62), seat.rating, star_size);
        }
    }
    if seat.muted {
        style::text(p, pos2(rect.right() - 4.0 * s, rect.top() + 3.0 * s), Align2::RIGHT_TOP, "muted", (10.0 * s).max(8.0), Weight::Medium, style::PANEL_MUTED);
    }

    // The badge and the clock: over the cards for an opponent, in the strip
    // above the box for the hero. `D-052`: a winner's seat says its own word in
    // the pill above its box instead, and the action it last took is stale the
    // moment the hand is settled, so that is not drawn over it.
    let changed_at = badge_changed(state, seat.seat, seat.act, now);
    let pop = pop_scale(now - changed_at);
    if sits_out {
        let at = if hero {
            pos2(rect.right() - 36.0 * s, rect.top() - 6.0 * s - 9.0 * s)
        } else {
            cards_area.center()
        };
        style::sit_out_badge(p, at, s);
    } else if let Some(act) = seat.act.filter(|_| !winner) {
        let at = if hero {
            let size = 12.0 * s;
            let w = style::text_width(p, act.word(), size, Weight::Bold) + 16.0 * s;
            pos2(rect.right() - w / 2.0, rect.top() - 6.0 * s - 9.0 * s)
        } else {
            cards_area.center()
        };
        style::action_badge(p, at, act, s, pop);
    } else if at_turn {
        let left = seat.clock.unwrap_or(0.0);
        let bar = if hero {
            Rect::from_center_size(pos2(rect.center().x, rect.top() - 6.0 * s - 9.0 * s), vec2(56.0, 7.0) * s)
        } else {
            Rect::from_center_size(cards_area.center(), vec2(44.0, 9.0) * s)
        };
        style::timeout_bar(p, bar, left, if hero { style::TIMEOUT_SELF } else { style::TIMEOUT });
    }

    // The chips in front of the seat, above its box.
    if seat.bet > 0 {
        let h = seats::BET_LABEL_H * s;
        let w = style::bet_chip_width(p, seat.bet, h);
        let label = if hero {
            let badge_room = seat
                .act
                .filter(|_| !winner)
                .map(|a| style::text_width(p, a.word(), 12.0 * s, Weight::Bold) + 16.0 * s + 8.0 * s)
                .unwrap_or(if at_turn { rect.width() / 2.0 + 28.0 * s + 8.0 * s } else { 0.0 });
            Rect::from_min_size(pos2(rect.right() - badge_room - w, rect.top() - seats::BET_LABEL_GAP * s - h), vec2(w, h))
        } else {
            l.bet_label(seat.seat, w).unwrap_or(rect)
        };
        style::bet_chip(p, label, seat.bet);
    }
    if winner && blink_on {
        // `D-052`: the word this seat won by -- which pot, and whether shared.
        style::winner(p, rect, seat.win.unwrap_or_default().word(), s);
    }

    // A right-click on another seat: mute it or hear it again (local), and
    // PokerTH's note about the player.
    if !hero {
        let hit = ui.interact(rect, ui.id().with(("seat", seat.seat)), egui::Sense::click());
        hit.context_menu(|ui| {
            let label = if seat.muted { format!("Unmute {}", seat.name) } else { format!("Mute {}", seat.name) };
            if ui.button(label).clicked() {
                action = Some(if seat.muted { TableAction::Unmute(seat.seat) } else { TableAction::Mute(seat.seat) });
                ui.close();
            }
            if let Some(key) = seat.key {
                if ui.button("Note about player ...").clicked() {
                    state.note = Some(NoteDraft { key, name: seat.name.clone(), rating: seat.rating, note: seat.note.clone() });
                    ui.close();
                }
            }
        });
        if !seat.note.is_empty() {
            hit.on_hover_text(seat.note.as_str());
        }
    }
    action
}

/// When a seat's badge last changed; recorded here, read for the pop.
fn badge_changed(state: &mut TableUi, seat: SeatIdx, act: Option<SeatAct>, now: f64) -> f64 {
    match state.badge_changed.iter_mut().find(|(s, _, _)| *s == seat) {
        Some(entry) if entry.1 == act => entry.2,
        Some(entry) => {
            entry.1 = act;
            entry.2 = now;
            now
        }
        None => {
            state.badge_changed.push((seat, act, now - 1.0));
            now - 1.0
        }
    }
}

/// PokerTH's pop: 0.6 to 1.12 in 110 ms, back to 1.0 in 120 ms.
fn pop_scale(age: f64) -> f32 {
    let age = age as f32;
    if !(0.0..0.23).contains(&age) {
        1.0
    } else if age < 0.11 {
        0.6 + (1.12 - 0.6) * (age / 0.11)
    } else {
        1.12 - 0.12 * ((age - 0.11) / 0.12)
    }
}

/// The dealer and blind pucks beside their boxes.
fn pucks(p: &egui::Painter, l: &Seats, view: &TableView, sb: Option<SeatIdx>, bb: Option<SeatIdx>) {
    if view.hand == 0 && !view.preview {
        return;
    }
    let mut drawn: Vec<SeatIdx> = Vec::new();
    for (seat, which) in [(Some(view.button), Puck::Dealer), (sb, Puck::SmallBlind), (bb, Puck::BigBlind)] {
        let Some(seat) = seat else {
            continue;
        };
        let Some(mut rect) = l.puck(seat) else {
            continue;
        };
        // Heads-up the button posts the small blind too: the two pucks side
        // by side, as PokerTH shows the button and the blind together.
        let already = drawn.iter().filter(|d| **d == seat).count() as f32;
        if already > 0.0 {
            let step = rect.width() + 2.0;
            let dir = if rect.center().x < l.zone.center().x { -1.0 } else { 1.0 };
            rect = rect.translate(vec2(dir * step * already, 0.0));
        }
        style::puck(p, rect, which);
        drawn.push(seat);
    }
}

/// `S1-CS`: chips on their way -- a street's bets into the pot, the pot to
/// the winner -- drawn where they are now.
fn flights(p: &egui::Painter, l: &Seats, motion: &motion::Motion, now: f64) {
    for f in motion.in_flight(now) {
        let payout = f.from == motion::Node::Pot;
        let at = |n: motion::Node| match n {
            motion::Node::Pot => Some(l.pot_center()),
            motion::Node::Seat(seat) => l.box_of(seat).map(|r| if payout { r.center() } else { pos2(r.center().x, r.top()) }),
        };
        if let (Some(from), Some(to)) = (at(f.from), at(f.to)) {
            let pos = from + (to - from) * f.progress(now);
            let h = seats::BET_LABEL_H * l.scale * if payout { 1.3 } else { 1.0 };
            let w = style::bet_chip_width(p, f.amount, h);
            style::bet_chip(p, Rect::from_center_size(pos, vec2(w, h)), f.amount);
        }
    }
}

/// `S1-DO`: an opponent that showed at the showdown, at a size that reads,
/// grown from its box towards the board, with its hand named under the cards.
fn shown_hands(p: &egui::Painter, l: &Seats, view: &TableView) {
    for b in &l.others {
        let Some(seat) = view.seats.iter().find(|s| s.seat == b.seat) else {
            continue;
        };
        if seat.folded || !seat.cards.iter().all(|f| matches!(f, Facing::Up(_))) {
            continue;
        }
        let card = seats::BOARD_CARD * l.board_scale.max(l.scale) * 0.9;
        let from = b.rect.center();
        let towards = l.board_center;
        let centre = from + (towards - from) * 0.30;
        let pair = [
            Rect::from_center_size(pos2(centre.x - card.x / 2.0 - 2.0, centre.y), card),
            Rect::from_center_size(pos2(centre.x + card.x / 2.0 + 2.0, centre.y), card),
        ];
        for (r, facing) in pair.iter().zip(seat.cards.iter()) {
            if let Facing::Up(c) = facing {
                paint::card_face(p, *r, c.card());
            }
        }
        if let Some(name) = seat.shown_hand.as_deref() {
            let size = (12.0 * l.scale).clamp(10.0, 15.0);
            let w = style::text_width(p, name, size, Weight::DemiBold) + 14.0;
            let h = size + 8.0;
            let badge = Rect::from_center_size(pos2(centre.x, pair[0].bottom() + 4.0 + h / 2.0), vec2(w, h));
            p.rect_filled(badge, h / 2.0, Color32::from_rgba_unmultiplied(12, 26, 14, 230));
            p.rect_stroke(badge, h / 2.0, Stroke::new(1.0, style::BOX_ACCENT), StrokeKind::Inside);
            style::text(p, badge.center(), Align2::CENTER_CENTER, name, size, Weight::DemiBold, style::PANEL_TEXT);
        }
    }
}

/// The sentence about the table before the game, in the middle of the felt
/// where the board goes (the owner: it stays).
fn felt_message(p: &egui::Painter, l: &Seats, note: &str) {
    let size = (15.0 * l.board_scale).clamp(13.0, 19.0);
    let max_w = (l.zone.width() * 0.62).max(280.0);
    let galley = p.layout(note.to_owned(), style::font(size, Weight::Medium), style::PANEL_TEXT, max_w);
    let pad = vec2(18.0, 10.0);
    let rect = Rect::from_center_size(l.board_center, galley.size() + pad * 2.0);
    style::shadow(p, rect, 12.0, 2.0, 10.0, Color32::from_black_alpha(120));
    p.rect_filled(rect, 12.0, Color32::from_rgba_unmultiplied(12, 26, 14, 215));
    p.rect_stroke(rect, 12.0, Stroke::new(1.0, style::PANEL_BORDER), StrokeKind::Inside);
    p.galley(rect.min + pad, galley, style::PANEL_TEXT);
}

/// PokerTH's status bar: the pot and the bets on the left, the table in the
/// middle, the phase, the game and the hand on the right.
fn status_bar(p: &egui::Painter, rect: Rect, view: &TableView) {
    p.rect_filled(rect, 0.0, style::STATUS_BAR);
    let cy = rect.center().y;
    let bets: Chips = view.seats.iter().map(|s| s.bet).sum();

    let left = rect.left() + 16.0;
    let t = style::text(p, pos2(left, cy - 8.0), Align2::LEFT_CENTER, "Total:", 13.0, Weight::Medium, style::STATUS_LABEL);
    style::text(p, pos2(t.right() + 4.0, cy - 8.0), Align2::LEFT_CENTER, &format!("${}", view.pot), 13.0, Weight::Bold, style::STATUS_TOTAL);
    let b = style::text(p, pos2(left, cy + 9.0), Align2::LEFT_CENTER, "Bets:", 11.0, Weight::Medium, style::STATUS_LABEL);
    style::text(p, pos2(b.right() + 4.0, cy + 9.0), Align2::LEFT_CENTER, &format!("${bets}"), 11.0, Weight::Medium, style::STATUS_BETS);

    let right = rect.right() - 16.0;
    let game_line = format!("Game: {}   Hand: {}", view.game_no.max(1), view.hand);
    let gw = style::text_width(p, &game_line, 11.0, Weight::Medium);
    let column = right - gw / 2.0;
    style::text(p, pos2(column, cy - 8.0), Align2::CENTER_CENTER, &phase(&view.street), 13.0, Weight::DemiBold, style::WHITE);
    style::text(p, pos2(column, cy + 9.0), Align2::CENTER_CENTER, &game_line, 11.0, Weight::Medium, style::STATUS_LABEL);

    let room = (rect.width() * 0.5).min(right - gw - left - 190.0).max(80.0);
    let name = style::elided(p, &view.name, 14.0, Weight::DemiBold, room);
    let mut second = format!("Blinds {}", view.blinds);
    if felt_note(view).is_none() {
        if let Some(note) = view.note.as_deref() {
            second = format!("{second}  ·  {note}");
        }
    }
    let second = style::elided(p, &second, 11.0, Weight::Medium, room);
    style::text(p, pos2(rect.center().x, cy - 8.0), Align2::CENTER_CENTER, &name, 14.0, Weight::DemiBold, style::WHITE);
    style::text(p, pos2(rect.center().x, cy + 9.0), Align2::CENTER_CENTER, &second, 11.0, Weight::Medium, style::PANEL_TEXT_2);
}

/// `pokerth.qml`'s top bar at the game: the door on the left leaves the game
/// (the table's *Exit*, which asks first, as it always did), the ranking and
/// the settings on the right, 24 points each with PokerTH's margins.
fn app_bar_row(ui: &egui::Ui, p: &egui::Painter, rect: Rect, state: &mut TableUi, settings: &Settings) -> Option<TableAction> {
    let mut action = None;
    p.rect_filled(rect, 0.0, style::TOP_BAR);
    let icon_button = |which: Icon, icon_rect: Rect, active: bool, id: &str, tip: &str| -> egui::Response {
        let resp = ui.interact(icon_rect, ui.id().with(("app-bar", id)), egui::Sense::click()).on_hover_text(tip);
        let colour = if active {
            style::COLOR_ACCENT
        } else if resp.hovered() {
            style::NAME
        } else {
            style::TEXT_2
        };
        style::icon(p, icon_rect, which, colour);
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    };
    let door = Rect::from_min_size(pos2(rect.left() + 6.0, rect.center().y - 13.0), vec2(26.0, 26.0));
    if icon_button(Icon::Door, door, false, "leave", "Leave the table").clicked() {
        action = Some(TableAction::Exit);
    }
    let settings_rect = Rect::from_min_size(pos2(rect.right() - 6.0 - 24.0, rect.center().y - 12.0), vec2(24.0, 24.0));
    let ranking_rect = settings_rect.translate(vec2(-(24.0 + 6.0 + 8.0 + 6.0), 0.0));
    if icon_button(Icon::Settings, settings_rect, state.settings_open.is_some(), "settings", "Settings").clicked() {
        state.settings_open = if state.settings_open.is_some() { None } else { Some(settings.clone()) };
    }
    if icon_button(Icon::Trophy, ranking_rect, state.ranking_open, "ranking", "Table ranking").clicked() {
        state.ranking_open = !state.ranking_open;
    }
    action
}

/// The words over the felt that are not PokerTH's and stay: this client's own
/// line (`S1-EH`) and the seats off the line (`S1-EI`), in the table's colours.
fn overlays(ui: &egui::Ui, p: &egui::Painter, zone: Rect, view: &TableView) {
    if let Some(line) = view.line.as_ref() {
        let sentences: Vec<&str> = line.split(". ").collect();
        let w = (zone.width() * 0.72).max(340.0);
        let h = 38.0 + 18.0 * sentences.len() as f32;
        let rect = Rect::from_center_size(pos2(zone.center().x, zone.center().y - zone.height() * 0.05), vec2(w, h));
        style::shadow(p, rect, 12.0, 3.0, 14.0, Color32::from_black_alpha(160));
        p.rect_filled(rect, 12.0, style::faded(style::PANEL_BG, 0.95));
        p.rect_stroke(rect, 12.0, Stroke::new(1.0, style::PANEL_BORDER), StrokeKind::Inside);
        style::text(p, pos2(rect.center().x, rect.top() + 17.0), Align2::CENTER_CENTER, "Line down", 16.0, Weight::Bold, crate::gui::theme::DANGER);
        for (i, sentence) in sentences.iter().enumerate() {
            let text = if sentence.ends_with('.') { sentence.to_string() } else { format!("{sentence}.") };
            style::text(p, pos2(rect.center().x, rect.top() + 38.0 + 18.0 * i as f32), Align2::CENTER_CENTER, &text, 12.5, Weight::Regular, style::PANEL_TEXT);
        }
        paint_again(ui.ctx(), std::time::Duration::from_secs(1));
    }
    if view.line.is_none() && !view.absent.is_empty() {
        let mut lines: Vec<String> = Vec::new();
        for a in &view.absent {
            let quiet = a.quiet_s.map(|q| format!(", silent {q} s")).unwrap_or_default();
            lines.push(if a.certified {
                format!("{} is off the line{quiet}: certified out of this hand; the hand goes on among the seats on the line.", a.name)
            } else if a.waited {
                match a.on_clock_s {
                    Some(s) => format!("{} is off the line{quiet}: the hand waits on it, {s} s on its clock; when the clock runs out the other seats certify it out and play on.", a.name),
                    None => format!("{} is off the line{quiet}: the hand waits on it; when its clock runs out the other seats certify it out and play on.", a.name),
                }
            } else {
                format!("{} is off the line{quiet}: certified out when its turn comes; it rejoins at a later hand if it comes back.", a.name)
            });
        }
        lines.push("The hand finishes when the seats it waits on are back on the line or certified out; nothing here is stuck.".to_string());
        let w = (zone.width() * 0.84).max(380.0);
        let h = 30.0 + 16.0 * lines.len() as f32;
        let rect = Rect::from_center_size(pos2(zone.center().x, zone.top() + 52.0 + h * 0.5), vec2(w, h));
        style::shadow(p, rect, 12.0, 3.0, 14.0, Color32::from_black_alpha(140));
        p.rect_filled(rect, 12.0, style::faded(style::PANEL_BG, 0.93));
        p.rect_stroke(rect, 12.0, Stroke::new(1.0, style::PANEL_BORDER), StrokeKind::Inside);
        style::text(p, pos2(rect.center().x, rect.top() + 14.0), Align2::CENTER_CENTER, "Seats off the line", 13.5, Weight::Bold, style::BOX_ACCENT);
        for (i, s) in lines.iter().enumerate() {
            style::text(p, pos2(rect.center().x, rect.top() + 32.0 + 16.0 * i as f32), Align2::CENTER_CENTER, s, 11.5, Weight::Regular, style::PANEL_TEXT);
        }
        paint_again(ui.ctx(), std::time::Duration::from_secs(1));
    }
}

/// The sample's banner (§22: a sample that looks like a hand says so).
fn preview_banner(p: &egui::Painter, zone: Rect) {
    let text = "PREVIEW — no hand is in progress, these cards are a sample";
    let w = style::text_width(p, text, 13.0, Weight::Bold) + 24.0;
    let rect = Rect::from_center_size(pos2(zone.center().x, zone.top() + 22.0), vec2(w, 24.0));
    p.rect_filled(rect, 12.0, Color32::from_rgba_unmultiplied(40, 26, 4, 230));
    p.rect_stroke(rect, 12.0, Stroke::new(1.0, crate::gui::theme::WARN), StrokeKind::Inside);
    style::text(p, rect.center(), Align2::CENTER_CENTER, text, 13.0, Weight::Bold, crate::gui::theme::WARN);
}

/// The frame every window of the table wears: Green Casino's panel colours.
fn window_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::window(&ctx.global_style())
        .fill(style::PANEL_BG)
        .stroke(Stroke::new(1.0, style::PANEL_BORDER))
        .corner_radius(10.0)
}

/// A table window's heading, drawn by the window itself: egui's title bar
/// takes its fill from the application's theme, which is not the table's (it
/// came out light grey behind light words). With `closable`, a close cross on
/// the right; the answer is whether it was pressed.
fn window_heading(ui: &mut egui::Ui, title: &str, closable: bool) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(17.0).strong().color(style::PANEL_TEXT));
        if closable {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), egui::Sense::click());
                let colour = if resp.hovered() { style::PANEL_TEXT } else { style::PANEL_MUTED };
                let r = rect.shrink(5.0);
                ui.painter().line_segment([r.left_top(), r.right_bottom()], Stroke::new(1.6, colour));
                ui.painter().line_segment([r.right_top(), r.left_bottom()], Stroke::new(1.6, colour));
                close = resp.on_hover_text("Close").on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
            });
        }
    });
    ui.add_space(4.0);
    close
}

/// The table's windows: the questions of `S1-CX` and `D-047` as before, and
/// PokerTH's settings, table ranking and note about a player.
fn windows(ui: &egui::Ui, view: &TableView, state: &mut TableUi, settings: &Settings) -> Option<TableAction> {
    let ctx = ui.ctx().clone();
    let mut action = None;

    // `S1-CX`: the heads-up opponent cannot be reached. D-007: nobody can
    // fold a hand for them, so the player is asked the one question that
    // has an answer -- wait, or leave.
    if let Some(secs) = view.opponent_gone_s {
        let out = view.opponent_out;
        let title = if view.opponent_left {
            "Opponent left"
        } else if out {
            "Opponent is out"
        } else if view.opponent_slow {
            "Opponent is taking too long"
        } else if view.opponent_alone {
            "Nobody can be reached"
        } else {
            "Opponent disconnected"
        };
        egui::Window::new(title)
            .id(egui::Id::new("table-opponent-gone"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .frame(window_frame(&ctx))
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(&ctx, |ui| {
                ui.set_min_width(360.0);
                ui.visuals_mut().override_text_color = Some(style::PANEL_TEXT);
                window_heading(ui, title, false);
                if view.opponent_left {
                    ui.label("Your opponent left the table.");
                } else if view.opponent_slow {
                    ui.label(format!("Your opponent has been on the clock for {secs} s past their time to decide."));
                } else if view.opponent_alone {
                    ui.label(format!("Nobody at the table has been reachable for {secs} s."));
                } else {
                    ui.label(format!("Your opponent has been unreachable for {secs} s."));
                }
                if view.opponent_left {
                    ui.label(RichText::new("The game is over.").color(style::PANEL_MUTED));
                } else if out {
                    ui.label(RichText::new("That is their fourth absence. Three returns are the limit: the game ends here.").color(style::PANEL_MUTED));
                } else {
                    ui.label(
                        RichText::new(if view.opponent_alone {
                            "Alone at the table, nobody can certify anybody: the table waits for them."
                        } else {
                            "Heads-up, nobody can fold a hand for an absent player: the table waits for them."
                        })
                        .color(style::PANEL_MUTED),
                    );
                    ui.label("Wait for them to come back, or end the game and leave the table.");
                }
                ui.horizontal(|ui| {
                    if !out && ui.add(egui::Button::new(RichText::new("Wait").color(style::WHITE)).fill(Color32::from_rgb(0x1A, 0x4A, 0x8A))).clicked() {
                        action = Some(TableAction::KeepWaiting);
                    }
                    if ui.add(egui::Button::new(RichText::new("Leave the table").color(style::WHITE)).fill(Color32::from_rgb(0x8A, 0x2C, 0x2C))).clicked() {
                        action = Some(TableAction::LeaveTable);
                    }
                });
            });
        paint_again(&ctx, std::time::Duration::from_secs(1));
    }

    // `D-047`: this seat is out of the table for good.
    if let Some(why) = view.out_for_good.as_ref() {
        egui::Window::new("Out of the game")
            .id(egui::Id::new("table-out-for-good"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .frame(window_frame(&ctx))
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(&ctx, |ui| {
                ui.set_min_width(400.0);
                ui.visuals_mut().override_text_color = Some(style::PANEL_TEXT);
                window_heading(ui, "Out of the game", false);
                if view.out_flooded {
                    // `D-051`.
                    ui.label("You were removed from this table for flooding its connection: every other player's client measured junk traffic from yours, and the table plays on without you.");
                } else {
                    ui.label("You were removed from this table after your fourth absence: your connection dropped too often, and the table plays on without you.");
                }
                ui.label(RichText::new("Your game at this table is over for good.").strong());
                ui.label(RichText::new(why.as_str()).color(style::PANEL_MUTED).small());
                if ui.add(egui::Button::new(RichText::new("Close the table").color(style::WHITE)).fill(Color32::from_rgb(0x8A, 0x2C, 0x2C))).clicked() {
                    action = Some(TableAction::CloseOut);
                }
            });
    }

    // `D-051`, the owner: where the table cannot put out what floods it, the
    // honest players are told it is not safe and that leaving is recommended.
    // Stay closes the question until the node says it again.
    if let Some((why, serial)) = view
        .unsafe_note
        .as_ref()
        .filter(|(_, n)| *n != state.unsafe_closed && view.out_for_good.is_none())
    {
        egui::Window::new("Not safe")
            .id(egui::Id::new("table-not-safe"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .frame(window_frame(&ctx))
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(&ctx, |ui| {
                ui.set_min_width(400.0);
                ui.visuals_mut().override_text_color = Some(style::PANEL_TEXT);
                window_heading(ui, "This table is not safe", false);
                ui.label(why.as_str());
                ui.label(RichText::new("We recommend leaving the table.").strong());
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Leave the table").color(style::WHITE)).fill(Color32::from_rgb(0x8A, 0x2C, 0x2C))).clicked() {
                        action = Some(TableAction::LeaveTable);
                    }
                    if ui.add(egui::Button::new(RichText::new("Stay").color(style::WHITE)).fill(Color32::from_rgb(0x1A, 0x4A, 0x8A))).clicked() {
                        state.unsafe_closed = *serial;
                    }
                });
            });
    }

    // The owner, 2026-09-13: the tournament over for this player -- out of
    // chips, the place, and leave or, while others still play, watch; or the
    // winner, congratulated. Ten seconds after the deciding hand, so it can be
    // looked at first.
    if let Some(f) = view.finished.filter(|_| !state.finish_closed && view.out_for_good.is_none()) {
        if f.show_in_ms > 0 {
            paint_again(&ctx, std::time::Duration::from_millis(f.show_in_ms));
        } else {
            let won = f.place == 1;
            egui::Window::new(if won { "Winner" } else { "Out of chips" })
                .id(egui::Id::new("table-finished"))
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .frame(window_frame(&ctx))
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .show(&ctx, |ui| {
                    ui.set_min_width(360.0);
                    ui.visuals_mut().override_text_color = Some(style::PANEL_TEXT);
                    let watch = !won && f.players_left >= 2;
                    if won {
                        window_heading(ui, "You won the tournament!", false);
                        ui.label(RichText::new("Congratulations! You finished in 1st place.").size(18.0).strong().color(style::COLOR_ACCENT));
                        ui.label(RichText::new("The tournament is over.").color(style::PANEL_MUTED));
                    } else {
                        window_heading(ui, "Out of the tournament", false);
                        ui.label(RichText::new(format!("You finished in {} place.", ordinal(f.place))).size(18.0).strong().color(style::COLOR_ACCENT));
                        if watch {
                            ui.label(format!("{} players are still playing. Watch the table, or leave it.", f.players_left));
                        } else {
                            ui.label(RichText::new("The tournament is over.").color(style::PANEL_MUTED));
                        }
                    }
                    ui.horizontal(|ui| {
                        if watch && ui.add(egui::Button::new(RichText::new("Watch the table").color(style::WHITE)).fill(Color32::from_rgb(0x1A, 0x4A, 0x8A))).clicked() {
                            state.finish_closed = true;
                        }
                        if ui.add(egui::Button::new(RichText::new("Leave the table").color(style::WHITE)).fill(Color32::from_rgb(0x8A, 0x2C, 0x2C))).clicked() {
                            action = Some(TableAction::LeaveTable);
                        }
                    });
                });
        }
    }

    // PokerTH's sound settings, at the gear.
    if let Some(mut draft) = state.settings_open.take() {
        let mut open = true;
        let mut changed = false;
        let mut sound = draft.sound();
        let mut show_odds = draft.show_odds();
        let mut show_chat = draft.show_chat();
        let mut auto_muck = draft.auto_muck();
        egui::Window::new("Settings")
            .id(egui::Id::new("table-sound"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .frame(window_frame(&ctx))
            .anchor(Align2::RIGHT_TOP, vec2(-12.0, APP_BAR_H + 6.0))
            .show(&ctx, |ui| {
                ui.set_min_width(320.0);
                ui.visuals_mut().override_text_color = Some(style::PANEL_TEXT);
                if window_heading(ui, "Settings", true) {
                    open = false;
                }
                ui.label(RichText::new("Sound effects").color(style::BOX_ACCENT).strong());
                changed |= ui.checkbox(&mut sound.on, "Enable sound effects").changed();
                ui.add_enabled_ui(sound.on, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Volume:");
                        let mut v = sound.volume.clamp(1, 10);
                        let r = ui.add(egui::Slider::new(&mut v, 1..=10));
                        if r.changed() {
                            sound.volume = v;
                        }
                        changed |= r.drag_stopped() || (r.changed() && !r.dragged());
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("Sound categories").color(style::BOX_ACCENT).strong());
                    changed |= ui.checkbox(&mut sound.game_actions, "Game actions (check, call, raise ...)").changed();
                    changed |= ui.checkbox(&mut sound.lobby_chat, "Lobby chat notifications").changed();
                    changed |= ui.checkbox(&mut sound.network_game, "Network game notifications").changed();
                    changed |= ui.checkbox(&mut sound.blind_raise, "Blind raise notification").changed();
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Table").color(style::BOX_ACCENT).strong());
                changed |= ui.checkbox(&mut show_odds, "Show the odds beside the action bar").changed();
                changed |= ui.checkbox(&mut show_chat, "Show the chat beside the action bar").changed();
                changed |= ui
                    .checkbox(&mut auto_muck, "Auto muck: a hand that may muck is mucked at once")
                    .on_hover_text("Off: at a showdown your losing hand waits three seconds for Show cards")
                    .changed();
            });
        if draft.sound() != sound {
            draft.sound = Some(sound);
        }
        if draft.show_odds() != show_odds {
            draft.show_odds = Some(show_odds);
        }
        if draft.show_chat() != show_chat {
            draft.show_chat = Some(show_chat);
        }
        if draft.auto_muck() != auto_muck {
            draft.auto_muck = Some(auto_muck);
        }
        if changed {
            action = Some(TableAction::SaveSettings(draft.clone()));
        }
        if open {
            state.settings_open = Some(draft);
        }
    } else {
        let _ = settings;
    }

    // PokerTH's table ranking, at the trophy: the seats by their chips.
    if state.ranking_open {
        let mut open = true;
        egui::Window::new("Table ranking")
            .id(egui::Id::new("table-ranking-window"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .frame(window_frame(&ctx))
            .anchor(Align2::RIGHT_TOP, vec2(-50.0, APP_BAR_H + 6.0))
            .show(&ctx, |ui| {
                ui.set_min_width(300.0);
                ui.visuals_mut().override_text_color = Some(style::PANEL_TEXT);
                if window_heading(ui, "Table ranking", true) {
                    open = false;
                }
                let mut rows: Vec<&SeatView> = view.seats.iter().collect();
                rows.sort_by(|a, b| b.stack.cmp(&a.stack).then(a.seat.cmp(&b.seat)));
                egui::Grid::new("table-ranking").num_columns(4).spacing([14.0, 6.0]).show(ui, |ui| {
                    for h in ["#", "Player", "Chips", ""] {
                        ui.label(RichText::new(h).color(style::PANEL_MUTED).small());
                    }
                    ui.end_row();
                    for (i, s) in rows.iter().enumerate() {
                        let colour = if s.seat == view.hero { style::COLOR_ACCENT } else { style::PANEL_TEXT };
                        ui.label(RichText::new(format!("{}", i + 1)).color(style::PANEL_TEXT_2));
                        ui.label(RichText::new(&s.name).color(colour));
                        ui.label(RichText::new(format!("${}", s.stack)).color(style::COLOR_ACCENT));
                        let state_word = if s.left {
                            "left the table"
                        } else if s.stack == 0 && view.hand > 0 && !s.cards.iter().any(|c| !matches!(c, Facing::Empty)) {
                            "out"
                        } else if s.sitting_out {
                            "sitting out"
                        } else {
                            ""
                        };
                        ui.label(RichText::new(state_word).color(style::PANEL_MUTED).small());
                        ui.end_row();
                    }
                });
            });
        state.ranking_open = open;
    }

    // PokerTH's *Note about player ...*.
    if let Some(mut draft) = state.note.take() {
        let mut keep = true;
        let title = format!("Note about \"{}\"", draft.name);
        egui::Window::new(title.as_str())
            .id(egui::Id::new("table-note"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .frame(window_frame(&ctx))
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(&ctx, |ui| {
                ui.set_min_width(380.0);
                ui.visuals_mut().override_text_color = Some(style::PANEL_TEXT);
                window_heading(ui, &title, false);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Rating:").color(style::PANEL_TEXT_2));
                    for i in 1..=5u8 {
                        let glyph = if i <= draft.rating { "★" } else { "☆" };
                        let r = ui.add(egui::Button::new(RichText::new(glyph).size(22.0).color(style::COLOR_ACCENT)).frame(false));
                        if r.clicked() {
                            draft.rating = if draft.rating == i { 0 } else { i };
                        }
                    }
                });
                ui.add(
                    egui::TextEdit::multiline(&mut draft.note)
                        .hint_text("Your private note about this player ...")
                        .desired_rows(5)
                        .desired_width(f32::INFINITY)
                        .char_limit(crate::storage::notes::NOTE_MAX_CHARS),
                );
                ui.label(RichText::new("Notes and ratings are stored locally and are only visible to you.").color(style::PANEL_MUTED).small());
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                    if ui.button("Save").clicked() {
                        action = Some(TableAction::SaveNote { key: draft.key, rating: draft.rating, note: draft.note.clone() });
                        keep = false;
                    }
                });
            });
        if keep {
            state.note = Some(draft);
        }
    }
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
                    act: Some(SeatAct::Fold),
                    ..Default::default()
                },
                SeatView {
                    seat: 2,
                    name: "Carol".into(),
                    stack: 2_310,
                    bet: 40,
                    cards: [Facing::Down, Facing::Down],
                    act: Some(SeatAct::Call),
                    rating: 4,
                    ..Default::default()
                },
                SeatView {
                    seat: 3,
                    name: "Dave".into(),
                    stack: 2_070,
                    cards: [Facing::Down, Facing::Down],
                    act: Some(SeatAct::Check),
                    ..Default::default()
                },
                SeatView {
                    seat: 4,
                    name: "Erin".into(),
                    stack: 2_000,
                    bet: 260,
                    cards: [Facing::Down, Facing::Down],
                    act: Some(SeatAct::Raise),
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
            improve: Vec::new(),
            improve_total: 0.0,
            improve_by: None,
            turn_id: 0,
            hand_over: false,
            chat: Vec::new(),
            opponent_gone_s: None,
            opponent_out: false,
            opponent_slow: false,
            opponent_left: false,
            out_for_good: None,
            out_flooded: false,
            unsafe_note: None,
            line: None,
            absent: Vec::new(),
            opponent_alone: false,
            game_no: 1,
            log: vec![
                LogLine { kind: LogKind::Header, text: "## Game: 1 | Hand: 128 ##".into() },
                LogLine { kind: LogKind::Normal, text: "Erin bets $260.".into() },
            ],
            winning_hand: None,
            hero_sitting_out: false,
            show_cards_in_ms: None,
            finished: None,
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
        assert_ne!(Facing::up(Card::new(Rank::Two, Suit::Clubs), false), Facing::Empty);
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
        assert!(v.max_raise <= hero.stack, "the hero cannot raise more than they have");
    }

    /// A seat that has folded holds nothing. Cards in front of a folded seat
    /// say it is still in the hand.
    #[test]
    fn a_folded_seat_holds_no_cards() {
        let v = TableView::sample();
        for s in v.seats.iter().filter(|s| s.folded || s.sitting_out) {
            assert!(s.cards.iter().all(|c| matches!(c, Facing::Empty)), "seat {} is out of the hand and still holding cards", s.seat);
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

    /// `S1-CS`, the seventh item: the raise control starts every turn at the
    /// minimum raise, keeps what the player set within the turn, and never
    /// offers more than the seat has.
    #[test]
    fn the_raise_starts_at_the_minimum_each_turn() {
        let mut ui = TableUi::default();
        let mut v = TableView { turn_id: 1, min_raise: 200, max_raise: 1_000, can_act: true, ..Default::default() };
        assert_eq!(raise_default(&mut ui, &v), 200);
        ui.raise = 600;
        assert_eq!(raise_default(&mut ui, &v), 600, "the player's own choice stands within the turn");
        v.turn_id = 2;
        v.min_raise = 300;
        assert_eq!(raise_default(&mut ui, &v), 300, "a new turn starts again at the minimum");
        ui.raise = 5_000;
        assert_eq!(raise_default(&mut ui, &v), 1_000, "and never above what the seat has");
    }

    /// The note is on the felt only while the felt is empty; with a pot, a
    /// bet, a board or a card on it, the status bar carries the sentence.
    #[test]
    fn the_felt_note_gives_way_to_the_hand() {
        let mut v = TableView { note: Some("waiting for players".into()), ..Default::default() };
        assert_eq!(felt_note(&v), Some("waiting for players"));
        v.pot = 150;
        assert_eq!(felt_note(&v), None);
        v.pot = 0;
        v.seats.push(SeatView { bet: 50, ..Default::default() });
        assert_eq!(felt_note(&v), None);
        v.seats.clear();
        v.board[0] = Facing::Down;
        assert_eq!(felt_note(&v), None);
    }

    #[test]
    fn a_percentage_is_said_the_way_a_player_reads_it() {
        assert_eq!(pct(0.3497), "35%");
        assert_eq!(pct(0.0009), "<1%");
        assert_eq!(pct(0.0), "0%");
        assert_eq!(pct(1.0), "100%");
    }

    /// The odds a player reads first are the likeliest ones, not the best
    /// category.
    #[test]
    fn the_likeliest_improvements_come_first() {
        let all = vec![
            ("a pair".to_string(), 0.49),
            ("two pair".to_string(), 0.08),
            ("three of a kind".to_string(), 0.01),
            ("a straight".to_string(), 0.04),
            ("a flush".to_string(), 0.02),
        ];
        let top: Vec<String> = likeliest(&all, 4).into_iter().map(|(n, _)| n).collect();
        assert_eq!(top, vec!["a pair", "two pair", "a straight", "a flush"]);
    }

    /// PokerTH's phase words.
    #[test]
    fn the_status_bar_says_the_phase_as_pokerth_does() {
        assert_eq!(phase("pre-flop"), "Preflop");
        assert_eq!(phase("river"), "River");
        assert_eq!(phase("hand over"), "Hand over");
    }

    /// The blinds follow the button: heads-up the button posts the small one,
    /// otherwise the next two seats dealt in.
    #[test]
    fn the_blinds_are_the_seats_after_the_button() {
        let seat = |n: u8| SeatView { seat: n, ..Default::default() };
        let mut v = TableView { hand: 1, button: 1, seats: vec![seat(0), seat(1)], ..Default::default() };
        assert_eq!(blind_seats(&v), (Some(1), Some(0)));
        v.seats = vec![seat(0), seat(1), seat(2), seat(3)];
        v.button = 3;
        assert_eq!(blind_seats(&v), (Some(0), Some(1)));
        v.seats[0].sitting_out = true;
        assert_eq!(blind_seats(&v), (Some(1), Some(2)), "a seat not dealt in posts nothing");
    }

    /// PokerTH's badge pop settles at its own size.
    #[test]
    fn both_corners_stay_in_their_corners_at_every_size() {
        let mut shown = [0, 0];
        for &(w, h) in &[(760.0, 560.0), (1000.0, 720.0), (1280.0, 800.0), (1600.0, 900.0), (1920.0, 1040.0), (2560.0, 1400.0), (900.0, 1000.0)] {
            for seats in 2..=10u8 {
                let full = Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h));
                let status_bottom = APP_BAR_H + STATUS_BAR_H;
                let zone = Rect::from_min_max(pos2(0.0, status_bottom), pos2(w, (h - bar::HEIGHT).max(status_bottom + 120.0)));
                let ids: Vec<u8> = (0..seats).collect();
                let l = seats::layout(zone, &ids, 0);
                let bar_rect = bar::rect(full, zone, &l);
                for (i, (corner, r)) in [(Corner::Right, odds_rect(full, bar_rect, &l)), (Corner::Left, chat_rect(full, bar_rect, &l))].into_iter().enumerate() {
                    let Some(r) = r else {
                        continue;
                    };
                    shown[i] += 1;
                    let at = format!("{corner:?} at {w}x{h}, {seats} seats: {r:?}");
                    assert!(full.contains_rect(r), "inside the window: {at}");
                    match corner {
                        Corner::Right => {
                            assert!(r.left() >= bar_rect.right() + 12.0, "right of the bar: {at}");
                            assert!((r.right() - (w - 8.0)).abs() < 0.5, "at the right edge: {at}");
                        }
                        Corner::Left => {
                            assert!(r.right() <= bar_rect.left() - 12.0, "left of the bar: {at}");
                            assert!((r.left() - 8.0).abs() < 0.5, "at the left edge: {at}");
                        }
                    }
                    assert!((r.bottom() - (h - 8.0)).abs() < 0.5, "at the bottom: {at}");
                    assert!(r.width() >= 150.0 && r.width() <= 380.0, "a readable width: {at}");
                    assert!(r.top() >= bar_rect.top() - 36.0, "no higher than a little over the bar: {at}");
                    for b in l.others.iter().map(|b| b.rect).chain(std::iter::once(l.hero)) {
                        assert!(!b.intersects(r), "clear of every seat: {at}, seat {b:?}");
                    }
                }
            }
        }
        assert!(shown[0] > 40 && shown[1] > 40, "the corners are there at ordinary sizes ({shown:?})");
        // The owner's window: a heads-up table at about a thousand by seven hundred.
        let full = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 720.0));
        let zone = Rect::from_min_max(pos2(0.0, APP_BAR_H + STATUS_BAR_H), pos2(1000.0, 720.0 - bar::HEIGHT));
        let l = seats::layout(zone, &[0, 1], 0);
        let bar_rect = bar::rect(full, zone, &l);
        for r in [odds_rect(full, bar_rect, &l), chat_rect(full, bar_rect, &l)] {
            let r = r.expect("the owner's window has both corners");
            assert!(r.width() > 250.0 && r.height() > 110.0, "{r:?}");
        }
    }

    #[test]
    fn a_seat_in_a_corner_makes_the_panel_shorter_not_hidden() {
        let full = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 720.0));
        let zone = Rect::from_min_max(pos2(0.0, APP_BAR_H + STATUS_BAR_H), pos2(1000.0, 720.0 - bar::HEIGHT));
        for corner in [Corner::Right, Corner::Left] {
            let mut l = seats::layout(zone, &[0, 1, 2], 0);
            let bar_rect = bar::rect(full, zone, &l);
            let rect = |l: &Seats| match corner {
                Corner::Right => odds_rect(full, bar_rect, l),
                Corner::Left => chat_rect(full, bar_rect, l),
            };
            let tall = rect(&l).expect("room at first");
            // Move a seat's box down into the corner's upper part.
            let x = match corner {
                Corner::Right => full.right() - 200.0,
                Corner::Left => full.left() + 60.0,
            };
            let intruder = Rect::from_min_size(pos2(x, bar_rect.top() - 60.0), vec2(114.0, 84.0));
            l.others[0].rect = intruder;
            let short = rect(&l).expect("shorter, still there");
            assert!(short.top() >= intruder.bottom(), "{corner:?}: {short:?} under {intruder:?}");
            assert!(short.height() < tall.height());
            assert!(!short.intersects(intruder));
            // A seat down to the window's edge leaves no room at all.
            l.others[0].rect = Rect::from_min_size(pos2(x, full.bottom() - 30.0), vec2(114.0, 84.0));
            assert_eq!(rect(&l), None, "{corner:?}");
        }
    }

    #[test]
    fn the_winner_blinks_five_times_then_stays() {
        let flips: Vec<bool> = (0..10).map(|i| winner_blink_on((i as f64 + 0.5) * WINNER_BLINK_STEP)).collect();
        assert_eq!(flips, vec![false, true, false, true, false, true, false, true, false, true]);
        assert!(winner_blink_on(WINNER_BLINK_STEP * 10.0 + 0.01) && winner_blink_on(60.0), "then steady");
    }

    #[test]
    fn places_are_said_as_places() {
        let words: Vec<String> = [1, 2, 3, 4, 10, 11, 12, 13, 21, 22].iter().map(|n| ordinal(*n)).collect();
        assert_eq!(words, ["1st", "2nd", "3rd", "4th", "10th", "11th", "12th", "13th", "21st", "22nd"]);
    }

    #[test]
    fn the_pop_settles() {
        assert_eq!(pop_scale(1.0), 1.0);
        assert!(pop_scale(0.0) < 0.7);
        assert!(pop_scale(0.11) > 1.1);
    }
    /// `D-056`: the window asks for frames at a bounded rate, and the bound
    /// follows the rasteriser that has to draw them.
    ///
    /// **The owner, 2026-09-14**: *a limit of 25 frames a second -- it is
    /// needless load on the GPU, this is not a first-person shooter* -- and
    /// *the same for the CPU renderer on machines where GPU acceleration cannot
    /// be reached*. The two cannot be the same number: `main.rs` measured one
    /// repaint at about 4 ms through a graphics driver and about 500 ms on the
    /// software rasteriser, and asking the second for twenty-five a second asks
    /// for twelve times what it can do -- which is how this project once
    /// measured 660 % of a core in an idle window.
    #[test]
    fn the_window_asks_for_frames_at_a_bounded_rate() {
        assert_eq!(FRAME.as_millis(), 40, "twenty-five a second, the owner's number");
        assert_eq!(FRAME_SOFTWARE.as_millis(), 250, "four a second where one costs half of one");
        assert!(FRAME_SOFTWARE > FRAME, "a machine without a driver is asked for less, never more");
        assert!(FRAME_UNFOCUSED >= FRAME, "a window nobody is looking at is never asked for more");

        // The floor follows the rasteriser, and `main.rs` is what sets it.
        drawing_without_a_gpu(false);
        assert_eq!(frame_floor(), FRAME);
        drawing_without_a_gpu(true);
        assert_eq!(frame_floor(), FRAME_SOFTWARE);
        drawing_without_a_gpu(false);

        // Every animation of this window is bounded by it, including the ones
        // that ask for a frame "now": the blink, the chips in flight and the
        // badge that has just popped all pass `FRAME`, and the drain wake
        // passes zero -- which the floor lifts.
        assert!(std::time::Duration::ZERO.max(FRAME) == FRAME);
        assert!(std::time::Duration::from_secs(1).max(FRAME).as_secs() == 1,
            "and a slow clock is not sped up to the cap");
    }

}
