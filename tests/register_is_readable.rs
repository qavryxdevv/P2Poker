//! The finding register's queue must be readable by machine, not by eye.
//!
//! # What this exists for
//!
//! `DECISIONS.md`'s `S1-` table is the project's list of open faults, and until
//! 2026-09-06 the only way to ask *"what is still open?"* was to read eighty-two
//! rows of prose. The verdict had always been free text and the vocabulary had
//! drifted — `closed`, `fixed`, `open`, `owed`, `not built`, `measured, not
//! fixed`, and **fifteen rows carried no verdict word at all**. A grep for
//! *open* was worse than useless: it counted `S1-H`, which reads **closed**,
//! because its sources column names the function `hand_one_may_open`.
//!
//! So every row's last column now opens with one of five tokens, and this test
//! is what keeps that true on a machine that has never seen this conversation.
//! It is the same shape as `corpus_constants.rs`: a convention nobody has to
//! remember, because the build fails when it is broken.
//!
//! # The second check, and it is the one that found something
//!
//! A markdown table row is split on **unescaped** pipes, so a Rust operator or
//! closure written into the prose — `past_deadline || long_past_stage`,
//! `.map(|ttl| …)`, `held = |voters| + 1`, `return sent > 0 || …` — silently
//! turns a three-column row into five. Four rows were malformed that way, and
//! `S1-AQ` had been rendering as four columns with **no sources column at all**
//! for long enough that nobody noticed. Escaping is the fix; this test is what
//! makes the next one impossible to miss.
//!
//! # The third check, and this one the first two could not see
//!
//! Every row is edited by anchoring on a substring and replacing it. On
//! 2026-09-02 an anchor ending in a section number's full stop matched inside
//! `6.1`, so an inserted correction landed **in the middle of a token**: `S1-V`
//! read *"which is a 6."* and the rest of that sentence -- *"1 wire change and
//! pre-empts..."* -- ended up four hundred words later, glued to *"belongs to
//! the owner."*. The row still had three columns and balanced pipes, so both
//! checks above passed it for five days.
//!
//! The tell is a full stop with a letter before it and a digit after:
//! `owner.1`. This register's prose does not produce that -- a section
//! reference has a digit before the stop and a version has a digit on both
//! sides -- so it is a cheap and specific signature of a splice landing
//! mid-token. Zero occurrences in the register as it stands, which is what
//! makes it usable as a rule rather than as a warning.

use std::fs;

/// The five tokens, and the line between them.
///
/// `OWNER` the next move is a decision of the owner's, not a change to this
/// tree. `OPEN` a fault that is not fixed. `OWED` the fix is in and a run or a
/// re-measurement is owed before it is accepted. `FIXED` built, landed, nothing
/// owed. `CLOSED` disposed of without a change here — by scope, by analysis, or
/// superseded.
const TOKENS: [&str; 5] = ["OWNER:", "OPEN:", "OWED:", "FIXED:", "CLOSED:"];

/// Split a markdown table row on its real column separators.
///
/// `\|` inside a code span is a literal pipe and not a separator, which is what
/// four rows of this register get wrong and what this whole file is for.
fn cells(row: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut escaped = false;
    for c in row.chars() {
        match c {
            '\\' if !escaped => escaped = true,
            '|' if !escaped => out.push(String::new()),
            _ => {
                if escaped {
                    out.last_mut().expect("a cell").push('\\');
                    escaped = false;
                }
                out.last_mut().expect("a cell").push(c);
            }
        }
    }
    out
}

fn rows() -> Vec<String> {
    let text = fs::read_to_string("docs/DECISIONS.md").expect("the register");
    text.lines()
        .filter(|l| l.starts_with("| S1-"))
        .map(str::to_owned)
        .collect()
}

/// **Every row is three columns.** A stray pipe in the prose makes a row that
/// renders wrong and that every tool reading this file mis-parses, including
/// the one below.
#[test]
fn every_finding_row_has_exactly_three_columns() {
    let bad: Vec<(String, usize)> = rows()
        .iter()
        .map(|r| (r.split('|').nth(1).unwrap_or("?").trim().to_owned(), cells(r).len()))
        // a `| a | b | c |` row splits into five: "", a, b, c, ""
        .filter(|(_, n)| *n != 5)
        .collect();
    assert!(
        bad.is_empty(),
        "rows whose column count is not three — a literal `|` in the prose must be \
         written `\\|`: {bad:?}"
    );
}

/// **Every row's last column opens with a status token**, so the queue is
/// `grep -oE "^\\| S1-[A-Z]+ \\|.*\\| (OPEN|OWED|OWNER):" docs/DECISIONS.md`
/// and not a reading exercise.
#[test]
fn every_finding_row_declares_its_status() {
    let mut missing: Vec<String> = Vec::new();
    for r in rows() {
        let c = cells(&r);
        let key = c.get(1).map(|s| s.trim().to_owned()).unwrap_or_default();
        let last = c
            .get(c.len().saturating_sub(2))
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();
        if !TOKENS.iter().any(|t| last.starts_with(t)) {
            missing.push(key);
        }
    }
    assert!(
        missing.is_empty(),
        "rows whose last column does not open with one of {TOKENS:?}: {missing:?}"
    );
}

/// A splice that landed inside a token, which the two checks above cannot see.
///
/// See the module note: an anchor ending in a section number's full stop
/// matched inside `6.1` and cut a sentence in half, leaving
/// `owner.1 wire change ...` stranded elsewhere in the row.
///
/// **To make this fail**: put `owner.1 wire` back into `S1-V`. It was run.
#[test]
fn no_row_carries_a_splice_that_landed_inside_a_token() {
    let mut bad: Vec<String> = Vec::new();
    for r in rows() {
        let c = cells(&r);
        let key = c.get(1).map(|s| s.trim().to_owned()).unwrap_or_default();
        let chars: Vec<char> = r.chars().collect();
        for w in chars.windows(3) {
            if w[0].is_ascii_alphabetic() && w[1] == '.' && w[2].is_ascii_digit() {
                let at: String = w.iter().collect();
                bad.push(format!("{key} ({at})"));
                break;
            }
        }
    }
    assert!(
        bad.is_empty(),
        "rows with a full stop between a letter and a digit, which is what an \
         edit anchored inside a token looks like: {bad:?}"
    );
}

/// The register is the queue, so an empty one would mean the project is
/// finished. It is not, and a test that passes on an empty file is a test that
/// passes when the file moves.
#[test]
fn the_register_is_where_it_says_it_is() {
    let all = rows();
    assert!(
        all.len() >= 80,
        "only {} `S1-` rows found — has the register moved?",
        all.len()
    );
}
