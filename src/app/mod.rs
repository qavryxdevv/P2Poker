//! Wiring between the GUI, the protocol and the engine.
//!
//! Owns the worker thread that parses, verifies, proves and applies, so that
//! `docs/SPEC_CS.md` section 33 holds: cryptography never blocks the GUI event
//! loop.
