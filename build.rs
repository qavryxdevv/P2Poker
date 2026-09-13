//! Build-time resources.
//!
//! Windows takes an application's icon from a resource compiled **into** the
//! executable, not from a file beside it — which matters here more than usual:
//! this client is one portable binary, and an icon that lived in a second file
//! would be an icon that vanished the moment somebody copied the thing.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");

    refuse_a_release_that_names_this_machine();

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("FileDescription", "Decentralised peer-to-peer poker");
        res.set("ProductName", "p2p-poker");
        res.set("LegalCopyright", "");
        // A failure here is not worth failing the build over: the client runs
        // perfectly well with the default icon, and refusing to compile because
        // of a picture would be the wrong trade.
        if let Err(e) = res.compile() {
            println!("cargo:warning=the icon was not embedded: {e}");
        }
    }

    #[cfg(feature = "tox")]
    tox::build();
}

/// `S1-EN`: a release names nothing of the machine it was built on.
///
/// rustc writes the source path of every panic location and trace point into
/// the binary, and a dependency's path is absolute: the cargo home, under the
/// builder's profile, with the user name in it. Some three thousand of them were
/// in the owner's release, and two build scripts' generated bindings named the
/// repository's own folders. Cargo's `trim-paths` would say it once in
/// `Cargo.toml` and is not stable (cargo 1.95); rustc's `--remap-path-prefix`
/// is, but it has to name the paths it hides, so it cannot be committed.
/// `tools/remap-build-paths.ps1` writes it into this machine's cargo home, and
/// this refuses a release build that would still name the cargo home or the
/// target directory, so a machine without it cannot ship one by accident.
/// `tools/check-build-paths.ps1` reads the built binary for what this cannot
/// see.
fn refuse_a_release_that_names_this_machine() {
    println!("cargo:rerun-if-env-changed=CARGO_HOME");
    if std::env::var("PROFILE").as_deref() != Ok("release") {
        return;
    }
    let flags = std::env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    let mut hidden: Vec<std::path::PathBuf> = Vec::new();
    let mut args = flags.split('\u{1f}');
    while let Some(arg) = args.next() {
        let map = if arg == "--remap-path-prefix" {
            args.next()
        } else {
            arg.strip_prefix("--remap-path-prefix=")
        };
        // rustc: the FROM may itself contain `=`, the TO may not.
        if let Some((from, _)) = map.and_then(|m| m.rsplit_once('=')) {
            hidden.push(std::path::PathBuf::from(from));
        }
    }
    let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os(home).map(|h| std::path::PathBuf::from(h).join(".cargo")));
    let out_dir = std::env::var_os("OUT_DIR").map(std::path::PathBuf::from);
    let named: Vec<String> = [cargo_home, out_dir]
        .into_iter()
        .flatten()
        .filter(|path| !hidden.iter().any(|h| path.starts_with(h)))
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        named.is_empty(),
        "a release built here would carry this machine's paths in the binary ({}): rustc writes every \
         dependency's source path into panic locations and trace points. Run tools/remap-build-paths.ps1 \
         once -- it puts --remap-path-prefix for this machine into the cargo home's config.toml -- or add \
         those flags there yourself, and build again (S1-EN)",
        named.join(", ")
    );
}

/// Compiling the vendored `c-toxcore` (D-019).
///
/// # Why the C sources are compiled here and not by CMake
///
/// `c-toxcore` has a CMake build and on MSVC it cannot be used without two
/// things this machine does not have and this project does not want to
/// require: `find_package(PkgConfig REQUIRED)` fails outright when `pkg-config`
/// is absent, and its MSVC branch demands either a `libsodium` CMake package
/// config or vcpkg's `unofficial-sodium`. Satisfying both means shipping a
/// stub `pkg-config` and a hand-written package file — two pieces of build
/// machinery that exist only to talk another build system into doing what
/// `cc` does directly.
///
/// The file list is `CMakeLists.txt`'s `toxcore_SOURCES`, read from the
/// vendored tree rather than copied into this file, so a version bump cannot
/// leave the two out of step.
///
/// # Why it is behind a feature
///
/// `c-toxcore` is **GPL-3.0** and linking it makes the whole client GPL-3.0
/// (D-019, "The price"). That is a decision the owner has taken, but it should
/// be visible in the build rather than implicit: `--features tox` is the line
/// where the licence changes.
#[cfg(feature = "tox")]
mod tox {
    use std::path::{Path, PathBuf};

    pub fn build() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let tox = root.join("vendor/c-toxcore");
        let sodium = root.join("vendor/libsodium");

        println!("cargo:rerun-if-changed={}", tox.join("CMakeLists.txt").display());

        // **And the sources themselves, because they are ours to edit now.**
        //
        // While the tree was fetched and patched at build time this was
        // unnecessary: a change meant a re-fetch, which recreated the whole
        // directory. The tree is committed and patched in place now, so without
        // these lines an edit to `group_chats.c` produces `Finished in 0.57s`
        // and a binary containing the *previous* library — measured, on the
        // first edit after the trees were committed. That is the same shape as
        // the defect that cost this project a day: something that looks built,
        // links, runs, and is missing the change.
        //
        // **Name every file. A directory is not enough, and the belief that it
        // was cost four days of stale library.**
        //
        // This was two `rerun-if-changed` lines naming the two directories,
        // with a comment saying cargo walks a directory so the files need not
        // be listed. On this platform it does not: editing a file *inside*
        // `toxcore/` leaves the directory's own mtime alone, nothing looks
        // changed, and the C is not recompiled. Measured on 2026-09-04, adding
        // two entry points to `tox.c` and `group_connection.c`: `cargo build`
        // reported `Finished`, and `libtoxcore.a` in the target directory was
        // still the one built on 2026-08-31, without either symbol in it. The
        // comment those lines carried is the one directly above, warning about
        // exactly this failure — it was written for the previous instance of it
        // and the fix it describes did not hold.
        //
        // So: one line per file, which is what cargo is documented to honour
        // without qualification. A few hundred lines of build output is a small
        // price for a library that is actually the source next to it.
        for dir in [tox.join("toxcore"), tox.join("third_party/cmp")] {
            let mut stack = vec![dir];
            while let Some(d) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&d) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else {
                        println!("cargo:rerun-if-changed={}", path.display());
                    }
                }
            }
        }

        let sources = sources(&tox.join("CMakeLists.txt"));
        assert!(
            sources.len() > 40,
            "read {} sources from CMakeLists.txt, which is too few to be the whole library",
            sources.len()
        );

        // `S1-EN`: every path the compiler is given is relative to this
        // package's directory, which is the build script's working directory.
        // MSVC's `__FILE__` is the path as it was given, and toxcore's logger
        // passes `__FILE__` into the binary: given absolute paths, every source
        // file's name carried the builder's home and folder layout into the
        // release.
        let tox_here = Path::new("vendor/c-toxcore");
        let sodium_here = Path::new("vendor/libsodium");
        assert!(
            tox_here.join("CMakeLists.txt").is_file(),
            "the build script is not running in the package directory, so the C sources cannot be named relative to it"
        );

        let mut cc = cc::Build::new();
        cc.include(tox_here.join("toxcore"))
            .include(tox_here)
            .include(sodium_here.join("src/libsodium/include"))
            .warnings(false);

        // **The library's own diagnostics, at DEBUG, in the harness build only.**
        //
        // `logger.h` defaults `MIN_LOGGER_LEVEL` to `LOGGER_LEVEL_INFO`, and
        // `LOGGER_WRITE` compiles a call out entirely when its level is below
        // that — so every `LOGGER_DEBUG` in the group code is gone before a
        // callback could ever see it. Measured: with
        // `tox_options_set_log_callback` registered, a four-seat run in which a
        // seat's join was deliberately starved produced **zero** lines from any
        // node in ninety seconds.
        //
        // That silence was itself worth having — it excludes the four invite
        // branches that log at `WARNING` and `ERROR`, leaving the two that
        // return with no diagnostic at all, of which the unconfirmed-peer reap
        // is one. Lowering the threshold is what turns "one of two" into
        // "this one". `S1-AA` shape (i).
        //
        // **Only under `fault-harness`.** At `DEBUG` the whole library is loud —
        // thousands of lines about the DHT — and a release build has no use for
        // any of it. `LOGGER_LEVEL_DEBUG` is 1 in `logger.h`'s enum.
        if std::env::var("CARGO_FEATURE_FAULT_HARNESS").is_ok() {
            cc.define("MIN_LOGGER_LEVEL", "1");
            // **And the fault instruments in the C, on the same switch**
            // (`patch 0016`). `S1-BO`'s fix fires only on an asymmetric peer
            // timeout, and nothing above this layer can produce one: the
            // application's own outage knob leaves the transport alive on
            // purpose, so toxcore's pings keep the peer timer fed. Every line
            // it enables is inside `#ifdef P2P_POKER_FAULT_HARNESS`, so a
            // release build has no path to any of it, which is the same
            // property the Rust side's `fault-harness` feature has.
            cc.define("P2P_POKER_FAULT_HARNESS", None);
        }

        if cfg!(target_env = "msvc") {
            // MSVC has no `/std:c99`; c11 is the nearest it offers and is a
            // superset for everything toxcore uses. Passing c99 produced
            // `D9002: unknown option` on every one of sixty files.
            cc.std("c11");
            // **The pthread shim goes first.** toxcore's Windows build normally
            // pulls in PThreads4W - a third library, with its own build, for
            // fifteen calls Windows has native primitives for. `tools/msvc-shim`
            // supplies exactly those fifteen; see the header for what it does
            // not supply and why that is a compile error rather than a stub.
            cc.include("tools/msvc-shim");
            // Linking libsodium as a static archive rather than through its
            // import library, which is what the vendored build produces.
            cc.define("SODIUM_STATIC", "1");
            // `getaddrinfo` and friends, and the winsock headers toxcore
            // expects to be able to include.
            cc.define("_WIN32_WINNT", "0x0601");
            cc.define("WIN32_LEAN_AND_MEAN", None);
            // MSVC's C mode is strict about these and toxcore uses them.
            cc.define("_CRT_SECURE_NO_WARNINGS", None);
        }

        for s in &sources {
            cc.file(tox_here.join(s));
        }
        cc.compile("toxcore");

        // libsodium, built from the vendored source by `tools/build-tox.ps1`.
        // Not built here: it has its own MSVC solution and driving MSBuild from
        // a build script would be a second build system inside this one, run on
        // every `cargo check`.
        let lib = sodium.join("bin/x64/Release/v143/static");
        assert!(
            lib.join("libsodium.lib").exists() || !cfg!(target_env = "msvc"),
            "libsodium.lib is missing - run tools/build-tox.ps1 once before building with --features tox"
        );
        println!("cargo:rustc-link-search=native={}", lib.display());
        println!("cargo:rustc-link-lib=static=libsodium");

        if cfg!(windows) {
            for l in ["ws2_32", "iphlpapi", "advapi32", "bcrypt"] {
                println!("cargo:rustc-link-lib=dylib={l}");
            }
        }
    }

    /// The `.c` files of `toxcore_SOURCES`, from the vendored `CMakeLists.txt`.
    ///
    /// Parsed rather than transcribed. A list copied into this file is a list
    /// that silently stops matching the day the vendored tree moves, and the
    /// symptom would be a linker error a long way from the cause.
    ///
    /// **The first block only.** `CMakeLists.txt` appends to the same variable
    /// under `if(BUILD_TOXAV)`, and taking that too pulls in `toxav/`, which
    /// wants `opus.h` and `vpx`. This client carries no audio and no video -
    /// D-019 puts *only game data* on Tox - so those are not dependencies to
    /// acquire, they are files not to compile. A second `set(` on the variable
    /// ends the read, and the `toxav/` assertion below is the belt: if the
    /// upstream ever moves an A/V file into the first block, the build says so
    /// instead of failing at a missing header sixty files later.
    fn sources(cmakelists: &Path) -> Vec<String> {
        let text = std::fs::read_to_string(cmakelists).unwrap_or_else(|_| {
            panic!(
                "{} is not there.\n\n\
                 The `tox` feature is on by default because the released binary always \n\
                 carries Tox (D-019). It needs the vendored C, which is fetched at pinned \n\
                 commits rather than committed:\n\n    \
                 pwsh tools/build-tox.ps1\n\n\
                 To build without it - no C toolchain, or a test run with no business \n\
                 opening a socket:\n\n    \
                 cargo build --no-default-features\n",
                cmakelists.display()
            )
        });
        let mut out = Vec::new();
        let mut inside = false;
        let mut seen_block = false;
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with("set(toxcore_SOURCES") {
                if seen_block {
                    break;
                }
                seen_block = true;
                inside = true;
                continue;
            }
            if inside {
                if t.starts_with(')') || t.ends_with(')') && !t.ends_with(".c)") {
                    // The list ends at the closing paren; a trailing entry can
                    // carry it, which is why the last one is taken first.
                    if let Some(name) = t.strip_suffix(')') {
                        if name.ends_with(".c") {
                            out.push(name.to_string());
                        }
                    }
                    inside = false;
                    continue;
                }
                if t.ends_with(".c") {
                    out.push(t.to_string());
                }
            }
        }
        assert!(
            !out.iter().any(|s| s.starts_with("toxav/")),
            "an A/V source reached the core list; this client compiles no toxav"
        );
        out
    }
}
