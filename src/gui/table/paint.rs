//! Drawing the table: the felt, the rail, the cards, the chips, the plates.
//!
//! Every position comes from [`layout`](super::layout); nothing here decides
//! where anything goes. That split is what let the hard part — hero at the
//! bottom, chips never under a card, everything inside the window — be tested
//! without a window.
//!
//! # The look is the reference photograph's
//!
//! `assets/ggpoker-rush-and-cash-table.jpg`, whose colours were sampled into
//! [`theme`](crate::gui::theme): felt darkest at the rim and brightest just off
//! centre, a rail lit from below, and a four-colour deck whose cards are a solid
//! suit colour with a white notch in one corner and a large white rank.
//!
//! egui has no gradients, so both gradients are drawn as bands — concentric
//! ellipses for the felt, horizontal slices for the rail. At a dozen steps the
//! banding is below what the eye picks out, and it costs a dozen shapes.
//!
//! # The one rule that is not decoration
//!
//! `SPEC_CS.md` §22: **never display a cryptographically unverified card as
//! valid.** [`Facing`](super::Facing) has no public constructor that produces a
//! face-up card without a verification verdict, so a card whose proof has not
//! been checked is drawn as a back — not as a mistake that could be made, but as
//! the only thing the type can become.

use eframe::egui::{
    epaint::EllipseShape, pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Stroke,
    StrokeKind,
};

use super::layout::Layout;
use crate::gui::theme::{self, SuitColour};
use crate::poker::state::Card;

/// A blend of two colours, for the bands that stand in for gradients.
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let c = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(c(a.r(), b.r()), c(a.g(), b.g()), c(a.b(), b.b()))
}

fn ellipse(p: &Painter, rect: Rect, fill: Color32) {
    p.add(EllipseShape::filled(
        rect.center(),
        vec2(rect.width() * 0.5, rect.height() * 0.5),
        fill,
    ));
}

/// The rail and the felt, in that order.
pub fn table(p: &Painter, l: &Layout, name: &str) {
    // The room the table stands in. Warm and very dark, like the reference's
    // surround; the lobby's cool slate behind a wooden rail reads as a
    // screenshot pasted onto a different application.
    p.rect_filled(l.area, 0.0, theme::ROOM);

    // The rail, lit from below: horizontal slices from the dark top to the warm
    // bottom. Only the ring shows once the felt is drawn over it.
    const BANDS: usize = 14;
    for i in 0..BANDS {
        let t = i as f32 / (BANDS - 1) as f32;
        let colour = if t < 0.5 {
            mix(theme::RAIL_TOP, theme::RAIL_SIDE, t * 2.0)
        } else {
            mix(theme::RAIL_SIDE, theme::RAIL_BOTTOM, (t - 0.5) * 2.0)
        };
        let top = l.rail.top() + l.rail.height() * (i as f32 / BANDS as f32);
        let band = Rect::from_min_max(
            pos2(l.rail.left() - 2.0, top),
            pos2(
                l.rail.right() + 2.0,
                l.rail.top() + l.rail.height() * ((i + 1) as f32 / BANDS as f32) + 0.75,
            ),
        );
        ellipse(&p.with_clip_rect(band), l.rail, colour);
    }
    p.add(EllipseShape::stroke(
        l.rail.center(),
        vec2(l.rail.width() * 0.5, l.rail.height() * 0.5),
        Stroke::new(1.5, theme::RAIL_OUTER_EDGE),
    ));

    // The felt: concentric ellipses, dark at the rim and brightest just above
    // the middle, which is where the sampled highlight sits.
    const RINGS: usize = 16;
    let highlight = pos2(l.centre.x, l.centre.y - l.felt.height() * 0.07);
    for i in 0..RINGS {
        let t = i as f32 / (RINGS - 1) as f32;
        let k = 1.0 - t * 0.98;
        // The ramp is pulled towards the bright end, because the rings share
        // the radius evenly and a linear ramp puts almost all of the area in
        // the dark half — which is what made the first felt look flat.
        let g = t.powf(0.62);
        let colour = if g < 0.5 {
            mix(theme::FELT_EDGE, theme::FELT_MID, g / 0.5)
        } else {
            mix(theme::FELT_MID, theme::FELT_CENTRE, (g - 0.5) / 0.5)
        };
        let centre = pos2(
            l.centre.x + (highlight.x - l.centre.x) * t,
            l.centre.y + (highlight.y - l.centre.y) * t,
        );
        p.add(EllipseShape::filled(
            centre,
            vec2(l.felt.width() * 0.5 * k, l.felt.height() * 0.5 * k),
            colour,
        ));
    }
    p.add(EllipseShape::stroke(
        l.felt.center(),
        vec2(l.felt.width() * 0.5, l.felt.height() * 0.5),
        Stroke::new(2.0, theme::FELT_KEYLINE),
    ));

    // The table's name, spaced out the way the reference spaces its own along
    // the rail — but on the felt, above the pot.
    //
    // The rail is where it belongs and the rail is where it was, and it was
    // invisible: at six seats there is a plate at the top of the ring and
    // another at the bottom, and they are drawn over it. Here it is always
    // legible and never in the way, because the band above the pot is the one
    // part of the felt the layout guarantees is empty.
    if !name.is_empty() {
        let letters: String = name
            .to_uppercase()
            .chars()
            .take(22)
            .flat_map(|c| [c, ' '])
            .collect();
        p.text(
            pos2(l.centre.x, l.centre.y - l.felt.height() * 0.30),
            Align2::CENTER_CENTER,
            letters.trim_end(),
            FontId::proportional((l.felt.height() * 0.045).clamp(10.0, 22.0)),
            theme::FELT_LETTERING,
        );
    }
}

/// A card, face up.
///
/// The reference's design: the whole card is the suit's colour with a white
/// notch in the top-left corner carrying the rank and the pip, and a large white
/// rank across the rest. It reads at a glance from across the table, which a
/// white card with a small coloured corner does not.
pub fn card_face(p: &Painter, rect: Rect, card: Card) {
    let suit = SuitColour::from_index(card.suit() as u8).unwrap_or(SuitColour::Spades);
    let r = (rect.width() * 0.13).max(2.0);

    p.rect_filled(rect, r, suit.face());
    // Lit from the top, like the reference.
    p.rect_filled(
        Rect::from_min_max(rect.min, pos2(rect.right(), rect.center().y)),
        r,
        suit.face_light(),
    );
    p.rect_filled(
        Rect::from_min_max(pos2(rect.left(), rect.center().y - r), rect.max),
        r,
        suit.face(),
    );
    p.rect_stroke(
        rect,
        r,
        Stroke::new(1.0, theme::CARD_FACE),
        StrokeKind::Inside,
    );

    // The white corner notch.
    let notch = Rect::from_min_size(
        rect.min + vec2(1.0, 1.0),
        vec2(rect.width() * 0.50, rect.height() * 0.46),
    );
    p.rect_filled(notch, r * 0.8, theme::CARD_FACE);
    p.text(
        pos2(notch.center().x, notch.top() + notch.height() * 0.30),
        Align2::CENTER_CENTER,
        card.rank().symbol(),
        FontId::proportional(notch.height() * 0.52),
        suit.face(),
    );
    p.text(
        pos2(notch.center().x, notch.top() + notch.height() * 0.76),
        Align2::CENTER_CENTER,
        suit.glyph(),
        FontId::proportional(notch.height() * 0.40),
        suit.face(),
    );

    // The big rank, in the space the notch leaves.
    p.text(
        pos2(
            rect.left() + rect.width() * 0.60,
            rect.top() + rect.height() * 0.70,
        ),
        Align2::CENTER_CENTER,
        card.rank().symbol(),
        FontId::proportional(rect.height() * 0.46),
        theme::CARD_FACE,
    );
}

/// A card, face down — which is what an **unverified** card is, always.
pub fn card_back(p: &Painter, rect: Rect) {
    let r = (rect.width() * 0.13).max(2.0);
    p.rect_filled(rect, r, theme::CARD_FACE);
    let inner = rect.shrink(rect.width() * 0.07);
    p.rect_filled(inner, r * 0.8, theme::CARD_BACK);
    let panel = inner.shrink(inner.width() * 0.16);
    p.rect_filled(panel, r * 0.6, theme::CARD_BACK_DARK);
    p.add(EllipseShape::filled(
        panel.center(),
        vec2(panel.width() * 0.28, panel.height() * 0.20),
        theme::CARD_BACK,
    ));
}

/// A stack of chips with the amount beside it, drawn inside the rectangle the
/// layout reserved — which nothing else is allowed into.
pub fn chips(p: &Painter, rect: Rect, amount: &str) {
    let r = rect.height() * 0.46;
    let at = pos2(rect.left() + r + 1.0, rect.center().y);
    for i in 0..3u8 {
        let y = at.y + r * 0.30 - i as f32 * r * 0.30;
        p.add(EllipseShape::filled(
            pos2(at.x, y),
            vec2(r, r * 0.62),
            if i == 2 { theme::MONEY } else { theme::WARN },
        ));
        p.add(EllipseShape::stroke(
            pos2(at.x, y),
            vec2(r, r * 0.62),
            Stroke::new(1.0, theme::RAIL_TOP),
        ));
    }
    p.text(
        pos2(at.x + r + 5.0, rect.center().y),
        Align2::LEFT_CENTER,
        amount,
        FontId::proportional(rect.height() * 0.84),
        theme::MONEY,
    );
}

/// A rounded plaque, used for the seat plates and the pot.
pub fn plaque(p: &Painter, rect: Rect, fill: Color32, edge: Color32) {
    p.rect_filled(rect.translate(vec2(0.0, 2.0)), rect.height() * 0.3, shadow());
    p.rect_filled(rect, rect.height() * 0.3, fill);
    p.rect_stroke(
        rect,
        rect.height() * 0.3,
        Stroke::new(1.2, edge),
        StrokeKind::Inside,
    );
}

/// The soft dark under a plaque, which is what lifts it off the felt.
fn shadow() -> Color32 {
    Color32::from_black_alpha(90)
}

/// The portrait disc. No image yet, so it carries the first letter of the name —
/// which is more use than an empty circle and never wrong.
pub fn portrait(p: &Painter, rect: Rect, name: &str, ring: Color32) {
    let c = rect.center();
    let r = rect.width() * 0.5;
    p.add(EllipseShape::filled(
        pos2(c.x, c.y + 2.0),
        vec2(r, r),
        shadow(),
    ));
    p.add(EllipseShape::filled(c, vec2(r, r), theme::PANEL_LIGHT));
    p.add(EllipseShape::stroke(c, vec2(r, r), Stroke::new(2.0, ring)));
    let initial = name.chars().next().unwrap_or('?').to_uppercase().to_string();
    p.text(
        c,
        Align2::CENTER_CENTER,
        initial,
        FontId::proportional(r * 1.05),
        theme::TEXT,
    );
}

/// The dealer button.
pub fn dealer_button(p: &Painter, rect: Rect) {
    let c = rect.center();
    let r = rect.width() * 0.5;
    p.add(EllipseShape::filled(c, vec2(r, r), theme::CARD_FACE));
    p.add(EllipseShape::stroke(
        c,
        vec2(r, r),
        Stroke::new(1.2, theme::RAIL_TOP),
    ));
    p.text(
        c,
        Align2::CENTER_CENTER,
        "D",
        FontId::proportional(r * 1.25),
        theme::RAIL_TOP,
    );
}

/// The ring of time a seat has left to act, drawn round the portrait.
///
/// An arc would be better; egui has no arc primitive, so it is a run of dots,
/// which reads the same way and needs nothing added to the dependency list.
pub fn clock(p: &Painter, rect: Rect, left: f32) {
    let c = rect.center();
    let r = rect.width() * 0.5 + 3.0;
    let colour = if left < 0.25 {
        theme::DANGER
    } else if left < 0.5 {
        theme::WARN
    } else {
        theme::OK
    };
    let dots = 24;
    let lit = (left.clamp(0.0, 1.0) * dots as f32).round() as usize;
    for i in 0..lit {
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * i as f32 / dots as f32;
        p.circle_filled(
            pos2(c.x + r * a.cos(), c.y + r * a.sin()),
            1.6,
            colour,
        );
    }
}

/// Where a text label goes for a value that must never be mistaken for a card.
pub fn centred(p: &Painter, at: Pos2, text: &str, size: f32, colour: Color32) {
    p.text(
        at,
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(size),
        colour,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bands stand in for gradients, so the ends must be the sampled colours
    /// exactly — a gradient that does not reach its own endpoints is a different
    /// colour scheme.
    #[test]
    fn a_band_reaches_both_ends() {
        assert_eq!(mix(theme::FELT_EDGE, theme::FELT_CENTRE, 0.0), theme::FELT_EDGE);
        assert_eq!(
            mix(theme::FELT_EDGE, theme::FELT_CENTRE, 1.0),
            theme::FELT_CENTRE
        );
    }

    /// And it stays between them: a blend that overshoots would put a colour on
    /// the table that was never sampled from anything.
    #[test]
    fn a_band_stays_between_its_ends() {
        for i in 0..=20 {
            let t = i as f32 / 20.0;
            let c = mix(theme::RAIL_TOP, theme::RAIL_BOTTOM, t);
            assert!(c.r() >= theme::RAIL_TOP.r() && c.r() <= theme::RAIL_BOTTOM.r());
            assert!(c.g() >= theme::RAIL_TOP.g() && c.g() <= theme::RAIL_BOTTOM.g());
        }
        // Out of range is clamped rather than extrapolated.
        assert_eq!(mix(theme::RAIL_TOP, theme::RAIL_BOTTOM, -3.0), theme::RAIL_TOP);
        assert_eq!(
            mix(theme::RAIL_TOP, theme::RAIL_BOTTOM, 9.0),
            theme::RAIL_BOTTOM
        );
    }
}
