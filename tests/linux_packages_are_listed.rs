//! `S1-JA`: the Linux packages carry what a software centre lists them by.
//!
//! # What this exists for
//!
//! GNOME Software and KDE Discover list the components the system's AppStream
//! knows, not the packages dpkg or rpm installed. The `.deb` and the `.rpm` of
//! 0.1.4 carried the program, its menu entry, its icon and its licence and no
//! AppStream metainfo, so neither showed the program they had installed, and a
//! player who had installed it with a double click could remove it only from a
//! terminal. The owner found it on Ubuntu, 2026-09-25.
//!
//! That a system's AppStream lists the installed package is proved on Linux, by
//! `tools/check-linux-listing.sh`, which both workflows run once the `.deb` is
//! installed. This is the half a Windows machine can hold on every test run:
//! the joints between files that are edited apart.
//!
//! * The metainfo is named by its component's id, the id is the repository's
//!   (`io.github.<owner>.<repository>`, AppStream's form for a project hosted
//!   there), and the component is a desktop application.
//! * It is started by the menu entry the packages install, and its binary is
//!   the program they install, which that menu entry starts.
//! * `tools/package-linux.sh` installs it where AppStream reads it, in the tree
//!   both the `.deb` and the `.rpm` are made from, and the `.rpm` lists it; the
//!   listing check asks the installed system for the same id, and both
//!   workflows run it.
//! * One summary, in the metainfo, the menu entry and the packages; and a
//!   content rating that says what the privacy policy says.

use std::fs;
use std::path::PathBuf;

/// The component's id, and the metainfo's file name before `.metainfo.xml`.
const ID: &str = "io.github.qavryxdevv.P2Poker";

fn read(path: &str) -> String {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(&file).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The metainfo without its comments, which name tags they are not.
fn metainfo() -> String {
    let mut xml = read(&format!("packaging/linux/{ID}.metainfo.xml"));
    while let Some(open) = xml.find("<!--") {
        let close = xml[open..].find("-->").expect("every comment is closed") + open + 3;
        xml.replace_range(open..close, "");
    }
    xml
}

/// What stands between `open` -- which occurs once -- and the next `close`, its
/// whitespace squeezed to single spaces.
fn element(xml: &str, open: &str, close: &str) -> String {
    assert_eq!(xml.matches(open).count(), 1, "one {open} in the metainfo");
    let start = xml.find(open).unwrap() + open.len();
    let end = start + xml[start..].find(close).unwrap_or_else(|| panic!("{close} after {open}"));
    xml[start..end].split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The value of `key=` in the menu entry.
fn desktop(key: &str) -> String {
    let entry = read("packaging/linux/p2poker.desktop");
    let prefix = format!("{key}=");
    let values: Vec<&str> = entry.lines().filter_map(|l| l.strip_prefix(prefix.as_str())).collect();
    assert_eq!(values.len(), 1, "one {key}= in the menu entry");
    values[0].to_owned()
}

#[test]
fn the_metainfo_is_the_repositorys_desktop_application_named_by_its_id() {
    let xml = metainfo();
    assert!(
        xml.contains(r#"<component type="desktop-application">"#),
        "a software centre lists a desktop application among the programs it can remove"
    );
    assert_eq!(element(&xml, "<id>", "</id>"), ID, "the file is named by its component's id");

    let home = element(&xml, r#"<url type="homepage">"#, "</url>");
    let path = home.strip_prefix("https://github.com/").expect("the homepage is the repository on GitHub");
    let (owner, repository) = path.split_once('/').expect("github.com/<owner>/<repository>");
    assert_eq!(
        ID,
        format!("io.github.{owner}.{repository}"),
        "AppStream's id for a project on GitHub is io.github.<owner>.<repository>"
    );

    let script = read("tools/package-linux.sh");
    assert!(script.contains(&format!("Homepage: {home}\n")), "the .deb's Homepage is the metainfo's");
    assert!(script.contains(&format!("URL: {home}\n")), "the .rpm's URL is the metainfo's");
}

#[test]
fn it_is_started_by_the_menu_entry_the_packages_install_and_names_their_program() {
    let xml = metainfo();
    let script = read("tools/package-linux.sh");

    let entry = element(&xml, r#"<launchable type="desktop-id">"#, "</launchable>");
    assert_eq!(entry, "p2poker.desktop");
    assert!(script.contains(&format!(r#"desktop="$root/packaging/linux/{entry}""#)));
    assert!(
        script.contains(&format!(r#"install -Dm644 "$desktop" "$stage/usr/share/applications/{entry}""#)),
        "the packages install the menu entry the metainfo is started by, under its name"
    );

    let binary = element(&xml, "<binary>", "</binary>");
    assert_eq!(desktop("Exec"), binary, "the menu entry starts the program the metainfo names");
    assert!(
        script.contains(&format!(r#"install -Dm755 "$bin" "$stage/usr/bin/{binary}""#)),
        "the packages install that program under that name"
    );
}

#[test]
fn both_packages_carry_it_where_appstream_reads_it_and_both_workflows_ask_for_it() {
    let script = read("tools/package-linux.sh");
    let installed = format!("/usr/share/metainfo/{ID}.metainfo.xml");

    assert!(script.contains(&format!(r#"metainfo="$root/packaging/linux/{ID}.metainfo.xml""#)));
    assert!(
        script.contains(&format!(r#"install -Dm644 "$metainfo" "$stage{installed}""#)),
        "installed into the tree both packages are made from"
    );
    assert!(script.contains(r#"cp -a "$stage" "$deb""#), "the .deb is made from that tree");
    assert!(script.contains("cp -a $stage/. %{buildroot}/"), "and so is the .rpm");
    let files = &script[script.find("\n%files\n").expect("the .rpm's list of its files")..];
    assert!(
        files.lines().any(|l| l == installed),
        "the .rpm lists it: rpmbuild packages nothing its list does not name"
    );

    let check = read("tools/check-linux-listing.sh");
    assert!(check.contains(&format!("\nid={ID}\n")), "the listing check asks the installed system for this id");
    for workflow in [".github/workflows/release.yml", ".github/workflows/repackage-linux.yml"] {
        assert!(
            read(workflow).contains("sh tools/check-linux-listing.sh "),
            "{workflow} asks the installed system whether a software centre lists the package"
        );
    }
}

#[test]
fn one_summary_in_the_metainfo_the_menu_entry_and_the_packages() {
    let summary = element(&metainfo(), "<summary>", "</summary>");
    assert_eq!(desktop("Comment"), summary, "the menu entry's comment");
    assert!(
        read("tools/package-linux.sh").contains(&format!("summary=\"{summary}\"\n")),
        "the .deb's and the .rpm's summary"
    );
}

#[test]
fn the_content_rating_says_what_the_privacy_policy_says() {
    let xml = metainfo();
    let privacy = read("docs/PRIVACY.md");
    let rating = |id: &str| element(&xml, &format!(r#"<content_attribute id="{id}">"#), "</content_attribute>");

    // OARS 1.1: `moderate` is gambling with play money, `intense` with real money.
    assert!(privacy.contains("play money only"), "the policy still says play money only");
    assert_eq!(rating("money-gambling"), "moderate");
    // `intense` is chat between users that nobody controls.
    assert!(privacy.contains("not stored or moderated"), "the policy still says nobody moderates the chat");
    assert_eq!(rating("social-chat"), "intense");
    // `mild` is asking for the newest version (D-077), the one question the window sends GitHub.
    assert!(privacy.contains("which the newest released version is"), "the policy still names that one question");
    assert_eq!(rating("social-info"), "mild");
}
