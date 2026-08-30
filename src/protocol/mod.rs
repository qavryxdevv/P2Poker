//! The versioned wire protocol. See `docs/PROTOCOL.md`.

pub mod constants;
pub mod messages;
pub mod serialization;
pub mod signatures;
pub mod state_view;
pub mod transcript;

// **Drafts, not authorities.** These are written and tested and called by
// nothing: §6.2's checkpoint records and the seat set they are built on. They
// stay because they refuse nothing — wiring them in would add a stage this
// client does not have, not overrule a check it already runs.
//
// `slot.rs`, `antireplay.rs` and `staleness.rs` were the other kind and are
// gone. Each carried a rule about which events to reject, each passed its own
// tests, and applied to this tree the key they share convicts honest peers.
// See D-025 and `tests/anti_replay_authority.rs`.
pub mod checkpoint;
pub mod seats;
