//! `S1-IZ`: closing the strangers this client no longer needs.
//!
//! **No connection of this client ever went idle.** `ping` opens a stream on
//! every connection every fifteen seconds, and an open stream is what the
//! swarm's idle timer is reset by -- so a connection a DHT walk made to a
//! stranger stayed for as long as the stranger kept it, whatever
//! `IDLE_CONNECTION_TIMEOUT_MS` said. The pool filled to the connection limit
//! within seconds of a start and stayed full, and from then on the limit
//! refused every new connection **after its handshake**: 560 a minute on a
//! client alone in the lobby, each a lookup's failed request, each lookup asking
//! some 250 nodes where a few dozen answer, 909 dials a minute in all.
//!
//! So the strangers are closed here, the way IPFS Kubo's connection manager
//! closes them: past [`TRIM_ABOVE`] connected peers, the oldest strangers that
//! have had [`TRIM_GRACE`] to finish what they were dialled for are
//! disconnected until [`TRIM_TO`] remain. A stranger is anybody this client
//! does not keep: never a poker client, never a peer let through the limit
//! because a dial of it matters (`S1-IT`), never a relay holding or being asked
//! for this client's reservation. The limit stays, as the net under this.
//!
//! Nothing here touches the swarm.

use std::time::{Duration, Instant};

/// More connected peers than this, and the oldest strangers are closed.
///
/// **Measured, and the cautious one of two** (`S1-IZ`, 2026-09-24, side by
/// side from one build). Past 64, down to 48, after 20 s: a mean of 62 to 72
/// connections held against 83 without a trim, tables and relay reservations
/// the same on one machine, and across two networks a table set within 120 s
/// with 60 hands and new tables seen 4 to 5 s after they opened. Past 16 after
/// 10 s held fewer still (59), with a quarter fewer handshakes thrown away by
/// this client's own limit -- but its one run across two networks set its table
/// 30 s later and dealt a third fewer hands, which one run cannot tell from
/// the network's day, and a trim that might cost a table is not bought for a
/// dozen connections. Neither changes how often a client dials: that is the
/// walks' width, which Kademlia sets.
pub const TRIM_ABOVE: usize = 64;

/// How many peers a trim leaves, if there are enough strangers old enough.
pub const TRIM_TO: usize = 48;

/// How long a stranger is left alone after it connected -- long enough for the
/// request a walk dialled it for, which Kademlia gives ten seconds.
pub const TRIM_GRACE: Duration = Duration::from_secs(20);

/// A connected peer as the trim sees it: since when, and whether it is kept.
#[derive(Debug, Clone)]
pub struct Connected<P> {
    pub peer: P,
    pub since: Instant,
    pub kept: bool,
}

/// The three numbers a trim runs by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trim {
    pub above: usize,
    pub to: usize,
    pub grace: Duration,
}

impl Default for Trim {
    fn default() -> Self {
        Trim { above: TRIM_ABOVE, to: TRIM_TO, grace: TRIM_GRACE }
    }
}

impl Trim {
    /// The trim this client runs by. **Only a binary built to be measured can
    /// be told other numbers** -- `P2P_POKER_TRIM_ABOVE`, `P2P_POKER_TRIM_TO`,
    /// `P2P_POKER_TRIM_GRACE_S` -- so that two trims compared come out of one
    /// build; a player's client runs by the constants.
    pub fn chosen() -> Self {
        #[allow(unused_mut)]
        let mut trim = Trim::default();
        #[cfg(feature = "fault-harness")]
        {
            let read = |name: &str| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok());
            if let Some(v) = read("P2P_POKER_TRIM_ABOVE") {
                trim.above = v as usize;
            }
            if let Some(v) = read("P2P_POKER_TRIM_TO") {
                trim.to = v as usize;
            }
            if let Some(v) = read("P2P_POKER_TRIM_GRACE_S") {
                trim.grace = Duration::from_secs(v);
            }
            trim.to = trim.to.min(trim.above);
        }
        trim
    }
}

/// The peers to disconnect now, oldest first: nothing while at most
/// [`TRIM_ABOVE`] are connected, and otherwise the oldest strangers past their
/// grace until [`TRIM_TO`] would remain -- or all of them, if there are fewer.
pub fn to_close<P: Clone>(connected: &[Connected<P>], now: Instant) -> Vec<P> {
    to_close_by(Trim::default(), connected, now)
}

/// [`to_close`], by the numbers given.
pub fn to_close_by<P: Clone>(trim: Trim, connected: &[Connected<P>], now: Instant) -> Vec<P> {
    if connected.len() <= trim.above {
        return Vec::new();
    }
    let mut strangers: Vec<&Connected<P>> = connected
        .iter()
        .filter(|c| !c.kept && now.saturating_duration_since(c.since) >= trim.grace)
        .collect();
    strangers.sort_by_key(|c| c.since);
    let over = connected.len() - trim.to;
    strangers.into_iter().take(over).map(|c| c.peer.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, secs: u64) -> Instant {
        start + Duration::from_secs(secs)
    }

    /// The owner's case in miniature: a pool of strangers the pings keep open,
    /// three poker clients and a relay among them.
    ///
    /// The break that must make this fail: close the kept peers when there are
    /// not strangers enough, which is a player's table going dark to save a
    /// router an entry.
    #[test]
    fn the_oldest_strangers_past_their_grace_are_closed_and_nobody_kept_is() {
        let start = Instant::now();
        let now = at(start, 1_000);
        // 70 peers: 3 kept (a relay and two players), 60 strangers connected long
        // ago, 7 strangers that have just connected.
        let mut connected: Vec<Connected<u32>> = Vec::new();
        for i in 0..3 {
            connected.push(Connected { peer: 1_000 + i, since: at(start, i as u64), kept: true });
        }
        for i in 0..60 {
            connected.push(Connected { peer: i, since: at(start, 10 + i as u64), kept: false });
        }
        for i in 0..7 {
            connected.push(Connected { peer: 500 + i, since: at(start, 995), kept: false });
        }
        // By numbers that leave room -- above 64, down to 48, after 20 s: 22
        // closed, the oldest strangers, in the order they came.
        let roomy = Trim { above: 64, to: 48, grace: Duration::from_secs(20) };
        let closed = to_close_by(roomy, &connected, now);
        assert_eq!(closed, (0..22).collect::<Vec<u32>>());
        assert!(!closed.iter().any(|p| *p >= 500), "a stranger inside its grace is left alone");
        assert!(!closed.iter().any(|p| *p >= 1_000), "nobody kept is closed");
        // At the threshold nothing is closed at all.
        assert!(to_close_by(roomy, &connected[..64], now).is_empty());

        // And an eager trim -- past 16 after 10 s -- takes every old stranger
        // down to the sixteen the pool is allowed; the young and the kept stay.
        let eager = Trim { above: 16, to: 16, grace: Duration::from_secs(10) };
        let closed = to_close_by(eager, &connected, now);
        assert_eq!(closed, (0..54).collect::<Vec<u32>>());
        assert!(!closed.iter().any(|p| *p >= 500) && !closed.iter().any(|p| *p >= 1_000));
    }

    /// The numbers the measurement chose, pinned: changing them is a new
    /// measurement, not an edit.
    #[test]
    fn the_trim_runs_by_the_measured_numbers() {
        assert_eq!(Trim::default(), Trim { above: 64, to: 48, grace: Duration::from_secs(20) });
        assert_eq!(Trim::default(), Trim { above: TRIM_ABOVE, to: TRIM_TO, grace: TRIM_GRACE });
    }

    /// A trim told to keep no stranger past its grace closes every one of them,
    /// at any size of the pool -- and still none that is kept or young.
    #[test]
    fn a_trim_by_other_numbers_closes_every_stranger_past_its_grace() {
        let start = Instant::now();
        let now = at(start, 100);
        let connected = vec![
            Connected { peer: 1u32, since: at(start, 0), kept: false },
            Connected { peer: 2, since: at(start, 50), kept: false },
            Connected { peer: 3, since: at(start, 95), kept: false },
            Connected { peer: 4, since: at(start, 0), kept: true },
        ];
        let eager = Trim { above: 0, to: 0, grace: Duration::from_secs(10) };
        assert_eq!(to_close_by(eager, &connected, now), vec![1, 2]);
        // The constants leave a pool this small alone.
        assert!(to_close(&connected, now).is_empty());
    }

    /// With the kept and the young making up most of the pool, fewer are closed
    /// than would bring it to the target -- and never one of them.
    #[test]
    fn a_pool_of_kept_and_young_peers_is_trimmed_only_as_far_as_it_can_be() {
        let start = Instant::now();
        let now = at(start, 100);
        let mut connected: Vec<Connected<u32>> = Vec::new();
        for i in 0..60 {
            connected.push(Connected { peer: i, since: at(start, 0), kept: true });
        }
        for i in 0..5 {
            connected.push(Connected { peer: 100 + i, since: at(start, 10), kept: false });
        }
        for i in 0..5 {
            connected.push(Connected { peer: 200 + i, since: at(start, 95), kept: false });
        }
        assert_eq!(to_close(&connected, now), vec![100, 101, 102, 103, 104]);
    }
}
