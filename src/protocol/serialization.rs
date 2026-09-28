//! Canonical bytes.
//!
//! `SPEC_CS.md` section 12 requires a genuinely deterministic serialisation and
//! forbids signing non-canonical JSON. Everything that is hashed into the
//! transcript or covered by a signature goes through this module, so there is
//! one definition of "the bytes" and no caller can invent a second.
//!
//! # Why arrays and never maps
//!
//! Determinism here is **structural** rather than enforced after the fact. A
//! CBOR map has to be canonicalised by sorting its keys, and every encoder that
//! forgets is a silent fork; a CBOR array has one order by construction. So
//! every type on the wire derives `#[cbor(array)]` — which `minicbor` uses by
//! default — and field numbering carries the compatibility story instead.
//!
//! # The canonicality gate
//!
//! Decoding is not enough. `SPEC_CS.md` section 17 assumes a peer that emits
//! arbitrary bytes, and CBOR admits several encodings of the same value —
//! indefinite-length items, non-minimal integers, trailing data after a
//! complete item. A signature covers *bytes*, so two encodings of one value are
//! two different signed statements, and a verifier that accepts both can be
//! shown two versions of the same event.
//!
//! [`from_canonical`] therefore decodes, then **re-encodes and compares**. Our
//! encoder emits exactly one form, so anything that does not round-trip
//! byte-identically was not canonical and is rejected before it can reach the
//! engine.

use minicbor::{Decode, Encode};

use crate::poker::state::Hash;

/// Why some bytes were not accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The bytes are not valid CBOR, or not the shape the type expects.
    Malformed(String),
    /// A complete item was decoded but bytes remained after it.
    ///
    /// A real condition from the network, not a bug: an attacker can append
    /// anything, and accepting it would let two distinct byte strings carry one
    /// event.
    TrailingBytes { decoded: usize, total: usize },
    /// The value decoded, but its encoding was not the canonical one.
    NotCanonical { expected: Vec<u8>, got: Vec<u8> },
    /// The message is larger than its cap allows (`SPEC_CS.md` sections 17, 27).
    TooLarge { limit: usize, got: usize },
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Malformed(why) => write!(f, "malformed CBOR: {why}"),
            Error::TrailingBytes { decoded, total } => {
                write!(f, "{} trailing byte(s) after a complete item", total - decoded)
            }
            Error::NotCanonical { expected, got } => write!(
                f,
                "non-canonical encoding: {} bytes offered, {} bytes canonical",
                got.len(),
                expected.len()
            ),
            Error::TooLarge { limit, got } => {
                write!(f, "{got} bytes exceeds the {limit}-byte limit")
            }
        }
    }
}

impl std::error::Error for Error {}

/// Encode a value to its one canonical byte string.
pub fn to_canonical<T>(value: &T) -> Result<Vec<u8>, Error>
where
    T: Encode<()>,
{
    minicbor::to_vec(value).map_err(|e| Error::Malformed(e.to_string()))
}

/// Decode bytes that arrived from outside, rejecting anything non-canonical.
///
/// Three checks, in order, each of which an attacker can otherwise exploit:
/// the length cap, then a clean decode with nothing trailing, then the
/// re-encode comparison.
pub fn from_canonical<'b, T>(bytes: &'b [u8], limit: usize) -> Result<T, Error>
where
    T: Decode<'b, ()> + Encode<()>,
{
    if bytes.len() > limit {
        return Err(Error::TooLarge { limit, got: bytes.len() });
    }

    let mut decoder = minicbor::Decoder::new(bytes);
    let value: T = decoder
        .decode()
        .map_err(|e| Error::Malformed(e.to_string()))?;

    let decoded = decoder.position();
    if decoded != bytes.len() {
        return Err(Error::TrailingBytes { decoded, total: bytes.len() });
    }

    let canonical = to_canonical(&value)?;
    if canonical != bytes {
        return Err(Error::NotCanonical { expected: canonical, got: bytes.to_vec() });
    }

    Ok(value)
}

/// The one hash constructor in the protocol (`PROTOCOL.md` §2.8).
///
/// Every protocol hash is domain-separated **and length-prefixed**.
/// Concatenating variable-length fields without a length prefix is ambiguous —
/// `"AB" ‖ "C"` and `"A" ‖ "BC"` are the same byte string — so two different
/// logical events could collide to one transcript hash, which breaks the chain
/// of `SPEC_CS.md` sections 13 and 14.
///
/// The domain is bound through BLAKE3's `derive_key`, whose output keys the
/// hasher. Taking the context as key material rather than as a prefix means a
/// caller cannot forget to write it.
///
/// `domain` must come from `PROTOCOL.md` §2.8's register. A construction that
/// invents its own string is a bug, and the register is closed for exactly that
/// reason — see [`crate::protocol::signatures::Domain`].
pub fn h(domain: &'static str, parts: &[&[u8]]) -> Hash {
    let key = blake3::derive_key(domain, b"p2p-poker/v1");
    let mut hasher = blake3::Hasher::new_keyed(&key);
    for part in parts {
        hasher.update(&(part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use minicbor::{Decode, Encode};

    /// Arrays, never maps: the field order is the encoding order.
    #[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
    #[cbor(array)]
    struct Event {
        #[n(0)]
        table_id: [u8; 4],
        #[n(1)]
        hand_id: u32,
        #[n(2)]
        sequence: u64,
        #[n(3)]
        payload: Vec<u8>,
    }

    fn sample() -> Event {
        Event {
            table_id: [1, 2, 3, 4],
            hand_id: 7,
            sequence: 1234,
            payload: vec![9, 9, 9],
        }
    }

    const CAP: usize = 4096;

    #[test]
    fn a_value_round_trips_through_its_canonical_bytes() {
        let bytes = to_canonical(&sample()).unwrap();
        let back: Event = from_canonical(&bytes, CAP).unwrap();
        assert_eq!(back, sample());
    }

    #[test]
    fn encoding_is_stable_across_calls() {
        // Determinism is structural here, but a regression in the encoder would
        // silently fork the chain, so it is asserted rather than assumed.
        let a = to_canonical(&sample()).unwrap();
        let b = to_canonical(&sample()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = to_canonical(&sample()).unwrap();
        let clean = bytes.len();
        bytes.extend_from_slice(&[0xff, 0x00]);
        assert_eq!(
            from_canonical::<Event>(&bytes, CAP),
            Err(Error::TrailingBytes { decoded: clean, total: clean + 2 })
        );
    }

    /// An indefinite-length array decodes to the same value but is a different
    /// byte string, so it would be a second signature over one event.
    #[test]
    fn an_indefinite_length_encoding_is_rejected() {
        let canonical = to_canonical(&sample()).unwrap();

        // 0x9f = indefinite-length array, 0xff = break. The four elements in
        // between are copied from the canonical form, which starts with 0x84.
        assert_eq!(canonical[0], 0x84, "four-element definite array");
        let mut indefinite = vec![0x9f];
        indefinite.extend_from_slice(&canonical[1..]);
        indefinite.push(0xff);

        let decoded: Result<Event, _> = from_canonical(&indefinite, CAP);
        assert!(
            matches!(decoded, Err(Error::NotCanonical { .. })),
            "expected a canonicality rejection, got {decoded:?}"
        );
    }

    #[test]
    fn a_non_minimal_integer_is_rejected() {
        let canonical = to_canonical(&sample()).unwrap();
        // hand_id 7 encodes as the single byte 0x07. Re-encode it the long way,
        // as 0x18 0x07, which is legal CBOR but not the canonical form.
        let at = canonical
            .iter()
            .position(|&b| b == 0x07)
            .expect("hand_id 7 is in there as a one-byte value");
        let mut padded = canonical.clone();
        padded.splice(at..at + 1, [0x18, 0x07]);

        let decoded: Result<Event, _> = from_canonical(&padded, CAP);
        assert!(
            matches!(decoded, Err(Error::NotCanonical { .. })),
            "expected a canonicality rejection, got {decoded:?}"
        );
    }

    #[test]
    fn oversized_input_is_rejected_before_it_is_parsed() {
        let bytes = to_canonical(&sample()).unwrap();
        let limit = bytes.len() - 1;
        assert_eq!(
            from_canonical::<Event>(&bytes, limit),
            Err(Error::TooLarge { limit, got: bytes.len() })
        );
    }

    #[test]
    fn garbage_does_not_panic() {
        // Section 17 assumes arbitrary bytes from a modified peer.
        for bytes in [
            vec![],
            vec![0xff],
            vec![0x9f; 64],
            vec![0x84, 0x84, 0x84, 0x84],
            (0u8..=255).collect::<Vec<u8>>(),
        ] {
            let _: Result<Event, _> = from_canonical(&bytes, CAP);
        }
    }

    const D1: &str = "p2p-poker v1 state";
    const D2: &str = "p2p-poker v1 stage";

    #[test]
    fn domain_separation_changes_the_hash() {
        let parts: &[&[u8]] = &[b"the same input"];
        assert_ne!(h(D1, parts), h(D2, parts), "a value must not carry across domains");
        assert_eq!(h(D1, parts), h(D1, parts), "and it is stable");
    }

    /// The property PROTOCOL.md section 2.8 names explicitly, and the reason
    /// the length prefix is there at all: without it, "AB" then "C" and "A"
    /// then "BC" are the same byte string, so two different logical events
    /// could collide to one transcript hash.
    #[test]
    fn length_prefixing_stops_concatenation_collisions() {
        let split_one: &[&[u8]] = &[b"AB", b"C"];
        let split_two: &[&[u8]] = &[b"A", b"BC"];
        assert_ne!(h(D1, split_one), h(D1, split_two));

        // And the empty part is not free either.
        let with_empty: &[&[u8]] = &[b"A", b"", b"BC"];
        assert_ne!(h(D1, split_two), h(D1, with_empty));
    }

    #[test]
    fn the_part_count_matters_not_just_the_bytes() {
        let one: &[&[u8]] = &[b"abc"];
        let three: &[&[u8]] = &[b"a", b"b", b"c"];
        assert_ne!(h(D1, one), h(D1, three));
    }

    #[test]
    fn no_parts_is_still_a_well_defined_hash() {
        let none: &[&[u8]] = &[];
        let empty_one: &[&[u8]] = &[b""];
        assert_ne!(h(D1, none), h(D1, empty_one));
        assert_eq!(h(D1, none), h(D1, none));
    }
}
