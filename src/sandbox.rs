//! `D-082`: on Linux the client confines where it can write -- its profile and
//! the few places a window, sound and the desktop session need -- so that a
//! fault in it cannot delete or overwrite anything else of the player's.
//!
//! **Landlock**, the kernel's own sandbox for unprivileged programs (Linux 5.13
//! and later): the client asks for it itself, needs no root, no package format
//! and no store, and a package manager has no say in it. A `.deb`, an `.rpm`, an
//! AppImage and a `.tar.gz` are all *unconfined* formats -- the program runs with
//! every right its user has, and a package manager can only say so -- so the
//! limit has to come from the program.
//!
//! **Writes, not reads, and not the network.** Reading stays as it was: the
//! window, the GPU driver and the sound library read from all over the system,
//! and a list that forgot one would leave a player without a window on one
//! distribution. What is confined is what a fault can destroy. The network is a
//! peer-to-peer client's whole job.
//!
//! **Three rules come from the kernel, not from taste.** A restriction holds for
//! the thread that asks for it and every thread and process it starts after --
//! so it is asked for at the top of `main`, before a single other thread exists.
//! A process the client starts inherits it -- so the browser and the file manager
//! are opened by a small helper started just before the restriction
//! ([`spawn_opener`]), which keeps the rights the client gives up. And from ABI 5
//! (Linux 6.10) the device `ioctl`s the GPU and the sound card are driven by are
//! write rights too -- so `/dev` is among the places allowed.
//!
//! Nothing here is used on Windows, where the functions that act are absent and
//! [`writable_roots`] and the helper's words are only tested.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Where a confined client may still write: its profile, the cache Mesa keeps
/// its compiled shaders in, PulseAudio's client folder, `/tmp`, the session's
/// runtime folder (the sockets of the display and the sound server, the shared
/// memory Wayland draws through) and `/dev` (the GPU, the sound card, `/dev/null`).
/// Relative values of the `XDG_` variables are ignored, as the specification
/// says they must be; a missing `HOME` leaves the two that hang from it out.
pub fn writable_roots(profile: &Path, var: impl Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let absolute = |name: &str| var(name).map(PathBuf::from).filter(|p| p.has_root());
    let home = absolute("HOME");
    let mut roots = vec![profile.to_path_buf()];
    if let Some(cache) = absolute("XDG_CACHE_HOME").or_else(|| home.as_ref().map(|h| h.join(".cache"))) {
        roots.push(cache);
    }
    if let Some(config) = absolute("XDG_CONFIG_HOME").or_else(|| home.as_ref().map(|h| h.join(".config"))) {
        roots.push(config.join("pulse"));
    }
    roots.push(PathBuf::from("/tmp"));
    if let Some(run) = absolute("XDG_RUNTIME_DIR") {
        roots.push(run);
    }
    roots.push(PathBuf::from("/dev"));
    roots
}

/// What asking for the confinement came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confinement {
    /// Every write right asked for is enforced.
    Full,
    /// The kernel knows an older Landlock and enforces the rights it knows.
    Partial,
    /// Nothing is confined, and why: an old kernel, Landlock switched off, or
    /// the player's `--no-sandbox`.
    None(String),
}

impl Confinement {
    /// The line the client says it in at start.
    pub fn words(&self) -> String {
        match self {
            Confinement::Full => "writes confined to the profile, the cache, /tmp, the session's folder and /dev (Landlock)".into(),
            Confinement::Partial => {
                "writes confined to the profile, the cache, /tmp, the session's folder and /dev (Landlock, as far as this kernel knows it)".into()
            }
            Confinement::None(why) => format!("writes not confined: {why}"),
        }
    }

    pub fn confined(&self) -> bool {
        !matches!(self, Confinement::None(_))
    }
}

/// `D-082`: confine this process's writes to `roots`, for good. **Called before
/// any other thread exists**; a root that does not exist is created where it
/// can be (the cache, PulseAudio's folder) and left out where it cannot.
#[cfg(target_os = "linux")]
pub fn confine_writes(roots: &[PathBuf]) -> Confinement {
    use landlock::{AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr, RulesetStatus, ABI};
    // The write rights up to ABI 5 and no further: ABI 9 adds the resolving of
    // Unix sockets, which is how the window, the sound server and D-Bus are
    // reached, and a list of those is a list that forgets one.
    let access = AccessFs::from_write(ABI::V5);
    let created = match Ruleset::default().handle_access(access).and_then(|r| r.create()) {
        Ok(created) => created,
        Err(e) => return Confinement::None(format!("Landlock refused the ruleset ({e})")),
    };
    let mut created = created;
    for root in roots {
        let _ = std::fs::create_dir_all(root);
        let Ok(fd) = PathFd::new(root) else { continue };
        created = match created.add_rule(PathBeneath::new(fd, access)) {
            Ok(next) => next,
            Err(e) => return Confinement::None(format!("Landlock refused {} ({e})", root.display())),
        };
    }
    match created.restrict_self() {
        Ok(status) => match status.ruleset {
            RulesetStatus::FullyEnforced => Confinement::Full,
            RulesetStatus::PartiallyEnforced => Confinement::Partial,
            RulesetStatus::NotEnforced => Confinement::None("this kernel has no Landlock, or it is switched off".into()),
        },
        Err(e) => Confinement::None(format!("Landlock could not be applied ({e})")),
    }
}

/// `--sandbox-check`: whether a write in the profile still goes through and one
/// beside it in the home folder does not. `(inside, outside)`, each `true` when
/// it came out as a confined client's must.
pub fn check_writes(profile: &Path, outside: &Path) -> (bool, bool) {
    let inside_probe = profile.join(".sandbox-probe");
    let inside = std::fs::write(&inside_probe, b"probe").is_ok();
    let _ = std::fs::remove_file(&inside_probe);
    let refused = match std::fs::write(outside, b"probe") {
        Ok(()) => {
            let _ = std::fs::remove_file(outside);
            false
        }
        Err(e) => e.kind() == std::io::ErrorKind::PermissionDenied,
    };
    (inside, refused)
}

/// `D-082`: what a confined client asks the helper to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Open {
    /// An address for the browser. The helper opens only what the client
    /// itself would (`https://`, nothing that is not an address).
    Url(String),
    /// A folder for the file manager.
    Folder(PathBuf),
}

impl Open {
    /// One line on the pipe: a word, a tab, the value. A value with a line
    /// break or a tab in it is not sent: it could not be read back as sent.
    pub fn line(&self) -> Option<String> {
        let (word, value) = match self {
            Open::Url(url) => ("url", url.clone()),
            Open::Folder(path) => ("folder", path.to_str()?.to_owned()),
        };
        if value.is_empty() || value.contains(['\n', '\r', '\t']) {
            return None;
        }
        Some(format!("{word}\t{value}\n"))
    }

    /// A line read back; anything else is nothing.
    pub fn read(line: &str) -> Option<Open> {
        let (word, value) = line.trim_end_matches(['\n', '\r']).split_once('\t')?;
        if value.is_empty() || value.contains('\t') {
            return None;
        }
        match word {
            "url" => Some(Open::Url(value.to_owned())),
            "folder" => Some(Open::Folder(PathBuf::from(value))).filter(|o| matches!(o, Open::Folder(p) if p.has_root())),
            _ => None,
        }
    }
}

/// Whether this process confined its writes, said once at start.
static CONFINED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Said once, at start, by whoever asked for the confinement.
pub fn set_confined(confined: bool) {
    CONFINED.store(confined, std::sync::atomic::Ordering::Relaxed);
}

/// Whether this process's writes are confined -- where it is, anything it
/// starts itself is confined as well.
pub fn is_confined() -> bool {
    CONFINED.load(std::sync::atomic::Ordering::Relaxed)
}

/// The helper's pipe, once it is started.
static OPENER: std::sync::OnceLock<std::sync::Mutex<std::process::ChildStdin>> = std::sync::OnceLock::new();

/// `D-082`: start the helper that opens what a confined client cannot --
/// **before** the confinement, so that it and what it starts keep the rights
/// the client gives up. It is this program with `--opener`, reading lines from
/// its standard input and ending when the client does.
pub fn spawn_opener(program: &Path) -> bool {
    let child = std::process::Command::new(program)
        .arg("--opener")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn();
    match child.ok().and_then(|mut c| c.stdin.take()) {
        Some(pipe) => OPENER.set(std::sync::Mutex::new(pipe)).is_ok(),
        None => false,
    }
}

/// Hand `what` to the helper. `false` when there is no helper, or it is gone.
pub fn ask_opener(what: &Open) -> bool {
    use std::io::Write;
    let (Some(pipe), Some(line)) = (OPENER.get(), what.line()) else { return false };
    let mut pipe = pipe.lock().unwrap_or_else(|held| held.into_inner());
    pipe.write_all(line.as_bytes()).and_then(|_| pipe.flush()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The places a confined client may write, from the session's variables:
    /// the profile first, relative `XDG_` values ignored, the home's defaults
    /// when a variable is missing, and nothing hung from a home that is not set.
    ///
    /// The break that must make this fail: take a relative `XDG_CACHE_HOME` --
    /// a folder under whatever the working directory happens to be.
    #[test]
    fn a_confined_client_writes_to_its_profile_and_the_sessions_folders() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| OsString::from(v))
        };
        let profile = Path::new("/home/p/.local/share/p2poker/profile");
        let roots = writable_roots(profile, env(&[("HOME", "/home/p"), ("XDG_RUNTIME_DIR", "/run/user/1000")]));
        let want: Vec<PathBuf> = [
            "/home/p/.local/share/p2poker/profile",
            "/home/p/.cache",
            "/home/p/.config/pulse",
            "/tmp",
            "/run/user/1000",
            "/dev",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        assert_eq!(roots, want);

        let roots = writable_roots(
            profile,
            env(&[("HOME", "/home/p"), ("XDG_CACHE_HOME", "relative/cache"), ("XDG_CONFIG_HOME", "/cfg")]),
        );
        assert!(roots.contains(&PathBuf::from("/home/p/.cache")), "a relative XDG_CACHE_HOME is ignored");
        assert!(roots.contains(&PathBuf::from("/cfg/pulse")));
        assert!(!roots.iter().any(|r| !r.has_root()), "{roots:?}");

        let roots = writable_roots(profile, env(&[]));
        assert_eq!(roots, [profile.to_path_buf(), PathBuf::from("/tmp"), PathBuf::from("/dev")]);
    }

    /// The helper's words: what is sent is read back as sent, and a line that
    /// could smuggle a second request, or names nothing, is not sent at all.
    ///
    /// The break that must make this fail: send a value with a line break in
    /// it, which the helper would read as two requests.
    #[test]
    fn the_helper_reads_back_exactly_what_was_asked() {
        let url = Open::Url("https://github.com/qavryxdevv/P2Poker/releases".into());
        assert_eq!(Open::read(&url.line().unwrap()), Some(url));
        let folder = Open::Folder(PathBuf::from("/opt/p2poker"));
        assert_eq!(Open::read(&folder.line().unwrap()), Some(folder));

        assert_eq!(Open::Url("https://a\nurl\thttps://b".into()).line(), None, "a second request smuggled in");
        assert_eq!(Open::Url(String::new()).line(), None);
        assert_eq!(Open::read("folder\trelative/path\n"), None, "a folder is a whole path");
        assert_eq!(Open::read("run\t/bin/sh\n"), None, "only the two words");
        assert_eq!(Open::read("url\n"), None);
    }

    /// `D-082`'s order in `main`, read as it is written: the helper is started
    /// before the confinement (or it would be confined too), the confinement
    /// comes before the first thread -- the node's runtime and the window --
    /// and the helper opens only what the client itself would.
    ///
    /// The breaks that must make this fail: confine first and start the helper
    /// after; move the confinement below the window; let the helper open any
    /// line it reads.
    #[test]
    fn the_helper_is_started_first_and_the_confinement_before_any_thread() {
        let main = include_str!("main.rs");
        let helper = main.find("sandbox::spawn_opener(&program)").expect("the helper is started");
        let confine = main.find("sandbox::confine_writes(").expect("the confinement");
        assert!(helper < confine, "the helper before the confinement");
        let fn_main = main.find("\nfn main() {").expect("main");
        let first_thread = [main.find("headless(player, run, join);"), main.find("windowed(player, run);")]
            .into_iter()
            .map(|at| at.expect("where main hands over to the node and the window"))
            .min()
            .unwrap_or_default();
        assert!(fn_main < confine && confine < first_thread, "confined in main, before the node or the window");
        let serve = main.find("fn serve_opener() {").expect("the helper's loop");
        let body = &main[serve..serve + main[serve..].find("\n}\n").expect("its end")];
        assert!(body.contains("if is_a_web_address(&url)"), "an address only if the client would open it");
        assert!(body.contains("if folder.is_dir()"), "a folder only if it is one");
    }

    /// The line said at start, and the one question the client asks of it.
    #[test]
    fn the_confinement_says_what_it_is() {
        assert!(Confinement::Full.confined());
        assert!(Confinement::Partial.confined());
        let off = Confinement::None("--no-sandbox".into());
        assert!(!off.confined());
        assert_eq!(off.words(), "writes not confined: --no-sandbox");
        assert!(Confinement::Full.words().starts_with("writes confined to the profile"));
    }
}
