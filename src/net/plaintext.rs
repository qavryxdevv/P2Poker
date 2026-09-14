//! What a line of chat may be made of.
//!
//! `D-054`. A chat line is the one thing at this table that is **written by
//! somebody else's client**, and a rogue peer writes whatever it likes: the
//! caps in [`say`](super::tabletalk::say) are the sender's own courtesy and
//! bind nobody. Everything here is therefore a **receiver's** rule, checked
//! against the bytes that arrived.
//!
//! The caps on *length* were already kept. What was not is what the bytes
//! **are**: a line of 256 bytes of `\n` is eighty-five empty rows in the chat
//! pane, and it costs one message of an honest budget. A right-to-left
//! override turns the rest of the pane around. Two hundred combining marks on
//! one letter draw over the whole window. None of that is text, and none of it
//! is refused by a length cap.
//!
//! So: a line is **one line of printable text**. The sender cleans its own
//! ([`to_plain_line`]) and the receiver refuses anything that is not already
//! clean ([`is_plain_line`]) -- strict, because an honest client of this build
//! cleans before it signs, so nothing honest is ever refused, and what is left
//! is a peer that wrote its own.

/// How many base characters a line may hold.
///
/// Bytes are capped too (`SAID_MAX`), but bytes are not what a pane is made
/// of: 256 bytes is 256 Latin letters and only 64 emoji, and it is the count
/// on the screen that decides whether a line fits in it.
pub const BASES_MAX: usize = 120;

/// How many combining marks may sit on one base character.
///
/// Real writing needs a few -- Thai and Arabic stack two or three, Hebrew
/// points and cantillation more -- and no writing needs a dozen. What needs a
/// dozen is a line drawn across the rest of the window.
pub const MARKS_MAX: usize = 4;

/// Whether `c` is a combining mark, for [`MARKS_MAX`].
///
/// The blocks that stack: the Latin diacritics, the marks of the scripts that
/// use them, and the three general-purpose combining blocks. Not a complete
/// Unicode category -- `char` cannot answer that without a table -- but every
/// range a stack can actually be built out of.
fn is_mark(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F   // combining diacritical marks
        | 0x0483..=0x0489 // Cyrillic
        | 0x0591..=0x05BD | 0x05BF | 0x05C1..=0x05C2 | 0x05C4..=0x05C5 | 0x05C7 // Hebrew
        | 0x0610..=0x061A | 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06DC // Arabic
        | 0x0730..=0x074A | 0x07A6..=0x07B0 | 0x07EB..=0x07F3
        | 0x0900..=0x0903 | 0x093A..=0x094F | 0x0951..=0x0957 // Devanagari
        | 0x0E31 | 0x0E34..=0x0E3A | 0x0E47..=0x0E4E // Thai
        | 0x0F71..=0x0F84 // Tibetan
        | 0x1AB0..=0x1AFF // combining diacritical marks extended
        | 0x1DC0..=0x1DFF // combining diacritical marks supplement
        | 0x20D0..=0x20F0 // combining marks for symbols
        | 0xFE20..=0xFE2F // combining half marks
    )
}

/// Whether `c` joins characters rather than being one: a zero-width joiner, a
/// zero-width non-joiner, or a variation selector.
///
/// Allowed, and **not** counted as a base: an emoji flag or a family is one
/// picture built out of several characters and a joiner between each pair, and
/// a line that refused them would refuse ordinary typing.
fn is_joiner(c: char) -> bool {
    matches!(c as u32, 0x200C | 0x200D | 0xFE0E | 0xFE0F | 0xE0020..=0xE007F)
}

/// Whether `c` is invisible and changes how what is around it is read or
/// drawn: the bidirectional overrides and isolates, the marks, the byte order
/// mark, the soft hyphen.
///
/// **A right-to-left override in a chat line reverses the pane.** It has no
/// place in one: a language written right to left is written right to left by
/// its own characters, which carry that direction themselves.
fn is_deceiving(c: char) -> bool {
    matches!(c as u32,
        0x00AD                // soft hyphen
        | 0x061C              // Arabic letter mark
        | 0x200E | 0x200F     // left-to-right / right-to-left mark
        | 0x202A..=0x202E     // embeddings and overrides
        | 0x2060..=0x2064     // word joiner and invisible operators
        | 0x2066..=0x2069     // isolates
        | 0xFEFF              // zero width no-break space
    )
}

/// Why a line is not one line of printable text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotPlain {
    /// Nothing but space, or nothing at all.
    Empty,
    /// Space at one end: the sender did not trim, so this is not what this
    /// build writes.
    Untrimmed,
    /// A control character -- a newline among them, which is what turns one
    /// message into a screenful.
    Control,
    /// A character that changes the direction or the shape of what is around
    /// it without being seen itself.
    Deceiving,
    /// More combining marks on one character than any writing uses.
    Stacked,
    /// More characters than a pane can show.
    TooManyBases,
}

/// Whether this is one line of printable text, as a receiver requires it.
pub fn is_plain_line(s: &str) -> Result<(), NotPlain> {
    if s.trim().is_empty() {
        return Err(NotPlain::Empty);
    }
    if s.trim() != s {
        return Err(NotPlain::Untrimmed);
    }
    let mut bases = 0usize;
    let mut marks = 0usize;
    for c in s.chars() {
        if c.is_control() {
            return Err(NotPlain::Control);
        }
        if is_deceiving(c) {
            return Err(NotPlain::Deceiving);
        }
        if is_mark(c) {
            marks += 1;
            if marks > MARKS_MAX {
                return Err(NotPlain::Stacked);
            }
            continue;
        }
        marks = 0;
        if is_joiner(c) {
            continue;
        }
        bases += 1;
        if bases > BASES_MAX {
            return Err(NotPlain::TooManyBases);
        }
    }
    if bases == 0 {
        // Marks and joiners and nothing to hang them on.
        return Err(NotPlain::Empty);
    }
    Ok(())
}

/// The same line, made clean: what this client says of what its own player
/// typed.
///
/// A control character becomes a space (a pasted paragraph stays one line and
/// keeps its words apart), the invisible direction characters go, a stack of
/// marks is cut to [`MARKS_MAX`], and the whole is cut to [`BASES_MAX`] bases.
/// Byte caps are applied by the caller, which knows its own.
pub fn to_plain_line(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut bases = 0usize;
    let mut marks = 0usize;
    for c in s.chars() {
        if c.is_control() {
            out.push(' ');
            marks = 0;
            continue;
        }
        if is_deceiving(c) {
            continue;
        }
        if is_mark(c) {
            // A mark with nothing to sit on is not a character, and keeping it
            // would clean a line into something the checker then refuses.
            if bases > 0 && marks < MARKS_MAX {
                marks += 1;
                out.push(c);
            }
            continue;
        }
        marks = 0;
        if is_joiner(c) {
            if bases > 0 {
                out.push(c);
            }
            continue;
        }
        if bases == BASES_MAX {
            break;
        }
        bases += 1;
        out.push(c);
    }
    // The spaces control characters left behind are still spaces: one run of
    // them, and none at either end.
    let mut clean = String::with_capacity(out.len());
    let mut space = false;
    for c in out.trim().chars() {
        if c == ' ' {
            space = true;
            continue;
        }
        if space && !clean.is_empty() {
            clean.push(' ');
        }
        space = false;
        clean.push(c);
    }
    clean
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What people actually type is text, and every bit of it is accepted.
    #[test]
    fn ordinary_writing_is_plain() {
        for line in [
            "nice hand",
            "dobrý flop, ale ten river byl brutální",
            "gg 🙂",
            "🏳️‍🌈 👨‍👩‍👧‍👦",       // joiners: one picture out of several characters
            "מה קורה",              // right to left by its own letters, no override
            "ก่อนหน้านี้",             // Thai, marks stacked as the script stacks them
            "he said \"raise\" -- 3/4 pot?",
        ] {
            assert_eq!(is_plain_line(line), Ok(()), "{line}");
            assert_eq!(to_plain_line(line), line, "cleaning does not change it: {line}");
        }
    }

    /// A newline in a message is a screenful of pane out of one line of
    /// budget, which is the cheapest way there is to push a table's chat off
    /// the screen. It is not text and it is refused.
    #[test]
    fn a_line_is_one_line() {
        assert_eq!(is_plain_line("one\ntwo"), Err(NotPlain::Control));
        assert_eq!(is_plain_line(&"\n".repeat(80)), Err(NotPlain::Empty));
        assert_eq!(is_plain_line("tabs\tare\tcontrol\ttoo"), Err(NotPlain::Control));
        // And what this client says of it stays one line, with its words apart.
        assert_eq!(to_plain_line("one\ntwo\n\nthree"), "one two three");
    }

    /// A right-to-left override reverses everything after it, so a line can be
    /// made to read as another seat's.
    #[test]
    fn the_invisible_direction_characters_are_refused() {
        assert_eq!(is_plain_line("hello \u{202E}dlrow"), Err(NotPlain::Deceiving));
        assert_eq!(is_plain_line("\u{2066}isolated\u{2069}"), Err(NotPlain::Deceiving));
        assert_eq!(is_plain_line("soft\u{00AD}hyphen"), Err(NotPlain::Deceiving));
        assert_eq!(to_plain_line("hello \u{202E}dlrow"), "hello dlrow");
    }

    /// Two hundred marks on one letter draw over the rest of the window.
    #[test]
    fn a_stack_of_marks_is_refused_and_cut() {
        let zalgo = format!("a{}", "\u{0301}".repeat(60));
        assert_eq!(is_plain_line(&zalgo), Err(NotPlain::Stacked));
        let cut = to_plain_line(&zalgo);
        assert_eq!(cut.chars().count(), 1 + MARKS_MAX, "the base and its four marks");
        assert_eq!(is_plain_line(&cut), Ok(()), "and what is cut is then plain");
    }

    /// The count that decides whether a line fits a pane is characters, not
    /// bytes: 120 emoji are 480 bytes and 120 letters are 120.
    #[test]
    fn a_line_is_bounded_in_characters_as_well_as_bytes() {
        let many = "x".repeat(BASES_MAX + 1);
        assert_eq!(is_plain_line(&many), Err(NotPlain::TooManyBases));
        assert_eq!(to_plain_line(&many).chars().count(), BASES_MAX);
        assert_eq!(is_plain_line(&"x".repeat(BASES_MAX)), Ok(()));
    }

    /// Space at an end is not what this build signs, so a receiver takes it as
    /// somebody else's client -- and this client's own cleaning removes it.
    #[test]
    fn a_line_is_trimmed_and_not_empty() {
        assert_eq!(is_plain_line(" padded "), Err(NotPlain::Untrimmed));
        assert_eq!(is_plain_line("   "), Err(NotPlain::Empty));
        assert_eq!(is_plain_line("\u{200D}"), Err(NotPlain::Empty), "joiners alone are nothing");
        assert_eq!(to_plain_line("  padded  "), "padded");
    }

    /// Whatever is thrown at the cleaner, what comes out is accepted by the
    /// checker or is empty -- the two halves are one rule.
    #[test]
    fn what_this_client_cleans_is_what_a_receiver_accepts() {
        for line in [
            "one\ntwo",
            " padded ",
            &"\u{0301}".repeat(9),
            &format!("a{}", "\u{0301}".repeat(60)),
            &"x".repeat(BASES_MAX * 2),
            "\u{202E}reversed",
            "   ",
            "\u{FEFF}",
        ] {
            let clean = to_plain_line(line);
            if clean.is_empty() {
                continue;
            }
            assert_eq!(is_plain_line(&clean), Ok(()), "cleaned {line:?} into {clean:?}");
        }
    }
}
