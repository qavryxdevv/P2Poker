//! `D-073`, the Windows half: where this user's folders really are, shortcuts
//! that are **read back after they are written**, and the hand-over to the
//! installed copy.
//!
//! Every path here comes from the system's own answer (`SHGetKnownFolderPath`)
//! and never from `%USERPROFILE%\Desktop` glued together by hand: a desktop
//! kept in OneDrive, a redirected profile and a Windows in another language
//! are all ordinary, and all three break the glued path.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

use windows::core::{Interface, GUID, HSTRING};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED, STGM_READ,
};
use windows::Win32::UI::Shell::{
    IShellLinkW, SHGetKnownFolderPath, ShellLink, FOLDERID_Desktop, FOLDERID_LocalAppData, FOLDERID_Programs,
    FOLDERID_UserProgramFiles, KF_FLAG_DEFAULT, KF_FLAG_DONT_VERIFY, KNOWN_FOLDER_FLAG, SLGP_RAWPATH,
};

use super::copy::{self, Copied, CopyError, MoveError, Moved};
use super::{same_place, Installed, Offer, Places, Relation, ROOT_OVERRIDE, SHORTCUT_NAME};

/// COM on this thread for as long as this lives. The installer's work runs on
/// a thread of its own, so the apartment is its own too.
struct Com(bool);

impl Com {
    fn enter() -> Com {
        // SAFETY: no reserved pointer, a documented flag. `S_OK` and `S_FALSE`
        // (already initialised here) both owe a `CoUninitialize`; a failure --
        // another threading model was chosen first -- owes none.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        Com(hr.is_ok())
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the successful `CoInitializeEx` above, on the
            // same thread -- `Com` is not `Send`, holding a raw flag only by
            // convention, and is never moved off the thread that made it.
            unsafe { CoUninitialize() };
        }
    }
}

fn known(id: &GUID, flag: KNOWN_FOLDER_FLAG) -> Option<PathBuf> {
    // SAFETY: `id` is a constant of the bindings; the returned buffer is the
    // caller's to free with `CoTaskMemFree`, which is done on every path once
    // the text has been copied out of it.
    unsafe {
        let text = SHGetKnownFolderPath(id, flag, None).ok()?;
        let path = PathBuf::from(OsString::from_wide(text.as_wide()));
        CoTaskMemFree(Some(text.0 as *const core::ffi::c_void));
        (!path.as_os_str().is_empty()).then_some(path)
    }
}

/// Where an installation goes on this machine, for this user.
///
/// The programs folder is asked for **without verifying it exists**: on a new
/// Windows account it often does not yet, and asking the system to create it
/// would be writing to the disk before the player has agreed to anything.
pub fn places() -> Option<Places> {
    if let Some(root) = std::env::var_os(ROOT_OVERRIDE).filter(|v| !v.is_empty()) {
        let root = std::path::absolute(PathBuf::from(root)).ok()?;
        return Some(Places::under(&root));
    }
    let programs = known(&FOLDERID_UserProgramFiles, KF_FLAG_DONT_VERIFY)
        .or_else(|| known(&FOLDERID_LocalAppData, KF_FLAG_DEFAULT).map(|local| local.join("Programs")))?;
    Some(Places {
        programs,
        desktop: known(&FOLDERID_Desktop, KF_FLAG_DEFAULT),
        start_menu: known(&FOLDERID_Programs, KF_FLAG_DEFAULT),
    })
}

fn words(e: windows::core::Error) -> String {
    let text = e.message();
    if text.is_empty() {
        format!("{:#010x}", e.code().0)
    } else {
        format!("{text} ({:#010x})", e.code().0)
    }
}

/// Write a shortcut to `target` at `lnk`: started in the program's own folder,
/// wearing the program's own icon.
pub fn create_shortcut(lnk: &Path, target: &Path, description: &str) -> Result<(), String> {
    let _com = Com::enter();
    // SAFETY: plain COM calls on interfaces this function owns; every string is
    // an `HSTRING` that outlives the call it is passed to.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(words)?;
        link.SetPath(&HSTRING::from(target.as_os_str())).map_err(words)?;
        if let Some(dir) = target.parent() {
            link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str())).map_err(words)?;
        }
        link.SetDescription(&HSTRING::from(description)).map_err(words)?;
        link.SetIconLocation(&HSTRING::from(target.as_os_str()), 0).map_err(words)?;
        let file: IPersistFile = link.cast().map_err(words)?;
        file.Save(&HSTRING::from(lnk.as_os_str()), true).map_err(words)?;
    }
    Ok(())
}

/// Where the shortcut at `lnk` points, as the system reads it.
pub fn shortcut_target(lnk: &Path) -> Result<PathBuf, String> {
    let _com = Com::enter();
    // SAFETY: as above; the buffer is ours and the find-data pointer may be null.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(words)?;
        let file: IPersistFile = link.cast().map_err(words)?;
        file.Load(&HSTRING::from(lnk.as_os_str()), STGM_READ).map_err(words)?;
        let mut buffer = [0u16; 1_024];
        link.GetPath(&mut buffer, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32).map_err(words)?;
        let len = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        Ok(PathBuf::from(OsString::from_wide(&buffer[..len])))
    }
}

/// The names a shortcut of ours may take in a folder, in the order tried.
fn shortcut_names() -> impl Iterator<Item = String> {
    std::iter::once(format!("{SHORTCUT_NAME}.lnk")).chain((2..=9).map(|n| format!("{SHORTCUT_NAME} ({n}).lnk")))
}

/// The shortcut in `dir` that already points at `target`, if there is one.
pub fn shortcut_to(dir: &Path, target: &Path) -> Option<PathBuf> {
    shortcut_names()
        .map(|name| dir.join(name))
        .find(|lnk| lnk.is_file() && shortcut_target(lnk).is_ok_and(|t| same_place(&t, target)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The player did not ask for it.
    NotAsked,
    Made(PathBuf),
    /// One was there already, pointing at this very program.
    AlreadyThere(PathBuf),
    /// This machine has no such folder to put it in.
    NoFolder,
    Failed(String),
}

/// Put a shortcut to `target` into `dir` **without taking anybody's name**.
///
/// A `P2Poker.lnk` that points somewhere else is the player's own -- to a
/// portable copy, most likely -- and stays exactly as it is; ours becomes
/// `P2Poker (2).lnk`. What is written is then **read back through the system**
/// and must name the program, or it is removed again: a shortcut is the one
/// thing the player will ever click, and a desktop blocked by a security tool
/// fails in ways that only reading back shows.
pub fn place_shortcut(dir: Option<&Path>, target: &Path) -> Step {
    let Some(dir) = dir else {
        return Step::NoFolder;
    };
    if let Some(there) = shortcut_to(dir, target) {
        return Step::AlreadyThere(there);
    }
    if let Err(e) = std::fs::create_dir_all(dir) {
        return Step::Failed(e.to_string());
    }
    let Some(lnk) = shortcut_names().map(|name| dir.join(name)).find(|lnk| !lnk.exists()) else {
        return Step::Failed(format!("every name from {SHORTCUT_NAME} to {SHORTCUT_NAME} (9) is taken"));
    };
    if let Err(e) = create_shortcut(&lnk, target, "P2Poker \u{2014} decentralised poker") {
        let _ = std::fs::remove_file(&lnk);
        return Step::Failed(e);
    }
    match shortcut_target(&lnk) {
        Ok(read) if same_place(&read, target) => Step::Made(lnk),
        Ok(read) => {
            let _ = std::fs::remove_file(&lnk);
            Step::Failed(format!("the shortcut read back as {}", read.display()))
        }
        Err(e) => {
            let _ = std::fs::remove_file(&lnk);
            Step::Failed(e)
        }
    }
}

/// What the player ticked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choices {
    pub desktop: bool,
    pub start_menu: bool,
    pub move_profile: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileStep {
    NotAsked,
    Moved(Moved),
}

/// What was done, step by step and as it really went -- the window's last page
/// is this, said in words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub program: PathBuf,
    /// `None`: the same build stood there already and was left alone.
    pub copied: Option<Copied>,
    pub profile: ProfileStep,
    pub desktop: Step,
    pub start_menu: Step,
}

#[derive(Debug)]
pub enum InstallError {
    Copy(CopyError),
    /// The program is in place and the profile is where it was. No shortcut
    /// was made and nothing should be started: a client started there now
    /// would make a **new** player, and the move could never be made after.
    Profile(MoveError),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::Copy(e) => write!(f, "{e}"),
            InstallError::Profile(e) => write!(
                f,
                "Your player profile could not be moved: {e}. Nothing of yours was changed, and the installed copy \
                 has not been started. Close any running P2Poker and try again."
            ),
        }
    }
}

/// The installation: the program, then the profile, then the shortcuts.
///
/// **In that order on purpose.** A verified program with nothing pointing at it
/// harms nobody if a later step fails. The profile goes before the shortcuts
/// because a failed move must end the whole thing *before* there is anything to
/// click: see [`InstallError::Profile`].
pub fn install(offer: &Offer, choices: Choices, now_unix_ms: u64) -> Result<Report, InstallError> {
    let program = offer.places.installed_exe();
    // **Never a downgrade, and that is this function's rule rather than a
    // button's.** The same build is left alone because there is nothing to do;
    // a newer one because a profile a newer client has written may be one an
    // older client cannot read, and the profile is the player.
    let leave = matches!(
        offer.installed,
        Installed::SameBuild | Installed::OtherBuild { relation: Relation::Newer, .. }
    );
    let copied = if leave {
        None
    } else {
        Some(copy::install_file(&offer.source, &program).map_err(InstallError::Copy)?)
    };
    let profile = if choices.move_profile && offer.profile_can_move {
        let from = offer.source.parent().map(|d| d.join("profile")).unwrap_or_default();
        let moved = copy::move_profile(&from, &offer.places.installed_profile(), now_unix_ms);
        ProfileStep::Moved(moved.map_err(InstallError::Profile)?)
    } else {
        ProfileStep::NotAsked
    };
    let place = |asked: bool, dir: Option<&PathBuf>| {
        if asked {
            place_shortcut(dir.map(PathBuf::as_path), &program)
        } else {
            Step::NotAsked
        }
    };
    Ok(Report {
        desktop: place(choices.desktop, offer.places.desktop.as_ref()),
        start_menu: place(choices.start_menu, offer.places.start_menu.as_ref()),
        program,
        copied,
        profile,
    })
}

/// Whether the desktop lacks a shortcut to the installed copy.
pub fn desktop_shortcut_missing(places: &Places) -> bool {
    places.desktop.as_deref().is_some_and(|d| shortcut_to(d, &places.installed_exe()).is_none())
}

/// Start `program` from its own folder, with no arguments, and let it go.
pub fn start(program: &Path) -> std::io::Result<()> {
    let mut command = std::process::Command::new(program);
    if let Some(dir) = program.parent() {
        command.current_dir(dir);
    }
    command.spawn().map(|_| ())
}

/// The folder opened in Explorer with `path` selected in it.
pub fn show_in_explorer(path: &Path) {
    // `raw_arg`: Explorer wants `/select,"path"` exactly, and quoting the whole
    // argument -- which is what `arg` does to a path with a space in it -- makes
    // it open the Documents folder instead. A Windows path cannot hold a quote.
    let _ = std::process::Command::new("explorer").raw_arg(format!("/select,\"{}\"", path.display())).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("p2p-poker-test-shell-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn program(path: &Path, fill: u8) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![fill; 200_000]).unwrap();
    }

    /// **A shortcut is written, and then the system is asked where it points.**
    /// In a folder with a space and an accent in its name, because a user's
    /// own name is in every one of these paths.
    #[test]
    fn a_shortcut_is_read_back_through_the_system() {
        let dir = scratch("lnk");
        let target = dir.join("Pro gramy ěšč").join("p2p-poker.exe");
        program(&target, 1);
        let lnk = dir.join("Plocha ž").join("P2Poker.lnk");
        fs::create_dir_all(lnk.parent().unwrap()).unwrap();

        create_shortcut(&lnk, &target, "P2Poker").expect("the shell writes a link");
        assert!(lnk.is_file());
        let read = shortcut_target(&lnk).expect("and reads it back");
        assert!(same_place(&read, &target), "{} is not {}", read.display(), target.display());
    }

    /// **Nobody's shortcut is taken.** The player's own `P2Poker.lnk`, to a
    /// portable copy, stays what it was; ours takes the next name; and a second
    /// installation finds its own and makes no third.
    #[test]
    fn a_shortcut_of_the_players_own_keeps_its_name() {
        let dir = scratch("names");
        let (mine, theirs) = (dir.join("installed").join("p2p-poker.exe"), dir.join("portable").join("p2p-poker.exe"));
        program(&mine, 1);
        program(&theirs, 2);
        let desktop = dir.join("Desktop");
        fs::create_dir_all(&desktop).unwrap();
        let own = desktop.join("P2Poker.lnk");
        create_shortcut(&own, &theirs, "the player's own").unwrap();

        let placed = place_shortcut(Some(&desktop), &mine);
        assert_eq!(placed, Step::Made(desktop.join("P2Poker (2).lnk")));
        assert!(same_place(&shortcut_target(&own).unwrap(), &theirs), "the player's own link still points where it did");

        assert_eq!(place_shortcut(Some(&desktop), &mine), Step::AlreadyThere(desktop.join("P2Poker (2).lnk")));
        assert_eq!(fs::read_dir(&desktop).unwrap().count(), 2, "and there is no third");
        assert_eq!(place_shortcut(None, &mine), Step::NoFolder);
    }

    fn offer_in(root: &Path, source: &Path) -> Offer {
        let places = Places::under(root);
        let missing = desktop_shortcut_missing(&places);
        super::super::survey(source, places, missing)
    }

    /// **The whole installation on a scratch root**: the program verified into
    /// its folder, both shortcuts pointing at it, the file that was started
    /// untouched -- and a second run over it is *the same build*, copies
    /// nothing, and makes no second shortcut.
    #[test]
    fn a_first_run_installs_and_a_second_changes_nothing() {
        let dir = scratch("whole");
        let source = dir.join("Downloads").join("p2p-poker.exe");
        program(&source, 7);
        let root = dir.join("root");

        let offer = offer_in(&root, &source);
        assert_eq!(offer.installed, Installed::Nothing);
        assert!(!offer.profile_can_move && !offer.two_players);
        assert!(offer.desktop_shortcut_missing);

        let all = Choices { desktop: true, start_menu: true, move_profile: true };
        let report = install(&offer, all, 1).expect("a first installation");
        let program_at = root.join("Programs").join("P2Poker").join("p2p-poker.exe");
        assert_eq!(report.program, program_at);
        assert!(report.copied.as_ref().is_some_and(|c| !c.replaced && c.bytes == 200_000));
        assert_eq!(report.desktop, Step::Made(root.join("Desktop").join("P2Poker.lnk")));
        assert_eq!(report.start_menu, Step::Made(root.join("StartMenu").join("P2Poker.lnk")));
        assert_eq!(report.profile, ProfileStep::NotAsked, "there was no profile to move");
        assert_eq!(fs::read(&source).unwrap(), vec![7u8; 200_000], "the download is as it was");

        let again = offer_in(&root, &source);
        assert_eq!(again.installed, Installed::SameBuild);
        assert!(!again.desktop_shortcut_missing);
        let report = install(&again, all, 2).expect("a second run");
        assert_eq!(report.copied, None, "the same build is not copied again");
        assert!(matches!(report.desktop, Step::AlreadyThere(_)) && matches!(report.start_menu, Step::AlreadyThere(_)));
    }

    /// An update, a refusal to downgrade's raw material, and a player who asked
    /// for no shortcuts getting none.
    #[test]
    fn another_build_is_an_update_and_an_unticked_box_is_obeyed() {
        let dir = scratch("update");
        let old = dir.join("old").join("p2p-poker.exe");
        program(&old, 1);
        let root = dir.join("root");
        let quiet = Choices { desktop: false, start_menu: false, move_profile: false };
        let report = install(&offer_in(&root, &old), quiet, 1).unwrap();
        assert_eq!((report.desktop, report.start_menu), (Step::NotAsked, Step::NotAsked));
        assert!(!root.join("Desktop").exists(), "an unticked box writes nothing, the folder included");

        let new = dir.join("new").join("p2p-poker.exe");
        let mut bytes = vec![2u8; 150_000];
        bytes.extend_from_slice("\u{1}P2POKER-BUILD-VERSION=99.0.0\u{1}".as_bytes());
        fs::create_dir_all(new.parent().unwrap()).unwrap();
        fs::write(&new, &bytes).unwrap();

        // The installed one has no mark: it reads as older, and is replaced.
        let offer = offer_in(&root, &new);
        assert_eq!(offer.installed, Installed::OtherBuild { version: None, relation: Relation::Older });
        let report = install(&offer, quiet, 2).unwrap();
        assert!(report.copied.is_some_and(|c| c.replaced));

        // And seen from the old file, what is installed now is NEWER -- and
        // **an installation from the old file replaces nothing**, whatever a
        // window might have been made to ask for: the rule is `install`'s own.
        let back = offer_in(&root, &old);
        assert_eq!(back.installed, Installed::OtherBuild { version: Some("99.0.0".into()), relation: Relation::Newer });
        let kept = copy::sha256_file(&root.join("Programs").join("P2Poker").join("p2p-poker.exe")).unwrap();
        let report = install(&back, Choices { desktop: true, start_menu: false, move_profile: false }, 3).unwrap();
        assert_eq!(report.copied, None, "a newer program was replaced by an older one");
        assert_eq!(copy::sha256_file(&report.program).unwrap(), kept);
        assert!(matches!(report.desktop, Step::Made(_)), "the shortcut it was asked for is still made");
    }

    /// **A portable player installs and stays the same player**; and where the
    /// installed copy has a player already, neither profile is touched.
    #[test]
    fn a_portable_players_profile_goes_with_the_program() {
        let dir = scratch("profile");
        let source = dir.join("stick").join("p2p-poker.exe");
        program(&source, 3);
        let here = dir.join("stick").join("profile");
        fs::create_dir_all(&here).unwrap();
        fs::write(here.join("identity.key"), [5u8; 32]).unwrap();
        let root = dir.join("root");

        let offer = offer_in(&root, &source);
        assert!(offer.profile_can_move && !offer.two_players);
        let report = install(&offer, Choices { desktop: true, start_menu: false, move_profile: true }, 9).unwrap();
        assert_eq!(report.profile, ProfileStep::Moved(Moved::Renamed));
        let there = root.join("Programs").join("P2Poker").join("profile");
        assert_eq!(fs::read(there.join("identity.key")).unwrap(), [5u8; 32]);
        assert!(!here.exists(), "one player, in one place");

        // A second portable copy, with another player: two players, no move.
        let other = dir.join("other").join("p2p-poker.exe");
        program(&other, 3);
        fs::create_dir_all(other.parent().unwrap().join("profile")).unwrap();
        fs::write(other.parent().unwrap().join("profile").join("identity.key"), [6u8; 32]).unwrap();
        let offer = offer_in(&root, &other);
        assert!(offer.two_players && !offer.profile_can_move);
        let report = install(&offer, Choices { desktop: false, start_menu: false, move_profile: true }, 10).unwrap();
        assert_eq!(report.profile, ProfileStep::NotAsked, "ticked or not: never over another player");
        assert_eq!(fs::read(there.join("identity.key")).unwrap(), [5u8; 32]);
        assert_eq!(fs::read(other.parent().unwrap().join("profile").join("identity.key")).unwrap(), [6u8; 32]);
    }

    /// The real answer of this machine, read and not written: three folders,
    /// all absolute, the desktop and the Start menu existing. This is the one
    /// test that touches the user's own places, and it only looks.
    #[test]
    fn this_machine_says_where_its_folders_are() {
        if std::env::var_os(ROOT_OVERRIDE).is_some() {
            return;
        }
        let p = places().expect("the system names this user's folders");
        assert!(p.programs.is_absolute(), "{}", p.programs.display());
        assert!(p.install_dir().ends_with("P2Poker"));
        let desktop = p.desktop.expect("a desktop");
        assert!(desktop.is_absolute() && desktop.is_dir(), "{}", desktop.display());
        let menu = p.start_menu.expect("a Start menu");
        assert!(menu.is_absolute() && menu.is_dir(), "{}", menu.display());
    }
}
