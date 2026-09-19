//! `D-068`: the album's cards and the quests' templates -- data, and the one
//! table every number of the rewards is read from.
//!
//! **Every condition names the node's event it is counted from**, because the
//! window takes the node's word for every game fact: a card is earned by what
//! the node said happened, never by what this window worked out.
//!
//! **Nothing here is earned more easily by playing worse.** A card honours a
//! result -- a full house at a showdown, a place, a game played to its end --
//! and never a decision a player makes at the table: no *go all-in five times*,
//! no *win with seven-deuce*, no *call a river*. Those would spoil the game for
//! the others at the table, who came to play poker against somebody trying to
//! win. Nothing is drawn by lot either: a card is earned by a deed.

/// Experience for a Sit & Go played to its end.
pub const XP_GAME: u64 = 60;
/// And for each player beyond two the first hand dealt in: a fuller table is
/// worth more, which is what the search (`D-064`) tries for as well.
pub const XP_PER_PLAYER: u64 = 10;
/// For each hand dealt in, up to `XP_HANDS_CAP` a game.
pub const XP_PER_HAND: u64 = 2;
pub const XP_HANDS_CAP: u64 = 60;
/// For the win, per player at the table; and for a top-half place.
pub const XP_WIN_PER_PLAYER: u64 = 12;
pub const XP_TOP_HALF_PER_PLAYER: u64 = 4;
/// The first game finished in a day -- in place of a bonus for logging in,
/// because it is finished games that fill tables.
pub const XP_FIRST_OF_DAY: u64 = 50;
/// Table manners at or over the first number add the second, in per cent of a
/// game's experience: the meter's fall is felt, and so is its recovery.
pub const MANNERS_BONUS: [(u32, u64); 2] = [(90, 10), (70, 5)];
/// A day away adds this to the rested pool, up to the cap; a finished game
/// draws up to half its own experience from it. Rest is rewarded, not grind.
pub const RESTED_PER_DAY: u64 = 100;
pub const RESTED_MAX: u64 = 300;
/// Past this many finished games in a day, a game's completion earns half.
pub const SOFT_CAP_GAMES: u64 = 8;
/// What a deliberate leave of a running game costs the meter, of 100, and
/// what each finished game gives back.
pub const MANNERS_LEAVE: u32 = 25;
pub const MANNERS_BACK: u32 = 5;
/// Experience for a card, by rarity, and for a completed suit.
pub const CARD_XP: [u64; 4] = [30, 60, 120, 250];
pub const SUIT_XP: u64 = 500;
/// A hand in a cash game, and the most a day of them earns.
pub const XP_CASH_HAND: u64 = 1;
pub const XP_CASH_DAY_CAP: u64 = 60;
/// A game under way and unheard of for this long is settled as not finished
/// through no fault of the player: its hands are paid, nothing is taken.
pub const PENDING_EXPIRES_MS: u64 = 12 * 3_600_000;
/// The calm sentence about a break: after this long at the tables in one
/// sitting, or this many games in a row ended in the bottom half.
pub const BREAK_AFTER_MS: u64 = 2 * 3_600_000;
pub const BREAK_AFTER_LOSSES: u32 = 3;
/// Stars: three to a rank, ten ranks.
pub const STARS_PER_RANK: u32 = 3;
pub const STARS_MAX: u32 = 29;
/// A table of this many players or more pays the win one star more.
pub const BIG_TABLE: u32 = 6;
/// A new season begins this many ranks under where the last one ended.
pub const SEASON_DROP_RANKS: u32 = 3;

/// The season's ranks, lowest first: the hands every player knows by heart.
pub const RANKS: [&str; 10] = [
    "High Card",
    "Pair",
    "Two Pair",
    "Three of a Kind",
    "Straight",
    "Flush",
    "Full House",
    "Four of a Kind",
    "Straight Flush",
    "Royal Flush",
];
/// The ranks a season's stars never fall back under once reached (indices of
/// `RANKS`): Two Pair, Straight, Full House, Straight Flush.
pub const FLOOR_RANKS: [u32; 4] = [2, 4, 6, 8];
/// Below this rank no star is ever lost: a newcomer's first ranks only rise.
pub const PROTECTED_BELOW_RANK: u32 = 2;

/// The chip a level looks like: a new colour every ten levels.
pub const CHIPS: [&str; 6] = ["White", "Red", "Green", "Black", "Purple", "Gold"];

/// The experience a level begins at. Quick at first, slower later: level 2 is
/// inside the first game, level 10 about ten games in, level 60 some months.
pub const fn xp_for_level(level: u32) -> u64 {
    let n = if level == 0 { 0 } else { (level - 1) as u64 };
    100 * n + 20 * n * n.saturating_sub(1)
}

pub fn level_of(xp: u64) -> u32 {
    let mut level = 1;
    while xp_for_level(level + 1) <= xp {
        level += 1;
    }
    level
}

/// The chip's colour name for a level.
pub fn chip_of(level: u32) -> &'static str {
    CHIPS[(((level.max(1) - 1) / 10) as usize).min(CHIPS.len() - 1)]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
    /// The extra cards outside the fifty-two: hidden ones and the Bad Beat set.
    Joker,
}

impl Suit {
    pub const DECK: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];

    pub const fn symbol(self) -> &'static str {
        match self {
            Suit::Clubs => "\u{2663}",
            Suit::Diamonds => "\u{2666}",
            Suit::Hearts => "\u{2665}",
            Suit::Spades => "\u{2660}",
            Suit::Joker => "\u{2605}",
        }
    }

    /// What the set is about, as the album's page heads it.
    pub const fn set_name(self) -> &'static str {
        match self {
            Suit::Clubs => "The Regular",
            Suit::Diamonds => "The Hands",
            Suit::Hearts => "Good Company",
            Suit::Spades => "The Results",
            Suit::Joker => "Jokers",
        }
    }

    pub const fn set_about(self) -> &'static str {
        match self {
            Suit::Clubs => "Games played to the end, hands dealt, days at the tables.",
            Suit::Diamonds => "Hands that held up at a showdown.",
            Suit::Hearts => "Staying to the end, and the players you have met.",
            Suit::Spades => "Places, wins and seasons.",
            Suit::Joker => "Found by playing. Some remember a bad beat.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rarity {
    Bronze,
    Silver,
    Gold,
    Holo,
}

impl Rarity {
    pub const fn name(self) -> &'static str {
        match self {
            Rarity::Bronze => "Bronze",
            Rarity::Silver => "Silver",
            Rarity::Gold => "Gold",
            Rarity::Holo => "Holo",
        }
    }

    pub const fn xp(self) -> u64 {
        CARD_XP[self as usize]
    }
}

/// What a card's picture is of. Drawn by the window's painter from these --
/// chips, cards, felt -- so every picture is this project's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    /// Stacks of chips, this many chips in all.
    Chips(u8),
    /// Five cards fanned: `(rank 2..=14, suit 0..=3)` each.
    Hand([(u8, u8); 5]),
    /// Two hole cards; `cracked` draws the break through them.
    Hole { cards: [(u8, u8); 2], cracked: bool },
    /// The table from above, this many seats taken of ten.
    Table(u8),
    /// A month's page with this many days marked.
    Calendar(u8),
    /// A chain of this many links: games in a row.
    Chain(u8),
    /// This many players around.
    People(u8),
    /// A cup, with the number on its plinth.
    Trophy(u8),
    /// The podium, this many steps lit.
    Podium(u8),
    /// The table-manners meter, full.
    Gauge,
    /// An hourglass: a long game.
    Hourglass,
    /// One chip and a chair.
    ChipAndChair,
    /// A crown over five cards.
    Crown,
    /// A chip cut in two: a split pot.
    SplitChip,
    /// A season's emblem: this rank's stars in a wreath.
    Emblem(u8),
    /// A deck with a counter: hands dealt.
    Deck(u16),
}

pub struct Card {
    pub id: &'static str,
    pub suit: Suit,
    /// `2..=14` in the deck; a joker's number in its own row.
    pub rank: u8,
    pub title: &'static str,
    /// How it is earned, in words; the back of a locked card says this.
    pub how: &'static str,
    /// The counter it is read off, and the node's event that moves it.
    pub metric: &'static str,
    pub event: &'static str,
    pub need: u64,
    /// Shown as `?` until earned.
    pub hidden: bool,
    pub art: Art,
}

impl Card {
    /// Two is the easiest of a suit and the ace the hardest; the frame says so.
    pub const fn rarity(&self) -> Rarity {
        match (self.suit, self.rank) {
            (Suit::Joker, 1) => Rarity::Holo,
            (Suit::Joker, _) => Rarity::Gold,
            (_, 2..=5) => Rarity::Bronze,
            (_, 6..=9) => Rarity::Silver,
            (_, 10..=13) => Rarity::Gold,
            _ => Rarity::Holo,
        }
    }

    pub fn rank_label(&self) -> String {
        match self.rank {
            14 => "A".to_string(),
            13 => "K".to_string(),
            12 => "Q".to_string(),
            11 => "J".to_string(),
            n => n.to_string(),
        }
    }

    /// `♦9`, or `★3` for a joker.
    pub fn label(&self) -> String {
        format!("{}{}", self.suit.symbol(), self.rank_label())
    }
}

const fn card(
    id: &'static str,
    suit: Suit,
    rank: u8,
    title: &'static str,
    how: &'static str,
    metric: &'static str,
    event: &'static str,
    need: u64,
    art: Art,
) -> Card {
    Card { id, suit, rank, title, how, metric, event, need, hidden: false, art }
}

const fn hidden(
    id: &'static str,
    rank: u8,
    title: &'static str,
    how: &'static str,
    metric: &'static str,
    event: &'static str,
    need: u64,
    art: Art,
) -> Card {
    Card { id, suit: Suit::Joker, rank, title, how, metric, event, need, hidden: true, art }
}

use Suit::{Clubs as C, Diamonds as D, Hearts as H, Spades as S};

const FINISHED: &str = "Finished (this seat)";
const HAND_BEGAN: &str = "HandBegan (dealt in)";
const HAND_ENDED: &str = "HandEnded (shown, pots) with HoleCards and Board";
const ROSTER_KEYS: &str = "RosterKeys at the first HandBegan";

/// The fifty-two, suit by suit and two to ace, then the jokers.
pub const CARDS: &[Card] = &[
    // Clubs: the regular -- effort, which a player controls entirely.
    card("C2", C, 2, "First Game", "Finish your first Sit & Go.", "games", FINISHED, 1, Art::Chips(1)),
    card("C3", C, 3, "Warming Up", "Play 50 hands.", "hands", HAND_BEGAN, 50, Art::Deck(50)),
    card("C4", C, 4, "Five Down", "Finish 5 games.", "games", FINISHED, 5, Art::Chips(5)),
    card("C5", C, 5, "Company", "Finish a game at a table of 4 or more.", "games_4", FINISHED, 1, Art::Table(4)),
    card("C6", C, 6, "Three Days In", "Finish a game on 3 different days.", "days", FINISHED, 3, Art::Calendar(3)),
    card("C7", C, 7, "Ten Games", "Finish 10 games.", "games", FINISHED, 10, Art::Chips(10)),
    card("C8", C, 8, "Five Hundred Hands", "Play 500 hands.", "hands", HAND_BEGAN, 500, Art::Deck(500)),
    card("C9", C, 9, "Six-Max Regular", "Finish 5 games at tables of 6 or more.", "games_6", FINISHED, 5, Art::Table(6)),
    card("C10", C, 10, "A Fortnight", "Finish a game on 14 different days.", "days", FINISHED, 14, Art::Calendar(14)),
    card("CJ", C, 11, "Thirty Games", "Finish 30 games.", "games", FINISHED, 30, Art::Chips(18)),
    card("CQ", C, 12, "Two Thousand Hands", "Play 2,000 hands.", "hands", HAND_BEGAN, 2_000, Art::Deck(2_000)),
    card("CK", C, 13, "Fifty Games", "Finish 50 games.", "games", FINISHED, 50, Art::Chips(26)),
    card("CA", C, 14, "The Regular", "Finish 80 games.", "games", FINISHED, 80, Art::Chips(40)),
    // Diamonds: the hands -- what the cards came to at a showdown.
    card("D2", D, 2, "Showdown", "Win a pot at a showdown.", "showdowns", HAND_ENDED, 1, Art::Hand([(14, 3), (9, 2), (7, 1), (5, 0), (3, 1)])),
    card("D3", D, 3, "A Pair", "Win a showdown with a pair.", "sd_pair", HAND_ENDED, 1, Art::Hand([(10, 3), (10, 2), (13, 1), (7, 0), (4, 1)])),
    card("D4", D, 4, "Two Pair", "Win a showdown with two pair.", "sd_two_pair", HAND_ENDED, 1, Art::Hand([(12, 3), (12, 1), (8, 2), (8, 0), (14, 1)])),
    card("D5", D, 5, "Three of a Kind", "Win a showdown with three of a kind.", "sd_trips", HAND_ENDED, 1, Art::Hand([(7, 3), (7, 2), (7, 0), (13, 1), (2, 1)])),
    card("D6", D, 6, "Straight", "Win a showdown with a straight.", "sd_straight", HAND_ENDED, 1, Art::Hand([(5, 3), (6, 1), (7, 2), (8, 0), (9, 1)])),
    card("D7", D, 7, "Flush", "Win a showdown with a flush.", "sd_flush", HAND_ENDED, 1, Art::Hand([(14, 1), (11, 1), (9, 1), (6, 1), (3, 1)])),
    card("D8", D, 8, "Pocket Rockets", "Be dealt a pair of aces.", "pocket_aces", "HoleCards", 1, Art::Hole { cards: [(14, 3), (14, 2)], cracked: false }),
    card("D9", D, 9, "Full House", "Win a showdown with a full house.", "sd_full_house", HAND_ENDED, 1, Art::Hand([(13, 3), (13, 2), (13, 0), (9, 1), (9, 2)])),
    card("D10", D, 10, "Ace High", "Win a showdown with no pair at all.", "sd_high_card", HAND_ENDED, 1, Art::Hand([(14, 0), (12, 1), (9, 2), (6, 3), (4, 0)])),
    card("DJ", D, 11, "The Ladder", "Win showdowns with 6 different hand ranks.", "sd_kinds", HAND_ENDED, 6, Art::Hand([(2, 1), (5, 1), (8, 1), (11, 1), (14, 1)])),
    card("DQ", D, 12, "A Hundred Showdowns", "Win 100 pots at a showdown.", "showdowns", HAND_ENDED, 100, Art::Hand([(12, 1), (12, 2), (12, 0), (12, 3), (10, 1)])),
    card("DK", D, 13, "Four of a Kind", "Win a showdown with four of a kind.", "sd_quads", HAND_ENDED, 1, Art::Hand([(13, 0), (13, 1), (13, 2), (13, 3), (14, 1)])),
    card("DA", D, 14, "Straight Flush", "Win a showdown with a straight flush.", "sd_straight_flush", HAND_ENDED, 1, Art::Hand([(5, 1), (6, 1), (7, 1), (8, 1), (9, 1)])),
    // Hearts: good company -- staying to the end, and who was met on the way.
    card("H2", H, 2, "Stayed to the End", "Finish 3 games in a row.", "finish_streak_best", FINISHED, 3, Art::Chain(3)),
    card("H3", H, 3, "Good Standing", "Finish 5 games with table manners at 100.", "good_standing", FINISHED, 5, Art::Gauge),
    card("H4", H, 4, "New Faces", "Play with 5 different players.", "opponents", ROSTER_KEYS, 5, Art::People(3)),
    card("H5", H, 5, "Ten in a Row", "Finish 10 games in a row.", "finish_streak_best", FINISHED, 10, Art::Chain(5)),
    card("H6", H, 6, "The Long Game", "Finish a game of 60 hands or more.", "long_game", FINISHED, 60, Art::Hourglass),
    card("H7", H, 7, "Familiar Room", "Play with 15 different players.", "opponents", ROSTER_KEYS, 15, Art::People(5)),
    card("H8", H, 8, "A Week Straight", "Finish a game on 7 days in a row.", "day_streak_best", FINISHED, 7, Art::Calendar(7)),
    card("H9", H, 9, "Twenty-Five in a Row", "Finish 25 games in a row.", "finish_streak_best", FINISHED, 25, Art::Chain(7)),
    card("H10", H, 10, "Wide Circle", "Play with 30 different players.", "opponents", ROSTER_KEYS, 30, Art::People(7)),
    card("HJ", H, 11, "Full Table", "Finish a game at a table of 9 or 10.", "games_9", FINISHED, 1, Art::Table(10)),
    card("HQ", H, 12, "Three Weeks", "Finish a game on 21 days in a row.", "day_streak_best", FINISHED, 21, Art::Calendar(21)),
    card("HK", H, 13, "Everybody Knows You", "Play with 60 different players.", "opponents", ROSTER_KEYS, 60, Art::People(9)),
    card("HA", H, 14, "Iron Seat", "Finish 100 games in a row.", "finish_streak_best", FINISHED, 100, Art::Chain(9)),
    // Spades: the results -- places and wins, which luck has a hand in.
    card("S2", S, 2, "Top Half", "Finish a game in the top half.", "top_half", FINISHED, 1, Art::Podium(1)),
    card("S3", S, 3, "First Win", "Win a Sit & Go.", "wins", FINISHED, 1, Art::Trophy(1)),
    card("S4", S, 4, "Steady", "Finish in the top half 5 times.", "top_half", FINISHED, 5, Art::Podium(2)),
    card("S5", S, 5, "Hat-Trick", "Win 3 games.", "wins", FINISHED, 3, Art::Trophy(3)),
    card("S6", S, 6, "Beat the Table", "Win a game at a table of 4 or more.", "wins_4", FINISHED, 1, Art::Table(1)),
    card("S7", S, 7, "Back to Back", "Win 2 games in a row.", "win_streak_best", FINISHED, 2, Art::Trophy(2)),
    card("S8", S, 8, "Reliable", "Finish in the top half 25 times.", "top_half", FINISHED, 25, Art::Podium(3)),
    card("S9", S, 9, "Ten Wins", "Win 10 games.", "wins", FINISHED, 10, Art::Trophy(10)),
    card("S10", S, 10, "Six-Max Champion", "Win 3 games at tables of 6 or more.", "wins_6", FINISHED, 3, Art::Table(2)),
    card("SJ", S, 11, "Chip and a Chair", "Win a game after falling under a tenth of your starting stack.", "comebacks", FINISHED, 1, Art::ChipAndChair),
    card("SQ", S, 12, "Twenty-Five Wins", "Win 25 games.", "wins", FINISHED, 25, Art::Trophy(25)),
    card("SK", S, 13, "Full House Season", "Reach the season rank Full House.", "season_best", FINISHED, 6, Art::Emblem(6)),
    card("SA", S, 14, "Fifty Wins", "Win 50 games.", "wins", FINISHED, 50, Art::Trophy(50)),
    // Jokers, outside the fifty-two: found by playing, never announced. The
    // Bad Beat set turns the worst moment of an evening into a keepsake, as a
    // card room's bad-beat jackpot does.
    hidden("J1", 1, "Royal Flush", "Win a showdown with a royal flush.", "sd_royal", HAND_ENDED, 1, Art::Crown),
    hidden("J2", 2, "Split Pot", "Share a pot at a showdown.", "splits", HAND_ENDED, 1, Art::SplitChip),
    hidden("J3", 3, "Marathon", "Finish a game of 150 hands or more.", "long_game", FINISHED, 150, Art::Hourglass),
    hidden("J4", 4, "Three-Peat", "Win 3 games in a row.", "win_streak_best", FINISHED, 3, Art::Trophy(3)),
    hidden("J5", 5, "Bad Beat: Full House", "Lose a showdown holding a full house or better.", "bb_full_house", HAND_ENDED, 1, Art::Hand([(9, 3), (9, 2), (9, 0), (4, 1), (4, 2)])),
    hidden("J6", 6, "Aces Cracked", "Lose a showdown holding a pair of aces.", "bb_aces", HAND_ENDED, 1, Art::Hole { cards: [(14, 0), (14, 1)], cracked: true }),
    hidden("J7", 7, "Quads Beaten", "Lose a showdown holding four of a kind.", "bb_quads", HAND_ENDED, 1, Art::Hand([(8, 0), (8, 1), (8, 2), (8, 3), (2, 1)])),
    hidden("J8", 8, "So Close", "Finish second 5 times.", "runner_up", FINISHED, 5, Art::Podium(2)),
];

pub fn card_by_id(id: &str) -> Option<&'static Card> {
    CARDS.iter().find(|c| c.id == id)
}

/// A suit's thirteen, two first.
pub fn suit_cards(suit: Suit) -> impl Iterator<Item = &'static Card> {
    CARDS.iter().filter(move |c| c.suit == suit)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Always within the player's reach: games finished, hands played.
    Effort,
    /// The table and the people at it, and conduct.
    Table,
    /// Luck has a hand in these; a day draws one at most.
    Result,
}

pub struct QuestTemplate {
    pub id: &'static str,
    pub weekly: bool,
    pub family: Family,
    pub text: &'static str,
    /// The counter's name, and the node's event that moves it.
    pub metric: &'static str,
    pub event: &'static str,
    pub need: u64,
    pub xp: u64,
    /// Counts different things (table sizes, hand ranks), not occurrences.
    pub distinct: bool,
    /// A deliberate leave puts it back to nothing: *in a row*.
    pub unbroken: bool,
}

const fn quest(
    id: &'static str,
    weekly: bool,
    family: Family,
    text: &'static str,
    metric: &'static str,
    event: &'static str,
    need: u64,
    xp: u64,
) -> QuestTemplate {
    QuestTemplate { id, weekly, family, text, metric, event, need, xp, distinct: false, unbroken: false }
}

const fn special(mut q: QuestTemplate, distinct: bool, unbroken: bool) -> QuestTemplate {
    q.distinct = distinct;
    q.unbroken = unbroken;
    q
}

use Family::{Effort as E, Result as R, Table as T};

pub const QUESTS: &[QuestTemplate] = &[
    quest("q_finish_1", false, E, "Finish a game", "games", FINISHED, 1, 40),
    quest("q_finish_2", false, E, "Finish 2 games", "games", FINISHED, 2, 70),
    quest("q_hands_30", false, E, "Play 30 hands", "hands", HAND_BEGAN, 30, 40),
    quest("q_hands_60", false, E, "Play 60 hands", "hands", HAND_BEGAN, 60, 60),
    quest("q_table_4", false, T, "Finish a game at a table of 4 or more", "games_4", FINISHED, 1, 50),
    quest("q_table_6", false, T, "Finish a game at a table of 6 or more", "games_6", FINISHED, 1, 70),
    quest("q_new_faces", false, T, "Play with 2 players you have not met", "opponents", ROSTER_KEYS, 2, 50),
    quest("q_clean", false, T, "Finish a game without your clock running out", "clean_games", "Finished, with no SeatCertified for this seat", 1, 50),
    special(quest("q_sizes", false, T, "Finish games at 2 different table sizes", "size", FINISHED, 2, 60), true, false),
    quest("q_pots_3", false, R, "Win 3 pots", "pots", "HandEnded (pots)", 3, 40),
    quest("q_pots_6", false, R, "Win 6 pots", "pots", "HandEnded (pots)", 6, 60),
    quest("q_showdown", false, R, "Win a pot at a showdown", "showdowns", HAND_ENDED, 1, 40),
    quest("q_two_pair", false, R, "Win a showdown with two pair or better", "sd_two_pair_plus", HAND_ENDED, 1, 50),
    quest("q_top_half", false, R, "Finish in the top half", "top_half", FINISHED, 1, 60),
    quest("q_final_two", false, R, "Reach the final two", "final_two", FINISHED, 1, 70),
    quest("q_win", false, R, "Win a game", "wins", FINISHED, 1, 90),
    quest("w_finish_10", true, E, "Finish 10 games", "games", FINISHED, 10, 300),
    quest("w_hands_300", true, E, "Play 300 hands", "hands", HAND_BEGAN, 300, 300),
    quest("w_days_5", true, E, "Finish a game on 5 different days", "days", FINISHED, 5, 300),
    special(quest("w_row_8", true, E, "Finish 8 games in a row", "games", FINISHED, 8, 300), false, true),
    quest("w_table_4", true, T, "Finish 5 games at tables of 4 or more", "games_4", FINISHED, 5, 350),
    quest("w_new_faces", true, T, "Play with 5 players you have not met", "opponents", ROSTER_KEYS, 5, 300),
    quest("w_top_half_5", true, R, "Finish in the top half 5 times", "top_half", FINISHED, 5, 350),
    quest("w_wins_3", true, R, "Win 3 games", "wins", FINISHED, 3, 400),
    quest("w_pots_25", true, R, "Win 25 pots", "pots", "HandEnded (pots)", 25, 300),
    special(quest("w_kinds_4", true, R, "Win showdowns with 4 different hand ranks", "sd_kind", HAND_ENDED, 4, 350), true, false),
];

pub fn quest_by_id(id: &str) -> Option<&'static QuestTemplate> {
    QUESTS.iter().find(|q| q.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Fifty-two cards in four suits of thirteen, two to ace, and the jokers
    /// beside them; every id once.
    #[test]
    fn the_album_is_a_deck_of_fifty_two_and_its_jokers() {
        for suit in Suit::DECK {
            let ranks: Vec<u8> = suit_cards(suit).map(|c| c.rank).collect();
            assert_eq!(ranks, (2..=14).collect::<Vec<u8>>(), "{suit:?}");
        }
        assert_eq!(CARDS.iter().filter(|c| c.suit != Suit::Joker).count(), 52);
        assert!(suit_cards(Suit::Joker).count() >= 6);
        assert!(suit_cards(Suit::Joker).all(|c| c.hidden), "a joker is found by playing");
        let ids: BTreeSet<&str> = CARDS.iter().map(|c| c.id).collect();
        assert_eq!(ids.len(), CARDS.len());
    }

    /// Every card has a picture of its own: no two cards draw the same thing.
    #[test]
    fn every_card_has_its_own_picture() {
        for (i, a) in CARDS.iter().enumerate() {
            for b in &CARDS[i + 1..] {
                // The same motif is allowed across suits only where the frame,
                // the suit's colour and the number differ; inside the deck the
                // motif and its number are unique but for the two streak cups.
                if a.art == b.art {
                    assert!(a.suit != b.suit, "{} and {} draw the same picture", a.id, b.id);
                }
            }
        }
    }

    /// Within a suit the two is the easiest and the need never falls where the
    /// counter is the same.
    #[test]
    fn a_suit_gets_harder_towards_the_ace() {
        for suit in Suit::DECK {
            let cards: Vec<&Card> = suit_cards(suit).collect();
            for (i, a) in cards.iter().enumerate() {
                for b in &cards[i + 1..] {
                    if a.metric == b.metric {
                        assert!(a.need < b.need, "{} before {}", a.id, b.id);
                    }
                }
            }
            assert_eq!(cards[0].rarity(), Rarity::Bronze);
            assert_eq!(cards[12].rarity(), Rarity::Holo);
        }
    }

    /// `D-068`'s line: no quest and no card is about a decision at the table.
    #[test]
    fn nothing_rewards_playing_worse() {
        let banned = ["all-in", "all in", "bluff", "call ", "raise", "bet ", "fold", "7-2", "seven-deuce", "limp", "see a flop", "see the flop"];
        for text in CARDS.iter().map(|c| c.how).chain(QUESTS.iter().map(|q| q.text)) {
            let lower = text.to_lowercase();
            for b in banned {
                assert!(!lower.contains(b), "{text:?} names a decision at the table ({b:?})");
            }
        }
    }

    /// At least twenty templates; each names the node's event it counts; a
    /// day can always draw one of each family.
    #[test]
    fn the_quests_name_their_events() {
        assert!(QUESTS.len() >= 20);
        assert!(QUESTS.iter().all(|q| !q.event.is_empty() && !q.metric.is_empty() && q.need > 0 && q.xp > 0));
        assert!(CARDS.iter().all(|c| !c.event.is_empty() && c.need > 0));
        for weekly in [false, true] {
            for family in [E, T, R] {
                assert!(QUESTS.iter().any(|q| q.weekly == weekly && q.family == family), "{weekly} {family:?}");
            }
        }
        let ids: BTreeSet<&str> = QUESTS.iter().map(|q| q.id).collect();
        assert_eq!(ids.len(), QUESTS.len());
    }

    /// The first level is inside the first game; ten levels to a chip colour.
    #[test]
    fn levels_are_quick_at_first_and_slower_later() {
        assert_eq!(level_of(0), 1);
        assert_eq!(level_of(99), 1);
        assert_eq!(level_of(100), 2);
        assert!(xp_for_level(3) - xp_for_level(2) > xp_for_level(2) - xp_for_level(1));
        assert_eq!(chip_of(1), "White");
        assert_eq!(chip_of(10), "White");
        assert_eq!(chip_of(11), "Red");
        assert_eq!(chip_of(51), "Gold");
        assert_eq!(chip_of(400), "Gold");
        for xp in [0u64, 100, 101, 2_340, 74_340, 1_000_000] {
            let l = level_of(xp);
            assert!(xp_for_level(l) <= xp && xp < xp_for_level(l + 1));
        }
    }
}
