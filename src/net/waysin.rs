//! `S1-IT`: the relays a client behind a router can be reached through -- how
//! many it keeps, and how it learns that one is gone.
//!
//! A player behind address translation is reached through a relay that holds a
//! **reservation** for it, and the client kept exactly one: the first relay that
//! said yes. Two things were wrong with that, and one run showed both
//! (`split000336-9`, a table of nine that never set because its founder and one
//! seat never met):
//!
//! * **One way in is one point of failure, and public relays fail in a way the
//!   holder cannot see.** The founder's relay kept its reservation and took no
//!   NEW connection -- 314 dials of that relay by the eight other seats, every
//!   one *refused*, *cut off by the far end* or left with *no protocol in
//!   common* -- so nobody could come in
//!   through it for seven minutes while the founder, connected to it all along,
//!   saw a healthy reservation. go-libp2p's own clients keep two or three relays
//!   for this reason; this client now keeps [`WAYS_IN_WANTED`].
//! * **A way in that went away was never noticed.** `libp2p-relay` closes a
//!   circuit listener with `reason: Ok(())` when the connection to the relay
//!   closes -- the ordinary way a reservation dies -- and `net::run` matched
//!   `ListenerClosed` only with `reason: Err(_)`. So the flag that says *this
//!   client has a way in* stayed up for the rest of the process, no other relay
//!   was asked, and every circuit address of the dead reservation stayed
//!   *confirmed external* and went on being announced: the founder's dials of
//!   that seat were answered *the relay holds no reservation for it* twenty-one
//!   times by the relay the seat believed it was listening through.
//!
//! Nothing here touches the swarm. `net::run` owns that; this is the part that
//! can be wrong in a test.

use libp2p::{Multiaddr, PeerId};
use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

/// How many relays this client keeps a reservation on.
///
/// Two: one more than the one whose failure nothing on this side can see.
/// go-libp2p's AutoRelay keeps two by default for the same reason. Each costs a
/// connection and a renewal an hour; a dialler gets both relays' addresses and
/// tries them side by side.
pub const WAYS_IN_WANTED: usize = 2;

/// How long a reservation request counts as under way. A relay answers within a
/// round trip or two; past this the request is taken for lost and another relay
/// may be asked.
pub const RESERVATION_ASK_LINGERS: Duration = Duration::from_secs(10);

/// The most requests remembered. Identify arrives from hundreds of peers that
/// speak the relay protocol; only the recent ones matter.
pub const RESERVATION_ASKS_MAX: usize = 64;

/// The relays this client holds a reservation on, and the requests under way.
#[derive(Debug)]
pub struct WaysIn {
    wanted: usize,
    held: BTreeSet<PeerId>,
    asked: HashMap<PeerId, Instant>,
}

impl WaysIn {
    pub fn new(wanted: usize) -> Self {
        WaysIn { wanted: wanted.max(1), held: BTreeSet::new(), asked: HashMap::new() }
    }

    /// Whether a relay that has just been identified should be asked: this
    /// client holds and awaits fewer ways in than it wants, and this relay is
    /// neither held nor being asked.
    pub fn wants(&self, relay: &PeerId, now: Instant) -> bool {
        let under_way = self
            .asked
            .iter()
            .filter(|(r, at)| !self.held.contains(*r) && now.saturating_duration_since(**at) < RESERVATION_ASK_LINGERS)
            .count();
        !self.held.contains(relay)
            && !self.asked.get(relay).is_some_and(|at| now.saturating_duration_since(*at) < RESERVATION_ASK_LINGERS)
            && self.held.len() + under_way < self.wanted
    }

    /// Whether another relay is worth LOOKING for -- the lookup of the relays'
    /// key, once a discovery tick.
    pub fn short_of(&self) -> bool {
        self.held.len() < self.wanted
    }

    /// A reservation request went out.
    pub fn asked(&mut self, relay: PeerId, now: Instant) {
        if self.asked.len() >= RESERVATION_ASKS_MAX {
            self.asked.retain(|_, at| now.saturating_duration_since(*at) < RESERVATION_ASK_LINGERS);
            if self.asked.len() >= RESERVATION_ASKS_MAX {
                self.asked.clear();
            }
        }
        self.asked.insert(relay, now);
    }

    /// A relay accepted. `true` when it is a way in this client did not hold --
    /// not a renewal -- which is when the lobby has something new to be told.
    pub fn accepted(&mut self, relay: PeerId) -> bool {
        self.asked.remove(&relay);
        self.held.insert(relay)
    }

    /// A listener closed, **for whatever reason**: the relays among its
    /// addresses are ways in no longer. Returns the ones that were held.
    pub fn closed<'a>(&mut self, addresses: impl IntoIterator<Item = &'a Multiaddr>) -> Vec<PeerId> {
        let mut gone = Vec::new();
        for (relay, _) in addresses.into_iter().filter_map(super::dialable::relay_of) {
            if self.held.remove(&relay) {
                gone.push(relay);
            }
        }
        gone
    }

    pub fn any(&self) -> bool {
        !self.held.is_empty()
    }

    /// `S1-IZ`: whether this relay holds this client's reservation or is being
    /// asked for one -- which the trim of strangers must never close.
    pub fn keeps(&self, relay: &PeerId) -> bool {
        self.held.contains(relay) || self.asked.contains_key(relay)
    }

    pub fn len(&self) -> usize {
        self.held.len()
    }

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circuit(relay: &PeerId, leg: &str) -> Multiaddr {
        format!("{leg}/p2p/{relay}/p2p-circuit/p2p/{}", PeerId::random()).parse().expect("a literal")
    }

    /// Two ways in, asked for one after the other, and no third relay asked
    /// while two are held or on their way.
    ///
    /// The break that must make this fail: stop asking once ONE is held
    /// (`wanted` of 1), which is the client as it was.
    #[test]
    fn a_second_way_in_is_asked_for_and_a_third_is_not() {
        let t0 = Instant::now();
        let (a, b, c) = (PeerId::random(), PeerId::random(), PeerId::random());
        let mut w = WaysIn::new(WAYS_IN_WANTED);
        assert!(w.wants(&a, t0) && w.short_of());
        w.asked(a, t0);
        assert!(!w.wants(&a, t0), "not the relay already being asked");
        assert!(w.wants(&b, t0), "one request under way: a second relay is still wanted");
        w.asked(b, t0);
        assert!(!w.wants(&c, t0), "two under way: no third");
        assert!(w.accepted(a), "a new way in");
        assert!(!w.wants(&c, t0), "one held and one under way");
        assert!(w.accepted(b));
        assert!(!w.accepted(b), "a renewal is nothing new");
        assert!(w.any() && w.len() == 2 && !w.short_of());
        assert!(!w.wants(&c, t0 + Duration::from_secs(3600)), "and two held is enough for as long as they stand");
    }

    /// A request nobody answered stops counting, so a relay that swallowed it
    /// does not hold a place for the rest of the session.
    #[test]
    fn a_request_that_was_never_answered_gives_its_place_up() {
        let t0 = Instant::now();
        let (a, b, c) = (PeerId::random(), PeerId::random(), PeerId::random());
        let mut w = WaysIn::new(WAYS_IN_WANTED);
        w.asked(a, t0);
        w.asked(b, t0);
        assert!(!w.wants(&c, t0 + RESERVATION_ASK_LINGERS - Duration::from_millis(1)));
        assert!(w.wants(&c, t0 + RESERVATION_ASK_LINGERS));
        assert!(w.wants(&a, t0 + RESERVATION_ASK_LINGERS), "and the silent relay itself may be asked again");
    }

    /// **A listener that closed is a way in that is gone, whatever reason it
    /// closed with**, and the client wants another at once.
    ///
    /// The break that must make this fail: `closed` forgetting nothing.
    #[test]
    fn a_way_in_whose_listener_closed_is_gone_and_another_is_wanted() {
        let t0 = Instant::now();
        let (a, b, c) = (PeerId::random(), PeerId::random(), PeerId::random());
        let mut w = WaysIn::new(WAYS_IN_WANTED);
        w.accepted(a);
        w.accepted(b);
        // The listener's addresses, as the swarm reports them: every leg of the relay.
        let closed = [circuit(&a, "/ip4/147.75.87.27/tcp/4001"), circuit(&a, "/ip4/147.75.87.27/udp/4001/quic-v1")];
        assert_eq!(w.closed(&closed), [a], "said once, however many legs the relay had");
        assert!(w.len() == 1 && w.short_of() && w.wants(&c, t0));
        // Said twice, or of a relay never held, it is nobody's.
        assert!(w.closed(&closed).is_empty());
        assert!(w.closed(&[circuit(&c, "/ip4/1.1.1.1/tcp/4001")]).is_empty());
        assert_eq!(w.closed(&[circuit(&b, "/ip4/1.1.1.1/tcp/4001")]), [b]);
        assert!(!w.any(), "and with the last one gone this client has no way in, and knows it");
    }

    /// The book of requests is bounded: identify arrives from hundreds of peers
    /// that speak the relay protocol.
    #[test]
    fn the_requests_remembered_are_bounded() {
        let t0 = Instant::now();
        let mut w = WaysIn::new(WAYS_IN_WANTED);
        for i in 0..RESERVATION_ASKS_MAX * 3 {
            w.asked(PeerId::random(), t0 + Duration::from_millis(i as u64));
            assert!(w.asked.len() <= RESERVATION_ASKS_MAX);
        }
    }
}
