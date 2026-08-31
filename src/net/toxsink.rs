//! Where a table's game traffic goes when it rides Tox (D-019), and a stub of
//! the same shape when this build has no Tox at all.
//!
//! # Why this type exists rather than `#[cfg]` in the node loop
//!
//! `run.rs` is one `select!` inside one loop, and it is the most carefully
//! measured file in this project — the single-exit verdict in its table arm was
//! a day's debugging to arrive at. Scattering `#[cfg(feature = "tox")]` through
//! it would mean two versions of that loop, only one of which any given build
//! compiles, and the untested one would be the one that rots.
//!
//! So the cfg lives here, once, behind an API that is the same either way. A
//! build without the feature gets a sink that is always empty, whose
//! [`next`](TableSink::next) never resolves, and whose branch in the `select!`
//! is therefore inert. The loop reads identically in both.
//!
//! # What rides it, and what does not
//!
//! **Only the hand.** D-019 is explicit that everything up to the table
//! existing — discovery, the lobby, the join RPC, the roster, the ratification
//! that produces `session_id` — stays on libp2p exactly as it is. The table's
//! own mesh keeps carrying `PLAYER_LIST` and `TABLE_READY`; what moves is the
//! game.

use crate::table::transport::FromTable;

/// The table's game transport, when there is one.
///
/// Empty until a table forms on Tox, and empty for ever in a build without the
/// feature.
pub struct TableSink {
    #[cfg(feature = "tox")]
    inner: Option<crate::tox::table::ToxTable>,
}

impl Default for TableSink {
    fn default() -> Self {
        Self::none()
    }
}

impl TableSink {
    /// No Tox table. The node publishes to its GossipSub mesh as before.
    pub const fn none() -> Self {
        Self {
            #[cfg(feature = "tox")]
            inner: None,
        }
    }

    /// Whether a table's traffic is on Tox.
    ///
    /// The one question `run.rs` asks, and it is what decides where
    /// `publish_hand` sends. In a build without the feature it is always false
    /// and the compiler removes the branch.
    pub fn is_on_tox(&self) -> bool {
        #[cfg(feature = "tox")]
        {
            self.inner.is_some()
        }
        #[cfg(not(feature = "tox"))]
        {
            false
        }
    }

    /// Take a Tox table over.
    #[cfg(feature = "tox")]
    pub fn set(&mut self, table: crate::tox::table::ToxTable) {
        self.inner = Some(table);
    }

    /// Give it up, leaving the group.
    pub fn clear(&mut self) {
        #[cfg(feature = "tox")]
        {
            // Dropping the handle tells the driver to leave and joins its
            // thread, which flushes whatever it still holds — the last message
            // of a hand is exactly what is in that queue.
            self.inner = None;
        }
    }

    /// Send one whole protocol message. **Not async, and not blocking.**
    ///
    /// `publish_hand` is called from inside the node loop, from arms that
    /// already hold the swarm, and an `await` there would be an await in the
    /// middle of handling one event. The driver's channel is bounded, so a
    /// caller that outruns it gets `false` rather than a stall — and a message
    /// dropped here is re-sent by the five-second loop that exists because no
    /// transport in this design keeps history.
    ///
    /// `false` also when there is no Tox table, which is how `publish_hand`
    /// falls back to the mesh.
    pub fn try_broadcast(&self, bytes: &[u8]) -> bool {
        #[cfg(feature = "tox")]
        {
            match self.inner.as_ref() {
                Some(t) => t.try_broadcast(bytes),
                None => false,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            let _ = bytes;
            false
        }
    }

    /// The next message from the table's group.
    ///
    /// **Never resolves when there is no Tox table**, which is what makes the
    /// `select!` branch safe to write unconditionally: a branch whose future is
    /// pending contributes nothing, in either build.
    ///
    /// Cancel-safe, because the underlying receiver is: `select!` drops this
    /// future every time another branch wins, and a `next` that lost a message
    /// on being dropped would lose it exactly when the table is busiest.
    pub async fn next(&mut self) -> Option<FromTable> {
        #[cfg(feature = "tox")]
        {
            use crate::table::transport::TableTransport;
            match self.inner.as_mut() {
                Some(t) => t.next().await,
                None => std::future::pending().await,
            }
        }
        #[cfg(not(feature = "tox"))]
        {
            std::future::pending().await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty sink says no to everything and never yields, in both builds.
    /// That is what lets `run.rs` carry the branch unconditionally.
    #[tokio::test]
    async fn an_empty_sink_is_inert() {
        let mut sink = TableSink::none();
        assert!(!sink.is_on_tox());
        assert!(!sink.try_broadcast(b"anything"));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), sink.next())
                .await
                .is_err(),
            "an empty sink must never resolve, or the select! branch would spin"
        );
    }

    /// Clearing an empty one is safe, because leaving a table happens on paths
    /// that do not all know whether there was one.
    #[test]
    fn clearing_nothing_is_safe() {
        let mut sink = TableSink::default();
        sink.clear();
        sink.clear();
        assert!(!sink.is_on_tox());
    }
}
