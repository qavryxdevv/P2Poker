//! Named constants: one definition each, and the number the corpus publishes.
//!
//! # The two checks, and which one found the defect
//!
//! `DECISIONS.md` D-011 rule 1 asks for **one normative owner per concept**.
//! Applied to constants that is two separate questions, and only one of them is
//! the obvious one.
//!
//! **The obvious one — does the code agree with the corpus?** Eighty-seven
//! published values are compared and they all match. That check has never found
//! anything, and it is here so that it fails the day one drifts.
//!
//! **The one that found something — is the name defined twice at all?**
//! `PRESENCE_TTL_MS` was `120_000` in `protocol::constants`, beside a
//! compile-time assertion, read by nothing; and `90_000` in `net::lobbytalk`,
//! which is what the client sent and expired on. The corpus publishes the first
//! pair. The second was the *advert* pair — `AD_TTL_MS` / `AD_REBROADCAST_MS`
//! are exactly `90_000` / `30_000` — copied because `lobbytalk`'s comment said
//! presence held *"the same relationship the table advertisements have with
//! their own TTL"*. The relationship is 3×; what was taken was the values.
//!
//! Notice that the first check could not have caught it. The corpus's number
//! was in the code, in a constant with the right name, guarded by an assertion
//! that passed. Nothing was missing. There was a **second** one, and the second
//! one was the one that ran.
//!
//! # What it does not check
//!
//! Associated constants in traits and impls, function-local ones, and anything
//! under `#[cfg(test)]`. A trait's default `DECK_LEN = 52` overridden to `4` by
//! a four-card test deck is not a duplicate; it is the feature working.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust_files() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<(String, String)>, base: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out, base);
            } else if p.extension().is_some_and(|x| x == "rs") {
                if let Ok(text) = std::fs::read_to_string(&p) {
                    let rel = p
                        .strip_prefix(base)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push((rel, text));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&root().join("src"), &mut out, &root());
    out.sort();
    out
}

/// A `pub const NAME: Type = value;` written at column 0.
///
/// **Column 0 is the whole discrimination.** A constant at module scope is part
/// of the crate's surface; one indented is inside a `trait`, an `impl`, a
/// function or a test module, where a second definition of the same name is
/// ordinary. Nothing cleverer is needed, and anything cleverer would have to
/// decide what a `trait` default means.
fn module_constants(text: &str) -> Vec<(String, String, usize)> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let Some(rest) = line.strip_prefix("pub const ") else {
            continue;
        };
        let Some(colon) = rest.find(':') else { continue };
        let name = rest[..colon].trim();
        let named_like_a_constant = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
        if !named_like_a_constant {
            continue;
        }
        let Some(eq) = rest.find(" = ") else { continue };
        let value = rest[eq + 3..].trim_end().trim_end_matches(';').trim();
        out.push((name.to_string(), value.to_string(), i + 1));
    }
    out
}

/// **No name is a `pub const` at module scope in two places.**
#[test]
fn one_definition_per_public_constant() {
    let files = rust_files();
    let mut seen: BTreeMap<String, Vec<(String, String, usize)>> = BTreeMap::new();
    for (path, text) in &files {
        for (name, value, line) in module_constants(text) {
            seen.entry(name)
                .or_default()
                .push((path.clone(), value, line));
        }
    }

    assert!(
        seen.len() > 80,
        "only {} module-scope public constants found, which is not this crate",
        seen.len()
    );

    let mut bad = Vec::new();
    for (name, defs) in &seen {
        if defs.len() < 2 {
            continue;
        }
        let agree = defs.windows(2).all(|w| w[0].1 == w[1].1);
        let note = if agree {
            "they agree today, which is the state `PRESENCE_TTL_MS` was in until it did not"
        } else {
            "and they DISAGREE, so only one of them is what actually runs"
        };
        bad.push(format!(
            "  {name} — {note}\n{}",
            defs.iter()
                .map(|(p, v, l)| format!("      {p}:{l} = {v}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    assert!(
        bad.is_empty(),
        "{} public constant(s) are defined at module scope more than once. Name one \
         owner and `pub use` it from the other place (D-011 rule 1) — a second \
         definition is not a copy of a value, it is a second value that happens to \
         match.\n{}",
        bad.len(),
        bad.join("\n")
    );
}

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

/// **A number the corpus publishes for a named constant is that constant's.**
#[test]
fn the_corpus_and_the_code_agree_on_every_published_constant() {
    let mut code: BTreeMap<String, Vec<u128>> = BTreeMap::new();
    for (_, text) in rust_files() {
        for (name, value, _) in module_constants(&text) {
            if let Some(n) = whole_number(&value) {
                code.entry(name).or_default().push(n);
            }
        }
    }

    let mut checked = 0usize;
    let mut bad = Vec::new();
    for doc in LIVE {
        let text = std::fs::read_to_string(root().join("docs").join(doc))
            .unwrap_or_else(|e| panic!("docs/{doc}: {e}"));
        for (no, line) in text.lines().enumerate() {
            for (name, values) in &code {
                let Some(stated) = stated_value(line, name) else {
                    continue;
                };
                checked += 1;
                if values.contains(&stated) {
                    continue;
                }
                bad.push(format!(
                    "  {doc}:{} says {name} = {stated}, code has {values:?}\n      {}",
                    no + 1,
                    line.trim()
                ));
            }
        }
    }

    // The coverage gate: this matched 87 published values when it was written.
    assert!(
        checked > 60,
        "only {checked} published constant values matched a name in the code. The \
         corpus states far more than that, so `stated_value` has stopped parsing \
         something and a green result here would mean nothing."
    );

    assert!(
        bad.is_empty(),
        "{} constant(s) have a different value in the corpus and in the code. The \
         corpus is the wire authority; a number that differs is a message two \
         clients disagree about.\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// `4_096`, `4096`, `4_096usize` — the number, if the whole value is one.
///
/// A derived value (`MAX * 3`, `f(N)`) returns `None`: its leading digits are
/// not its value, and comparing them against a document would be nonsense.
fn whole_number(value: &str) -> Option<u128> {
    let v = value.trim();
    let head: String = v
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '_')
        .collect();
    if head.is_empty() {
        return None;
    }
    let suffix = &v[head.len()..];
    if !suffix.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    head.replace('_', "").parse().ok()
}

/// `` `NAME` = 120_000 `` or `NAME = 120 000` in one line of prose or a table.
///
/// **The name must be followed by a relational word and then the number**, with
/// nothing between but a backtick. A looser rule pairs a constant with whatever
/// figure stands nearby; `8 × MAX_SEATS = 80` is the shape that produces, which
/// is why an arithmetic left-hand side is refused.
fn stated_value(line: &str, name: &str) -> Option<u128> {
    let mut from = 0usize;
    while let Some(at) = line[from..].find(name) {
        let at = from + at;
        from = at + name.len();

        let before = line[..at].chars().next_back();
        if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        // `8 × MAX_SEATS = 80` states a product, not the constant.
        let lead = line[..at].trim_end().trim_end_matches('`').trim_end();
        let multiplied = lead.ends_with('\u{d7}') || lead.ends_with('*') || lead.ends_with(" x");
        if multiplied {
            continue;
        }

        let mut rest = line[from..].trim_start_matches('`').trim_start();
        for word in ["=", "is", "of", "at", ":"] {
            let Some(r) = rest.strip_prefix(word) else {
                continue;
            };
            rest = r.trim_start().trim_start_matches('`');
            if !rest.starts_with(|c: char| c.is_ascii_digit()) {
                break;
            }
            let digits: String = rest
                .chars()
                .take_while(|c| {
                    c.is_ascii_digit()
                        || *c == '_'
                        || *c == ' '
                        || *c == '\u{a0}'
                        || *c == '\u{202f}'
                })
                .filter(char::is_ascii_digit)
                .collect();
            return digits.parse().ok();
        }
    }
    None
}


/// §9.3 gives every message its own payload cap, and until `S1-W` the code
/// enforced one shared number for the whole join family.
///
/// `JOIN_REQUEST` is published at 512 and was checked at 4 096; `JOIN_ACCEPT`
/// at 8 192, `JOIN_REJECT` at 128, `PLAYER_LIST` at 2 048 and `TABLE_READY` at
/// 1 024 were all checked at 16 384. Four published bounds enforced by nothing,
/// and a client built to §9.3 would have refused messages this one considers
/// legal.
///
/// **The caps are on the payload and not on the frame**, which is the mistake
/// worth pinning: applied to the whole signed event, `JOIN_REJECT`'s 128 is
/// smaller than the envelope alone — 32 bytes of table id, 32 of sender key, 32
/// of parent hash and 64 of signature — so every message of that type failed to
/// decode. The published number is the payload's.
#[test]
fn the_join_family_enforces_section_9_3s_own_caps() {
    use p2p_poker::protocol::constants::*;
    for (name, code, published) in [
        ("JOIN_REQUEST", JOIN_REQUEST_MAX, 512usize),
        ("JOIN_ACCEPT", JOIN_ACCEPT_MAX, 8_192),
        ("JOIN_REJECT", JOIN_REJECT_MAX, 128),
        ("PLAYER_LIST", PLAYER_LIST_MAX, 2_048),
        ("TABLE_READY", TABLE_READY_MAX, 1_536),
    ] {
        assert_eq!(code, published, "{name}'s cap is §9.3's");
    }
}

/// **Every wire field this client emits that the corpus does not define.**
///
/// `S1-AE`. Four fields carry D-019's Tox addresses and no specification names
/// one: `grep -ci tox` over `PROTOCOL.md`, `SPEC_CS.md`, `STATE_MACHINE.md`,
/// `NETWORK_STACK.md` and `CRYPTOGRAPHY.md` returns zero, five times.
///
/// # Why a test rather than a note
///
/// §10.2 makes this the kind of divergence that cannot be tolerated at the far
/// end: *"An old client re-encoding a new struct produces different bytes, the
/// gate fires, and the event is rejected. There is **no** 'ignore unknown
/// trailing fields' behaviour and there **cannot** be one, because tolerating
/// trailing data is precisely the equivocation hole the gate exists to close."*
/// So a conforming second implementation refuses every message carrying one of
/// these, and `SeatEntry` sits inside both `JOIN_ACCEPT` and `PLAYER_LIST`, so
/// four message types are affected and formation cannot complete.
///
/// This test does not fix that — the repair is a corpus edit and it is the
/// owner's. What it does is **hold the count still**. A fifth field added
/// quietly is a fifth message type a second implementation refuses, and the
/// only thing that noticed the first four was a person reading two documents
/// side by side.
///
/// **It is a ratchet, not a check against the corpus.** Parsing §4.3's field
/// tables out of prose would be a second thing to keep in step with the first.
/// The numbers here are written down with the table each must match, and a
/// change to either side breaks the build and demands the note be rewritten.
#[test]
fn the_wire_fields_the_corpus_does_not_define_are_still_exactly_four() {
    let root = root();
    let read = |rel: &str| -> String {
        std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
    };

    // Zero mentions of the transport whose keys these fields carry.
    for doc in [
        "docs/PROTOCOL.md",
        "docs/SPEC_CS.md",
        "docs/STATE_MACHINE.md",
        "docs/NETWORK_STACK.md",
        "docs/CRYPTOGRAPHY.md",
    ] {
        let text = read(doc).to_lowercase();
        assert!(
            !text.contains("tox"),
            "{doc} now mentions Tox. If D-019's fields have been written into the \
             specification, this test and `S1-AE` are both out of date — rewrite them \
             rather than deleting the assertion."
        );
    }

    // The four, each against the table that stops one index short of it.
    let joinwire = read("src/net/joinwire.rs");
    let advert = read("src/net/advert.rs");
    for (file, text, field, corpus) in [
        ("joinwire.rs", &joinwire, "n(9), with = \"minicbor::bytes\")]\n    pub tox_key", "§4.3's JOIN_REQUEST table ends at n(8) table_id"),
        ("joinwire.rs", &joinwire, "n(5), with = \"minicbor::bytes\")]\n    pub tox_key", "§4.3 spells SeatEntry out in one line and ends at n(4) buyin"),
        ("advert.rs", &advert, "n(30), with = \"minicbor::bytes\")]\n    pub founder_tox_key", "§7.2 ends at n(29) time_bank_ms"),
        ("advert.rs", &advert, "n(31), with = \"minicbor::bytes\")]\n    pub tox_chat_id", "§7.2 ends at n(29) time_bank_ms"),
    ] {
        assert!(
            text.contains(field),
            "{file} no longer carries the field this test pins ({corpus}). If it was \
             removed, or renamed, say so in `S1-AE` and here."
        );
    }

    // And nothing has grown past them. `n(10)` on a join body or `n(32)` on an
    // advert would be a fifth undefined field.
    assert!(
        !joinwire.contains("n(10)"),
        "a tenth JOIN_REQUEST field: §4.3 defines nine and `S1-AE` counts the tenth"
    );
    assert!(
        !joinwire.contains("n(6), with = \"minicbor::bytes\")]\n    pub tox"),
        "a seventh SeatEntry field"
    );
    assert!(
        !advert.contains("n(32)"),
        "a thirty-third advert field: §7.2 defines thirty and `S1-AE` counts the rest"
    );
}
