//! `D-068`: what the rewards say -- the words and the numbers the lobby's card,
//! the summary after a game and the album are drawn from, worked out here so
//! they can be tested without a window (as `gui::lobby` does for the lobby).
//!
//! **The tone**: a reward says what was earned and why; a penalty says why and
//! how the way back goes, in the same calm voice and never in red; a loss is a
//! place, named neutrally. Nothing counts down, nothing says *come back*, and
//! nothing after a loss says *win it back*. A true *one more game* is said; an
//! invented *almost* never is.

use crate::app::rewards::catalog::{self, Card, Suit, CARDS, RANKS, STARS_PER_RANK};
use crate::app::rewards::{date_words, rank_of, Rewards};
use crate::storage::progress::Quest;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestLine {
    pub text: String,
    pub have: u64,
    pub need: u64,
    pub xp: u64,
    pub done: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalLine {
    /// `♣7 Ten Games`.
    pub label: String,
    pub how: String,
    pub have: u64,
    pub need: u64,
    /// `3 more games`-style words where the counter allows them; else empty.
    pub left: String,
}

/// The level as a casino chip: the colour's name, the number on it, how far
/// into the level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelView {
    pub level: u32,
    pub chip: &'static str,
    pub into: u64,
    pub span: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeasonView {
    pub rank: &'static str,
    /// Stars lit in the rank, of three.
    pub lit: u32,
    pub last: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SummaryView {
    pub headline: String,
    pub lines: Vec<(String, u64)>,
    pub cards: Vec<&'static Card>,
    pub quests: Vec<String>,
    pub level_up: Option<u32>,
    /// Already in words, empty where nothing moved.
    pub stars: String,
    /// The game was left: the summary is about conduct, in the calm colour.
    pub left: bool,
}

/// What the lobby's card about the player shows of the rewards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YouRewards {
    pub level: LevelView,
    pub manners: u32,
    pub manners_words: &'static str,
    pub season: SeasonView,
    pub daily: Vec<QuestLine>,
    pub weekly: Option<QuestLine>,
    /// The week's challenge has not been picked: the card says so.
    pub weekly_to_pick: bool,
    pub goal: Option<GoalLine>,
    pub showcase: Option<&'static Card>,
    pub cards_have: usize,
    pub summary: Option<SummaryView>,
    pub day_streak: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardView {
    pub card: &'static Card,
    /// `♦9 · 21/52`; a joker's is `★3`.
    pub number: String,
    pub have: u64,
    pub need: u64,
    /// The back of an earned card: when, where, what it remembers.
    pub earned: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalLine {
    pub when: String,
    pub delta: String,
    pub why: String,
}

pub struct AlbumView {
    pub you: YouRewards,
    pub pages: Vec<(Suit, Vec<CardView>)>,
    pub journal: Vec<JournalLine>,
    pub weekly_offer: Vec<QuestLine>,
    /// Today's swap has not been used.
    pub may_swap: bool,
}

fn quest_line(q: &Quest) -> QuestLine {
    let text = catalog::quest_by_id(&q.id).map_or_else(|| q.id.clone(), |t| t.text.to_string());
    QuestLine { text, have: q.have.min(q.need), need: q.need, xp: q.xp, done: q.done }
}

/// What the meter reads as. Never a threat: above all it says how to get back.
pub const fn manners_words(meter: u32) -> &'static str {
    match meter {
        100 => "Full: you play your games to the end",
        90..=99 => "Good standing",
        70..=89 => "Recovering: each finished game adds 5",
        _ => "Low: each finished game adds 5",
    }
}

/// `3 more games`, where the counter is a plain count of something sayable.
fn left_words(metric: &str, left: u64) -> String {
    let (one, many) = match metric {
        "games" | "games_4" | "games_6" | "games_9" => ("game", "games"),
        "hands" => ("hand", "hands"),
        "wins" | "wins_4" | "wins_6" => ("win", "wins"),
        "days" => ("day", "days"),
        "opponents" => ("new player", "new players"),
        "top_half" => ("top-half finish", "top-half finishes"),
        "showdowns" => ("showdown win", "showdown wins"),
        _ => return String::new(),
    };
    format!("{left} more {}", if left == 1 { one } else { many })
}

/// The number on a card: its place in the deck, clubs first -- `♦9 · 21/52`.
pub fn card_number(card: &Card) -> String {
    let suit = match card.suit {
        Suit::Clubs => 0,
        Suit::Diamonds => 1,
        Suit::Hearts => 2,
        Suit::Spades => 3,
        Suit::Joker => return card.label(),
    };
    format!("{} \u{00b7} {}/52", card.label(), suit * 13 + u32::from(card.rank) - 1)
}

pub fn level_view(xp: u64) -> LevelView {
    let level = catalog::level_of(xp);
    let (from, to) = (catalog::xp_for_level(level), catalog::xp_for_level(level + 1));
    LevelView { level, chip: catalog::chip_of(level), into: xp - from, span: to - from }
}

fn stars_words(delta: i64) -> String {
    match delta {
        0 => String::new(),
        1 => "+1 star this season".to_string(),
        d if d > 0 => format!("+{d} stars this season"),
        -1 => "\u{2212}1 star this season".to_string(),
        d => format!("\u{2212}{} stars this season", -d),
    }
}

pub fn you(r: &Rewards) -> YouRewards {
    let p = &r.progress;
    let goal = r.next_goal().map(|(card, have)| GoalLine {
        label: format!("{} {}", card.label(), card.title),
        how: card.how.to_string(),
        have,
        need: card.need,
        left: left_words(card.metric, card.need - have),
    });
    let summary = p.last_game.as_ref().filter(|g| !g.seen).map(|g| SummaryView {
        headline: g.headline.clone(),
        lines: g.xp.clone(),
        cards: g.cards.iter().filter_map(|id| catalog::card_by_id(id)).collect(),
        quests: g.quests.clone(),
        level_up: (g.level_after > g.level_before).then_some(g.level_after),
        stars: stars_words(g.stars),
        left: g.manners < 0,
    });
    YouRewards {
        level: level_view(p.xp),
        manners: p.manners,
        manners_words: manners_words(p.manners),
        season: SeasonView {
            rank: RANKS[rank_of(p.season.stars) as usize],
            lit: p.season.stars % STARS_PER_RANK,
            last: p.season.last_rank.map(|r| RANKS[(r as usize).min(9)]),
        },
        daily: p.quests.daily.iter().map(quest_line).collect(),
        weekly: p.quests.weekly.as_ref().map(quest_line),
        weekly_to_pick: p.quests.weekly.is_none() && !p.quests.weekly_offer.is_empty(),
        goal,
        showcase: catalog::card_by_id(&p.showcase),
        cards_have: CARDS.iter().filter(|c| c.suit != Suit::Joker && p.cards.contains_key(c.id)).count(),
        summary,
        day_streak: p.stat("day_streak"),
    }
}

pub fn album(r: &Rewards, offset_min: i32) -> AlbumView {
    let p = &r.progress;
    let page = |suit: Suit| -> Vec<CardView> {
        catalog::suit_cards(suit)
            .map(|card| CardView {
                card,
                number: card_number(card),
                have: p.stat(card.metric).min(card.need),
                need: card.need,
                earned: p.cards.get(card.id).map(|e| {
                    let mut back = date_words(e.when_unix_ms, offset_min);
                    if !e.table.is_empty() {
                        back.push_str(&format!(" \u{00b7} {}", e.table));
                    }
                    if !e.note.is_empty() {
                        back.push_str(&format!("\n{}", e.note));
                    }
                    back
                }),
            })
            .collect()
    };
    let mut pages: Vec<(Suit, Vec<CardView>)> = Suit::DECK.iter().map(|s| (*s, page(*s))).collect();
    pages.push((Suit::Joker, page(Suit::Joker)));
    let journal = p
        .journal
        .iter()
        .rev()
        .map(|l| JournalLine {
            when: date_words(l.when_unix_ms, offset_min),
            delta: match (l.axis.as_str(), l.delta) {
                (_, 0) => String::new(),
                ("xp", d) => format!("+{d} XP"),
                ("manners", d) if d < 0 => format!("\u{2212}{} manners", -d),
                ("manners", d) => format!("+{d} manners"),
                ("stars", -1) => "\u{2212}1 star".to_string(),
                ("stars", 1) => "+1 star".to_string(),
                ("stars", d) if d < 0 => format!("\u{2212}{} stars", -d),
                ("stars", d) => format!("+{d} stars"),
                ("level", d) => format!("+{d} level"),
                (_, d) => d.to_string(),
            },
            why: l.why.clone(),
        })
        .collect();
    AlbumView {
        you: you(r),
        pages,
        journal,
        weekly_offer: p.quests.weekly_offer.iter().map(quest_line).collect(),
        may_swap: p.quests.swapped_day != p.quests.day,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::progress::{GameSummary, Keys, Progress};

    fn rewards(p: Progress) -> Rewards {
        Rewards::new(p, Keys::derive(&ed25519_dalek::SigningKey::from_bytes(&[3u8; 32])))
    }

    /// The prompt's own example: the nine of diamonds is the twenty-first card.
    #[test]
    fn a_card_is_numbered_by_its_place_in_the_deck() {
        assert_eq!(card_number(catalog::card_by_id("D9").unwrap()), "\u{2666}9 \u{00b7} 21/52");
        assert_eq!(card_number(catalog::card_by_id("C2").unwrap()), "\u{2663}2 \u{00b7} 1/52");
        assert_eq!(card_number(catalog::card_by_id("SA").unwrap()), "\u{2660}A \u{00b7} 52/52");
        assert_eq!(card_number(catalog::card_by_id("J1").unwrap()), "\u{2605}1");
    }

    /// A newcomer sees a level, a full meter, and a goal one game away.
    #[test]
    fn a_new_profile_has_a_goal_in_sight() {
        let y = you(&rewards(Progress::default()));
        assert_eq!((y.level.level, y.level.chip, y.level.into, y.level.span), (1, "White", 0, 100));
        assert_eq!((y.manners, y.manners_words), (100, "Full: you play your games to the end"));
        let goal = y.goal.expect("always a nearest goal");
        assert_eq!(goal.label, "\u{2663}2 First Game");
        assert_eq!(goal.left, "1 more game");
        assert_eq!(y.season.rank, "High Card");
        assert!(y.summary.is_none());
    }

    /// The penalty's words: why, and the way back; no shame in any of them.
    #[test]
    fn a_penalty_reads_calmly_and_says_the_way_back() {
        for meter in [0u32, 40, 75, 95, 100] {
            let w = manners_words(meter).to_lowercase();
            for bad in ["bad", "shame", "cheat", "quit", "penalt", "punish", "!"] {
                assert!(!w.contains(bad), "{w}");
            }
        }
        assert!(manners_words(75).contains("each finished game adds 5"));
        let mut p = Progress::default();
        p.last_game = Some(GameSummary {
            headline: "Table manners \u{2212}25: you left a game in progress. Finish 5 games to restore it.".into(),
            manners: -25,
            stars: -1,
            ..GameSummary::default()
        });
        let s = you(&rewards(p)).summary.unwrap();
        assert!(s.left);
        assert_eq!(s.stars, "\u{2212}1 star this season");
    }

    /// A summary is shown until it has been looked at, and then not again.
    #[test]
    fn a_summary_is_shown_once() {
        let mut p = Progress::default();
        p.last_game = Some(GameSummary {
            headline: "Game finished \u{00b7} +140 XP".into(),
            xp: vec![("Game finished".into(), 100), ("Hands played".into(), 40)],
            cards: vec!["C2".into(), "no such card".into()],
            level_before: 1,
            level_after: 2,
            stars: 2,
            ..GameSummary::default()
        });
        let mut r = rewards(p);
        let s = you(&r).summary.unwrap();
        assert_eq!(s.headline, "Game finished \u{00b7} +140 XP");
        assert_eq!(s.cards.len(), 1, "an id this build does not know is not drawn");
        assert_eq!((s.level_up, s.stars.as_str(), s.left), (Some(2), "+2 stars this season", false));
        r.mark_summary_seen();
        assert!(you(&r).summary.is_none());
    }

    /// The album holds the whole deck and the jokers, locked cards with how far
    /// they are, and the journal newest first with every reason.
    #[test]
    fn the_album_shows_the_deck_and_the_journal() {
        let mut p = Progress::default();
        p.stats.insert("games".into(), 7);
        p.cards.insert(
            "C2".into(),
            crate::storage::progress::Earned { when_unix_ms: 1_789_000_000_000, table: "Riverside".into(), place: 3, note: "3rd place of 6".into() },
        );
        p.log(1_789_000_000_000, "xp", 140, "Game finished");
        p.log(1_789_000_100_000, "manners", -25, "You left a game in progress");
        let a = album(&rewards(p), 0);
        assert_eq!(a.pages.len(), 5);
        assert_eq!(a.pages.iter().take(4).map(|(_, c)| c.len()).sum::<usize>(), 52);
        let clubs = &a.pages[0].1;
        assert!(clubs[0].earned.as_ref().unwrap().contains("Riverside"));
        assert!(clubs[0].earned.as_ref().unwrap().ends_with("3rd place of 6"));
        assert_eq!((clubs[5].card.id, clubs[5].have, clubs[5].need), ("C7", 7, 10));
        assert!(clubs[5].earned.is_none());
        assert_eq!(a.journal[0].delta, "\u{2212}25 manners");
        assert_eq!(a.journal[1].delta, "+140 XP");
        assert!(a.journal.iter().all(|l| !l.why.is_empty()));
    }
}
