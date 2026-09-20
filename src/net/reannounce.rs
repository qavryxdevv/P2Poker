//! `S1-IS`: when does this client say its GossipSub subscriptions again?
//!
//! GossipSub tells a peer what this client subscribes to **once**, in the hello
//! of the first connection, and that hello is sometimes lost -- measured: a
//! founder connected to both joiners that knew of no peer on the lobby topic,
//! and a table that never formed for it. The library has no *send my
//! subscriptions to this peer*, so the repair that exists is blunt: drop a
//! topic and take it again, which tells **everybody connected**, makes each of
//! them prune this client from its mesh for that topic until a later heartbeat,
//! and -- on a table's topic -- makes every seat say its formation messages
//! again, over the table's Tox group as well.
//!
//! The repair was aimed wrong and fired on about two meetings in three
//! (`S1-IS`: 3 828 *joined the lobby mesh* lines in one nine-seat run, by eight
//! peers). Two faults, both in *when* and *of whom* the question was asked:
//!
//! * **Of whom.** Every poker client was asked to hold *this client's own table
//!   topic*. A client that is not a seat of this table never will, so for every
//!   other player in the lobby the answer was *no* on every meeting, for ever.
//!   A peer is now asked only for what it can hold: [`wanted`].
//! * **When.** The question was asked at the identify -- before the peer's own
//!   hello can have arrived on that very connection. It is now asked
//!   [`LOOK_AFTER`] later, and only a subscription **still** unknown then is a
//!   hello that was lost.
//!
//! And one re-announce repairs every peer at once, so they are bounded:
//! [`AT_MOST_EVERY`].
//!
//! Nothing here touches the swarm. `net::run` owns that; this is the part that
//! can be wrong in a test.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// How long after a poker client is identified its subscriptions are looked at.
///
/// The hello travels on the connection the identify just finished on; between
/// two seats of one bed it arrives within milliseconds, across two networks
/// within a round trip or two. Three seconds is past either, and short beside
/// the seconds a table takes to seat anybody.
pub const LOOK_AFTER: Duration = Duration::from_secs(3);

/// The least time between two re-announces.
///
/// One re-announce tells everybody connected, so a second one a moment later
/// repairs nothing the first did not -- it only prunes this client from every
/// mesh again. A peer still unknown inside this window is looked at again
/// after it.
pub const AT_MOST_EVERY: Duration = Duration::from_secs(30);

/// The most peers waiting for their look. A buffer fed from the network is
/// bounded where it is filled; the oldest waits are the ones given up.
pub const REANNOUNCE_WAITS_MAX: usize = 512;

/// The topics a peer should be known to hold: the lobby's two of any poker
/// client, and a table's topic **only of a peer that is at that table**.
pub fn wanted<T: Clone>(lobby: &[T], tables_the_peer_is_at: &[T]) -> Vec<T> {
    lobby.iter().chain(tables_the_peer_is_at).cloned().collect()
}

/// Which of `wanted` GossipSub does not know the peer to hold.
pub fn missing<T: PartialEq + Clone>(known: &[T], wanted: &[T]) -> Vec<T> {
    wanted.iter().filter(|w| !known.contains(w)).cloned().collect()
}

/// The peers whose subscriptions are still to be looked at, and when topics
/// were last said again.
#[derive(Debug)]
pub struct Reannounce<P: Ord + Clone> {
    pending: BTreeMap<P, Instant>,
    last: Option<Instant>,
    /// How many times topics were said again, for the harness's count.
    pub said: u64,
    /// How many looks found nothing missing -- a hello that simply arrived.
    pub fine: u64,
}

impl<P: Ord + Clone> Default for Reannounce<P> {
    fn default() -> Self {
        Reannounce { pending: BTreeMap::new(), last: None, said: 0, fine: 0 }
    }
}

impl<P: Ord + Clone> Reannounce<P> {
    /// A poker client was identified. Met again on a new connection, its wait
    /// starts again: that connection's hello is the one in flight now.
    pub fn met(&mut self, peer: P, now: Instant) {
        if self.pending.len() >= REANNOUNCE_WAITS_MAX && !self.pending.contains_key(&peer) {
            if let Some(oldest) = self.pending.iter().min_by_key(|(_, at)| **at).map(|(p, _)| p.clone()) {
                self.pending.remove(&oldest);
            }
        }
        self.pending.insert(peer, now);
    }

    /// The peer is gone; there is nobody to repair anything for.
    pub fn gone(&mut self, peer: &P) {
        self.pending.remove(peer);
    }

    /// The peers whose look is due, taken off the list. The caller looks at
    /// each and calls [`Self::fine`], [`Self::later`] or [`Self::announced`].
    pub fn due(&mut self, now: Instant) -> Vec<P> {
        let due: Vec<P> = self
            .pending
            .iter()
            .filter(|(_, at)| now.saturating_duration_since(**at) >= LOOK_AFTER)
            .map(|(p, _)| p.clone())
            .collect();
        for p in &due {
            self.pending.remove(p);
        }
        due
    }

    /// Nothing was missing: the hello arrived, as it nearly always does.
    pub fn was_fine(&mut self) {
        self.fine += 1;
    }

    /// Whether topics may be said again now.
    pub fn may_announce(&self, now: Instant) -> bool {
        self.last.is_none_or(|at| now.saturating_duration_since(at) >= AT_MOST_EVERY)
    }

    /// Something is missing but it is too soon after the last re-announce --
    /// which told this peer too, if it was connected. Looked at again later.
    pub fn later(&mut self, peer: P, now: Instant) {
        self.met(peer, now);
    }

    /// Topics were said again, to everybody.
    pub fn announced(&mut self, now: Instant) {
        self.last = Some(now);
        self.said += 1;
    }

    pub fn waiting(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **`S1-IS`, of whom: a peer is asked only for what it can hold.** A player
    /// who is not at this table is never missing this table's topic -- which is
    /// the whole of why the repair fired on every meeting with every stranger.
    #[test]
    fn a_stranger_is_not_asked_for_this_tables_topic() {
        let (lobby, chat, table, other_table) = ("lobby", "chat", "table-A", "table-B");
        // A lobby player who knows nothing of table A, and holds what a lobby player holds.
        let stranger_knows = [lobby, chat];
        assert!(missing(&stranger_knows, &wanted(&[lobby, chat], &[])).is_empty(), "a stranger with the lobby is complete");
        // The rule as it was: asked for the table's topic too, it was missing for ever.
        assert_eq!(missing(&stranger_knows, &wanted(&[lobby, chat], &[table])), vec![table]);

        // A seat of this table IS asked for it, and a lost hello shows.
        assert_eq!(missing(&[lobby, chat], &wanted(&[lobby, chat], &[table])), vec![table]);
        assert!(missing(&[lobby, chat, table], &wanted(&[lobby, chat], &[table])).is_empty());
        // A seat of two of this client's tables (multitabling) is asked for both.
        assert_eq!(missing(&[lobby, table], &wanted(&[lobby, chat], &[table, other_table])), vec![chat, other_table]);
        // And a peer GossipSub knows nothing of at all is missing everything it should hold.
        assert_eq!(missing(&[], &wanted(&[lobby, chat], &[])), vec![lobby, chat]);
    }

    /// **`S1-IS`, when: not at the identify.** The look comes [`LOOK_AFTER`]
    /// later, once; a peer met again waits again; a peer that left is not
    /// looked at.
    #[test]
    fn the_look_comes_after_the_hello_has_had_its_time() {
        let t0 = Instant::now();
        let mut r: Reannounce<u8> = Reannounce::default();
        r.met(1, t0);
        r.met(2, t0 + Duration::from_secs(2));
        assert!(r.due(t0).is_empty(), "asked at the identify, the answer is always 'not yet'");
        assert!(r.due(t0 + Duration::from_millis(2_900)).is_empty());
        assert_eq!(r.due(t0 + LOOK_AFTER), vec![1]);
        assert!(r.due(t0 + LOOK_AFTER).is_empty(), "looked at once");
        assert_eq!(r.due(t0 + Duration::from_secs(5)), vec![2]);

        // Met again on a new connection: the wait starts again.
        r.met(3, t0);
        r.met(3, t0 + Duration::from_secs(2));
        assert!(r.due(t0 + Duration::from_secs(4)).is_empty());
        assert_eq!(r.due(t0 + Duration::from_secs(5)), vec![3]);

        // Gone before its look: nobody to repair anything for.
        r.met(4, t0);
        r.gone(&4);
        assert!(r.due(t0 + Duration::from_secs(60)).is_empty());
        assert_eq!(r.waiting(), 0);
    }

    /// **One re-announce repairs everybody, so they are bounded.** The break this
    /// catches is the one the census measured: a re-announce per meeting.
    #[test]
    fn saying_it_again_is_bounded() {
        let t0 = Instant::now();
        let mut r: Reannounce<u8> = Reannounce::default();
        assert!(r.may_announce(t0), "the first is never held back");
        r.announced(t0);
        assert!(!r.may_announce(t0 + Duration::from_secs(1)));
        assert!(!r.may_announce(t0 + AT_MOST_EVERY - Duration::from_millis(1)));
        assert!(r.may_announce(t0 + AT_MOST_EVERY));
        assert_eq!(r.said, 1);

        // Held back, a peer is looked at again -- after the window, not dropped.
        r.later(9, t0 + Duration::from_secs(1));
        assert!(r.due(t0 + Duration::from_secs(2)).is_empty());
        assert_eq!(r.due(t0 + Duration::from_secs(4)), vec![9]);

        // Nine seats meeting each other within a second -- a table forming --
        // may cost one re-announce, not nine.
        let mut r: Reannounce<u8> = Reannounce::default();
        let mut announces = 0;
        for peer in 0..9u8 {
            r.met(peer, t0 + Duration::from_millis(u64::from(peer) * 100));
        }
        for tick in 0..10u64 {
            let now = t0 + Duration::from_secs(tick * 2);
            for peer in r.due(now) {
                // every one of them with something missing: the worst case
                if r.may_announce(now) {
                    r.announced(now);
                    announces += 1;
                } else {
                    r.later(peer, now);
                }
            }
        }
        assert_eq!(announces, 1, "the nine were repaired by one");
    }

    /// The list of waits is bounded, and what is given up is the oldest wait.
    #[test]
    fn the_waiting_list_is_bounded() {
        let t0 = Instant::now();
        let mut r: Reannounce<u32> = Reannounce::default();
        for peer in 0..(REANNOUNCE_WAITS_MAX as u32 + 50) {
            r.met(peer, t0 + Duration::from_millis(u64::from(peer)));
        }
        assert_eq!(r.waiting(), REANNOUNCE_WAITS_MAX);
        let due = r.due(t0 + Duration::from_secs(60));
        assert!(!due.contains(&0) && due.contains(&(REANNOUNCE_WAITS_MAX as u32 + 49)), "the oldest waits were the ones given up");
    }
}
