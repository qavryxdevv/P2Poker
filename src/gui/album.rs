//! `D-068`: the album -- a window of its own, as a table has, opened from the
//! lobby's card about the player -- and the pieces the lobby borrows from it:
//! the level's chip, the stars, the meter.
//!
//! **Cards like the ice-hockey cards of a childhood**: a frame by rarity
//! (bronze, silver, gold, holo), a picture, a name, a number -- `♦9 · 21/52` --
//! and a bar of how far a locked one is. A locked card lies face down with how
//! it is earned written on its back; a hidden one says `?`. A click turns an
//! earned card over to the moment it remembers.
//!
//! **Every picture is drawn here**, by the painter, from chips, cards and felt:
//! none is somebody else's image, so none needs a licence. One picture per
//! card, chosen for what the card is about (`catalog::Art`).

use eframe::egui::{self, pos2, vec2, Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, StrokeKind};

use super::rewards::{AlbumView, CardView, LevelView, QuestLine, YouRewards};
use super::table::style::{self, Weight};
use super::theme;
use crate::app::rewards::catalog::{Art, Rarity, Suit};

/// The album's card, in points, before the window's zoom.
pub const CARD: egui::Vec2 = vec2(150.0, 214.0);
pub const ALBUM_MIN_WINDOW: egui::Vec2 = vec2(520.0, 420.0);

const fn rgb(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

const BRONZE: [Color32; 3] = [rgb(0xD9A066), rgb(0xA8683A), rgb(0x5E3A1E)];
const SILVER: [Color32; 3] = [rgb(0xF1F4F7), rgb(0xAEB7C0), rgb(0x5F6A75)];
const GOLDS: [Color32; 3] = [rgb(0xFFE89A), rgb(0xD9AE3C), rgb(0x7A5A12)];
const HOLO: [Color32; 5] = [rgb(0x7FE7FF), rgb(0xB79BFF), rgb(0xFF9BD2), rgb(0xFFE38A), rgb(0x8AFFC1)];
const CHIP_COLOURS: [(&str, Color32, Color32); 6] = [
    ("White", rgb(0xECEFF2), rgb(0x2B3A47)),
    ("Red", rgb(0xC0392B), rgb(0xFFF5D6)),
    ("Green", rgb(0x1E8449), rgb(0xFFF5D6)),
    ("Black", rgb(0x20262D), rgb(0xE3C800)),
    ("Purple", rgb(0x6C3483), rgb(0xFFF5D6)),
    ("Gold", rgb(0xD4AF37), rgb(0x1A1400)),
];

/// A suit's ink: clubs and spades light on the dark card, diamonds and hearts
/// warm. The jokers are gold.
const fn suit_ink(suit: Suit) -> Color32 {
    match suit {
        Suit::Clubs => rgb(0x7FD4A8),
        Suit::Diamonds => rgb(0x6FB8FF),
        Suit::Hearts => rgb(0xFF8A8A),
        Suit::Spades => rgb(0xD5DEE6),
        Suit::Joker => rgb(0xFFD54A),
    }
}

#[derive(Default)]
pub struct AlbumUi {
    pub page: usize,
    /// The earned card turned over, by id.
    pub flipped: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlbumAction {
    None,
    Swap(usize),
    PickWeekly(usize),
    Showcase(String),
}

// ---- pieces the lobby borrows ------------------------------------------------

/// The level as a casino chip with the number on it.
pub fn chip(p: &egui::Painter, c: Pos2, r: f32, level: &LevelView) {
    let (_, body, ink) = CHIP_COLOURS.iter().find(|(n, _, _)| *n == level.chip).copied().unwrap_or(CHIP_COLOURS[0]);
    p.circle_filled(c + vec2(0.0, r * 0.08), r, Color32::from_black_alpha(90));
    p.circle_filled(c, r, body);
    // Eight inserts round the edge, as a clay chip has.
    for i in 0..8 {
        let a = std::f32::consts::TAU * (i as f32) / 8.0;
        let (s, co) = a.sin_cos();
        let mid = c + vec2(co, s) * r * 0.86;
        let t = vec2(-s, co) * r * 0.16;
        let n = vec2(co, s) * r * 0.13;
        p.add(Shape::convex_polygon(vec![mid - t - n, mid + t - n, mid + t + n, mid - t + n], ink, Stroke::NONE));
    }
    p.circle_stroke(c, r * 0.70, Stroke::new((r * 0.06).max(1.0), ink));
    p.circle_filled(c, r * 0.62, style::mix(body, Color32::BLACK, 0.18));
    let digits = level.level.to_string();
    let size = r * if digits.len() >= 3 { 0.62 } else { 0.82 };
    style::text(p, c, Align2::CENTER_CENTER, &digits, size, Weight::Bold, ink);
}

/// A five-pointed star, lit or an outline.
pub fn star(p: &egui::Painter, c: Pos2, r: f32, lit: bool, colour: Color32) {
    let pts: Vec<Pos2> = (0..10)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * (i as f32) / 5.0;
            c + vec2(a.cos(), a.sin()) * if i % 2 == 0 { r } else { r * 0.42 }
        })
        .collect();
    if lit {
        // A star is not convex: five triangles about the centre and a pentagon.
        for i in 0..5 {
            let tip = pts[i * 2];
            let (l, rr) = (pts[(i * 2 + 9) % 10], pts[(i * 2 + 1) % 10]);
            p.add(Shape::convex_polygon(vec![tip, rr, c, l], colour, Stroke::NONE));
        }
    } else {
        p.add(Shape::closed_line(pts, Stroke::new(1.2, colour.gamma_multiply(0.7))));
    }
}

/// A bar of progress: a track and what is filled of it.
pub fn bar(p: &egui::Painter, rect: Rect, have: u64, need: u64, colour: Color32) {
    let r = rect.height() / 2.0;
    p.rect_filled(rect, r, Color32::from_black_alpha(120));
    let f = if need == 0 { 0.0 } else { (have as f32 / need as f32).clamp(0.0, 1.0) };
    if f > 0.0 {
        let w = (rect.width() * f).max(rect.height());
        p.rect_filled(Rect::from_min_size(rect.min, vec2(w, rect.height())), r, colour);
    }
}

/// The meter's colour: full is the good green, recovering is gold, low is
/// amber. **Never red**: a penalty here is a way back, not an alarm.
pub const fn meter_colour(meter: u32) -> Color32 {
    match meter {
        90..=u32::MAX => theme::OK,
        70..=89 => theme::GOLD_EDGE,
        _ => theme::WARN,
    }
}

// ---- a card --------------------------------------------------------------------

fn frame_stops(rarity: Rarity, t: f32) -> Vec<(f32, Color32)> {
    match rarity {
        Rarity::Bronze => vec![(0.0, BRONZE[0]), (0.5, BRONZE[1]), (1.0, BRONZE[2])],
        Rarity::Silver => vec![(0.0, SILVER[0]), (0.5, SILVER[1]), (1.0, SILVER[2])],
        Rarity::Gold => vec![(0.0, GOLDS[0]), (0.45, GOLDS[1]), (1.0, GOLDS[2])],
        // The holo frame's bands drift slowly; a still card is still a holo.
        Rarity::Holo => (0..=4)
            .map(|i| {
                let k = (i as f32 + t) % 5.0;
                (i as f32 / 4.0, style::mix(HOLO[k as usize % 5], HOLO[(k as usize + 1) % 5], k.fract()))
            })
            .collect(),
    }
}

/// One card of the album at `rect`. Returns whether it was clicked.
pub fn card(ui: &mut egui::Ui, rect: Rect, v: &CardView, flipped: bool, time: f32) -> egui::Response {
    let resp = ui.interact(rect, ui.id().with(("album-card", v.card.id)), Sense::click());
    let p = ui.painter_at(rect.expand(6.0));
    let s = rect.width() / CARD.x;
    let earned = v.earned.is_some();
    let rarity = v.card.rarity();
    let lift = if resp.hovered() && earned { -2.0 * s } else { 0.0 };
    let rect = rect.translate(vec2(0.0, lift));
    style::shadow(&p, rect, 10.0 * s, 3.0 * s, 10.0 * s, Color32::from_black_alpha(140));
    if earned {
        style::gradient_rect(&p, rect, 10.0 * s, &frame_stops(rarity, if rarity == Rarity::Holo { time * 0.35 } else { 0.0 }));
    } else {
        p.rect_filled(rect, 10.0 * s, theme::PANEL_LIGHT);
    }
    let inner = rect.shrink(5.0 * s);
    let ink = suit_ink(v.card.suit);

    if !earned {
        // Face down: the back, how it is earned, and how far it is.
        style::gradient_rect(&p, inner, 7.0 * s, &[(0.0, rgb(0x1B2A35)), (1.0, rgb(0x0F1820))]);
        let mut y = inner.top() + 6.0 * s;
        while y < inner.bottom() - 4.0 * s {
            p.line_segment([pos2(inner.left() + 6.0 * s, y), pos2(inner.right() - 6.0 * s, y)], Stroke::new(1.0, Color32::from_white_alpha(6)));
            y += 7.0 * s;
        }
        let head = pos2(inner.center().x, inner.top() + 30.0 * s);
        if v.card.hidden {
            style::text(&p, head + vec2(0.0, 40.0 * s), Align2::CENTER_CENTER, "?", 64.0 * s, Weight::Bold, ink.gamma_multiply(0.55));
            style::text(&p, pos2(inner.center().x, inner.bottom() - 22.0 * s), Align2::CENTER_CENTER, "Found by playing", 11.0 * s, Weight::Medium, theme::TEXT_DIM);
            return resp;
        }
        style::text(&p, head, Align2::CENTER_CENTER, &v.card.label(), 30.0 * s, Weight::Bold, ink.gamma_multiply(0.75));
        wrapped(&p, Rect::from_min_max(pos2(inner.left() + 8.0 * s, head.y + 22.0 * s), pos2(inner.right() - 8.0 * s, inner.bottom() - 34.0 * s)), v.card.how, 12.0 * s, theme::TEXT);
        let b = Rect::from_min_size(pos2(inner.left() + 10.0 * s, inner.bottom() - 24.0 * s), vec2(inner.width() - 20.0 * s, 7.0 * s));
        bar(&p, b, v.have, v.need, ink.gamma_multiply(0.85));
        style::text(&p, pos2(inner.center().x, b.bottom() + 8.0 * s), Align2::CENTER_CENTER, &format!("{} / {}", v.have, v.need), 10.0 * s, Weight::Medium, theme::TEXT_DIM);
        return resp;
    }

    style::gradient_rect(&p, inner, 7.0 * s, &[(0.0, rgb(0x16222B)), (1.0, rgb(0x0B1218))]);
    if flipped {
        // The back of an earned card: the condition and the moment.
        style::text(&p, pos2(inner.center().x, inner.top() + 20.0 * s), Align2::CENTER_CENTER, &v.number, 12.0 * s, Weight::DemiBold, ink);
        let body = Rect::from_min_max(pos2(inner.left() + 8.0 * s, inner.top() + 36.0 * s), pos2(inner.right() - 8.0 * s, inner.bottom() - 8.0 * s));
        let used = wrapped(&p, body, v.card.how, 12.0 * s, theme::TEXT);
        let when = v.earned.clone().unwrap_or_default();
        wrapped(&p, Rect::from_min_max(pos2(body.left(), used + 12.0 * s), body.max), &when, 11.0 * s, theme::ON_FELT_DIM);
        return resp;
    }
    // The corner index, as on a playing card.
    style::text(&p, pos2(inner.left() + 8.0 * s, inner.top() + 6.0 * s), Align2::LEFT_TOP, &v.card.label(), 15.0 * s, Weight::Bold, ink);
    style::text(&p, pos2(inner.right() - 8.0 * s, inner.top() + 8.0 * s), Align2::RIGHT_TOP, rarity.name(), 9.5 * s, Weight::DemiBold, theme::TEXT_DIM);
    let window = Rect::from_min_max(pos2(inner.left() + 7.0 * s, inner.top() + 28.0 * s), pos2(inner.right() - 7.0 * s, inner.top() + 138.0 * s));
    picture(&p, window, v.card.art, v.card.suit);
    p.rect_stroke(window, 5.0 * s, Stroke::new(1.0, ink.gamma_multiply(0.45)), StrokeKind::Inside);
    // The name's banner, and the number under it.
    let banner = Rect::from_min_max(pos2(inner.left(), window.bottom() + 6.0 * s), pos2(inner.right(), window.bottom() + 40.0 * s));
    p.rect_filled(banner, 0.0, Color32::from_black_alpha(90));
    let name = style::elided(&p, v.card.title, 12.5 * s, Weight::Bold, banner.width() - 10.0 * s);
    style::text(&p, banner.center() - vec2(0.0, 7.0 * s), Align2::CENTER_CENTER, &name, 12.5 * s, Weight::Bold, theme::TEXT);
    style::text(&p, banner.center() + vec2(0.0, 9.0 * s), Align2::CENTER_CENTER, &v.number, 10.0 * s, Weight::Medium, ink);
    style::text(&p, pos2(inner.center().x, inner.bottom() - 9.0 * s), Align2::CENTER_CENTER, "click to turn", 8.5 * s, Weight::Regular, theme::TEXT_DIM.gamma_multiply(0.7));
    resp
}

/// Text wrapped into a rectangle, centred; returns the bottom of what was drawn.
fn wrapped(p: &egui::Painter, rect: Rect, text: &str, size: f32, colour: Color32) -> f32 {
    let galley = p.layout(text.to_string(), style::font(size, Weight::Medium), colour, rect.width());
    let h = galley.size().y;
    let x = rect.center().x - galley.size().x / 2.0;
    p.galley(pos2(x, rect.top()), galley, colour);
    rect.top() + h
}

// ---- the pictures ---------------------------------------------------------------

fn ellipse(c: Pos2, rx: f32, ry: f32, n: usize) -> Vec<Pos2> {
    (0..n).map(|i| { let a = std::f32::consts::TAU * i as f32 / n as f32; c + vec2(a.cos() * rx, a.sin() * ry) }).collect()
}

/// A chip seen from the side, `w` wide.
fn side_chip(p: &egui::Painter, c: Pos2, w: f32, body: Color32, ink: Color32) {
    let h = w * 0.24;
    let r = Rect::from_center_size(c, vec2(w, h));
    p.rect_filled(r, h * 0.45, body);
    for i in 0..3 {
        let x = r.left() + w * (0.2 + 0.3 * i as f32);
        p.rect_filled(Rect::from_center_size(pos2(x, c.y), vec2(w * 0.12, h * 0.8)), 1.0, ink);
    }
    p.rect_stroke(r, h * 0.45, Stroke::new(1.0, Color32::from_black_alpha(110)), StrokeKind::Inside);
}

/// A small playing card, face up: `(rank 2..=14, suit 0..=3)`.
fn mini_card(p: &egui::Painter, rect: Rect, card: (u8, u8)) {
    let rr = rect.width() * 0.14;
    p.rect_filled(rect.translate(vec2(0.0, 1.5)), rr, Color32::from_black_alpha(90));
    p.rect_filled(rect, rr, theme::CARD_FACE);
    p.rect_stroke(rect, rr, Stroke::new(1.0, rgb(0x9AA7B2)), StrokeKind::Inside);
    let rank = match card.0 { 14 => "A".to_string(), 13 => "K".to_string(), 12 => "Q".to_string(), 11 => "J".to_string(), 10 => "10".to_string(), n => n.to_string() };
    let (sym, ink) = match card.1 { 0 => ("\u{2663}", rgb(0x1B2B22)), 1 => ("\u{2666}", rgb(0xC0392B)), 2 => ("\u{2665}", rgb(0xC0392B)), _ => ("\u{2660}", rgb(0x1B2329)) };
    // The index in the corner, as on a real card: it stays readable where the
    // next card of a fan lies over this one.
    let x = rect.left() + rect.width() * 0.30;
    let size = rect.width() * if rank.len() > 1 { 0.40 } else { 0.50 };
    style::text(p, pos2(x, rect.top() + rect.height() * 0.24), Align2::CENTER_CENTER, &rank, size, Weight::Bold, ink);
    style::text(p, pos2(x, rect.top() + rect.height() * 0.56), Align2::CENTER_CENTER, sym, rect.width() * 0.46, Weight::Regular, ink);
}

fn card_back(p: &egui::Painter, rect: Rect) {
    let rr = rect.width() * 0.14;
    p.rect_filled(rect, rr, theme::CARD_BACK);
    p.rect_stroke(rect.shrink(rect.width() * 0.1), rr * 0.6, Stroke::new(1.0, theme::CARD_BACK_DARK), StrokeKind::Inside);
    p.rect_stroke(rect, rr, Stroke::new(1.0, Color32::from_black_alpha(120)), StrokeKind::Inside);
}

fn person(p: &egui::Painter, c: Pos2, r: f32, colour: Color32) {
    let body = Rect::from_center_size(c + vec2(0.0, r * 1.55), vec2(r * 2.6, r * 1.9));
    p.rect_filled(body, r * 0.95, colour);
    p.circle_filled(c, r, colour);
    p.circle_stroke(c, r, Stroke::new(1.0, Color32::from_black_alpha(90)));
}

fn count_label(n: u32) -> String {
    if n >= 1_000 { format!("{}k", n / 1_000) } else { n.to_string() }
}

/// The picture in a card's window.
pub fn picture(p: &egui::Painter, r: Rect, art: Art, suit: Suit) {
    let ink = suit_ink(suit);
    // Felt, tinted by the suit, with the suit's pip faint in the corner.
    style::gradient_rect(p, r, 5.0, &[(0.0, style::tint(theme::FELT_CENTRE, ink, 0.10)), (1.0, style::tint(theme::FELT_EDGE, ink, 0.06))]);
    let u = r.width() / 136.0;
    style::text(p, r.right_bottom() - vec2(16.0 * u, 18.0 * u), Align2::CENTER_CENTER, suit.symbol(), 30.0 * u, Weight::Regular, Color32::from_white_alpha(16));
    let at = |fx: f32, fy: f32| pos2(r.left() + r.width() * fx, r.top() + r.height() * fy);
    let gold = theme::GOLD_EDGE;
    match art {
        Art::Chips(n) => {
            let stacks = usize::from(n).div_ceil(8).clamp(1, 5);
            let bodies = [rgb(0xC0392B), rgb(0x1E8449), rgb(0x20262D), rgb(0x6C3483), rgb(0xD4AF37)];
            // A few chips are drawn large, in the middle; many, as stacks.
            let w = (r.width() * 0.8 / stacks as f32).min(if n <= 5 { 56.0 * u } else { 34.0 * u });
            let tallest = usize::from(n).min(8) as f32 * w * 0.25;
            let base = (r.center().y + tallest / 2.0).min(r.bottom() - 14.0 * u);
            let mut left = usize::from(n);
            for i in 0..stacks {
                let here = left.min(8).max(1);
                left = left.saturating_sub(8);
                let x = r.center().x + (i as f32 - (stacks as f32 - 1.0) / 2.0) * (w + 3.0 * u);
                for k in 0..here {
                    side_chip(p, pos2(x, base - k as f32 * w * 0.25), w, bodies[i % 5], theme::CARD_FACE);
                }
            }
        }
        Art::Deck(n) => {
            for k in (0..6).rev() {
                card_back(p, Rect::from_center_size(at(0.42, 0.52) + vec2(k as f32 * 2.2 * u, -(k as f32) * 2.0 * u), vec2(44.0 * u, 62.0 * u)));
            }
            let plaque = Rect::from_center_size(at(0.74, 0.74), vec2(46.0 * u, 22.0 * u));
            p.rect_filled(plaque, 5.0 * u, Color32::from_black_alpha(170));
            p.rect_stroke(plaque, 5.0 * u, Stroke::new(1.0, gold), StrokeKind::Inside);
            style::text(p, plaque.center(), Align2::CENTER_CENTER, &count_label(u32::from(n)), 13.0 * u, Weight::Bold, theme::TEXT);
        }
        Art::Hand(cards) => {
            let (w, h) = (30.0 * u, 42.0 * u);
            for (i, c) in cards.iter().enumerate() {
                let k = i as f32 - 2.0;
                let centre = at(0.5, 0.54) + vec2(k * 22.0 * u, k * k * 3.2 * u);
                mini_card(p, Rect::from_center_size(centre, vec2(w, h)), *c);
            }
        }
        Art::Hole { cards, cracked } => {
            let (w, h) = (46.0 * u, 64.0 * u);
            mini_card(p, Rect::from_center_size(at(0.38, 0.52), vec2(w, h)), cards[0]);
            mini_card(p, Rect::from_center_size(at(0.62, 0.56), vec2(w, h)), cards[1]);
            if cracked {
                let pts = [at(0.16, 0.30), at(0.36, 0.46), at(0.30, 0.58), at(0.56, 0.62), at(0.52, 0.76), at(0.84, 0.88)];
                p.add(Shape::line(pts.to_vec(), Stroke::new(3.0 * u, rgb(0x10161B))));
                p.add(Shape::line(pts.to_vec(), Stroke::new(1.0 * u, theme::WARN)));
            }
        }
        Art::Table(n) => {
            let c = at(0.5, 0.54);
            p.add(Shape::convex_polygon(ellipse(c, 52.0 * u, 30.0 * u, 40), theme::RAIL_SIDE, Stroke::new(1.0, theme::RAIL_OUTER_EDGE)));
            p.add(Shape::convex_polygon(ellipse(c, 45.0 * u, 23.0 * u, 40), theme::FELT_CENTRE, Stroke::new(1.0, theme::FELT_KEYLINE)));
            for (i, seat) in ellipse(c, 58.0 * u, 37.0 * u, 10).into_iter().enumerate() {
                let taken = i < usize::from(n);
                p.circle_filled(seat, 6.0 * u, if taken { gold } else { Color32::from_black_alpha(130) });
                p.circle_stroke(seat, 6.0 * u, Stroke::new(1.0, if taken { theme::INK_ON_GOLD } else { theme::TEXT_DIM.gamma_multiply(0.5) }));
            }
        }
        Art::Calendar(n) => {
            let page = Rect::from_center_size(at(0.5, 0.54), vec2(104.0 * u, 80.0 * u));
            p.rect_filled(page, 5.0 * u, theme::CARD_FACE);
            p.rect_filled(Rect::from_min_size(page.min, vec2(page.width(), 14.0 * u)), 5.0 * u, rgb(0xC0392B));
            let rows = if n > 14 { 3 } else { 2 };
            for i in 0..(7 * rows) {
                let cell = pos2(page.left() + 12.0 * u + (i % 7) as f32 * 13.4 * u, page.top() + 28.0 * u + (i / 7) as f32 * (48.0 / rows as f32) * u);
                if i < u32::from(n) {
                    p.circle_filled(cell, 4.6 * u, rgb(0x1E8449));
                } else {
                    p.circle_stroke(cell, 4.2 * u, Stroke::new(1.0, rgb(0x9AA7B2)));
                }
            }
        }
        Art::Chain(n) => {
            let n = usize::from(n).clamp(2, 9);
            let per = n.div_ceil(2).max(3);
            for i in 0..n {
                let (row, col) = (i / per, i % per);
                let in_row = if row == 0 { per.min(n) } else { n - per };
                let c = at(0.5, if n > per { 0.38 + 0.30 * row as f32 } else { 0.54 }) + vec2((col as f32 - (in_row as f32 - 1.0) / 2.0) * 19.0 * u, 0.0);
                let (rx, ry) = if col % 2 == 0 { (13.0 * u, 8.0 * u) } else { (8.0 * u, 13.0 * u) };
                p.add(Shape::closed_line(ellipse(c, rx, ry, 28), Stroke::new(3.2 * u, if i + 1 == n { gold } else { rgb(0xC9D3DB) })));
            }
        }
        Art::People(n) => {
            let n = usize::from(n).clamp(1, 9);
            let tints = [rgb(0x7FD4FF), rgb(0xFFCB57), rgb(0x35C48C), rgb(0xFF8A8A), rgb(0xB79BFF)];
            let per = if n > 5 { n.div_ceil(2) } else { n };
            for i in (0..n).rev() {
                let (row, col) = (i / per, i % per);
                let in_row = if row == 0 { per } else { n - per };
                let c = at(0.5, if n > per { 0.30 + 0.34 * row as f32 } else { 0.42 }) + vec2((col as f32 - (in_row as f32 - 1.0) / 2.0) * 23.0 * u, 0.0);
                person(p, c, 8.0 * u, tints[i % 5]);
            }
        }
        Art::Trophy(n) => {
            let c = at(0.5, 0.40);
            for side in [-1.0f32, 1.0] {
                p.circle_stroke(c + vec2(side * 25.0 * u, -2.0 * u), 11.0 * u, Stroke::new(3.0 * u, GOLDS[1]));
            }
            let bowl = vec![c + vec2(-24.0 * u, -22.0 * u), c + vec2(24.0 * u, -22.0 * u), c + vec2(15.0 * u, 12.0 * u), c + vec2(0.0, 20.0 * u), c + vec2(-15.0 * u, 12.0 * u)];
            p.add(Shape::convex_polygon(bowl, GOLDS[1], Stroke::new(1.0, GOLDS[2])));
            p.rect_filled(Rect::from_center_size(c + vec2(-10.0 * u, -6.0 * u), vec2(5.0 * u, 22.0 * u)), 2.0, GOLDS[0].gamma_multiply(0.8));
            p.rect_filled(Rect::from_center_size(c + vec2(0.0, 26.0 * u), vec2(8.0 * u, 14.0 * u)), 1.0, GOLDS[2]);
            let plinth = Rect::from_center_size(c + vec2(0.0, 42.0 * u), vec2(52.0 * u, 18.0 * u));
            p.rect_filled(plinth, 3.0 * u, rgb(0x20262D));
            p.rect_stroke(plinth, 3.0 * u, Stroke::new(1.0, GOLDS[1]), StrokeKind::Inside);
            style::text(p, plinth.center(), Align2::CENTER_CENTER, &format!("\u{00d7}{n}"), 12.0 * u, Weight::Bold, GOLDS[0]);
        }
        Art::Podium(n) => {
            for (i, (fx, h)) in [(0.28f32, 30.0f32), (0.5, 46.0), (0.72, 22.0)].into_iter().enumerate() {
                let lit = [1usize, 0, 2][i] < usize::from(n);
                let step = Rect::from_min_max(pos2(r.left() + r.width() * fx - 17.0 * u, r.bottom() - 12.0 * u - h * u), pos2(r.left() + r.width() * fx + 17.0 * u, r.bottom() - 12.0 * u));
                p.rect_filled(step, 2.0 * u, if lit { GOLDS[1] } else { rgb(0x3A4853) });
                p.rect_stroke(step, 2.0 * u, Stroke::new(1.0, Color32::from_black_alpha(120)), StrokeKind::Inside);
                style::text(p, step.center(), Align2::CENTER_CENTER, ["2", "1", "3"][i], 13.0 * u, Weight::Bold, if lit { theme::INK_ON_GOLD } else { theme::TEXT_DIM });
                if lit {
                    person(p, pos2(step.center().x, step.top() - 22.0 * u), 6.0 * u, theme::CARD_FACE);
                }
            }
        }
        Art::Gauge => {
            let c = at(0.5, 0.74);
            let arc = |from: f32, to: f32, colour: Color32| {
                let pts: Vec<Pos2> = (0..=24).map(|i| { let a = std::f32::consts::PI * (1.0 + (from + (to - from) * i as f32 / 24.0)); c + vec2(a.cos(), a.sin()) * 44.0 * u }).collect();
                p.add(Shape::line(pts, Stroke::new(9.0 * u, colour)));
            };
            arc(0.0, 0.5, theme::WARN);
            arc(0.5, 0.8, gold);
            arc(0.8, 1.0, theme::OK);
            let a = std::f32::consts::PI * 1.94;
            p.line_segment([c, c + vec2(a.cos(), a.sin()) * 36.0 * u], Stroke::new(3.0 * u, theme::CARD_FACE));
            p.circle_filled(c, 6.0 * u, theme::CARD_FACE);
            style::text(p, c + vec2(0.0, 16.0 * u), Align2::CENTER_CENTER, "100", 13.0 * u, Weight::Bold, theme::OK);
        }
        Art::Hourglass => {
            let c = at(0.5, 0.54);
            let (w, h) = (26.0 * u, 36.0 * u);
            p.add(Shape::convex_polygon(vec![c + vec2(-w, -h), c + vec2(w, -h), c], rgb(0xBFE3F2).gamma_multiply(0.35), Stroke::new(1.5 * u, theme::CARD_FACE)));
            p.add(Shape::convex_polygon(vec![c + vec2(-w, h), c + vec2(w, h), c], rgb(0xBFE3F2).gamma_multiply(0.35), Stroke::new(1.5 * u, theme::CARD_FACE)));
            p.add(Shape::convex_polygon(vec![c + vec2(-w * 0.78, h - 2.0 * u), c + vec2(w * 0.78, h - 2.0 * u), c + vec2(0.0, h * 0.35)], GOLDS[1], Stroke::NONE));
            p.add(Shape::convex_polygon(vec![c + vec2(-w * 0.30, -h * 0.40), c + vec2(w * 0.30, -h * 0.40), c], GOLDS[1], Stroke::NONE));
            for dy in [-h - 3.0 * u, h + 3.0 * u] {
                p.rect_filled(Rect::from_center_size(c + vec2(0.0, dy), vec2(w * 2.4, 6.0 * u)), 2.0 * u, theme::RAIL_SIDE);
            }
        }
        Art::ChipAndChair => {
            let seat = Rect::from_center_size(at(0.40, 0.60), vec2(42.0 * u, 8.0 * u));
            let wood = theme::RAIL_BOTTOM;
            p.rect_filled(Rect::from_min_max(pos2(seat.left(), seat.top() - 44.0 * u), pos2(seat.left() + 7.0 * u, seat.bottom())), 2.0, wood);
            p.rect_filled(Rect::from_min_max(pos2(seat.left(), seat.top() - 44.0 * u), pos2(seat.left() + 30.0 * u, seat.top() - 34.0 * u)), 2.0, wood);
            p.rect_filled(seat, 2.0, wood);
            for x in [seat.left() + 2.0 * u, seat.right() - 8.0 * u] {
                p.rect_filled(Rect::from_min_size(pos2(x, seat.bottom()), vec2(6.0 * u, 26.0 * u)), 1.0, theme::RAIL_SIDE);
            }
            side_chip(p, at(0.76, 0.84), 30.0 * u, rgb(0xC0392B), theme::CARD_FACE);
        }
        Art::Crown => {
            for (i, rank) in [10u8, 11, 12, 13, 14].into_iter().enumerate() {
                mini_card(p, Rect::from_center_size(at(0.5, 0.70) + vec2((i as f32 - 2.0) * 20.0 * u, 0.0), vec2(24.0 * u, 34.0 * u)), (rank, 2));
            }
            let c = at(0.5, 0.26);
            let pts = vec![c + vec2(-26.0 * u, 14.0 * u), c + vec2(-30.0 * u, -10.0 * u), c + vec2(-14.0 * u, 2.0 * u), c + vec2(0.0, -16.0 * u), c + vec2(14.0 * u, 2.0 * u), c + vec2(30.0 * u, -10.0 * u), c + vec2(26.0 * u, 14.0 * u)];
            for w in pts.windows(2).take(6) {
                p.add(Shape::convex_polygon(vec![w[0], w[1], pos2(w[1].x, c.y + 14.0 * u), pos2(w[0].x, c.y + 14.0 * u)], GOLDS[1], Stroke::NONE));
            }
            for tip in [pts[1], pts[3], pts[5]] {
                p.circle_filled(tip, 3.4 * u, GOLDS[0]);
            }
        }
        Art::SplitChip => {
            for (dx, from) in [(-9.0f32, 0.25f32), (9.0, 0.75)] {
                let c = at(0.5, 0.54) + vec2(dx * u, 0.0);
                let half: Vec<Pos2> = (0..=20).map(|i| { let a = std::f32::consts::TAU * (from + i as f32 / 40.0); c + vec2(a.cos(), a.sin()) * 34.0 * u }).collect();
                p.add(Shape::convex_polygon(half, rgb(0xC0392B), Stroke::new(1.5 * u, theme::CARD_FACE)));
            }
            style::text(p, at(0.5, 0.54), Align2::CENTER_CENTER, "\u{00bd}", 22.0 * u, Weight::Bold, theme::CARD_FACE);
        }
        Art::Emblem(rank) => {
            let c = at(0.5, 0.52);
            for side in [-1.0f32, 1.0] {
                for k in 0..7 {
                    let a = std::f32::consts::FRAC_PI_2 + side * (0.35 + k as f32 * 0.36);
                    let leaf = c + vec2(a.cos(), a.sin()) * 40.0 * u;
                    p.add(Shape::convex_polygon(ellipse(leaf, 7.0 * u, 3.6 * u, 12), rgb(0x2E8A55), Stroke::NONE));
                }
            }
            for i in 0..3 {
                star(p, c + vec2((i as f32 - 1.0) * 17.0 * u, -8.0 * u), 8.0 * u, true, GOLDS[0]);
            }
            let name = crate::app::rewards::catalog::RANKS[usize::from(rank).min(9)];
            style::text(p, c + vec2(0.0, 14.0 * u), Align2::CENTER_CENTER, name, 11.0 * u, Weight::Bold, theme::TEXT);
        }
    }
}

// ---- the album's window -----------------------------------------------------------

fn quest_row(ui: &mut egui::Ui, q: &QuestLine, button: Option<&str>) -> bool {
    let mut pressed = false;
    egui::Frame::new().fill(theme::FIELD).stroke(Stroke::new(1.0, theme::LINE)).corner_radius(8.0).inner_margin(10.0).show(ui, |ui| {
        // Clear of the scroll bar on the right.
        ui.set_width(ui.available_width() - 10.0);
        ui.horizontal_wrapped(|ui| {
            let colour = if q.done { theme::OK } else { theme::TEXT };
            ui.add(egui::Label::new(egui::RichText::new(&q.text).color(colour).size(15.0)).wrap());
            ui.label(egui::RichText::new(format!("+{} XP", q.xp)).color(theme::GOLD_EDGE).size(13.0));
            if let Some(label) = button {
                if ui.add(egui::Button::new(egui::RichText::new(label).size(13.0))).clicked() {
                    pressed = true;
                }
            }
        });
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::hover());
        bar(ui.painter(), rect, q.have, q.need, if q.done { theme::OK } else { theme::ACCENT });
        ui.label(egui::RichText::new(if q.done { "Done".to_string() } else { format!("{} / {}", q.have, q.need) }).color(theme::TEXT_DIM).size(12.0));
    });
    ui.add_space(6.0);
    pressed
}

/// The head of the album: the chip, the level's bar, the meter, the season.
pub fn head(ui: &mut egui::Ui, you: &YouRewards) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(58.0, 58.0), Sense::hover());
        chip(ui.painter(), rect.center(), 27.0, &you.level);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.label(egui::RichText::new(format!("Level {} \u{00b7} {} chip", you.level.level, you.level.chip)).color(theme::TEXT).size(17.0).strong());
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(320.0), 9.0), Sense::hover());
            bar(ui.painter(), rect, you.level.into, you.level.span, theme::GOLD_EDGE);
            ui.label(egui::RichText::new(format!("{} / {} XP to level {}", you.level.into, you.level.span, you.level.level + 1)).color(theme::TEXT_DIM).size(12.5));
        });
    });
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(format!("Table manners {}", you.manners)).color(meter_colour(you.manners)).size(14.0).strong());
        ui.label(egui::RichText::new(you.manners_words).color(theme::TEXT_DIM).size(13.0));
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(format!("Season: {}", you.season.rank)).color(theme::TEXT).size(14.0).strong());
        let (rect, _) = ui.allocate_exact_size(vec2(54.0, 16.0), Sense::hover());
        for i in 0..3 {
            star(ui.painter(), pos2(rect.left() + 9.0 + i as f32 * 18.0, rect.center().y), 7.5, i < you.season.lit, theme::GOLD_ACTION);
        }
        if let Some(last) = you.season.last {
            ui.label(egui::RichText::new(format!("last season: {last}")).color(theme::TEXT_DIM).size(12.5));
        }
        ui.label(egui::RichText::new(format!("\u{00b7} {} of 52 cards", you.cards_have)).color(theme::TEXT_DIM).size(13.0));
    });
}

const TABS: [&str; 7] = ["\u{2663} Regular", "\u{2666} Hands", "\u{2665} Company", "\u{2660} Results", "Jokers", "Quests", "Journal"];

/// The album, in its own window's central panel.
pub fn draw(ui: &mut egui::Ui, view: &AlbumView, state: &mut AlbumUi) -> AlbumAction {
    let mut action = AlbumAction::None;
    let time = ui.input(|i| i.time) as f32;
    egui::Frame::new().fill(theme::WINDOW).inner_margin(14.0).show(ui, |ui| {
        head(ui, &view.you);
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for (i, tab) in TABS.iter().enumerate() {
                let on = state.page == i;
                let text = egui::RichText::new(*tab).size(14.0).color(if on { theme::INK_ON_GOLD } else { theme::TEXT });
                if ui.add(egui::Button::new(text).fill(if on { theme::GOLD_EDGE } else { theme::PANEL_LIGHT }).corner_radius(12.0)).clicked() {
                    state.page = i;
                    state.flipped = None;
                }
            }
        });
        ui.add_space(8.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match state.page {
            5 => {
                ui.label(egui::RichText::new("Today").color(theme::TEXT).size(16.0).strong());
                ui.add_space(4.0);
                for (i, q) in view.you.daily.iter().enumerate() {
                    let swap = (view.may_swap && !q.done).then_some("Swap");
                    if quest_row(ui, q, swap) {
                        action = AlbumAction::Swap(i);
                    }
                }
                ui.label(egui::RichText::new(if view.may_swap { "One quest a day can be swapped for another." } else { "Today's swap is used; tomorrow brings another." }).color(theme::TEXT_DIM).size(12.5));
                ui.add_space(10.0);
                ui.label(egui::RichText::new("This week").color(theme::TEXT).size(16.0).strong());
                ui.add_space(4.0);
                match view.you.weekly.as_ref() {
                    Some(q) => {
                        quest_row(ui, q, None);
                    }
                    None => {
                        ui.label(egui::RichText::new("Pick this week's challenge:").color(theme::TEXT_DIM).size(13.5));
                        ui.add_space(4.0);
                        for (i, q) in view.weekly_offer.iter().enumerate() {
                            if quest_row(ui, q, Some("Pick")) {
                                action = AlbumAction::PickWeekly(i);
                            }
                        }
                    }
                }
            }
            6 => {
                ui.label(egui::RichText::new("Every change, with its reason \u{2014} newest first.").color(theme::TEXT_DIM).size(13.0));
                ui.add_space(4.0);
                for l in &view.journal {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(&l.when).color(theme::TEXT_DIM).size(12.5));
                        if !l.delta.is_empty() {
                            let colour = if l.delta.starts_with('+') { theme::GOLD_EDGE } else { theme::WARN };
                            ui.label(egui::RichText::new(&l.delta).color(colour).size(13.5).strong());
                        }
                        ui.add(egui::Label::new(egui::RichText::new(&l.why).color(theme::TEXT).size(13.5)).wrap());
                    });
                }
                if view.journal.is_empty() {
                    ui.label(egui::RichText::new("Nothing yet. Your first finished game writes the first line.").color(theme::TEXT_DIM).size(14.0));
                }
            }
            page => {
                let Some((suit, cards)) = view.pages.get(page) else {
                    return;
                };
                let have = cards.iter().filter(|c| c.earned.is_some()).count();
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(format!("{} {}", suit.symbol(), suit.set_name())).color(suit_ink(*suit)).size(18.0).strong());
                    ui.label(egui::RichText::new(format!("{have} of {}", cards.len())).color(theme::TEXT_DIM).size(14.0));
                });
                ui.add(egui::Label::new(egui::RichText::new(suit.set_about()).color(theme::TEXT_DIM).size(13.5)).wrap());
                ui.add_space(8.0);
                let gap = 12.0;
                let across = (((ui.available_width() + gap) / (CARD.x + gap)).floor() as usize).max(1);
                for row in cards.chunks(across) {
                    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), CARD.y + gap), Sense::hover());
                    for (i, c) in row.iter().enumerate() {
                        let at = Rect::from_min_size(pos2(rect.left() + i as f32 * (CARD.x + gap), rect.top()), CARD);
                        let turned = state.flipped == Some(c.card.id);
                        let resp = card(ui, at, c, turned, time);
                        if resp.clicked() && c.earned.is_some() {
                            state.flipped = if turned { None } else { Some(c.card.id) };
                        }
                        if turned {
                            let on_show = view.you.showcase.is_some_and(|s| s.id == c.card.id);
                            let b = Rect::from_min_size(pos2(at.left() + 12.0, at.bottom() - 34.0), vec2(CARD.x - 24.0, 24.0));
                            let label = if on_show { "On your card" } else { "Show on my card" };
                            if ui.put(b, egui::Button::new(egui::RichText::new(label).size(12.0))).clicked() {
                                action = AlbumAction::Showcase(if on_show { String::new() } else { c.card.id.to_string() });
                            }
                        }
                    }
                }
                if cards.iter().any(|c| c.card.rarity() == Rarity::Holo && c.earned.is_some()) {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
                }
            }
        });
    });
    action
}
