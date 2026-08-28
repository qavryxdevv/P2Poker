//! Canonical bytes. Deterministic CBOR via `minicbor`, arrays only and never
//! maps, so determinism is structural rather than sorted after the fact.
//!
//! Section 12 forbids signing non-canonical JSON. Every decode is followed by a
//! re-encode-and-compare gate before the bytes are trusted.
