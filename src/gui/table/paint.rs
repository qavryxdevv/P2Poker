//! The cards: the one thing the table draws exactly as it always did.
//!
//! The owner, 2026-09-13, when the table took PokerTH's Green Casino look:
//! *leave the look of the playing cards completely unchanged -- do not recolour
//! them or remake them in the table's green and gold*. So these two functions
//! are the old window's, byte for byte; everything else the old painter drew
//! (the felt, the rail, the plates, the chips) went with the old look, and
//! the table's pieces are drawn by [`style`](super::style).
//!
//! # The one rule that is not decoration
//!
//! `SPEC_CS.md` §22: **never display a cryptographically unverified card as
//! valid.** [`Facing`](super::Facing) has no public constructor that produces a
//! face-up card without a verification verdict, so a card whose proof has not
//! been checked is drawn as a back — not as a mistake that could be made, but as
//! the only thing the type can become.

use eframe::egui::{epaint::EllipseShape, pos2, vec2, Align2, FontId, Painter, Rect, Stroke, StrokeKind};

use crate::gui::theme::{self, SuitColour};
use crate::poker::state::Card;

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

