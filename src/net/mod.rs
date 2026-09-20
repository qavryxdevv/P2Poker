//! Transport only. This module must never decide a poker rule and never create
//! a card (`docs/SPEC_CS.md` section 1). See `docs/NETWORK_STACK.md`.

pub mod advert;
pub mod chained;
pub mod formation;
pub mod joinrpc;
pub mod joinwire;
pub mod lobby;
pub mod lobbytalk;
/// `S1-IV`: what a lookup of a provider key found, said once, when it ends.
pub mod lookups;
pub mod matchmaker;
pub mod tabletalk;
pub mod node;
pub mod peerbook;
pub mod plaintext;
pub mod portmap;
/// `S1-IS`: when this client says its subscriptions again, and of whom it asks what.
pub mod reannounce;
pub mod relay;
pub mod shard;
pub mod run;
pub mod snapshot;
pub mod streams;
pub mod swarm;
/// Where a table's game traffic goes when it rides Tox (D-019), and a stub of
/// the same shape when this build has no Tox. The cfg lives there so that
/// `run.rs` reads identically in both.
pub mod toxsink;
