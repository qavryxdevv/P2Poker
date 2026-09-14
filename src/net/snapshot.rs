//! The lobby snapshot RPC of `PROTOCOL.md` §7.5: one question, one answer, on
//! `/p2p-poker/lobby-snapshot/1`.
//!
//! # Why a question, when the lobby has a mesh
//!
//! An advert rides the lobby topic, and GossipSub tells a peer what topics
//! this client is on **once**: with the first connection to that peer, and
//! then only on a later `subscribe` (`libp2p-gossipsub-0.49.5/src/behaviour.rs`,
//! `on_connection_established`: `if other_established > 0 { return }`). A first
//! connection that could not carry that exchange leaves the peer deaf to this
//! client's subscriptions for as long as any connection between the two is
//! open -- and two clients on one machine reconnect to each other constantly,
//! so one is always open. Measured, `run113401-2`: the founder's status line
//! read *lobby topic: 0 of 0 subscribed peers grafted; subscribed []; connected
//! [the joiner]* for the whole run, its every `publish` came back
//! `NoPeersSubscribedToTopic`, and the joiner -- connected, subscribed, and
//! holding the founder in its own mesh -- never saw the table (`S1-DK`).
//!
//! A question asked over a stream of its own does not depend on any of that:
//! it needs a connection, which the two have, and it is answered or it times
//! out. So every poker peer is asked when it is first recognised and every
//! `AD_REBROADCAST_MS` after that, and the founder answers with the advert it
//! is offering right now. **The responder is not trusted for anything**
//! (§7.5): each advert in an answer goes through the same checklist as one
//! heard over gossip. And an answer that no longer names a table this client
//! lists from that founder is that table withdrawn -- a closed table leaves
//! the lobby at the next question rather than at `AD_TTL_MS`.
//!
//! # The codec moves bytes and nothing else
//!
//! As `joinrpc`: the bodies are canonical CBOR, signed, and a codec that
//! encoded would put a second encoding around a signature. Length-prefixed
//! bytes, with the caps enforced before the allocation -- `SNAPSHOT_REQ_MAX`
//! inbound and `SNAPSHOT_RESP_MAX` outbound, §13's numbers.

use std::io;

use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use futures::prelude::*;
use libp2p::{request_response, StreamProtocol};
use minicbor::{Decode, Encode};

use super::lobbytalk::CLOCK_SLACK_MS;
use crate::protocol::constants::{
    SNAPSHOT_MAX_ADS, SNAPSHOT_PROTOCOL, SNAPSHOT_REQ_MAX, SNAPSHOT_RESP_MAX, TABLE_AD_SIGNED_MAX,
};
use crate::protocol::messages::{EventBody, EventType, SignedEvent};
use crate::protocol::serialization::{from_canonical, to_canonical};
use crate::protocol::signatures::to_be_signed;

/// The protocol name, from the constants table.
pub fn protocol() -> StreamProtocol {
    StreamProtocol::new(SNAPSHOT_PROTOCOL)
}

/// A signed lobby message, as bytes.
pub type Frame = Vec<u8>;

/// Length-prefixed bytes, with the cap enforced before the allocation.
#[derive(Debug, Clone, Default)]
pub struct SnapshotCodec;

async fn read_capped<T>(io: &mut T, cap: usize) -> io::Result<Frame>
where
    T: AsyncRead + Unpin + Send,
{
    let mut len = [0u8; 4];
    io.read_exact(&mut len).await?;
    let len = u32::from_be_bytes(len) as usize;
    if len > cap {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("a snapshot frame of {len} bytes is over the {cap} cap"),
        ));
    }
    let mut buf = vec![0u8; len];
    io.read_exact(&mut buf).await?;
    Ok(buf)
}

async fn write_capped<T>(io: &mut T, bytes: Frame, cap: usize) -> io::Result<()>
where
    T: AsyncWrite + Unpin + Send,
{
    if bytes.len() > cap {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "this client tried to send a snapshot frame over its own cap",
        ));
    }
    io.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
    io.write_all(&bytes).await?;
    io.close().await
}

impl request_response::Codec for SnapshotCodec {
    type Protocol = StreamProtocol;
    type Request = Frame;
    type Response = Frame;

    async fn read_request<T>(&mut self, _: &StreamProtocol, io: &mut T) -> io::Result<Frame>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_capped(io, SNAPSHOT_REQ_MAX).await
    }

    async fn read_response<T>(&mut self, _: &StreamProtocol, io: &mut T) -> io::Result<Frame>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_capped(io, SNAPSHOT_RESP_MAX).await
    }

    async fn write_request<T>(
        &mut self,
        _: &StreamProtocol,
        io: &mut T,
        req: Frame,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_capped(io, req, SNAPSHOT_REQ_MAX).await
    }

    async fn write_response<T>(
        &mut self,
        _: &StreamProtocol,
        io: &mut T,
        res: Frame,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_capped(io, res, SNAPSHOT_RESP_MAX).await
    }
}

/// §7.5's `LOBBY_SNAPSHOT_REQUEST` body: `n(0) max_tables`, `n(1)
/// since_unix_ms` (0 for everything), `n(2) nonce`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct Ask {
    #[n(0)]
    pub max_tables: u16,
    #[n(1)]
    pub since_unix_ms: u64,
    #[cbor(n(2), with = "minicbor::bytes")]
    pub nonce: [u8; 32],
}

/// §7.5's `LOBBY_SNAPSHOT_RESPONSE` body: `n(0) request_nonce`, `n(1)
/// adverts`, each a complete signed `LOBBY_TABLE_AD`, `n(2) truncated`.
/// `D-047`: the table's word about a seat out of it for good -- the
/// certificate of its fourth absence, so the seat's own client can verify
/// it against the table's roster from the bytes alone and needs nobody's
/// authority for it, the founder's included.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct OutWord {
    #[cbor(n(0), with = "minicbor::bytes")]
    pub table_id: [u8; 32],
    #[cbor(n(1), with = "minicbor::bytes")]
    pub app_key: [u8; 32],
    #[n(2)]
    pub seat: u8,
    #[n(3)]
    pub hand_id: u64,
    #[n(4)]
    pub cert: minicbor::bytes::ByteVec,
}

/// `D-047`: how many such words one answer carries at most.
pub const SNAPSHOT_MAX_OUT: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct Tell {
    #[cbor(n(0), with = "minicbor::bytes")]
    pub request_nonce: [u8; 32],
    #[n(1)]
    pub adverts: Vec<minicbor::bytes::ByteVec>,
    #[n(2)]
    pub truncated: bool,
    /// `D-047`: the words about seats out for good, from the tables this
    /// client sits at; absent in an answer from before the field existed.
    #[n(3)]
    pub out: Option<Vec<OutWord>>,
}

/// Why a frame was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    Malformed(&'static str),
    /// A signed event of another type.
    WrongType,
    /// Emitted too far from this client's clock.
    Stale,
    /// The signature does not verify.
    Forged,
    /// An answer to a question this client did not ask.
    NotMyQuestion,
    /// More adverts than §7.5 allows, or one over its own cap.
    TooMany,
}

/// The question: everything, up to `SNAPSHOT_MAX_ADS`, under a fresh nonce.
pub fn ask(key: &SigningKey, nonce: [u8; 32], now_ms: u64) -> Result<Vec<u8>, &'static str> {
    let body = Ask {
        max_tables: SNAPSHOT_MAX_ADS as u16,
        since_unix_ms: 0,
        nonce,
    };
    seal(key, EventType::LobbySnapshotRequest, &body, now_ms)
}

/// The answer: the adverts this client offers, under the question's nonce.
pub fn tell(
    key: &SigningKey,
    request_nonce: [u8; 32],
    adverts: Vec<Vec<u8>>,
    truncated: bool,
    now_ms: u64,
) -> Result<Vec<u8>, &'static str> {
    if adverts.len() > SNAPSHOT_MAX_ADS {
        return Err("more adverts than a snapshot may carry");
    }
    if adverts.iter().any(|a| a.len() > TABLE_AD_SIGNED_MAX) {
        return Err("an advert over its cap");
    }
    tell_out(key, request_nonce, adverts, truncated, Vec::new(), now_ms)
}

/// `tell`, with the words about seats out for good (`D-047`).
pub fn tell_out(
    key: &SigningKey,
    request_nonce: [u8; 32],
    adverts: Vec<Vec<u8>>,
    truncated: bool,
    out: Vec<OutWord>,
    now_ms: u64,
) -> Result<Vec<u8>, &'static str> {
    if adverts.len() > SNAPSHOT_MAX_ADS {
        return Err("more adverts than a snapshot may carry");
    }
    if adverts.iter().any(|a| a.len() > TABLE_AD_SIGNED_MAX) {
        return Err("an advert over its cap");
    }
    if out.len() > SNAPSHOT_MAX_OUT || out.iter().any(|w| w.cert.len() > crate::table::hand::FRAME_CAP) {
        return Err("more words about seats out than a snapshot may carry");
    }
    let body = Tell {
        request_nonce,
        adverts: adverts.into_iter().map(minicbor::bytes::ByteVec::from).collect(),
        truncated,
        out: if out.is_empty() { None } else { Some(out) },
    };
    seal(key, EventType::LobbySnapshotResponse, &body, now_ms)
}

/// A question off the wire: its body and the asker's application key.
pub fn open_ask(bytes: &[u8], now_ms: u64) -> Result<(Ask, [u8; 32]), Refused> {
    let (payload, who, _) = open(bytes, EventType::LobbySnapshotRequest, SNAPSHOT_REQ_MAX, now_ms)?;
    let ask: Ask = from_canonical(&payload, SNAPSHOT_REQ_MAX)
        .map_err(|_| Refused::Malformed("not a snapshot request body"))?;
    Ok((ask, who))
}

/// An answer off the wire, checked against the nonce this client asked under:
/// the adverts it carries, each still to be taken through §7.2's checklist,
/// and when the responder emitted it -- on the responder's own clock, which is
/// the clock its adverts carry, so the two compare: an answer older than an
/// advert this client holds from that founder says nothing about it (the
/// founder had not set the table up when it answered; measured in
/// `run192317-3`, where the first question beat the hosting by a second and
/// its empty answer withdrew the table for 29 s).
pub fn open_tell(bytes: &[u8], expected_nonce: &[u8; 32], now_ms: u64) -> Result<(Vec<Vec<u8>>, u64), Refused> {
    open_tell_out(bytes, expected_nonce, now_ms).map(|(adverts, at, _)| (adverts, at))
}

/// `open_tell`, with the words about seats out for good (`D-047`) -- as
/// bytes still: the receiver verifies each against its own roster.
pub fn open_tell_out(
    bytes: &[u8],
    expected_nonce: &[u8; 32],
    now_ms: u64,
) -> Result<(Vec<Vec<u8>>, u64, Vec<OutWord>), Refused> {
    let (payload, _who, emitted_at) = open(bytes, EventType::LobbySnapshotResponse, SNAPSHOT_RESP_MAX, now_ms)?;
    let tell: Tell = from_canonical(&payload, SNAPSHOT_RESP_MAX)
        .map_err(|_| Refused::Malformed("not a snapshot response body"))?;
    if &tell.request_nonce != expected_nonce {
        return Err(Refused::NotMyQuestion);
    }
    if tell.adverts.len() > SNAPSHOT_MAX_ADS {
        return Err(Refused::TooMany);
    }
    let adverts: Vec<Vec<u8>> = tell.adverts.iter().map(|b| b.to_vec()).collect();
    if adverts.iter().any(|a| a.len() > TABLE_AD_SIGNED_MAX) {
        return Err(Refused::TooMany);
    }
    let out = tell.out.unwrap_or_default();
    if out.len() > SNAPSHOT_MAX_OUT || out.iter().any(|w| w.cert.len() > crate::table::hand::FRAME_CAP) {
        return Err(Refused::TooMany);
    }
    Ok((adverts, emitted_at, out))
}

fn seal<B: Encode<()>>(
    key: &SigningKey,
    kind: EventType,
    body: &B,
    now_ms: u64,
) -> Result<Vec<u8>, &'static str> {
    let body_bytes = to_canonical(body).map_err(|_| "the body does not encode")?;
    let envelope = EventBody::unchained(kind, key.verifying_key().to_bytes(), body_bytes, now_ms)
        .ok_or("a snapshot message is an unchained event")?;
    let envelope_bytes = to_canonical(&envelope).map_err(|_| "the envelope does not encode")?;
    let signature = {
        use ed25519_dalek::Signer;
        key.sign(&to_be_signed(&envelope_bytes))
    };
    to_canonical(&SignedEvent {
        body: envelope_bytes,
        signature: signature.to_bytes(),
    })
    .map_err(|_| "the signed event does not encode")
}

fn open(bytes: &[u8], kind: EventType, cap: usize, now_ms: u64) -> Result<(Vec<u8>, [u8; 32], u64), Refused> {
    if bytes.len() > cap {
        return Err(Refused::Malformed("over the cap"));
    }
    let signed: SignedEvent =
        from_canonical(bytes, cap).map_err(|_| Refused::Malformed("not a signed event"))?;
    let envelope: EventBody =
        from_canonical(&signed.body, cap).map_err(|_| Refused::Malformed("not an envelope"))?;
    match envelope.check_envelope() {
        Ok(k) if k == kind => {}
        Ok(_) => return Err(Refused::WrongType),
        Err(_) => return Err(Refused::Malformed("the envelope is not conforming")),
    }
    if envelope.emitted_at_unix_ms.abs_diff(now_ms) > CLOCK_SLACK_MS {
        return Err(Refused::Stale);
    }
    let who = envelope.sender_public_key;
    let verifying = VerifyingKey::from_bytes(&who).map_err(|_| Refused::Forged)?;
    let signature = Signature::from_bytes(&signed.signature);
    verifying
        .verify_strict(&to_be_signed(&signed.body), &signature)
        .map_err(|_| Refused::Forged)?;
    Ok((envelope.payload, who, envelope.emitted_at_unix_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_700_000_000_000;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    #[test]
    fn a_question_and_its_answer_round_trip_under_the_nonce() {
        let asker = key(1);
        let founder = key(2);
        let nonce = [7u8; 32];
        let q = ask(&asker, nonce, NOW).unwrap();
        assert!(q.len() <= SNAPSHOT_REQ_MAX);
        let (body, who) = open_ask(&q, NOW + 1_000).unwrap();
        assert_eq!(body.nonce, nonce);
        assert_eq!(body.max_tables as usize, SNAPSHOT_MAX_ADS);
        assert_eq!(who, asker.verifying_key().to_bytes());

        let adverts = vec![vec![1u8, 2, 3], vec![9u8; 40]];
        let a = tell(&founder, body.nonce, adverts.clone(), false, NOW + 2_000).unwrap();
        assert!(a.len() <= SNAPSHOT_RESP_MAX);
        assert_eq!(open_tell(&a, &nonce, NOW + 3_000).unwrap(), (adverts, NOW + 2_000), "the adverts, and when the founder answered");
        // An empty answer is an answer: the founder offers nothing.
        let none = tell(&founder, nonce, Vec::new(), false, NOW).unwrap();
        assert!(open_tell(&none, &nonce, NOW).unwrap().0.is_empty());
    }

    /// Broken deliberately, one rule at a time: an answer to somebody else's
    /// question, a question read as an answer, a forged signature, a stale
    /// clock, too many adverts.
    #[test]
    fn what_is_refused_and_why() {
        let asker = key(1);
        let founder = key(2);
        let nonce = [7u8; 32];
        let a = tell(&founder, nonce, vec![vec![1u8]], false, NOW).unwrap();
        assert_eq!(open_tell(&a, &[8u8; 32], NOW), Err(Refused::NotMyQuestion));
        let q = ask(&asker, nonce, NOW).unwrap();
        assert_eq!(open_tell(&q, &nonce, NOW), Err(Refused::WrongType));
        assert_eq!(open_ask(&a, NOW).map(|_| ()), Err(Refused::WrongType));
        let mut forged = a.clone();
        let last = forged.len() - 1;
        forged[last] ^= 0x01;
        assert_eq!(open_tell(&forged, &nonce, NOW), Err(Refused::Forged));
        assert_eq!(open_tell(&a, &nonce, NOW + CLOCK_SLACK_MS * 2), Err(Refused::Stale));
        let many: Vec<Vec<u8>> = (0..=SNAPSHOT_MAX_ADS).map(|_| vec![1u8]).collect();
        assert!(tell(&founder, nonce, many, false, NOW).is_err(), "a sender is stopped at its own cap");
        assert!(
            tell(&founder, nonce, vec![vec![0u8; TABLE_AD_SIGNED_MAX + 1]], false, NOW).is_err(),
            "and at an advert over its cap"
        );
    }
}
