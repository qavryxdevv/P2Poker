//! What a table needs from a network, and nothing else.
//!
//! D-019 moves a table's traffic onto a Tox group while the lobby stays on
//! libp2p. This is the seam that makes that a change of transport rather than a
//! change of game: `TableSession`, the protocol and the state machine speak
//! through here and never name libp2p, Tox, GossipSub or a peer id.
//!
//! It is written **before** either side needs it. A seam added after the hand is
//! wired is a refactor of working code; added first, it costs one trait.
//!
//! # The one rule this type exists to enforce
//!
//! [`FromTable::claimed`] is what the transport *says* about who sent something,
//! and it is **advisory**. It is an `Option` and it is named `claimed` so that
//! reading a sender out of it looks like what it is: a hint from a channel.
//!
//! Who actually sent a message is decided by the signature inside the payload,
//! against the player key on the ratified roster. A Tox group can be joined by
//! anybody who has the chat id, and a chat id travels in a public lobby
//! advertisement — so "it arrived over the table's group" is worth precisely
//! nothing as a claim about authorship. The same is true of GossipSub, where a
//! peer relaying somebody else's message is the ordinary case.

use std::fmt;

/// A player, as the protocol knows them: the public half of the key that signs
/// their events (§20). **Not** a peer id, a Tox public key, or anything else a
/// transport happens to use — those are the transport's business and stop here.
pub type PlayerId = [u8; 32];

/// Something that arrived over a table's transport.
#[derive(Clone, PartialEq, Eq)]
pub struct FromTable {
    /// Who the transport believes sent this. **Advisory, and often absent.**
    ///
    /// Present when the transport happens to know — a Tox group tells you which
    /// member a packet came from — and it is a convenience for logging and for
    /// rate limiting, never an authorisation. See the module documentation.
    pub claimed: Option<PlayerId>,
    /// The bytes, exactly as they were sent.
    pub bytes: Vec<u8>,
}

impl fmt::Debug for FromTable {
    /// Length rather than contents. A table's traffic is hole cards and shuffle
    /// proofs, and a debug line that printed them would be a debug line that
    /// leaked them into a log file.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FromTable")
            .field("claimed", &self.claimed.map(|k| hex8(&k)))
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

fn hex8(k: &PlayerId) -> String {
    k[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// Why something could not be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// Nobody is there to receive it. Not an error at a table that is still
    /// filling, which is why it is its own case.
    NobodyThere,
    /// The message is larger than this transport will carry.
    TooLarge { bytes: usize, cap: usize },
    /// The transport has stopped: the group was left, the connection is gone.
    Closed,
    /// Anything else, with whatever the transport had to say about it.
    Other(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NobodyThere => write!(f, "nobody is at the table yet"),
            Self::TooLarge { bytes, cap } => {
                write!(f, "{bytes} bytes is over this transport's {cap}")
            }
            Self::Closed => write!(f, "the table's transport has closed"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TransportError {}

/// A table's network, whatever it happens to be.
///
/// Deliberately small. Everything a poker table does is one of four things: say
/// something to everybody, say something to one player who is catching up, hear
/// what was said, and stop. Membership is **not** here — who is at the table is
/// the ratified roster's answer, and a transport that offered its own list would
/// be offering a second one to disagree with it.
#[async_trait::async_trait]
pub trait TableTransport: Send {
    /// Say something to everyone at the table.
    async fn broadcast(&mut self, bytes: &[u8]) -> Result<(), TransportError>;

    /// Say something to one player.
    ///
    /// For the snapshot a joiner needs and the events after it. A transport
    /// with no private channel may implement this as a broadcast — it costs
    /// bandwidth and leaks nothing, because everything is signed and a snapshot
    /// is public to the table anyway.
    async fn send_to(&mut self, to: &PlayerId, bytes: &[u8]) -> Result<(), TransportError>;

    /// The next thing that arrived, or `None` once the transport has closed.
    ///
    /// Cancel-safe: this is awaited inside a `select!` beside the rest of the
    /// node's work, and a `next` that lost a message when its future was
    /// dropped would lose it in exactly the situation the table is busiest.
    async fn next(&mut self) -> Option<FromTable>;

    /// Leave, and stop.
    ///
    /// Called when a player leaves a table and when the client shuts down. It
    /// must be safe to call twice: a client that crashes on the way out is a
    /// client that leaves a seat occupied.
    async fn leave(&mut self);

    /// The largest message this transport will carry, for a caller that has to
    /// decide whether to split something.
    fn cap(&self) -> usize;
}

/// The transport a table has when its traffic rides the node's own swarm.
///
/// Two channels rather than a reference to the swarm, because the swarm lives
/// inside the node's `select!` loop and is borrowed by it for the whole run.
/// The loop forwards: what is written here it publishes, what it receives on the
/// table's topic it puts in here.
pub struct ChannelTransport {
    out: tokio::sync::mpsc::Sender<Vec<u8>>,
    inbox: tokio::sync::mpsc::Receiver<FromTable>,
    cap: usize,
}

impl ChannelTransport {
    pub fn new(
        out: tokio::sync::mpsc::Sender<Vec<u8>>,
        inbox: tokio::sync::mpsc::Receiver<FromTable>,
        cap: usize,
    ) -> Self {
        Self { out, inbox, cap }
    }
}

#[async_trait::async_trait]
impl TableTransport for ChannelTransport {
    async fn broadcast(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        if bytes.len() > self.cap {
            return Err(TransportError::TooLarge {
                bytes: bytes.len(),
                cap: self.cap,
            });
        }
        self.out
            .send(bytes.to_vec())
            .await
            .map_err(|_| TransportError::Closed)
    }

    /// One topic, so a message to one player is a message to the table.
    ///
    /// It costs bandwidth and leaks nothing: everything is signed, and a
    /// snapshot is public to the table by construction.
    async fn send_to(&mut self, _to: &PlayerId, bytes: &[u8]) -> Result<(), TransportError> {
        self.broadcast(bytes).await
    }

    async fn next(&mut self) -> Option<FromTable> {
        self.inbox.recv().await
    }

    async fn leave(&mut self) {
        self.inbox.close();
    }

    fn cap(&self) -> usize {
        self.cap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(cap: usize) -> (ChannelTransport, tokio::sync::mpsc::Receiver<Vec<u8>>, tokio::sync::mpsc::Sender<FromTable>) {
        let (out_tx, out_rx) = tokio::sync::mpsc::channel(8);
        let (in_tx, in_rx) = tokio::sync::mpsc::channel(8);
        (ChannelTransport::new(out_tx, in_rx, cap), out_rx, in_tx)
    }

    #[tokio::test]
    async fn what_is_broadcast_comes_out_of_the_channel() {
        let (mut t, mut out, _in) = pair(1024);
        t.broadcast(b"hello").await.unwrap();
        assert_eq!(out.recv().await.unwrap(), b"hello".to_vec());
    }

    #[tokio::test]
    async fn a_message_over_the_cap_is_refused_rather_than_split() {
        let (mut t, _out, _in) = pair(4);
        assert_eq!(
            t.broadcast(b"more than four").await,
            Err(TransportError::TooLarge {
                bytes: 14,
                cap: 4
            })
        );
    }

    #[tokio::test]
    async fn what_arrives_is_handed_over_with_its_claim() {
        let (mut t, _out, inbox) = pair(1024);
        let who = [7u8; 32];
        inbox
            .send(FromTable {
                claimed: Some(who),
                bytes: b"an event".to_vec(),
            })
            .await
            .unwrap();
        let got = t.next().await.unwrap();
        assert_eq!(got.claimed, Some(who));
        assert_eq!(got.bytes, b"an event");
    }

    /// A closed transport says so rather than blocking, and says it every time.
    #[tokio::test]
    async fn leaving_twice_is_safe_and_then_it_is_shut() {
        let (mut t, out, _in) = pair(1024);
        t.leave().await;
        t.leave().await;
        assert_eq!(t.next().await, None);
        drop(out);
        assert_eq!(t.broadcast(b"anyone?").await, Err(TransportError::Closed));
    }

    /// The debug line must not carry the table's cards into a log file.
    #[test]
    fn the_debug_form_shows_a_length_and_not_the_bytes() {
        let m = FromTable {
            claimed: Some([0xab; 32]),
            bytes: b"the ace of spades".to_vec(),
        };
        let shown = format!("{m:?}");
        assert!(shown.contains("17"), "{shown}");
        assert!(!shown.contains("ace"), "{shown}");
        assert!(shown.contains("abababab"), "the sender is still identifiable");
    }
}
