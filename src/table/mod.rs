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
/// `STATE_HASH` and `STATE_ACK` on the wire, and the sequence band they occupy
/// (`PROTOCOL.md` §4.9).
pub mod checkwire;
/// Carrying a message over a transport whose packets are smaller than it
/// (D-019: every Tox channel caps at about 1372 bytes).
pub mod fragment;
pub mod join;
