//! The join RPC's transport: one request, one answer, on
//! `/p2p-poker/join/1`.
//!
//! §4.3 calls `JOIN_REQUEST` / `JOIN_ACCEPT` / `JOIN_REJECT` a **join RPC** and
//! everything else a table mesh, and the distinction is not stylistic. A join is
//! between two peers who are not yet at a table together: there is no mesh to
//! carry it, the joiner needs to know whether it was answered, and a request
//! that goes unanswered has to become a timeout rather than silence. That is
//! request-response, and libp2p has one.
//!
//! # The codec moves bytes and nothing else
//!
//! The payloads are already canonical CBOR, signed, with the signature over
//! exactly those bytes. Handing them to a codec that encodes — libp2p ships a
//! CBOR one — would put a second encoding around them, and a second encoding is
//! a second thing two implementations can disagree about while both verify. So
//! this codec writes a length and then the bytes, and reads the same back.
//!
//! # The length is the cap, and the cap is the specification's
//!
//! `JOIN_REQ_MAX` inbound, `JOIN_RESP_MAX` outbound, refused **before** the
//! allocation rather than after it. A reader that allocates what the sender
//! declares is a reader the sender chooses the working set of.

use std::io;

use futures::prelude::*;
use libp2p::{request_response, StreamProtocol};

use crate::protocol::constants::{JOIN_PROTOCOL, JOIN_REQ_MAX, JOIN_RESP_MAX};

/// The protocol name, from the constants table.
pub fn protocol() -> StreamProtocol {
    StreamProtocol::new(JOIN_PROTOCOL)
}

/// A signed formation message, as bytes.
pub type Frame = Vec<u8>;

/// Length-prefixed bytes, with the cap enforced before the allocation.
#[derive(Debug, Clone, Default)]
pub struct JoinCodec;

async fn read_capped<T>(io: &mut T, cap: usize) -> io::Result<Frame>
where
    T: AsyncRead + Unpin + Send,
{
    let mut len = [0u8; 4];
    io.read_exact(&mut len).await?;
    let len = u32::from_be_bytes(len) as usize;
    if len > cap {
        // Before the allocation. A peer that declares four gigabytes gets an
        // error, not four gigabytes.
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("a join frame of {len} bytes is over the {cap} cap"),
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
            "this client tried to send a join frame over its own cap",
        ));
    }
    io.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
    io.write_all(&bytes).await?;
    io.close().await
}

impl request_response::Codec for JoinCodec {
    type Protocol = StreamProtocol;
    type Request = Frame;
    type Response = Frame;

    async fn read_request<T>(&mut self, _: &StreamProtocol, io: &mut T) -> io::Result<Frame>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_capped(io, JOIN_REQ_MAX).await
    }

    async fn read_response<T>(&mut self, _: &StreamProtocol, io: &mut T) -> io::Result<Frame>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_capped(io, JOIN_RESP_MAX).await
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
        write_capped(io, req, JOIN_REQ_MAX).await
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
        write_capped(io, res, JOIN_RESP_MAX).await
    }
}

/// The table mesh topic for one table.
///
/// A topic per table, named by the table key. `PLAYER_LIST` and `TABLE_READY`
/// go to everybody at the table and to nobody else, and a single shared topic
/// would have every client in the lobby carrying every table's formation
/// traffic — which is both a waste and a way to learn who is sitting where.
pub fn table_topic(table_id: &[u8; 32]) -> libp2p::gossipsub::IdentTopic {
    let mut name = String::with_capacity(23 + 64);
    name.push_str("/p2p-poker/table/");
    for b in table_id {
        name.push_str(&format!("{b:02x}"));
    }
    name.push_str("/1");
    libp2p::gossipsub::IdentTopic::new(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::io::Cursor;
    use libp2p::request_response::Codec as _;

    /// The bytes that go in are the bytes that come out, unchanged. The
    /// signature covers exactly them, so a codec that touched them would break
    /// every verification downstream.
    #[tokio::test]
    async fn a_frame_survives_the_codec_unchanged() {
        let payload: Vec<u8> = (0..=255u8).cycle().take(3_000).collect();
        let mut buf = Vec::new();
        JoinCodec
            .write_request(&protocol(), &mut buf, payload.clone())
            .await
            .unwrap();
        let mut cursor = Cursor::new(buf);
        let back = JoinCodec
            .read_request(&protocol(), &mut cursor)
            .await
            .unwrap();
        assert_eq!(back, payload);
    }

    /// A declared length over the cap is refused before anything is allocated.
    #[tokio::test]
    async fn an_oversized_declaration_is_refused() {
        let mut framed = ((JOIN_REQ_MAX + 1) as u32).to_be_bytes().to_vec();
        framed.extend_from_slice(&[0u8; 8]);
        let mut cursor = Cursor::new(framed);
        let err = JoinCodec
            .read_request(&protocol(), &mut cursor)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    /// A truncated frame is an error rather than a short read that a caller
    /// might treat as a message.
    #[tokio::test]
    async fn a_truncated_frame_is_an_error() {
        let mut framed = 100u32.to_be_bytes().to_vec();
        framed.extend_from_slice(&[7u8; 40]);
        let mut cursor = Cursor::new(framed);
        assert!(JoinCodec
            .read_request(&protocol(), &mut cursor)
            .await
            .is_err());
    }

    /// A request and a response have different caps, and the response's is the
    /// larger — an acceptance carries the whole advert back plus a roster.
    #[tokio::test]
    async fn the_response_may_be_larger_than_the_request() {
        const { assert!(JOIN_RESP_MAX > JOIN_REQ_MAX) };
        let big = vec![0u8; JOIN_REQ_MAX + 1_000];
        let mut buf = Vec::new();
        assert!(JoinCodec
            .write_response(&protocol(), &mut buf, big.clone())
            .await
            .is_ok());
        let mut buf2 = Vec::new();
        assert!(JoinCodec
            .write_request(&protocol(), &mut buf2, big)
            .await
            .is_err());
    }

    /// One table, one topic, and two tables never share one. A shared topic
    /// would have every client in the lobby carrying every table's formation
    /// traffic and learning who sits where.
    #[test]
    fn every_table_gets_its_own_topic() {
        let a = table_topic(&[1u8; 32]);
        let b = table_topic(&[2u8; 32]);
        assert_ne!(a.to_string(), b.to_string());
        assert_eq!(table_topic(&[1u8; 32]).to_string(), a.to_string());
        assert!(a.to_string().starts_with("/p2p-poker/table/"));
        assert!(a.to_string().ends_with("/1"));
    }

    /// The protocol name is the one in the constants table, so the string in the
    /// handshake and the string in the documentation cannot drift apart.
    #[test]
    fn the_protocol_name_is_the_declared_one() {
        assert_eq!(protocol().as_ref(), JOIN_PROTOCOL);
    }
}
