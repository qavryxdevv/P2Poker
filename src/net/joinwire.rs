//! The formation messages, on the wire.
//!
//! `PROTOCOL.md` §4.3 specifies five payloads — `JOIN_REQUEST`, `JOIN_ACCEPT`,
//! `JOIN_REJECT`, `PLAYER_LIST`, `TABLE_READY` — and every admission rule over
//! them was written and tested in [`table::join`](crate::table::join) months
//! before anything could carry one. This is the carrying.
//!
//! It is the same shape as [`advert`](super::advert) and for the same reasons:
//! a wire struct per message, distinct from the domain type the rules read; a
//! cap on every decode; canonical re-encoding checked on the way in; and the
//! signature verified before a single field is believed.
//!
//! # Who signs what, and why it is not the same key twice
//!
//! * `JOIN_REQUEST` is signed by the **joiner's application key**, and
//!   `n(1) app_public_key` must equal the envelope's `sender_public_key`. Two
//!   copies of one identity, checked against each other, because the rules read
//!   the payload and the signature covers the envelope.
//! * `JOIN_ACCEPT`, `JOIN_REJECT` and `PLAYER_LIST` are signed by the **table
//!   key**. The table's identity is that key and nothing else: the envelope's
//!   `table_id` is the unchained sentinel `ZERO32`, so there is no second place
//!   for an identity to disagree with itself.
//! * `TABLE_READY` is signed by **each seat's own application key** and is the
//!   only chained message here: `hand_id = 0`, `sequence = 0`,
//!   `previous_event_hash = GENESIS(0)`. It is stage 0 of the setup chain, and
//!   it is where the founder's special position ends.
//!
//! # Every limit in the specification is a limit here
//!
//! `peer_id` ≤ 42 B, `display_name` ≤ 32 B, `advert_event` ≤
//! `TABLE_AD_SIGNED_MAX`, a roster ≤ `MAX_SEATS`, a capability set ≤ 32. They
//! are checked **on decode**, before the value reaches a rule, because a rule
//! that receives an over-long field has already allocated it.

use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use minicbor::{Decode, Encode};

use crate::poker::state::Hash;
use crate::protocol::constants::{
    JOIN_REQ_MAX, JOIN_RESP_MAX, MAX_SEATS, TABLE_AD_SIGNED_MAX,
};
use crate::protocol::messages::{EventBody, EventType, SignedEvent, PROTOCOL_VERSION};
use crate::protocol::serialization::{from_canonical, to_canonical};
use crate::protocol::signatures::to_be_signed;
use crate::protocol::transcript::event_hash;
use crate::table::formation::SeatEntry;
use crate::table::join::{JoinAccept, JoinRequest, PlayerList, RejectReason, TableReady};

/// The most capabilities a `TABLE_READY` may declare (§4.3).
pub const MAX_CAPABILITIES: usize = 32;
/// The most bytes one capability string may take. Not in §4.3 by name; the
/// message cap would otherwise be the only bound, and a bound that is only a
/// total is a bound one field can spend entirely.
pub const CAPABILITY_MAX: usize = 64;
/// §4.3: `peer_id` ≤ 42 B.
pub const PEER_ID_MAX: usize = 42;
/// §4.3: `display_name` ≤ 32 B of UTF-8.
pub const DISPLAY_NAME_MAX: usize = 32;

/// Why a formation message was not accepted, before any admission rule saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    /// Not canonical CBOR, or not the shape expected.
    Malformed(&'static str),
    /// A field is longer than §4.3 allows.
    TooLong(&'static str),
    /// The envelope failed §2.3's catalogue check.
    Envelope(&'static str),
    /// The signature does not verify under the key that claims to have made it.
    BadSignature,
    /// Signed by a key other than the one this message must come from.
    WrongSigner,
    /// The message is of a different type than the one being read.
    WrongType,
    /// Something did not encode. Only reachable for values this client built.
    Unencodable(&'static str),
}

// ---------------------------------------------------------------------------
// Wire bodies
// ---------------------------------------------------------------------------

/// One roster entry, as §4.3's `SeatEntry` array.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct SeatWire {
    #[n(0)]
    pub seat: u8,
    #[cbor(n(1), with = "minicbor::bytes")]
    pub app_public_key: [u8; 32],
    #[cbor(n(2), with = "minicbor::bytes")]
    pub peer_id: Vec<u8>,
    /// **Bytes, not text.** §4.3 types it as bytes, and a decoder that accepted
    /// a CBOR text string here would accept an encoding no honest sender emits.
    #[cbor(n(3), with = "minicbor::bytes")]
    pub display_name: Vec<u8>,
    #[n(4)]
    pub buyin: u64,
}

impl SeatWire {
    fn from_entry(e: &SeatEntry) -> Self {
        SeatWire {
            seat: e.seat,
            app_public_key: e.app_public_key,
            peer_id: e.peer_id.clone(),
            display_name: e.display_name.as_bytes().to_vec(),
            buyin: e.buyin,
        }
    }

    /// The domain value, with §4.3's limits enforced here rather than by the
    /// rule that reads it.
    fn into_entry(self) -> Result<SeatEntry, WireError> {
        if self.peer_id.len() > PEER_ID_MAX {
            return Err(WireError::TooLong("peer_id"));
        }
        if self.display_name.len() > DISPLAY_NAME_MAX {
            return Err(WireError::TooLong("display_name"));
        }
        let display_name = String::from_utf8(self.display_name)
            .map_err(|_| WireError::Malformed("display_name is not UTF-8"))?;
        if display_name.chars().any(|c| c.is_control()) {
            return Err(WireError::Malformed("display_name has control characters"));
        }
        Ok(SeatEntry {
            seat: self.seat,
            app_public_key: self.app_public_key,
            peer_id: self.peer_id,
            display_name,
            buyin: self.buyin,
        })
    }
}

fn roster_out(seats: &[SeatEntry]) -> Vec<SeatWire> {
    seats.iter().map(SeatWire::from_entry).collect()
}

fn roster_in(wire: Vec<SeatWire>) -> Result<Vec<SeatEntry>, WireError> {
    if wire.len() > MAX_SEATS as usize {
        return Err(WireError::TooLong("roster"));
    }
    wire.into_iter().map(SeatWire::into_entry).collect()
}

/// `0x0201 JOIN_REQUEST`, nine fields.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct JoinRequestBody {
    #[cbor(n(0), with = "minicbor::bytes")]
    pub advert_hash: [u8; 32],
    #[cbor(n(1), with = "minicbor::bytes")]
    pub app_public_key: [u8; 32],
    #[cbor(n(2), with = "minicbor::bytes")]
    pub peer_id: Vec<u8>,
    #[cbor(n(3), with = "minicbor::bytes")]
    pub display_name: Vec<u8>,
    #[n(4)]
    pub requested_seat: Option<u8>,
    #[cbor(n(5), with = "minicbor::bytes")]
    pub password_proof: Option<[u8; 32]>,
    #[n(6)]
    pub buyin: u64,
    #[cbor(n(7), with = "minicbor::bytes")]
    pub join_nonce: [u8; 32],
    #[cbor(n(8), with = "minicbor::bytes")]
    pub table_id: [u8; 32],
}

/// `0x0202 JOIN_ACCEPT`, four fields.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct JoinAcceptBody {
    #[cbor(n(0), with = "minicbor::bytes")]
    pub request_hash: [u8; 32],
    #[n(1)]
    pub seat: u8,
    #[cbor(n(2), with = "minicbor::bytes")]
    pub advert_event: Vec<u8>,
    #[n(3)]
    pub roster_so_far: Vec<SeatWire>,
}

/// `0x0203 JOIN_REJECT`, three fields.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct JoinRejectBody {
    #[cbor(n(0), with = "minicbor::bytes")]
    pub request_hash: [u8; 32],
    #[n(1)]
    pub reason: u16,
    #[n(2)]
    pub retry_after_ms: u32,
}

/// `0x0204 PLAYER_LIST`, three fields.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct PlayerListBody {
    #[n(0)]
    pub roster: Vec<SeatWire>,
    #[cbor(n(1), with = "minicbor::bytes")]
    pub table_params_hash: [u8; 32],
    #[n(2)]
    pub list_serial: u64,
}

/// `0x0205 TABLE_READY`, five fields.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct TableReadyBody {
    #[cbor(n(0), with = "minicbor::bytes")]
    pub roster_hash: [u8; 32],
    #[n(1)]
    pub list_serial: u64,
    #[cbor(n(2), with = "minicbor::bytes")]
    pub table_params_hash: [u8; 32],
    #[n(3)]
    pub my_seat: u8,
    #[n(4)]
    pub capability_set: Vec<Vec<u8>>,
}

// ---------------------------------------------------------------------------
// Sealing and opening
// ---------------------------------------------------------------------------

/// Sign an unchained formation message.
fn seal_unchained<T: Encode<()>>(
    kind: EventType,
    payload: &T,
    key: &SigningKey,
    now_ms: u64,
    cap: usize,
) -> Result<Vec<u8>, WireError> {
    let payload_bytes =
        to_canonical(payload).map_err(|_| WireError::Unencodable("the payload"))?;
    if payload_bytes.len() > cap {
        return Err(WireError::TooLong("the payload is over its cap"));
    }
    // Built by the one constructor that knows the sentinels, so a sender cannot
    // disagree with the checker about them.
    let envelope = EventBody::unchained(
        kind,
        key.verifying_key().to_bytes(),
        payload_bytes,
        now_ms,
    )
    .ok_or(WireError::Unencodable("that type is not unchained"))?;
    finish(envelope, key)
}

/// Sign `TABLE_READY`, which is stage 0 of the setup chain and therefore the one
/// message here whose envelope is not the unchained sentinel shape.
fn seal_ready(
    payload: &TableReadyBody,
    table_id: &Hash,
    genesis: &Hash,
    key: &SigningKey,
    now_ms: u64,
    next_deadline_ms: u32,
) -> Result<Vec<u8>, WireError> {
    let payload_bytes =
        to_canonical(payload).map_err(|_| WireError::Unencodable("the payload"))?;
    if payload_bytes.len() > JOIN_RESP_MAX {
        return Err(WireError::TooLong("the payload is over its cap"));
    }
    let envelope = EventBody {
        protocol_version: PROTOCOL_VERSION,
        table_id: *table_id,
        hand_id: 0,
        sequence: 0,
        sender_public_key: key.verifying_key().to_bytes(),
        event_type: EventType::TableReady.code(),
        payload: payload_bytes,
        previous_event_hash: *genesis,
        emitted_at_unix_ms: now_ms,
        next_deadline_ms,
        chain_scope: EventType::TableReady.chain_scope(),
        event_class: 0,
    };
    finish(envelope, key)
}

fn finish(envelope: EventBody, key: &SigningKey) -> Result<Vec<u8>, WireError> {
    let envelope_bytes =
        to_canonical(&envelope).map_err(|_| WireError::Unencodable("the envelope"))?;
    let signature = {
        use ed25519_dalek::Signer;
        key.sign(&to_be_signed(&envelope_bytes))
    };
    to_canonical(&SignedEvent {
        body: envelope_bytes,
        signature: signature.to_bytes(),
    })
    .map_err(|_| WireError::Unencodable("the signed event"))
}

/// What opening a formation message yields before its payload is read.
pub struct Opened {
    pub sender: [u8; 32],
    pub event_hash: Hash,
    pub envelope: EventBody,
}

/// Decode, check the envelope, verify the signature — in that order, and all
/// three before any field is believed.
///
/// The signature is checked with `verify_strict`, which additionally rejects a
/// small-order public key. That is not the whole of what it does here: the
/// measurement in [`advert`](super::advert) found that plain `verify` also
/// rejects the all-zero forgery, so the reason to prefer it is the narrower one
/// — one decoder, one rule, at every call site in this crate.
fn open(bytes: &[u8], cap: usize, expected: EventType) -> Result<Opened, WireError> {
    let signed: SignedEvent = from_canonical(bytes, cap)
        .map_err(|_| WireError::Malformed("not a canonical signed event"))?;
    let envelope: EventBody = from_canonical(&signed.body, cap)
        .map_err(|_| WireError::Malformed("not a canonical envelope"))?;

    let kind = envelope
        .check_envelope()
        .map_err(|_| WireError::Envelope("the envelope is not what the catalogue says"))?;
    if kind != expected {
        return Err(WireError::WrongType);
    }

    let key = VerifyingKey::from_bytes(&envelope.sender_public_key)
        .map_err(|_| WireError::BadSignature)?;
    let sig = Signature::from_bytes(&signed.signature);
    key.verify_strict(&to_be_signed(&signed.body), &sig)
        .map_err(|_| WireError::BadSignature)?;

    Ok(Opened {
        sender: envelope.sender_public_key,
        event_hash: event_hash(&signed.body),
        envelope,
    })
}

/// The payload of an opened message, decoded under its own cap.
fn payload_of<'a, T: Decode<'a, ()> + Encode<()>>(o: &'a Opened, cap: usize) -> Result<T, WireError> {
    from_canonical(&o.envelope.payload, cap)
        .map_err(|_| WireError::Malformed("the payload is not canonical"))
}

// ---------------------------------------------------------------------------
// JOIN_REQUEST
// ---------------------------------------------------------------------------

/// Put a join request on the wire, signed by the joiner's application key.
pub fn publish_join_request(
    req: &JoinRequest,
    joiner_key: &SigningKey,
    now_ms: u64,
) -> Result<Vec<u8>, WireError> {
    if req.peer_id.len() > PEER_ID_MAX {
        return Err(WireError::TooLong("peer_id"));
    }
    if req.display_name.len() > DISPLAY_NAME_MAX {
        return Err(WireError::TooLong("display_name"));
    }
    let body = JoinRequestBody {
        advert_hash: req.advert_hash,
        app_public_key: req.app_public_key,
        peer_id: req.peer_id.clone(),
        display_name: req.display_name.as_bytes().to_vec(),
        requested_seat: req.requested_seat,
        password_proof: req.password_proof,
        buyin: req.buyin,
        join_nonce: req.join_nonce,
        table_id: req.table_id,
    };
    seal_unchained(
        EventType::JoinRequest,
        &body,
        joiner_key,
        now_ms,
        JOIN_REQ_MAX,
    )
}

/// Take a join request off the wire.
///
/// Returns the request, the key that signed it, and the request's `event_hash`
/// — which the founder must echo in whichever answer it sends, and which it
/// cannot compute later because the bytes are gone by then.
pub fn receive_join_request(bytes: &[u8]) -> Result<(JoinRequest, [u8; 32], Hash), WireError> {
    let o = open(bytes, JOIN_REQ_MAX, EventType::JoinRequest)?;
    let b: JoinRequestBody = payload_of(&o, JOIN_REQ_MAX)?;

    if b.peer_id.len() > PEER_ID_MAX {
        return Err(WireError::TooLong("peer_id"));
    }
    if b.display_name.len() > DISPLAY_NAME_MAX {
        return Err(WireError::TooLong("display_name"));
    }
    let display_name = String::from_utf8(b.display_name)
        .map_err(|_| WireError::Malformed("display_name is not UTF-8"))?;
    if display_name.chars().any(|c| c.is_control()) {
        return Err(WireError::Malformed("display_name has control characters"));
    }

    let req = JoinRequest {
        advert_hash: b.advert_hash,
        app_public_key: b.app_public_key,
        peer_id: b.peer_id,
        display_name,
        requested_seat: b.requested_seat,
        password_proof: b.password_proof,
        buyin: b.buyin,
        join_nonce: b.join_nonce,
        table_id: b.table_id,
    };
    Ok((req, o.sender, o.event_hash))
}

// ---------------------------------------------------------------------------
// JOIN_ACCEPT and JOIN_REJECT
// ---------------------------------------------------------------------------

pub fn publish_join_accept(
    accept: &JoinAccept,
    table_key: &SigningKey,
    now_ms: u64,
) -> Result<Vec<u8>, WireError> {
    if accept.advert_event.len() > TABLE_AD_SIGNED_MAX {
        return Err(WireError::TooLong("advert_event"));
    }
    if accept.roster_so_far.len() > MAX_SEATS as usize {
        return Err(WireError::TooLong("roster"));
    }
    let body = JoinAcceptBody {
        request_hash: accept.request_hash,
        seat: accept.seat,
        advert_event: accept.advert_event.clone(),
        roster_so_far: roster_out(&accept.roster_so_far),
    };
    seal_unchained(
        EventType::JoinAccept,
        &body,
        table_key,
        now_ms,
        JOIN_RESP_MAX,
    )
}

/// Take an acceptance off the wire, with the key that signed it.
///
/// Whether that key is the table this client asked to join is
/// [`admit_accept`](crate::table::join::admit_accept)'s question, not this
/// module's: the rules live next door and this returns what they need.
pub fn receive_join_accept(bytes: &[u8]) -> Result<(JoinAccept, [u8; 32]), WireError> {
    let o = open(bytes, JOIN_RESP_MAX, EventType::JoinAccept)?;
    let b: JoinAcceptBody = payload_of(&o, JOIN_RESP_MAX)?;
    if b.advert_event.len() > TABLE_AD_SIGNED_MAX {
        return Err(WireError::TooLong("advert_event"));
    }
    Ok((
        JoinAccept {
            request_hash: b.request_hash,
            seat: b.seat,
            advert_event: b.advert_event,
            roster_so_far: roster_in(b.roster_so_far)?,
        },
        o.sender,
    ))
}

/// Refuse a seat.
///
/// Two of the eight reason codes have no mechanism behind them anywhere in the
/// corpus, and this refuses to send those rather than assert something the
/// founder cannot know. See [`RejectReason::emittable`].
pub fn publish_join_reject(
    request_hash: Hash,
    reason: RejectReason,
    retry_after_ms: u32,
    table_key: &SigningKey,
    now_ms: u64,
) -> Result<Vec<u8>, WireError> {
    if !reason.emittable() {
        return Err(WireError::Unencodable(
            "that reason has no mechanism behind it and is never sent",
        ));
    }
    let body = JoinRejectBody {
        request_hash,
        reason: reason.code(),
        retry_after_ms: retry_after_ms.min(3_600_000),
    };
    seal_unchained(
        EventType::JoinReject,
        &body,
        table_key,
        now_ms,
        JOIN_RESP_MAX,
    )
}

/// A refusal, as a **claim**: the founder may lie, so nothing here is proof of
/// anything and the reason is returned as the number it is.
pub fn receive_join_reject(bytes: &[u8]) -> Result<(Hash, u16, u32, [u8; 32]), WireError> {
    let o = open(bytes, JOIN_RESP_MAX, EventType::JoinReject)?;
    let b: JoinRejectBody = payload_of(&o, JOIN_RESP_MAX)?;
    Ok((b.request_hash, b.reason, b.retry_after_ms, o.sender))
}

// ---------------------------------------------------------------------------
// PLAYER_LIST
// ---------------------------------------------------------------------------

pub fn publish_player_list(
    list: &PlayerList,
    table_key: &SigningKey,
    now_ms: u64,
) -> Result<Vec<u8>, WireError> {
    if list.roster.len() > MAX_SEATS as usize {
        return Err(WireError::TooLong("roster"));
    }
    let body = PlayerListBody {
        roster: roster_out(&list.roster),
        table_params_hash: list.table_params_hash,
        list_serial: list.list_serial,
    };
    seal_unchained(
        EventType::PlayerList,
        &body,
        table_key,
        now_ms,
        JOIN_RESP_MAX,
    )
}

pub fn receive_player_list(bytes: &[u8]) -> Result<(PlayerList, [u8; 32]), WireError> {
    receive_player_list_at(bytes).map(|(l, s, _)| (l, s))
}

/// The same, and the founder's own signed emission time with it.
///
/// A list carries no timestamp of its own, but its **envelope** does and the
/// founder signed it. Discarding it left a client with no serial of its own —
/// which is every client until the first list arrives — with nothing to say
/// against a genuine list from an hour ago, replayed by anybody who had seen
/// it.
pub fn receive_player_list_at(
    bytes: &[u8],
) -> Result<(PlayerList, [u8; 32], u64), WireError> {
    let o = open(bytes, JOIN_RESP_MAX, EventType::PlayerList)?;
    let b: PlayerListBody = payload_of(&o, JOIN_RESP_MAX)?;
    Ok((
        PlayerList {
            roster: roster_in(b.roster)?,
            table_params_hash: b.table_params_hash,
            list_serial: b.list_serial,
        },
        o.sender,
        o.envelope.emitted_at_unix_ms,
    ))
}

// ---------------------------------------------------------------------------
// TABLE_READY
// ---------------------------------------------------------------------------

/// Ratify a roster.
///
/// Signed by **this seat's own** application key, not by the table's: §4.3 ends
/// the founder's special position here, and a ratification the founder could
/// have produced would ratify nothing.
pub fn publish_table_ready(
    ready: &TableReady,
    table_id: &Hash,
    genesis: &Hash,
    seat_key: &SigningKey,
    now_ms: u64,
    next_deadline_ms: u32,
) -> Result<Vec<u8>, WireError> {
    if ready.capability_set.len() > MAX_CAPABILITIES {
        return Err(WireError::TooLong("capability_set"));
    }
    if ready.capability_set.iter().any(|c| c.len() > CAPABILITY_MAX) {
        return Err(WireError::TooLong("a capability"));
    }
    let body = TableReadyBody {
        roster_hash: ready.roster_hash,
        list_serial: ready.list_serial,
        table_params_hash: ready.table_params_hash,
        my_seat: ready.my_seat,
        capability_set: ready.capability_set.clone(),
    };
    seal_ready(
        &body,
        table_id,
        genesis,
        seat_key,
        now_ms,
        next_deadline_ms,
    )
}

/// Take a ratification off the wire.
///
/// The envelope is checked against **this** client's `table_id` and its own
/// `GENESIS(0)`. A sender whose genesis differs joined a different game, and
/// that is precisely the fork D-013 exists to refuse — it is refused here,
/// before the payload is read, rather than surfacing later as a collective stage
/// that never completes.
pub fn receive_table_ready(
    bytes: &[u8],
    table_id: &Hash,
    genesis: &Hash,
) -> Result<(TableReady, [u8; 32], Hash), WireError> {
    let o = open(bytes, JOIN_RESP_MAX, EventType::TableReady)?;
    if &o.envelope.table_id != table_id {
        return Err(WireError::Envelope("a ratification of another table"));
    }
    if o.envelope.hand_id != 0 || o.envelope.sequence != 0 {
        return Err(WireError::Envelope("not stage 0 of the setup chain"));
    }
    if &o.envelope.previous_event_hash != genesis {
        return Err(WireError::Envelope(
            "a different genesis: the sender joined a different game",
        ));
    }

    let b: TableReadyBody = payload_of(&o, JOIN_RESP_MAX)?;
    if b.capability_set.len() > MAX_CAPABILITIES {
        return Err(WireError::TooLong("capability_set"));
    }
    if b.capability_set.iter().any(|c| c.len() > CAPABILITY_MAX) {
        return Err(WireError::TooLong("a capability"));
    }
    Ok((
        TableReady {
            roster_hash: b.roster_hash,
            list_serial: b.list_serial,
            table_params_hash: b.table_params_hash,
            my_seat: b.my_seat,
            capability_set: b.capability_set,
        },
        o.sender,
        o.event_hash,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::messages::ZERO32;
    use crate::security::rng::secret_32;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn entry(seat: u8) -> SeatEntry {
        SeatEntry {
            seat,
            app_public_key: [seat + 1; 32],
            peer_id: vec![seat; 38],
            display_name: format!("seat {seat}"),
            buyin: 1_000 + seat as u64,
        }
    }

    fn request() -> JoinRequest {
        JoinRequest {
            advert_hash: [7u8; 32],
            app_public_key: key(3).verifying_key().to_bytes(),
            peer_id: vec![9u8; 38],
            display_name: "Alice".into(),
            requested_seat: Some(2),
            password_proof: Some([4u8; 32]),
            buyin: 1_500,
            join_nonce: [5u8; 32],
            table_id: [6u8; 32],
        }
    }

    const NOW: u64 = 1_700_000_000_000;

    /// The round trip, for each of the five, over the wire this client actually
    /// writes. Nothing in the corpus had ever encoded one of these.
    /// **A list's age is its envelope's, and the founder signed it.**
    ///
    /// The body carries no time at all. Discarding the envelope's left a client
    /// with no serial of its own — which is every client until the first list
    /// arrives — with nothing to say against a genuine list from an hour ago,
    /// replayed by anybody who had seen it.
    ///
    /// **To make this fail:** have `receive_player_list_at` return `0`, or the
    /// time of decoding, instead of `o.envelope.emitted_at_unix_ms`.
    #[test]
    fn a_player_list_carries_the_time_its_founder_sealed_it() {
        let key = SigningKey::from_bytes(&[9; 32]);
        let list = PlayerList {
            roster: vec![],
            table_params_hash: [3; 32],
            list_serial: 7,
        };
        let old = 1_700_000_000_000u64;
        let wire = publish_player_list(&list, &key, old).unwrap();
        let (back, sender, at) = receive_player_list_at(&wire).unwrap();
        assert_eq!(back.list_serial, 7);
        assert_eq!(sender, key.verifying_key().to_bytes());
        assert_eq!(at, old, "the emission time is the founder's, not the reader's");

        // A second sealing an hour later is a different time, so a receiver can
        // tell the two apart even though every field of the body is identical.
        let fresh = publish_player_list(&list, &key, old + 3_600_000).unwrap();
        let (_, _, later) = receive_player_list_at(&fresh).unwrap();
        assert_eq!(later, old + 3_600_000);
    }

    #[test]
    fn every_formation_message_survives_the_wire() {
        let joiner = key(3);
        let table = key(11);

        let wire = publish_join_request(&request(), &joiner, NOW).unwrap();
        let (back, sender, hash) = receive_join_request(&wire).unwrap();
        assert_eq!(back, request());
        assert_eq!(sender, joiner.verifying_key().to_bytes());
        assert_ne!(hash, ZERO32);

        let accept = JoinAccept {
            request_hash: hash,
            seat: 2,
            advert_event: vec![1, 2, 3, 4],
            roster_so_far: vec![entry(0), entry(2)],
        };
        let wire = publish_join_accept(&accept, &table, NOW).unwrap();
        let (back, sender) = receive_join_accept(&wire).unwrap();
        assert_eq!(back, accept);
        assert_eq!(sender, table.verifying_key().to_bytes());

        let wire =
            publish_join_reject(hash, RejectReason::TableFull, 30_000, &table, NOW).unwrap();
        let (rh, reason, retry, sender) = receive_join_reject(&wire).unwrap();
        assert_eq!((rh, reason, retry), (hash, 1, 30_000));
        assert_eq!(sender, table.verifying_key().to_bytes());

        let list = PlayerList {
            roster: vec![entry(0), entry(1)],
            table_params_hash: [8u8; 32],
            list_serial: 4,
        };
        let wire = publish_player_list(&list, &table, NOW).unwrap();
        let (back, sender) = receive_player_list(&wire).unwrap();
        assert_eq!(back, list);
        assert_eq!(sender, table.verifying_key().to_bytes());

        let ready = TableReady {
            roster_hash: [2u8; 32],
            list_serial: 4,
            table_params_hash: [8u8; 32],
            my_seat: 1,
            capability_set: vec![b"deck/bs-bg12-secp256k1/1".to_vec()],
        };
        let table_id = [6u8; 32];
        let genesis = [9u8; 32];
        let wire =
            publish_table_ready(&ready, &table_id, &genesis, &joiner, NOW, 30_000).unwrap();
        let (back, sender, _) = receive_table_ready(&wire, &table_id, &genesis).unwrap();
        assert_eq!(back, ready);
        assert_eq!(sender, joiner.verifying_key().to_bytes());
    }

    /// A request with no seat preference and no password is the ordinary case
    /// and its two absent fields must survive as absent — a decoder that turned
    /// "any seat" into seat zero would seat everybody in the same chair.
    #[test]
    fn the_two_optional_fields_survive_being_absent() {
        let mut req = request();
        req.requested_seat = None;
        req.password_proof = None;
        let wire = publish_join_request(&req, &key(3), NOW).unwrap();
        let (back, _, _) = receive_join_request(&wire).unwrap();
        assert_eq!(back.requested_seat, None);
        assert_eq!(back.password_proof, None);
    }

    /// One flipped bit anywhere and nothing is accepted. The whole message is
    /// under the signature, so this is a statement about the envelope and the
    /// payload together.
    #[test]
    fn a_single_altered_byte_is_refused() {
        let wire = publish_join_request(&request(), &key(3), NOW).unwrap();
        let mut refused = 0;
        for i in 0..wire.len() {
            let mut bad = wire.clone();
            bad[i] ^= 0x01;
            if receive_join_request(&bad).is_err() {
                refused += 1;
            }
        }
        assert_eq!(
            refused,
            wire.len(),
            "some byte of a signed join request can be changed and still accepted"
        );
    }

    /// A message of one type must not open as another. The event type is inside
    /// the signature, so this is not a formality: without the check, a founder's
    /// acceptance and its refusal are the same bytes to a reader that does not
    /// look.
    #[test]
    fn a_message_does_not_open_as_a_different_type() {
        let table = key(11);
        let reject =
            publish_join_reject([1u8; 32], RejectReason::SeatTaken, 0, &table, NOW).unwrap();
        assert_eq!(receive_join_accept(&reject), Err(WireError::WrongType));
        assert_eq!(receive_player_list(&reject), Err(WireError::WrongType));
        assert!(receive_join_request(&reject).is_err());
    }

    /// The two reason codes with no mechanism behind them are never sent. A
    /// reason emitted falsely is worse than no reason at all, because the player
    /// believes it.
    #[test]
    fn a_reason_with_no_mechanism_is_never_put_on_the_wire() {
        let table = key(11);
        for reason in [RejectReason::Banned, RejectReason::CapabilityMismatch] {
            assert!(
                publish_join_reject([1u8; 32], reason, 0, &table, NOW).is_err(),
                "{reason:?} has no mechanism and was sent anyway"
            );
        }
        for reason in [
            RejectReason::TableFull,
            RejectReason::SeatTaken,
            RejectReason::BadPassword,
            RejectReason::BuyinOutOfRange,
            RejectReason::AdvertExpired,
            RejectReason::AlreadySeated,
        ] {
            assert!(publish_join_reject([1u8; 32], reason, 0, &table, NOW).is_ok());
        }
    }

    /// Every limit §4.3 states is enforced on the way in, not by the rule that
    /// reads the value afterwards.
    #[test]
    fn every_stated_limit_is_enforced() {
        let joiner = key(3);
        let table = key(11);

        let mut long = request();
        long.peer_id = vec![0u8; PEER_ID_MAX + 1];
        assert_eq!(
            publish_join_request(&long, &joiner, NOW),
            Err(WireError::TooLong("peer_id"))
        );

        let mut long = request();
        long.display_name = "x".repeat(DISPLAY_NAME_MAX + 1);
        assert_eq!(
            publish_join_request(&long, &joiner, NOW),
            Err(WireError::TooLong("display_name"))
        );

        let accept = JoinAccept {
            request_hash: [1u8; 32],
            seat: 0,
            advert_event: vec![0u8; TABLE_AD_SIGNED_MAX + 1],
            roster_so_far: vec![],
        };
        assert_eq!(
            publish_join_accept(&accept, &table, NOW),
            Err(WireError::TooLong("advert_event"))
        );

        let list = PlayerList {
            roster: (0..=MAX_SEATS).map(entry).collect(),
            table_params_hash: [0u8; 32],
            list_serial: 1,
        };
        assert_eq!(
            publish_player_list(&list, &table, NOW),
            Err(WireError::TooLong("roster"))
        );

        let ready = TableReady {
            roster_hash: [0u8; 32],
            list_serial: 1,
            table_params_hash: [0u8; 32],
            my_seat: 0,
            capability_set: vec![b"x".to_vec(); MAX_CAPABILITIES + 1],
        };
        assert_eq!(
            publish_table_ready(&ready, &[0u8; 32], &[0u8; 32], &joiner, NOW, 0),
            Err(WireError::TooLong("capability_set"))
        );
    }

    /// And a limit is enforced on **decode** too, against a sender that never
    /// asked this client's opinion. Checking only on the way out protects the
    /// honest and nobody else.
    #[test]
    fn an_over_long_field_from_a_stranger_is_refused() {
        let table = key(11);
        // Built by hand, past the publisher's own check.
        let body = PlayerListBody {
            roster: (0..=MAX_SEATS)
                .map(|s| SeatWire::from_entry(&entry(s)))
                .collect(),
            table_params_hash: [0u8; 32],
            list_serial: 1,
        };
        let wire =
            seal_unchained(EventType::PlayerList, &body, &table, NOW, JOIN_RESP_MAX).unwrap();
        assert_eq!(receive_player_list(&wire), Err(WireError::TooLong("roster")));

        let mut e = entry(0);
        e.peer_id = vec![0u8; PEER_ID_MAX + 1];
        let body = PlayerListBody {
            roster: vec![SeatWire::from_entry(&e)],
            table_params_hash: [0u8; 32],
            list_serial: 1,
        };
        let wire =
            seal_unchained(EventType::PlayerList, &body, &table, NOW, JOIN_RESP_MAX).unwrap();
        assert_eq!(
            receive_player_list(&wire),
            Err(WireError::TooLong("peer_id"))
        );
    }

    /// A display name is untrusted display data forever, and a control
    /// character in one is how a name overwrites the line above it in a terminal
    /// log.
    #[test]
    fn a_display_name_with_control_characters_is_refused() {
        let table = key(11);
        let mut e = entry(0);
        e.display_name = "a\u{7}b".into();
        let body = PlayerListBody {
            roster: vec![SeatWire::from_entry(&e)],
            table_params_hash: [0u8; 32],
            list_serial: 1,
        };
        let wire =
            seal_unchained(EventType::PlayerList, &body, &table, NOW, JOIN_RESP_MAX).unwrap();
        assert!(receive_player_list(&wire).is_err());
    }

    /// A ratification carries the table and the genesis it belongs to, and a
    /// ratification of another game is refused before its payload is read.
    ///
    /// This is D-013's fork check at the transport: two peers who joined under
    /// different parameters derive different `GENESIS(0)`, and without this the
    /// symptom is a collective stage that silently never completes.
    #[test]
    fn a_ratification_of_another_game_is_refused() {
        let seat = key(3);
        let ready = TableReady {
            roster_hash: [2u8; 32],
            list_serial: 1,
            table_params_hash: [8u8; 32],
            my_seat: 0,
            capability_set: vec![],
        };
        let wire =
            publish_table_ready(&ready, &[6u8; 32], &[9u8; 32], &seat, NOW, 0).unwrap();

        assert!(receive_table_ready(&wire, &[6u8; 32], &[9u8; 32]).is_ok());
        assert!(
            receive_table_ready(&wire, &[7u8; 32], &[9u8; 32]).is_err(),
            "a ratification of another table was taken"
        );
        assert!(
            receive_table_ready(&wire, &[6u8; 32], &[10u8; 32]).is_err(),
            "a ratification under another genesis was taken"
        );
    }

    /// `TABLE_READY` is the one message here that occupies a stage slot, and its
    /// envelope says so. The other four are unchained and carry the sentinels —
    /// which is what keeps a join RPC out of an equivocation proof.
    #[test]
    fn only_the_ratification_is_chained() {
        let joiner = key(3);
        let table = key(11);

        for wire in [
            publish_join_request(&request(), &joiner, NOW).unwrap(),
            publish_join_accept(
                &JoinAccept {
                    request_hash: [1u8; 32],
                    seat: 0,
                    advert_event: vec![],
                    roster_so_far: vec![],
                },
                &table,
                NOW,
            )
            .unwrap(),
            publish_join_reject([1u8; 32], RejectReason::TableFull, 0, &table, NOW).unwrap(),
            publish_player_list(
                &PlayerList {
                    roster: vec![],
                    table_params_hash: [0u8; 32],
                    list_serial: 1,
                },
                &table,
                NOW,
            )
            .unwrap(),
        ] {
            let signed: SignedEvent = from_canonical(&wire, JOIN_RESP_MAX).unwrap();
            let env: EventBody = from_canonical(&signed.body, JOIN_RESP_MAX).unwrap();
            assert_eq!(env.chain_scope, 0);
            assert_eq!(env.table_id, ZERO32);
            assert_eq!(env.sequence, 0);
            assert_eq!(env.previous_event_hash, ZERO32);
        }

        let wire = publish_table_ready(
            &TableReady {
                roster_hash: [2u8; 32],
                list_serial: 1,
                table_params_hash: [8u8; 32],
                my_seat: 0,
                capability_set: vec![],
            },
            &[6u8; 32],
            &[9u8; 32],
            &joiner,
            NOW,
            0,
        )
        .unwrap();
        let signed: SignedEvent = from_canonical(&wire, JOIN_RESP_MAX).unwrap();
        let env: EventBody = from_canonical(&signed.body, JOIN_RESP_MAX).unwrap();
        assert_eq!(env.chain_scope, 1);
        assert_eq!(env.table_id, [6u8; 32]);
        assert_eq!(env.previous_event_hash, [9u8; 32]);
    }

    /// The request hash a founder echoes is the `event_hash` of the bytes it
    /// received, and two different requests never share one. Without that, one
    /// acceptance answers two requests.
    #[test]
    fn two_requests_have_two_hashes() {
        let joiner = key(3);
        let a = publish_join_request(&request(), &joiner, NOW).unwrap();
        let mut other = request();
        other.join_nonce = secret_32().expect("the OS CSPRNG is available");
        let b = publish_join_request(&other, &joiner, NOW).unwrap();

        let (_, _, ha) = receive_join_request(&a).unwrap();
        let (_, _, hb) = receive_join_request(&b).unwrap();
        assert_ne!(ha, hb);
    }

    /// Garbage is refused rather than panicking. Every one of these arrives from
    /// a peer that chose the bytes.
    #[test]
    fn no_byte_string_can_panic_a_decoder() {
        let cases: Vec<Vec<u8>> = vec![
            vec![],
            vec![0x00],
            vec![0xff; 64],
            vec![0x82, 0x40, 0x40],
            (0..255u8).collect(),
        ];
        for bytes in cases {
            let _ = receive_join_request(&bytes);
            let _ = receive_join_accept(&bytes);
            let _ = receive_join_reject(&bytes);
            let _ = receive_player_list(&bytes);
            let _ = receive_table_ready(&bytes, &[0u8; 32], &[0u8; 32]);
        }
    }

    /// The signature is over the envelope, and the envelope names the sender. A
    /// message re-signed by somebody else is a different message, not the same
    /// one from a new source.
    #[test]
    fn a_message_cannot_be_re_signed_by_another_key() {
        let table = key(11);
        let impostor = key(12);
        let list = PlayerList {
            roster: vec![entry(0)],
            table_params_hash: [8u8; 32],
            list_serial: 1,
        };
        let honest = publish_player_list(&list, &table, NOW).unwrap();
        let forged = publish_player_list(&list, &impostor, NOW).unwrap();

        let (_, who) = receive_player_list(&honest).unwrap();
        assert_eq!(who, table.verifying_key().to_bytes());
        let (_, who) = receive_player_list(&forged).unwrap();
        assert_eq!(
            who,
            impostor.verifying_key().to_bytes(),
            "the sender reported must be the key that actually signed"
        );
        assert_ne!(honest, forged);
    }

    /// The signature covers the **domain-separated** bytes, the same way every
    /// other signed event in this crate does. A second convention here would
    /// mean a formation message and a hand event could not be verified by one
    /// code path.
    #[test]
    fn the_signature_covers_the_domain_separated_bytes() {
        let joiner = key(3);
        let wire = publish_join_request(&request(), &joiner, NOW).unwrap();
        let signed: SignedEvent = from_canonical(&wire, JOIN_REQ_MAX).unwrap();
        let key = joiner.verifying_key();
        let sig = Signature::from_bytes(&signed.signature);
        assert!(key.verify_strict(&to_be_signed(&signed.body), &sig).is_ok());
        // And not over the raw body: a signature that verified over both would
        // be a signature with no domain separation at all.
        assert!(key.verify_strict(&signed.body, &sig).is_err());
    }
}
