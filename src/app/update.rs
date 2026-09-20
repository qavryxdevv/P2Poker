//! `D-075`: is there a newer version? -- asked when the player presses the
//! button, and never by the client on its own.
//!
//! This client has no house and no server, and it does not phone anybody. The
//! one place it could learn of a newer release is where its releases are
//! published, so **the player asks, and the client asks GitHub once**: one
//! `GET` of the repository's public list of releases, with nothing of the
//! player or of this client in it but the address the request comes from --
//! exactly what opening the releases page in a browser would show.
//!
//! # What this never does
//!
//! * **It never asks by itself** -- not at start, not on a timer, not after a
//!   game. There is no setting to turn that on, because there is nothing to
//!   turn on.
//! * **It never downloads and never installs.** A client that fetches a program
//!   and runs it is a different question of trust from one that says *there is
//!   a newer one*. The player downloads the release, can check it against this
//!   source (`D-074`), and starts it; the installer of `D-073` then offers to
//!   update the installed copy.
//! * **It opens no address the network chose.** What comes back is read for one
//!   thing -- version numbers -- and the page the player is sent to is a constant
//!   of the build (`gui::render::RELEASES_URL`), as the donation and bug pages
//!   are (`D-071`).
//!
//! The answer is bounded before it is parsed, redirects are not followed, and a
//! tag is believed only if it is a plain version: whatever is shown to the
//! player from it is digits and dots.

use crate::install::Relation;

/// The repository's releases, newest first as GitHub lists them. Ten is more
/// than enough to contain the newest by NUMBER, which need not be the first.
pub const RELEASES_API: &str = "https://api.github.com/repos/qavryxdevv/P2Poker/releases?per_page=10";

/// The most of an answer that is read. Ten releases with their notes are a few
/// tens of kilobytes; this is a ceiling for a bad day, not a budget.
pub const RELEASES_ANSWER_MAX: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing has been released yet.
    NoRelease,
    /// The newest release is this version, or an older one.
    Newest { latest: String },
    /// A release with a higher version number is out.
    Newer { latest: String },
}

/// The version a tag names, if it is a plain one: an optional `v`, one to three
/// numbers with dots between, and an optional short suffix after `-` or `+`.
///
/// **Anything else is not a version and is not shown to anybody.** The tag is
/// text from the network that ends up on the player's screen, and *digits and
/// dots* is a claim that can be checked where *whatever GitHub sent* is not.
pub fn plain_version(tag: &str) -> Option<&str> {
    let v = tag.strip_prefix('v').unwrap_or(tag);
    if v.is_empty() || v.len() > 40 {
        return None;
    }
    let (core, suffix) = match v.find(['-', '+']) {
        Some(i) => (&v[..i], &v[i + 1..]),
        None => (v, ""),
    };
    let parts: Vec<&str> = core.split('.').collect();
    let numbers = (1..=3).contains(&parts.len())
        && parts.iter().all(|p| !p.is_empty() && p.len() <= 6 && p.bytes().all(|b| b.is_ascii_digit()));
    let tail = suffix.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    (numbers && tail && (suffix.is_empty() == !v.contains(['-', '+']))).then_some(v)
}

/// The highest version among the releases of `json` -- GitHub's answer to
/// [`RELEASES_API`] -- or `None` when there is none. A draft is nobody's
/// release; a pre-release is one, because every release of a beta is.
///
/// **The highest by number, not the first in the list**: GitHub lists by date,
/// and a fix released for an older line after a newer one is first and not
/// newest.
pub fn newest_release(json: &str) -> Result<Option<String>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "GitHub's answer could not be read as a list of releases".to_owned())?;
    let list = value.as_array().ok_or_else(|| "GitHub's answer was not a list of releases".to_owned())?;
    let mut best: Option<&str> = None;
    for release in list {
        if release.get("draft").and_then(|d| d.as_bool()).unwrap_or(false) {
            continue;
        }
        let Some(version) = release.get("tag_name").and_then(|t| t.as_str()).and_then(plain_version) else {
            continue;
        };
        if best.is_none_or(|b| Relation::of(Some(b), version) == Relation::Older) {
            best = Some(version);
        }
    }
    Ok(best.map(str::to_owned))
}

/// What to tell the player, from this build's version and GitHub's answer.
pub fn verdict(this_version: &str, json: &str) -> Result<Verdict, String> {
    Ok(match newest_release(json)? {
        None => Verdict::NoRelease,
        // `Relation::of(installed, this)` reads *installed is OLDER than this*:
        // here *this build* is the installed one and the release is the other.
        Some(latest) if Relation::of(Some(this_version), &latest) == Relation::Older => Verdict::Newer { latest },
        Some(latest) => Verdict::Newest { latest },
    })
}

/// Ask GitHub. **Blocking** -- called on a thread of its own, never on the
/// paint thread or the node's loop.
#[cfg(feature = "tox")]
pub fn check() -> Result<Verdict, String> {
    use std::io::Read;
    let response = attohttpc::get(RELEASES_API)
        // GitHub's API refuses a request with no `User-Agent`. The product's
        // name and nothing else: not the version, not the system, not a key.
        .header("User-Agent", "P2Poker")
        .header("Accept", "application/vnd.github+json")
        // A redirect means the repository is somewhere else, and *somewhere
        // else* is not a place this client follows anybody to.
        .follow_redirects(false)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .map_err(|e| format!("GitHub could not be reached: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            403 | 429 => "GitHub is not answering this address right now (too many questions from it in the last hour)".into(),
            code => format!("GitHub answered {code}"),
        });
    }
    let (_, _, body) = response.split();
    let mut bytes = Vec::new();
    body.take(RELEASES_ANSWER_MAX as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("GitHub's answer could not be read: {e}"))?;
    if bytes.len() > RELEASES_ANSWER_MAX {
        return Err("GitHub's answer was larger than a list of releases has any reason to be".into());
    }
    let text = String::from_utf8(bytes).map_err(|_| "GitHub's answer was not text".to_owned())?;
    verdict(env!("CARGO_PKG_VERSION"), &text)
}

/// A build without the HTTPS client (`--no-default-features`) cannot ask.
#[cfg(not(feature = "tox"))]
pub fn check() -> Result<Verdict, String> {
    Err("this build has no way to ask; the releases page says what the newest version is".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn releases(tags: &[(&str, bool, bool)]) -> String {
        let items: Vec<String> = tags
            .iter()
            .map(|(tag, draft, pre)| {
                format!(r#"{{"tag_name":"{tag}","draft":{draft},"prerelease":{pre},"body":"notes","html_url":"https://example.invalid/x"}}"#)
            })
            .collect();
        format!("[{}]", items.join(","))
    }

    /// **`D-075`: newer means a higher NUMBER, of a real release.** The breaks
    /// this must catch: versions compared as text (`0.10.0` under `0.9.0`), the
    /// first of the list taken for the newest, and a draft counted as out.
    #[test]
    fn newer_is_a_higher_number_of_a_real_release() {
        let json = releases(&[("v0.9.1", false, true), ("v0.10.0", false, true), ("v0.2.0", false, false)]);
        assert_eq!(newest_release(&json).unwrap().as_deref(), Some("0.10.0"), "by number, and not the first listed");
        assert_eq!(verdict("0.9.1", &json).unwrap(), Verdict::Newer { latest: "0.10.0".into() });
        assert_eq!(verdict("0.10.0", &json).unwrap(), Verdict::Newest { latest: "0.10.0".into() });
        // A build AHEAD of every release -- the owner's own -- is told it is the newest, not nagged.
        assert_eq!(verdict("0.11.0", &json).unwrap(), Verdict::Newest { latest: "0.10.0".into() });

        // A draft is nobody's release.
        let json = releases(&[("v9.0.0", true, false), ("v0.1.0", false, true)]);
        assert_eq!(verdict("0.1.0", &json).unwrap(), Verdict::Newest { latest: "0.1.0".into() });

        // A pre-release is one: every release of a beta is.
        let json = releases(&[("v0.2.0", false, true)]);
        assert_eq!(verdict("0.1.0", &json).unwrap(), Verdict::Newer { latest: "0.2.0".into() });

        assert_eq!(verdict("0.1.0", "[]").unwrap(), Verdict::NoRelease);
    }

    /// What is shown to the player from the network is digits and dots, or
    /// nothing. A tag that is not a plain version is passed over -- it cannot
    /// make itself the newest, and it cannot put words on the About page.
    #[test]
    fn only_a_plain_version_is_believed() {
        for good in ["v0.1.0", "0.1.0", "v1", "v2.10", "v0.1.0-beta.2", "v1.0.0+build.7"] {
            assert!(plain_version(good).is_some(), "{good}");
        }
        assert_eq!(plain_version("v0.1.0-beta.2"), Some("0.1.0-beta.2"));
        for bad in [
            "",
            "v",
            "latest",
            "v1.2.3.4",
            "v1..2",
            "v1.2.",
            "v0.1.0 <b>now</b>",
            "v0.1.0\nDownload from evil.example",
            "v1.2.3-",
            "v99999999.0.0",
            "v0.1.0-ěščř",
            "../../x",
        ] {
            assert_eq!(plain_version(bad), None, "{bad:?}");
        }
        let json = releases(&[("PLEASE UPDATE NOW v9", false, false), ("v0.1.0", false, false)]);
        assert_eq!(verdict("0.1.0", &json).unwrap(), Verdict::Newest { latest: "0.1.0".into() });
    }

    /// An answer that is not a list of releases is an error said in words, and
    /// never a verdict: *you have the newest* must not be what a broken answer
    /// reads as.
    #[test]
    fn a_broken_answer_is_not_good_news() {
        for bad in ["", "not json", "{\"message\":\"Not Found\"}", "null", "42"] {
            assert!(verdict("0.1.0", bad).is_err(), "{bad:?}");
        }
        // Entries that are not releases are passed over, not fatal.
        assert_eq!(verdict("0.1.0", "[1, \"x\", {}, {\"tag_name\": 7}]").unwrap(), Verdict::NoRelease);
    }

    /// The address asked is this project's own repository on GitHub's API, over
    /// TLS, and carries no query but the page size.
    #[test]
    fn the_question_goes_to_this_repository_and_carries_nothing() {
        assert!(RELEASES_API.starts_with("https://api.github.com/repos/qavryxdevv/P2Poker/releases"));
        assert_eq!(RELEASES_API.matches('?').count(), 1);
        assert!(RELEASES_API.ends_with("?per_page=10"));
    }
}
