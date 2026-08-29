//! The palette.
//!
//! Two sources, and they answer different questions.
//!
//! **The lobby's colours are the Python client's**, ported from
//! `p2p_poker/gui/theme.py`. The first version of this file used the table
//! photograph's palette for the lobby too, and the result was nearly unreadable:
//! a near-black window with grey-on-grey text, no panel separation and no line
//! colour, because the photograph is of green felt and has nothing to say about
//! how a list of tables should look. The Python client had already solved that
//! and the answer is imported rather than re-derived.
//!
//! **The table's colours stay sampled from the reference image**
//! (`docs/research/GUI_STACK.md` measured them off
//! `assets/ggpoker-rush-and-cash-table.jpg`), because that *is* a question about
//! how felt and a wooden rail look, and a measurement beats a preference.
//!
//! # The four-colour deck is not decoration
//!
//! Two black suits at a glance is how a player misreads a flush. Clubs and
//! spades here differ by a whole hue, and a test says so.

use eframe::egui::Color32;

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb(
        ((hex >> 16) & 0xFF) as u8,
        ((hex >> 8) & 0xFF) as u8,
        (hex & 0xFF) as u8,
    )
}

// ---------------------------------------------------------------------------
// The lobby, from the Python client
// ---------------------------------------------------------------------------

/// The window behind everything.
pub const WINDOW: Color32 = rgb(0x12181D);
/// A panel: the three columns and the network strip sit on this.
pub const PANEL: Color32 = rgb(0x1A2228);
/// A raised panel — headers, buttons, the selected row.
pub const PANEL_LIGHT: Color32 = rgb(0x243038);
/// The inside of a list or a text field, darker than the panel it sits in.
pub const FIELD: Color32 = rgb(0x0E1418);
/// Every other row of a list.
pub const FIELD_ALT: Color32 = rgb(0x121A1F);
/// A selected row.
pub const SELECTED: Color32 = rgb(0x1D3F5C);
/// Borders and separators. Without one, panels do not read as panels.
pub const LINE: Color32 = rgb(0x2C3A44);

/// Ordinary text. Bright enough to read on [`PANEL`], which the first version's
/// was not.
pub const TEXT: Color32 = rgb(0xE8EEF2);
/// Secondary text: column headings, units, anything the eye should skip.
pub const TEXT_DIM: Color32 = rgb(0x9FB0BB);

/// The one colour that draws the eye, used for the primary action and nothing
/// else.
pub const ACCENT: Color32 = rgb(0x4EA3FF);
/// A chip count.
pub const STACK: Color32 = rgb(0x7EC9FF);
/// Money.
pub const MONEY: Color32 = rgb(0xFFD35C);

pub const OK: Color32 = rgb(0x3FAE6A);
pub const WARN: Color32 = rgb(0xE0A03A);
pub const DANGER: Color32 = rgb(0xD33B3B);

// ---------------------------------------------------------------------------
// The table, sampled from the reference photograph
// ---------------------------------------------------------------------------

/// Felt, from the sampled radial profile: darkest at the edge, brightest at the
/// highlight near the centre.
pub const FELT_EDGE: Color32 = rgb(0x00411E);
pub const FELT_MID: Color32 = rgb(0x004A24);
pub const FELT_CENTRE: Color32 = rgb(0x006C3A);
/// The thin dark keyline where the wood meets the felt.
pub const FELT_KEYLINE: Color32 = rgb(0x0D4A2A);

/// The rail, lit from below in the reference: dark at the top, a warm band at
/// the bottom.
pub const RAIL_TOP: Color32 = rgb(0x1C1311);
pub const RAIL_SIDE: Color32 = rgb(0x8B5A4C);
pub const RAIL_BOTTOM: Color32 = rgb(0xCBAF94);
pub const RAIL_OUTER_EDGE: Color32 = rgb(0x5B4035);
pub const RAIL_INNER_EDGE: Color32 = rgb(0x6B452B);
/// The engraved lettering on the bottom rail, darker than the rail it sits on.
pub const RAIL_LETTERING: Color32 = rgb(0x8B6A55);

// ---------------------------------------------------------------------------
// Cards
// ---------------------------------------------------------------------------

/// The four suits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuitColour {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

impl SuitColour {
    /// The suit's face colour, from the Python client's four-colour deck.
    pub const fn face(self) -> Color32 {
        match self {
            SuitColour::Clubs => rgb(0x1F9D55),
            SuitColour::Diamonds => rgb(0x1668D0),
            SuitColour::Hearts => rgb(0xD92B2B),
            SuitColour::Spades => rgb(0x20242B),
        }
    }

    /// The lighter end of the top-to-bottom gradient, from the reference image.
    pub const fn face_light(self) -> Color32 {
        match self {
            SuitColour::Clubs => rgb(0x0A8210),
            SuitColour::Diamonds => rgb(0x0A50A0),
            SuitColour::Hearts => rgb(0xAD0A0B),
            SuitColour::Spades => rgb(0x2A2A2A),
        }
    }

    /// The glyph, so the suit is legible without colour at all — which matters
    /// for the eight per cent of men who cannot rely on it.
    pub const fn glyph(self) -> &'static str {
        match self {
            SuitColour::Clubs => "\u{2663}",
            SuitColour::Diamonds => "\u{2666}",
            SuitColour::Hearts => "\u{2665}",
            SuitColour::Spades => "\u{2660}",
        }
    }

    /// From the canonical suit index of [`crate::poker::state::Card`].
    pub const fn from_index(suit: u8) -> Option<SuitColour> {
        match suit {
            0 => Some(SuitColour::Clubs),
            1 => Some(SuitColour::Diamonds),
            2 => Some(SuitColour::Hearts),
            3 => Some(SuitColour::Spades),
            _ => None,
        }
    }
}

/// The face of a card.
pub const CARD_FACE: Color32 = rgb(0xF7F9FB);

/// The back of a card, which is what an **unverified** card is drawn as —
/// always. `SPEC_CS.md` §22: never display a cryptographically unverified card
/// as valid.
pub const CARD_BACK: Color32 = rgb(0xC0392B);
pub const CARD_BACK_DARK: Color32 = rgb(0x7B1F16);

/// How far apart two colours are, as the sum of the channel differences.
///
/// Not a perceptual metric and not claimed to be one. It is enough to catch the
/// failure this file has already had once — text and background so close that
/// the result is unreadable — and a real perceptual model would be a dependency
/// for a check that only has to answer *is this obviously too close*.
pub fn separation(a: Color32, b: Color32) -> u32 {
    let d = |x: u8, y: u8| (x as i32 - y as i32).unsigned_abs();
    d(a.r(), b.r()) + d(a.g(), b.g()) + d(a.b(), b.b())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The failure this palette was replaced over: text the same shade as what
    /// it sits on.
    ///
    /// Every pairing that actually occurs on screen is checked, because the
    /// first version was legible in isolation and not in combination.
    #[test]
    fn every_text_colour_is_legible_on_every_surface_it_sits_on() {
        let surfaces = [
            ("window", WINDOW),
            ("panel", PANEL),
            ("panel light", PANEL_LIGHT),
            ("field", FIELD),
            ("field alt", FIELD_ALT),
            ("selected", SELECTED),
        ];
        let inks = [
            ("text", TEXT),
            ("dim", TEXT_DIM),
            ("accent", ACCENT),
            ("stack", STACK),
            ("money", MONEY),
            ("ok", OK),
            ("warn", WARN),
            ("danger", DANGER),
        ];

        for (sname, surface) in surfaces {
            for (iname, ink) in inks {
                let gap = separation(surface, ink);
                assert!(
                    gap >= 150,
                    "{iname} on {sname} is {gap} apart; the version this replaced \
                     failed here and the client was unreadable"
                );
            }
        }
    }

    /// The surfaces must be distinguishable from each other too, or the panels
    /// do not read as panels and the whole layout collapses into one field of
    /// grey.
    #[test]
    fn the_surfaces_are_distinguishable() {
        assert!(separation(WINDOW, PANEL) >= 15);
        assert!(separation(PANEL, PANEL_LIGHT) >= 20);
        assert!(separation(PANEL, FIELD) >= 15);
        assert!(separation(FIELD, FIELD_ALT) >= 8, "alternating rows");
        assert!(separation(PANEL, LINE) >= 20, "a border must be visible");
        assert!(separation(FIELD, SELECTED) >= 40, "a selection must be obvious");
    }

    /// Two black suits at a glance is how a player misreads a flush.
    #[test]
    fn every_suit_is_a_different_colour() {
        let all = [
            SuitColour::Clubs,
            SuitColour::Diamonds,
            SuitColour::Hearts,
            SuitColour::Spades,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert!(
                    separation(a.face(), b.face()) >= 60,
                    "{a:?} and {b:?} are too close"
                );
            }
        }
    }

    /// And clubs against spades by a hue, not a shade — the pair a two-colour
    /// deck confuses.
    #[test]
    fn clubs_and_spades_differ_by_a_hue_and_not_a_shade() {
        let c = SuitColour::Clubs.face();
        let s = SuitColour::Spades.face();
        assert!(
            c.g() as i32 - s.g() as i32 > 60,
            "clubs are green and spades are near-black"
        );
    }

    /// Colour is not the only channel. A glyph carries the suit for a player who
    /// cannot rely on hue, and every suit has a distinct one.
    #[test]
    fn the_suit_survives_without_colour() {
        let glyphs: Vec<&str> = [
            SuitColour::Clubs,
            SuitColour::Diamonds,
            SuitColour::Hearts,
            SuitColour::Spades,
        ]
        .iter()
        .map(|s| s.glyph())
        .collect();
        let mut sorted = glyphs.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 4, "two suits share a glyph: {glyphs:?}");
    }

    /// Every card face is legible on the card, which is a different surface from
    /// any panel.
    #[test]
    fn every_suit_is_legible_on_a_card() {
        for s in [
            SuitColour::Clubs,
            SuitColour::Diamonds,
            SuitColour::Hearts,
            SuitColour::Spades,
        ] {
            assert!(
                separation(CARD_FACE, s.face()) >= 200,
                "{s:?} is not legible on a card face"
            );
        }
    }

    /// The suit index is `poker::state::Card`'s, so the colours follow the
    /// canonical encoding rather than a second ordering.
    #[test]
    fn the_suit_index_is_the_canonical_one() {
        assert_eq!(SuitColour::from_index(0), Some(SuitColour::Clubs));
        assert_eq!(SuitColour::from_index(1), Some(SuitColour::Diamonds));
        assert_eq!(SuitColour::from_index(2), Some(SuitColour::Hearts));
        assert_eq!(SuitColour::from_index(3), Some(SuitColour::Spades));
        assert_eq!(SuitColour::from_index(4), None);
    }

    /// The felt gradient runs dark at the edge to bright at the centre, which is
    /// the sampled profile and not a guess.
    #[test]
    fn the_felt_brightens_towards_the_middle() {
        let sum = |c: Color32| c.r() as u32 + c.g() as u32 + c.b() as u32;
        assert!(sum(FELT_EDGE) < sum(FELT_MID));
        assert!(sum(FELT_MID) < sum(FELT_CENTRE));
    }

    /// The rail is lit from below.
    #[test]
    fn the_rail_is_lit_from_below() {
        let sum = |c: Color32| c.r() as u32 + c.g() as u32 + c.b() as u32;
        assert!(sum(RAIL_TOP) < sum(RAIL_SIDE));
        assert!(sum(RAIL_SIDE) < sum(RAIL_BOTTOM));
    }

    /// The lobby does not share the felt's colour, so two windows do not read as
    /// one surface.
    #[test]
    fn the_lobby_is_not_the_table() {
        assert!(separation(WINDOW, FELT_EDGE) >= 40);
        assert!(separation(PANEL, FELT_CENTRE) >= 40);
    }
}
