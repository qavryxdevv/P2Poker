//! `S1-IV`: what a lookup of a provider key found, said **once**, when it ends.
//!
//! Kademlia reports a `get_providers` query answer by answer: one event for
//! every DHT node that replied, **whether or not that node named anybody** --
//! and most do not, because most nodes on the way to a key hold no record of it
//! and answer with closer nodes only. The client wrote a line for each of those
//! events, and the line said *N player(s) in the public lobby*. So one lookup
//! that found thirty-one players read, within one second, *1 player*, *24
//! players*, *31 players* and *0 players* a dozen times over: each line true of
//! one node's reply and none of them true of the lobby.
//!
//! Measured, `split190546-9` (nine seats, 420 s): **7 443 of 24 643 lines, 30 %,
//! were that line, and 4 203 of those said 0** -- at about eight lookups of each
//! key a seat, a hundred lines a lookup. The window the player opens holds
//! `MAX_LOG_LINES` = 500.
//!
//! What a reader wants is what the *lookup* found, so that is what is kept here:
//! who the answers of one query named, counted once each, and said when the
//! query ends -- with how many answers there were and how many of them named
//! anybody, because *nobody answered* and *forty nodes answered and none knew a
//! player* are two different lobbies.
//!
//! Nothing here touches the swarm. `net::run` owns that; this is the part that
//! can be wrong in a test.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::time::Duration;

/// The most lookups tallied at once. Kademlia runs a handful; a book fed from
/// the network is bounded where it is filled all the same, and one that is full
/// forgets the lookups it holds rather than refuse the one that is running now.
pub const LOOKUPS_TALLIED_MAX: usize = 64;

/// The most names one lookup's tally holds: the lobby's own window (`D-055`).
/// Past it the count is said as *at least*.
pub const LOOKUP_NAMES_MAX: usize = 512;

/// What a lookup was asked for, which decides the words its line is said in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// The lobby's own key, or a slice of it (`S1-EX`).
    Lobby,
    /// The lobby's key of one hour (`D-070`).
    Hour,
    /// The relay key every libp2p client looks under.
    Relays,
}

#[derive(Debug)]
struct Tally<P> {
    asked: Asked,
    named: HashSet<P>,
    answers: u32,
    naming: u32,
    more: bool,
    /// `S1-IZ`: the peers the last answer named for the first time, until
    /// `progress` stamps where the lookup stood.
    fresh: Vec<P>,
    /// Nodes asked and time run when somebody other than this client was first
    /// named, and when the last such peer was named for the first time.
    first: Option<(u32, Duration)>,
    last_new: Option<(u32, Duration)>,
    /// Of those, the ones this client already knew as poker clients when they
    /// were named, and where the last of them was.
    players: u32,
    last_player: Option<(u32, Duration)>,
    /// The cap on how far a lookup walks finished it.
    stopped: bool,
}

/// What one finished lookup found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub asked: Asked,
    /// Distinct peers the answers named, this client included.
    pub named: usize,
    /// How many of them are not this client. A client's own store answers a
    /// lookup first and names the client itself (`D-070`).
    pub others: usize,
    /// How many DHT nodes answered, and how many of those named anybody.
    pub answers: u32,
    pub naming: u32,
    /// The tally was full: `named` is a floor.
    pub at_least: bool,
    /// `S1-IZ`: nodes asked and time run when somebody other than this client
    /// was first named, and when the last one was named for the first time.
    pub first: Option<(u32, Duration)>,
    pub last_new: Option<(u32, Duration)>,
    /// `S1-IZ`: how many of the peers named were poker clients this client
    /// already knew -- live players, where the rest may be records of clients
    /// long gone -- and where the last of them was named.
    pub players: u32,
    pub last_player: Option<(u32, Duration)>,
    /// `S1-IZ`: the cap on how far a lookup walks finished this one, so its
    /// closest are only the closest it reached.
    pub stopped: bool,
}

impl Found {
    /// The line. Its first words are the ones the per-answer line had, so that
    /// what drops or keeps a line by them (`app::worth_logging`) still does.
    pub fn words(&self) -> String {
        let n = if self.at_least { format!("at least {}", self.named) } else { self.named.to_string() };
        let answers = match (self.naming, self.answers) {
            (_, 0) => "no DHT node answered".to_owned(),
            (0, a) => format!("none of {a} answer(s) named any"),
            (k, a) => format!("{k} of {a} answer(s) named any"),
        };
        match self.asked {
            Asked::Lobby => {
                format!("{n} player(s) in the public lobby, {} of them not this client ({answers})", self.others)
            }
            Asked::Hour => format!(
                "{n} player(s) in the public lobby within the hour, {} of them not this client (D-070; {answers})",
                self.others
            ),
            Asked::Relays => format!("{n} relay(s) advertised in the DHT ({answers})"),
        }
    }

    /// `S1-IZ`: the lookup's appetite -- how far it walked in all, and where it
    /// stood when it found the last peer it found at all. What it asked after
    /// that is what a shorter lookup would have saved. Fixed words with the
    /// numbers beside them, for a script to read back.
    pub fn appetite(&self, asked_nodes: u32, running: Duration) -> String {
        let at = |x: Option<(u32, Duration)>| match x {
            Some((n, t)) => format!("{n} nodes {:.1} s", t.as_secs_f32()),
            None => "never".to_owned(),
        };
        format!(
            "appetite {}: {asked_nodes} nodes {:.1} s in all, {} other(s) named, first {}, last new {}, \
             {} known player(s) named, last of them {}{}",
            match self.asked {
                Asked::Lobby => "lobby",
                Asked::Hour => "hour",
                Asked::Relays => "relays",
            },
            running.as_secs_f32(),
            self.others,
            at(self.first),
            at(self.last_new),
            self.players,
            at(self.last_player),
            if self.stopped { ", stopped at the cap" } else { "" }
        )
    }
}

/// The lookups still running, and who their answers have named so far.
#[derive(Debug)]
pub struct Lookups<Q: Hash + Eq, P: Hash + Eq> {
    running: HashMap<Q, Tally<P>>,
}

impl<Q: Hash + Eq, P: Hash + Eq> Default for Lookups<Q, P> {
    fn default() -> Self {
        Lookups { running: HashMap::new() }
    }
}

impl<Q: Hash + Eq, P: Hash + Eq + Clone> Lookups<Q, P> {
    /// A lookup was started.
    pub fn asked(&mut self, id: Q, asked: Asked) {
        if self.running.len() >= LOOKUPS_TALLIED_MAX && !self.running.contains_key(&id) {
            self.running.clear();
        }
        self.running.insert(
            id,
            Tally {
                asked,
                named: HashSet::new(),
                answers: 0,
                naming: 0,
                more: false,
                fresh: Vec::new(),
                first: None,
                last_new: None,
                players: 0,
                last_player: None,
                stopped: false,
            },
        );
    }

    /// One DHT node's answer to it. An answer to a lookup this book does not
    /// hold -- forgotten when the book was full -- is not tallied, and that
    /// lookup says nothing when it ends: a number known to be short is not said
    /// as the lookup's.
    pub fn heard<'a>(&mut self, id: &Q, providers: impl IntoIterator<Item = &'a P>)
    where
        P: 'a,
    {
        let Some(t) = self.running.get_mut(id) else { return };
        t.answers = t.answers.saturating_add(1);
        t.fresh.clear();
        let mut any = false;
        for p in providers {
            any = true;
            if t.named.len() < LOOKUP_NAMES_MAX {
                if t.named.insert(p.clone()) {
                    t.fresh.push(p.clone());
                }
            } else if !t.named.contains(p) {
                t.more = true;
            }
        }
        if any {
            t.naming = t.naming.saturating_add(1);
        }
    }

    /// `S1-IZ`: what a running lookup was asked for, read without ending it --
    /// the dial book enters a finished query under its walk before the arm that
    /// says its line takes it out of here.
    pub fn asked_of(&self, id: &Q) -> Option<Asked> {
        self.running.get(id).map(|t| t.asked)
    }

    /// `S1-IZ`, a lookup's appetite: where the lookup stood -- how many nodes it
    /// had asked, how long it had run -- when the answer just [`heard`] was
    /// taken. Stamped only on an answer that named somebody new other than this
    /// client, whose own store answers first and names itself; and stamped
    /// again as a player's when one of those is a poker client this client
    /// already knows (`is_player`).
    ///
    /// [`heard`]: Lookups::heard
    pub fn progress(&mut self, id: &Q, me: &P, asked_nodes: u32, running: Duration, is_player: impl Fn(&P) -> bool) {
        let Some(t) = self.running.get_mut(id) else { return };
        let fresh = std::mem::take(&mut t.fresh);
        let at = (asked_nodes, running);
        let mut anybody = false;
        for p in fresh.iter().filter(|p| *p != me) {
            anybody = true;
            if is_player(p) {
                t.players = t.players.saturating_add(1);
                t.last_player = Some(at);
            }
        }
        if anybody {
            t.first.get_or_insert(at);
            t.last_new = Some(at);
        }
    }

    /// The lookup ended, however it ended: what it found, to be said once.
    pub fn ended(&mut self, id: &Q, me: &P) -> Option<Found> {
        let t = self.running.remove(id)?;
        Some(Found {
            asked: t.asked,
            named: t.named.len(),
            others: t.named.iter().filter(|p| *p != me).count(),
            answers: t.answers,
            naming: t.naming,
            at_least: t.more,
            first: t.first,
            last_new: t.last_new,
            players: t.players,
            last_player: t.last_player,
            stopped: t.stopped,
        })
    }

    /// `S1-IZ`: the cap on how far a lookup walks finished this one.
    pub fn stopped(&mut self, id: &Q) {
        if let Some(t) = self.running.get_mut(id) {
            t.stopped = true;
        }
    }

    /// How many lookups are being tallied.
    pub fn running(&self) -> usize {
        self.running.len()
    }

    /// `S1-IZ`: the lookups being tallied, for the cap on how far each walks.
    pub fn ids(&self) -> impl Iterator<Item = &Q> {
        self.running.keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The run the row was measured on, in small: one lookup, a dozen answers,
    /// most of them naming nobody -- one line, saying what the lookup found.
    ///
    /// The break that must make this fail: count names and not distinct peers
    /// (`named: t.named.len()` replaced by a sum of the answers' sizes).
    #[test]
    fn a_lookup_says_once_what_all_its_answers_named() {
        let mut l: Lookups<u32, &str> = Lookups::default();
        l.asked(7, Asked::Lobby);
        l.heard(&7, &["me"]);
        for _ in 0..9 {
            l.heard(&7, &[]);
        }
        l.heard(&7, &["ann", "bob", "me"]);
        l.heard(&7, &["bob", "cy"]);
        let found = l.ended(&7, &"me").expect("the lookup was asked");
        assert_eq!((found.named, found.others), (4, 3), "ann, bob, cy and this client: each once");
        assert_eq!((found.answers, found.naming), (12, 3));
        assert_eq!(found.words(), "4 player(s) in the public lobby, 3 of them not this client (3 of 12 answer(s) named any)");
        // Said once: the lookup is gone from the book.
        assert_eq!(l.ended(&7, &"me"), None);
        assert_eq!(l.running(), 0);
    }

    /// `S1-IZ`: a lookup's appetite -- where it stood when somebody else was
    /// first named, and when the last newcomer was; this client's own record,
    /// which its own store answers with first, and a name heard again stamp
    /// nothing.
    ///
    /// The break that must make this fail: stamp every answer that names
    /// anybody, which puts the last new peer at the lookup's end.
    #[test]
    fn a_lookup_says_where_it_stood_when_it_found_its_last_new_peer() {
        let s = Duration::from_secs;
        // Ann is a poker client this client knows; bob and old are records.
        let known = |p: &&str| *p == "ann";
        let mut l: Lookups<u32, &str> = Lookups::default();
        l.asked(7, Asked::Hour);
        // Its own store: this client itself, at once.
        l.heard(&7, &["me"]);
        l.progress(&7, &"me", 0, s(0), known);
        // A node that names nobody.
        l.heard(&7, &[]);
        l.progress(&7, &"me", 6, s(2), known);
        // The first other player, then a second, then a record nobody else
        // held, then only names heard before.
        l.heard(&7, &["ann"]);
        l.progress(&7, &"me", 14, s(4), known);
        l.heard(&7, &["ann", "bob", "me"]);
        l.progress(&7, &"me", 25, s(7), known);
        l.heard(&7, &["bob", "ann"]);
        l.progress(&7, &"me", 40, s(12), known);
        l.heard(&7, &["old", "ann"]);
        l.progress(&7, &"me", 90, s(30), known);
        l.heard(&7, &["bob", "ann"]);
        l.progress(&7, &"me", 160, s(58), known);
        let found = l.ended(&7, &"me").expect("asked");
        assert_eq!(found.first, Some((14, s(4))));
        assert_eq!(found.last_new, Some((90, s(30))));
        assert_eq!((found.players, found.last_player), (1, Some((14, s(4)))), "ann, once, where she was first named");
        assert_eq!(
            found.appetite(170, s(60)),
            "appetite hour: 170 nodes 60.0 s in all, 3 other(s) named, first 14 nodes 4.0 s, \
             last new 90 nodes 30.0 s, 1 known player(s) named, last of them 14 nodes 4.0 s"
        );

        // A lookup that found nobody else says so.
        l.asked(8, Asked::Lobby);
        l.heard(&8, &["me"]);
        l.progress(&8, &"me", 0, s(0), known);
        let alone = l.ended(&8, &"me").expect("asked");
        assert_eq!(
            alone.appetite(200, s(60)),
            "appetite lobby: 200 nodes 60.0 s in all, 0 other(s) named, first never, last new never, \
             0 known player(s) named, last of them never"
        );
    }

    /// `S1-IZ`: the book says which lookups it holds, so the cap on how far
    /// each walks reads every one that runs and none that has ended; and a
    /// lookup the cap finished says so when it ends, and only that one.
    ///
    /// The breaks that must make this fail: `ids` that yields nothing, which
    /// lets every walk run to its natural end; a stop that marks nothing.
    #[test]
    fn the_book_says_which_lookups_run_and_which_the_cap_stopped() {
        let mut l: Lookups<u32, &str> = Lookups::default();
        l.asked(1, Asked::Lobby);
        l.asked(2, Asked::Hour);
        l.asked(3, Asked::Relays);
        let _ = l.ended(&2, &"me");
        let mut ids: Vec<u32> = l.ids().copied().collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![1, 3]);

        l.stopped(&1);
        l.stopped(&2); // ended already: nothing to mark
        let one = l.ended(&1, &"me").expect("asked");
        assert!(one.stopped);
        assert!(one.appetite(80, Duration::from_secs(3)).ends_with(", stopped at the cap"), "{}", one.appetite(80, Duration::from_secs(3)));
        let three = l.ended(&3, &"me").expect("asked");
        assert!(!three.stopped);
        assert!(!three.appetite(200, Duration::from_secs(15)).contains("stopped"));
    }

    /// Two lookups run at once -- the lobby's key and an hour's are asked in
    /// the same breath -- and an answer belongs to the question it answers.
    #[test]
    fn an_answer_is_tallied_under_the_lookup_it_answers() {
        let mut l: Lookups<u32, &str> = Lookups::default();
        l.asked(1, Asked::Lobby);
        l.asked(2, Asked::Hour);
        l.asked(3, Asked::Relays);
        l.heard(&1, &["old", "older", "me"]);
        l.heard(&2, &["me"]);
        l.heard(&3, &["r1", "r2"]);
        l.heard(&2, &[]);
        assert_eq!(
            l.ended(&2, &"me").unwrap().words(),
            "1 player(s) in the public lobby within the hour, 0 of them not this client (D-070; 1 of 2 answer(s) named any)"
        );
        assert_eq!(l.ended(&3, &"me").unwrap().words(), "2 relay(s) advertised in the DHT (1 of 1 answer(s) named any)");
        assert_eq!(l.ended(&1, &"me").unwrap().others, 2);
    }

    /// *Nobody answered* and *everybody answered and nobody knew a player* are
    /// two lobbies, and the line tells them apart.
    #[test]
    fn no_answer_and_no_player_are_told_apart() {
        let mut l: Lookups<u32, &str> = Lookups::default();
        l.asked(1, Asked::Hour);
        assert_eq!(
            l.ended(&1, &"me").unwrap().words(),
            "0 player(s) in the public lobby within the hour, 0 of them not this client (D-070; no DHT node answered)"
        );
        l.asked(2, Asked::Hour);
        for _ in 0..40 {
            l.heard(&2, &[]);
        }
        assert_eq!(
            l.ended(&2, &"me").unwrap().words(),
            "0 player(s) in the public lobby within the hour, 0 of them not this client (D-070; none of 40 answer(s) named any)"
        );
    }

    /// The lines keep the words the log's filter knows them by
    /// (`app::worth_logging`): the lobby's and the relays' are dropped from the
    /// file, the hour's is kept by its decision's number.
    #[test]
    fn the_lines_are_filed_as_the_lines_they_replace_were() {
        let found = |asked| Found {
            asked,
            named: 3,
            others: 2,
            answers: 9,
            naming: 2,
            at_least: false,
            first: None,
            last_new: None,
            players: 0,
            last_player: None,
            stopped: false,
        };
        assert!(!crate::app::worth_logging(&found(Asked::Lobby).words()));
        assert!(!crate::app::worth_logging(&found(Asked::Relays).words()));
        assert!(crate::app::worth_logging(&found(Asked::Hour).words()));
    }

    /// Both books are bounded, because both are filled from an open DHT.
    #[test]
    fn the_tallies_are_bounded() {
        let mut l: Lookups<usize, usize> = Lookups::default();
        l.asked(0, Asked::Lobby);
        let crowd: Vec<usize> = (1..=LOOKUP_NAMES_MAX + 40).collect();
        l.heard(&0, &crowd);
        let found = l.ended(&0, &0).unwrap();
        assert_eq!(found.named, LOOKUP_NAMES_MAX);
        assert!(found.at_least);
        assert!(found.words().starts_with(&format!("at least {LOOKUP_NAMES_MAX} player(s)")), "{}", found.words());

        l.asked(1, Asked::Lobby);
        let few: Vec<usize> = (1..=LOOKUP_NAMES_MAX).collect();
        l.heard(&1, &few);
        // The same names again are nobody new, and do not make the count a
        // floor: only a name that did not fit does.
        l.heard(&1, &few);
        assert!(!l.ended(&1, &0).unwrap().at_least, "a full tally hearing the same names again is still exact");

        for id in 0..LOOKUPS_TALLIED_MAX * 3 {
            l.asked(id, Asked::Relays);
            assert!(l.running() <= LOOKUPS_TALLIED_MAX);
        }
        // The lookup asked last is always held, whatever was forgotten for it.
        assert!(l.ended(&(LOOKUPS_TALLIED_MAX * 3 - 1), &0).is_some());
    }

    /// An answer to a lookup the book does not hold -- one forgotten when the
    /// book was full -- is nobody's: it is tallied under no other lookup, and the
    /// lookup it answers says nothing when it ends.
    ///
    /// The break that must make this fail: tally it under whatever is held.
    #[test]
    fn an_answer_to_a_lookup_not_held_is_nobodys() {
        let mut l: Lookups<u32, &str> = Lookups::default();
        l.asked(1, Asked::Hour);
        l.heard(&2, &["ann", "bob"]);
        assert_eq!(l.ended(&2, &"me"), None, "a lookup the book does not hold says nothing");
        let held = l.ended(&1, &"me").expect("the one that was asked");
        assert_eq!((held.answers, held.naming, held.named), (0, 0, 0), "and its answer went under no other");
    }
}
