//! Decentralised No-Limit Texas Hold'em.
//!
//! The layering is the one `docs/SPEC_CS.md` section 1 mandates, and the rule
//! that governs the whole crate is stated there: **the network layer never
//! decides a poker rule and never creates a card.** Each module names the spec
//! sections it implements, so a reviewer can go from a requirement to the code.
//!
//! ```text
//! gui  --> app --> protocol --> net
//!              |-> poker          deterministic rules, no crypto, no clock
//!              |-> mental_poker   the deck, and nothing about betting
//!              `-> security       keys, randomness, validation, limits
//! ```

pub mod app;
pub mod gui;
pub mod mental_poker;
pub mod net;
pub mod poker;
pub mod protocol;
pub mod security;
pub mod storage;
