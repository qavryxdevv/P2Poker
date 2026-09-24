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

        // A fresh minute says nothing happened, in the same three lines.
        assert_eq!(
            book.take_lines(),
            vec![
                "dialbook dials: 0 started, 0 established, 0 failed (none), 0 strangers closed".to_owned(),
                "dialbook roads of the failed dials: 0 (none)".to_owned(),
                "dialbook walks: none".to_owned(),
                "dialbook reachability tests served to others: 0 (0 reached, 0 not)".to_owned(),
                "dialbook distinct peers: 0 dialled, 0 refused by the own limit".to_owned(),
            ]
        );
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
