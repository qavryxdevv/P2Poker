use super::*;
use crate::net::node::PotEnd;
use crate::storage::progress::Keys;

const DAY_MS: u64 = 86_400_000;
/// A Monday: day 20 003 is 2024-10-07.
const D0: u64 = 20_003;

fn keys() -> Keys {
    Keys::derive(&ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]))
}

fn fresh() -> Rewards {
    let mut r = Rewards::new(Progress::default(), keys());
    r.import(&crate::storage::results::Results::default(), at(D0, 0));
    r
}

fn at(day: u64, ms: u64) -> Now {
    Now { unix_ms: day * DAY_MS + ms, offset_min: 0 }
}

/// A small generator, so a simulated month is the same month every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn seven(&mut self) -> [u8; 7] {
        let mut out = [255u8; 7];
        let mut n = 0;
        while n < 7 {
            let c = (self.next() % 52) as u8;
            if !out[..n].contains(&c) {
                out[n] = c;
                n += 1;
            }
        }
        out
    }
}

/// One Sit & Go as the node tells it, this client in seat 0.
struct Game {
    slot: u8,
    key: [u8; 32],
    session: [u8; 32],
    players: u8,
    hand: u64,
    /// Whose keys sit in the other seats: varied, so opponents are met.
    who: u8,
}

impl Game {
    fn new(n: u64, players: u8) -> Game {
        let mut session = [0u8; 32];
        session[..8].copy_from_slice(&n.to_le_bytes());
        Game { slot: 0, key: [1; 32], session, players, hand: 0, who: (n % 200) as u8 }
    }

    fn sit(&self, r: &mut Rewards, now: Now, tournament: bool) {
        let seats: Vec<u8> = (0..self.players).collect();
        r.on_event(&NodeEvent::AtTable { slot: self.slot, key: Some(self.key) }, now);
        r.on_event(
            &NodeEvent::TableParams {
                key: self.key,
                name: "Riverside".into(),
                seats: self.players,
                needed: 2,
                small_blind: 10,
                big_blind: 20,
                action_ms: 30_000,
                tournament,
            },
            now,
        );
        r.on_event(&NodeEvent::Seated { key: self.key, seat: 0 }, now);
        r.on_event(&NodeEvent::Roster { key: self.key, seats: seats.iter().map(|s| (*s, format!("p{s}"), 1_500)).collect() }, now);
        let keys = seats.iter().map(|s| (*s, [self.who.wrapping_add(*s).wrapping_mul(3) % 47 + 1; 32])).collect();
        r.on_event(&NodeEvent::RosterKeys { key: self.key, keys }, now);
        r.on_event(&NodeEvent::TableReal { key: self.key, session: self.session }, now);
    }

    fn begin(&mut self, r: &mut Rewards, now: Now) {
        self.hand += 1;
        r.on_event(&NodeEvent::AtTable { slot: self.slot, key: Some(self.key) }, now);
        r.on_event(
            &NodeEvent::HandBegan { hand_id: self.hand, button: 0, dealt_in: (0..self.players).collect(), small_blind: 10, big_blind: 20 },
            now,
        );
    }

    /// A hand with these cards: `won` by this seat or not, at a showdown or not.
    fn hand(&mut self, r: &mut Rewards, now: Now, cards: [u8; 7], won: bool, showdown: bool) {
        self.begin(r, now);
        let n = usize::from(self.players);
        r.on_event(&NodeEvent::HoleCards { hand_id: self.hand, cards: [cards[0], cards[1]] }, now);
        if showdown {
            r.on_event(&NodeEvent::Board { hand_id: self.hand, cards: cards[2..].to_vec() }, now);
        }
        let mut folded = vec![false; n];
        folded[0] = !won && !showdown;
        r.on_event(
            &NodeEvent::TableState { hand_id: self.hand, street: 3, pot: 100, to_act: None, stacks: vec![1_500; n], bets: vec![0; n], folded },
            now,
        );
        let mut shown = vec![None; n];
        if showdown {
            shown[0] = Some([cards[0], cards[1]]);
            shown[1] = Some([50, 51]);
        }
        let winners = vec![if won { 0 } else { 1 }];
        r.on_event(
            &NodeEvent::HandEnded { hand_id: self.hand, stacks: vec![1_500; n], shown, pots: vec![PotEnd { size: 100, winners }], gained: vec![] },
            now,
        );
    }

    fn quiet_hands(&mut self, r: &mut Rewards, now: Now, n: u64) {
        for _ in 0..n {
            self.hand(r, now, [0, 13, 1, 2, 3, 4, 5], false, false);
        }
    }

    fn finish(&self, r: &mut Rewards, now: Now, place: u8) {
        r.on_event(&NodeEvent::AtTable { slot: self.slot, key: Some(self.key) }, now);
        r.on_event(
            &NodeEvent::Finished { hand_id: self.hand, seat: 0, place, tied: false, players_left: place.saturating_sub(1), over: place <= 2 },
            now,
        );
    }

    fn leave(&self, r: &mut Rewards, now: Now) {
        r.on_event(&NodeEvent::AtTable { slot: self.slot, key: Some(self.key) }, now);
        r.on_event(&NodeEvent::LeftTable { why: "left the table".into() }, now);
    }
}

/// A whole game: `hands` quiet hands and a place.
fn play(r: &mut Rewards, n: u64, now: Now, players: u8, hands: u64, place: u8) {
    let mut g = Game::new(n, players);
    g.sit(r, now, true);
    g.quiet_hands(r, now, hands);
    g.finish(r, now, place);
}

fn xp_lines(r: &Rewards) -> i64 {
    r.progress.journal.iter().filter(|l| l.axis == "xp").map(|l| l.delta).sum()
}

/// The newcomer's first game, lost: experience with its reasons, a new level,
/// the first card, a quest moved -- and the nearest goal in sight.
#[test]
fn a_first_game_even_lost_moves_everything_forward() {
    let mut r = fresh();
    assert_eq!(r.level(), 1);
    assert!(r.next_goal().is_some(), "a goal is in sight before the first game");
    play(&mut r, 1, at(D0, 1_000), 6, 20, 6);
    let g = r.progress.last_game.clone().expect("the game left a summary");
    assert!(g.headline.starts_with("Game finished \u{00b7} +"), "{}", g.headline);
    assert!(g.xp.iter().any(|(why, xp)| why == "Game finished" && *xp == XP_GAME + 4 * XP_PER_PLAYER), "{:?}", g.xp);
    assert!(g.xp.iter().any(|(why, xp)| why == "Hands played" && *xp == 40));
    assert!(g.xp.iter().any(|(why, _)| why == "First game today"));
    assert!(g.level_after > g.level_before, "the first level comes inside the first game");
    assert!(g.cards.contains(&"C2".to_string()), "{:?}", g.cards);
    assert_eq!(r.progress.cards["C2"].table, "Riverside");
    assert_eq!(r.progress.cards["C2"].note, "6th place of 6");
    let q = &r.progress.quests;
    assert!(q.daily.iter().any(|d| d.have > 0), "a quest moved: {:?}", q.daily);
    let notices = r.take_notices();
    assert!(notices.contains(&Notice::Card("C2")) && notices.contains(&Notice::Summary));
    assert!(notices.iter().any(|n| matches!(n, Notice::TableLine { slot: 0, text } if text.starts_with("Game finished"))));
    assert!(r.next_goal().is_some());
}

/// A loss costs no experience and no level, ever; every change has its reason
/// and the reasons add up to the number.
#[test]
fn a_loss_never_costs_experience_and_every_change_says_why() {
    let mut r = fresh();
    let mut last = 0;
    for n in 0..30u64 {
        play(&mut r, n, at(D0 + n / 3, n * 1_000), 6, 15, 6);
        assert!(r.progress.xp > last, "game {n}");
        last = r.progress.xp;
    }
    assert!(r.progress.journal.iter().all(|l| !l.why.is_empty()));
    assert!(r.progress.journal.iter().filter(|l| l.axis == "xp").all(|l| l.delta >= 0));
    // The journal is bounded, so add up a short run instead.
    let mut r = fresh();
    for n in 0..4u64 {
        play(&mut r, n, at(D0, n), 3, 5, 3);
    }
    assert_eq!(xp_lines(&r), r.progress.xp as i64, "the reasons add up to the experience");
}

/// The one penalty: the player's own command, a set table, a hand dealt, this
/// seat not finished, nothing to excuse it. The game's experience is not
/// granted, the run of finished games ends, the meter falls by 25 -- and says
/// why, and five finished games bring it back.
#[test]
fn leaving_a_running_game_costs_manners_and_says_the_way_back() {
    let mut r = fresh();
    play(&mut r, 1, at(D0, 0), 6, 10, 3);
    let (xp, level) = (r.progress.xp, r.level());
    assert_eq!(r.progress.stat("finish_streak"), 1);

    let mut g = Game::new(2, 6);
    g.sit(&mut r, at(D0, 10), true);
    g.quiet_hands(&mut r, at(D0, 20), 12);
    g.leave(&mut r, at(D0, 30));

    assert_eq!(r.progress.manners, 100 - MANNERS_LEAVE);
    assert_eq!(r.progress.xp, xp, "experience never falls, and the left game's is not granted");
    assert_eq!(r.level(), level);
    assert_eq!(r.progress.stat("finish_streak"), 0);
    assert_eq!(r.progress.stat("games"), 1, "the game left is not a game finished");
    let s = r.progress.last_game.clone().unwrap();
    assert_eq!(s.headline, "Table manners \u{2212}25: you left a game in progress. Finish 5 games to restore it.");
    assert_eq!(s.manners, -25);
    assert!(r.progress.journal.iter().any(|l| l.axis == "manners" && l.delta == -25 && l.why.contains("left a game")));
    assert!(r.progress.journal.iter().any(|l| l.why.contains("24 XP for its hands not granted")));

    for n in 3..8u64 {
        play(&mut r, n, at(D0, 100 + n), 2, 3, 2);
    }
    assert_eq!(r.progress.manners, 100, "five finished games restore it");
}

/// Never the network, never the table's fault, never a doubt.
#[test]
fn nothing_but_a_deliberate_leave_is_penalised() {
    type Setup = fn(&mut Rewards, &Game, Now);
    let cases: [(&str, Setup); 9] = [
        ("an unsafe table", |r, _, now| r.on_event(&NodeEvent::TableUnsafe { why: Some("flooders".into()) }, now)),
        // `S1-IX`: a table stopped on a disagreement about a hand's result.
        ("a table stopped on a disagreement", |r, g, now| {
            let stop = crate::net::node::TableStop { hand_id: g.hand, seats: vec![2], ended: None };
            r.on_event(&NodeEvent::TableStopped { stop: Some(stop) }, now)
        }),
        ("put out by the table", |r, g, now| {
            r.on_event(&NodeEvent::OutForGood { key: g.key, why: "fourth absence".into(), flooded: false }, now)
        }),
        ("a hand standing on an absent seat", |r, g, now| {
            r.on_event(&NodeEvent::StageStands { hand_id: g.hand, seats: vec![3] }, now)
        }),
        ("the player's own line down", |r, _, now| r.on_event(&NodeEvent::ToxLine { how: "offline" }, now)),
        ("the table lost first", |r, _, now| r.on_event(&NodeEvent::TableLost { why: "the founder is gone".into() }, now)),
        ("every opponent off the line", |r, g, now| {
            for seat in 1..g.players {
                r.on_event(&NodeEvent::SeatLink { seat, rtt_ms: None, group: false, quiet_s: Some(90), away: false }, now);
            }
        }),
        ("a timeout and sitting out, then the table's end", |r, g, now| {
            r.on_event(&NodeEvent::SeatCertified { seat: 0, hand_id: g.hand }, now);
            r.on_event(&NodeEvent::SittingOut { on: true }, now);
            g.finish(r, now, 5);
        }),
        ("this seat finished already", |r, g, now| g.finish(r, now, 4)),
    ];
    for (name, setup) in cases {
        let mut r = fresh();
        let mut g = Game::new(9, 6);
        g.sit(&mut r, at(D0, 0), true);
        g.quiet_hands(&mut r, at(D0, 1), 8);
        setup(&mut r, &g, at(D0, 2));
        g.leave(&mut r, at(D0, 3));
        assert_eq!(r.progress.manners, 100, "{name}");
        assert_eq!(r.progress.stat("leaves"), 0, "{name}");
        assert!(r.progress.xp >= 16, "{name}: the hands played are paid: {}", r.progress.xp);
        assert!(r.progress.journal.iter().all(|l| l.delta >= 0), "{name}: nothing was taken");
        r.on_event(&NodeEvent::ToxLine { how: "udp" }, at(D0, 4));
    }

    // Before the first hand nothing is under way; a cash game is left at will.
    let mut r = fresh();
    let g = Game::new(10, 6);
    g.sit(&mut r, at(D0, 0), true);
    g.leave(&mut r, at(D0, 1));
    let mut c = Game::new(11, 6);
    c.sit(&mut r, at(D0, 2), false);
    c.quiet_hands(&mut r, at(D0, 3), 5);
    c.leave(&mut r, at(D0, 4));
    assert_eq!((r.progress.manners, r.progress.stat("leaves")), (100, 0));
    assert_eq!(r.progress.stat("hands"), 5, "cash hands count as hands");
    assert!(r.progress.pending.is_empty());
}

/// Sitting out at the end with a sound line: the game counts and its place
/// stands, the bonus for playing it to the end is not paid, nothing is taken.
#[test]
fn a_game_finished_while_sitting_out_pays_no_completion_bonus() {
    let mut r = fresh();
    let mut g = Game::new(1, 6);
    g.sit(&mut r, at(D0, 0), true);
    g.quiet_hands(&mut r, at(D0, 1), 10);
    r.on_event(&NodeEvent::SittingOut { on: true }, at(D0, 2));
    g.finish(&mut r, at(D0, 3), 5);
    let s = r.progress.last_game.clone().unwrap();
    assert!(s.headline.contains("while sitting out"));
    assert!(!s.xp.iter().any(|(why, _)| why == "Game finished"));
    assert!(s.xp.iter().any(|(why, _)| why == "Hands played"));
    assert_eq!((r.progress.manners, r.progress.stat("games"), r.progress.stat("finish_streak")), (100, 1, 0));
}

/// A game is counted once: the node says the table is set again to a seat
/// back from a restart, and a place twice over is one place.
#[test]
fn the_same_game_is_counted_once_and_survives_a_restart() {
    let mut r = fresh();
    let mut g = Game::new(5, 4);
    g.sit(&mut r, at(D0, 0), true);
    g.quiet_hands(&mut r, at(D0, 1), 7);
    assert_eq!(r.progress.pending.len(), 1);
    assert_eq!(r.progress.pending[0].hands, 7);
    assert!(!r.progress.pending[0].id.contains("0500"), "the session is not written down");

    // The client is killed here. What the file holds is what comes back.
    let saved = serde_json::to_string(&r.progress).unwrap();
    let mut r = Rewards::new(serde_json::from_str(&saved).unwrap(), keys());
    g.sit(&mut r, at(D0, 2), true);
    g.quiet_hands(&mut r, at(D0, 3), 3);
    assert_eq!(r.progress.pending[0].hands, 10, "the game goes on where it was");
    g.finish(&mut r, at(D0, 4), 2);
    let xp = r.progress.xp;
    assert_eq!(r.progress.stat("games"), 1);

    g.finish(&mut r, at(D0, 5), 2);
    let mut r = Rewards::new(r.progress.clone(), keys());
    g.sit(&mut r, at(D0, 6), true);
    g.quiet_hands(&mut r, at(D0, 7), 3);
    g.finish(&mut r, at(D0, 8), 2);
    g.leave(&mut r, at(D0, 9));
    assert_eq!((r.progress.stat("games"), r.progress.xp, r.progress.manners), (1, xp, 100));
}

/// A game the client never came back to is settled as nobody's fault: its
/// hands are paid half a day later, nothing is taken.
#[test]
fn a_game_never_returned_to_is_settled_without_a_penalty() {
    let mut r = fresh();
    let mut g = Game::new(5, 4);
    g.sit(&mut r, at(D0, 0), true);
    g.quiet_hands(&mut r, at(D0, 1), 9);
    let mut r = Rewards::new(r.progress.clone(), keys());
    r.tick(at(D0, PENDING_EXPIRES_MS - 1));
    assert_eq!(r.progress.pending.len(), 1);
    r.tick(at(D0, PENDING_EXPIRES_MS + 1));
    assert!(r.progress.pending.is_empty());
    assert_eq!((r.progress.xp, r.progress.manners, r.progress.stat("games")), (18, 100, 0));
}

/// A clock turned back earns nothing new; a jump forward turns the day once.
#[test]
fn the_clock_cannot_be_farmed() {
    let mut r = fresh();
    play(&mut r, 1, at(D0 + 5, 0), 2, 2, 2);
    let quests = r.progress.quests.clone();
    assert_eq!(quests.day, D0 + 5);
    let first_bonuses = |r: &Rewards| r.progress.journal.iter().filter(|l| l.why == "First game today").count();
    assert_eq!(first_bonuses(&r), 1);

    // Back three days: no new day, no new quests, no second first-game bonus.
    play(&mut r, 2, at(D0 + 2, 0), 2, 2, 2);
    assert_eq!(r.progress.quests.day, D0 + 5);
    assert_eq!(first_bonuses(&r), 1);
    assert_eq!(r.progress.clock_high_unix_ms, (D0 + 5) * DAY_MS);
    assert!(r.progress.journal.iter().all(|l| l.when_unix_ms >= (D0 + 5) * DAY_MS || l.when_unix_ms == D0 * DAY_MS));

    // Forward forty days: one new set of dailies, one weekly offer, one season turn.
    r.tick(at(D0 + 45, 0));
    assert_eq!(r.progress.quests.day, D0 + 45);
    assert_eq!(r.progress.quests.daily.len(), 3);
    assert_eq!(r.progress.quests.weekly_offer.len(), 3);
    assert_eq!(r.progress.journal.iter().filter(|l| l.why.starts_with("A new season")).count(), 1);
    assert!(r.progress.stat("st_rested") <= RESTED_MAX);
}

/// The hands at a showdown are named by the evaluator from the node's cards:
/// a full house won is the ♦9, lost it is a Bad Beat; aces dealt, aces cracked.
#[test]
fn showdown_cards_come_from_the_nodes_cards() {
    use crate::poker::state::{Card, Rank, Suit as S};
    let c = |r, s| Card::new(r, s).index();
    let mut r = fresh();
    let mut g = Game::new(1, 3);
    g.sit(&mut r, at(D0, 0), true);
    // K K in hand, K 9 9 x x on the board.
    let boat = [
        c(Rank::King, S::Spades),
        c(Rank::King, S::Hearts),
        c(Rank::King, S::Clubs),
        c(Rank::Nine, S::Diamonds),
        c(Rank::Nine, S::Hearts),
        c(Rank::Four, S::Clubs),
        c(Rank::Two, S::Spades),
    ];
    g.hand(&mut r, at(D0, 1), boat, true, true);
    assert!(r.progress.cards.contains_key("D9"), "{:?}", r.progress.cards.keys());
    assert!(r.progress.cards.contains_key("D2"));
    assert_eq!(r.progress.cards["D9"].note, "Won with a full house");
    assert!(!r.progress.cards.contains_key("J5"));
    assert!(r.progress.journal.iter().any(|l| l.why == "New card: \u{2666}9 Full House"));

    g.hand(&mut r, at(D0, 2), boat, false, true);
    assert!(r.progress.cards.contains_key("J5"), "a full house beaten is remembered");

    let aces = [c(Rank::Ace, S::Spades), c(Rank::Ace, S::Hearts), boat[2], boat[3], boat[5], boat[6], c(Rank::Seven, S::Clubs)];
    g.hand(&mut r, at(D0, 3), aces, false, true);
    assert!(r.progress.cards.contains_key("D8") && r.progress.cards.contains_key("J6"));

    // Won without a showdown: a pot, and no hand to name.
    let before = r.progress.stat("showdowns");
    g.hand(&mut r, at(D0, 4), boat, true, false);
    assert_eq!(r.progress.stat("showdowns"), before);
    assert_eq!(r.progress.stat("pots"), 2);

    let royal = [
        c(Rank::Ace, S::Hearts),
        c(Rank::King, S::Hearts),
        c(Rank::Queen, S::Hearts),
        c(Rank::Jack, S::Hearts),
        c(Rank::Ten, S::Hearts),
        c(Rank::Two, S::Clubs),
        c(Rank::Three, S::Spades),
    ];
    assert_eq!(showdown_hand([royal[0], royal[1]], &royal[2..]), Some((Category::StraightFlush, true)));
    let steel = [c(Rank::Nine, S::Hearts), royal[1], royal[2], royal[3], royal[4], royal[5], royal[6]];
    assert_eq!(showdown_hand([steel[0], steel[1]], &steel[2..]), Some((Category::StraightFlush, false)));
}

/// The season: a win is two stars (three at a table of six), the top half one,
/// the bottom half takes one -- never from a newcomer's ranks, never under a
/// floor reached -- the third top half in a row adds one, a leave is last
/// place, and a new month begins three ranks lower.
#[test]
fn the_seasons_stars_have_floors_and_a_soft_reset() {
    let mut r = fresh();
    r.tick(at(D0, 0));
    assert_eq!(r.stars(1, 6, 6, false), 0, "a newcomer's bottom half costs nothing");
    assert_eq!(r.stars(1, 1, 2, false), 2);
    assert_eq!(r.stars(1, 1, 6, false), 3);
    assert_eq!(r.stars(1, 2, 6, false), 2, "the third top half in a row adds a star");
    assert_eq!(r.progress.season.stars, 7);
    assert_eq!(rank_of(7), 2);
    assert_eq!(r.stars(1, 6, 6, false), -1);
    assert_eq!(r.stars(1, 6, 6, false), 0, "Two Pair was reached: the season does not fall under it");
    assert_eq!(r.progress.season.stars, 6);
    assert_eq!(r.stars(1, u32::MAX, 6, true), 0, "a leave is last place, under the same floors");

    r.progress.season.stars = 20;
    r.progress.season.best = 20;
    assert_eq!(floor_of(20), 18);
    r.tick(at(D0 + 31, 0));
    let s = &r.progress.season;
    assert_eq!((s.last_rank, s.stars, s.best), (Some(6), 9, 9), "three ranks under Full House");
    assert_eq!(r.progress.stat("season_best"), 2, "only what a result reached counts for the card");
    for _ in 0..40 {
        r.stars(1, 1, 9, false);
    }
    assert_eq!(r.progress.season.stars, STARS_MAX);
    assert_eq!(RANKS[rank_of(STARS_MAX) as usize], "Royal Flush");
}

/// Autonomy: one daily may be swapped a day, for another of its family; the
/// week's challenge is picked from three and counts only once picked.
#[test]
fn a_daily_swaps_once_a_day_and_the_weekly_is_chosen() {
    let mut r = fresh();
    r.tick(at(D0, 0));
    let before = r.progress.quests.daily.clone();
    let families: Vec<Family> = before.iter().map(|q| quest_by_id(&q.id).unwrap().family).collect();
    assert_eq!(families, vec![Family::Effort, Family::Table, Family::Result]);
    assert!(r.swap_daily(2, at(D0, 1)));
    assert_ne!(r.progress.quests.daily[2].id, before[2].id);
    assert_eq!(quest_by_id(&r.progress.quests.daily[2].id).unwrap().family, Family::Result);
    assert!(!r.swap_daily(1, at(D0, 2)), "one swap a day");
    assert!(r.swap_daily(1, at(D0 + 1, 0)), "and one the next day");

    assert_eq!(r.progress.quests.weekly_offer.len(), 3);
    assert!(r.progress.quests.weekly_offer.iter().all(|q| quest_by_id(&q.id).unwrap().weekly));
    play(&mut r, 1, at(D0 + 1, 5), 2, 2, 2);
    assert!(r.progress.quests.weekly.is_none(), "nothing counts towards a challenge nobody chose");
    let chosen = r.progress.quests.weekly_offer[0].id.clone();
    assert!(r.pick_weekly(0, at(D0 + 1, 6)));
    assert_eq!(r.progress.quests.weekly.as_ref().unwrap().id, chosen);
    assert!(!r.pick_weekly(0, at(D0 + 1, 7)));
    // The same profile draws the same quests for the same day.
    let mut again = fresh();
    again.tick(at(D0, 0));
    assert_eq!(again.progress.quests.daily, before);
    // The next Monday offers three again.
    r.tick(at(D0 + 7, 0));
    assert!(r.progress.quests.weekly.is_none() && r.progress.quests.weekly_offer.len() == 3);
}

/// A quest that counts different things counts each once, and *in a row* goes
/// back to nothing at a deliberate leave.
#[test]
fn distinct_and_unbroken_quests_count_as_they_say() {
    let mut r = fresh();
    r.tick(at(D0, 0));
    r.progress.quests.daily[1] = Quest { id: "q_sizes".into(), need: 2, xp: 60, ..Quest::default() };
    r.progress.quests.weekly = Some(Quest { id: "w_row_8".into(), need: 8, xp: 300, ..Quest::default() });
    play(&mut r, 1, at(D0, 1), 4, 2, 4);
    play(&mut r, 2, at(D0, 2), 4, 2, 4);
    assert_eq!(r.progress.quests.daily[1].have, 1, "two games at one size are one size");
    play(&mut r, 3, at(D0, 3), 6, 2, 6);
    assert!(r.progress.quests.daily[1].done);
    assert_eq!(r.progress.quests.weekly.as_ref().unwrap().have, 3);
    let mut g = Game::new(4, 4);
    g.sit(&mut r, at(D0, 4), true);
    g.quiet_hands(&mut r, at(D0, 5), 2);
    g.leave(&mut r, at(D0, 6));
    assert_eq!(r.progress.quests.weekly.as_ref().unwrap().have, 0);
}

/// `D-043`: two tables at once, their events interleaved, each settled as
/// its own game.
#[test]
fn two_tables_at_once_are_two_games() {
    let mut r = fresh();
    let mut a = Game::new(1, 4);
    let mut b = Game { slot: 1, key: [2; 32], ..Game::new(2, 6) };
    a.sit(&mut r, at(D0, 0), true);
    b.sit(&mut r, at(D0, 1), true);
    for i in 0..6 {
        a.hand(&mut r, at(D0, 10 + i), [0, 13, 1, 2, 3, 4, 5], false, false);
        b.hand(&mut r, at(D0, 10 + i), [0, 13, 1, 2, 3, 4, 5], false, false);
    }
    a.finish(&mut r, at(D0, 20), 1);
    assert_eq!(r.progress.pending.len(), 1, "the other table is still under way");
    b.leave(&mut r, at(D0, 21));
    assert_eq!((r.progress.stat("games"), r.progress.stat("wins"), r.progress.stat("leaves")), (1, 1, 1));
    assert_eq!(r.progress.manners, 100 + 0 - MANNERS_LEAVE);
    let lines: Vec<u8> = r.take_notices().iter().filter_map(|n| if let Notice::TableLine { slot, .. } = n { Some(*slot) } else { None }).collect();
    assert_eq!(lines, vec![0, 1], "each table's log gets its own line");
}

/// The returning player: `results.cbor` is read in once.
#[test]
fn the_record_is_read_in_once() {
    use crate::storage::results::{Entry, Results};
    let mut results = Results::default();
    for (i, place) in [1u8, 4, 2, 6, 1].into_iter().enumerate() {
        results.record(Entry { when_unix_ms: (D0 - 9 + i as u64) * DAY_MS, table: "Old".into(), seats: 6, place, tied: false });
    }
    let mut r = Rewards::new(Progress::default(), keys());
    r.import(&results, at(D0, 0));
    assert_eq!((r.progress.stat("games"), r.progress.stat("wins"), r.progress.stat("days")), (5, 2, 5));
    assert!(r.progress.cards.contains_key("C4") && r.progress.cards.contains_key("S3"));
    assert_eq!(r.progress.cards["C4"].note, "From your record");
    assert!(r.level() > 1);
    let xp = r.progress.xp;
    r.import(&results, at(D0, 1));
    assert_eq!(r.progress.xp, xp, "once");
    assert!(r.progress.quests.daily.iter().all(|q| q.have == 0), "old games complete no quest");
}

/// Rest pays and grind does not: days away fill a pool a game draws on, and
/// past eight games a day a game's completion earns half.
#[test]
fn rest_is_rewarded_and_a_long_day_earns_half() {
    let mut r = fresh();
    play(&mut r, 1, at(D0, 0), 6, 10, 6);
    play(&mut r, 2, at(D0 + 4, 0), 6, 10, 6);
    let s = r.progress.last_game.clone().unwrap();
    let rested = s.xp.iter().find(|(why, _)| why == "Rested bonus").map(|(_, x)| *x);
    assert_eq!(rested, Some(60), "half of the game's own 120, from a pool of 300: {:?}", s.xp);
    assert_eq!(r.progress.stat("st_rested"), 240);

    for n in 3..12u64 {
        play(&mut r, n, at(D0 + 9, n), 2, 0, 2);
    }
    let s = r.progress.last_game.clone().unwrap();
    assert!(s.xp.iter().any(|(why, xp)| why.contains("half") && *xp == XP_GAME / 2), "{:?}", s.xp);
}

/// The calm sentence about a break is said once a sitting, after three rough
/// games in a row.
#[test]
fn a_break_is_suggested_once_and_calmly() {
    let mut r = fresh();
    for n in 0..6u64 {
        play(&mut r, n, at(D0, n), 6, 5, 6);
    }
    let breaks: Vec<&'static str> = r.take_notices().into_iter().filter_map(|n| if let Notice::Break(w) = n { Some(w) } else { None }).collect();
    assert_eq!(breaks.len(), 1);
    assert!(!breaks[0].contains('!') && !breaks[0].to_lowercase().contains("win it back"));
}

struct Month {
    rewards: Rewards,
    /// Whether each game in turn brought something new.
    news: Vec<bool>,
    first_suit_day: Option<u64>,
}

/// A month of Sit & Gos as the node would tell them: table sizes as a search
/// finds them, a place drawn evenly, real cards at the showdowns.
fn a_month(games_a_day: u64) -> Month {
    let mut r = fresh();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ games_a_day);
    let (mut news, mut first_suit_day, mut n) = (Vec::new(), None, 0u64);
    for day in 0..30u64 {
        r.tick(at(D0 + day, 0));
        if r.progress.quests.weekly.is_none() {
            r.pick_weekly(0, at(D0 + day, 0));
        }
        for g in 0..games_a_day {
            n += 1;
            let players = [2u8, 6, 4, 6, 3, 9, 6, 5][(rng.next() % 8) as usize];
            let place = (rng.next() % u64::from(players)) as u8 + 1;
            let hands = 25 + rng.next() % 35;
            let now = at(D0 + day, 3_600_000 * (8 + g));
            let mut game = Game::new(n, players);
            game.sit(&mut r, now, true);
            r.take_notices();
            for _ in 0..hands {
                let cards = rng.seven();
                let showdown = rng.next() % 4 == 0;
                let won = rng.next() % u64::from(players) == 0;
                game.hand(&mut r, now, cards, won, showdown || !won && false);
            }
            game.finish(&mut r, now, place);
            news.push(r.take_notices().iter().any(|x| matches!(x, Notice::Card(_) | Notice::Level(_) | Notice::Quest { .. })));
            if first_suit_day.is_none() && Suit::DECK.iter().any(|s| suit_cards(*s).all(|c| r.progress.cards.contains_key(c.id))) {
                first_suit_day = Some(day);
            }
        }
    }
    Month { rewards: r, news, first_suit_day }
}

/// `D-068`'s pace, measured: at three games a day something new at least every
/// second game of the first week, the first suit within about a month, the
/// whole deck not for months; at one game a day a month still goes somewhere;
/// at eight a day the deck is still not done. And two thirds of the
/// experience is for what a player controls.
#[test]
fn the_pace_of_a_month() {
    let m = a_month(3);
    let r = &m.rewards;
    for pair in m.news[..21].chunks(2) {
        assert!(pair.iter().any(|x| *x), "two games of the first week brought nothing new: {:?}", &m.news[..21]);
    }
    let day = m.first_suit_day.expect("a suit is complete within the month at three games a day");
    assert!((20..30).contains(&day), "the first suit on day {day}");
    let deck = CARDS.iter().filter(|c| c.suit != Suit::Joker).filter(|c| r.progress.cards.contains_key(c.id)).count();
    assert!((26..48).contains(&deck), "{deck} of 52 after a month");
    assert!((20..45).contains(&r.level()), "level {}", r.level());

    let slow = a_month(1);
    let cards = slow.rewards.progress.cards.len();
    assert!(slow.rewards.level() >= 8 && cards >= 14, "level {} and {cards} cards at one game a day", slow.rewards.level());
    assert!(slow.first_suit_day.is_none());

    let fast = a_month(8);
    let deck = CARDS.iter().filter(|c| c.suit != Suit::Joker).filter(|c| fast.rewards.progress.cards.contains_key(c.id)).count();
    assert!(deck < 52, "the whole deck takes months even at eight games a day");
}

/// Reward what the player controls: at least two thirds of a month's
/// experience is for games played to the end, hands, quests, conduct and rest;
/// the result moves the stars.
#[test]
fn two_thirds_of_the_experience_is_for_effort() {
    // The journal is bounded, so the month is added up game by game.
    let mut r = fresh();
    let mut rng = Rng(42);
    let (mut effort, mut result) = (0u64, 0u64);
    for n in 0..90u64 {
        let players = [2u8, 6, 4, 6, 3, 9][(rng.next() % 6) as usize];
        let place = (rng.next() % u64::from(players)) as u8 + 1;
        let before = r.progress.journal.len().min(crate::storage::progress::JOURNAL_MAX);
        let mark = r.progress.journal.last().cloned();
        play(&mut r, n, at(D0 + n / 3, n), players, 30, place);
        let fresh_lines: Vec<_> = match mark {
            Some(m) => r.progress.journal.iter().rev().take_while(|l| **l != m).cloned().collect(),
            None => r.progress.journal.clone(),
        };
        let _ = before;
        for l in fresh_lines.iter().filter(|l| l.axis == "xp") {
            let by_result = l.why == "Won the game"
                || l.why == "Top half"
                || l.why.starts_with("New card: \u{2660}")
                || l.why.starts_with("New card: \u{2666}")
                || ["Win ", "Finish in the top", "Reach the final"].iter().any(|w| l.why.starts_with(&format!("Quest: {w}")));
            if by_result {
                result += l.delta as u64;
            } else {
                effort += l.delta as u64;
            }
        }
    }
    assert!(effort * 3 >= (effort + result) * 2, "effort {effort}, result {result}");
}

/// `D-078`: Linux says its offset through `date +%z`, and only that shape is
/// believed -- anything else is UTC, never a day that turns over at noon.
#[test]
fn an_offset_is_read_as_date_prints_it_and_nothing_else() {
    assert_eq!(parse_utc_offset("+0200"), Some(120));
    assert_eq!(parse_utc_offset("-0530"), Some(-330));
    assert_eq!(parse_utc_offset("+0000"), Some(0));
    assert_eq!(parse_utc_offset("+1400"), Some(840));
    for bad in ["", "0200", "+02:00", "+200", "+02000", "+2400", "+0260", "UTC", "-", "+ab12"] {
        assert_eq!(parse_utc_offset(bad), None, "{bad:?}");
    }
}
