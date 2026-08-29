//! Deterministic No-Limit Texas Hold'em. See `docs/STATE_MACHINE.md`.
//!
//! No cryptography, no clock, no I/O and no randomness. `apply(state, event)`
//! returns the next state, and the same inputs produce byte-identical state on
//! every client. Time enters only as a signed event (D-006).

pub mod actions;
pub mod engine;
pub mod evaluator;
pub mod pots;
pub mod seating;
pub mod state;
pub mod tournament;
