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
