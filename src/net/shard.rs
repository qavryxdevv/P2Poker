//! `S1-EX`: which slice of the lobby this client listens to.
//!
//! `DECISIONS.md` D-055 §6 is the design and the arithmetic; this is the part
//! of it that can be written as arithmetic. Nothing here talks to the network.
//!
//! # Why a lobby has slices at all
//!
//! Every subscriber of a GossipSub topic receives every message published to
//! it, so one global lobby topic makes a client's cost the **network's** size
//! rather than its own: at a thousand tables it relays 93 KB/s and at twenty
//! thousand 1.9 MB/s, whatever it is looking at. Slicing the topic by the
//! table's own key gives each client a share it chooses.
//!
//! # The shape of it
//!
//! A table belongs to the slice named by the leading bits of its key, and a
//! key is a public key, so the slices are even and nobody can choose theirs.
//! The **depth** is how many of those bits name the slice: none (the whole
//! lobby, which is what every client of every earlier build listens to), then
//! four, eight, twelve, sixteen. A founder says its advert at **every** depth,
//! which is five publications every thirty seconds and costs nothing where
//! nobody is listening -- with no mesh under a topic there is nobody to send
//! to. So no two clients have to agree on a depth, and a client that has never
//! heard of slices keeps working.

/// The depths a lobby topic may be sliced to, in bits of the table key.
///
/// Four bits a step: each one divides the traffic by sixteen, which is coarse
/// enough that a client changes depth rarely and fine enough to follow a
/// network growing by orders of magnitude. Sixteen bits is 65 536 slices --
/// thirty tables each at two million tables -- and the ladder ends there
/// because a slice that small is already below what one client wants to hold.
pub const DEPTHS: [u8; 5] = [0, 4, 8, 12, 16];

/// How many slices this client listens to at once.
///
/// **Bounded by connections, not by bandwidth** (D-055 §6). A topic's mesh is
/// built only out of peers this client is *connected to* that also subscribe to
/// that topic, so each slice wants ten or twelve of the eighty connections this
/// client keeps -- and the rest are owed to the table being played at, the
/// relays and the DHT.
pub const SLICES: usize = 4;

/// The slice of the lobby a table belongs to at `depth`, as the hexadecimal
/// prefix that names the topic.
///
/// Empty at depth zero, which is the whole lobby.
pub fn slice_of(table_key: &[u8; 32], depth: u8) -> String {
    let chars = usize::from(depth) / 4;
    table_key
        .iter()
        .flat_map(|b| [b >> 4, b & 0x0f])
        .take(chars)
        .map(|n| char::from_digit(u32::from(n), 16).unwrap_or('0'))
        .collect()
}

/// The topic a slice is said on: the lobby's own name at depth zero, and that
/// name with the slice under it below.
///
/// **The lobby's version is not bumped.** Depth zero *is* the topic every
/// earlier build subscribes to, so a client that knows nothing of slices hears
/// every table as it always did, and one that does hears the same tables on a
/// narrower topic. There is nothing to migrate and no flag day.
pub fn topic_of(slice: &str) -> String {
    if slice.is_empty() {
        crate::protocol::constants::LOBBY_TOPIC.to_owned()
    } else {
        format!("{}/{slice}", crate::protocol::constants::LOBBY_TOPIC)
    }
}

/// Every topic a founder says its advert on: one per depth.
pub fn topics_for(table_key: &[u8; 32]) -> Vec<String> {
    DEPTHS.iter().map(|d| topic_of(&slice_of(table_key, *d))).collect()
}

/// `D-064`: the queue topic of a slice -- the whole queue at depth zero, the
/// slice's own below it, named exactly as the lobby's is.
pub fn queue_topic_of(slice: &str) -> String {
    if slice.is_empty() {
        crate::protocol::constants::SEARCH_QUEUE_TOPIC.to_owned()
    } else {
        format!("{}/{slice}", crate::protocol::constants::SEARCH_QUEUE_TOPIC)
    }
}

/// `D-064`: every queue topic a searcher says its presence on: one per depth,
/// the slice named by its own application key, so a listener at any depth
/// hears it on the slice it holds.
pub fn queue_topics_for(app_key: &[u8; 32]) -> Vec<String> {
    DEPTHS.iter().map(|d| queue_topic_of(&slice_of(app_key, *d))).collect()
}

/// What this client currently listens to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listening {
    /// How many bits of the key name a slice.
    pub depth: u8,
    /// The slices, as their hexadecimal prefixes. Exactly one -- the empty one
    /// -- at depth zero.
    pub slices: Vec<String>,
}

impl Default for Listening {
    /// The whole lobby, which is what every earlier build does and what a
    /// network small enough never needs to leave.
    fn default() -> Self {
        Listening { depth: 0, slices: vec![String::new()] }
    }
}

impl Listening {
    /// The topics to subscribe to.
    pub fn topics(&self) -> Vec<String> {
        self.slices.iter().map(|s| topic_of(s)).collect()
    }

    /// Whether a topic name is one of this client's lobby topics.
    pub fn holds(&self, topic: &str) -> bool {
        self.topics().iter().any(|t| t == topic)
    }

    /// `D-064`: the queue topics beside the lobby's: the same slices.
    pub fn queue_topics(&self) -> Vec<String> {
        self.slices.iter().map(|s| queue_topic_of(s)).collect()
    }

    /// Whether a topic name is one of this client's queue topics.
    pub fn holds_queue(&self, topic: &str) -> bool {
        self.queue_topics().iter().any(|t| t == topic)
    }
}

/// How many tables the network holds, read off what this client hears.
///
/// Every table advertises once every `AD_REBROADCAST_MS`, so a client that
/// hears `rate` adverts a second on `slices` slices of `depth` is hearing a
/// `slices / 2^depth` share of a network of about
/// `rate * 30 / (slices / 2^depth)` tables. It is an estimate off a sample and
/// it is used only to choose a depth, which moves in steps of sixteen.
pub fn tables_out_there(rate_per_s: f64, at: &Listening) -> f64 {
    let share = at.slices.len() as f64 / f64::from(1u32 << at.depth);
    if share <= 0.0 {
        return 0.0;
    }
    rate_per_s * (crate::protocol::constants::AD_REBROADCAST_MS as f64 / 1000.0) / share
}

/// `S1-EX`: the depth this client should listen at, given what it hears.
///
/// `rate_per_s` is the adverts a second arriving on the slices it holds now,
/// `budget_per_s` what it is willing to take (D-055's cost ceiling) and `rows`
/// the size of its lobby window.
///
/// **Hysteresis on both sides**, because the two directions must not chase each
/// other: deeper when the rate is over budget, shallower only when the network
/// would leave the window less than half full at the shallower depth. A client
/// sitting between the two stays where it is.
pub fn depth_for(rate_per_s: f64, budget_per_s: f64, rows: usize, at: &Listening) -> u8 {
    let tables = tables_out_there(rate_per_s, at);
    let here = DEPTHS.iter().position(|d| *d == at.depth).unwrap_or(0);
    // Over budget: one step deeper, which divides what arrives by sixteen.
    if rate_per_s > budget_per_s && here + 1 < DEPTHS.len() {
        return DEPTHS[here + 1];
    }
    // Under-full: one step shallower, but only if the whole network at that
    // depth would still be inside the budget -- otherwise this is the client
    // that has just come down and would go straight back up.
    if here > 0 {
        let up = DEPTHS[here - 1];
        let share = SLICES as f64 / f64::from(1u32 << up);
        let would_hold = tables * share;
        let would_arrive = would_hold / (crate::protocol::constants::AD_REBROADCAST_MS as f64 / 1000.0);
        if would_hold < rows as f64 / 2.0 && would_arrive <= budget_per_s {
            return up;
        }
    }
    at.depth
}

/// `S1-EX`: the slices to listen to at `depth`, keeping what is already held
/// where it can and drawing the rest from `pick`.
///
/// `pick` supplies arbitrary numbers -- a client's slices must **not** be
/// derived from its own key, which would pin a player to one slice of the
/// network for ever and hide the rest from them.
pub fn slices_at(depth: u8, held: &[String], want: usize, mut pick: impl FnMut() -> u16) -> Vec<String> {
    if depth == 0 {
        return vec![String::new()];
    }
    let chars = usize::from(depth) / 4;
    let mut out: Vec<String> = held
        .iter()
        .filter(|s| s.len() == chars)
        .take(want)
        .cloned()
        .collect();
    // A slice is `depth` bits, and `pick` gives sixteen of them at a time.
    let mut guard = 0;
    while out.len() < want && guard < want * 64 {
        guard += 1;
        let mut s = String::with_capacity(chars);
        while s.len() < chars {
            let n = pick();
            for shift in (0..16).step_by(4) {
                if s.len() == chars {
                    break;
                }
                let nibble = (n >> shift) & 0x0f;
                s.push(char::from_digit(u32::from(nibble), 16).unwrap_or('0'));
            }
        }
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(first: u8, second: u8) -> [u8; 32] {
        let mut k = [0u8; 32];
        k[0] = first;
        k[1] = second;
        k
    }

    /// A table's slice is the leading bits of its own key, and a key is a
    /// public key: the slices are even and no founder can choose one.
    #[test]
    fn a_table_belongs_to_the_slice_its_key_names() {
        let k = key(0xab, 0xcd);
        assert_eq!(slice_of(&k, 0), "");
        assert_eq!(slice_of(&k, 4), "a");
        assert_eq!(slice_of(&k, 8), "ab");
        assert_eq!(slice_of(&k, 12), "abc");
        assert_eq!(slice_of(&k, 16), "abcd");
    }

    /// Depth zero **is** the lobby topic every earlier build subscribes to, so
    /// a client that knows nothing of slices hears every table as it always
    /// did. There is nothing to migrate.
    #[test]
    fn the_whole_lobby_is_the_topic_it_always_was() {
        use crate::protocol::constants::LOBBY_TOPIC;
        assert_eq!(topic_of(""), LOBBY_TOPIC);
        assert_eq!(topic_of("ab"), format!("{LOBBY_TOPIC}/ab"));
        assert_eq!(Listening::default().topics(), vec![LOBBY_TOPIC.to_owned()]);

        // And a founder says it on every depth, the whole lobby included, so
        // nobody has to agree with anybody about which depth is in use.
        let said = topics_for(&key(0xab, 0xcd));
        assert_eq!(said.len(), DEPTHS.len());
        assert_eq!(said[0], LOBBY_TOPIC);
        assert_eq!(said[4], format!("{LOBBY_TOPIC}/abcd"));
    }

    /// The size of the network is read off what arrives: a table advertises
    /// once every thirty seconds, so the rate on a known share of the lobby is
    /// the whole of it.
    #[test]
    fn the_network_is_measured_and_not_guessed() {
        // The whole lobby, 12 adverts a second: 360 tables.
        let all = Listening::default();
        assert!((tables_out_there(12.0, &all) - 360.0).abs() < 1.0);

        // Four slices of 256 at eight bits, 6 adverts a second. The share is
        // 4/256, so the network is 6 * 30 * 64 = 11 520 tables.
        let deep = Listening { depth: 8, slices: vec!["00".into(), "01".into(), "02".into(), "03".into()] };
        assert!((tables_out_there(6.0, &deep) - 11_520.0).abs() < 1.0);
    }

    /// Over the budget, a step deeper; comfortably under it with a window that
    /// will not fill, a step shallower; and in between, stay put -- the two
    /// directions must not chase each other.
    #[test]
    fn the_depth_follows_what_arrives_and_does_not_oscillate() {
        let budget = 12.0;
        let rows = 512;

        // A small network: nothing moves, and depth zero is where it stays.
        let all = Listening::default();
        assert_eq!(depth_for(2.0, budget, rows, &all), 0, "a quiet lobby never slices");

        // A hundred thousand tables on the whole lobby is 3 333 a second.
        assert_eq!(depth_for(3_333.0, budget, rows, &all), 4, "over budget: deeper");

        // Four slices of sixteen is a quarter of that network: 833 a second,
        // still far over.
        let d4 = Listening { depth: 4, slices: vec!["0".into(), "1".into(), "2".into(), "3".into()] };
        assert_eq!(depth_for(833.0, budget, rows, &d4), 8);

        // At twelve bits it is 3 a second, inside the budget, and the window is
        // full -- so it stays.
        let d12 = Listening {
            depth: 12,
            slices: vec!["000".into(), "111".into(), "222".into(), "333".into()],
        };
        assert_eq!(depth_for(3.0, budget, rows, &d12), 12, "inside the budget: stay");

        // The network shrank to a few hundred tables: at twelve bits this
        // client now hears almost nothing and its window is empty, and the
        // shallower depth would hold all of it inside the budget.
        assert_eq!(depth_for(0.02, budget, rows, &d12), 8, "under-full: shallower");
    }

    /// A client's slices are arbitrary and not its own key's: deriving them
    /// would pin a player to one slice of the network for ever.
    #[test]
    fn the_slices_are_arbitrary_kept_where_they_can_be_and_never_repeated() {
        let mut n = 0u16;
        let mut pick = || {
            n = n.wrapping_add(0x1111);
            n
        };
        let first = slices_at(8, &[], SLICES, &mut pick);
        assert_eq!(first.len(), SLICES);
        assert!(first.iter().all(|s| s.len() == 2), "two hex characters at eight bits");
        let mut sorted = first.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), SLICES, "no slice twice");

        // Asked again, what is already held is kept -- a client must not throw
        // its whole window away to add one slice.
        let again = slices_at(8, &first, SLICES, &mut pick);
        assert_eq!(again, first);

        // A deeper depth cannot keep them: they are the wrong length.
        let deeper = slices_at(12, &first, SLICES, &mut pick);
        assert!(deeper.iter().all(|s| s.len() == 3));

        // And depth zero is the whole lobby, one topic.
        assert_eq!(slices_at(0, &first, SLICES, &mut pick), vec![String::new()]);
    }

    /// The slice a table is in is the slice a listener computes for it, at
    /// every depth: that is the whole of the agreement between them.
    #[test]
    fn a_listener_computes_the_same_slice_the_founder_published_to() {
        for seed in 0..64u8 {
            let k = key(seed, seed.wrapping_mul(7));
            let said = topics_for(&k);
            for (i, d) in DEPTHS.iter().enumerate() {
                let listening = Listening { depth: *d, slices: vec![slice_of(&k, *d)] };
                assert!(listening.holds(&said[i]), "depth {d}, key {seed}");
            }
        }
    }
}
