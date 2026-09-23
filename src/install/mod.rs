//! `D-073`: the client gives itself a home -- the first run on a new machine.
//!
//! `SPEC_CS.md` §22 asks for one portable folder: no installer, no registry,
//! nothing outside its own directory. That is still what this client **is**.
//! What the owner added on 2026-09-20 is where that folder goes when a player
//! who has never seen it double-clicks a download: left alone, the folder is
//! `Downloads`, the profile -- the player's identity, album and results -- is
//! made beside a file the player will tidy away next month, and nothing on the
//! desktop says how to start it again.
//!
//! So the client offers, once, to put itself somewhere it can stay: a copy in
//! the user's own programs folder, a shortcut on the desktop and one in the
//! Start menu. **It is still the same portable folder** -- program and profile
//! side by side, copyable to another machine as one directory -- and the offer
//! can be declined, which leaves exactly §22's behaviour.
//!
//! # What this never does
//!
//! Stated as the discipline the code follows, not as an absence to be trusted:
//!
//! * **No registry, no administrator rights, no autorun, no service, no
//!   scheduled task, no file association, no `PATH`.** Everything written is
//!   one folder and two shortcuts, and deleting those removes all of it.
//! * **Nothing is deleted and nothing of the player's is overwritten.** The file
//!   the player started stays where it is. A shortcut that points somewhere
//!   else keeps its name and ours takes another. A profile is never merged
//!   into, or put over, another profile.
//! * **Nothing is installed that was not verified.** The copy is hashed as it
//!   is written and hashed again from the disk before it takes the program's
//!   name (`copy::install_file`).
//! * **No running game is cut off.** An installed copy that is running is not
//!   replaced under it; the player is told to close it.
//! * **Never from a script.** Any argument but `--install`, `--portable` and
//!   `--renderer` means somebody is driving this client on purpose -- a bed, a
//!   test, a relay -- and it starts where it stands, as before.
//!
//! # The shape
//!
//! [`decide`] is a pure function of [`Facts`], so the table of who is asked and
//! who is not is a unit test. What stands at the install location is read by
//! [`survey`]. The work is `copy` (any platform, plain files) and `shell`
//! (Windows: the user's known folders, shortcuts read back after they are
//! written, the hand-over to the installed copy). The window is
//! `gui::installer`, and `main` only wires them.

use std::path::{Path, PathBuf};

pub mod copy;
#[cfg(windows)]
pub mod shell;

/// `D-080`: whether this copy runs from an MSIX package -- the Microsoft
/// Store's, which is the only place one comes from.
///
/// **Windows answers it, so one binary knows which of its two homes it is in.**
/// `GetCurrentPackageFamilyName` gives a packaged process its package family
/// name and every other process `APPMODEL_ERROR_NO_PACKAGE`; there is no build
/// flavour, no feature and nothing to get wrong at packaging time. Three things
/// hang off the answer: the update gate opens the Store instead of downloading
/// a file (`D-077`), the installer is never offered because the Store installs
/// and removes this copy (`D-073`), and the profile goes where a packaged app
/// may write rather than beside the program.
pub fn packaged() -> bool {
    family_name().is_some()
}

/// The package family name of this copy, which is also how the Store's own
/// page for it is addressed: `ms-windows-store://pdp/?PFN=<family name>`.
/// `None` for a copy that is not packaged, and on every other system.
#[cfg(windows)]
pub fn family_name() -> Option<String> {
    use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
    use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFamilyName;

    let mut len: u32 = 0;
    // SAFETY: the documented way to ask for the length -- a null buffer and a
    // length of zero, which the call fills in. Anything but "too small" here
    // means this process has no package, which is the ordinary case.
    if unsafe { GetCurrentPackageFamilyName(&mut len, std::ptr::null_mut()) } != ERROR_INSUFFICIENT_BUFFER {
        return None;
    }
    let mut name = vec![0u16; len as usize];
    // SAFETY: `name` holds the `len` units the call just asked for.
    if unsafe { GetCurrentPackageFamilyName(&mut len, name.as_mut_ptr()) } != 0 {
        return None;
    }
    // `len` counts the terminating NUL, which is not part of the name.
    name.truncate(len.saturating_sub(1) as usize);
    Some(String::from_utf16_lossy(&name))
}

#[cfg(not(windows))]
pub fn family_name() -> Option<String> {
    None
}

/// The folder under the user's programs folder.
pub const FOLDER_NAME: &str = "P2Poker";
/// The program's file name there. The same as the build's own: scripts, the
/// task list and the firewall's prompt all know this client by it.
pub const EXE_NAME: &str = "p2p-poker.exe";
/// What the shortcuts are called, without the `.lnk`.
pub const SHORTCUT_NAME: &str = "P2Poker";

/// `P2P_POKER_INSTALL_ROOT=DIR`: install under `DIR` instead of the user's own
/// folders -- `DIR\Programs`, `DIR\Desktop`, `DIR\StartMenu`.
///
/// In every build, deliberately: it is how the shipped binary's own flow is
/// photographed and tested without putting a shortcut on somebody's real
/// desktop. It crosses no boundary -- whoever sets this process's environment
/// already runs as this user and could write those folders directly.
pub const ROOT_OVERRIDE: &str = "P2P_POKER_INSTALL_ROOT";

/// This build's version, written into the binary between two marks so that
/// another copy of this client can read it **without running the file**.
///
/// An installer that has to decide between *update* and *do not downgrade*
/// needs the version of a program it must not start. A sidecar file goes stale
/// the first time somebody replaces the program by hand, which this project's
/// own deploy script does; the bytes of the program cannot disagree with the
/// program. Builds from before `D-073` carry no mark and read as *unknown*,
/// which [`Relation::of`] treats as older -- they are, by construction.
pub static BUILD_MARK: &str = concat!("\u{1}P2POKER-BUILD-VERSION=", env!("CARGO_PKG_VERSION"), "\u{1}");

/// How much of [`BUILD_MARK`] is the prefix. The needle is cut out of the mark
/// itself rather than written down again, so the bare prefix is not in the
/// binary a second time for a scan to trip on.
const MARK_PREFIX_LEN: usize = "\u{1}P2POKER-BUILD-VERSION=".len();

/// Where an installation goes. The two shortcut folders are optional because
/// a machine may honestly not have them (a redirected desktop that is offline),
/// and a program without a shortcut is still installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Places {
    /// The user's programs folder: `%LOCALAPPDATA%\Programs`.
    pub programs: PathBuf,
    pub desktop: Option<PathBuf>,
    pub start_menu: Option<PathBuf>,
}

impl Places {
    /// The three folders under one root, for [`ROOT_OVERRIDE`] and the tests.
    pub fn under(root: &Path) -> Places {
        Places {
            programs: root.join("Programs"),
            desktop: Some(root.join("Desktop")),
            start_menu: Some(root.join("StartMenu")),
        }
    }

    pub fn install_dir(&self) -> PathBuf {
        self.programs.join(FOLDER_NAME)
    }

    pub fn installed_exe(&self) -> PathBuf {
        self.install_dir().join(EXE_NAME)
    }

    /// The installed copy's profile: beside the program, as §22 has it.
    pub fn installed_profile(&self) -> PathBuf {
        self.install_dir().join("profile")
    }
}

/// What [`decide`] is told. Everything it needs and nothing it has to go and
/// find, so the decision is a table and the table is a test.
#[derive(Debug, Clone)]
pub struct Facts {
    /// The command line without the program's own name.
    pub args: Vec<String>,
    /// Whether this build can install at all (Windows).
    pub supported: bool,
    /// This executable runs from the install location already.
    pub at_home: bool,
    /// A player's profile stands beside this executable.
    pub player_here: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Start as the client always has, where it stands.
    RunHere,
    /// Show the installer's window instead of the client.
    Offer,
}

/// Whether this start is the installer's.
///
/// In order, and the order is the design:
///
/// 1. **`--portable`** -- the player, or the installer's own *run without
///    installing*, has answered already.
/// 2. **`--install`** -- asked for by name, from anywhere (the About page of a
///    portable copy does this).
/// 3. **Any other argument** -- a script, a bed, a relay, a preview. Never
///    interrupted. `--renderer` alone does not count: it is what this client
///    passes to itself when it starts again in software.
/// 4. **At home** -- this is the installed copy.
/// 5. **A player lives beside this file** -- an established portable folder.
///    The owner's two clients on one machine are this case, and so is every
///    folder from before `D-073`. They are never asked.
/// 6. Otherwise this is a first run with nothing beside it: **offer**.
pub fn decide(facts: &Facts) -> Decision {
    if !facts.supported {
        return Decision::RunHere;
    }
    let args = without_renderer(&facts.args);
    if args.iter().any(|a| *a == "--portable") {
        return Decision::RunHere;
    }
    if args.iter().any(|a| *a == "--install") {
        return Decision::Offer;
    }
    if !args.is_empty() || facts.at_home || facts.player_here {
        return Decision::RunHere;
    }
    Decision::Offer
}

/// The arguments minus `--renderer` **and its value**.
fn without_renderer(args: &[String]) -> Vec<&String> {
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if a == "--renderer" {
            skip = true;
            continue;
        }
        out.push(a);
    }
    out
}

/// Whether `dir` holds a player: either key is enough. A profile folder with
/// only a lock file or a log in it is nobody yet.
pub fn holds_a_player(profile: &Path) -> bool {
    crate::storage::profile::identity_path(profile).is_file() || crate::storage::profile::app_key_path(profile).is_file()
}

/// Whether two paths are one place. Canonical where the system will say, as
/// written where it will not, and without regard to case either way: this is
/// asked about Windows paths.
pub fn same_place(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let text = |p: PathBuf| p.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase();
    text(canon(a)) == text(canon(b))
}

/// How the installed program's version stands to this one's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Older,
    /// The same version and other bytes: another build of it. A beta is
    /// rebuilt many times under one number, and the player has just started
    /// **this** file on purpose, so this one wins.
    SameVersion,
    Newer,
}

impl Relation {
    /// `installed` is `None` for a program with no mark, which is one from
    /// before the mark existed.
    pub fn of(installed: Option<&str>, this: &str) -> Relation {
        let Some(installed) = installed else {
            return Relation::Older;
        };
        match numbers(installed).cmp(&numbers(this)) {
            std::cmp::Ordering::Less => Relation::Older,
            std::cmp::Ordering::Equal => Relation::SameVersion,
            std::cmp::Ordering::Greater => Relation::Newer,
        }
    }
}

/// `major.minor.patch` as numbers; whatever follows a `-` or a `+` is not
/// ordered. A part that is not a number reads as zero rather than as an error:
/// the answer decides a button's label, not a security property.
fn numbers(version: &str) -> [u64; 3] {
    let core = version.split(['-', '+']).next().unwrap_or("");
    let mut out = [0u64; 3];
    for (slot, part) in out.iter_mut().zip(core.split('.')) {
        *slot = part.parse().unwrap_or(0);
    }
    out
}

/// The version a program's bytes state about themselves, if they state one.
pub fn version_in(bytes: &[u8]) -> Option<String> {
    let needle = BUILD_MARK[..MARK_PREFIX_LEN].as_bytes();
    let mut from = 0;
    while let Some(at) = find(&bytes[from..], needle) {
        let start = from + at + needle.len();
        let rest = &bytes[start..bytes.len().min(start + 40)];
        if let Some(end) = rest.iter().position(|b| *b == 1) {
            let text = &rest[..end];
            let plausible = !text.is_empty() && text.iter().all(|b| b.is_ascii_alphanumeric() || b".-+".contains(b));
            if plausible {
                return Some(String::from_utf8_lossy(text).into_owned());
            }
        }
        from = start;
    }
    None
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// This build's own version, read out of its own mark -- the same way another
/// copy would read it, so a mark that stopped parsing fails here first.
pub fn this_version() -> String {
    version_in(BUILD_MARK.as_bytes()).unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned())
}

/// What stands at the install location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    Nothing,
    /// The same bytes as this file.
    SameBuild,
    OtherBuild { version: Option<String>, relation: Relation },
}

/// Whether this file's own folder can be a home for a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Portable {
    Fine,
    /// Under the system's temporary folder -- started from inside an archive,
    /// most often. A profile made here is deleted by the next clean-up, and it
    /// is the player's identity.
    Temporary,
    /// The folder cannot be written: a read-only medium or share.
    ReadOnly,
}

/// Everything the installer's window needs to make its offer.
#[derive(Debug, Clone)]
pub struct Offer {
    /// This executable.
    pub source: PathBuf,
    pub places: Places,
    pub installed: Installed,
    pub this_version: String,
    pub portable: Portable,
    /// A player's profile beside this file, and none at the install location:
    /// it can move with the program.
    pub profile_can_move: bool,
    /// Both places hold a player. Neither is touched.
    pub two_players: bool,
    /// The desktop has no shortcut to the installed copy.
    pub desktop_shortcut_missing: bool,
}

/// Read what is there. Hashes two programs when one is installed, so it is
/// called off the paint thread.
pub fn survey(source: &Path, places: Places, desktop_shortcut_missing: bool) -> Offer {
    let target = places.installed_exe();
    let installed = if !target.is_file() {
        Installed::Nothing
    } else {
        match (copy::sha256_file(source), copy::sha256_file(&target)) {
            (Ok(a), Ok(b)) if a == b => Installed::SameBuild,
            _ => {
                let version = std::fs::read(&target).ok().and_then(|bytes| version_in(&bytes));
                let relation = Relation::of(version.as_deref(), &this_version());
                Installed::OtherBuild { version, relation }
            }
        }
    };
    let here = source.parent().map(|d| d.join("profile")).unwrap_or_default();
    let player_here = holds_a_player(&here) && !same_place(&here, &places.installed_profile());
    let player_there = holds_a_player(&places.installed_profile());
    Offer {
        source: source.to_path_buf(),
        portable: portable(source),
        installed,
        this_version: this_version(),
        profile_can_move: player_here && !player_there,
        two_players: player_here && player_there,
        desktop_shortcut_missing,
        places,
    }
}

/// Whether the folder this file is in can keep a profile.
pub fn portable(source: &Path) -> Portable {
    portable_given(source, &std::env::temp_dir())
}

/// [`portable`], told where the temporary folder is -- so that the test can
/// name one. The real one cannot be tested against honestly: a process started
/// from inside a packaged application has its writes beside `...\Local\Temp`
/// quietly redirected into the package's own store, and a *look-alike folder
/// beside the temporary one* made there is somewhere else entirely. Measured,
/// when the first version of this test passed under the very break it was
/// written to catch.
fn portable_given(source: &Path, temp: &Path) -> Portable {
    let Some(dir) = source.parent() else {
        return Portable::ReadOnly;
    };
    // By components, not by spelling: `...\Temp2\` is not under `...\Temp\`.
    let lower = |p: &Path| {
        PathBuf::from(std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()).to_string_lossy().to_lowercase())
    };
    if lower(dir).starts_with(lower(temp)) {
        return Portable::Temporary;
    }
    // Asked by doing it: permissions, a read-only medium and a folder some
    // security tool guards all answer the same way, and nothing else does.
    let probe = dir.join(format!(".p2poker-write-test-{}", std::process::id()));
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(file) => {
            drop(file);
            let _ = std::fs::remove_file(&probe);
            Portable::Fine
        }
        Err(_) => Portable::ReadOnly,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(args: &[&str]) -> Facts {
        Facts { args: args.iter().map(|s| s.to_string()).collect(), supported: true, at_home: false, player_here: false }
    }

    /// `D-080`: the question Windows is asked about this copy -- *are you in a
    /// package?* -- answered for a process that is not. This test's own is
    /// not, so the answer is known, and asking is what shows the call works
    /// at all: a wrong signature or a wrong error code would read *packaged*
    /// here, and every copy from GitHub would then refuse to install itself
    /// and look for its profile somewhere else.
    #[test]
    fn a_copy_that_is_not_in_a_package_says_so() {
        assert_eq!(family_name(), None, "the test process runs from no package");
        assert!(!packaged());
    }

    /// **`D-073`: who is asked, and who is never interrupted.** The table in
    /// `decide`'s own words, one line each.
    ///
    /// The breaks this must catch: a bed seat or a headless relay shown a
    /// window nobody is there to answer; the owner's two established folders
    /// asked to move; and the installed copy offering to install itself.
    #[test]
    fn a_first_run_is_offered_a_home_and_nobody_else_is_asked() {
        // A double-click on a download: nothing beside it, no arguments.
        assert_eq!(decide(&facts(&[])), Decision::Offer);
        // The client's own second start in software is still that first run.
        assert_eq!(decide(&facts(&["--renderer", "software"])), Decision::Offer);

        // A script is never interrupted -- any argument at all.
        for args in [
            &["--headless"][..],
            &["--profile", "D"],
            &["--headless", "--host", "T", "--for", "45"],
            &["--table-preview"],
            &["--renderer", "software", "--profile", "D"],
            &["--search", "auto"],
        ] {
            assert_eq!(decide(&facts(args)), Decision::RunHere, "{args:?}");
        }

        // An established portable folder: a player lives beside the file.
        assert_eq!(decide(&Facts { player_here: true, ..facts(&[]) }), Decision::RunHere);
        // The installed copy does not offer to install itself.
        assert_eq!(decide(&Facts { at_home: true, ..facts(&[]) }), Decision::RunHere);
        // Not on a system this cannot install on.
        assert_eq!(decide(&Facts { supported: false, ..facts(&[]) }), Decision::RunHere);
        assert_eq!(decide(&Facts { supported: false, ..facts(&["--install"]) }), Decision::RunHere);

        // Asked for by name: from an established folder too.
        assert_eq!(decide(&Facts { player_here: true, ..facts(&["--install"]) }), Decision::Offer);
        assert_eq!(decide(&facts(&["--install", "--renderer", "software"])), Decision::Offer);
        // And declined by name: that wins over everything, `--install` included.
        assert_eq!(decide(&facts(&["--portable"])), Decision::RunHere);
        assert_eq!(decide(&facts(&["--portable", "--install"])), Decision::RunHere);
    }

    /// `--renderer` goes **with its value**, or `software` would read as an
    /// argument of the player's and a first run in software would never be
    /// offered a home.
    #[test]
    fn the_renderer_flag_goes_with_its_value() {
        let args: Vec<String> = ["--renderer", "software", "--x"].iter().map(|s| s.to_string()).collect();
        assert_eq!(without_renderer(&args), vec![&"--x".to_string()]);
        let only: Vec<String> = ["--renderer", "gl"].iter().map(|s| s.to_string()).collect();
        assert!(without_renderer(&only).is_empty());
    }

    /// The binary states its version to whoever reads its bytes, and a build
    /// from before the mark reads as older.
    #[test]
    fn a_program_states_its_version_without_being_run() {
        assert_eq!(this_version(), env!("CARGO_PKG_VERSION"));

        let mut program = vec![0u8; 5_000];
        program.extend_from_slice("\u{1}P2POKER-BUILD-VERSION=9.4.17\u{1}".as_bytes());
        program.extend_from_slice(&[7u8; 3_000]);
        assert_eq!(version_in(&program).as_deref(), Some("9.4.17"));

        // The bare prefix followed by something that is no version -- a needle
        // in a scanner's own code, say -- is passed over, not returned.
        let mut tricky = "\u{1}P2POKER-BUILD-VERSION=\0\0garbage".as_bytes().to_vec();
        tricky.extend_from_slice("\u{1}P2POKER-BUILD-VERSION=1.2.3-beta+7\u{1}".as_bytes());
        assert_eq!(version_in(&tricky).as_deref(), Some("1.2.3-beta+7"));

        assert_eq!(version_in(b"no mark in here at all"), None);
        assert_eq!(version_in(b""), None);
    }

    #[test]
    fn versions_are_ordered_by_their_numbers() {
        assert_eq!(Relation::of(None, "0.1.0"), Relation::Older, "no mark: from before the mark");
        assert_eq!(Relation::of(Some("0.1.0"), "0.1.0"), Relation::SameVersion);
        assert_eq!(Relation::of(Some("0.1.0"), "0.2.0"), Relation::Older);
        assert_eq!(Relation::of(Some("0.10.0"), "0.9.9"), Relation::Newer, "numbers, not text");
        assert_eq!(Relation::of(Some("1.0.0-beta"), "1.0.0"), Relation::SameVersion);
        assert_eq!(Relation::of(Some("2"), "1.9.9"), Relation::Newer);
    }

    #[test]
    fn the_places_are_one_folder_and_the_profile_is_beside_the_program() {
        let p = Places::under(Path::new("R"));
        assert_eq!(p.install_dir(), Path::new("R").join("Programs").join("P2Poker"));
        assert_eq!(p.installed_exe(), p.install_dir().join("p2p-poker.exe"));
        assert_eq!(p.installed_profile(), p.install_dir().join("profile"));
    }

    /// A folder under the system's temporary one cannot keep a profile, and a
    /// folder whose name merely **begins** like it can.
    #[test]
    fn a_temporary_folder_is_told_from_one_that_is_spelled_like_it() {
        // The real one, read only: a folder under it cannot keep a profile.
        let inside = std::env::temp_dir().join("p2p-poker-test-install-portable");
        std::fs::create_dir_all(&inside).unwrap();
        assert_eq!(portable(&inside.join("p2p-poker.exe")), Portable::Temporary);

        // And a named one, with a neighbour whose name only BEGINS like it.
        let root = std::env::temp_dir().join("p2p-poker-test-install-lookalike");
        let _ = std::fs::remove_dir_all(&root);
        let (temp, beside) = (root.join("Temp"), root.join("Temp-and-more"));
        std::fs::create_dir_all(temp.join("zip")).unwrap();
        std::fs::create_dir_all(&beside).unwrap();
        assert_eq!(portable_given(&temp.join("zip").join("p2p-poker.exe"), &temp), Portable::Temporary);
        assert_eq!(portable_given(&beside.join("p2p-poker.exe"), &temp), Portable::Fine, "spelled like it, not inside it");
        // Windows paths: the same folder shouted is the same folder.
        if cfg!(windows) {
            let shouted = PathBuf::from(temp.to_string_lossy().to_uppercase());
            assert_eq!(portable_given(&temp.join("zip").join("p2p-poker.exe"), &shouted), Portable::Temporary);
        }
        assert_eq!(portable(Path::new("")), Portable::ReadOnly, "nowhere is not a home");
    }

    #[test]
    fn one_place_under_two_spellings_is_one_place() {
        let dir = std::env::temp_dir();
        let shouted = PathBuf::from(dir.to_string_lossy().to_uppercase());
        if cfg!(windows) {
            assert!(same_place(&dir, &shouted));
        }
        assert!(same_place(&dir, &dir.join(".")));
        assert!(!same_place(&dir, &dir.join("somewhere-else")));
    }
}
