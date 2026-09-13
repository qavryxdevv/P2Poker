//! Where the seats of PokerTH's table go.
//!
//! A port of the QML client's `pages/seatlayout.js` and the geometry around it
//! in `GamePage.qml` (PokerTH, commit `944e8b83`), desktop branch: the hero's
//! box at the bottom in the middle, the opponents on the arc of an open
//! ellipse around it ("the necklace model"), every box scaled by the largest
//! factor at which neighbours do not overlap, found by bisection; the
//! community cards at the boxes' centre of gravity. A tall window takes
//! PokerTH's fixed portrait slots instead.
//!
//! Two departures, both the owner's words (2026-09-13), both making room the
//! original does not reserve:
//!
//! * **the chips a seat has in front of it sit above its box** ("a small label
//!   with a chip and the amount above the seat"), so the top of the ellipse
//!   keeps that label's height free under the status bar and two boxes one
//!   above the other keep it between them;
//! * the seats are **the roster's**, all of them, in seat order clockwise from
//!   the hero's left -- a seat that left stays where it was, drawn dim, as
//!   PokerTH's *keep empty seats* keeps it.
//!
//! Pure geometry, tested without a window.

use eframe::egui::{pos2, vec2, Pos2, Rect, Vec2};

/// PokerTH's opponent box on a wide desktop table (`GamePage.qml`: 84 high,
/// `2·4 + 40 + 4 + 2·29 + 4` wide).
pub const OPP_W: f32 = 114.0;
pub const OPP_H: f32 = 84.0;
/// The hero's box: 96 high, `2·4 + 52 + 4 + 2·37 + 4` wide.
pub const SELF_W: f32 = 142.0;
pub const SELF_H: f32 = 96.0;
/// The chip-and-amount label above a box, and the air under it.
pub const BET_LABEL_H: f32 = 20.0;
pub const BET_LABEL_GAP: f32 = 4.0;
/// The community row at scale one: five 46×64 slots, 3 apart, 8 more between
/// the flop and the turn and between the turn and the river.
pub const BOARD_CARD: Vec2 = Vec2::new(46.0, 64.0);
pub const BOARD_GAP: f32 = 3.0;
pub const BOARD_STREET_GAP: f32 = 8.0;
/// The pot badge above the row.
pub const POT_H: f32 = 20.0;
pub const POT_GAP: f32 = 8.0;
/// The dealer and blind pucks.
pub const PUCK: f32 = 32.0;

/// Where a seat's chips and puck go beside its box (`betSide`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    /// The top centre seat: the puck to the left, as PokerTH's `betSplit`.
    Split,
}

/// One opponent's box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeatBox {
    pub seat: u8,
    /// The box as drawn, scaled.
    pub rect: Rect,
    /// Where its puck goes.
    pub side: Side,
    /// The box sits in the upper half of the ellipse.
    pub upper: bool,
}

/// The whole table zone's geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct Seats {
    pub zone: Rect,
    pub wide: bool,
    /// PokerTH's `boxScale`.
    pub scale: f32,
    /// The hero's box as drawn.
    pub hero: Rect,
    pub hero_seat: u8,
    pub others: Vec<SeatBox>,
    /// The vertical centre of the community row, in screen coordinates.
    pub board_center: Pos2,
    /// PokerTH's `communityScale`.
    pub board_scale: f32,
}

impl Seats {
    /// The five community slots, flop, turn and river spaced as PokerTH spaces
    /// them.
    pub fn board(&self) -> [Rect; 5] {
        let s = self.board_scale;
        let card = BOARD_CARD * s;
        let width = 5.0 * card.x + 4.0 * BOARD_GAP * s + 2.0 * BOARD_STREET_GAP * s;
        let mut x = self.board_center.x - width / 2.0;
        let top = self.board_center.y - card.y / 2.0;
        let mut out = [Rect::NOTHING; 5];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = Rect::from_min_size(pos2(x, top), card);
            x += card.x + BOARD_GAP * s;
            if i == 2 || i == 3 {
                x += BOARD_STREET_GAP * s;
            }
        }
        out
    }

    /// The pot badge's centre, above the community row.
    pub fn pot_center(&self) -> Pos2 {
        let s = self.board_scale;
        pos2(
            self.board_center.x,
            self.board_center.y - BOARD_CARD.y * s / 2.0 - POT_GAP * s - POT_H * s / 2.0,
        )
    }

    /// The box of `seat`, the hero's included.
    pub fn box_of(&self, seat: u8) -> Option<Rect> {
        if seat == self.hero_seat {
            return Some(self.hero);
        }
        self.others.iter().find(|b| b.seat == seat).map(|b| b.rect)
    }

    /// The chip label's rectangle above a box, `width` wide.
    pub fn bet_label(&self, seat: u8, width: f32) -> Option<Rect> {
        let r = self.box_of(seat)?;
        let h = BET_LABEL_H * self.scale;
        let gap = BET_LABEL_GAP * self.scale;
        if seat == self.hero_seat {
            // PokerTH's own box keeps its chips in the strip above it, on the
            // right; the badge and the clock take the rest of the strip.
            return Some(Rect::from_min_size(pos2(r.right() - width, r.top() - gap - h), vec2(width, h)));
        }
        Some(Rect::from_min_size(pos2(r.center().x - width / 2.0, r.top() - gap - h), vec2(width, h)))
    }

    /// The puck beside a box.
    pub fn puck(&self, seat: u8) -> Option<Rect> {
        let size = vec2(PUCK, PUCK) * self.scale;
        if seat == self.hero_seat {
            let r = self.hero;
            return Some(Rect::from_min_size(pos2(r.right() + 6.0 * self.scale, r.top()), size));
        }
        let b = self.others.iter().find(|b| b.seat == seat)?;
        let r = b.rect;
        let y = r.top() + r.height() * 5.0 / 6.0 - size.y / 2.0;
        Some(match b.side {
            Side::Left => Rect::from_min_size(pos2(r.left() - 8.0 * self.scale - size.x, y), size),
            Side::Right => Rect::from_min_size(pos2(r.right() + 8.0 * self.scale, y), size),
            Side::Split => Rect::from_min_size(
                pos2(r.left() - 8.0 * self.scale - size.x, r.center().y - size.y / 2.0),
                size,
            ),
        })
    }
}

/// PokerTH's inputs, in one place.
#[derive(Debug, Clone, Copy)]
struct Env {
    w: f32,
    h: f32,
    wide: bool,
    /// Opponents on the ring.
    ring: usize,
}

const SELF_BADGE_GAP: f32 = 8.0;
const SIDE_BADGE_GAP: f32 = 48.0;

/// `buildLandscapeSlots`, desktop: each opponent's centre as fractions of the
/// zone, and the ellipse they sit on.
fn landscape_slots(e: &Env, s: f32, for_probe: bool) -> (Vec<(f32, f32)>, f32, f32, f32) {
    let visual_w = OPP_W * s;
    let visual_h = OPP_H * s;
    let self_visual_h = SELF_H * s;
    let side_margin = (e.w * 0.025).max(18.0) + SIDE_BADGE_GAP * s;
    let self_gap_y = SELF_BADGE_GAP * s;
    let side_x = (side_margin + visual_w / 2.0) / e.w.max(1.0);
    let radius_x = (0.5 - side_x).clamp(0.22, 0.36);
    // The top keeps the chip label above the topmost box clear of the bar.
    let top_y = (12.0 + (BET_LABEL_H + BET_LABEL_GAP) * s + visual_h / 2.0) / e.h.max(1.0);
    let self_top = e.h - 4.0 - self_visual_h;
    let bottom_y = (self_top - self_gap_y - visual_h / 2.0) / e.h.max(1.0);
    let center_y = (top_y + bottom_y) / 2.0;
    let radius_y = (bottom_y - top_y) / 2.0;

    let side_gravity = 0.25;
    let top_cos_squash = 1.4;
    let lower_gravity = 0.15;
    let point = |degrees: f32| -> (f32, f32) {
        let radians = degrees.to_radians();
        let sin_v = radians.sin();
        let mut cos_v = radians.cos();
        let sin_orig = sin_v;
        if sin_v <= 0.0 && cos_v != 0.0 {
            cos_v = cos_v.signum() * cos_v.abs().powf(top_cos_squash);
        }
        let mut v = sin_v + side_gravity * cos_v.abs() + if sin_v > 0.0 { lower_gravity * sin_v } else { 0.0 };
        if !for_probe && cos_v != 0.0 {
            let pair_spread = 0.02 * cos_v.abs();
            v += if sin_orig < 0.0 { -pair_spread } else { pair_spread };
        }
        v = v.clamp(-1.0, 1.0);
        if v < 0.0 {
            v *= 0.82;
        }
        let sq_lift = ((1.6 - e.w / e.h.max(1.0)) / 0.6).clamp(0.0, 1.0);
        if sin_v < 0.0 && sq_lift > 0.0 {
            v = (v - 0.3 * sq_lift * cos_v.abs()).max(-1.0);
        }
        (0.5 + radius_x * cos_v, center_y + radius_y * v)
    };
    let opps = e.ring.max(1);
    let self_weight = 0.3;
    let d_opp = 360.0 / (opps as f32 + self_weight);
    let d_self = self_weight * d_opp;
    let first = 90.0 + (d_self + d_opp) / 2.0;
    let slots = (0..e.ring).map(|i| point(first + i as f32 * d_opp)).collect();
    (slots, radius_x, radius_y, center_y)
}

/// `slotPosPortraitFixed` with `slotSeqPortrait`: a tall window's slots.
fn portrait_slots(ring: usize) -> Vec<(f32, f32)> {
    let pos = |name: &str| -> (f32, f32) {
        match name {
            "L_bottom" => (0.15, 0.785),
            "L_lower" => (0.15, 0.65),
            "L_upper" => (0.15, 0.345),
            "TL" => (0.15, 0.21),
            "TC" => (0.50, 0.075),
            "TR" => (0.85, 0.21),
            "R_upper" => (0.85, 0.345),
            "R_lower" => (0.85, 0.65),
            _ => (0.85, 0.785),
        }
    };
    let seq: &[&str] = match ring {
        0 => &[],
        1 => &["TC"],
        2 => &["TL", "TR"],
        3 => &["TL", "TC", "TR"],
        4 => &["L_upper", "TL", "TR", "R_upper"],
        5 => &["L_upper", "TL", "TC", "TR", "R_upper"],
        6 => &["L_lower", "L_upper", "TL", "TR", "R_upper", "R_lower"],
        7 => &["L_lower", "L_upper", "TL", "TC", "TR", "R_upper", "R_lower"],
        8 => &["L_bottom", "L_lower", "L_upper", "TL", "TR", "R_upper", "R_lower", "R_bottom"],
        _ => &["L_bottom", "L_lower", "L_upper", "TL", "TC", "TR", "R_upper", "R_lower", "R_bottom"],
    };
    seq.iter().map(|n| pos(n)).collect()
}

/// `fillCap`: how large the boxes may grow at all.
fn fill_cap(e: &Env, max_scale: f32) -> f32 {
    let base = 0.95;
    let opp = e.ring as f32;
    let t = ((opp - 1.0) / 5.0).clamp(0.0, 1.0);
    let count_cap = base + (max_scale - base) * t;
    let grow = (1.0 - t) * (((e.w * e.h).sqrt() - 760.0) / 700.0).max(0.0);
    let dense_shrink = t * ((e.w - 1024.0) / 4000.0).clamp(0.0, 0.15);
    (count_cap * (1.0 + grow) - dense_shrink).min(2.2)
}

/// Room the community row needs between the upper seats and the hero.
fn board_room(s: f32) -> f32 {
    0.72 * s * 124.0 + 28.0
}

/// `boxScale`: the largest scale at which the boxes lie without overlapping.
fn box_scale(e: &Env) -> f32 {
    if e.w <= 0.0 || e.h <= 0.0 {
        return 1.0;
    }
    let label = BET_LABEL_H + BET_LABEL_GAP;
    if e.wide {
        let feasible = |s: f32| -> bool {
            if e.ring < 2 {
                let visual_h = OPP_H * s;
                let bottom = e.h - 12.0 - SELF_H * s;
                let top_bottom = 4.0 + label * s + visual_h + s * 25.0;
                return bottom - top_bottom >= board_room(s);
            }
            let (pos, rx, ry, cy) = landscape_slots(e, s, true);
            if rx <= 0.0 || ry <= 0.0 {
                return false;
            }
            let bottom = e.h - 12.0 - SELF_H * s;
            let mut top_bottom = f32::MIN;
            for p in &pos {
                if (p.1 - cy) / ry >= 0.0 {
                    continue;
                }
                let vx = (p.0 - 0.5) / rx;
                let b = p.1 * e.h + OPP_H * s / 2.0 + if vx.abs() < 0.25 { s * 25.0 } else { 0.0 };
                top_bottom = top_bottom.max(b);
            }
            if top_bottom > f32::MIN && bottom - top_bottom < board_room(s) {
                return false;
            }
            let x_needed = s * (OPP_W + SIDE_BADGE_GAP) + 4.0;
            let y_needed = s * (OPP_H + label) + 4.0;
            pos.windows(2).all(|w| {
                let (a, b) = (w[0], w[1]);
                !((a.0 - b.0).abs() * e.w < x_needed && (a.1 - b.1).abs() * e.h < y_needed)
            })
        };
        bisect(feasible, 0.55, fill_cap(e, 1.9))
    } else {
        let pos = portrait_slots(e.ring);
        let feasible = |s: f32| -> bool {
            let visual_w = OPP_W * s;
            let visual_h = OPP_H * s;
            if visual_w > 2.0 * (0.15 * e.w - 4.0) || visual_h + label * s > 2.0 * (0.075 * e.h - 4.0) + label * s {
                return false;
            }
            if e.ring >= 8 && 0.215 * e.h - 26.0 - SELF_H * s - visual_h / 2.0 < 8.0 {
                return false;
            }
            let x_needed = s * OPP_W + 8.0;
            let y_needed = s * (OPP_H + label) + 8.0;
            pos.windows(2).all(|w| {
                let (a, b) = (w[0], w[1]);
                !((a.0 - b.0).abs() * e.w < x_needed && (a.1 - b.1).abs() * e.h < y_needed)
            })
        };
        bisect(feasible, 0.55, fill_cap(e, 1.85))
    }
}

fn bisect(feasible: impl Fn(f32) -> bool, lo: f32, hi: f32) -> f32 {
    let (mut lo, mut hi) = (lo, hi.max(lo));
    if !feasible(lo) {
        return lo;
    }
    if feasible(hi) {
        return hi;
    }
    for _ in 0..14 {
        let mid = (lo + hi) / 2.0;
        if feasible(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo.max(0.55)
}

/// The seats of a table, in ring order: clockwise on the screen from the
/// hero's left, which is the order of play.
pub fn ring_order(seats: &[u8], hero: u8) -> Vec<u8> {
    let mut others: Vec<u8> = seats.iter().copied().filter(|s| *s != hero).collect();
    others.sort_unstable();
    let split = others.iter().position(|s| *s > hero).unwrap_or(others.len());
    others.rotate_left(split);
    others
}

/// Lay out a table: `seats` every seat of the roster, the hero's included.
pub fn layout(zone: Rect, seats: &[u8], hero: u8) -> Seats {
    let ring = ring_order(seats, hero);
    let e = Env {
        w: zone.width().max(1.0),
        h: zone.height().max(1.0),
        wide: zone.width() >= zone.height(),
        ring: ring.len(),
    };
    let s = box_scale(&e);
    let opp = vec2(OPP_W, OPP_H) * s;
    let self_size = vec2(SELF_W, SELF_H) * s;
    let bottom_margin = if e.wide { 12.0 } else { 4.0 };
    let hero_rect = Rect::from_min_size(
        pos2(zone.center().x - self_size.x / 2.0, zone.bottom() - bottom_margin - self_size.y),
        self_size,
    );

    let slots = if e.wide { landscape_slots(&e, s, false).0 } else { portrait_slots(e.ring) };
    let count = ring.len();
    let mut others = Vec::with_capacity(count);
    for (i, (seat, slot)) in ring.iter().zip(slots.iter()).enumerate() {
        let flank = e.wide && (i == 0 || i + 1 == count) && slot.1 > 0.5;
        let (mut nudge_x, mut nudge_y) = (0.0, 0.0);
        if flank {
            nudge_y = OPP_H * s * 0.6;
            let dir = if slot.0 < 0.5 { -1.0 } else { 1.0 };
            let want = e.w / 2.0 + dir * (SELF_W * s / 2.0 + 40.0 * s + OPP_W * s / 2.0 + 18.0);
            let d = want - e.w * slot.0;
            nudge_x = if dir < 0.0 { d.min(0.0) } else { d.max(0.0) };
        } else if !e.wide {
            nudge_y = match (slot.0, slot.1) {
                (_, y) if y > 0.6 => 14.0,
                (x, y) if (x - 0.5).abs() > 0.01 && y < 0.4 => -4.0,
                _ => 0.0,
            };
        }
        let center = pos2(zone.left() + e.w * slot.0 + nudge_x, zone.top() + e.h * slot.1 + nudge_y);
        let side = if e.wide && (0.45..=0.55).contains(&slot.0) {
            Side::Split
        } else if slot.0 < 0.5 {
            if e.wide { Side::Left } else { Side::Right }
        } else if e.wide {
            Side::Right
        } else {
            Side::Left
        };
        others.push(SeatBox {
            seat: *seat,
            rect: Rect::from_center_size(center, opp),
            side,
            upper: slot.1 < 0.5,
        });
    }

    // `communityCenterY`: the boxes' vertical centre of gravity on a wide
    // table, the middle between the top row and the hero on a tall one.
    let self_top = hero_rect.top() - zone.top();
    let top_opp_bottom = {
        let top = slots.iter().map(|p| p.1).fold(0.13f32, f32::min);
        top * e.h + OPP_H * s / 2.0
    };
    let center_y = if e.wide && count > 0 {
        let mut sum = e.h - 12.0 - SELF_H * s / 2.0;
        for b in &others {
            sum += b.rect.center().y - zone.top();
        }
        sum / (count as f32 + 1.0)
    } else {
        (top_opp_bottom + self_top) / 2.0
    };
    let board_scale = if e.wide {
        let top_b = top_opp_bottom + 26.0 * s;
        let above = center_y - top_b - 6.0;
        let below = self_top - center_y - 6.0;
        let avail = above.min(below);
        let gap_fill = if avail > 0.0 { avail / 84.0 } else { 0.0 };
        let cap_w = 0.70 * e.w / 264.0;
        let cap = 1.8f32.min(s * 2.0).min(cap_w);
        let floor = s * 0.72;
        cap.min(floor.max(gap_fill)).max(0.55)
    } else {
        let v_half = 0.15 * e.h - OPP_H * s / 2.0 - 6.0;
        let max_v = if v_half > 0.0 { v_half / 62.0 } else { 0.55 };
        let max_screen = (e.w - 16.0).max(0.0) / 264.0;
        1.8f32.min(max_v).min(max_screen).max(0.55)
    };
    Seats {
        zone,
        wide: e.wide,
        scale: s,
        hero: hero_rect,
        hero_seat: hero,
        others,
        board_center: pos2(zone.center().x, zone.top() + center_y),
        board_scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_table(mut f: impl FnMut(&Seats, usize, Rect)) {
        for (w, h) in [(760.0, 420.0), (1000.0, 560.0), (1400.0, 820.0), (1920.0, 960.0), (600.0, 900.0)] {
            let zone = Rect::from_min_size(pos2(0.0, 76.0), vec2(w, h));
            for n in 2..=10usize {
                let seats: Vec<u8> = (0..n as u8).collect();
                for hero in [0u8, (n as u8) / 2, n as u8 - 1] {
                    let l = layout(zone, &seats, hero);
                    f(&l, n, zone);
                }
            }
        }
    }

    /// PokerTH's order of play: the seat after the hero is the first on the
    /// ring, bottom left, and the ring wraps past the highest seat.
    #[test]
    fn the_ring_runs_clockwise_from_the_heros_left() {
        assert_eq!(ring_order(&[0, 1, 2, 3, 4], 2), vec![3, 4, 0, 1]);
        assert_eq!(ring_order(&[0, 1], 0), vec![1]);
        assert_eq!(ring_order(&[0, 3, 5, 8], 8), vec![0, 3, 5]);
    }

    /// The hero's box is at the bottom in the middle, at every size.
    #[test]
    fn the_hero_is_at_the_bottom_in_the_middle() {
        every_table(|l, _, zone| {
            assert!((l.hero.center().x - zone.center().x).abs() < 0.5);
            for b in &l.others {
                assert!(b.rect.center().y < l.hero.bottom(), "seat {} below the hero", b.seat);
            }
        });
    }

    /// Every seat of the roster but the hero's has a box, once.
    #[test]
    fn every_seat_has_one_box() {
        every_table(|l, n, _| {
            assert_eq!(l.others.len(), n - 1);
            let mut seen: Vec<u8> = l.others.iter().map(|b| b.seat).collect();
            seen.push(l.hero_seat);
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len(), n);
        });
    }

    /// Neighbours on the ring do not overlap, their chip labels included, on
    /// a wide table where PokerTH's bisection holds them apart.
    #[test]
    fn neighbours_do_not_overlap_on_a_wide_table() {
        every_table(|l, n, zone| {
            if !l.wide || l.scale <= 0.551 {
                return;
            }
            for w in l.others.windows(2) {
                let a = w[0].rect.expand2(vec2(0.0, 0.0)).union(l.bet_label(w[0].seat, w[0].rect.width()).unwrap());
                let b = w[1].rect.union(l.bet_label(w[1].seat, w[1].rect.width()).unwrap());
                assert!(
                    !a.shrink(2.0).intersects(b.shrink(2.0)),
                    "{n} seats at {:?}: seats {} and {} overlap ({a:?} / {b:?}), scale {}",
                    zone.size(),
                    w[0].seat,
                    w[1].seat,
                    l.scale
                );
            }
        });
    }

    /// The top of every chip label is under the status bar: nothing hangs out
    /// of the zone at the top.
    #[test]
    fn nothing_hangs_over_the_top_of_the_zone() {
        every_table(|l, n, zone| {
            if !l.wide {
                return;
            }
            for b in &l.others {
                let label = l.bet_label(b.seat, b.rect.width()).unwrap();
                assert!(label.top() >= zone.top() - 1.0, "{n} seats at {:?}: seat {} label at {}", zone.size(), b.seat, label.top());
            }
        });
    }

    /// The community row is between the topmost box and the hero, and centred.
    #[test]
    fn the_board_is_between_the_seats_and_centred() {
        every_table(|l, _, zone| {
            let board = l.board();
            assert!((board[0].left() + board[4].right() - 2.0 * zone.center().x).abs() < 1.0);
            assert!(board[2].bottom() <= l.hero.top() + 1.0, "the board runs into the hero");
            assert!(board[0].width() > 0.0 && (board[0].height() / board[0].width() - 64.0 / 46.0).abs() < 0.01);
        });
    }

    #[test]
    fn the_scale_stays_in_pokerths_range() {
        every_table(|l, _, _| {
            assert!(l.scale >= 0.55 && l.scale <= 2.2, "{}", l.scale);
            assert!(l.board_scale >= 0.55 && l.board_scale <= 1.8, "{}", l.board_scale);
        });
    }
}
