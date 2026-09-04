//! Every cross-document section reference in the corpus, against the sections
//! that exist.
//!
//! # Why this is a test and not a one-off sweep
//!
//! `DECISIONS.md` D-011 rule 1 is the corpus's answer to two copies of a value
//! drifting apart: name one owner and point at it from everywhere else. The
//! corpus follows it carefully, and it has been swept for duplicates more than
//! once.
//!
//! **The failure that rule cannot catch is the opposite one**: a document
//! deletes its copy, points at the owner, and the owner does not have it. There
//! is nothing to compare, so a duplicate sweep sees nothing; and the reader is
//! sent somewhere there is nothing. `DECISIONS.md`'s `S1-B` is the first
//! instance found by hand — the seed-to-button rule, which three documents each
//! name another as the owner of and none writes.
//!
//! A pointer to a section that does not exist is the half of that class a
//! machine can settle, so it is settled here on every `cargo test`.
//!
//! # What it does not check
//!
//! That the section it points at actually contains the thing. That needs a
//! reader. This catches the cheaper case — the address is wrong — and the
//! allow-list below carries the ones a reader has already looked at.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The documents that are current. Everything under `research/` is a dated
/// record of what was true at a phase gate; a stale reference inside one is
/// history, and repairing it would be rewriting the record.
const LIVE: &[&str] = &[
    "PROTOCOL.md",
    "STATE_MACHINE.md",
    "CRYPTOGRAPHY.md",
    "NETWORK_STACK.md",
    "THREAT_MODEL.md",
    "DEPENDENCIES.md",
    "CONTRIBUTING.md",
    "DECISIONS.md",
    "SPEC_CS.md",
];

/// References a reader has looked at, with what is wrong and who owns the fix.
///
/// Every entry is a real defect and is on `DECISIONS.md`'s open list as `S1-C`.
/// They are listed rather than fixed because choosing the right number is a
/// documentation decision — for two of them the cited content is not in the
/// cited document at all, so there is no number to correct it to.
const KNOWN: &[(&str, &str)] = &[
    // **Empty, and that is the point of keeping it.** It held three references
    // that pointed at sections nobody had written, listed rather than fixed
    // because each needed an editorial decision. All three are made:
    //
    //   `NETWORK_STACK.md` §12.12 - three decision rows meant §12, which
    //   exists; and the two prose citers claimed "no two-network test has been
    //   run", which `NEXT.md` had already disproved. Renumbered where it was a
    //   number and rewritten where it was a claim.
    //
    //   `CRYPTOGRAPHY.md` §3.2 - `PROTOCOL.md` §2.8 sent its BLAKE3 rationale
    //   there under D-011 rule 1. The content is in §1's summary table, §9's
    //   library table and OQ-6, so the citations point there.
    //
    //   `CRYPTOGRAPHY.md` §4.7 - `THREAT_MODEL.md` X4 named it as the home of
    //   the canonicality gate. `research/CRYPTO_LIBS.md` §4.7 is titled *The
    //   canonicality gate*; the number was right and the document was a slip.
];

fn docs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("docs")
}

fn read_all() -> BTreeMap<String, String> {
    fn walk(dir: &Path, out: &mut BTreeMap<String, String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "md") {
                if let (Some(name), Ok(text)) = (
                    p.file_name().and_then(|n| n.to_str()),
                    std::fs::read_to_string(&p),
                ) {
                    out.insert(name.to_string(), text);
                }
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(&docs_dir(), &mut out);
    out
}

/// The section numbers a document actually has.
///
/// **`SPEC_CS.md` is the exception and it matters.** It is the original
/// specification and numbers its sections as plain `N. Title` lines with no
/// markdown heading, so a heading-only sweep reads it as having none — and then
/// all four hundred and eighty-nine references into it look dangling. A sweep
/// that reports that many defects in a corpus this carefully kept is a sweep
/// that is wrong, which is how this was caught before anything was reported.
fn sections(name: &str, text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in text.lines() {
        let num = if let Some(rest) = line.strip_prefix("##") {
            let rest = rest.trim_start_matches('#').trim_start();
            let rest = rest.strip_prefix('\u{a7}').unwrap_or(rest);
            leading_number(rest)
        } else if name == "SPEC_CS.md" {
            leading_number(line).filter(|n| !n.contains('.'))
        } else {
            None
        };
        let Some(num) = num else { continue };
        // A subsection makes its parents addressable.
        let parts: Vec<&str> = num.split('.').collect();
        for i in 1..=parts.len() {
            out.insert(parts[..i].join("."));
        }
    }
    out
}

/// `7`, `7.9`, `12.12` — the number at the start of a heading, if there is one.
fn leading_number(s: &str) -> Option<String> {
    let head: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let head = head.trim_end_matches('.');
    if head.is_empty() || !head.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some(head.to_string())
}

/// Every `` `DOC.md` §N `` in one line.
///
/// **Tight on purpose.** A bare section mark in this corpus usually means *this*
/// document's section, and a loose window let one pair with whatever document
/// name happened to stand a few words earlier. That produced twenty-three hits
/// of which most were exactly that. The mark must follow the name immediately,
/// with nothing between but a possessive.
fn references(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(open) = line[from..].find('`') {
        let open = from + open;
        let Some(close) = line[open + 1..].find('`') else {
            break;
        };
        let close = open + 1 + close;
        let name = &line[open + 1..close];
        from = close + 1;
        if !name.ends_with(".md") || !name.starts_with(|c: char| c.is_ascii_uppercase()) {
            continue;
        }
        let mut at = close + 1;
        if line[at..].starts_with("'s") {
            at += 2;
        }
        if !line[at..].starts_with(' ') {
            continue;
        }
        at += 1;
        if !line[at..].starts_with('\u{a7}') {
            continue;
        }
        at += '\u{a7}'.len_utf8();
        let _ = bytes;
        let Some(num) = leading_number(&line[at..]) else {
            continue;
        };
        out.push((name.to_string(), num));
    }
    out
}

/// **No live document may point at a section that does not exist.**
#[test]
fn every_cross_reference_lands_somewhere() {
    let files = read_all();
    assert!(
        files.len() > 20,
        "found {} documents, which is not the corpus",
        files.len()
    );

    let have: BTreeMap<&String, BTreeSet<String>> = files
        .iter()
        .map(|(n, t)| (n, sections(n, t)))
        .collect();

    // **The guard the 489-false-positive run needed.** A document read as having
    // no sections is a parser that does not understand it, not a document
    // without any — and then every reference into it is reported.
    //
    // It applies to the documents that are *pointed at*, which is not the live
    // corpus: `DECISIONS.md` numbers its entries `D-001` and carries no section
    // marks at all, so it is a source of references and never a target. The
    // guard learned that the first time it ran, which is the argument for
    // deriving the set rather than listing it.
    let mut targeted: BTreeSet<String> = BTreeSet::new();
    for text in files.values() {
        for line in text.lines() {
            for (target, _) in references(line) {
                targeted.insert(target);
            }
        }
    }
    for name in &targeted {
        let Some(theirs) = have.get(name) else { continue };
        assert!(
            !theirs.is_empty(),
            "{name} is pointed at with a section mark and parsed as having no \
             numbered sections, so this test would report every reference into \
             it. Teach `sections` its heading style before trusting any result."
        );
    }

    let known: BTreeSet<(String, String)> = KNOWN
        .iter()
        .map(|(d, s)| ((*d).to_string(), (*s).to_string()))
        .collect();

    let mut fresh: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for (name, text) in &files {
        if !LIVE.contains(&name.as_str()) {
            continue;
        }
        for (line_no, line) in text.lines().enumerate() {
            for (target, num) in references(line) {
                let Some(theirs) = have.get(&target) else {
                    continue;
                };
                if theirs.is_empty() {
                    continue;
                }
                checked += 1;
                if theirs.contains(&num) {
                    continue;
                }
                if known.contains(&(target.clone(), num.clone())) {
                    continue;
                }
                fresh.push(format!(
                    "{name}:{} points at {target} \u{a7}{num}, which does not exist\n    {}",
                    line_no + 1,
                    line.trim()
                ));
            }
        }
    }

    // **A green run must have been a run.** The reference syntax is tight, so a
    // small edit to `references` can quietly stop matching and leave a test that
    // passes by checking nothing. The corpus carried well over a thousand
    // cross-document references when this floor was set; it is set low enough
    // that ordinary editing will not trip it and high enough that a parser that
    // has stopped working will.
    assert!(
        checked > 800,
        "only {checked} cross-document references matched, which is not this          corpus. `references` has probably stopped parsing something it used to          - a green result from here would mean nothing."
    );

    assert!(
        fresh.is_empty(),
        "{} cross-reference(s) in the live corpus point at a section that is not \
         there. Either the number is wrong or the section was never written - and \
         the second is the shape of DECISIONS.md's S1-B, where every document \
         named another as the owner and none wrote the value.\n\n{}",
        fresh.len(),
        fresh.join("\n")
    );
}

/// The allow-list is a record of open defects, not a place to put new ones.
///
/// An entry that starts to resolve means somebody fixed the document; leaving it
/// listed would keep an `S1-C` row open against a defect that is gone, which is
/// the same stale-record failure `CRYPTOGRAPHY.md`'s own rule warns about — *a
/// claim that was false when made and is true now must not be left reading as
/// evidence that the check was once run*.
#[test]
fn the_allow_list_holds_only_live_defects() {
    let files = read_all();
    let have: BTreeMap<&String, BTreeSet<String>> = files
        .iter()
        .map(|(n, t)| (n, sections(n, t)))
        .collect();

    for (doc, num) in KNOWN {
        let key = files
            .keys()
            .find(|k| k.as_str() == *doc)
            .unwrap_or_else(|| panic!("{doc} is not in docs/"));
        assert!(
            !have[key].contains(*num),
            "{doc} \u{a7}{num} exists now: somebody wrote the section. Take it off \
             KNOWN and close its row on DECISIONS.md's open list."
        );
    }
}

/// Split a line on **unescaped** pipes.
///
/// GFM honours `\|` and nothing else — a pipe inside a code span is still a
/// column separator — so this is the split a renderer actually performs.
fn split_cells(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            cur.push(c);
            if let Some(n) = chars.next() {
                cur.push(n);
            }
            continue;
        }
        if c == '|' {
            cells.push(std::mem::take(&mut cur));
            continue;
        }
        cur.push(c);
    }
    cells.push(cur);
    cells
}

/// How many columns a table line declares.
///
/// `None` for a line that is not a table row at all. The leading pipe is
/// required and the trailing one is optional, which is what GFM allows.
fn columns(line: &str) -> Option<usize> {
    let t = line.trim_end();
    if !t.starts_with('|') {
        return None;
    }
    let cells = split_cells(t);
    // `| a | b |` splits to ["", " a ", " b ", ""]; the empty head is the
    // leading pipe and the empty tail the optional trailing one.
    let mut n = cells.len() - 1;
    if cells.last().is_some_and(|c| c.trim().is_empty()) {
        n -= 1;
    }
    Some(n)
}

fn is_separator(line: &str) -> bool {
    let t = line.trim();
    if !t.starts_with('|') {
        return false;
    }
    let cells = split_cells(t);
    let inner: Vec<&String> = cells
        .get(1..cells.len().saturating_sub(1))
        .unwrap_or(&[])
        .iter()
        .collect();
    !inner.is_empty()
        && inner.iter().all(|c| {
            let c = c.trim();
            !c.is_empty() && c.contains('-') && c.chars().all(|ch| ch == '-' || ch == ':')
        })
}

/// Every row of every table must have exactly the columns its header declares.
///
/// # Why this is a test
///
/// GFM **ignores** cells past the header's count. A row that grows an extra
/// cell does not render wide — it renders with its last cell **deleted**, in
/// silence, in every viewer, while the file on disk still contains the text.
///
/// That is not hypothetical here. `DECISIONS.md`'s findings register is updated
/// by appending the new verdict as a new final cell, and fifteen rows had
/// acquired a fourth column that way — so for each of them the cell being
/// dropped was **the newest verdict**, and the register rendered the superseded
/// one instead. Found only because a stray pipe in one row was traced back.
///
/// The related trap is set-cardinality notation: `|P(k)|` and
/// `|signed_this_hand| == 1` are column separators to GFM even inside a code
/// span, and must be written `\|`. Twenty-two rows across ten documents were
/// splitting themselves that way.
///
/// Both failures are invisible in the source and obvious to this test, which is
/// exactly the shape that earns a test rather than a sweep.
#[test]
fn every_table_row_has_the_columns_its_header_declares() {
    let docs = read_all();
    let mut wrong: Vec<String> = Vec::new();

    for (name, text) in &docs {
        if !LIVE.contains(&name.as_str()) {
            continue;
        }
        let lines: Vec<&str> = text.lines().collect();
        let mut in_fence = false;
        let mut i = 0;
        while i < lines.len() {
            let trimmed = lines[i].trim_start();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_fence = !in_fence;
                i += 1;
                continue;
            }
            if in_fence {
                i += 1;
                continue;
            }
            let header = columns(lines[i]);
            if let Some(want) = header {
                if i + 1 < lines.len() && is_separator(lines[i + 1]) {
                    let mut j = i + 2;
                    while j < lines.len() && lines[j].starts_with('|') {
                        if let Some(got) = columns(lines[j]) {
                            if got != want {
                                let id = split_cells(lines[j])
                                    .get(1)
                                    .map(|c| c.trim().to_string())
                                    .unwrap_or_default();
                                let id: String = id.chars().take(40).collect();
                                wrong.push(format!(
                                    "{name}:{} row [{id}] has {got} cells, its header declares {want}",
                                    j + 1
                                ));
                            }
                        }
                        j += 1;
                    }
                    i = j;
                    continue;
                }
            }
            i += 1;
        }
    }

    assert!(
        wrong.is_empty(),
        "{} table row(s) do not have their header's column count. GFM does not \
         render these wide - it DELETES the cells past the header's count, in \
         silence. Where the register is updated by appending a new final cell, \
         the deleted cell is the newest verdict and the file reads as if the \
         superseded one still stood. Merge the extra cells back into the body, \
         and write set-cardinality pipes as \\| so they stop splitting rows:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// **The code and the corpus disagree about `PublicTableState`, deliberately,
/// and nothing else in this tree would notice.**
///
/// `signed_this_hand` was §6.1's field 28 and is deleted from
/// `PublicTableState` — it was *"true where that seat signed at least one
/// chained event of this hand **that this peer accepted**"*, an observation of
/// the listener rather than a fact about the hand, so two honest peers on a
/// lossy link had to differ in it while the protocol required them to agree.
/// Measured across ten-seat runs before and after: settlement disagreements
/// **124 → 0**, false timeout accusations **14 → 0**, readmissions **10 → 20**
/// (`S1-AZ` … `S1-BD`, and the row that records the removal).
///
/// **The corpus is not an implementation's to edit** (D-011 rule 1), so
/// `PROTOCOL.md` §6.1 still lists the field. That is a real interop hazard and
/// not a cosmetic one: `PublicTableState` is `#[cbor(array)]`, so the element
/// count is part of the encoding — the array header goes `0x98 0x1D` to
/// `0x98 0x1C` — and a second implementation transcribed from the document
/// would derive a different `state_hash` for identical state at every
/// checkpoint of every hand. §6.1's table is declared the single normative
/// statement of that field order and the only list an implementer reads.
///
/// Nothing else catches it. `corpus_constants` does not cover `FIELD_COUNT`,
/// and this file's other checks only assert that a cited section exists, not
/// that it says what the citation claims.
///
/// So the divergence is pinned rather than hidden: this test passes only while
/// both halves are in the state described. **When the owner corrects §6.1, this
/// test fails — and the right response is to delete it**, not to weaken it.
#[test]
fn the_corpus_still_lists_a_field_the_code_has_removed() {
    let corpus = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/PROTOCOL.md"),
    )
    .expect("the corpus is readable");

    assert_eq!(
        p2p_poker::protocol::state_view::PublicTableState::FIELD_COUNT,
        28,
        "the struct's field count moved; if a field was added or removed, this \
         pin and PROTOCOL.md §6.1 both need revisiting"
    );

    let listed = corpus
        .lines()
        .any(|l| l.starts_with("| `signed_this_hand`"));
    assert!(
        listed,
        "PROTOCOL.md §6.1 no longer lists `signed_this_hand`, so the corpus and \
         the code finally agree. Delete this test: it exists only to hold the \
         divergence visible while it lasts."
    );
}
