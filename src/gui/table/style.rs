//! PokerTH's Green Casino table style, and the QML client's pieces drawn in it.
//!
//! The owner, 2026-09-13: the table window after PokerTH's table, as faithfully
//! as it can be, in the *Green Casino* style. The values are PokerTH's own
//! (commit `944e8b83`):
//!
//! * `data/gfx/qml/table/greencasino/greencasinotablestyle.xml` -- the player
//!   box accent, the chat and log colours, the four action buttons (their SVG
//!   gradients are drawn here stop for stop);
//! * `src/gui/qt6-qml/config/Theme.qml` and the components -- the accent of the
//!   stacks and the pot, the action badges, the clock, the winner's gold, the
//!   box's gradient and shadow, the quick bet buttons, the slider.
//!
//! Where the owner's own words set a piece differently they win, and the
//! constant says so: the pucks are gold chips, and a bet or a raise wears a
//! gold badge.
//!
//! The cards are not here: `paint::card_face` and `paint::card_back` draw them
//! exactly as before (the owner: leave the cards as they are).

use eframe::egui::{
    self, epaint::Shadow, pos2, vec2, Align2, Color32, CornerRadius, FontFamily, FontId, Mesh, Painter, Pos2,
    Rect, Shape, Stroke, StrokeKind, Vec2,
};

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const fn rgba(hex: u32, a: u8) -> Color32 {
    Color32::from_rgba_premultiplied(
        (((hex >> 16) & 0xff) * a as u32 / 255) as u8,
        (((hex >> 8) & 0xff) * a as u32 / 255) as u8,
        ((hex & 0xff) * a as u32 / 255) as u8,
        a,
    )
}

/// `PlayerBoxAccent`: casino gold, the boxes' edge and highlight.
pub const BOX_ACCENT: Color32 = rgb(0xC8A84A);
/// `Theme.colorAccent`: the stacks, the pot, the rating.
pub const COLOR_ACCENT: Color32 = rgb(0xE3C800);
/// The chat and log palette (`ChatLog*`).
pub const PANEL_BG: Color32 = rgb(0x0C1A0E);
pub const PANEL_SURFACE: Color32 = rgb(0x14281A);
pub const PANEL_BORDER: Color32 = rgb(0x5A4A20);
pub const PANEL_TEXT: Color32 = rgb(0xFFF5D6);
pub const PANEL_TEXT_2: Color32 = rgb(0xE6D6A8);
pub const PANEL_MUTED: Color32 = rgb(0xA99760);
/// The log's roles the style leaves at PokerTH's dark defaults.
pub const LOG_WINNER: Color32 = rgb(0xFFFF00);
pub const LOG_BOARD: Color32 = rgb(0xFF6633);
pub const SEND: Color32 = rgb(0x4ADE80);
/// The app around the table, and the bars over it.
pub const APP_BG: Color32 = rgb(0x1D222B);
pub const TOP_BAR: Color32 = rgb(0x14171C);
pub const STATUS_BAR: Color32 = rgba(0x000000, 199);
pub const STATUS_LABEL: Color32 = rgb(0x9E9E9E);
pub const STATUS_TOTAL: Color32 = rgb(0x99D500);
pub const STATUS_BETS: Color32 = rgb(0x7AA800);
pub const WHITE: Color32 = rgb(0xFFFFFF);
pub const NAME: Color32 = rgb(0xEFF1F5);
pub const TEXT_2: Color32 = rgb(0xCDD3E0);
/// The clock (`PlayerTimeoutBar`).
pub const TIMEOUT: Color32 = rgb(0x4070D0);
pub const TIMEOUT_SELF: Color32 = rgb(0x6E9CEC);
pub const TIMEOUT_TRACK: Color32 = rgb(0x0E1A30);
/// The turn's glow and the winner's gold.
pub const GLOW: Color32 = rgb(0xFFD54A);
pub const GOLD: Color32 = rgb(0xFFD700);
pub const WINNER_BADGE: Color32 = rgb(0x0D3D0D);
/// `dimmedOpacity`: a seat out of the game.
pub const DIMMED: f32 = 0.40;

/// The fonts: PokerTH's Inter at the weights its components ask for.
pub const INTER: &[u8] = include_bytes!("../../../assets/pokerth/fonts/Inter-VariableFont.ttf");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Medium,
    DemiBold,
    Bold,
}

impl Weight {
    fn key(self) -> &'static str {
        match self {
            Weight::Regular => "pokerth-inter-400",
            Weight::Medium => "pokerth-inter-500",
            Weight::DemiBold => "pokerth-inter-600",
            Weight::Bold => "pokerth-inter-700",
        }
    }

    fn value(self) -> f32 {
        match self {
            Weight::Regular => 400.0,
            Weight::Medium => 500.0,
            Weight::DemiBold => 600.0,
            Weight::Bold => 700.0,
        }
    }
}

/// Inter at `size` and `weight`.
pub fn font(size: f32, weight: Weight) -> FontId {
    FontId::new(size, FontFamily::Name(weight.key().into()))
}

/// Add PokerTH's Inter to the context as four named families, one per weight,
/// each falling back to egui's own fonts for what Inter does not draw. The
/// lobby's fonts are untouched.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for w in [Weight::Regular, Weight::Medium, Weight::DemiBold, Weight::Bold] {
        let tweak = egui::FontTweak {
            coords: egui::epaint::text::VariationCoords::new([(b"wght", w.value())]),
            ..Default::default()
        };
        fonts
            .font_data
            .insert(w.key().to_owned(), std::sync::Arc::new(egui::FontData::from_static(INTER).tweak(tweak)));
        let mut family = vec![w.key().to_owned()];
        family.extend(fallback.iter().cloned());
        fonts.families.insert(FontFamily::Name(w.key().into()), family);
    }
    ctx.set_fonts(fonts);
}

/// `a` and `b` mixed, `t` of the way to `b`, alpha included.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let [ar, ag, ab, aa] = a.to_srgba_unmultiplied();
    let [br, bg, bb, ba] = b.to_srgba_unmultiplied();
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(ar, br), l(ag, bg), l(ab, bb), l(aa, ba))
}

/// `c` at `opacity` of its alpha.
pub fn faded(c: Color32, opacity: f32) -> Color32 {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * opacity.clamp(0.0, 1.0)).round() as u8)
}

/// Qt's `tint(base, tint at strength)`.
pub fn tint(base: Color32, tint: Color32, strength: f32) -> Color32 {
    mix(base, tint, strength)
}

/// The corner radius of PokerTH's rounded rectangle as egui takes it.
fn radius(r: f32) -> CornerRadius {
    CornerRadius::from(r.clamp(0.0, 255.0))
}

/// The outline of a rounded rectangle, for meshes.
fn rounded_outline(rect: Rect, r: f32) -> Vec<Pos2> {
    let r = r.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    let steps = 6;
    let mut pts = Vec::with_capacity(4 * (steps + 1));
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -90.0f32),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 90.0),
        (pos2(rect.left() + r, rect.top() + r), 180.0),
    ];
    for (c, start) in corners {
        for i in 0..=steps {
            let a = (start + 90.0 * i as f32 / steps as f32).to_radians();
            pts.push(pos2(c.x + r * a.cos(), c.y + r * a.sin()));
        }
    }
    pts
}

/// A rounded rectangle filled with a vertical gradient through `stops`
/// (`(position 0..=1, colour)`, in order), as the SVGs and the QML gradients
/// fill them.
pub fn gradient_rect(p: &Painter, rect: Rect, r: f32, stops: &[(f32, Color32)]) {
    if stops.is_empty() || rect.height() <= 0.0 {
        return;
    }
    let colour_at = |y: f32| -> Color32 {
        let t = ((y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        let mut prev = stops[0];
        for s in stops {
            if t <= s.0 {
                let span = (s.0 - prev.0).max(1e-6);
                return mix(prev.1, s.1, (t - prev.0) / span);
            }
            prev = *s;
        }
        stops[stops.len() - 1].1
    };
    let outline = rounded_outline(rect, r);
    let mut mesh = Mesh::default();
    let c = rect.center();
    mesh.colored_vertex(c, colour_at(c.y));
    for pt in &outline {
        mesh.colored_vertex(*pt, colour_at(pt.y));
    }
    let n = outline.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(Shape::mesh(mesh));
}

/// A soft drop shadow under a rounded rectangle.
pub fn shadow(p: &Painter, rect: Rect, r: f32, offset_y: f32, blur: f32, colour: Color32) {
    let s = Shadow {
        offset: [0, offset_y.round().clamp(-100.0, 100.0) as i8],
        blur: blur.round().clamp(0.0, 255.0) as u8,
        spread: 0,
        color: colour,
    };
    p.add(s.as_shape(rect, radius(r)));
}

/// A glow around a rounded rectangle (a shadow with no offset).
pub fn glow(p: &Painter, rect: Rect, r: f32, blur: f32, colour: Color32) {
    shadow(p, rect, r, 0.0, blur, colour);
}

/// Text at a weight, anchored.
pub fn text(p: &Painter, at: Pos2, align: Align2, s: &str, size: f32, weight: Weight, colour: Color32) -> Rect {
    p.text(at, align, s, font(size, weight), colour)
}

/// How wide `s` is at a size and weight.
pub fn text_width(p: &Painter, s: &str, size: f32, weight: Weight) -> f32 {
    p.layout_no_wrap(s.to_owned(), font(size, weight), WHITE).size().x
}

/// Text cut to `width` with an ellipsis, PokerTH's `elide: Text.ElideRight`.
pub fn elided(p: &Painter, s: &str, size: f32, weight: Weight, width: f32) -> String {
    if text_width(p, s, size, weight) <= width {
        return s.to_owned();
    }
    let mut out: String = s.to_owned();
    while !out.is_empty() {
        out.pop();
        let candidate = format!("{}…", out.trim_end());
        if text_width(p, &candidate, size, weight) <= width {
            return candidate;
        }
    }
    "…".to_owned()
}

/// `PlayerBoxBackground`: the gold-tinted gradient, the accent edge at 60 %,
/// the shadow, all at 90 % opacity; `opacity` below that for a seat that folded
/// or is out of the game.
pub fn player_box(p: &Painter, rect: Rect, s: f32, opacity: f32) {
    let r = 6.0 * s;
    let base_top = rgb(0x434D5E);
    let base_bottom = rgb(0x1D222B);
    let top = faded(tint(base_top, BOX_ACCENT, 0.34), 0.9 * opacity);
    let bottom = faded(tint(base_bottom, BOX_ACCENT, 0.22), 0.9 * opacity);
    shadow(p, rect, r, 3.0 * s, 10.0 * s, faded(Color32::BLACK, 0.42 * opacity));
    gradient_rect(p, rect, r, &[(0.0, top), (1.0, bottom)]);
    p.rect_stroke(rect, radius(r), Stroke::new(1.0, faded(BOX_ACCENT, 0.60 * opacity)), StrokeKind::Inside);
}

fn pulse(time: f64) -> f32 {
    0.65 + 0.35 * (0.5 + 0.5 * ((time * std::f64::consts::PI / 0.75).sin() as f32))
}

/// A glow outside a rounded rectangle only: rings fading outwards, so the
/// light falls on the felt around the box and never on the box itself.
pub fn halo(p: &Painter, rect: Rect, r: f32, reach: f32, colour: Color32) {
    let rings = 7;
    let step = (reach / rings as f32).max(0.5);
    for i in 0..rings {
        let t = i as f32 / rings as f32;
        let ring = rect.expand(step * (i as f32 + 0.5));
        p.rect_stroke(ring, radius(r + step * i as f32), Stroke::new(step + 0.3, faded(colour, (1.0 - t) * (1.0 - t))), StrokeKind::Middle);
    }
}

/// The halo of `PlayerTurnGlow`, drawn under the box.
pub fn turn_halo(p: &Painter, rect: Rect, s: f32, time: f64) {
    halo(p, rect.expand(2.0 * s), 6.0 * s, 9.0 * s, faded(GOLD, 0.5 * pulse(time)));
}

/// `PlayerTurnGlow`: the seat on turn, a pulsing gold edge.
pub fn turn_glow(p: &Painter, rect: Rect, s: f32, width: f32, time: f64) {
    let r = 6.0 * s;
    let outer = rect.expand(2.0 * s);
    p.rect_stroke(outer, radius(r), Stroke::new(width * s.max(0.8), faded(GLOW, pulse(time))), StrokeKind::Inside);
}

/// The halo of `PlayerWinnerOverlay`, drawn under the box.
pub fn winner_glow(p: &Painter, rect: Rect, s: f32) {
    halo(p, rect, 6.0 * s, 11.0 * s, faded(GOLD, 0.75));
}

/// `PlayerWinnerOverlay`: the gold frame and the *WINNER* badge above the box.
pub fn winner(p: &Painter, rect: Rect, s: f32) {
    let r = 6.0 * s;
    p.rect_stroke(rect, radius(r), Stroke::new(3.0 * s, GOLD), StrokeKind::Inside);
    let size = 9.0 * s.max(0.9);
    let w = text_width(p, "WINNER", size, Weight::Bold) + 12.0 * s;
    let h = 16.0 * s;
    let badge = Rect::from_center_size(pos2(rect.center().x, rect.top() - 6.0 * s - h / 2.0), vec2(w, h));
    p.rect_filled(badge, radius(h / 2.0), WINNER_BADGE);
    p.rect_stroke(badge, radius(h / 2.0), Stroke::new(1.0, GOLD), StrokeKind::Inside);
    text(p, badge.center(), Align2::CENTER_CENTER, "WINNER", size, Weight::Bold, GOLD);
}

/// The colours of a seat's action badge: fill, edge, text, edge width.
///
/// PokerTH's `actionBadgeColor`/`actionBadgeBorder` for fold, check and call;
/// the owner's gold for a bet or a raise; and all in in the Green Casino
/// All-In button's own look, a bright red edge round a dark pill, so it reads
/// apart from a fold.
pub fn badge_colours(act: super::SeatAct) -> (Color32, Color32, Color32, f32) {
    use super::SeatAct;
    match act {
        SeatAct::Fold => (rgb(0x5A1010), rgb(0xE87070), rgb(0xEAF1FF), 1.0),
        SeatAct::Check | SeatAct::Call => (rgb(0x122A55), rgb(0x6AA0E8), rgb(0xEAF1FF), 1.0),
        SeatAct::Bet | SeatAct::Raise => (rgb(0x4A3A10), rgb(0xDCC065), PANEL_TEXT, 1.0),
        SeatAct::AllIn => (rgba(0x000000, 170), rgb(0xEF5350), WHITE, 2.0),
    }
}

/// `PlayerActionBadge`: the word in a pill, centred at `at`.
pub fn action_badge(p: &Painter, at: Pos2, act: super::SeatAct, s: f32, pop: f32) -> Rect {
    let (fill, edge, ink, edge_w) = badge_colours(act);
    let size = 12.0 * s;
    let w = (text_width(p, act.word(), size, Weight::Bold) + 14.0 * s) * pop;
    let h = 18.0 * s * pop;
    let rect = Rect::from_center_size(at, vec2(w, h));
    p.rect_filled(rect, radius(h / 2.0), fill);
    p.rect_stroke(rect, radius(h / 2.0), Stroke::new(edge_w, edge), StrokeKind::Inside);
    text(p, rect.center(), Align2::CENTER_CENTER, act.word(), size * pop, Weight::Bold, ink);
    rect
}

/// `PlayerTimeoutBar`: the time left, shrinking to the left.
pub fn timeout_bar(p: &Painter, rect: Rect, left: f32, fill: Color32) {
    let r = rect.height() / 2.0;
    shadow(p, rect, r, 1.0, 4.0, faded(Color32::BLACK, 0.6));
    p.rect_filled(rect, radius(r), TIMEOUT_TRACK);
    p.rect_stroke(rect, radius(r), Stroke::new(1.0, faded(WHITE, 0.55)), StrokeKind::Inside);
    let inner = rect.shrink(1.0);
    let w = inner.width() * left.clamp(0.0, 1.0);
    if w > 0.5 {
        let bar = Rect::from_min_size(inner.min, vec2(w, inner.height()));
        p.rect_filled(bar, radius(inner.height() / 2.0), fill);
    }
}

/// `chipStack.svg`, drawn: three chips, the top one lit.
pub fn chip_icon(p: &Painter, rect: Rect) {
    let w = rect.width() * 0.46;
    let h = rect.height() * 0.17;
    for i in 0..3 {
        let c = pos2(rect.center().x, rect.bottom() - h * 1.2 - i as f32 * h * 1.15);
        let body = if i == 2 { COLOR_ACCENT } else { BOX_ACCENT };
        p.add(egui::epaint::EllipseShape::filled(pos2(c.x, c.y + h * 0.35), vec2(w, h), rgb(0x5A4A20)));
        p.add(egui::epaint::EllipseShape::filled(c, vec2(w, h), body));
        p.add(egui::epaint::EllipseShape::stroke(c, vec2(w * 0.62, h * 0.62), Stroke::new(1.0, faded(WHITE, 0.55))));
    }
}

/// `BetChip`: the chip and `$amount`, on a small dark pill so it reads on the
/// felt; `rect` is where the layout put it. Returns the width it needs at
/// height `h`.
pub fn bet_chip_width(p: &Painter, amount: u64, h: f32) -> f32 {
    let size = h * 0.68;
    h * 0.9 + 3.0 + text_width(p, &format!("${amount}"), size, Weight::Bold) + h * 0.7
}

pub fn bet_chip(p: &Painter, rect: Rect, amount: u64) {
    let h = rect.height();
    p.rect_filled(rect, radius(h / 2.0), rgba(0x000000, 150));
    p.rect_stroke(rect, radius(h / 2.0), Stroke::new(1.0, faded(BOX_ACCENT, 0.45)), StrokeKind::Inside);
    let icon = Rect::from_min_size(pos2(rect.left() + h * 0.3, rect.top() + h * 0.08), vec2(h * 0.84, h * 0.84));
    chip_icon(p, icon);
    text(
        p,
        pos2(icon.right() + 3.0, rect.center().y),
        Align2::LEFT_CENTER,
        &format!("${amount}"),
        h * 0.68,
        Weight::Bold,
        rgb(0xF0F0F0),
    );
}

/// The pot above the community cards: a black pill with the accent edge and
/// glow, the chip and `$pot` in the accent.
pub fn pot_badge(p: &Painter, center: Pos2, pot: u64, s: f32) -> Rect {
    let size = 11.0 * s;
    let label = format!("${pot}");
    let icon = 14.0 * s;
    let w = icon + 4.0 * s + text_width(p, &label, size, Weight::Bold) + 12.0 * s;
    let h = 20.0 * s;
    let rect = Rect::from_center_size(center, vec2(w, h));
    glow(p, rect, h / 2.0, 10.0 * s, faded(COLOR_ACCENT, 0.45));
    p.rect_filled(rect, radius(12.0 * s), rgba(0x000000, 158));
    p.rect_stroke(rect, radius(12.0 * s), Stroke::new(1.0, COLOR_ACCENT), StrokeKind::Inside);
    let icon_rect = Rect::from_min_size(pos2(rect.left() + 6.0 * s, rect.center().y - icon / 2.0), vec2(icon, icon));
    chip_icon(p, icon_rect);
    text(p, pos2(icon_rect.right() + 4.0 * s, rect.center().y), Align2::LEFT_CENTER, &label, size, Weight::Bold, COLOR_ACCENT);
    rect
}

/// A community slot with no card yet.
pub fn empty_slot(p: &Painter, rect: Rect, s: f32) {
    p.rect_filled(rect, radius(4.0 * s), rgba(0x000000, 77));
    p.rect_stroke(rect, radius(4.0 * s), Stroke::new(1.0, faded(WHITE, 0.38)), StrokeKind::Inside);
}

/// The warm light behind the community row.
pub fn board_glow(p: &Painter, rect: Rect) {
    let r = rect.height() / 2.0;
    glow(p, rect.shrink(8.0), r, 40.0, Color32::from_rgba_unmultiplied(255, 237, 184, 22));
}

/// Which puck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Puck {
    Dealer,
    SmallBlind,
    BigBlind,
}

/// A casino puck: `dealerPuck.svg` drawn -- the dark chip, eight gold edge
/// stripes, the gold-ringed centre, the gloss -- with its letters. Gold for
/// all three, the owner's word (Green Casino's own SB and BB pucks are blue
/// and red).
pub fn puck(p: &Painter, rect: Rect, which: Puck) {
    let c = rect.center();
    let u = rect.width() / 32.0;
    let gold = rgb(0xC8A850);
    shadow(p, rect.shrink(1.0 * u), 15.5 * u, 1.5 * u, 4.0 * u, faded(Color32::BLACK, 0.55));
    p.circle_filled(c, 15.5 * u, rgb(0x3D2B00));
    p.circle_filled(c, 13.0 * u, rgb(0x1A1A1A));
    for k in 0..8 {
        let a = (k as f32 * 45.0).to_radians();
        let rot = |x: f32, y: f32| -> Pos2 {
            let (dx, dy) = ((x - 16.0) * u, (y - 16.0) * u);
            pos2(c.x + dx * a.cos() - dy * a.sin(), c.y + dx * a.sin() + dy * a.cos())
        };
        let pts = vec![rot(13.0, 0.5), rot(19.0, 0.5), rot(19.0, 7.5), rot(13.0, 7.5)];
        p.add(Shape::convex_polygon(pts, faded(gold, 0.9), Stroke::NONE));
    }
    p.circle_filled(c, 9.0 * u, rgb(0x1A1A1A));
    p.circle_stroke(c, 9.0 * u, Stroke::new(1.5 * u, gold));
    let (label, size) = match which {
        Puck::Dealer => ("D", 11.0 * u),
        Puck::SmallBlind => ("SB", 7.6 * u),
        Puck::BigBlind => ("BB", 7.6 * u),
    };
    text(p, pos2(c.x, c.y + 0.3 * u), Align2::CENTER_CENTER, label, size, Weight::Bold, GOLD);
    // The gloss: light from the upper left.
    let mut mesh = Mesh::default();
    let centre = pos2(rect.left() + rect.width() * 0.37, rect.top() + rect.height() * 0.30);
    mesh.colored_vertex(centre, Color32::from_rgba_unmultiplied(255, 255, 255, 50));
    let n = 28u32;
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(pos2(c.x + 15.5 * u * a.cos(), c.y + 15.5 * u * a.sin()), Color32::from_rgba_unmultiplied(0, 0, 0, 20));
    }
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(Shape::mesh(mesh));
}

/// The round avatar: a dark disc with the gold edge and the name's initial.
/// Grey when the seat is out of the game, as PokerTH desaturates it.
pub fn avatar(p: &Painter, rect: Rect, name: &str, active: bool) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) / 2.0;
    p.circle_filled(c, r, faded(rgb(0x1D222B), 0.9));
    let edge = if active { TEXT_2 } else { rgb(0x7787A3) };
    p.circle_stroke(c, r - 0.5, Stroke::new(1.0, edge));
    let initial = name.chars().find(|ch| !ch.is_whitespace()).unwrap_or('?').to_uppercase().to_string();
    let colour = if active { BOX_ACCENT } else { rgb(0x8A8F99) };
    text(p, pos2(c.x, c.y + r * 0.04), Align2::CENTER_CENTER, &initial, r * 1.05, Weight::Bold, colour);
}

/// The stars under a name: `rating` of five.
pub fn stars(p: &Painter, at: Pos2, rating: u8, size: f32) -> Rect {
    let s: String = (0..5).map(|i| if i < rating { '★' } else { '☆' }).collect();
    text(p, at, Align2::LEFT_CENTER, &s, size, Weight::Regular, COLOR_ACCENT)
}

pub use super::icons::{icon, Icon};

/// `GameRoundIconButton`: a 34-point round button with an icon, the accent
/// when on, and an unread count.
pub fn round_button(ui: &egui::Ui, center: Pos2, id: egui::Id, which: Icon, active: bool, unread: usize, tip: &str) -> egui::Response {
    let rect = Rect::from_center_size(center, vec2(34.0, 34.0));
    let resp = ui.interact(rect, id, egui::Sense::click()).on_hover_text(tip);
    let p = ui.painter();
    let fill = if active { COLOR_ACCENT } else { rgba(0x000000, 115) };
    p.circle_filled(rect.center(), 17.0, if resp.hovered() && !active { rgba(0x000000, 160) } else { fill });
    icon(p, Rect::from_center_size(rect.center(), vec2(20.0, 20.0)), which, if active { rgb(0x101010) } else { WHITE });
    if unread > 0 {
        let label = if unread > 99 { "99+".to_owned() } else { unread.to_string() };
        let w = (text_width(p, &label, 10.0, Weight::Bold) + 8.0).max(17.0);
        let badge = Rect::from_min_size(pos2(rect.right() + 3.0 - w, rect.top() - 3.0), vec2(w, 17.0));
        p.rect_filled(badge, radius(8.5), rgb(0xE05050));
        p.rect_stroke(badge, radius(8.5), Stroke::new(1.5, APP_BG), StrokeKind::Outside);
        text(p, badge.center(), Align2::CENTER_CENTER, &label, 10.0, Weight::Bold, WHITE);
    }
    resp
}

/// The Green Casino action buttons: the SVGs' gradients, strokes and gloss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonLook {
    Fold,
    Call,
    Raise,
    AllIn,
}

impl ButtonLook {
    fn stops(self) -> Option<[(f32, Color32); 3]> {
        match self {
            ButtonLook::Fold => Some([(0.0, rgb(0x8A2C2C)), (0.5, rgb(0x6B2020)), (1.0, rgb(0x561818))]),
            ButtonLook::Call => Some([(0.0, rgb(0x245AA0)), (0.5, rgb(0x1A4A8A)), (1.0, rgb(0x143F78))]),
            ButtonLook::Raise => Some([(0.0, rgb(0xDCC065)), (0.5, rgb(0xC8A84A)), (1.0, rgb(0xAD8C33))]),
            ButtonLook::AllIn => None,
        }
    }

    fn stroke(self) -> (Color32, f32) {
        match self {
            ButtonLook::Fold => (rgb(0x3A0E0E), 1.5),
            ButtonLook::Call => (rgb(0x0C2F5E), 1.5),
            ButtonLook::Raise => (rgb(0x7D6420), 1.5),
            ButtonLook::AllIn => (rgb(0xC0392B), 2.5),
        }
    }

    /// `contrastTextColor`: dark on the gold, white on the others.
    pub fn ink(self) -> Color32 {
        match self {
            ButtonLook::Raise => rgb(0x1A1A1A),
            _ => rgb(0xF0F0F0),
        }
    }

    /// The edge colour of the highlight (`colorFoldEdge` and its kin).
    fn edge(self) -> Color32 {
        match self {
            ButtonLook::Fold => rgb(0xE87070),
            ButtonLook::Call => rgb(0x6AA0E8),
            ButtonLook::Raise => rgb(0x7AD06A),
            ButtonLook::AllIn => rgb(0xEF5350),
        }
    }
}

/// `themeButtonRadius`: the SVG's 9 of 168×43, fitted.
pub fn button_radius(size: Vec2) -> f32 {
    (size.x / 2.0).min(size.y / 2.0).min(9.0 * size.x / 168.0).min(9.0 * size.y / 43.0)
}

/// One of the action buttons, painted into `rect`: `armed` is PokerTH's
/// clickable, `hovered` and `pressed` the pointer, `highlight` the raise's glow,
/// `pre` the gold ring of a preselected action.
#[allow(clippy::too_many_arguments)]
pub fn action_button(
    p: &Painter,
    rect: Rect,
    look: ButtonLook,
    label: &str,
    size: f32,
    opacity: f32,
    hovered: bool,
    pressed: bool,
    highlight: bool,
    pre: bool,
) {
    let rect = if pressed { Rect::from_center_size(rect.center(), rect.size() * 0.96) } else { rect };
    let r = button_radius(rect.size());
    if highlight {
        glow(p, rect, r, 10.0, faded(look.edge(), 0.55 * opacity));
    }
    let sx = rect.width() / 168.0;
    let sy = rect.height() / 43.0;
    match look.stops() {
        Some(stops) => {
            let inner = Rect::from_min_max(
                pos2(rect.left() + 1.5 * sx, rect.top() + 1.5 * sy),
                pos2(rect.right() - 1.5 * sx, rect.bottom() - 1.5 * sy),
            );
            let faded_stops: Vec<(f32, Color32)> = stops.iter().map(|(t, c)| (*t, faded(*c, opacity))).collect();
            gradient_rect(p, inner, r, &faded_stops);
            let (edge, w) = look.stroke();
            p.rect_stroke(inner, radius(r), Stroke::new(w * sx.min(sy).max(0.7), faded(edge, opacity)), StrokeKind::Middle);
            let gloss = Rect::from_min_max(pos2(rect.left() + 12.0 * sx, rect.top() + 4.0 * sy), pos2(rect.right() - 12.0 * sx, rect.top() + 18.0 * sy));
            p.rect_filled(gloss, radius(7.0 * sy), faded(Color32::from_rgba_unmultiplied(255, 255, 255, 33), opacity));
        }
        None => {
            let inner = rect.shrink(2.0 * sx.min(sy));
            let (edge, w) = look.stroke();
            p.rect_stroke(inner, radius(r), Stroke::new(w * sx.min(sy).max(0.8), faded(edge, opacity)), StrokeKind::Middle);
        }
    }
    if hovered || pressed {
        p.rect_filled(rect.shrink(2.0), radius(r), Color32::from_rgba_unmultiplied(255, 255, 255, if pressed { 46 } else { 20 }));
    }
    if pre || highlight {
        p.rect_stroke(rect, radius(r), Stroke::new(2.6 * sx.min(sy).max(0.8), faded(if pre { GOLD } else { look.edge() }, opacity)), StrokeKind::Inside);
    }
    let lines: Vec<&str> = label.split('\n').collect();
    let line_h = size * 1.0;
    let top = rect.center().y - line_h * (lines.len() as f32 - 1.0) / 2.0;
    for (i, line) in lines.iter().enumerate() {
        text(p, pos2(rect.center().x, top + i as f32 * line_h), Align2::CENTER_CENTER, line, size, Weight::Bold, faded(look.ink(), opacity));
    }
    if pre {
        p.circle_filled(pos2(rect.right() - 8.0, rect.top() + 8.0), 4.0, GOLD);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Green Casino XML's values, as written there.
    #[test]
    fn the_style_is_green_casinos() {
        assert_eq!(BOX_ACCENT, Color32::from_rgb(0xC8, 0xA8, 0x4A));
        assert_eq!(PANEL_BG, Color32::from_rgb(0x0C, 0x1A, 0x0E));
        assert_eq!(PANEL_SURFACE, Color32::from_rgb(0x14, 0x28, 0x1A));
        assert_eq!(PANEL_BORDER, Color32::from_rgb(0x5A, 0x4A, 0x20));
        assert_eq!(PANEL_TEXT, Color32::from_rgb(0xFF, 0xF5, 0xD6));
        assert_eq!(PANEL_TEXT_2, Color32::from_rgb(0xE6, 0xD6, 0xA8));
        assert_eq!(PANEL_MUTED, Color32::from_rgb(0xA9, 0x97, 0x60));
    }

    /// The owner's colours for the badges: fold red, check and call blue, bet
    /// and raise gold, all in apart from all of them.
    #[test]
    fn the_badges_wear_the_owners_colours() {
        use super::super::SeatAct;
        let hue = |c: Color32| {
            let [r, g, b, _] = c.to_srgba_unmultiplied();
            (r as i32, g as i32, b as i32)
        };
        let (fold, _, _, _) = badge_colours(SeatAct::Fold);
        let (call, _, _, _) = badge_colours(SeatAct::Call);
        let (raise, raise_edge, _, _) = badge_colours(SeatAct::Raise);
        let (_, all_in_edge, _, all_in_w) = badge_colours(SeatAct::AllIn);
        let (r, g, b) = hue(fold);
        assert!(r > g && r > b, "fold is red");
        let (r, g, b) = hue(call);
        assert!(b > r && b > g, "call is blue");
        let (r, g, b) = hue(raise_edge);
        assert!(r > b && g > b && r >= g, "raise is gold");
        assert_eq!(badge_colours(SeatAct::Bet).0, raise);
        assert_eq!(badge_colours(SeatAct::Check), badge_colours(SeatAct::Call));
        assert!(all_in_w > 1.0 && all_in_edge != badge_colours(SeatAct::Fold).1, "all in is set apart");
    }

    #[test]
    fn a_mix_reaches_both_ends() {
        let a = Color32::from_rgb(10, 20, 30);
        let b = Color32::from_rgb(210, 120, 30);
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
    }

    /// PokerTH's radius: 9 units of the 168×43 drawing, never more than half
    /// the button's height.
    #[test]
    fn the_button_radius_is_the_svgs() {
        assert!((button_radius(vec2(168.0, 43.0)) - 9.0).abs() < 1e-4);
        assert!(button_radius(vec2(40.0, 10.0)) <= 5.0);
    }
}
