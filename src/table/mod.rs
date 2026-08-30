//! The table: where the deck, the protocol and the poker engine meet.
//!
//! Nothing here is cryptography and nothing here is poker rules. It is the seam,
//! and it exists as its own module so that the seam has a name — an integration
//! defect between two well-tested halves has nowhere else to live.

pub mod dealing;
pub mod hand;
pub mod handwire;
pub mod stage;
pub mod transport;
pub mod formation;
pub mod join;
