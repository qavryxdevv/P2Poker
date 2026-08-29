//! The versioned wire protocol. See `docs/PROTOCOL.md`.

pub mod antireplay;
pub mod checkpoint;
pub mod constants;
pub mod messages;
pub mod seats;
pub mod serialization;
pub mod signatures;
pub mod slot;
pub mod staleness;
pub mod state_view;
pub mod transcript;
