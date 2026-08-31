//! `DEPENDENCIES.md`'s register, against `Cargo.lock`.
//!
//! # Why this is a test
//!
//! The register carried, dated three days before this file was written, a
//! blockquote saying every `name`+`version` in every §5 table had been matched
//! against a `[[package]]` entry in `Cargo.lock` and **all 122 rows matched**.
//! Running that check found **seven** that did not: `mainline` and four crates
//! under it, plus the two bencode parsers that decoded its packets. They had
//! left in `c7e6317` — *"Discovery is libp2p's now, and BitTorrent is out of
//! the binary"* — and nothing in the document was told.
//!
//! That is not a cosmetic staleness. §3.3 carried `lru 0.16.4`'s RUSTSEC
//! unsoundness as **accepted with justification**, and an accepted advisory is
//! a decision a reader relies on. §3.7 stated a hard constraint on a source
//! file that had been deleted. §4's table listed the advisory as compiled.
//!
//! **A claim that a check was run is worth nothing next to the check.** So the
//! check runs here, on every `cargo test`, and the claim is gone.
//!
//! # What it does not check
//!
//! Versions, licences and repository URLs. Those need the registry checkouts,
//! which are not in the tree. This settles the cheapest and most load-bearing
//! part: **the crate exists at all**.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let p: PathBuf = root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Every `[[package]]` name in `Cargo.lock`, and how many entries there are.
fn locked() -> (BTreeSet<String>, usize) {
    let text = read("Cargo.lock");
    let mut names = BTreeSet::new();
    let mut packages = 0usize;
    let mut in_package = false;
    for line in text.lines() {
        if line.trim() == "[[package]]" {
            in_package = true;
            packages += 1;
            continue;
        }
        if line.starts_with('[') {
            in_package = false;
        }
        if in_package {
            if let Some(rest) = line.strip_prefix("name = \"") {
                if let Some(name) = rest.strip_suffix('"') {
                    names.insert(name.to_string());
                }
            }
        }
    }
    (names, packages)
}

/// The crate rows of `DEPENDENCIES.md` §5, as `(section, name, line)`.
///
/// A register row starts `` | `name` | version | ``. The version column is what
/// separates a crate row from §6's licence counts and from prose tables, and it
/// is why the pattern insists the second cell begin with a digit.
fn register_rows(doc: &str) -> Vec<(String, String, usize)> {
    let lines: Vec<&str> = doc.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.starts_with("## 5. The register"))
        .expect("DEPENDENCIES.md has no `## 5. The register`");
    let end = lines[start + 1..]
        .iter()
        .position(|l| l.starts_with("## ") && l[3..4].chars().all(|c| c.is_ascii_digit()))
        .map(|i| start + 1 + i)
        .unwrap_or(lines.len());

    let mut section = String::new();
    let mut out = Vec::new();
    for (i, line) in lines[start..end].iter().enumerate() {
        if let Some(rest) = line.strip_prefix("### ") {
            section = rest.split_whitespace().next().unwrap_or("").to_string();
        }
        let Some(rest) = line.strip_prefix("| `") else {
            continue;
        };
        let Some(close) = rest.find('`') else { continue };
        let name = &rest[..close];
        let after = &rest[close..];
        let Some(bar) = after.find("| ") else { continue };
        let cell = &after[bar + 2..];
        if !cell.starts_with(|c: char| c.is_ascii_digit()) {
            continue;
        }
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            continue;
        }
        out.push((section.clone(), name.to_string(), start + i + 1));
    }
    out
}

/// **Every crate the register names must be in the lockfile.**
#[test]
fn the_register_names_no_crate_that_left_the_build() {
    let doc = read("docs/DEPENDENCIES.md");
    let rows = register_rows(&doc);
    let (names, _) = locked();

    // The coverage gate. A parser that stops matching rows passes silently, and
    // this test would then certify a register it never read. The register held
    // 114 crate rows when this floor was set.
    assert!(
        rows.len() > 90,
        "only {} register rows parsed, which is not DEPENDENCIES.md \u{a7}5. A green \
         result from here would mean nothing until `register_rows` is fixed.",
        rows.len()
    );

    let missing: Vec<String> = rows
        .iter()
        .filter(|(_, name, _)| !names.contains(name))
        .map(|(section, name, line)| {
            format!("  \u{a7}{section:<6} {name:<16} DEPENDENCIES.md:{line}")
        })
        .collect();

    assert!(
        missing.is_empty(),
        "{} of {} crates in DEPENDENCIES.md \u{a7}5 are not in Cargo.lock. A register \
         row is a claim that the crate is compiled, and \u{a7}3's acceptances and \
         \u{a7}4's advisory table are argued from it - see \u{a7}5.8, which is what this \
         test was written after.\n{}",
        missing.len(),
        rows.len(),
        missing.join("\n")
    );
}

/// **§1's lockfile figure must be the lockfile's.**
///
/// It said 646 where the file had 659, and the §5 blockquote said 611 in the
/// same document. Three statements of one number, none of them measured since
/// the number moved.
#[test]
fn section_one_states_the_lockfile_size_it_has() {
    let doc = read("docs/DEPENDENCIES.md");
    let (_, packages) = locked();
    assert!(
        packages > 400,
        "{packages} `[[package]]` entries is not this lockfile"
    );

    let want = format!("| Crates **recorded in `Cargo.lock`** | **{}** |", packages - 1);
    assert!(
        doc.contains(&want),
        "DEPENDENCIES.md \u{a7}1 does not state the lockfile size. Cargo.lock holds {} \
         `[[package]]` entries, so the row should read:\n    {want}\n\
         The figure excludes the root package, as \u{a7}1 says.",
        packages
    );
}

/// A `.md` in `docs/` that names a crate as a dependency of ours is not checked
/// here, and this test says so out loud so the next reader does not assume it.
#[test]
fn the_check_is_scoped_to_the_register_and_says_so() {
    let doc = read("docs/DEPENDENCIES.md");
    assert!(
        doc.contains("tests/corpus_dependencies.rs"),
        "DEPENDENCIES.md does not mention this test. A gate nobody knows about is \
         a gate somebody removes."
    );
    assert!(Path::new(&root().join("Cargo.lock")).exists());
}
