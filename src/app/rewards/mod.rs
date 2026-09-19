//! `D-068`: rewards, quests and the one penalty -- the engine.
//!
//! A pure fold: **a word of the node's, the state, the time -> the new state and
//! what was earned, each with its reason.** No file, no socket, no window, no
//! randomness; [`Host`] at the bottom is the thin part that owns the file.
//!
//! **Three axes, three meanings**, never mixed. *Effort* is experience and the
//! level, which never fall. *Performance* is the season's stars. *Conduct* is
//! the table-manners meter. A poker player can tell luck from merit; when each
//! number means one thing, a reward reads as earned and a penalty as fair.
//!
//! **The window takes the node's word.** Who won, a place, a pot, a leave: all
//! of it is read off [`NodeEvent`]s, per table slot (`D-043`), and nothing is
//! worked out from the window's copies of the game. A hand shown at a showdown
//! is named by `poker::evaluator`, as the table's own log names it.
//!
//! **The penalty is for one thing:** the player's own command to leave
//! (`LeftTable`) a Sit & Go that is set (`TableReal`) and has dealt a hand,
//! before this seat finished. Never for the network, a timeout, a table that
//! put the seat out, a table that was not safe, a hand that stood waiting for
//! a seat that was away, or opponents who were gone. When in doubt: nothing.

pub mod catalog;

use std::collections::{BTreeMap, BTreeSet};

use crate::net::node::NodeEvent;
use crate::poker::evaluator::Category;
use crate::storage::progress::{Earned, GameSummary, Keys, Pending, Progress, Quest, PENDING_MAX};
use crate::storage::progress::{DONE_MAX, OPPONENTS_MAX};

use catalog::*;

/// The time an event is folded at: this machine's clock and its offset from
/// UTC, so that a day and a Monday are the player's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Now {
    pub unix_ms: u64,
    pub offset_min: i32,
}

impl Now {
    pub fn system() -> Now {
        let unix_ms =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        Now { unix_ms, offset_min: local_offset_min() }
    }
}

/// Minutes east of UTC, from the operating system; UTC where it cannot say.
#[cfg(windows)]
pub fn local_offset_min() -> i32 {
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    // SAFETY: the structure is plain data, zero is a valid value of it, and
    // the call only writes into it.
    unsafe {
        let mut tz: TIME_ZONE_INFORMATION = std::mem::zeroed();
        let which = GetTimeZoneInformation(&mut tz);
        let bias = match which {
            1 => tz.Bias + tz.StandardBias,
            2 => tz.Bias + tz.DaylightBias,
            _ => tz.Bias,
        };
        -bias
    }
}

#[cfg(not(windows))]
pub fn local_offset_min() -> i32 {
    0
}

/// Days since 1970-01-01, local.
pub fn day_of(unix_ms: u64, offset_min: i32) -> u64 {
    let s = (unix_ms / 1_000) as i64 + i64::from(offset_min) * 60;
    s.div_euclid(86_400).max(0) as u64
}

/// Weeks, Monday first: 1970-01-01 was a Thursday.
pub const fn week_of(day: u64) -> u64 {
    (day + 3) / 7
}

/// `(year, month 1..=12, day 1..=31)` of a day number (Hinnant's algorithm).
pub fn civil(day: u64) -> (i64, u32, u32) {
    let z = day as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub fn month_of(day: u64) -> u64 {
    let (y, m, _) = civil(day);
    (y.max(0) as u64) * 12 + u64::from(m - 1)
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// `19 Sep 2026`, for the back of a card.
pub fn date_words(unix_ms: u64, offset_min: i32) -> String {
    let (y, m, d) = civil(day_of(unix_ms, offset_min));
    format!("{d} {} {y}", MONTHS[(m as usize - 1).min(11)])
}

/// What the window may show for a change, when no hand is being played.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// A card was earned: its id.
    Card(&'static str),
    Level(u32),
    Quest { text: String, xp: u64 },
    /// A game was settled and `Progress::last_game` says what it came to.
    Summary,
    /// One line for the log of the table in this slot.
    TableLine { slot: u8, text: String },
    /// The calm sentence about a break; once a sitting each.
    Break(&'static str),
}

/// One table this client sits at, as the node has described it so far.
#[derive(Default)]
struct Slot {
    key: Option<[u8; 32]>,
    name: String,
    /// A Sit & Go, by the node's word; a cash game has no end to play to.
    tournament: bool,
    me: Option<u8>,
    roster: usize,
    keys: BTreeMap<u8, [u8; 32]>,
    /// The game's tag, once the session and the seat are both known.
    id: Option<String>,
    session: Option<[u8; 32]>,
    /// Settled already -- finished, left or counted in an earlier run.
    settled: bool,
    dealt_me: bool,
    in_hand: Vec<u8>,
    hole: Option<[u8; 2]>,
    board: Vec<u8>,
    folded_me: bool,
    stands: bool,
    waiting_since: Option<u64>,
    unsafe_ever: bool,
    out_for_good: bool,
    lost: bool,
    sitting_out: bool,
    off_line: BTreeSet<u8>,
    finished: BTreeSet<u8>,
}

/// What is collected while one game is being settled, for its summary.
#[derive(Default)]
struct Collect {
    xp: Vec<(String, u64)>,
    cards: Vec<String>,
    quests: Vec<String>,
}

pub struct Rewards {
    pub progress: Progress,
    /// Something changed since the last save.
    pub changed: bool,
    keys: Keys,
    seed: String,
    slots: BTreeMap<u8, Slot>,
    current: u8,
    line_down: bool,
    collect: Option<Collect>,
    notices: Vec<Notice>,
    sitting_began_ms: Option<u64>,
    lost_run: u32,
    break_said: [bool; 2],
}

const CATEGORY_METRICS: [(Category, &str); 9] = [
    (Category::HighCard, "sd_high_card"),
    (Category::OnePair, "sd_pair"),
    (Category::TwoPair, "sd_two_pair"),
    (Category::ThreeOfAKind, "sd_trips"),
    (Category::Straight, "sd_straight"),
    (Category::Flush, "sd_flush"),
    (Category::FullHouse, "sd_full_house"),
    (Category::FourOfAKind, "sd_quads"),
    (Category::StraightFlush, "sd_straight_flush"),
];

/// The season's rank for a number of stars.
pub const fn rank_of(stars: u32) -> u32 {
    let r = stars / STARS_PER_RANK;
    if r > 9 {
        9
    } else {
        r
    }
}

/// The lowest the stars may fall to this season: the highest floor under the
/// best they have been.
pub fn floor_of(best: u32) -> u32 {
    FLOOR_RANKS.iter().rev().find(|r| rank_of(best) >= **r).map_or(0, |r| r * STARS_PER_RANK)
}

/// Whether a place is in the top half of a table of `players`.
pub const fn top_half(place: u32, players: u32) -> bool {
    let half = players / 2;
    place >= 1 && place <= if half == 0 { 1 } else { half }
}

/// The category of the best hand in these seven cards, and whether it is the
/// royal flush. `None` without a whole board.
fn showdown_hand(hole: [u8; 2], board: &[u8]) -> Option<(Category, bool)> {
    use crate::poker::state::{Card, Rank};
    if board.len() != 5 {
        return None;
    }
    let h = [Card::from_index(hole[0]).ok()?, Card::from_index(hole[1]).ok()?];
    let mut b = [h[0]; 5];
    for (i, c) in board.iter().enumerate() {
        b[i] = Card::from_index(*c).ok()?;
    }
    let category = crate::poker::evaluator::evaluate_holdem(h, &b).category();
    let royal = category == Category::StraightFlush && {
        // A straight flush that holds the ace and the ten of one suit with
        // the three between them.
        let all: Vec<Card> = h.iter().chain(b.iter()).copied().collect();
        all.iter().filter(|c| c.rank() == Rank::Ace).any(|ace| {
            [Rank::King, Rank::Queen, Rank::Jack, Rank::Ten]
                .iter()
                .all(|r| all.iter().any(|c| c.rank() == *r && c.suit() == ace.suit()))
        })
    };
    Some((category, royal))
}

fn is_ace(index: u8) -> bool {
    crate::poker::state::Card::from_index(index).is_ok_and(|c| c.rank() == crate::poker::state::Rank::Ace)
}

impl Rewards {
    pub fn new(progress: Progress, keys: Keys) -> Rewards {
        let seed = keys.tag(b"quest draws");
        Rewards {
            progress,
            changed: false,
            keys,
            seed,
            slots: BTreeMap::new(),
            current: 0,
            line_down: false,
            collect: None,
            notices: Vec::new(),
            sitting_began_ms: None,
            lost_run: 0,
            break_said: [false; 2],
        }
    }

    pub fn level(&self) -> u32 {
        level_of(self.progress.xp)
    }

    /// What the window has not been told yet.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    // ---- time ------------------------------------------------------------

    /// The moment things are dated at: never before the latest this file has
    /// seen. A clock turned back stands still here; it earns nothing new.
    fn stamp(&mut self, now: Now) -> u64 {
        if now.unix_ms > self.progress.clock_high_unix_ms {
            self.progress.clock_high_unix_ms = now.unix_ms;
        }
        self.progress.clock_high_unix_ms
    }

    /// The day's, the week's and the month's turn, and games unheard of for
    /// half a day. A jump forward of any length turns each once.
    pub fn tick(&mut self, now: Now) {
        let t = self.stamp(now);
        let day = day_of(t, now.offset_min);
        if day > self.progress.quests.day {
            self.progress.quests.day = day;
            self.progress.quests.daily = (0..3).map(|slot| self.draw(false, day, slot, 0, &[])).collect();
            self.changed = true;
        }
        let week = week_of(day);
        if week > self.progress.quests.week {
            self.progress.quests.week = week;
            self.progress.quests.weekly = None;
            self.progress.quests.weekly_offer = (0..3).map(|slot| self.draw(true, week, slot, 0, &[])).collect();
            self.changed = true;
        }
        let month = month_of(day);
        let s = &mut self.progress.season;
        if s.month == 0 {
            s.month = month;
            self.changed = true;
        } else if month > s.month {
            // A fresh start, and a soft one: three ranks under where it ended.
            let ended = rank_of(s.stars);
            s.last_rank = Some(ended);
            s.stars = ended.saturating_sub(SEASON_DROP_RANKS) * STARS_PER_RANK;
            s.best = s.stars;
            s.top_run = 0;
            s.month = month;
            let stars = i64::from(s.stars);
            self.progress.log(t, "stars", stars, format!("A new season begins. Last season ended at {}.", RANKS[ended as usize]));
            self.changed = true;
        }
        let stale: Vec<String> = self
            .progress
            .pending
            .iter()
            .filter(|p| t.saturating_sub(p.last_unix_ms) >= PENDING_EXPIRES_MS)
            .filter(|p| !self.slots.values().any(|s| s.id.as_deref() == Some(p.id.as_str()) && !s.settled))
            .map(|p| p.id.clone())
            .collect();
        for id in stale {
            self.settle_unfinished(t, &id, None, "the game was not finished here");
        }
    }

    // ---- quests ----------------------------------------------------------

    fn pick(&self, what: &str) -> u64 {
        let h = blake3::hash(format!("{}|{what}", self.seed).as_bytes());
        u64::from_le_bytes(h.as_bytes()[..8].try_into().unwrap_or([0; 8]))
    }

    /// Draw one quest: a period's slot is a family, so a day always holds one
    /// quest that effort alone completes and one result at most. The draw is
    /// a function of the profile, the period and the slot -- varied, and no
    /// lottery: it rewards nothing by itself.
    fn draw(&self, weekly: bool, period: u64, slot: usize, salt: u64, not: &[&str]) -> Quest {
        let family = [Family::Effort, Family::Table, Family::Result][slot % 3];
        let pool: Vec<&QuestTemplate> =
            QUESTS.iter().filter(|q| q.weekly == weekly && q.family == family && !not.contains(&q.id)).collect();
        let t = pool[(self.pick(&format!("{weekly}|{period}|{slot}|{salt}")) % pool.len() as u64) as usize];
        Quest { id: t.id.to_string(), have: 0, need: t.need, xp: t.xp, done: false, seen: Vec::new() }
    }

    /// Autonomy: one daily quest a day may be exchanged for another of its kind.
    pub fn swap_daily(&mut self, index: usize, now: Now) -> bool {
        self.tick(now);
        let q = &self.progress.quests;
        if q.swapped_day == q.day || q.daily.get(index).is_none_or(|d| d.done) {
            return false;
        }
        let was = q.daily[index].id.clone();
        let salt = q.draws + 1;
        let fresh = self.draw(false, q.day, index, salt, &[was.as_str()]);
        let q = &mut self.progress.quests;
        q.daily[index] = fresh;
        q.swapped_day = q.day;
        q.draws = salt;
        self.changed = true;
        true
    }

    /// Autonomy: the week's challenge is the one the player picks of three.
    pub fn pick_weekly(&mut self, index: usize, now: Now) -> bool {
        self.tick(now);
        let q = &mut self.progress.quests;
        if q.weekly.is_some() || index >= q.weekly_offer.len() {
            return false;
        }
        q.weekly = Some(q.weekly_offer.swap_remove(index));
        q.weekly_offer.clear();
        self.changed = true;
        true
    }

    /// The card on the lobby's card about the player: any card earned, or none.
    pub fn set_showcase(&mut self, id: &str) {
        if id.is_empty() || self.progress.cards.contains_key(id) {
            self.progress.showcase = id.to_string();
            self.changed = true;
        }
    }

    pub fn mark_summary_seen(&mut self) {
        if let Some(g) = self.progress.last_game.as_mut().filter(|g| !g.seen) {
            g.seen = true;
            self.changed = true;
        }
    }

    // ---- the three numbers -----------------------------------------------

    fn grant_xp(&mut self, t: u64, amount: u64, why: &str) {
        if amount == 0 {
            return;
        }
        let before = self.level();
        self.progress.xp = self.progress.xp.saturating_add(amount);
        self.progress.log(t, "xp", amount as i64, why);
        if let Some(c) = self.collect.as_mut() {
            c.xp.push((why.to_string(), amount));
        }
        let after = self.level();
        if after > before {
            self.progress.log(t, "level", i64::from(after - before), format!("Level {after}: a {} chip", chip_of(after)));
            self.notices.push(Notice::Level(after));
        }
        self.changed = true;
    }

    fn quests_mut(&mut self) -> impl Iterator<Item = &mut Quest> {
        let q = &mut self.progress.quests;
        q.daily.iter_mut().chain(q.weekly.iter_mut())
    }

    fn quest_done(&mut self, t: u64, done: Vec<(String, u64)>) {
        for (text, xp) in done {
            self.progress.log(t, "quest", 0, format!("Quest done: {text}"));
            if let Some(c) = self.collect.as_mut() {
                c.quests.push(text.clone());
            }
            self.grant_xp(t, xp, &format!("Quest: {text}"));
            self.notices.push(Notice::Quest { text, xp });
        }
    }

    /// Count `n` more of a metric: the lifetime counter, and every quest that
    /// listens to it.
    fn bump(&mut self, t: u64, metric: &str, n: u64) {
        *self.progress.stats.entry(metric.to_string()).or_insert(0) += n;
        let mut done = Vec::new();
        for q in self.quests_mut().filter(|q| !q.done) {
            let Some(tpl) = quest_by_id(&q.id).filter(|tpl| tpl.metric == metric && !tpl.distinct) else {
                continue;
            };
            q.have = (q.have + n).min(q.need);
            if q.have >= q.need {
                q.done = true;
                done.push((tpl.text.to_string(), q.xp));
            }
        }
        self.changed = true;
        self.quest_done(t, done);
    }

    /// Count a *different* thing of a metric, for the quests that count those.
    fn bump_distinct(&mut self, t: u64, metric: &str, what: &str) {
        let mut done = Vec::new();
        for q in self.quests_mut().filter(|q| !q.done) {
            let Some(tpl) = quest_by_id(&q.id).filter(|tpl| tpl.metric == metric && tpl.distinct) else {
                continue;
            };
            if q.seen.iter().any(|s| s == what) {
                continue;
            }
            q.seen.push(what.to_string());
            q.have = (q.seen.len() as u64).min(q.need);
            if q.have >= q.need {
                q.done = true;
                done.push((tpl.text.to_string(), q.xp));
            }
        }
        self.changed = true;
        self.quest_done(t, done);
    }

    fn raise(&mut self, metric: &str, value: u64) {
        let e = self.progress.stats.entry(metric.to_string()).or_insert(0);
        if value > *e {
            *e = value;
            self.changed = true;
        }
    }

    fn set(&mut self, metric: &str, value: u64) {
        self.progress.stats.insert(metric.to_string(), value);
        self.changed = true;
    }

    /// Every card whose counter has reached its need and is not in the album
    /// yet, with the moment it will remember.
    fn award_cards(&mut self, t: u64, table: &str, place: u32, note: &str) {
        for card in CARDS {
            if self.progress.cards.contains_key(card.id) || self.progress.stat(card.metric) < card.need {
                continue;
            }
            self.progress.cards.insert(
                card.id.to_string(),
                Earned { when_unix_ms: t, table: table.to_string(), place, note: note.to_string() },
            );
            self.progress.log(t, "card", 0, format!("New card: {} {}", card.label(), card.title));
            if let Some(c) = self.collect.as_mut() {
                c.cards.push(card.id.to_string());
            }
            self.notices.push(Notice::Card(card.id));
            self.grant_xp(t, card.rarity().xp(), &format!("New card: {} {}", card.label(), card.title));
            if card.suit != Suit::Joker
                && suit_cards(card.suit).all(|c| self.progress.cards.contains_key(c.id))
                && self.progress.stat(&format!("suit_{}", card.suit.symbol())) == 0
            {
                self.set(&format!("suit_{}", card.suit.symbol()), 1);
                self.grant_xp(t, SUIT_XP, &format!("Suit complete: {} {}", card.suit.symbol(), card.suit.set_name()));
            }
        }
    }

    fn manners(&mut self, t: u64, delta: i64, why: &str) -> i64 {
        let before = i64::from(self.progress.manners);
        let after = (before + delta).clamp(0, 100);
        if after != before {
            self.progress.manners = after as u32;
            self.progress.log(t, "manners", after - before, why);
            self.changed = true;
        }
        after - before
    }

    /// The season's stars for a result. Returns what changed.
    fn stars(&mut self, t: u64, place: u32, players: u32, left: bool) -> i64 {
        let s = &mut self.progress.season;
        let before = s.stars;
        let won = place == 1 && !left;
        let top = !left && top_half(place, players);
        let mut why = String::new();
        if won {
            s.stars += if players >= BIG_TABLE { 3 } else { 2 };
            why = format!("Won a table of {players}");
        } else if top {
            s.stars += 1;
            why = "Top half".to_string();
        } else if rank_of(s.stars) >= PROTECTED_BELOW_RANK && s.stars > floor_of(s.best) {
            s.stars -= 1;
            why = if left { "Left a game in progress: counted as last place".to_string() } else { "Bottom half".to_string() };
        }
        if top {
            s.top_run += 1;
            if s.top_run >= 3 {
                s.top_run = 0;
                s.stars += 1;
                why.push_str(", and the third top-half finish in a row");
            }
        } else {
            s.top_run = 0;
        }
        s.stars = s.stars.min(STARS_MAX);
        s.best = s.best.max(s.stars);
        s.best_rank_ever = s.best_rank_ever.max(rank_of(s.stars));
        let delta = i64::from(s.stars) - i64::from(before);
        let best_rank = u64::from(s.best_rank_ever);
        if delta != 0 {
            self.progress.log(t, "stars", delta, why);
            self.changed = true;
        }
        self.raise("season_best", best_rank);
        delta
    }

    // ---- a game's end ----------------------------------------------------

    fn take_pending(&mut self, id: &str) -> Option<Pending> {
        let i = self.progress.pending.iter().position(|p| p.id == id)?;
        Some(self.progress.pending.remove(i))
    }

    fn mark_done(&mut self, id: &str) {
        self.progress.done.push(id.to_string());
        let over = self.progress.done.len().saturating_sub(DONE_MAX);
        self.progress.done.drain(..over);
        self.changed = true;
    }

    fn close_summary(&mut self, t: u64, table: &str, place: u32, players: u32, before: u32, manners: i64, stars: i64, headline: String) {
        let c = self.collect.take().unwrap_or_default();
        self.progress.last_game = Some(GameSummary {
            when_unix_ms: t,
            table: table.to_string(),
            place,
            players,
            xp: c.xp,
            cards: c.cards,
            quests: c.quests,
            level_before: before,
            level_after: self.level(),
            manners,
            stars,
            headline,
            seen: false,
        });
        self.notices.push(Notice::Summary);
        self.changed = true;
    }

    /// This seat finished a Sit & Go: the node said the place.
    #[allow(clippy::too_many_arguments)]
    fn settle_finished(&mut self, now: Now, slot_no: u8, id: &str, place: u32, away: bool, fallback_players: u32, table: &str) {
        let t = self.stamp(now);
        let p = self.take_pending(id).unwrap_or_default();
        let players = if p.players >= 2 { p.players } else { fallback_players.max(place).max(2) };
        let level_before = self.level();
        self.collect = Some(Collect::default());

        let day = day_of(t, now.offset_min);
        let last_day = self.progress.stat("st_last_day");
        let first_today = last_day != day;
        if self.progress.stat("st_games_day") != day {
            self.set("st_games_day", day);
            self.set("st_games_today", 0);
        }
        let today = self.progress.stat("st_games_today");
        self.set("st_games_today", today + 1);

        // Effort first: the game played to its end, and its hands.
        let mut base = XP_GAME + XP_PER_PLAYER * u64::from(players.saturating_sub(2));
        let mut base_why = "Game finished".to_string();
        if away {
            // Sitting out at the end with a sound line: the game counts, the
            // bonus for playing it to the end does not.
            base = 0;
        } else if today >= SOFT_CAP_GAMES {
            base /= 2;
            base_why = format!("Game finished (past {SOFT_CAP_GAMES} games today: half)");
        }
        let won = place == 1;
        let top = top_half(place, players);
        let placed = if won {
            XP_WIN_PER_PLAYER * u64::from(players)
        } else if top {
            XP_TOP_HALF_PER_PLAYER * u64::from(players)
        } else {
            0
        };
        let subtotal = base + p.xp_hands + placed;
        self.grant_xp(t, base, &base_why);
        self.grant_xp(t, p.xp_hands, "Hands played");
        self.grant_xp(t, placed, if won { "Won the game" } else { "Top half" });
        if first_today {
            self.grant_xp(t, XP_FIRST_OF_DAY, "First game today");
        }
        // Conduct pays a little on top, so the meter's fall is felt.
        let meter = self.progress.manners;
        if let Some((_, pct)) = MANNERS_BONUS.iter().find(|(at, _)| meter >= *at) {
            self.grant_xp(t, subtotal * pct / 100, &format!("Table manners {meter}: +{pct} %"));
        }
        // Rest pays, grind does not: whole days away fill a pool a game draws on.
        if first_today && last_day > 0 {
            let away_days = day.saturating_sub(last_day + 1);
            let pool = (self.progress.stat("st_rested") + away_days * RESTED_PER_DAY).min(RESTED_MAX);
            self.set("st_rested", pool);
        }
        let rested = self.progress.stat("st_rested").min(subtotal / 2);
        if rested > 0 {
            let pool = self.progress.stat("st_rested");
            self.set("st_rested", pool - rested);
            self.grant_xp(t, rested, "Rested bonus");
        }

        // The counters the cards and the quests read.
        self.bump(t, "games", 1);
        for (at, metric, win_metric) in [(4, "games_4", "wins_4"), (6, "games_6", "wins_6"), (9, "games_9", "wins_9")] {
            if players >= at {
                self.bump(t, metric, 1);
                if won {
                    self.bump(t, win_metric, 1);
                }
            }
        }
        self.bump_distinct(t, "size", &players.to_string());
        if won {
            self.bump(t, "wins", 1);
        }
        if top {
            self.bump(t, "top_half", 1);
        }
        if place == 2 {
            self.bump(t, "runner_up", 1);
        }
        if place <= 2 {
            self.bump(t, "final_two", 1);
        }
        if p.timeouts == 0 && !away {
            self.bump(t, "clean_games", 1);
        }
        if won && p.start_stack > 0 && p.low_stack > 0 && p.low_stack * 10 < p.start_stack {
            self.bump(t, "comebacks", 1);
        }
        self.raise("long_game", p.hands);
        if !away {
            let run = self.progress.stat("finish_streak") + 1;
            self.set("finish_streak", run);
            self.raise("finish_streak_best", run);
        }
        let wins_run = if won { self.progress.stat("win_streak") + 1 } else { 0 };
        self.set("win_streak", wins_run);
        self.raise("win_streak_best", wins_run);
        if first_today {
            self.bump(t, "days", 1);
            // Days in a row, with one day a week forgiven without asking.
            let week = week_of(day);
            let run = self.progress.stat("day_streak");
            let run = if last_day > 0 && day == last_day + 1 {
                run + 1
            } else if last_day > 0 && day == last_day + 2 && self.progress.stat("st_excused_week") != week {
                self.set("st_excused_week", week);
                run + 1
            } else {
                1
            };
            self.set("day_streak", run);
            self.raise("day_streak_best", run);
            self.set("st_last_day", day);
        }
        let manners = self.manners(t, i64::from(MANNERS_BACK), "A game played to its end");
        if self.progress.manners == 100 {
            self.bump(t, "good_standing", 1);
        }
        let stars = self.stars(t, place, players, false);

        let note = format!("{} place of {players}", crate::gui::table::ordinal(place as usize));
        self.award_cards(t, table, place, &note);
        self.mark_done(id);

        let earned: u64 = self.collect.as_ref().map_or(0, |c| c.xp.iter().map(|(_, x)| x).sum());
        let headline = if away {
            format!("Game finished while sitting out \u{00b7} +{earned} XP")
        } else {
            format!("Game finished \u{00b7} +{earned} XP")
        };
        self.notices.push(Notice::TableLine { slot: slot_no, text: headline.clone() });
        self.close_summary(t, table, place, players, level_before, manners, stars, headline);

        // The calm sentence, once a sitting each: after a run of rough games,
        // and after two hours at the tables.
        self.lost_run = if top { 0 } else { self.lost_run + 1 };
        if self.lost_run >= BREAK_AFTER_LOSSES && !self.break_said[0] {
            self.break_said[0] = true;
            self.notices.push(Notice::Break("Three rough games in a row. A short break often does more than the next game."));
        }
        if self.sitting_began_ms.is_some_and(|b| t.saturating_sub(b) >= BREAK_AFTER_MS) && !self.break_said[1] {
            self.break_said[1] = true;
            self.notices.push(Notice::Break("Two hours at the tables. A short break keeps the game sharp."));
        }
    }

    /// The player's own command left a running game, with no excuse: the one
    /// penalty. It says why and how the way back goes, and the way is short.
    fn settle_left(&mut self, now: Now, slot_no: u8, id: &str, table: &str) {
        let t = self.stamp(now);
        let p = self.take_pending(id).unwrap_or_default();
        let level_before = self.level();
        self.collect = Some(Collect::default());
        if p.xp_hands > 0 {
            self.progress.log(t, "xp", 0, format!("Left a game in progress: {} XP for its hands not granted", p.xp_hands));
        }
        let manners = self.manners(t, -i64::from(MANNERS_LEAVE), "You left a game in progress");
        self.set("finish_streak", 0);
        self.set("win_streak", 0);
        *self.progress.stats.entry("leaves".to_string()).or_insert(0) += 1;
        for q in self.quests_mut().filter(|q| !q.done) {
            if quest_by_id(&q.id).is_some_and(|tpl| tpl.unbroken) {
                q.have = 0;
            }
        }
        let stars = self.stars(t, u32::MAX, p.players.max(2), true);
        self.mark_done(id);
        let back = (100 - self.progress.manners).div_ceil(MANNERS_BACK).min(MANNERS_LEAVE.div_ceil(MANNERS_BACK));
        let headline = format!(
            "Table manners \u{2212}{}: you left a game in progress. Finish {back} games to restore it.",
            -manners
        );
        self.notices.push(Notice::TableLine { slot: slot_no, text: headline.clone() });
        self.close_summary(t, table, 0, p.players, level_before, manners, stars, headline);
    }

    /// A game that ended for this seat without its doing: its hands are paid,
    /// nothing is taken, no streak is touched.
    fn settle_unfinished(&mut self, t: u64, id: &str, slot_no: Option<u8>, why: &str) {
        let Some(p) = self.take_pending(id) else {
            return;
        };
        let level_before = self.level();
        self.collect = Some(Collect::default());
        self.grant_xp(t, p.xp_hands, &format!("Hands played ({why})"));
        self.raise("long_game", 0);
        self.award_cards(t, &p.table, 0, "");
        self.mark_done(id);
        let earned: u64 = self.collect.as_ref().map_or(0, |c| c.xp.iter().map(|(_, x)| x).sum());
        let headline = format!("Game not finished: {why} \u{00b7} +{earned} XP, nothing lost");
        if let Some(slot_no) = slot_no {
            self.notices.push(Notice::TableLine { slot: slot_no, text: headline.clone() });
        }
        self.close_summary(t, &p.table, 0, p.players, level_before, 0, 0, headline);
    }

    /// Why a leave of a running game costs nothing, if anything says so.
    fn excuse(&self, s: &Slot, t: u64) -> Option<&'static str> {
        if s.unsafe_ever {
            return Some("the table was not safe");
        }
        if s.out_for_good {
            return Some("the table had put this seat out");
        }
        if s.lost {
            return Some("the table was lost");
        }
        if self.line_down {
            return Some("your own connection was down");
        }
        if s.stands || s.waiting_since.is_some_and(|w| t.saturating_sub(w) >= 20_000) {
            return Some("the table stood waiting for a seat that was away");
        }
        let others: Vec<u8> =
            s.in_hand.iter().copied().filter(|x| Some(*x) != s.me && !s.finished.contains(x)).collect();
        if !others.is_empty() && others.iter().all(|x| s.off_line.contains(x)) {
            return Some("the other players were away");
        }
        None
    }

    // ---- the node's words --------------------------------------------------

    fn identify(&mut self, t: u64) {
        let Some(s) = self.slots.get_mut(&self.current) else {
            return;
        };
        let (Some(session), Some(me)) = (s.session, s.me) else {
            return;
        };
        if s.id.is_some() || !s.tournament {
            return;
        }
        let mut what = session.to_vec();
        what.push(me);
        let id = self.keys.tag(&what);
        s.settled = self.progress.done.contains(&id);
        s.id = Some(id.clone());
        let name = s.name.clone();
        if !s.settled && !self.progress.pending.iter().any(|p| p.id == id) {
            self.progress.pending.push(Pending { id, table: name, began_unix_ms: t, last_unix_ms: t, ..Pending::default() });
            let over = self.progress.pending.len().saturating_sub(PENDING_MAX);
            self.progress.pending.drain(..over);
            self.changed = true;
        }
    }

    /// Fold one word of the node's.
    pub fn on_event(&mut self, event: &NodeEvent, now: Now) {
        match event {
            NodeEvent::AtTable { slot, key } => {
                self.current = *slot;
                let s = self.slots.entry(*slot).or_insert_with(|| Slot { tournament: true, ..Slot::default() });
                if key.is_some() && s.key.is_some() && s.key != *key {
                    *s = Slot { tournament: true, ..Slot::default() };
                }
                if key.is_some() {
                    s.key = *key;
                }
                return;
            }
            NodeEvent::ToxLine { how } => {
                self.line_down = *how == "offline";
                return;
            }
            NodeEvent::Swept { .. } => {
                self.tick(now);
                return;
            }
            _ => {}
        }
        if !matches!(
            event,
            NodeEvent::TableParams { .. }
                | NodeEvent::Seated { .. }
                | NodeEvent::Roster { .. }
                | NodeEvent::RosterKeys { .. }
                | NodeEvent::TableReal { .. }
                | NodeEvent::HandBegan { .. }
                | NodeEvent::HandWaiting { .. }
                | NodeEvent::StageStands { .. }
                | NodeEvent::HoleCards { .. }
                | NodeEvent::Board { .. }
                | NodeEvent::TableState { .. }
                | NodeEvent::HandEnded { .. }
                | NodeEvent::Finished { .. }
                | NodeEvent::SeatCertified { .. }
                | NodeEvent::SittingOut { .. }
                | NodeEvent::SeatLink { .. }
                | NodeEvent::SeatLeft { .. }
                | NodeEvent::SeatLeftTable { .. }
                | NodeEvent::TableUnsafe { .. }
                | NodeEvent::OutForGood { .. }
                | NodeEvent::TableLost { .. }
                | NodeEvent::LeftTable { .. }
        ) {
            return;
        }
        self.tick(now);
        let t = self.stamp(now);
        let slot_no = self.current;
        self.slots.entry(slot_no).or_insert_with(|| Slot { tournament: true, ..Slot::default() });
        match event {
            NodeEvent::TableParams { name, tournament, .. } => {
                let s = self.slot();
                s.name = name.clone();
                s.tournament = *tournament;
            }
            NodeEvent::Seated { seat, .. } => {
                self.slot().me = Some(*seat);
                self.identify(t);
            }
            NodeEvent::Roster { seats, .. } => self.slot().roster = seats.len(),
            NodeEvent::RosterKeys { keys, .. } => self.slot().keys = keys.iter().copied().collect(),
            NodeEvent::TableReal { session, .. } => {
                let s = self.slot();
                if s.session != Some(*session) {
                    s.session = Some(*session);
                    s.id = None;
                    s.settled = false;
                }
                self.identify(t);
            }
            NodeEvent::HandBegan { dealt_in, .. } => self.hand_began(t, dealt_in),
            NodeEvent::HandWaiting { seats, .. } => {
                let s = self.slot();
                let others = seats.iter().any(|x| Some(*x) != s.me);
                s.waiting_since = if others { s.waiting_since.or(Some(t)) } else { None };
            }
            NodeEvent::StageStands { seats, .. } => {
                let s = self.slot();
                s.stands = seats.iter().any(|x| Some(*x) != s.me);
            }
            NodeEvent::HoleCards { cards, .. } => {
                let s = self.slot();
                let fresh = s.hole != Some(*cards);
                s.hole = Some(*cards);
                if fresh && is_ace(cards[0]) && is_ace(cards[1]) {
                    let table = s.name.clone();
                    self.bump(t, "pocket_aces", 1);
                    self.award_cards(t, &table, 0, "Dealt a pair of aces");
                }
            }
            NodeEvent::Board { cards, .. } => self.slot().board = cards.clone(),
            NodeEvent::TableState { folded, .. } => {
                let s = self.slot();
                if let Some(me) = s.me {
                    s.folded_me = folded.get(usize::from(me)).copied().unwrap_or(false);
                }
            }
            NodeEvent::HandEnded { stacks, shown, pots, .. } => self.hand_ended(t, stacks, shown, pots),
            NodeEvent::SeatCertified { seat, .. } => {
                let s = self.slot();
                if Some(*seat) == s.me {
                    if let Some(id) = s.id.clone() {
                        if let Some(p) = self.progress.pending.iter_mut().find(|p| p.id == id) {
                            p.timeouts += 1;
                            self.changed = true;
                        }
                    }
                }
            }
            NodeEvent::SittingOut { on } => self.slot().sitting_out = *on,
            NodeEvent::SeatLink { seat, group, .. } => {
                let s = self.slot();
                if *group {
                    s.off_line.remove(seat);
                } else {
                    s.off_line.insert(*seat);
                }
            }
            NodeEvent::SeatLeft { seat, .. } | NodeEvent::SeatLeftTable { seat } => {
                self.slot().off_line.insert(*seat);
            }
            NodeEvent::TableUnsafe { why } => {
                if why.is_some() {
                    self.slot().unsafe_ever = true;
                }
            }
            NodeEvent::OutForGood { .. } => self.slot().out_for_good = true,
            NodeEvent::TableLost { .. } => self.slot().lost = true,
            NodeEvent::Finished { seat, place, players_left, .. } => {
                let line_down = self.line_down;
                let s = self.slot();
                s.finished.insert(*seat);
                if Some(*seat) == s.me && !s.settled {
                    if let Some(id) = s.id.clone() {
                        s.settled = true;
                        // Sitting out with a sound line is the player away from
                        // a working client; with the line down it is the network.
                        let away = s.sitting_out && !line_down;
                        let fallback = (s.roster as u32).max(u32::from(*place) + u32::from(*players_left) - 1);
                        let table = s.name.clone();
                        self.settle_finished(now, slot_no, &id, u32::from(*place), away, fallback, &table);
                    }
                }
            }
            NodeEvent::LeftTable { .. } => {
                if let Some(s) = self.slots.remove(&slot_no) {
                    if let Some(id) = s.id.clone().filter(|_| !s.settled) {
                        let ran = self.progress.pending.iter().any(|p| p.id == id && p.hands >= 1);
                        match (ran, self.excuse(&s, t)) {
                            (true, None) => self.settle_left(now, slot_no, &id, &s.name),
                            (true, Some(why)) => self.settle_unfinished(t, &id, Some(slot_no), why),
                            // Left before a hand was dealt: nothing was under way.
                            (false, _) => {
                                let _ = self.take_pending(&id);
                                self.changed = true;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn slot(&mut self) -> &mut Slot {
        self.slots.entry(self.current).or_insert_with(|| Slot { tournament: true, ..Slot::default() })
    }

    fn hand_began(&mut self, t: u64, dealt_in: &[u8]) {
        self.sitting_began_ms.get_or_insert(t);
        let s = self.slot();
        s.in_hand = dealt_in.to_vec();
        s.dealt_me = s.me.is_some_and(|me| dealt_in.contains(&me));
        s.hole = None;
        s.board.clear();
        s.folded_me = false;
        s.stands = false;
        s.waiting_since = None;
        if s.settled || !s.dealt_me {
            return;
        }
        let (tournament, id, me) = (s.tournament, s.id.clone(), s.me);
        let met: Vec<[u8; 32]> =
            dealt_in.iter().filter(|x| Some(**x) != me).filter_map(|x| s.keys.get(x).copied()).collect();
        if tournament {
            let Some(id) = id else {
                return;
            };
            if let Some(p) = self.progress.pending.iter_mut().find(|p| p.id == id) {
                p.hands += 1;
                p.xp_hands = (p.hands * XP_PER_HAND).min(XP_HANDS_CAP);
                p.last_unix_ms = t;
                if p.players == 0 {
                    p.players = dealt_in.len() as u32;
                }
            }
        } else {
            // A cash game has no end to hold experience for: a hand pays as it
            // is dealt, up to a day's cap.
            let day = self.progress.quests.day;
            if self.progress.stat("st_cash_day") != day {
                self.set("st_cash_day", day);
                self.set("st_cash_xp", 0);
            }
            let had = self.progress.stat("st_cash_xp");
            if had < XP_CASH_DAY_CAP {
                self.set("st_cash_xp", had + XP_CASH_HAND);
                self.grant_xp(t, XP_CASH_HAND, "Hand played (cash game)");
            }
        }
        self.bump(t, "hands", 1);
        for key in met {
            let tag = self.keys.tag(&key);
            if !self.progress.opponents.contains(&tag) {
                self.progress.opponents.push(tag);
                let over = self.progress.opponents.len().saturating_sub(OPPONENTS_MAX);
                self.progress.opponents.drain(..over);
                self.bump(t, "opponents", 1);
            }
        }
        let table = self.slot().name.clone();
        self.award_cards(t, &table, 0, "");
    }

    fn hand_ended(&mut self, t: u64, stacks: &[u64], shown: &[Option<[u8; 2]>], pots: &[crate::net::node::PotEnd]) {
        let s = self.slot();
        let Some(me) = s.me.filter(|_| s.dealt_me && !s.settled) else {
            return;
        };
        s.dealt_me = false;
        let (hole, board, folded, id, table) = (s.hole, s.board.clone(), s.folded_me, s.id.clone(), s.name.clone());
        let won = pots.iter().any(|p| p.winners.contains(&me));
        let shared = pots.iter().any(|p| p.winners.contains(&me) && p.winners.len() > 1);
        let i_showed = shown.get(usize::from(me)).copied().flatten();
        let others_showed = shown.iter().enumerate().any(|(i, c)| i != usize::from(me) && c.is_some());
        if let Some(p) = id.and_then(|id| self.progress.pending.iter_mut().find(|p| p.id == id)) {
            let mine = stacks.get(usize::from(me)).copied().unwrap_or(0);
            if p.start_stack == 0 {
                // The first hand's end: what every seat began with is the
                // chips in play shared out evenly, whoever won this hand.
                p.start_stack = stacks.iter().sum::<u64>() / (p.players.max(1) as u64);
                p.low_stack = p.start_stack;
            }
            if mine > 0 {
                p.low_stack = p.low_stack.min(mine);
            }
            p.last_unix_ms = t;
            self.changed = true;
        }
        let mut note = String::new();
        if won {
            self.bump(t, "pots", 1);
            if let Some((category, royal)) = i_showed.and_then(|h| showdown_hand(h, &board)) {
                self.bump(t, "showdowns", 1);
                let (_, metric) = CATEGORY_METRICS[category as usize];
                self.bump(t, metric, 1);
                if category >= Category::TwoPair {
                    self.bump(t, "sd_two_pair_plus", 1);
                }
                if royal {
                    self.bump(t, "sd_royal", 1);
                }
                let mask = self.progress.stat("sd_mask") | (1 << (category as u64));
                self.set("sd_mask", mask);
                self.raise("sd_kinds", u64::from(mask.count_ones()));
                self.bump_distinct(t, "sd_kind", metric);
                if shared {
                    self.bump(t, "splits", 1);
                }
                note = format!("Won with {}", crate::poker::strength::category_name(category).to_lowercase());
            }
        } else if let Some((category, _)) = hole.filter(|_| !folded && others_showed).and_then(|h| showdown_hand(h, &board)) {
            // A bad beat is remembered, not rewarded for being sought: nobody
            // plays to lose with a full house.
            if category >= Category::FullHouse {
                self.bump(t, "bb_full_house", 1);
            }
            if category >= Category::FourOfAKind {
                self.bump(t, "bb_quads", 1);
            }
            if hole.is_some_and(|h| is_ace(h[0]) && is_ace(h[1])) {
                self.bump(t, "bb_aces", 1);
            }
            note = format!("Lost with {}", crate::poker::strength::category_name(category).to_lowercase());
        }
        self.award_cards(t, &table, 0, &note);
    }

    /// `D-067`'s record, read in once: a returning player starts with what
    /// their finished games came to, not from nothing.
    pub fn import(&mut self, results: &crate::storage::results::Results, now: Now) {
        if self.progress.imported {
            return;
        }
        self.progress.imported = true;
        self.changed = true;
        if results.entries.is_empty() {
            return;
        }
        let t = self.stamp(now);
        let mut days = BTreeSet::new();
        let (mut xp, mut games, mut wins, mut tops, mut seconds) = (0u64, 0u64, 0u64, 0u64, 0u64);
        for e in &results.entries {
            let seats = u32::from(e.seats.max(2));
            games += 1;
            xp += XP_GAME;
            days.insert(day_of(e.when_unix_ms, now.offset_min));
            if e.won() {
                wins += 1;
                xp += XP_WIN_PER_PLAYER * 2;
            }
            if top_half(u32::from(e.place), seats) {
                tops += 1;
            }
            if e.place == 2 {
                seconds += 1;
            }
        }
        for (metric, n) in [("games", games), ("wins", wins), ("top_half", tops), ("runner_up", seconds), ("days", days.len() as u64)] {
            *self.progress.stats.entry(metric.to_string()).or_insert(0) += n;
        }
        self.grant_xp(t, xp, &format!("Your record so far: {games} finished games"));
        self.award_cards(t, "", 0, "From your record");
    }

    /// `--album-preview`: a sample of progress to draw the album from, with no
    /// node and no profile -- a player a fortnight in, or (`all`) every card.
    pub fn sample(all: bool, now: Now) -> Rewards {
        let keys = Keys::derive(&ed25519_dalek::SigningKey::from_bytes(&[1u8; 32]));
        let mut r = Rewards::new(Progress::default(), keys);
        r.progress.imported = true;
        r.tick(now);
        let t = r.stamp(now);
        let stats: &[(&str, u64)] = &[
            ("games", 12),
            ("hands", 520),
            ("wins", 2),
            ("top_half", 5),
            ("days", 5),
            ("opponents", 9),
            ("showdowns", 14),
            ("sd_pair", 6),
            ("sd_two_pair", 4),
            ("sd_trips", 1),
            ("sd_flush", 1),
            ("sd_kinds", 4),
            ("pocket_aces", 1),
            ("finish_streak_best", 12),
            ("finish_streak", 4),
            ("good_standing", 6),
            ("games_4", 6),
            ("games_6", 3),
            ("wins_4", 1),
            ("bb_aces", 1),
            ("day_streak", 3),
        ];
        for (metric, n) in stats {
            r.progress.stats.insert((*metric).to_string(), *n);
        }
        if all {
            for card in CARDS {
                let have = r.progress.stat(card.metric).max(card.need);
                r.progress.stats.insert(card.metric.to_string(), have);
            }
        }
        r.award_cards(t, "Riverside", 2, "2nd place of 6");
        r.progress.xp = if all { 61_000 } else { 4_100 };
        r.progress.manners = if all { 100 } else { 80 };
        r.progress.season.stars = if all { 19 } else { 7 };
        r.progress.season.last_rank = Some(4);
        r.progress.showcase = "D7".to_string();
        r.progress.journal.clear();
        r.progress.log(t, "xp", 140, "Game finished");
        r.progress.log(t, "card", 0, "New card: \u{2666}7 Flush");
        r.progress.log(t, "stars", 2, "Won a table of 4");
        r.progress.log(t, "manners", -25, "You left a game in progress");
        r.progress.log(t, "manners", 5, "A game played to its end");
        r.take_notices();
        r
    }

    /// The nearest thing to aim at: the unearned card furthest along.
    pub fn next_goal(&self) -> Option<(&'static Card, u64)> {
        let mut best: Option<(&'static Card, u64)> = None;
        for c in CARDS.iter().filter(|c| !c.hidden && !self.progress.cards.contains_key(c.id)) {
            let have = self.progress.stat(c.metric).min(c.need);
            // have/need compared without fractions; on a tie the easier card
            // (the lower rank, then the smaller need), and the first of those.
            let better = best.is_none_or(|(b, hb)| {
                (have * b.need).cmp(&(hb * c.need)).then(b.rank.cmp(&c.rank)).then(b.need.cmp(&c.need)).is_gt()
            });
            if better {
                best = Some((c, have));
            }
        }
        best
    }
}

/// The engine with its file: what both the window and the headless client own.
pub struct Host {
    pub rewards: Rewards,
    store: crate::storage::progress::Store,
    /// What the load had to say, until the lobby has said it.
    pub notice: Option<crate::storage::progress::Notice>,
}

impl Host {
    pub fn open(dir: &std::path::Path, app_key: &ed25519_dalek::SigningKey) -> Host {
        let now = Now::system();
        let loaded = crate::storage::progress::Store::open(dir, app_key, now.unix_ms);
        let mut rewards = Rewards::new(loaded.progress, loaded.store.keys().clone());
        rewards.import(&crate::storage::results::load(dir), now);
        rewards.tick(now);
        let mut host = Host { rewards, store: loaded.store, notice: loaded.notice };
        host.flush();
        host
    }

    pub fn on_event(&mut self, event: &NodeEvent) {
        self.rewards.on_event(event, Now::system());
    }

    /// Write what changed. A write that fails is owed: the state stays in
    /// memory and the next change, or the exit, writes it. Never stops a game.
    pub fn flush(&mut self) -> Option<String> {
        if !self.rewards.changed && !self.store.dirty {
            return None;
        }
        self.rewards.changed = false;
        self.store.save(&self.rewards.progress).err().map(|e| format!("the rewards file did not save (it will be tried again): {e}"))
    }
}

#[cfg(test)]
mod tests;
