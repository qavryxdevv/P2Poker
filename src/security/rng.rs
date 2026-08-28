//! The only source of cryptographic randomness in this crate:
//! `getrandom::SysRng`.
//!
//! Section 7 forbids `SmallRng`, `StdRng` and any self-seeded generator. The
//! `rand` crate *is* in the dependency tree - libp2p and arkworks pull it in -
//! so this module exists to make the boundary explicit and reviewable
//! (`docs/research/INTEGRATION.md` section 2).
