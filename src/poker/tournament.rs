//! Tournament presets and the blind schedule
//! (`docs/research/POKER_RULES.md` part B).
//!
//! `SPEC_CS.md` section 4 requires one named preset with a fixed seat count,
//! equal starting stacks, a blind schedule that raises after a given number of
//! hands, and time limits — with the values taken from PokerTH's default rated
//! Sit-and-Go.
//!
//! Every value below that PokerTH fixes is cited to the file and line it was
//! read from. PokerTH's `ServerGame::CheckSettings` *rejects* a rated game
//! whose settings differ from these constants, which is the strongest evidence
//! available of what "the rated preset" means: they are enforced, not defaults.

use crate::poker::state::Chips;

/// A named set of table parameters, agreed before the first card.
///
/// Every field is part of the signed table advertisement, so all participants
/// saw the same values before any cryptographic material existed
/// (`SPEC_CS.md` section 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    pub id: &'static str,
    pub seats: u8,
    pub min_players_to_start: u8,
    pub start_stack: Chips,
    pub first_small_blind: Chips,
    pub ante: Chips,
    /// Blinds double after this many hands.
    pub raise_every_hands: u32,
    /// Ceiling on the small blind: half of all the chips in the tournament.
    pub small_blind_cap: Chips,
    pub action_timeout_sec: u32,
    pub action_timeout_grace_sec: u32,
    /// How long one cryptographic step of a hand may take.
    ///
    /// Feeds [`Preset::hand_deadline_floor_ms`], where it is by far the largest
    /// term: a hand has `2n + 23` crypto steps against `4n` betting actions.
    pub crypto_step_timeout_sec: u32,
    /// Presentation only. The protocol never waits for it.
    pub hand_delay_sec: u32,
    pub hand_deadline_sec: u32,
    pub join_deadline_sec: u32,
}

/// PokerTH's rated Sit-and-Go, `GAME_TYPE_RANKING`.
///
/// Sources, all in `the PokerTH source tree`:
///
/// | Value | Source |
/// |---|---|
/// | seats 10 | `src/game_defs.h:80` `RANKING_GAME_NUMBER_OF_PLAYERS` |
/// | stack 10000 | `src/game_defs.h:79` `RANKING_GAME_START_CASH` |
/// | small blind 50 | `src/game_defs.h:81` `RANKING_GAME_START_SBLIND` |
/// | every 11 hands | `src/game_defs.h:82` `RANKING_GAME_RAISE_EVERY_HAND` |
/// | doubling | `src/net/servergame.cpp:1267` `DOUBLE_BLINDS` |
/// | cap seats*stack/2 | `src/engine/game.cpp:318` |
/// | action timeout 20 s | `src/gamedata.h:80`, `src/config/configfile.cpp:233` |
/// | hand delay 7 s | `src/config/configfile.cpp:232` |
///
/// PokerTH has no ante, no per-hand time limit and no payout structure — it
/// records the finishing place only. The grace period, the hand and join
/// deadlines and `min_players_to_start` are ours; PokerTH leaves them free and
/// `SPEC_CS.md` sections 4 and 19 require them. The grace is 5 s rather than
/// PokerTH's 2 s because PokerTH's only has to cover one client-to-server hop
/// on a trusted clock, while here every peer times out independently with no
/// shared clock.
pub const RATED_SNG_POKERTH_V1: Preset = Preset {
    id: "RATED_SNG_POKERTH_V1",
    seats: 10,
    min_players_to_start: 10,
    start_stack: 10_000,
    first_small_blind: 50,
    ante: 0,
    raise_every_hands: 11,
    small_blind_cap: 50_000,
    action_timeout_sec: 20,
    action_timeout_grace_sec: 5,
    crypto_step_timeout_sec: 30,
    hand_delay_sec: 7,
    // 2 297 s is this preset's own floor at ten seats; the extra buys
    // reopening raises. See `hand_deadline_floor_ms`.
    hand_deadline_sec: 2_700,
    join_deadline_sec: 120,
};

/// A two-seat table, for the first supported mode.
///
/// **Ours, not PokerTH's.** `SPEC_CS.md` section 32 requires heads-up play
/// money to work before anything else, and the rated Sit-and-Go cannot be
/// played two-handed — `CheckSettings` rejects a rated game with any other seat
/// count. Section 4 permits a founder to use custom parameters, so this is one
/// such set, named so that two clients can agree on it without negotiating.
///
/// The blind structure is the rated one scaled to a two-player chip pool, so
/// the cap stays at half the chips in play.
pub const HEADS_UP_PLAY_MONEY_V1: Preset = Preset {
    id: "HEADS_UP_PLAY_MONEY_V1",
    seats: 2,
    min_players_to_start: 2,
    start_stack: 10_000,
    first_small_blind: 50,
    ante: 0,
    raise_every_hands: 11,
    small_blind_cap: 10_000, // seats * start_stack / 2
    action_timeout_sec: 20,
    action_timeout_grace_sec: 5,
    crypto_step_timeout_sec: 30,
    hand_delay_sec: 7,
    // 1 017 s is the floor at two seats.
    hand_deadline_sec: 1_200,
    join_deadline_sec: 120,
};

impl Preset {
    /// Half of all the chips the tournament started with.
    ///
    /// PokerTH computes this from the number of players the game *started*
    /// with, not the number still alive, so it is constant for the whole
    /// tournament (`src/engine/game.cpp:318`).
    pub const fn derived_cap(&self) -> Chips {
        (self.seats as Chips) * self.start_stack / 2
    }

    /// The blind level a hand belongs to, counting from 1.
    ///
    /// Hands are numbered from 1. PokerTH raises when
    /// `lastHandBlindsRaised + raiseEvery <= currentHandID`, with
    /// `lastHandBlindsRaised` starting at 1, so the first raise lands on hand
    /// 12 and each level is exactly `raise_every_hands` long
    /// (`src/engine/game.cpp:286-291`, `:51-52`).
    pub const fn level(&self, hand: u32) -> u32 {
        if hand == 0 {
            return 1;
        }
        1 + (hand - 1) / self.raise_every_hands
    }

    /// The small blind for a hand, doubling each level and then clamped.
    pub fn small_blind(&self, hand: u32) -> Chips {
        let doublings = self.level(hand) - 1;
        // Beyond 63 doublings the shift would overflow, and the cap has long
        // since bitten anyway. A tournament never gets there, but a malformed
        // hand number off the network could.
        let scaled = if doublings >= Chips::BITS {
            Chips::MAX
        } else {
            self.first_small_blind.saturating_mul(1u64 << doublings)
        };
        scaled.min(self.small_blind_cap)
    }

    /// The big blind is twice the small blind
    /// (`src/engine/local_engine/localberopreflop.cpp:44`).
    pub fn big_blind(&self, hand: u32) -> Chips {
        self.small_blind(hand) * 2
    }

    /// The smallest whole-hand deadline at which legal play can finish
    /// (`PROTOCOL.md` §8.2).
    ///
    /// A hand is `6n + 20` round trips. `4n` of them are betting actions, each
    /// costing an action timeout plus its grace; the other `2n + 23` are
    /// cryptographic steps. So
    ///
    /// ```text
    /// floor(n) = hand_delay
    ///          + (2n + 23) * crypto_step_timeout
    ///          + 4n        * (action_timeout + grace)
    /// ```
    ///
    /// A single unscaled figure here is what made legal play at six seats and
    /// up abort itself: the action term alone crosses 600 s at six seats. Two
    /// and four seats survived only because the crypto term carries three
    /// orders of magnitude of margin a healthy table never spends — which is
    /// why narrow testing missed it and widening the seat count found it.
    ///
    /// A table advertising less than this is refused by the joiner rather than
    /// silently corrected: the deadline is a signed table parameter, and two
    /// peers running different whole-hand deadlines is a divergence.
    pub const fn hand_deadline_floor_ms(&self) -> u64 {
        let n = self.seats as u64;
        let delay = self.hand_delay_sec as u64 * 1_000;
        let crypto = (2 * n + 23) * (self.crypto_step_timeout_sec as u64 * 1_000);
        let action = 4 * n
            * ((self.action_timeout_sec as u64 + self.action_timeout_grace_sec as u64) * 1_000);
        delay + crypto + action
    }

    /// How many reopening raises one hand can afford above the floor.
    ///
    /// A raise that reopens the action entitles up to `n - 1` further actions,
    /// and the headroom above the floor is what pays for them. A hand with more
    /// reopenings than this still aborts on a legal path; that residual is
    /// stated rather than hidden.
    pub const fn reopenings(&self) -> u64 {
        let n = self.seats as u64;
        if n < 2 {
            return 0;
        }
        let deadline = self.hand_deadline_sec as u64 * 1_000;
        let floor = self.hand_deadline_floor_ms();
        if deadline <= floor {
            return 0;
        }
        let per_reopening =
            (n - 1) * ((self.action_timeout_sec as u64 + self.action_timeout_grace_sec as u64) * 1_000);
        (deadline - floor) / per_reopening
    }

    /// Total chips in play, which never changes in a tournament.
    pub const fn chips_in_tournament(&self) -> Chips {
        (self.seats as Chips) * self.start_stack
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rated_preset_matches_pokerth_constant_for_constant() {
        let p = RATED_SNG_POKERTH_V1;
        assert_eq!(p.seats, 10, "RANKING_GAME_NUMBER_OF_PLAYERS");
        assert_eq!(p.start_stack, 10_000, "RANKING_GAME_START_CASH");
        assert_eq!(p.first_small_blind, 50, "RANKING_GAME_START_SBLIND");
        assert_eq!(p.raise_every_hands, 11, "RANKING_GAME_RAISE_EVERY_HAND");
        assert_eq!(p.ante, 0, "PokerTH has no ante");
        assert_eq!(
            p.small_blind_cap,
            p.derived_cap(),
            "the shipped cap must equal seats * stack / 2"
        );
        assert_eq!(p.small_blind_cap, 50_000);
    }

    /// The published schedule from POKER_RULES.md B2, level by level.
    #[test]
    fn the_blind_schedule_matches_the_derived_table() {
        let p = RATED_SNG_POKERTH_V1;
        let expected: [(u32, u32, Chips); 12] = [
            (1, 11, 50),
            (12, 22, 100),
            (23, 33, 200),
            (34, 44, 400),
            (45, 55, 800),
            (56, 66, 1_600),
            (67, 77, 3_200),
            (78, 88, 6_400),
            (89, 99, 12_800),
            (100, 110, 25_600),
            (111, 121, 50_000), // 51 200 clamped to the cap
            (122, 132, 50_000),
        ];
        for (level, (first, last, sb)) in expected.iter().enumerate() {
            let level = level as u32 + 1;
            assert_eq!(p.level(*first), level, "hand {first} opens level {level}");
            assert_eq!(p.level(*last), level, "hand {last} closes level {level}");
            assert_eq!(p.small_blind(*first), *sb, "level {level} small blind");
            assert_eq!(p.small_blind(*last), *sb, "level {level} small blind");
            assert_eq!(p.big_blind(*first), sb * 2, "big blind is twice the small");
        }
    }

    #[test]
    fn the_first_raise_lands_on_hand_twelve() {
        let p = RATED_SNG_POKERTH_V1;
        assert_eq!(p.small_blind(11), 50);
        assert_eq!(p.small_blind(12), 100, "PokerTH raises on 1 + 11");
    }

    #[test]
    fn the_cap_is_half_the_chips_and_holds_forever() {
        let p = RATED_SNG_POKERTH_V1;
        assert_eq!(p.small_blind_cap * 2, p.chips_in_tournament());
        for hand in [111, 200, 1_000, 100_000, u32::MAX] {
            assert_eq!(p.small_blind(hand), 50_000, "hand {hand} must stay capped");
        }
    }

    /// A hand number off the network is attacker-controlled, so the schedule
    /// must not overflow or panic on any `u32`.
    #[test]
    fn no_hand_number_can_overflow_the_schedule() {
        let p = RATED_SNG_POKERTH_V1;
        for hand in [0u32, 1, 63, 64, 65, 700, 1_000_000, u32::MAX - 1, u32::MAX] {
            let sb = p.small_blind(hand);
            assert!(sb > 0 && sb <= p.small_blind_cap, "hand {hand} gave {sb}");
            assert_eq!(p.big_blind(hand), sb * 2);
        }
    }

    #[test]
    fn blinds_never_decrease_as_the_tournament_runs() {
        let p = RATED_SNG_POKERTH_V1;
        let mut previous = 0;
        for hand in 1..500u32 {
            let sb = p.small_blind(hand);
            assert!(sb >= previous, "small blind fell at hand {hand}");
            previous = sb;
        }
    }

    #[test]
    fn the_heads_up_preset_is_two_handed_and_caps_at_half_its_own_pool() {
        let p = HEADS_UP_PLAY_MONEY_V1;
        assert_eq!(p.seats, 2, "spec section 32 wants heads-up first");
        assert_eq!(p.min_players_to_start, 2);
        assert_eq!(p.small_blind_cap, p.derived_cap());
        assert_eq!(p.small_blind_cap, 10_000);
        assert_eq!(p.chips_in_tournament(), 20_000);
        // The rated preset cannot be played two-handed: PokerTH's
        // CheckSettings rejects any other seat count for a rated game.
        assert_ne!(RATED_SNG_POKERTH_V1.seats, 2);
    }

    /// The table `PROTOCOL.md` §8.2 derives, reproduced exactly.
    #[test]
    fn the_deadline_floor_matches_the_published_derivation() {
        let base = RATED_SNG_POKERTH_V1;
        for (seats, expected) in [
            (2u8, 1_017_000u64),
            (4, 1_337_000),
            (6, 1_657_000),
            (10, 2_297_000),
        ] {
            let p = Preset { seats, ..base };
            assert_eq!(p.hand_deadline_floor_ms(), expected, "at {seats} seats");
        }
    }

    /// The defect this floor exists to catch, kept as a regression: the
    /// shipped deadline used to be 600 000 ms, which is below the floor at
    /// **every** seat count, two and four included.
    #[test]
    fn six_hundred_seconds_is_below_the_floor_at_every_seat_count() {
        let base = RATED_SNG_POKERTH_V1;
        for seats in [2u8, 4, 6, 8, 10] {
            let p = Preset { seats, ..base };
            assert!(
                p.hand_deadline_floor_ms() > 600_000,
                "600 s would have been legal at {seats} seats"
            );
        }
    }

    /// Every preset this project ships must satisfy its own floor, or a joiner
    /// following the spec refuses the table it advertises.
    #[test]
    fn every_shipped_preset_meets_its_own_floor() {
        for p in [RATED_SNG_POKERTH_V1, HEADS_UP_PLAY_MONEY_V1] {
            let deadline_ms = p.hand_deadline_sec as u64 * 1_000;
            assert!(
                deadline_ms >= p.hand_deadline_floor_ms(),
                "{} advertises {} ms against a floor of {} ms",
                p.id,
                deadline_ms,
                p.hand_deadline_floor_ms()
            );
        }
    }

    #[test]
    fn the_shipped_presets_can_afford_at_least_one_reopening_raise() {
        for p in [RATED_SNG_POKERTH_V1, HEADS_UP_PLAY_MONEY_V1] {
            assert!(
                p.reopenings() >= 1,
                "{} leaves no headroom for a reopening raise",
                p.id
            );
        }
    }

    #[test]
    fn a_deadline_at_exactly_the_floor_affords_no_reopening() {
        let floor = RATED_SNG_POKERTH_V1.hand_deadline_floor_ms();
        let tight = Preset {
            hand_deadline_sec: (floor / 1_000) as u32,
            ..RATED_SNG_POKERTH_V1
        };
        assert_eq!(tight.reopenings(), 0, "no headroom means no reopening");
    }

    #[test]
    fn timing_values_respect_pokerths_only_hard_floor() {
        // src/net/servergame.cpp:1257-1260 rejects an action timeout below 5 s
        // on a non-LAN server. Ours must clear that for every preset.
        for p in [RATED_SNG_POKERTH_V1, HEADS_UP_PLAY_MONEY_V1] {
            assert!(p.action_timeout_sec >= 5, "{} is below PokerTH's floor", p.id);
            assert!(p.hand_deadline_sec > p.action_timeout_sec);
            assert!(p.join_deadline_sec > 0);
        }
    }
}
