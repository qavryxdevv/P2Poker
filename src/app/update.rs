//! `D-075`, `D-077`: is there a newer version? -- asked once as the window
//! opens, and again whenever the player presses the button; and **a newer
//! release is required**, because this beta's protocol still changes from one
//! version to the next.
//!
//! This client has no house and no server. The one place it could learn of a
//! newer release is where its releases are published, so it asks GitHub: one
//! `GET` of the repository's public list of releases, with nothing of the
//! player or of this client in it but the address the request comes from --
//! exactly what opening the releases page in a browser would show.
//!
//! # When it asks (`D-077`, the owner, 2026-09-21)
//!
//! * **As the window opens**, before the lobby lets anybody sit down, and when
//!   the player presses *Check for a new version*.
//! * **Not on a timer, not after a game, and never in the headless client**: a
//!   relay's operator and a bed seat read the releases page, and a bed that
//!   asked GitHub at every start would spend the hour's sixty questions of the
//!   address it runs from in a few runs.
//!
//! # What a newer release does
//!
//! It closes the lobby to this version (`gui::lobby::gate`): a window over it
//! says which version is out, hands the download to the player's browser, and
//! offers nothing else but closing the client. **A check that could not be
//! made closes nothing** -- no network, GitHub refusing, an answer that is not
//! a list of releases: a client that cannot ask plays, and says why. A build
//! ahead of every release -- the owner's own -- is the newest and is never
//! stopped.
//!
//! # What this never does
//!
//! * **It never downloads and never installs.** A client that fetches a program
//!   and runs it is a different question of trust from one that says *there is
//!   a newer one* -- and to the system it looks like a dropper. The player's
//!   browser downloads the release, which then carries the mark a download
//!   carries; the player starts it, and the installer of `D-073` updates the
//!   installed copy and keeps the profile.
//! * **It opens no address the network chose.** What comes back is read for
//!   one thing -- a version, and the tag it is released under -- and a tag is
//!   believed only if it is digits and dots (`plain_version`). The pages a
//!   player is sent to are this build's constant `gui::render::RELEASES_URL`
//!   with that tag in its place in the path: no host, no query and no other
//!   segment comes from the answer.
//!
//! The answer is bounded before it is parsed, and redirects are not followed.

use crate::install::Relation;

/// The repository's releases, newest first as GitHub lists them. Ten is more
/// than enough to contain the newest by NUMBER, which need not be the first.
pub const RELEASES_API: &str = "https://api.github.com/repos/qavryxdevv/P2Poker/releases?per_page=10";

/// The most of an answer that is read. Ten releases with their notes are a few
/// tens of kilobytes; this is a ceiling for a bad day, not a budget.
pub const RELEASES_ANSWER_MAX: usize = 512 * 1024;

/// How long one question may take, from the name lookup to the last byte. The
/// lobby waits for the question the window asks as it opens, so this is also
/// the longest a player can be kept from the tables by a network that drops
/// what it is sent. A network that is simply not there answers at once.
const CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing has been released yet.
    NoRelease,
    /// The newest release is this version, or an older one.
    Newest { latest: String },
    /// A release with a higher version number is out, under this tag.
    Newer { latest: String, tag: String },
}

/// A release as this client believes it: the version, and the tag it is
/// published under -- `v` and the version, as the release workflow makes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub tag: String,
}

/// The version a tag names, if it is a plain one: an optional `v`, one to three
/// numbers with dots between, and an optional short suffix after `-` or `+`.
///
/// **Anything else is not a version and is not shown to anybody.** The tag is
/// text from the network that ends up on the player's screen and in the path
/// of an address, and *digits and dots* is a claim that can be checked where
/// *whatever GitHub sent* is not.
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

/// The highest release among those of `json` -- GitHub's answer to
/// [`RELEASES_API`] -- or `None` when there is none. A draft is nobody's
/// release; a pre-release is one, because every release of a beta is.
///
/// **The highest by number, not the first in the list**: GitHub lists by date,
/// and a fix released for an older line after a newer one is first and not
/// newest.
pub fn newest_release(json: &str) -> Result<Option<Release>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "GitHub's answer could not be read as a list of releases".to_owned())?;
    let list = value.as_array().ok_or_else(|| "GitHub's answer was not a list of releases".to_owned())?;
    let mut best: Option<(&str, &str)> = None;
    for release in list {
        if release.get("draft").and_then(|d| d.as_bool()).unwrap_or(false) {
            continue;
        }
        let Some(tag) = release.get("tag_name").and_then(|t| t.as_str()) else {
            continue;
        };
        let Some(version) = plain_version(tag) else {
            continue;
        };
        if best.is_none_or(|(b, _)| Relation::of(Some(b), version) == Relation::Older) {
            best = Some((version, tag));
        }
    }
    Ok(best.map(|(version, tag)| Release { version: version.to_owned(), tag: tag.to_owned() }))
}

/// What to tell the player, from this build's version and GitHub's answer.
pub fn verdict(this_version: &str, json: &str) -> Result<Verdict, String> {
    Ok(verdict_of(this_version, newest_release(json)?))
}

fn verdict_of(this_version: &str, newest: Option<Release>) -> Verdict {
    match newest {
        None => Verdict::NoRelease,
        // `Relation::of(installed, this)` reads *installed is OLDER than this*:
        // here *this build* is the installed one and the release is the other.
        Some(r) if Relation::of(Some(this_version), &r.version) == Relation::Older => {
            Verdict::Newer { latest: r.version, tag: r.tag }
        }
        Some(r) => Verdict::Newest { latest: r.version },
    }
}

/// `D-077`: where the release under `tag` is downloaded from -- this build's
/// releases page, the tag, and the program's own name, which is the name the
/// release workflow uploads it under. `None` for a tag that is not a plain
/// version, which no verdict carries.
pub fn download_url(tag: &str) -> Option<String> {
    plain_version(tag)?;
    Some(format!("{}/download/{tag}/{}", crate::gui::render::RELEASES_URL, crate::install::EXE_NAME))
}

/// `D-077`: the page of the release under `tag`: its notes, what is new.
pub fn notes_url(tag: &str) -> Option<String> {
    plain_version(tag)?;
    Some(format!("{}/tag/{tag}", crate::gui::render::RELEASES_URL))
}

/// `D-078`: what the gate's gold button hands to the browser on `system` --
/// the program itself on Windows, and on Linux the release's page, which holds
/// the AppImage, the `.deb` and the `.rpm` for the player to choose from.
pub fn download_for(tag: &str, system: crate::gui::lobby::System) -> Option<String> {
    match system {
        crate::gui::lobby::System::Windows => download_url(tag),
        crate::gui::lobby::System::Linux => notes_url(tag),
    }
}

/// `D-077`: the tag the releases page's `latest` redirect names, if it names
/// one of this repository's releases and nothing else.
///
/// The second way to ask, for when the API will not answer: `…/releases/latest`
/// answers with a redirect to the latest release's own page, and its one header
/// says which release that is. Only an absolute address under this build's own
/// releases page is read, and only a plain version is taken from it.
pub fn tag_from_location(location: &str) -> Option<&str> {
    let prefix = format!("{}/tag/", crate::gui::render::RELEASES_URL);
    let tag = location.strip_prefix(prefix.as_str())?;
    plain_version(tag).map(|_| tag)
}

/// The version this build compares with the releases: its own -- or, in a
/// binary built to be measured, `P2P_POKER_PRETEND_VERSION`, so that the window
/// that closes the lobby can be tried and photographed against GitHub itself
/// without keeping an old build, and so that a harness build can be told it is
/// ahead of everything. A player's build has no such knob: the one thing that
/// makes this check worth making is that it cannot be talked out of it.
pub fn compared_version() -> String {
    let pretended = if cfg!(feature = "fault-harness") { std::env::var("P2P_POKER_PRETEND_VERSION").ok() } else { None };
    pretended.filter(|v| plain_version(v).is_some()).unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned())
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
        .timeout(CHECK_TIMEOUT)
        .send()
        .map_err(|e| format!("GitHub could not be reached: {e}"))?;
    let status = response.status();
    if matches!(status.as_u16(), 403 | 429) {
        // GitHub's API answers sixty questions an hour from one address, and
        // one address may be a whole building's. The releases page says which
        // release is the latest in the one header of a redirect, outside that
        // count -- asked only now, so a client that is answered asks once.
        return latest_by_page().map_err(|why| {
            format!("GitHub is not answering this address right now (too many questions from it in the last hour), and {why}")
        });
    }
    if !status.is_success() {
        return Err(format!("GitHub answered {}", status.as_u16()));
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
    verdict(&compared_version(), &text)
}

/// `D-077`: the latest release from the releases page's redirect -- the header,
/// and not a byte of the page.
#[cfg(feature = "tox")]
fn latest_by_page() -> Result<Verdict, String> {
    let page = format!("{}/latest", crate::gui::render::RELEASES_URL);
    let response = attohttpc::get(&page)
        .header("User-Agent", "P2Poker")
        .follow_redirects(false)
        .timeout(CHECK_TIMEOUT)
        .send()
        .map_err(|e| format!("the releases page could not be reached either: {e}"))?;
    if !response.status().is_redirection() {
        return Err(format!("the releases page answered {}", response.status().as_u16()));
    }
    let location = response
        .headers()
        .get(attohttpc::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| "the releases page named no release".to_owned())?;
    let tag = tag_from_location(location).ok_or_else(|| "the releases page named no release of this project".to_owned())?;
    let version = plain_version(tag).unwrap_or(tag);
    Ok(verdict_of(&compared_version(), Some(Release { version: version.to_owned(), tag: tag.to_owned() })))
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

    fn newer(latest: &str, tag: &str) -> Verdict {
        Verdict::Newer { latest: latest.into(), tag: tag.into() }
    }

    /// **`D-075`: newer means a higher NUMBER, of a real release.** The breaks
    /// this must catch: versions compared as text (`0.10.0` under `0.9.0`), the
    /// first of the list taken for the newest, and a draft counted as out.
    #[test]
    fn newer_is_a_higher_number_of_a_real_release() {
        let json = releases(&[("v0.9.1", false, true), ("v0.10.0", false, true), ("v0.2.0", false, false)]);
        let best = newest_release(&json).unwrap().expect("a release");
        assert_eq!((best.version.as_str(), best.tag.as_str()), ("0.10.0", "v0.10.0"), "by number, and not the first listed");
        assert_eq!(verdict("0.9.1", &json).unwrap(), newer("0.10.0", "v0.10.0"));
        assert_eq!(verdict("0.10.0", &json).unwrap(), Verdict::Newest { latest: "0.10.0".into() });
        // A build AHEAD of every release -- the owner's own -- is told it is the newest, not nagged.
        assert_eq!(verdict("0.11.0", &json).unwrap(), Verdict::Newest { latest: "0.10.0".into() });

        // A draft is nobody's release.
        let json = releases(&[("v9.0.0", true, false), ("v0.1.0", false, true)]);
        assert_eq!(verdict("0.1.0", &json).unwrap(), Verdict::Newest { latest: "0.1.0".into() });

        // A pre-release is one: every release of a beta is.
        let json = releases(&[("v0.2.0", false, true)]);
        assert_eq!(verdict("0.1.0", &json).unwrap(), newer("0.2.0", "v0.2.0"));

        // The tag is carried as it was published, with or without its `v`.
        let json = releases(&[("0.3.0", false, false)]);
        assert_eq!(verdict("0.1.0", &json).unwrap(), newer("0.3.0", "0.3.0"));

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
    /// reads as -- and, since `D-077`, neither must *the lobby is closed*.
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

    /// **`D-077`: the download is this repository's program, of that release.**
    /// The host, the path and the name are the build's; the answer supplies the
    /// tag and nothing else, and a tag that is not a plain version supplies no
    /// address at all.
    #[test]
    fn the_download_is_this_repositorys_program_of_that_release() {
        assert_eq!(
            download_url("v0.1.2").as_deref(),
            Some("https://github.com/qavryxdevv/P2Poker/releases/download/v0.1.2/p2p-poker.exe")
        );
        assert_eq!(notes_url("v0.1.2").as_deref(), Some("https://github.com/qavryxdevv/P2Poker/releases/tag/v0.1.2"));
        // `D-078`: Windows gets the program, Linux the page with its three packages.
        use crate::gui::lobby::System;
        assert_eq!(download_for("v0.1.2", System::Windows), download_url("v0.1.2"));
        assert_eq!(download_for("v0.1.2", System::Linux), notes_url("v0.1.2"));
        assert_eq!(download_for("../x", System::Linux), None);
        for bad in ["../../../evil", "v0.1.2/../../x", "v0.1.2?download=elsewhere", "v0.1.2#x", "https://evil.example/", ""] {
            assert_eq!(download_url(bad), None, "{bad:?}");
            assert_eq!(notes_url(bad), None, "{bad:?}");
        }
    }

    /// **`D-077`: the releases page's redirect names the latest release, and only
    /// one of this repository's is taken from it.**
    #[test]
    fn the_releases_pages_redirect_names_a_release_of_this_repository_or_none() {
        assert_eq!(tag_from_location("https://github.com/qavryxdevv/P2Poker/releases/tag/v0.1.1"), Some("v0.1.1"));
        for bad in [
            "https://github.com/someone-else/P2Poker/releases/tag/v9.9.9",
            "https://github.com/qavryxdevv/P2Poker/releases/tag/v9.9.9/../../x",
            "https://github.com/qavryxdevv/P2Poker/releases/tag/latest",
            "https://github.com/qavryxdevv/P2Poker/releases",
            "/qavryxdevv/P2Poker/releases/tag/v9.9.9",
            "http://github.com/qavryxdevv/P2Poker/releases/tag/v9.9.9",
        ] {
            assert_eq!(tag_from_location(bad), None, "{bad:?}");
        }
        // And what it names is judged as any release is.
        let r = Release { version: "0.1.1".into(), tag: "v0.1.1".into() };
        assert_eq!(verdict_of("0.1.0", Some(r.clone())), newer("0.1.1", "v0.1.1"));
        assert_eq!(verdict_of("0.1.1", Some(r)), Verdict::Newest { latest: "0.1.1".into() });
    }
}
