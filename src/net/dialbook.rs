//! `S1-IZ`: where a client's dials come from, a minute at a time.
//!
//! Measuring `S1-IY` found every client dialling the public DHT nine to eleven
//! times a second, most of those dials failing, whatever its connection
//! ceiling -- and nothing in the client could say which of its own walks asked
//! for them. Kademlia can: every query ends with `QueryStats`, how many nodes it
//! asked, how many answered and how many did not. So each finished query is
//! entered here under the walk that asked it (`Walk`), and beside the walks the
//! dials themselves -- how many were started, how many became connections, and
//! of the failed ones what each **road** ended in, because a dial of a peer with
//! four addresses is four entries in a router's table and the dial is one line.
//!
//! A binary built to be measured says the book once a discovery tick and starts
//! a fresh minute; a player's client keeps it and says nothing. Nothing here
//! touches the swarm.

use std::collections::BTreeMap;

/// Which of this client's walks a finished DHT query was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Walk {
    /// `start_providing` under the lobby's key.
    LobbyAnnounce,
    /// A lookup of the lobby's key or of one of its slices (`S1-EX`).
    LobbyLookup,
    /// `start_providing` under the hour's key (`D-070`).
    HourAnnounce,
    /// A lookup of an hour's key.
    HourLookup,
    /// `start_providing` under a slice of the lobby.
    SliceAnnounce,
    /// `start_providing` under the key relays are looked for under.
    RelayAnnounce,
    /// A lookup of that key.
    RelayLookup,
    /// A bootstrap -- this client's own, or the library's periodic one.
    Bootstrap,
    /// A walk towards one peer: a founder the search could not reach, or the
    /// library's own.
    Closest,
    /// The library republishing a record this client provides.
    Republish,
    /// Anything else the public DHT's behaviour reported.
    Other,
    /// A query of the poker clients' own DHT (`/p2p-poker/kad/1`).
    PrivateDht,
}

impl Walk {
    pub fn name(self) -> &'static str {
        match self {
            Walk::LobbyAnnounce => "lobby-announce",
            Walk::LobbyLookup => "lobby-lookup",
            Walk::HourAnnounce => "hour-announce",
            Walk::HourLookup => "hour-lookup",
            Walk::SliceAnnounce => "slice-announce",
            Walk::RelayAnnounce => "relay-announce",
            Walk::RelayLookup => "relay-lookup",
            Walk::Bootstrap => "bootstrap",
            Walk::Closest => "closest",
            Walk::Republish => "republish",
            Walk::Other => "other",
            Walk::PrivateDht => "private-dht",
        }
    }
}

/// What the queries of one walk came to: how many, and the nodes they asked.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Walked {
    pub queries: u64,
    pub requests: u64,
    pub successes: u64,
    pub failures: u64,
}

/// `S1-IZ`: who asked for a dial. The walks' own counts cover only the nodes a
/// query asked; a measured client dialled four times as often as its walks
/// asked, and this is where the rest are told apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    /// The public DHT's behaviour asking a node for a query.
    KadQuery,
    /// The public DHT's behaviour checking whether the oldest node of a full
    /// bucket is still there, because a new peer is waiting for its place.
    KadBucketCheck,
    /// A player the lobby's answers named (`DIALS_PER_ANSWER`).
    LobbyProvider,
    /// The founder of a table this client is joining.
    Founder,
    /// The public network's entry points.
    EntryPoint,
    /// A peer found on this network by multicast.
    Mdns,
    /// Any other behaviour: the relay client, the hole punch, the poker
    /// clients' own DHT, a request to a peer not connected.
    OtherBehaviour,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::KadQuery => "kad-query",
            Source::KadBucketCheck => "kad-bucket-check",
            Source::LobbyProvider => "lobby-provider",
            Source::Founder => "founder",
            Source::EntryPoint => "entry-point",
            Source::Mdns => "mdns",
            Source::OtherBehaviour => "other-behaviour",
        }
    }
}

/// What a dial came to, for the book of sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Established,
    /// This client's own limit refused it once its handshake was done.
    OwnLimit,
    Failed,
}

/// One source's dials in a minute: started, and what the ones that ended came
/// to.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FromSource {
    pub started: u64,
    pub established: u64,
    pub own_limit: u64,
    pub failed: u64,
}

/// `S1-IZ`: the most dials held in flight with their source. A dial whose end
/// is never reported would otherwise be held for ever; a book that is full is
/// emptied, and the dials it held end unattributed.
const IN_FLIGHT_MAX: usize = 4096;

/// `S1-IZ`: the dials in flight, by their connection id, with who asked for
/// them -- this client's own sites say so before they dial, anything else is
/// told at the start, and the end reads the source back once.
#[derive(Debug)]
pub struct InFlight<K: std::hash::Hash + Eq> {
    ours: std::collections::HashMap<K, Source>,
    started: std::collections::HashMap<K, Source>,
}

impl<K: std::hash::Hash + Eq> Default for InFlight<K> {
    fn default() -> Self {
        InFlight { ours: Default::default(), started: Default::default() }
    }
}

impl<K: std::hash::Hash + Eq> InFlight<K> {
    /// One of this client's own sites is about to dial with this id.
    pub fn ours(&mut self, id: K, source: Source) {
        if self.ours.len() >= IN_FLIGHT_MAX {
            self.ours.clear();
        }
        self.ours.insert(id, source);
    }

    /// The swarm started the dial with this id: its source, which is this
    /// client's own site's word if one gave it and `otherwise` if none did.
    pub fn start(&mut self, id: K, otherwise: impl FnOnce() -> Source) -> Source {
        let source = self.ours.remove(&id).unwrap_or_else(otherwise);
        if self.started.len() >= IN_FLIGHT_MAX {
            self.started.clear();
        }
        self.started.insert(id, source);
        source
    }

    /// The dial with this id ended: who had asked for it, once.
    pub fn end(&mut self, id: &K) -> Option<Source> {
        self.started.remove(id)
    }
}

/// One minute of dials, and the walks that finished in it.
#[derive(Debug, Default)]
pub struct DialBook {
    walks: BTreeMap<Walk, Walked>,
    started: u64,
    established: u64,
    closed: u64,
    served_ok: u64,
    served_failed: u64,
    failed: BTreeMap<&'static str, u64>,
    roads: BTreeMap<&'static str, u64>,
    /// The peers dialled this minute, by a hash of their id: a count of dials
    /// far above it is somebody dialling the same peers again and again.
    dialled: std::collections::HashSet<u64>,
    refused_peers: std::collections::HashSet<u64>,
    /// `S1-IZ`: the dials by who asked for them. Fed only where a run is
    /// measured; empty, and said as empty, anywhere else.
    sources: BTreeMap<Source, FromSource>,
}

impl DialBook {
    /// A query of this walk finished, with the library's own count of the nodes
    /// it asked.
    pub fn walked(&mut self, walk: Walk, requests: u32, successes: u32, failures: u32) {
        let w = self.walks.entry(walk).or_default();
        w.queries += 1;
        w.requests += u64::from(requests);
        w.successes += u64::from(successes);
        w.failures += u64::from(failures);
    }

    /// The swarm started a dial, whoever asked for it -- of this peer, by a hash
    /// of its id, where the dial named one.
    pub fn dialing(&mut self, peer: Option<u64>) {
        self.started += 1;
        if let Some(p) = peer {
            self.dialled.insert(p);
        }
    }

    /// `S1-IZ`: a dial started, asked for by this source.
    pub fn dialing_from(&mut self, source: Source) {
        self.sources.entry(source).or_default().started += 1;
    }

    /// `S1-IZ`: what a dial this source asked for came to. A dial started in
    /// the minute before is entered in this one, as the walks are.
    pub fn came_to(&mut self, source: Source, outcome: Outcome) {
        let s = self.sources.entry(source).or_default();
        match outcome {
            Outcome::Established => s.established += 1,
            Outcome::OwnLimit => s.own_limit += 1,
            Outcome::Failed => s.failed += 1,
        }
    }

    /// This client's own limit refused a connection to this peer after its
    /// handshake.
    pub fn refused_peer(&mut self, peer: Option<u64>) {
        if let Some(p) = peer {
            self.refused_peers.insert(p);
        }
    }

    /// A dial became a connection.
    pub fn established(&mut self) {
        self.established += 1;
    }

    /// This client closed a stranger it no longer needed (`net::trim`).
    pub fn closed(&mut self) {
        self.closed += 1;
    }

    /// This client's AutoNAT server tested somebody else's address -- which it
    /// does by dialling that address.
    pub fn served(&mut self, ok: bool) {
        if ok {
            self.served_ok += 1;
        } else {
            self.served_failed += 1;
        }
    }

    /// A dial failed: its one word, and the kind of every road it tried
    /// (`dialfail::tally`).
    pub fn failed(&mut self, class: &'static str, roads: &[&'static str]) {
        *self.failed.entry(class).or_default() += 1;
        for road in roads {
            *self.roads.entry(road).or_default() += 1;
        }
    }

    /// The walks that finished in this minute, as the book holds them.
    pub fn walks(&self) -> &BTreeMap<Walk, Walked> {
        &self.walks
    }

    /// The minute in three lines -- the dials, the roads the failed ones
    /// tried, the walks -- and a fresh minute. The words are fixed and the
    /// numbers stand beside them, so a script can read them back.
    pub fn take_lines(&mut self) -> Vec<String> {
        let minute = std::mem::take(self);
        let failed: u64 = minute.failed.values().sum();
        let classes: Vec<String> = minute.failed.iter().map(|(k, n)| format!("{k}={n}")).collect();
        let roads: u64 = minute.roads.values().sum();
        let kinds: Vec<String> = minute.roads.iter().map(|(k, n)| format!("{k}={n}")).collect();
        let walks: Vec<String> = minute
            .walks
            .iter()
            .map(|(w, t)| format!("{}={}q/{}r/{}ok/{}fail", w.name(), t.queries, t.requests, t.successes, t.failures))
            .collect();
        let sources: Vec<String> = minute
            .sources
            .iter()
            .map(|(s, n)| format!("{}={}d/{}e/{}l/{}f", s.name(), n.started, n.established, n.own_limit, n.failed))
            .collect();
        vec![
            format!(
                "dialbook dials: {} started, {} established, {failed} failed ({}), {} strangers closed",
                minute.started,
                minute.established,
                if classes.is_empty() { "none".into() } else { classes.join(", ") },
                minute.closed
            ),
            format!(
                "dialbook roads of the failed dials: {roads} ({})",
                if kinds.is_empty() { "none".into() } else { kinds.join(", ") }
            ),
            format!(
                "dialbook walks: {}",
                if walks.is_empty() { "none".into() } else { walks.join(", ") }
            ),
            format!(
                "dialbook reachability tests served to others: {} ({} reached, {} not)",
                minute.served_ok + minute.served_failed,
                minute.served_ok,
                minute.served_failed
            ),
            format!(
                "dialbook distinct peers: {} dialled, {} refused by the own limit",
                minute.dialled.len(),
                minute.refused_peers.len()
            ),
            // `S1-IZ`: dials started, established, refused by the own limit,
            // failed otherwise -- by who asked for them.
            format!(
                "dialbook sources: {}",
                if sources.is_empty() { "none".into() } else { sources.join(", ") }
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minute as the book is fed it, said and then forgotten.
    ///
    /// The break that must make this fail: count a failed dial's roads as one,
    /// which is the dial and not what the router holds.
    #[test]
    fn a_minute_is_said_by_walk_by_dial_and_by_road_and_then_forgotten() {
        let mut book = DialBook::default();
        book.walked(Walk::LobbyAnnounce, 40, 25, 15);
        book.walked(Walk::LobbyAnnounce, 20, 12, 8);
        book.walked(Walk::HourLookup, 30, 10, 20);
        // Five dials of two peers, and one that named nobody.
        for peer in [Some(7), Some(7), Some(9), Some(7), None] {
            book.dialing(peer);
        }
        book.refused_peer(Some(7));
        book.refused_peer(Some(7));
        book.established();
        book.closed();
        book.closed();
        book.served(true);
        book.served(false);
        book.served(false);
        book.failed("no road", &["timed out", "timed out", "refused"]);
        book.failed("no road", &["no transport for the address"]);
        book.failed("own limit", &[]);

        assert_eq!(
            book.walks().get(&Walk::LobbyAnnounce),
            Some(&Walked { queries: 2, requests: 60, successes: 37, failures: 23 })
        );
        let lines = book.take_lines();
        assert_eq!(
            lines[0],
            "dialbook dials: 5 started, 1 established, 3 failed (no road=2, own limit=1), 2 strangers closed"
        );
        assert_eq!(
            lines[1],
            "dialbook roads of the failed dials: 4 (no transport for the address=1, refused=1, timed out=2)"
        );
        assert_eq!(
            lines[2],
            "dialbook walks: lobby-announce=2q/60r/37ok/23fail, hour-lookup=1q/30r/10ok/20fail"
        );
        assert_eq!(lines[3], "dialbook reachability tests served to others: 3 (1 reached, 2 not)");
        assert_eq!(lines[4], "dialbook distinct peers: 2 dialled, 1 refused by the own limit");
        assert_eq!(lines[5], "dialbook sources: none", "a book nobody told the sources of says so");

        // A fresh minute says nothing happened, in the same lines.
        assert_eq!(
            book.take_lines(),
            vec![
                "dialbook dials: 0 started, 0 established, 0 failed (none), 0 strangers closed".to_owned(),
                "dialbook roads of the failed dials: 0 (none)".to_owned(),
                "dialbook walks: none".to_owned(),
                "dialbook reachability tests served to others: 0 (0 reached, 0 not)".to_owned(),
                "dialbook distinct peers: 0 dialled, 0 refused by the own limit".to_owned(),
                "dialbook sources: none".to_owned(),
            ]
        );
    }

    /// `S1-IZ`: the dials by who asked for them, each source with its own
    /// four numbers, and forgotten with the minute.
    ///
    /// The break that must make this fail: enter a refusal by the own limit as
    /// a failure like any other, which hides what the limit costs.
    #[test]
    fn the_sources_are_said_each_with_what_its_dials_came_to() {
        let mut book = DialBook::default();
        for _ in 0..3 {
            book.dialing_from(Source::KadBucketCheck);
        }
        book.dialing_from(Source::KadQuery);
        book.dialing_from(Source::LobbyProvider);
        book.came_to(Source::KadBucketCheck, Outcome::OwnLimit);
        book.came_to(Source::KadBucketCheck, Outcome::OwnLimit);
        book.came_to(Source::KadBucketCheck, Outcome::Established);
        book.came_to(Source::KadQuery, Outcome::Failed);
        // A dial started last minute ends in this one.
        book.came_to(Source::Founder, Outcome::Established);
        let lines = book.take_lines();
        assert_eq!(
            lines[5],
            "dialbook sources: kad-query=1d/0e/0l/1f, kad-bucket-check=3d/1e/2l/0f, lobby-provider=1d/0e/0l/0f, founder=0d/1e/0l/0f"
        );
        assert_eq!(book.take_lines()[5], "dialbook sources: none");
    }

    /// `S1-IZ`: a dial this client's own site announced keeps that site's
    /// source; any other is what the start says; the end reads it once.
    ///
    /// The break that must make this fail: take the start's word over the
    /// site's, which puts every own dial under the behaviour that carried it.
    #[test]
    fn a_dial_keeps_the_source_that_asked_for_it_until_it_ends() {
        let mut f: InFlight<u32> = InFlight::default();
        f.ours(1, Source::LobbyProvider);
        assert_eq!(f.start(1, || Source::OtherBehaviour), Source::LobbyProvider);
        assert_eq!(f.start(2, || Source::KadBucketCheck), Source::KadBucketCheck);
        assert_eq!(f.end(&1), Some(Source::LobbyProvider));
        assert_eq!(f.end(&1), None, "once");
        assert_eq!(f.end(&2), Some(Source::KadBucketCheck));
        assert_eq!(f.end(&3), None, "a dial never started ends unattributed");
        // Bounded: a book fed ids that never end stays below its limit.
        for id in 0..(IN_FLIGHT_MAX as u32 * 2) {
            f.start(id, || Source::KadQuery);
        }
        assert!(f.started.len() <= IN_FLIGHT_MAX);
    }

    /// Every source has a name of its own.
    #[test]
    fn every_source_is_named_once() {
        let all = [
            Source::KadQuery,
            Source::KadBucketCheck,
            Source::LobbyProvider,
            Source::Founder,
            Source::EntryPoint,
            Source::Mdns,
            Source::OtherBehaviour,
        ];
        let names: std::collections::HashSet<&str> = all.iter().map(|s| s.name()).collect();
        assert_eq!(names.len(), all.len());
    }

    /// Every walk has a name of its own, so no two are added up in a script.
    #[test]
    fn every_walk_is_named_once() {
        let all = [
            Walk::LobbyAnnounce,
            Walk::LobbyLookup,
            Walk::HourAnnounce,
            Walk::HourLookup,
            Walk::SliceAnnounce,
            Walk::RelayAnnounce,
            Walk::RelayLookup,
            Walk::Bootstrap,
            Walk::Closest,
            Walk::Republish,
            Walk::Other,
            Walk::PrivateDht,
        ];
        let names: std::collections::HashSet<&str> = all.iter().map(|w| w.name()).collect();
        assert_eq!(names.len(), all.len());
    }
}
