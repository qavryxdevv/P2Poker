//! Actions and the legal-action computation (`STATE_MACHINE.md` section 6).
//!
//! Every incoming action is re-validated locally; a peer is never trusted to
//! send a legal one.
