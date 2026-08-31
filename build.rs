//! Build-time resources.
//!
//! Windows takes an application's icon from a resource compiled **into** the
//! executable, not from a file beside it — which matters here more than usual:
//! this client is one portable binary, and an icon that lived in a second file
//! would be an icon that vanished the moment somebody copied the thing.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");

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

        let sources = sources(&tox.join("CMakeLists.txt"));
        assert!(
            sources.len() > 40,
            "read {} sources from CMakeLists.txt, which is too few to be the whole library",
            sources.len()
        );

        let mut cc = cc::Build::new();
        cc.include(tox.join("toxcore"))
            .include(&tox)
            .include(sodium.join("src/libsodium/include"))
            .warnings(false);

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
            cc.include(root.join("tools/msvc-shim"));
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
            cc.file(tox.join(s));
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
        let text = std::fs::read_to_string(cmakelists).expect("the vendored CMakeLists.txt");
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
