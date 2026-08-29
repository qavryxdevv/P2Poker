//! The cryptographic deck. See `docs/CRYPTOGRAPHY.md`.
//!
//! Barnett-Smart with a Bayer-Groth shuffle argument, behind the `DeckCrypto`
//! trait so the implementation can be replaced. That indirection is the
//! mitigation for an unaudited backing crate, not speculative generality.

pub mod deck;
pub mod proofs;
pub mod backend;
pub mod protocol;
pub mod reveal;
pub mod shuffle;
