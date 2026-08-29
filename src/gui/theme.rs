//! The palette, sampled from the reference image rather than chosen.
//!
//! `docs/research/GUI_STACK.md` measured every colour here off
//! `assets/ggpoker-rush-and-cash-table.jpg` — the felt's radial gradient, the
//! rail's three faces, and the four-colour deck. They are constants and not
//! literals scattered through the drawing code, so that a change to the look is
//! a change in one place and a drift between two panes is impossible.
//!
//! # The four-colour deck is not decoration
//!
//! Two black suits at a glance is how a player misreads a flush. The reference
//! image uses four, the sampled values are below, and the difference between
//! clubs and spades here is a whole hue rather than a shade.

use eframe::egui::Color32;

/// Felt, from the sampled radial profile: darkest at the edge, brightest at the
/// highlight around the centre.
pub const FELT_EDGE: Color32 = Color32::from_rgb(0x00, 0x41, 0x1E);
pub const FELT_MID: Color32 = Color32::from_rgb(0x00, 0x4A, 0x24);
pub const FELT_CENTRE: Color32 = Color32::from_rgb(0x00, 0x6C, 0x3A);
/// The thin dark keyline where the wood meets the felt.
pub const FELT_KEYLINE: Color32 = Color32::from_rgb(0x0D, 0x4A, 0x2A);

/// The rail, which is lit from below in the reference: dark at the top, a warm
/// band at the bottom.
pub const RAIL_TOP: Color32 = Color32::from_rgb(0x1C, 0x13, 0x11);
pub const RAIL_SIDE: Color32 = Color32::from_rgb(0x8B, 0x5A, 0x4C);
pub const RAIL_BOTTOM: Color32 = Color32::from_rgb(0xCB, 0xAF, 0x94);
pub const RAIL_OUTER_EDGE: Color32 = Color32::from_rgb(0x5B, 0x40, 0x35);
pub const RAIL_INNER_EDGE: Color32 = Color32::from_rgb(0x6B, 0x45, 0x2B);
/// The engraved lettering on the bottom rail, darker than the rail it sits on.
pub const RAIL_LETTERING: Color32 = Color32::from_rgb(0x8B, 0x6A, 0x55);

/// The lobby's own surface, kept away from the felt so the two windows do not
/// read as one.
pub const LOBBY_BG: Color32 = Color32::from_rgb(0x1A, 0x1A, 0x1D);
pub const LOBBY_ROW: Color32 = Color32::from_rgb(0x22, 0x22, 0x26);
pub const LOBBY_ROW_ALT: Color32 = Color32::from_rgb(0x1E, 0x1E, 0x22);
pub const LOBBY_TEXT: Color32 = Color32::from_rgb(0xE6, 0xE6, 0xE6);
pub const LOBBY_DIM: Color32 = Color32::from_rgb(0x90, 0x90, 0x96);

/// A table this client will not sit at, and the colour says so before the text
/// does.
pub const WARNING: Color32 = Color32::from_rgb(0xD8, 0x8A, 0x2A);
pub const REFUSED: Color32 = Color32::from_rgb(0xC0, 0x3A, 0x3A);
pub const GOOD: Color32 = Color32::from_rgb(0x3A, 0xA8, 0x5E);

/// The four suits, sampled. Rank and pip are white on all four.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuitColour {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

impl SuitColour {
    /// The dominant sampled colour of the suit's face.
    pub const fn face(self) -> Color32 {
        match self {
            SuitColour::Clubs => Color32::from_rgb(0x03, 0x69, 0x05),
            SuitColour::Diamonds => Color32::from_rgb(0x04, 0x3E, 0x7B),
            SuitColour::Hearts => Color32::from_rgb(0x96, 0x06, 0x07),
            SuitColour::Spades => Color32::from_rgb(0x13, 0x13, 0x13),
        }
    }

    /// The lighter end of the sampled top-to-bottom gradient.
    pub const fn face_light(self) -> Color32 {
        match self {
            SuitColour::Clubs => Color32::from_rgb(0x0A, 0x82, 0x10),
            SuitColour::Diamonds => Color32::from_rgb(0x0A, 0x50, 0xA0),
            SuitColour::Hearts => Color32::from_rgb(0xAD, 0x0A, 0x0B),
            SuitColour::Spades => Color32::from_rgb(0x2A, 0x2A, 0x2A),
        }
    }

    /// From the canonical suit index of `poker::state::Card`.
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

/// Rank and pip, on every suit.
pub const CARD_INK: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);

/// The back of a card, which is what an unverified card is drawn as — always.
pub const CARD_BACK: Color32 = Color32::from_rgb(0x7A, 0x1F, 0x2B);

#[cfg(test)]
mod tests {
    use super::*;

    /// Two black suits at a glance is how a player misreads a flush. The
    /// four-colour deck is the reference image's and it is kept.
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
                assert_ne!(a.face(), b.face(), "{a:?} and {b:?} share a colour");
            }
        }
    }

    /// And clubs against spades by more than a shade, since those are the two
    /// that a two-colour deck confuses.
    #[test]
    fn clubs_and_spades_differ_by_a_hue_and_not_a_shade() {
        let c = SuitColour::Clubs.face();
        let s = SuitColour::Spades.face();
        let green_gap = c.g() as i32 - s.g() as i32;
        assert!(
            green_gap > 60,
            "clubs are green and spades are black; the gap is {green_gap}"
        );
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

    /// The lit gradient runs light to dark on every suit, which is what the
    /// reference does and what keeps a card from reading as flat.
    #[test]
    fn each_suit_has_a_lighter_top() {
        for s in [
            SuitColour::Clubs,
            SuitColour::Diamonds,
            SuitColour::Hearts,
            SuitColour::Spades,
        ] {
            let light: u32 =
                s.face_light().r() as u32 + s.face_light().g() as u32 + s.face_light().b() as u32;
            let dark: u32 = s.face().r() as u32 + s.face().g() as u32 + s.face().b() as u32;
            assert!(light > dark, "{s:?} is not lit from the top");
        }
    }

    /// The felt gradient runs dark at the edge to bright at the centre, which is
    /// the sampled profile and not a guess.
    #[test]
    fn the_felt_brightens_towards_the_middle() {
        let sum = |c: Color32| c.r() as u32 + c.g() as u32 + c.b() as u32;
        assert!(sum(FELT_EDGE) < sum(FELT_MID));
        assert!(sum(FELT_MID) < sum(FELT_CENTRE));
    }

    /// The rail is lit from below: dark at the top, a warm band at the bottom.
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
        assert_ne!(LOBBY_BG, FELT_EDGE);
        assert_ne!(LOBBY_BG, FELT_CENTRE);
    }
}
