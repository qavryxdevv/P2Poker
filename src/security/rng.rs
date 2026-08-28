//! The only source of cryptographic randomness in this crate.
//!
//! `SPEC_CS.md` section 7 forbids drawing cryptographic randomness from a
//! non-cryptographic generator, and names `SmallRng` and self-seeded
//! generators specifically.
//!
//! # What we cannot promise, and what we can
//!
//! An earlier draft of the research claimed `SmallRng` "is not compiled in".
//! That is false, and it was verified false rather than argued about:
//! `rand 0.9.5` lists `small_rng` among its **default** features, and four
//! dependencies take `rand` with defaults —
//!
//! ```text
//! rand v0.9.5
//! ├── hickory-proto v0.25.2  ── hickory-resolver ── libp2p-dns ── libp2p
//! ├── hickory-resolver v0.25.2
//! ├── igd-next v0.16.2       ── libp2p-upnp ── libp2p
//! └── yamux v0.13.10         ── libp2p-yamux
//! ```
//!
//! so `SmallRng` is in the binary and there is nothing we can do about it short
//! of dropping libp2p features we need. Three `rand` majors are in the tree
//! altogether (`docs/research/INTEGRATION.md` section 2).
//!
//! An absence we cannot guarantee is worthless as a security property. What we
//! can guarantee is a **discipline**: our own code draws randomness only from
//! [`SysRng`], the operating system's CSPRNG. That is enforceable, and
//! [`tests::our_own_code_uses_no_generator_but_the_os_one`] enforces it by
//! scanning this crate's source on every test run — so the rule fails the build
//! rather than living in a document nobody re-reads.

pub use getrandom::SysRng;

use rand_core::TryRng;

/// Fill `dest` with bytes from the operating system CSPRNG.
///
/// Returns an error rather than panicking: on a system where the OS source is
/// unavailable, generating a key from a fallback would be far worse than
/// refusing to generate one.
pub fn fill(dest: &mut [u8]) -> Result<(), getrandom::Error> {
    SysRng.try_fill_bytes(dest)
}

/// A fresh 32-byte secret from the operating system CSPRNG.
pub fn secret_32() -> Result<[u8; 32], getrandom::Error> {
    let mut out = [0u8; 32];
    fill(&mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn the_os_source_produces_different_bytes_each_time() {
        let a = secret_32().expect("the OS CSPRNG must be available");
        let b = secret_32().expect("the OS CSPRNG must be available");
        assert_ne!(a, b, "two draws must not coincide");
        assert_ne!(a, [0u8; 32], "all-zero is not a plausible draw");
    }

    #[test]
    fn fill_writes_every_requested_byte() {
        // A short buffer is the case a partial write would hide in.
        for len in [1usize, 7, 32, 64, 129] {
            let mut buf = vec![0u8; len];
            fill(&mut buf).expect("the OS CSPRNG must be available");
            // Not a randomness test - just that something was written at all.
            assert!(
                buf.iter().any(|&b| b != 0),
                "a {len}-byte buffer came back all zero"
            );
        }
    }

    fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("src/ must be readable") {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                rust_sources(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    /// The enforcement of `SPEC_CS.md` section 7.
    ///
    /// `SmallRng` and `StdRng` are compiled into the binary through libp2p's
    /// dependencies, so their absence cannot be asserted. What is asserted here
    /// is that **our** code never reaches for them, nor for any self-seeded
    /// construction. If this test fails, the fix is to use [`fill`], not to
    /// widen the list.
    ///
    /// This file is the one exception: it names the forbidden identifiers in
    /// order to forbid them.
    #[test]
    fn our_own_code_uses_no_generator_but_the_os_one() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_sources(&src, &mut files);
        assert!(files.len() > 10, "the scan found suspiciously few sources");

        // Split so that this list does not itself match a naive grep of the
        // tree, and so the deny-list is legible.
        let forbidden = [
            concat!("Small", "Rng"),
            concat!("Std", "Rng"),
            concat!("thread_", "rng"),
            concat!("from_", "seed"),
            concat!("seed_from_", "u64"),
            concat!("rand::", "rngs"),
        ];

        let this_file = src.join("security").join("rng.rs");
        let mut offences = Vec::new();

        for file in files {
            if file == this_file {
                continue;
            }
            let text = fs::read_to_string(&file).expect("a readable source file");
            for (lineno, line) in text.lines().enumerate() {
                for needle in forbidden {
                    if line.contains(needle) {
                        offences.push(format!(
                            "{}:{}: {}",
                            file.strip_prefix(&src).unwrap_or(&file).display(),
                            lineno + 1,
                            needle
                        ));
                    }
                }
            }
        }

        assert!(
            offences.is_empty(),
            "SPEC_CS.md section 7 forbids these generators for cryptographic use; \
             draw from security::rng::fill instead:\n  {}",
            offences.join("\n  ")
        );
    }
}
