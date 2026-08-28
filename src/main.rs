//! Portable single-executable entry point (`docs/SPEC_CS.md` section 22).
//!
//! The profile - identity, settings, history - lives next to the binary so the
//! whole directory can be copied to another machine. Nothing is written to the
//! registry or outside this folder.

fn main() {
    println!("p2p-poker {}", env!("CARGO_PKG_VERSION"));
    println!("Phase 2: deterministic engine. See docs/STATE_MACHINE.md.");
}
