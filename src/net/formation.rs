//! Forming a table: the dance, as a state machine with no network in it.
//!
//! Every rule was written and tested in [`table::join`](crate::table::join) and
//! [`table::formation`](crate::table::formation); every message got a wire form
//! in [`joinwire`]. What was missing between them is the part
//! that decides **what to send next**, and that is here.
//!
//! # There is no libp2p in this file, and that is the point
//!
//! Every method takes bytes and returns bytes. A whole table can therefore be
//! formed inside one test — three clients, a founder and two joiners, passing
//! `Vec<u8>` to each other — and the test at the bottom does exactly that, from
//! the first `JOIN_REQUEST` to a `session_id` all three agree on. Wiring it to a
//! swarm afterwards moves bytes and decides nothing.
//!
//! # One type for both roles
//!
//! The founder is a player too. It holds a second key — the **table key**, which
//! is the table's whole identity — and it answers join requests; in every other
//! respect it runs the same code as anybody else, including ratifying the roster
//! at the end. Two types would have been two implementations of the checks that
//! matter most, and the founder's own copy is the one nobody else can verify.
//!
//! # Where the founder's authority ends
//!
//! At `TABLE_READY`, exactly as §4.3 says. Up to that point the founder decides
//! who sits down; from that point the roster is unanimous and the founder is one
//! signature among `n`. A `PLAYER_LIST` is a **proposal** and this file treats
//! it as one: it is checked against the parameters this client joined under
//! every single time, and a founder that changes them is left rather than
//! followed.

use std::collections::{BTreeMap, VecDeque};

use ed25519_dalek::SigningKey;

use super::joinwire::{self, WireError};
use super::lobby::TableAd;
use super::seating::{Owed, Seating, SeatingRefused};
use crate::poker::state::Hash;
use crate::protocol::constants::{
    LIST_MAX_AGE_MS, MAX_AD_LIFETIME_MS, MAX_CLOCK_SKEW_MS, MAX_SEATS,
};
use crate::protocol::transcript::{genesis_setup, session_id, Ratification};
use crate::table::formation::{password_proof, Roster, SeatEntry};
use crate::table::join::{
    admit_accept, admit_join, admit_list, admit_ready, AcceptRefused, JoinAccept, JoinRefused,
    JoinRequest, JoinedUnder, ListRefused, PlayerList, ReadyRefused, RejectReason, TableReady,
};

/// The capability every seat must declare (§4.3).
pub const DECK_CAPABILITY: &[u8] = b"deck/bs-bg12-secp256k1/1";

/// What this client should put on the wire as a result of a step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Send {
    /// The answer to the join RPC that produced this step.
    Reply(Vec<u8>),
    /// To every seat of the table.
    Broadcast(Vec<u8>),
}

/// Why a step did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failed {
    Wire(WireError),
    Join(JoinRefused),
    Accept(AcceptRefused),
    List(ListRefused),
    Ready(ReadyRefused),
    /// The founder refused, and this is the reason it claimed. **Advisory**: a
    /// founder may lie, so this is never proof of anything.
    Refused { reason: u16, retry_after_ms: u32 },
    /// A message that only a founder answers arrived at a client that is not one.
    NotTheFounder,
    /// A message arrived out of order — an acceptance with nothing outstanding,
    /// a ratification before any roster.
    OutOfOrder(&'static str),
    /// One seat, two different ratifications at one `list_serial`.
    ///
    /// Formation **is** a collective stage — a fixed set of seats each emitting
    /// once — and every other collective stage in this client answers a second
    /// differing copy with first-copy-wins and a named seat. This one kept a
    /// map and overwrote, so which of the two a peer ended up holding was
    /// decided by arrival order, and `session_id` is computed from those
    /// hashes and goes into **every subsequent hand's genesis**. Two honest
    /// peers, two orders, two tables that can never speak.
    ///
    /// The second copy needs no forged signature: the `event_hash` covers the
    /// envelope, so one seat re-signing the same body a millisecond later
    /// produces a different hash.
    RatifiedTwice { seat: u8 },
    /// `D-083`: a roster this client will not take, for what it does to the
    /// seating draw.
    Seating(SeatingRefused),
}

impl From<WireError> for Failed {
    fn from(e: WireError) -> Self {
        Failed::Wire(e)
    }
}

/// One advertisement this founder has signed for this table.
struct Issued {
    hash: Hash,
    /// The complete `SignedEvent`, echoed verbatim into an acceptance so the
    /// joiner re-verifies the table key's own signature rather than trusting a
    /// gossip copy or the founder's word.
    event: Vec<u8>,
    at_ms: u64,
}

/// The founder's extra half.
struct FounderPart {
    /// The table's identity. Nothing else is.
    key: SigningKey,
    /// **Every** advertisement this founder has signed for this table that could
    /// still be held somewhere, newest last.
    ///
    /// Not just the current one, and the reason is the defect the first
    /// two-instance run found. §7.2 obliges a re-broadcast every thirty seconds
    /// with a strictly greater timestamp, so every copy has a different
    /// `event_hash`; §4.3's receiver rule is that the hash names an advert this
    /// founder **actually signed**, not the newest one. The first version
    /// compared against the newest, so an honest joiner naming the copy it had
    /// heard was refused with *"the advertisement has expired"* — which was both
    /// wrong and unfixable from the joiner's side.
    ///
    /// Bounded by the advertisement's own lifetime rather than by a count:
    /// §7.2 puts `MAX_AD_LIFETIME_MS` on how long any receiver may still hold
    /// one, so anything older is a hash nobody can legitimately name.
    issued: VecDeque<Issued>,
    /// The table password, if it has one.
    password: Option<Vec<u8>>,
}

impl FounderPart {
    fn remember(&mut self, event: Vec<u8>, hash: Hash, now_ms: u64) {
        self.issued
            .retain(|i| now_ms.saturating_sub(i.at_ms) <= MAX_AD_LIFETIME_MS);
        self.issued.push_back(Issued {
            hash,
            event,
            at_ms: now_ms,
        });
    }

    fn find(&self, hash: &Hash) -> Option<&Issued> {
        self.issued.iter().find(|i| &i.hash == hash)
    }
}

/// One client's view of a table being formed.
pub struct Formation {
    /// This client's own application key. Signs `JOIN_REQUEST` and
    /// `TABLE_READY`, and never a `PLAYER_LIST`.
    app: SigningKey,
    founder: Option<FounderPart>,
    /// The advertisement everything is checked against, pinned at the moment
    /// this client committed to it — `U11`'s disposition.
    under: JoinedUnder,
    /// Who is seated, as this client believes.
    roster: Roster,
    serial: u64,
    my_seat: Option<u8>,
    my_buyin: u64,
    /// The hash of the join request this client is waiting on.
    pending: Option<Hash>,
    /// Ratifications collected, by seat. Cleared whenever the roster changes,
    /// because a ratification names the serial it ratifies.
    ratified: BTreeMap<u8, Hash>,
    /// **The ratifications themselves, so any seat can repair any other's.**
    ///
    /// `ratified` holds hashes, which is all `session_id` needs and all the
    /// first-copy-wins test needs. It is not enough to *re-send* one, and that
    /// was `S1-P`'s residue: `say_again` could repeat only this client's own
    /// copy, so a seat that missed seat 3's ratification could be helped by
    /// seat 3 alone — and if seat 3 had already settled and stopped saying
    /// anything, by nobody.
    ///
    /// **Relaying a third party's signed event is sound and the corpus already
    /// does it.** §9's join answer carries `advert_event` *"repeated verbatim so
    /// the joiner is not relying on a gossip copy and can re-verify the table
    /// key's signature itself"*. The same holds here: the bytes are signed by
    /// the seat that made them, every receiver verifies that signature, and a
    /// relayer that altered a byte would produce something no signature covers.
    /// Carrying it proves nothing about the relayer and asserts nothing on the
    /// author's behalf.
    ///
    /// Idempotent at the far end by construction: `take_ratification` returns
    /// `Ok(())` on a byte-identical repeat and only a **differing** copy is
    /// `RatifiedTwice`.
    ///
    /// Cost is one `TABLE_READY` per seat — about two hundred bytes each — held
    /// only while the table has no session.
    ///
    /// **Cleared with `ratified`, whenever the roster changes** (`D-083`). Kept
    /// across rosters, a seat's entry outlived the player in it: once the seating
    /// draw moved players, a client drawn to a seat whose old ratification it
    /// held said that -- another player's word on an older roster -- as its own,
    /// and its table never set.
    ratified_bytes: BTreeMap<u8, Vec<u8>>,
    sent_ready: bool,
    session: Option<Hash>,
    capabilities: Vec<Vec<u8>>,
    /// The last `PLAYER_LIST` this founder signed, and this client's own
    /// `TABLE_READY`, kept so they can be **said again**.
    ///
    /// GossipSub delivers to the peers on a topic at the moment of publishing
    /// and to nobody else. A founder that answers a join and announces the new
    /// roster in the same breath announces it to an empty topic — the joiner
    /// subscribed a moment ago and the mesh has not formed — so the message is
    /// not lost in transit, it is never sent. The first two-instance run seated
    /// the joiner and then sat there: both sides agreed on two seats and neither
    /// ever saw the other's ratification.
    ///
    /// Formation is short and these are two small messages. Repeating them
    /// whenever somebody new appears on the topic costs nothing and removes a
    /// class of silent stall that no amount of waiting fixes.
    said: Said,
    /// Ratifications that arrived before the roster they ratify.
    ///
    /// A GossipSub mesh does not order two messages against each other, and the
    /// founder sends the roster and its own ratification of it in the same
    /// breath. The second overtaking the first is not unusual — it happened on
    /// the first two-instance run — and the joiner then refused an honest
    /// ratification with *"too few to start: 0 seated"*, having no roster yet,
    /// and both sides sat waiting for each other for ever.
    ///
    /// A `TABLE_READY` is signed and names the serial it ratifies, so holding
    /// one costs nothing and gives up nothing: it is checked in full when the
    /// list it belongs to arrives, and if it was never honest it fails then.
    ///
    /// Bounded by the number of seats a table can have. Formation is short and
    /// there is exactly one ratification per seat per serial.
    early: VecDeque<Vec<u8>>,
    /// `S1-CR`: the ratification this client made before it restarted, from
    /// its session record, to be said again **verbatim** when the founder's
    /// re-said roster fits it. A ratification's `event_hash` is inside the
    /// `session_id`, so a seat that ratified anew after a restart -- a new
    /// timestamp, a new hash -- computed a session nobody else had
    /// (`run202634-3`: `ed73f924` against the table's `9718345e`), and every
    /// deck key of the hand it adopted was refused, the key's ownership
    /// proof being bound to the session id.
    recorded_ready: Option<Vec<u8>>,
    /// The recorded ratification did not fit the roster the founder said
    /// again, and this client ratified anew; the node says so.
    recorded_refused: bool,
    /// `S1-DV`: a roster the founder said again no longer names this
    /// client, which an earlier one did -- the seat was given back before
    /// the first hand (silent for `SEAT_SILENCE_MS`, or left and came
    /// back). The node reads it and leaves the table: there is nothing to
    /// sit at, and the player joins again from the lobby.
    released: bool,
    /// `S1-GB`: the newest serial of a `PLAYER_LIST` that named this client. Only
    /// a list newer than that one gives a seat back: an acceptance carries no
    /// serial, and the lists of the seconds before this client sat down, carried
    /// late, are newer than nothing and name it nowhere.
    named_serial: Option<u64>,
    /// `D-044`: a roster of at least `min_players_to_start` was adopted here
    /// once -- the table was set to start. From then on a roster the founder
    /// says again may be smaller, down to two seats (the owner's floor:
    /// heads-up), and is ratified like any other: a seat given back before
    /// the first hand does not un-set the table.
    started: bool,
    /// `D-060`: this client's own ratification waits for its node's word that it
    /// hears every seat of the roster (`ratify_now`); `false`, a roster is
    /// ratified the moment it is adopted, as before.
    hold_ready: bool,
    /// `S1-GI`: this client's seat came from its founder's own `JOIN_ACCEPT`,
    /// which a founder gives a stranger only before its table deals -- not from
    /// a roster said to a seat already on it, which a founder says at a table in
    /// play too.
    admitted: bool,
    /// `D-083`: this client's part in the table's seating draw.
    seating: Seating,
    /// `D-083`: when this client last said its lot or its opening, so the node's
    /// tick says it again every few seconds and not every tick.
    lot_said_ms: u64,
    /// `D-083`: when each member last gave the draw a word this client kept -- a
    /// lot of the round, newer than the one held, or its opening. The founder's
    /// judgement gives back only a member that has been silent to the draw for
    /// its whole grace, so a member keeping up with rounds begun again by others'
    /// joins and leaves is never given back for the latest one's lot.
    draw_word_ms: BTreeMap<[u8; 32], u64>,
}

/// `D-083`: how often a member says its sealed lot or its opening again while
/// the round waits on it -- signed afresh each time, since GossipSub refuses a
/// verbatim repeat for two minutes.
pub const LOT_EVERY_MS: u64 = 5_000;

/// fault-harness, `-NoLotNodes` / `-NoOpeningNodes` (`D-083`'s bed): a member
/// that stays at the table and gives the seating draw nothing -- never its
/// sealed lot (`P2P_POKER_NO_LOT=lot`), or never its opening (`=opening`). A
/// build without the feature never reads the variable.
pub fn withheld_for_harness() -> Option<Owed> {
    if !cfg!(feature = "fault-harness") {
        return None;
    }
    static WHAT: std::sync::OnceLock<Option<Owed>> = std::sync::OnceLock::new();
    *WHAT.get_or_init(|| match std::env::var("P2P_POKER_NO_LOT").ok().as_deref().map(str::trim) {
        Some("lot") => Some(Owed::Lot),
        Some("opening") => Some(Owed::Opening),
        _ => None,
    })
}

/// What this client may need to say again.
#[derive(Debug, Clone, Default)]
pub struct Said {
    pub list: Option<Vec<u8>>,
    pub ready: Option<Vec<u8>>,
}

impl Formation {
    /// Found a table. This client holds the table key and seat 0.
    #[allow(clippy::too_many_arguments)]
    pub fn found(
        app: SigningKey,
        table_key: SigningKey,
        ad: TableAd,
        advert_event: Vec<u8>,
        advert_hash: Hash,
        password: Option<Vec<u8>>,
        my_peer_id: Vec<u8>,
        my_name: String,
        my_buyin: u64,
    ) -> Result<Self, Failed> {
        let table_id = table_key.verifying_key().to_bytes();
        let under = JoinedUnder::pin(ad, advert_hash, table_id);

        let me = SeatEntry {
            seat: 0,
            app_public_key: app.verifying_key().to_bytes(),
            peer_id: my_peer_id.clone(),
            display_name: my_name.clone(),
            buyin: my_buyin,
            // `D-041`: the founder's own Tox key rides in its roster entry, as
            // every joiner's does in theirs -- it was `None`, so at every other
            // seat the founder had no friend link to read and was drawn as not on
            // the line until its first frame taught the group key.
            tox_key: under.ad.founder_tox_key,
        };
        // Through the same gate as anybody else's seat. A founder that seated
        // itself outside the rules would be the one entry no joiner could have
        // refused.
        let roster =
            Roster::form(vec![me], &under.ad, false).map_err(|e| Failed::List(ListRefused::Roster(e)))?;

        let mut part = FounderPart {
            key: table_key,
            issued: VecDeque::new(),
            password,
        };
        part.remember(advert_event, advert_hash, under.ad.timestamp_unix_ms);
        let seating = Seating::new(table_id, app.verifying_key().to_bytes(), app.verifying_key().to_bytes());

        Ok(Formation {
            app,
            founder: Some(part),
            under,
            roster,
            serial: 0,
            my_seat: Some(0),
            my_buyin,
            pending: None,
            ratified: BTreeMap::new(),
            ratified_bytes: BTreeMap::new(),
            sent_ready: false,
            session: None,
            capabilities: vec![DECK_CAPABILITY.to_vec()],
            said: Said::default(),
            early: VecDeque::new(),
            recorded_ready: None,
            recorded_refused: false,
            released: false,
            named_serial: None,
            started: false,
            hold_ready: false,
            admitted: false,
            seating,
            lot_said_ms: 0,
            draw_word_ms: BTreeMap::new(),
        })
    }

    /// `D-037`: the founder, back after a restart, rebuilt from its own record
    /// -- the table key it signed with, the advertisement as it stood, and the
    /// last `PLAYER_LIST` it signed, checked against the table key as any
    /// joiner checks a list. It holds the roster and its seat in it, ratifies (the
    /// recorded `TABLE_READY` verbatim, when given: the same `event_hash`, so
    /// the same session as the table's), and answers joins as the founder
    /// again. The group and the session it takes up from the members like any
    /// returning seat. Returns the state and what it says at once.
    #[allow(clippy::too_many_arguments)]
    pub fn found_back(
        app: SigningKey,
        table_seed: [u8; 32],
        ad: TableAd,
        advert_hash: Hash,
        list_bytes: &[u8],
        recorded_ready: Option<Vec<u8>>,
        now_ms: u64,
    ) -> Result<(Self, Vec<Send>), Failed> {
        let table_key = SigningKey::from_bytes(&table_seed);
        let table_id = table_key.verifying_key().to_bytes();
        let under = JoinedUnder::pin(ad, advert_hash, table_id);
        let (list, sender, _) = joinwire::receive_player_list_at(list_bytes).map_err(Failed::Wire)?;
        let roster = admit_list(&list, &sender, None, &under).map_err(Failed::List)?;
        // By its own key and not by seat 0: the seating draw (`D-083`) may have
        // put the founder anywhere at its table.
        let me = app.verifying_key().to_bytes();
        let mine = roster.seats().iter().find(|e| e.app_public_key == me);
        let my_buyin = mine.map(|e| e.buyin).unwrap_or(0);
        let my_seat = mine.map(|e| e.seat);
        let seating = Seating::new(table_id, me, me);
        let mut f = Formation {
            app,
            founder: Some(FounderPart {
                key: table_key,
                issued: VecDeque::new(),
                password: None,
            }),
            under,
            roster,
            serial: 0,
            my_seat,
            my_buyin,
            pending: None,
            ratified: BTreeMap::new(),
            ratified_bytes: BTreeMap::new(),
            sent_ready: false,
            session: None,
            capabilities: vec![DECK_CAPABILITY.to_vec()],
            said: Said::default(),
            early: VecDeque::new(),
            recorded_ready,
            recorded_refused: false,
            released: false,
            named_serial: None,
            started: false,
            hold_ready: false,
            admitted: false,
            seating,
            lot_said_ms: 0,
            draw_word_ms: BTreeMap::new(),
        };
        f.said.list = Some(list_bytes.to_vec());
        let out = f.adopt(&list, now_ms)?;
        Ok((f, out))
    }

    /// `D-037`: the table key's seed, for the founder's record. `None` for a
    /// seat that joined.
    pub fn table_seed(&self) -> Option<[u8; 32]> {
        self.founder.as_ref().map(|f| f.key.to_bytes())
    }

    /// `D-037`: the last `PLAYER_LIST` this founder signed, verbatim.
    pub fn my_list(&self) -> Option<&[u8]> {
        self.said.list.as_deref()
    }

    /// The advertisement this table was joined under, as it stands.
    pub fn ad(&self) -> &TableAd {
        &self.under.ad
    }

    /// The hash a `JOIN_REQUEST` names for that advertisement.
    pub fn advert_hash(&self) -> Hash {
        self.under.advert_hash
    }

    /// Ask to join somebody else's table. Returns the state and the signed
    /// `JOIN_REQUEST` to send.
    #[allow(clippy::too_many_arguments)]
    pub fn join(
        app: SigningKey,
        ad: TableAd,
        advert_hash: Hash,
        table_id: Hash,
        my_peer_id: Vec<u8>,
        my_name: String,
        my_buyin: u64,
        requested_seat: Option<u8>,
        password: Option<&[u8]>,
        join_nonce: Hash,
        now_ms: u64,
        // `my_tox_key`: this client's **Tox** public key, when the table's
        // advertisement says its traffic rides a group (D-019). A parameter and
        // not something set afterwards, because what this returns is already
        // sealed - a field added to a signed request is a field outside its
        // signature, which is the one thing a receiver would be right to
        // ignore.
        my_tox_key: Option<[u8; 32]>,
    ) -> Result<(Self, Vec<u8>), Failed> {
        let under = JoinedUnder::pin(ad, advert_hash, table_id);

        // The proof is per join and per table, so it does not replay to another
        // table. It is a possession proof, not a secret transfer — and it is not
        // a strength mechanism: a weak table password is guessable offline by
        // anyone who sees one proof.
        let proof = match (under.ad.password_required, password) {
            (true, Some(secret)) => Some(password_proof(secret, &under.table_id, &join_nonce)),
            (true, None) => return Err(Failed::Join(JoinRefused::BadPassword)),
            (false, _) => None,
        };

        let request = JoinRequest {
            advert_hash,
            app_public_key: app.verifying_key().to_bytes(),
            peer_id: my_peer_id.clone(),
            display_name: my_name.clone(),
            tox_key: my_tox_key,
            requested_seat,
            password_proof: proof,
            buyin: my_buyin,
            join_nonce,
            table_id,
        };
        let bytes = joinwire::publish_join_request(&request, &app, now_ms)?;
        // The hash the founder will echo is the hash of the bytes that went out,
        // so it is taken from them rather than recomputed from the value.
        let (_, _, request_hash) = joinwire::receive_join_request(&bytes)?;

        let roster = Roster::form(vec![], &under.ad, false)
            .map_err(|e| Failed::List(ListRefused::Roster(e)))?;
        let seating = Seating::new(table_id, app.verifying_key().to_bytes(), under.ad.founder_app_key);

        Ok((
            Formation {
                app,
                founder: None,
                under,
                roster,
                serial: 0,
                my_seat: None,
                my_buyin,
                pending: Some(request_hash),
                ratified: BTreeMap::new(),
                ratified_bytes: BTreeMap::new(),
                sent_ready: false,
                session: None,
                capabilities: vec![DECK_CAPABILITY.to_vec()],
                said: Said::default(),
                early: VecDeque::new(),
                recorded_ready: None,
                recorded_refused: false,
                released: false,
                named_serial: None,
                started: false,
                hold_ready: false,
                admitted: false,
                seating,
                lot_said_ms: 0,
                draw_word_ms: BTreeMap::new(),
            },
            bytes,
        ))
    }

    /// Re-sign this table's advertisement, and remember the copy.
    ///
    /// §7.2 obliges a re-broadcast every thirty seconds, and the timestamps are
    /// what rule 6 compares while the parameters are what rule 7 compares — so
    /// the advertisement is edited **only here** and only in time. A
    /// re-broadcast that changed a parameter would mark this table unjoinable at
    /// every receiver.
    ///
    /// Remembering it is not bookkeeping: a joiner names the copy it heard, and
    /// a founder that has forgotten that copy refuses an honest join.
    pub fn readvertise(&mut self, now_ms: u64, ttl_ms: u64) -> Result<Vec<u8>, Failed> {
        let f = self.founder.as_mut().ok_or(Failed::NotTheFounder)?;
        let mut ad = self.under.ad.clone();
        ad.timestamp_unix_ms = now_ms;
        ad.expires_at_unix_ms = now_ms + ttl_ms;
        ad.players = self.roster.len() as u8;

        let event = super::advert::publish(&ad, &f.key)
            .map_err(|e| Failed::Wire(WireError::Unencodable(e)))?;
        let hash = super::advert::verify_echoed(&event)
            .map(|(_, h)| h)
            .map_err(|_| Failed::Wire(WireError::Unencodable("the advert does not verify")))?;
        f.remember(event.clone(), hash, now_ms);

        // The advert this client holds moves with it; the parameters do not, and
        // `params` is what every later comparison is against.
        self.under.ad = ad;
        self.under.advert_hash = hash;
        Ok(event)
    }

    /// `GENESIS(0)` — the setup chain's genesis, which every `TABLE_READY` is
    /// chained to and which is therefore where a parameter fork is caught.
    pub fn genesis(&self) -> Hash {
        genesis_setup(&self.under.table_id, &self.under.params)
    }

    /// Everything this client should repeat to somebody who has just appeared
    /// on the table's topic.
    ///
    /// The list first and the ratification second, because a ratification names
    /// the serial of a list its receiver may not have yet.
    /// **The list is signed again, not repeated.** `S1-P`: `publish` hashes the
    /// message content for its id and holds it in a duplicate cache for 120
    /// seconds, so republished bytes are refused **on this side** for exactly
    /// the window a peer arriving late needs them in — and after that window the
    /// receiver refuses them instead, `on_player_list` dropping a list older
    /// than `LIST_MAX_AGE_MS`. Measured before this: `said 0 message(s) again,
    /// and Duplicate`, on every node of a run, thirty-two times.
    ///
    /// Only a founder can do it — a `PLAYER_LIST` is signed under the table key
    /// — and only for the list. **A ratification must arrive verbatim**:
    /// `emitted_at_unix_ms` is inside the body `event_hash` covers, `session_id`
    /// is computed over those hashes, and a re-signed `TABLE_READY` would give
    /// its receiver a different session identity from everybody else and make
    /// `take_ratification` report an honest seat as `RatifiedTwice`. So the
    /// ratification is still repeated as-is, still refused inside the window,
    /// and what carries it instead is the wire decision `S1-P` leaves open.
    pub fn say_again(&self, now_ms: u64) -> Vec<Vec<u8>> {
        let mut out = Vec::with_capacity(2);
        match (&self.founder, self.serial > 0) {
            (Some(f), true) => {
                if let Ok(bytes) = joinwire::publish_player_list(&self.current_list(), &f.key, now_ms) {
                    out.push(bytes);
                }
            }
            _ => {
                if let Some(l) = &self.said.list {
                    out.push(l.clone());
                }
            }
        }
        if let Some(r) = &self.said.ready {
            out.push(r.clone());
        }
        // **And every other seat's ratification this client holds.**
        //
        // `S1-P`. A seat that missed one `TABLE_READY` never computes a session
        // and never plays; before this, only its author could re-send it, and
        // only inside the 120 s in which GossipSub refuses a verbatim repeat.
        // Every seat that heard it can now repair it, and D-019's amendment of
        // 2026-09-02 puts these on the table's group where no duplicate cache
        // applies.
        //
        // Own copy excluded because `said.ready` above is already it, and a
        // second identical entry would be one wasted send per tick.
        for (seat, bytes) in &self.ratified_bytes {
            if Some(*seat) != self.my_seat {
                out.push(bytes.clone());
            }
        }
        out
    }

    /// The roster as this founder says it now, at the serial it holds -- with
    /// the founder's *set to start* and the seating draw as it stands (`D-083`):
    /// every list a founder says carries both, the ones it says again included,
    /// or a list said again would un-say them.
    fn current_list(&self) -> PlayerList {
        PlayerList {
            roster: self.roster.seats().to_vec(),
            table_params_hash: self.under.params,
            list_serial: self.serial,
            started: self.started,
            draw: self.seating.carried(),
        }
    }

    /// `D-044`: the fewest seats a roster may be ratified at here -- the
    /// advert's `min_players_to_start`, or two once a roster at that minimum
    /// was adopted (the table was set to start; a seat given back before the
    /// first hand does not un-set it). The owner's floor is heads-up.
    fn floor(&self) -> usize {
        if self.started {
            2
        } else {
            self.under.ad.min_players_to_start as usize
        }
    }

    /// `S1-DV`: whether a roster the founder said again dropped this client's
    /// seat, which an earlier one held -- given back before the first hand.
    pub fn released_before_the_first_hand(&self) -> bool {
        self.released
    }

    /// The founder's peer id, from the advert this table was formed under.
    pub fn founder_peer_id(&self) -> &[u8] {
        &self.under.ad.founder_peer_id
    }

    /// When the advert this table was formed under was made, on the
    /// founder's clock -- what a founder's later answer to the lobby's
    /// question is compared against (`D-044`).
    pub fn advert_time(&self) -> u64 {
        self.under.ad.timestamp_unix_ms
    }

    /// `D-061`: the password of a table this client founded, for its
    /// continuation.
    pub fn password(&self) -> Option<&[u8]> {
        self.founder.as_ref().and_then(|f| f.password.as_deref())
    }

    /// Whether this client founded the table, and therefore answers joins and
    /// re-signs the advertisement.
    pub fn is_founder(&self) -> bool {
        self.founder.is_some()
    }

    /// How many ratifications are waiting for a roster. For the test that
    /// bounds it.
    ///
    /// Public rather than `cfg(test)`: an integration test outside the crate
    /// is where the queue's behaviour under a replayed recording is actually
    /// checked, and a counter is the only way to see a slot being wasted.
    pub fn held_early(&self) -> usize {
        self.early.len()
    }

    /// The advertisement this table was formed under.
    ///
    /// Pinned at the moment this client committed to it, so it is the game this
    /// client agreed to and not whatever the founder is advertising now.
    pub fn advert(&self) -> &TableAd {
        &self.under.ad
    }

    /// The advertisement this founder is offering right now, as the signed
    /// bytes it last put on the wire -- what a lobby question (§7.5) is
    /// answered with. `None` for a seat that is not the founder.
    pub fn current_advert(&self) -> Option<Vec<u8>> {
        self.founder.as_ref()?.issued.back().map(|i| i.event.clone())
    }

    /// The parameter hash every later comparison is against.
    pub fn under_params(&self) -> Hash {
        self.under.params
    }

    pub fn table_id(&self) -> Hash {
        self.under.table_id
    }

    pub fn roster(&self) -> &Roster {
        &self.roster
    }

    pub fn my_seat(&self) -> Option<u8> {
        self.my_seat
    }

    pub fn serial(&self) -> u64 {
        self.serial
    }

    /// The `session_id`, once every seat has ratified. `None` until then, and
    /// the table is not real until then.
    pub fn session(&self) -> Option<Hash> {
        self.session
    }

    /// `TERMINAL(0)`: the hash of the `TABLE_READY` stage.
    ///
    /// Formation is itself a collective stage — a fixed set of seats each
    /// emitting once — so it has a stage hash like any other, and that hash is
    /// what the first hand's genesis hangs off. `PROTOCOL.md` says it in one
    /// line: *"`TERMINAL(0)` is a `stage_hash` and carries no fields at all."*
    ///
    /// `None` until every seat has ratified, for the same reason `session` is:
    /// a stage that has not completed has no hash, and a partial one computed
    /// from whoever happened to answer first would differ between peers.
    pub fn terminal_zero(&self) -> Option<Hash> {
        self.session?;
        // Ascending, which `stage_hash_collective` requires and a `BTreeMap`
        // gives for free — the same order `session_id` above depends on, and
        // for the same reason.
        let emitters: Vec<crate::protocol::transcript::StageEmitter> = self
            .ratified
            .iter()
            .map(
                |(&seat, &event_hash)| crate::protocol::transcript::StageEmitter {
                    seat,
                    event_hash,
                },
            )
            .collect();
        Some(crate::protocol::transcript::stage_hash_collective(
            0,
            crate::protocol::messages::EventType::TableReady.code(),
            &emitters,
        ))
    }

    /// `GENESIS(1)`: the parent of the first hand's first stage.
    ///
    /// `roster_hash(1) == roster_hash(0)` because a buy-in enters the ledger at
    /// the first `HAND_INIT` and nowhere earlier, which is what makes an
    /// abandoned formation move no chips (`PROTOCOL.md` §4.3).
    pub fn genesis_one(&self) -> Option<Hash> {
        Some(crate::protocol::transcript::genesis_hand(
            &self.under.table_id,
            1,
            &self.session?,
            &self.roster.hash_at_zero(),
            &self.terminal_zero()?,
            // `R(1)` **is** the signers of `TABLE_READY` (§3.2), so the first
            // hand commits to who ratified it exactly as every later one
            // commits to who is still playing.
            &self.ratifiers(),
        ))
    }

    /// The seats that ratified, ascending: the required emitter set `R` for the
    /// first hand.
    ///
    /// §3.2, the disposition of J2: *"`R` is derived from demonstrated
    /// participation in the agreed chain, never from a seat's status … for the
    /// first hand, the required set is the signers of `TABLE_READY`."*
    pub fn ratifiers(&self) -> Vec<u8> {
        self.ratified.keys().copied().collect()
    }

    /// How many ratifications are waiting for a roster they fit.
    ///
    /// `on_table_ready` has three outcomes and only two of them are visible: a
    /// refusal is reported by the caller and an acceptance moves `ratifiers`,
    /// while **held** returns `Ok(vec![])` and looks exactly like nothing having
    /// arrived. With `ratifiers` alone, a client stuck at `ratified 1/4` cannot
    /// be told apart from one nobody is talking to.
    pub fn held(&self) -> usize {
        self.early.len()
    }

    /// `connection_peer_id` is what the **transport** authenticated, not what
    /// the request claims: the whole of `U17` rests on those being compared.
    ///
    /// The founder's answer to a join request.
    ///
    /// `started` is whether this table has dealt a hand. **A stranger may not
    /// join one that has** — `STATE_MACHINE.md` §9.4: *"A seat may not be added
    /// after `Seating`, in either mode, in this version (P8)"*, and §3.1: *"No
    /// seat is added to the vector, removed from it, or reordered within it for
    /// the life of the table."* Nothing enforced it, and accepting one does
    /// `serial += 1; ratified.clear(); session = None` — **un-ratifying a table
    /// in the middle of a tournament** and giving its roster a row that
    /// `roster_hash(k)` does not contain. `S1-T`.
    ///
    /// The only barrier before this was `TableFull`, which never fires on a
    /// table that started at `min_players_to_start` below `max_players` — this
    /// client's own default.
    ///
    /// **A seat that is already on the roster is not a stranger and is not
    /// refused here.** It is answered as it was before, with `AlreadySeated`
    /// and a freshly signed roster (`S1-J`); that path exists for a peer that
    /// lost its list and gating it here would undo the fix without saying so.
    /// `D-047`: a seat out of this table for good asks to sit again -- refused
    /// with the reason, whatever else the request says.
    pub fn refuse_out(&self, bytes: &[u8], now_ms: u64) -> Result<Vec<u8>, Failed> {
        let f = self.founder.as_ref().ok_or(Failed::NotTheFounder)?;
        let (_, _, request_hash) = joinwire::receive_join_request(bytes)?;
        Ok(joinwire::publish_join_reject(request_hash, RejectReason::OutForGood, 0, &f.key, now_ms)?)
    }

    /// `D-083`: refuse a request from a key the draw's judgement gave back -- it
    /// held the seating draw up, and seated again it would hold it up again.
    pub fn refuse_held_up(&self, bytes: &[u8], now_ms: u64) -> Result<Vec<u8>, Failed> {
        let f = self.founder.as_ref().ok_or(Failed::NotTheFounder)?;
        let (_, _, request_hash) = joinwire::receive_join_request(bytes)?;
        Ok(joinwire::publish_join_reject(request_hash, RejectReason::HeldTheDrawUp, 0, &f.key, now_ms)?)
    }

    /// `S1-GR`: refuse a request from a key that sat down here and left, or was
    /// given back, too often lately -- with when to ask again.
    pub fn refuse_too_soon(&self, bytes: &[u8], retry_after_ms: u32, now_ms: u64) -> Result<Vec<u8>, Failed> {
        let f = self.founder.as_ref().ok_or(Failed::NotTheFounder)?;
        let (_, _, request_hash) = joinwire::receive_join_request(bytes)?;
        Ok(joinwire::publish_join_reject(request_hash, RejectReason::TooSoon, retry_after_ms, &f.key, now_ms)?)
    }

    pub fn on_join_request(
        &mut self,
        bytes: &[u8],
        connection_peer_id: &[u8],
        started: bool,
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let f = self.founder.as_ref().ok_or(Failed::NotTheFounder)?;
        let (req, sender, request_hash) = joinwire::receive_join_request(bytes)?;

        let seated = self
            .roster
            .seats()
            .iter()
            .any(|e| e.app_public_key == sender || e.peer_id == connection_peer_id);
        if started && !seated {
            // **`AdvertExpired`, and the choice is worth stating.** The refusal
            // vocabulary has no word for *the table has started*: `RejectReason`
            // is `TableFull`, `SeatTaken`, `BadPassword`, `BuyinOutOfRange`,
            // `AdvertExpired`, `Banned`, `CapabilityMismatch`, `AlreadySeated`.
            // This is the nearest true one — §7.3 withdraws a table's advert
            // when it starts (`reason = 1`), so an advert that still names this
            // table as joinable is one that should no longer exist — and it
            // tells the asker not to retry under it, which is the behaviour
            // that matters. A dedicated code point is a wire decision and is
            // the owner's.
            let reply = joinwire::publish_join_reject(
                request_hash,
                RejectReason::AdvertExpired,
                0,
                &f.key,
                now_ms,
            )?;
            return Ok(vec![Send::Reply(reply)]);
        }

        // The copy **this joiner** heard, which is very unlikely to be the
        // newest one. Every field but the hash is identical across
        // re-broadcasts — §7.2 rule 7 refuses one that changed a parameter — so
        // this is the advertisement they joined under, exactly as `JoinedUnder`
        // means it.
        // `S1-J`, and `D-037` moved it up here: a seat already on the roster is
        // answered *already seated* and given the roster again, whichever copy
        // of the advertisement it names. The lookup below is for strangers --
        // a founder back from a restart holds no signed copy at all, and a
        // seat that comes back names the copy it heard, which need not be the
        // founder's last.
        if seated {
            let reply = joinwire::publish_join_reject(
                request_hash,
                RejectReason::AlreadySeated,
                0,
                &f.key,
                now_ms,
            )?;
            let mut out = vec![Send::Reply(reply)];
            if self.serial > 0 {
                if let Ok(bytes) = joinwire::publish_player_list(&self.current_list(), &f.key, now_ms) {
                    self.said.list = Some(bytes.clone());
                    out.push(Send::Broadcast(bytes));
                }
            }
            return Ok(out);
        }

        let under = match f.find(&req.advert_hash) {
            Some(i) => JoinedUnder {
                advert_hash: i.hash,
                ..self.under.clone()
            },
            None => {
                let reply = joinwire::publish_join_reject(
                    request_hash,
                    RejectReason::AdvertExpired,
                    0,
                    &f.key,
                    now_ms,
                )?;
                return Ok(vec![Send::Reply(reply)]);
            }
        };
        let advert_event = f
            .find(&req.advert_hash)
            .map(|i| i.event.clone())
            .unwrap_or_default();

        // `D-083`: from the first sealing of the draw's lots nobody new sits down
        // until the table is set -- a player seated then would be one the draw
        // never drew, placed wherever the founder liked, or a player given back
        // for not opening, back to open the round run again instead. A member of
        // a complete draw coming back takes the seat the draw gave it, and nothing
        // else -- and not once this founder has ratified: its ratification sets
        // the table, and a seat added under it would split the table between the
        // roster set and the one after (`S1-HE`).
        let assigned = if self.seating.frozen() {
            match self.seating.drawn_seat(&sender).filter(|_| !self.sent_ready && self.session.is_none()) {
                Some(seat) => Some(seat),
                None => {
                    let reply = joinwire::publish_join_reject(request_hash, RejectReason::TableFull, 0, &f.key, now_ms)?;
                    return Ok(vec![Send::Reply(reply)]);
                }
            }
        } else {
            None
        };

        match admit_join(
            &req,
            &sender,
            connection_peer_id,
            &under,
            &self.roster,
            f.password.as_deref(),
            assigned,
        ) {
            Ok(entry) => {
                let mut seats = self.roster.seats().to_vec();
                seats.push(entry.clone());
                seats.sort_by_key(|e| e.seat);
                self.roster = Roster::form(seats, &self.under.ad, false)
                    .map_err(|e| Failed::List(ListRefused::Roster(e)))?;

                // The roster changed, so every previous ratification named a
                // serial that no longer describes this table.
                self.serial += 1;
                self.ratified.clear();
                self.ratified_bytes.clear();
                self.sent_ready = false;
                self.session = None;

                // The copy the joiner named, echoed back. Sending the
                // newest instead would fail the joiner's own check that the
                // echo is the advertisement it asked under — which is the check
                // that stops a founder swapping the game after the fact.
                let accept = JoinAccept {
                    request_hash,
                    seat: entry.seat,
                    advert_event,
                    roster_so_far: self.roster.seats().to_vec(),
                    list_serial: self.serial,
                };
                let f = self.founder.as_ref().ok_or(Failed::NotTheFounder)?;
                let reply = joinwire::publish_join_accept(&accept, &f.key, now_ms)?;

                let list = self.current_list();
                let list_bytes = joinwire::publish_player_list(&list, &f.key, now_ms)?;
                self.said.list = Some(list_bytes.clone());

                let mut out = vec![Send::Reply(reply), Send::Broadcast(list_bytes)];
                // The founder is a seat like any other and ratifies its own
                // proposal by the same path as everybody else.
                out.extend(self.adopt(&list, now_ms)?);
                Ok(out)
            }
            Err(why) => {
                // Two of the refusals are not refusals but protocol violations:
                // the key in the payload is not the key that signed, or the peer
                // id is not the connection. Answering those at all confirms to
                // the sender that its forgery reached a real founder, so they
                // get nothing and time out.
                let reason = match &why {
                    JoinRefused::KeyIsNotTheSender | JoinRefused::PeerIdIsNotTheConnection => {
                        return Err(Failed::Join(why))
                    }
                    JoinRefused::UnknownAdvert | JoinRefused::WrongTable => {
                        RejectReason::AdvertExpired
                    }
                    JoinRefused::BadPassword => RejectReason::BadPassword,
                    JoinRefused::AlreadySeated => RejectReason::AlreadySeated,
                    JoinRefused::SeatUnavailable { .. } => RejectReason::SeatTaken,
                    JoinRefused::TableFull => RejectReason::TableFull,
                    JoinRefused::Seat(_) => RejectReason::BuyinOutOfRange,
                };
                let reply =
                    joinwire::publish_join_reject(request_hash, reason, 0, &f.key, now_ms)?;

                // **A join request from a seat that is already seated is proof
                // that its roster is stale, so answer with the roster as well.**
                //
                // Measured at nine seats: three joiners stopped at *4 seated*
                // while the founder and four others reached nine, because the
                // `PLAYER_LIST` that carried seats five to nine never reached
                // them. `PLAYER_LIST` is broadcast when the roster **changes**
                // and there is no request for it, so a peer that misses the
                // last one misses it for ever — and its only recovery is to ask
                // to join again, every thirty seconds, and be told *already
                // seated*. Correct, and useless: the table never ratified and
                // the run ended with `NO TABLE` on every node.
                //
                // **Signed again rather than repeated, and the first version of
                // this got that wrong.** It re-broadcast `said.list` — the exact
                // bytes published when the roster last changed — and
                // `on_player_list` refuses a list older than `LIST_MAX_AGE_MS`,
                // ninety seconds. So the repeat helped only while the roster had
                // changed within the last minute and a half, and was discarded
                // by every receiver otherwise.
                //
                // That is precisely the case it exists for. Players arrive
                // irregularly and a tournament table stays open until it fills;
                // a seat that joined and then waited a quarter of an hour is
                // asking about a roster whose last change is long past ninety
                // seconds ago, and the stale bytes would be refused by the very
                // peer that needs them.
                //
                // The content and the `list_serial` are unchanged — this is the
                // same list, said again, which §14's table permits in terms:
                // `PLAYER_LIST` is `chain_scope = 0`, **"any number, any time"**,
                // and outside the equivocation predicate.
                let mut out = vec![Send::Reply(reply)];
                if matches!(reason, RejectReason::AlreadySeated) && self.serial > 0 {
                    if let Ok(bytes) = joinwire::publish_player_list(&self.current_list(), &f.key, now_ms) {
                        self.said.list = Some(bytes.clone());
                        out.push(Send::Broadcast(bytes));
                    }
                }
                Ok(out)
            }
        }
    }

    /// Give a seat back, **before the first hand and never after it**.
    ///
    /// # Why this exists, and why it is not eviction
    ///
    /// A player who sits down at a tournament table and leaves before it fills
    /// is ordinary, and until this existed nothing in the client ever removed a
    /// seat from a formation roster — not a clean leave, not a disconnect, not
    /// time. Measured: six seats, one leaving cleanly at 90 s, the replacement
    /// told *“the table is full”* at 95 s and again at 106 s, and the founder
    /// then dealing hand 1 to all six including the one that had gone. A
    /// tournament that lost a player before it started could not be completed
    /// by anybody. That is `S1-M`.
    ///
    /// **It is not D-010's forfeiture nor D-014's unseating**, and the
    /// difference is not a matter of degree: those govern a seat with chips
    /// behind it in a hand that is being played. Here no card has been dealt,
    /// no chip has moved, `session` is `None` or about to be, and the seat's
    /// buy-in is a number in an advert. Giving it back costs its owner a place
    /// in a queue, and they take it again by joining again.
    ///
    /// **The caller must hold the hand-has-not-started condition**, because
    /// this type cannot see it: `Formation` knows the roster and the session,
    /// not whether `HAND_INIT` has gone out. `net::run` gates on `ever_dealt`
    /// and this name says so, so a second caller has to read the sentence
    /// before it can get it wrong.
    ///
    /// The founder's own seat is never released: a table whose founder gave its
    /// own seat away is a table with an advert and nobody behind it.
    ///
    /// Returns an empty vector when there is no such seat, which is the
    /// ordinary case — a peer goes quiet, is released once, and the sweep that
    /// found it runs again a minute later.
    pub fn release_seat_before_the_first_hand(
        &mut self,
        peer_id: &[u8],
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        self.release_seats_before_the_first_hand(&[peer_id.to_vec()], now_ms)
    }

    /// Give several seats back in one roster (`D-083`): the draw's judgement
    /// gives back every member that held a round up at once, and one roster a
    /// seat would be one round a seat.
    ///
    /// **The draw goes on or starts again by one rule.** Before the lots are
    /// sealed any change of players starts a fresh round, which costs nothing
    /// since nothing is open. Once they are sealed, a member given back after it
    /// opened keeps its lot in the draw and its drawn seat stays empty -- giving
    /// it back re-draws nothing -- and one given back before it opened fails the
    /// round, which starts again without it. A complete draw is never re-drawn.
    pub fn release_seats_before_the_first_hand(
        &mut self,
        peer_ids: &[Vec<u8>],
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let f = self.founder.as_ref().ok_or(Failed::NotTheFounder)?;
        let key = f.key.clone();

        let going: Vec<SeatEntry> = self
            .roster
            .seats()
            .iter()
            .filter(|e| peer_ids.iter().any(|p| p == &e.peer_id) && Some(e.seat) != self.my_seat)
            .cloned()
            .collect();
        if going.is_empty() {
            return Ok(Vec::new());
        }
        let seats: Vec<SeatEntry> = self
            .roster
            .seats()
            .iter()
            .filter(|e| !going.iter().any(|g| g.seat == e.seat))
            .cloned()
            .collect();
        // `Roster::form` requires the seats sorted and unique and says nothing
        // about contiguity, so the hole this leaves is a seat number that is
        // free — which is exactly what the join path looks for.
        self.roster = Roster::form(seats, &self.under.ad, false)
            .map_err(|e| Failed::List(ListRefused::Roster(e)))?;

        // The roster changed, so every previous ratification named a serial that
        // no longer describes this table. The same four lines the accept path
        // runs, for the same reason.
        self.serial += 1;
        self.ratified.clear();
        self.ratified_bytes.clear();
        self.sent_ready = false;
        self.session = None;

        // `D-083`: a founder left alone keeps no draw. Nobody the draw protects
        // is still at the table, and a draw kept for players who are gone would
        // keep every new player from sitting down, for ever.
        if self.roster.len() < 2 {
            self.seating = Seating::new(self.under.table_id, self.app.verifying_key().to_bytes(), self.app.verifying_key().to_bytes());
            // And it fills as a new table: `D-044`'s *set to start* was the
            // table that left, and kept, the next player to sit down would have
            // started a table advertised for more at two.
            self.started = false;
        }

        let fails = self.seating.done().is_none()
            && self.seating.sealed()
            && going.iter().any(|e| !self.seating.opened_by(&e.app_public_key));
        let mut list = self.current_list();
        if fails {
            list.draw = None;
        }
        let list_bytes = joinwire::publish_player_list(&list, &key, now_ms)?;
        self.said.list = Some(list_bytes.clone());

        let mut out = vec![Send::Broadcast(list_bytes)];
        out.extend(self.adopt(&list, now_ms)?);
        Ok(out)
    }

    /// The joiner's handling of whatever came back.
    ///
    /// One entry point for both answers because the joiner does not get to
    /// choose which it receives, and a client that called the wrong reader would
    /// treat a refusal as silence.
    pub fn on_join_answer(&mut self, bytes: &[u8], now_ms: u64) -> Result<Vec<Send>, Failed> {
        let pending = self
            .pending
            .ok_or(Failed::OutOfOrder("no join request is outstanding"))?;

        if let Ok((request_hash, reason, retry_after_ms, sender)) =
            joinwire::receive_join_reject(bytes)
        {
            if sender != self.under.table_id || request_hash != pending {
                return Err(Failed::OutOfOrder("a refusal of something else"));
            }
            self.pending = None;
            return Err(Failed::Refused {
                reason,
                retry_after_ms,
            });
        }

        let (accept, sender) = joinwire::receive_join_accept(bytes)?;

        // The echoed advert is re-verified from its own bytes rather than
        // believed: §4.3 repeats it verbatim precisely so the joiner does not
        // have to trust the founder or a gossip copy.
        let (echo_ad, echo_hash) = super::advert::verify_echoed(&accept.advert_event)
            .map_err(|_| Failed::Accept(AcceptRefused::AdvertMismatch))?;

        let roster = admit_accept(
            &accept,
            &sender,
            &pending,
            &echo_hash,
            &echo_ad,
            &self.under,
            &self.app.verifying_key().to_bytes(),
            self.my_buyin,
        )
        .map_err(Failed::Accept)?;

        self.pending = None;
        self.admitted = true;
        // `D-083`: the acceptance says which roster it is, so a reply that took
        // longer than the lists behind it -- measured at ten seconds -- is ranked
        // against them and never puts back a seating the draw has since changed.
        if accept.list_serial <= self.serial {
            let _ = now_ms;
            return Ok(vec![]);
        }
        self.my_seat = Some(accept.seat);

        // **A `JOIN_ACCEPT` is a bootstrap, not an update.**
        //
        // `roster_so_far` is the roster as it stood *at the instant the founder
        // accepted this joiner* — exactly `seat + 1` entries. This write used to
        // be unconditional, and it is the one membership mutation of the four in
        // this type that moves no version: `on_join_request` and
        // `release_seat_before_the_first_hand` both do `serial += 1`, and
        // `on_player_list` is guarded by `admit_list`'s `NotNewer`. This one
        // touched neither `serial` nor `ratified` nor `session`, and
        // `JoinAcceptBody` carries no `list_serial` at all, so the snapshot
        // cannot even be *ordered* against what is held.
        //
        // It is not a race lost by microseconds: the reply is slow. Measured,
        // the peer this killed asked to join at t=10.6 s and was answered at
        // t=20.6 s, and in those ten seconds the founder seated two more players
        // and it adopted every list up to the founder's last. So the condition
        // is narrow — the accept lands after the last `PLAYER_LIST` this peer
        // will ever adopt, and the peer is not the last seat.
        //
        // Then this line rewound a correct roster to the founder's older
        // snapshot, and because `serial` was left untouched, every later
        // rebroadcast died at `list_serial <= held`. Permanently — including
        // down `say_again`, which is a genuine full-state anti-entropy push,
        // is called on every gossipsub `Subscribed`, and re-emits at the
        // *current* serial, so it is refused by the same equality it exists to
        // repair.
        //
        // Measured, ten seats, 900 s: the roster of every clobbered peer fell to
        // exactly `my_seat + 1`, with no exceptions. Seat 7 sat at `ratified
        // 5/8, 10 held` for the remaining 880 s and never sealed a table, and
        // seat 8 was clobbered 100 ms *after* it sealed — keeping a session
        // whose roster hash nobody else computes, which is why it opened every
        // hand late and finished none. Seat 9 was unharmed because by then the
        // snapshot was already the whole table, and the early joiners recovered
        // because a higher serial still followed.
        //
        // So: take the seat, and take the roster only when what is held does not
        // already seat us where the founder says. That covers both directions —
        // a stale snapshot cannot rewind a list, and a list that does not yet
        // contain us cannot leave us seated but absent from our own roster.
        // `JOIN_ACCEPT` carries its `list_serial` since protocol major 2 (`D-083`),
        // and an acceptance no newer than the roster held was answered above
        // without touching it; the serial itself stays the lists' to move, so
        // the list of that serial is still taken when it comes.
        let seated_here_already =
            self.roster.seat_of(&self.app.verifying_key().to_bytes()) == Some(accept.seat);
        if !seated_here_already {
            self.roster = roster;
        }
        let _ = now_ms;
        Ok(vec![])
    }

    /// A `PLAYER_LIST` from the founder — a proposal, checked every time.
    pub fn on_player_list(&mut self, bytes: &[u8], now_ms: u64) -> Result<Vec<Send>, Failed> {
        let (list, sender, emitted_at) = joinwire::receive_player_list_at(bytes)?;
        // **A stale list, with nothing to compare it against.** Until the first
        // list arrives a client has no serial, so `admit_list` was given `None`
        // and accepted any serial at all — including a genuine list from an
        // hour ago, replayed by anybody who had seen it. The joiner then
        // adopts a roster nobody is holding, ratifies against it, is refused by
        // the founder because the serial is stale, and waits for the next real
        // list; repeated, that is a joiner held out of a table for free.
        //
        // The list carries no time of its own, but the founder signed its
        // envelope. Bounded on both sides against this client's own local view,
        // which is what `lobby::admit` does with an advert and is not a value
        // anything hashes (D-012).
        if emitted_at > now_ms.saturating_add(MAX_CLOCK_SKEW_MS) {
            return Err(Failed::List(ListRefused::Stale));
        }
        if now_ms.saturating_sub(emitted_at) > LIST_MAX_AGE_MS {
            return Err(Failed::List(ListRefused::Stale));
        }
        let held = if self.serial == 0 { None } else { Some(self.serial) };
        let roster = admit_list(&list, &sender, held, &self.under).map_err(Failed::List)?;
        // `S1-DV`: a list that no longer names this client, which an earlier one
        // did. `S1-GB`: earlier by serial -- a list of the seconds before this
        // client sat down, carried late behind its acceptance, is newer than the
        // nothing an acceptance holds and names it nowhere, and two joiners on the
        // bed read such lists as their seats given back and dropped their table
        // (`churn163737-10`: seated at 54.4 s, *given back* at 55.5 s, and the
        // founder had given nobody back).
        let me = self.app.verifying_key().to_bytes();
        let names_me = roster.seat_of(&me).is_some();
        // `D-083`: what the roster does to the seating draw, checked before any
        // of it is taken. A roster that gives this client back is taken whatever
        // it does to the draw: this client is leaving it.
        if names_me {
            // A founder never gives its own seat back; a roster without it is one
            // a founder could run a round again by, leaving itself out of it.
            if roster.seat_of(&self.under.ad.founder_app_key).is_none() {
                return Err(Failed::Seating(SeatingRefused::FounderNotSeated));
            }
            let keys: Vec<[u8; 32]> = roster.seats().iter().map(|e| e.app_public_key).collect();
            match &list.draw {
                None => self.seating.check_plain(&keys).map_err(Failed::Seating)?,
                Some(d) => {
                    self.seating.check_draw(d, &roster).map_err(Failed::Seating)?;
                }
            }
            // A founder that moved this client's own stake between rosters would
            // have changed its stack in every hand's genesis. Held only to the
            // buy-in this client asked for and was accepted with: a seat taken up
            // again after a restart asks with its recorded stack, which the roster
            // it is answered with rightly does not carry.
            if let Some(e) = roster.seats().iter().find(|e| e.app_public_key == me) {
                if self.admitted && self.my_buyin != 0 && e.buyin != self.my_buyin {
                    return Err(Failed::Accept(AcceptRefused::BuyinNotOurs { asked: self.my_buyin, given: e.buyin }));
                }
            }
        }
        if !names_me && self.named_serial.is_some_and(|n| list.list_serial > n) {
            self.released = true;
        }
        if names_me {
            self.named_serial = Some(self.named_serial.map_or(list.list_serial, |n| n.max(list.list_serial)));
        }
        self.roster = roster;
        self.adopt(&list, now_ms)
    }

    /// Take a checked list as this client's own, and ratify it if it is
    /// ratifiable.
    fn adopt(&mut self, list: &PlayerList, now_ms: u64) -> Result<Vec<Send>, Failed> {
        if list.list_serial != self.serial {
            self.serial = list.list_serial;
            self.ratified.clear();
            self.ratified_bytes.clear();
            self.sent_ready = false;
            self.session = None;
        }
        self.my_seat = self
            .roster
            .seat_of(&self.app.verifying_key().to_bytes())
            .or(self.my_seat);

        let seat = match self.my_seat {
            Some(s) if self.roster.seat_of(&self.app.verifying_key().to_bytes()) == Some(s) => s,
            // Not in this roster: nothing to ratify, and nothing to complain
            // about either — a list can name a table this client is still
            // joining.
            _ => return Ok(vec![]),
        };

        if self.roster.len() >= self.under.ad.min_players_to_start as usize {
            self.started = true;
        }
        // `D-083`: the founder's word on it, which a player sitting down after
        // seats were given back has no other way to know.
        self.started |= list.started;
        let mut out = self.draw_on(list, now_ms);
        // `D-044`: a table that was set to start goes on with the seats that
        // remain, two at the least, when the founder says its roster again
        // without a seat given back before the first hand -- `floor`, on
        // both the saying and the taking of a ratification. `D-083`: and only
        // a roster that sits as its complete draw put it is ratified.
        let short = self.roster.len() < self.floor();
        if self.sent_ready || short || self.seating.digest_for(&self.roster).is_none() {
            self.replay_early();
            return Ok(out);
        }
        // `D-060`: not before this client hears every seat -- its node says when.
        if self.hold_ready && self.session.is_none() {
            self.replay_early();
            return Ok(out);
        }
        out.extend(self.ratify(seat, now_ms)?);
        Ok(out)
    }

    /// `D-083`: what a roster this client took does to its part in the draw --
    /// a roster at or above the floor without a draw begins a round of its
    /// players, and one with a draw is taken into it.
    fn draw_on(&mut self, list: &PlayerList, now_ms: u64) -> Vec<Send> {
        let mut out = Vec::new();
        match &list.draw {
            None => {
                if self.roster.len() >= self.floor() {
                    let keys: Vec<[u8; 32]> = self.roster.seats().iter().map(|e| e.app_public_key).collect();
                    self.seating.begin(list.list_serial, &keys);
                    out.extend(self.say_lot(now_ms));
                } else {
                    self.seating.end_round();
                }
            }
            Some(d) => {
                if let Ok(checked) = self.seating.check_draw(d, &self.roster) {
                    let taken = self.seating.take_draw(d, checked);
                    if taken.open_now {
                        out.extend(self.say_opening(now_ms));
                    }
                }
            }
        }
        out
    }

    /// This client's sealed lot, said now and heard by itself -- the founder
    /// seals its own lot with the rest.
    fn say_lot(&mut self, now_ms: u64) -> Option<Send> {
        if withheld_for_harness() == Some(Owed::Lot) && self.founder.is_none() {
            return None;
        }
        let word = self.seating.lot_word(&self.app, now_ms)?;
        if let Ok(said) = super::tabletalk::open_lot(&word, &self.under.table_id) {
            self.seating.hear_lot(&said, &word, now_ms);
        }
        self.lot_said_ms = now_ms;
        Some(Send::Broadcast(word))
    }

    /// This client's opening, said now and heard by itself.
    fn say_opening(&mut self, now_ms: u64) -> Option<Send> {
        if withheld_for_harness() == Some(Owed::Opening) && self.founder.is_none() {
            return None;
        }
        let word = self.seating.opening_word(&self.app, now_ms)?;
        if let Ok(said) = super::tabletalk::open_opening(&word, &self.under.table_id) {
            self.seating.hear_opening(&said, &word);
        }
        self.lot_said_ms = now_ms;
        Some(Send::Broadcast(word))
    }

    /// `D-083`: the node's tick for the draw. Every client says its lot or its
    /// opening again while the round waits on it, every [`LOT_EVERY_MS`]; the
    /// founder seals the lots once every member's is in and `may_seal` -- its
    /// node wants the table to start (a search's founder only once armed) --
    /// and completes the draw once every sealed lot is opened.
    pub fn draw_tick(&mut self, now_ms: u64, may_seal: bool) -> Vec<Send> {
        let mut out = Vec::new();
        if self.session.is_some() {
            return out;
        }
        // A clock that jumped back says it again now rather than waiting out the
        // jump (`S1-JB`).
        if now_ms < self.lot_said_ms || now_ms - self.lot_said_ms >= LOT_EVERY_MS {
            out.extend(self.say_lot(now_ms));
            out.extend(self.say_opening(now_ms));
            // A member carries every other member's word it holds with its
            // own, verbatim -- signed by their authors, so carried like a
            // ratification (`S1-P`). A founder that cannot hear one member
            // directly is not left to give back a player the others heard.
            if self.founder.is_none() {
                let held = self.seating.held_words();
                if !held.is_empty() {
                    self.lot_said_ms = now_ms;
                }
                out.extend(held.into_iter().map(Send::Broadcast));
            }
        }
        out.extend(self.advance_draw(now_ms, may_seal));
        out
    }

    /// The founder's two steps of the draw, whichever is due.
    fn advance_draw(&mut self, now_ms: u64, may_seal: bool) -> Vec<Send> {
        let Some(key) = self.founder.as_ref().map(|f| f.key.clone()) else { return Vec::new() };
        if self.session.is_some() {
            return Vec::new();
        }
        let next = if !self.seating.sealed() {
            if !may_seal {
                return Vec::new();
            }
            let Some(sealed) = self.seating.sealable(&self.app, now_ms) else { return Vec::new() };
            let mut list = self.current_list();
            list.draw = Some(sealed);
            list
        } else {
            let Some(full) = self.seating.drawable() else { return Vec::new() };
            let table = self.under.table_id;
            let Ok(checked) = crate::table::draw::check(
                &table,
                &full,
                usize::from(MAX_SEATS),
                |b| super::tabletalk::open_lot(b, &table).map_err(|_| "a lot"),
                |b| super::tabletalk::open_opening(b, &table).map_err(|_| "an opening"),
            ) else {
                return Vec::new();
            };
            let Some(drawn) = checked.drawn else { return Vec::new() };
            // Re-seated as the draw says: every player at its drawn seat, a
            // member given back after it opened leaving its seat empty.
            let mut seats: Vec<SeatEntry> = self
                .roster
                .seats()
                .iter()
                .filter_map(|e| drawn.seat_of(&e.app_public_key).map(|s| SeatEntry { seat: s, ..e.clone() }))
                .collect();
            seats.sort_by_key(|e| e.seat);
            let Ok(roster) = Roster::form(seats, &self.under.ad, false) else { return Vec::new() };
            self.roster = roster;
            let mut list = self.current_list();
            list.draw = Some(full);
            list
        };
        self.serial += 1;
        self.ratified.clear();
        self.ratified_bytes.clear();
        self.sent_ready = false;
        self.session = None;
        let list = PlayerList { list_serial: self.serial, ..next };
        let Ok(bytes) = joinwire::publish_player_list(&list, &key, now_ms) else { return Vec::new() };
        self.said.list = Some(bytes.clone());
        let mut out = vec![Send::Broadcast(bytes)];
        out.extend(self.adopt(&list, now_ms).unwrap_or_default());
        out
    }

    /// `D-083`: a member's sealed lot off the table's carriers. The founder
    /// keeps it, and seals once every member's is in and `may_seal`. A member
    /// hearing another member's lot for the first time carries it on at once,
    /// verbatim: a founder that cannot hear that member directly has it through
    /// this one within the moment, not at the next say.
    pub fn on_lot(&mut self, bytes: &[u8], now_ms: u64, may_seal: bool) -> Result<Vec<Send>, super::tabletalk::NotHeard> {
        let roster = &self.roster;
        let (_, said, said_ms) =
            super::tabletalk::receive_lot(bytes, &self.under.table_id, |k| roster.seat_of(k), now_ms)?;
        // Carried on the first time only: a member says its lot again every few
        // seconds under a fresh signature, and carrying every copy would have
        // every member forward a table's worth of words a window.
        let first = !self.seating.has_lot(&said.key);
        let kept = self.seating.hear_lot(&said, bytes, said_ms);
        if kept {
            self.draw_word_ms.insert(said.key, now_ms);
        }
        let mut out = self.carry_on(kept && first, &said.key, bytes);
        out.extend(self.advance_draw(now_ms, may_seal));
        Ok(out)
    }

    /// `D-083`: a member's opening off the table's carriers -- kept by every
    /// client, which is what lets a member refuse a round abandoned over an
    /// opening it holds, and carried on at once like a lot; and the founder
    /// completes the draw once every sealed lot is opened.
    ///
    /// **Taken from any member of the sealed round, seated or not, and whenever
    /// it was said.** An opening is bound to its round and to the sealed lots it
    /// names, so a copy of any age opens nothing else; and a member given back
    /// whose opening the others heard first -- its goodbye overtaking it at the
    /// founder -- must still be able to complete the round, or the founder runs
    /// it again and every member holding that opening refuses it.
    pub fn on_opening(&mut self, bytes: &[u8], now_ms: u64) -> Result<Vec<Send>, super::tabletalk::NotHeard> {
        let said = super::tabletalk::open_opening(bytes, &self.under.table_id)?;
        if self.roster.seat_of(&said.key).is_none() && !self.seating.is_sealed_member(&said.key) {
            return Err(super::tabletalk::NotHeard::NotASeat);
        }
        let kept = self.seating.hear_opening(&said, bytes);
        if kept {
            self.draw_word_ms.insert(said.key, now_ms);
        }
        let mut out = self.carry_on(kept, &said.key, bytes);
        out.extend(self.advance_draw(now_ms, false));
        Ok(out)
    }

    /// `D-083`: when this client last kept a word of the draw from `key`.
    pub fn last_draw_word_ms(&self, key: &[u8; 32]) -> Option<u64> {
        self.draw_word_ms.get(key).copied()
    }

    /// Another member's word, heard here for the first time, said on by a member
    /// -- never by the founder, whose rosters carry what it holds, and never a
    /// copy heard before, so a word goes round each member once.
    fn carry_on(&self, kept: bool, author: &[u8; 32], bytes: &[u8]) -> Vec<Send> {
        if kept && self.founder.is_none() && *author != self.app.verifying_key().to_bytes() {
            vec![Send::Broadcast(bytes.to_vec())]
        } else {
            Vec::new()
        }
    }

    /// `D-083`: what each player of the roster still owes the draw, by key --
    /// the founder's judgement gives back one that holds a round up.
    pub fn draw_owed(&self) -> Vec<([u8; 32], Owed)> {
        let me = self.app.verifying_key().to_bytes();
        self.seating
            .owed()
            .into_iter()
            .filter(|(k, _)| *k != me && self.roster.seat_of(k).is_some())
            .collect()
    }

    /// `D-083`: the round of the draw this client is in, and whether its lots are
    /// sealed and whether it is complete -- for the node's lines and judgement.
    pub fn draw_state(&self) -> (Option<u64>, bool, bool) {
        (self.seating.round(), self.seating.sealed(), self.seating.done().is_some())
    }

    /// `D-083`: the first hand's button -- the drawn seat if a player sits there,
    /// or the first occupied seat above it. `None` without a complete draw.
    pub fn first_button(&self) -> Option<u8> {
        let done = self.seating.done()?;
        let occupied: Vec<u8> = self.roster.seats().iter().map(|e| e.seat).collect();
        crate::table::draw::first_button(done.drawn.button, &occupied)
    }

    /// `D-060`: hold this client's own ratification until its node says it hears
    /// every seat of the roster, or let it go the moment a roster is adopted.
    pub fn hold_ratification(&mut self, hold: bool) {
        self.hold_ready = hold;
    }

    /// `D-060`: the node's word that this client hears every seat of the roster
    /// it holds -- the ratification is made now, if the roster is one this client
    /// is seated in, is not short, and has not been ratified here already.
    pub fn ratify_now(&mut self, now_ms: u64) -> Result<Vec<Send>, Failed> {
        let me = self.app.verifying_key().to_bytes();
        let Some(seat) = self.my_seat.filter(|s| self.roster.seat_of(&me) == Some(*s)) else {
            return Ok(vec![]);
        };
        if self.sent_ready
            || self.serial == 0
            || self.roster.len() < self.floor()
            || self.seating.digest_for(&self.roster).is_none()
        {
            return Ok(vec![]);
        }
        self.ratify(seat, now_ms)
    }

    /// `S1-GI`: whether this client's seat came from its founder's own
    /// acceptance -- given only before the table deals -- rather than from a
    /// roster said to a seat already on it.
    pub fn seated_by_acceptance(&self) -> bool {
        self.admitted
    }

    /// `D-060`: whether this client has ratified the roster it holds.
    pub fn ready_sent(&self) -> bool {
        self.sent_ready
    }

    /// `D-060`: whether the roster this client holds is large enough to be
    /// ratified -- the advert's minimum, or two once the table was set to start.
    pub fn may_start(&self) -> bool {
        self.roster.len() >= self.floor()
    }

    /// `D-060`: the seats whose ratification of the roster this client holds has
    /// been taken here.
    pub fn ratified_seats(&self) -> Vec<u8> {
        self.ratified.keys().copied().collect()
    }

    /// This client's ratification of the roster it holds, from seat `seat`.
    fn ratify(&mut self, seat: u8, now_ms: u64) -> Result<Vec<Send>, Failed> {
        // `S1-CR`: a seat that restarted says the ratification it recorded,
        // verbatim, when the roster the founder said again is the one it
        // ratified -- the same serial, the same seats. A new ratification
        // would be a new `event_hash` and so a session identity nobody else
        // computes. One that does not fit is refused here, this client
        // ratifies anew below, and the node is told.
        if let Some(bytes) = self.recorded_ready.take() {
            match self.take_ratification(&bytes) {
                Ok(()) if self.ratified.contains_key(&seat) => {
                    self.sent_ready = true;
                    self.said.ready = Some(bytes.clone());
                    self.replay_early();
                    return Ok(vec![Send::Broadcast(bytes)]);
                }
                _ => self.recorded_refused = true,
            }
        }
        // `D-060`: this seat's own ratification of this roster, carried back by
        // another seat, is said again as it is -- never a second one.
        if let Some(bytes) = self.ratified_bytes.get(&seat).cloned() {
            self.sent_ready = true;
            self.said.ready = Some(bytes.clone());
            self.settle();
            self.replay_early();
            return Ok(vec![Send::Broadcast(bytes)]);
        }

        // `D-083`: every caller has checked the roster sits as its complete draw
        // put it; a ratification without one is not said.
        let Some(draw) = self.seating.digest_for(&self.roster) else {
            return Ok(vec![]);
        };
        let ready = TableReady {
            roster_hash: self.roster.hash_at_zero(),
            list_serial: self.serial,
            table_params_hash: self.under.params,
            my_seat: seat,
            capability_set: self.capabilities.clone(),
            draw,
        };
        let bytes = joinwire::publish_table_ready(
            &ready,
            &self.under.table_id,
            &self.genesis(),
            &self.app,
            now_ms,
            self.under.ad.join_deadline_ms,
        )?;
        // Counted here rather than on the way back in: this client's own
        // ratification does not travel to itself.
        let (_, _, event_hash) =
            joinwire::receive_table_ready(&bytes, &self.under.table_id, &self.genesis())?;
        self.ratified.insert(seat, event_hash);
        self.sent_ready = true;
        self.said.ready = Some(bytes.clone());
        self.settle();
        // Whatever arrived before this roster did.
        self.replay_early();
        Ok(vec![Send::Broadcast(bytes)])
    }

    /// `S1-CR`: give a formation built for a resume the ratification this
    /// client recorded, to be said again verbatim when the roster fits.
    pub fn with_recorded_ratification(mut self, bytes: Vec<u8>) -> Self {
        self.recorded_ready = Some(bytes);
        self
    }

    /// This client's own `TABLE_READY`, as sent; what the session record
    /// keeps (`S1-CR`).
    pub fn my_ratification(&self) -> Option<&[u8]> {
        self.said.ready.as_deref()
    }

    /// Whether the recorded ratification did not fit the roster the founder
    /// said again, so that this client ratified anew (`S1-CR`).
    pub fn recorded_ratification_refused(&self) -> bool {
        self.recorded_refused
    }

    /// Somebody else's ratification.
    ///
    /// One that does not fit the roster this client holds is **kept**, not
    /// refused: on a mesh the ratification and the roster it ratifies race, and
    /// the loser is usually the roster. See `Formation::early`.
    pub fn on_table_ready(&mut self, bytes: &[u8]) -> Result<Vec<Send>, Failed> {
        match self.take_ratification(bytes) {
            Ok(()) => Ok(vec![]),
            Err(Failed::Ready(_)) => {
                // Held rather than dropped. Everything about it is signed and
                // self-contained, so it is judged again — in full — when the
                // list it names arrives.
                //
                // **But only if it could still become valid.** A ratification
                // naming a serial this client has already passed never will:
                // `admit_ready` compares against the held serial and `adopt`
                // only ever moves forward. Keeping one wasted a slot in a queue
                // of ten and pushed out a genuine early ratification — and the
                // bytes are somebody's real signed event, so a bystander who
                // had merely watched an earlier round could refill the queue
                // from its own recording and hold up the table without a key.
                let stale = joinwire::receive_table_ready(
                    bytes,
                    &self.under.table_id,
                    &self.genesis(),
                )
                .map(|(ready, _, _)| ready.list_serial < self.serial)
                .unwrap_or(true);
                if stale {
                    return Ok(vec![]);
                }
                if self.early.len() >= MAX_SEATS as usize {
                    self.early.pop_front();
                }
                self.early.push_back(bytes.to_vec());
                Ok(vec![])
            }
            Err(e) => Err(e),
        }
    }

    /// One ratification, checked in full against the roster this client holds.
    fn take_ratification(&mut self, bytes: &[u8]) -> Result<(), Failed> {
        let (ready, sender, event_hash) =
            joinwire::receive_table_ready(bytes, &self.under.table_id, &self.genesis())?;
        admit_ready(
            &ready,
            &sender,
            &self.roster,
            self.serial,
            &self.under,
            self.floor(),
            self.seating.digest_for(&self.roster).as_ref(),
        )
        .map_err(Failed::Ready)?;
        // **First copy wins, and a second differing one is named.** This was
        // `insert`, which overwrote.
        match self.ratified.get(&ready.my_seat) {
            Some(first) if *first == event_hash => return Ok(()),
            Some(_) => {
                return Err(Failed::RatifiedTwice {
                    seat: ready.my_seat,
                })
            }
            None => {
                self.ratified.insert(ready.my_seat, event_hash);
                self.ratified_bytes.insert(ready.my_seat, bytes.to_vec());
                // `D-060`: this client's own ratification, made before -- by a
                // life of this client that is gone -- and carried back by another
                // seat. It is this seat's word already: a new one would be a new
                // event hash, and so a session nobody else computes
                // (`churn161800-10`: three seats back at a set table, each at a
                // session of its own).
                if sender == self.app.verifying_key().to_bytes() && !self.sent_ready {
                    self.sent_ready = true;
                    self.said.ready = Some(bytes.to_vec());
                }
            }
        }
        self.settle();
        Ok(())
    }

    /// Judge everything that was waiting for a roster, now that there is one.
    ///
    /// Anything that still does not fit is dropped rather than held again: it
    /// has now been seen against the list it named, and holding it a second time
    /// would be holding it for ever.
    fn replay_early(&mut self) {
        let waiting: Vec<Vec<u8>> = self.early.drain(..).collect();
        for bytes in waiting {
            let _ = self.take_ratification(&bytes);
        }
    }

    /// If every seat has ratified, the table is real and has a `session_id`.
    fn settle(&mut self) {
        if self.ratified.len() != self.roster.len() {
            return;
        }
        if !self
            .roster
            .seats()
            .iter()
            .all(|e| self.ratified.contains_key(&e.seat))
        {
            return;
        }
        // Ascending seat order, which `session_id` asserts rather than sorts:
        // two peers that collected the ratifications as they arrived would
        // otherwise compute two different session identities, and that value is
        // in every subsequent hand's genesis.
        let ratifications: Vec<Ratification> = self
            .ratified
            .iter()
            .map(|(&seat, &event_hash)| Ratification { seat, event_hash })
            .collect();
        self.session = Some(session_id(
            &self.under.table_id,
            &self.under.params,
            &self.roster.hash_at_zero(),
            &ratifications,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::lobby::{BlindSchedule, Mode, DECK_SUITE_V1};
    use crate::protocol::constants::hand_deadline_min_ms;

    const NOW: u64 = 1_700_000_000_000;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn ad(founder: &SigningKey, seats: u8, min: u8) -> TableAd {
        let (action, grace, crypto, delay) = (20_000u32, 5_000u32, 30_000u32, 7_000u32);
        TableAd {
            game: 1,
            mode: Mode::CashPlayMoney.code(),
            preset_id: "CUSTOM".into(),
            table_name: "Riverside".into(),
            small_blind: 10,
            big_blind: 20,
            ante: 0,
            min_buyin: 200,
            max_buyin: 2_000,
            start_stack: 0,
            players: 1,
            max_players: seats,
            min_players_to_start: min,
            blind_schedule: BlindSchedule {
                mode: 1,
                every_n_hands: 20,
                first_small_blind: 10,
                small_blind_cap: 1_000,
            },
            action_timeout_ms: action,
            action_grace_ms: grace,
            crypto_step_timeout_ms: crypto,
            hand_deadline_ms: hand_deadline_min_ms(
                seats,
                action as u64,
                grace as u64,
                crypto as u64,
                delay as u64,
                0,
            ) as u32,
            join_deadline_ms: 120_000,
            hand_delay_ms: delay,
            time_bank_ms: 0,
            button_rule: 1,
            odd_chip_rule: 1,
            showdown_policy: 1,
            password_required: false,
            deck_suite: DECK_SUITE_V1.into(),
            founder_app_key: founder.verifying_key().to_bytes(),
            founder_peer_id: peer(1),
            timestamp_unix_ms: NOW,
            expires_at_unix_ms: NOW + 90_000,

            founder_tox_key: None,
            tox_chat_id: None,
        }
    }

    fn peer(n: u8) -> Vec<u8> {
        let mut v = vec![0u8; 38];
        v[0] = n;
        v
    }

    struct Table {
        founder: Formation,
        joiners: Vec<Formation>,
    }

    fn found(seats: u8, min: u8) -> (Table, Vec<u8>, TableAd, Hash) {
        let app = key(1);
        let table = key(200);
        let a = ad(&app, seats, min);
        let event = super::super::advert::publish(&a, &table).expect("the advert publishes");
        let hash = crate::protocol::transcript::event_hash(
            &crate::protocol::serialization::from_canonical::<
                crate::protocol::messages::SignedEvent,
            >(&event, 8_192)
            .unwrap()
            .body,
        );
        let f = Formation::found(
            app,
            table,
            a.clone(),
            event.clone(),
            hash,
            None,
            peer(1),
            "Alice".into(),
            1_000,
        )
        .expect("the founder seats itself");
        (
            Table {
                founder: f,
                joiners: vec![],
            },
            event,
            a,
            hash,
        )
    }

    /// A whole table, formed: two joiners ask, the founder seats them, every
    /// seat ratifies, and all three agree on one `session_id`.
    ///
    /// Nothing in this crate had ever run the formation dance end to end. Every
    /// rule was tested alone and every message had a wire form, and the first
    /// time the three were put in a room together this test is what found out
    /// whether they fitted.
    #[test]
    fn three_clients_form_one_table() {
        let (mut t, event, a, hash) = found(6, 3);
        let table_id = t.founder.table_id();

        for (n, seed) in [(2u8, 2u8), (3, 3)] {
            let (mut j, request) = Formation::join(
                key(seed),
                a.clone(),
                hash,
                table_id,
                peer(n),
                format!("player {n}"),
                1_000,
                None,
                None,
                [seed; 32],
                NOW,
                None,
            )
            .expect("the request builds");

            let out = t
                .founder
                .on_join_request(&request, &peer(n), false, NOW)
                .expect("the founder seats an honest joiner");

            // The reply to the joiner, then every broadcast to every seat --
            // the joiners already seated included -- until the table is quiet.
            // At three seats that is the seating draw (`D-083`): the lots, the
            // sealed roster, the openings, the re-seated roster, and every
            // ratification of it.
            deliver(&mut t, &mut j, out, table_id, NOW);

            t.joiners.push(j);
            let _ = &event;
        }

        assert_eq!(t.founder.roster().len(), 3, "three seats");
        let session = t.founder.session().expect("the founder has a session");
        for j in &t.joiners {
            assert_eq!(
                j.session(),
                Some(session),
                "a seat computed a different session identity"
            );
        }
        // `D-083`: every seat took the same complete draw, holds the roster it
        // re-seated, and names the same first button.
        let button = t.founder.first_button().expect("the draw is complete");
        for j in &t.joiners {
            assert_eq!(j.first_button(), Some(button), "a seat names another button");
            assert_eq!(j.roster().hash_at_zero(), t.founder.roster().hash_at_zero());
        }
        // **The table's blind schedule reaches the hand**, so that hand `k+1`
        // can derive its own level instead of copying hand `k`'s.
        //
        // This is the wiring that was missing, and its absence is why
        // `RATED_SNG_POKERTH_V1`'s doubling never happened in play: the formula
        // in `poker::tournament` was right and tested, `Opening.level` was set
        // to 1 at formation and copied forward for ever, and nothing in between
        // carried the three parameters the boundary needed to compute anything.
        // A unit that is right and unreachable tests green.
        //
        // All three are parts of `table_params_hash`, so they are the table's
        // and signed — which is what makes every peer's answer the same answer.
        let o = crate::table::hand::Opening::from_formation(&t.founder, 1)
            .expect("a ratified table can open hand one");
        assert_eq!(o.every_n_hands, a.blind_schedule.every_n_hands);
        assert_eq!(o.first_small_blind, a.blind_schedule.first_small_blind);
        assert_eq!(o.small_blind_cap, a.blind_schedule.small_blind_cap);
        assert_eq!(
            o.small_blind, o.first_small_blind,
            "§7.2 rule 2 forces small_blind == first_small_blind, so hand one is level one"
        );
        assert_eq!(o.big_blind, 2 * o.small_blind);
        assert_eq!(o.level, 1);

    }

    /// A joiner names the copy of the advertisement **it** heard, and a founder
    /// re-broadcasts every thirty seconds under a new timestamp and therefore a
    /// new hash.
    ///
    /// This is the defect the first two-instance run found. Everything passed in
    /// memory and in the wire test, because both formed a table faster than the
    /// first re-broadcast; on two real processes the joiner heard the second
    /// copy, named it, and was refused with *"the advertisement has expired"* —
    /// which was wrong, unfixable from the joiner's side, and invisible to every
    /// test that finished inside thirty seconds.
    #[test]
    fn a_joiner_may_name_any_copy_the_founder_signed() {
        let (mut t, _, a, hash) = found(6, 2);
        let table_id = t.founder.table_id();

        // The joiner keeps the copy it first heard.
        let (mut j, request) = Formation::join(
            key(2),
            a,
            hash,
            table_id,
            peer(2),
            "two".into(),
            1_000,
            None,
            None,
            [2u8; 32],
            NOW,
            None,
        )
        .unwrap();

        // The founder re-broadcasts twice before the request arrives.
        let later = t.founder.readvertise(NOW + 30_000, 90_000).unwrap();
        let newest = t.founder.readvertise(NOW + 60_000, 90_000).unwrap();
        let (_, later_hash) = super::super::advert::verify_echoed(&later).unwrap();
        let (_, newest_hash) = super::super::advert::verify_echoed(&newest).unwrap();
        assert_ne!(later_hash, hash, "a re-broadcast is a different hash");
        assert_ne!(newest_hash, later_hash);

        let out = t
            .founder
            .on_join_request(&request, &peer(2), false, NOW + 61_000)
            .expect("a request naming an older copy is still honest");
        let Send::Reply(reply) = &out[0] else {
            panic!("an acceptance is a reply")
        };
        assert!(
            joinwire::receive_join_accept(reply).is_ok(),
            "the founder refused a copy it signed itself"
        );

        // And the echo is the copy the joiner asked under, not the newest, or
        // the joiner's own check on the echo would fail.
        j.on_join_answer(reply, NOW + 61_000)
            .expect("the acceptance holds at the joiner");
        assert_eq!(j.my_seat(), Some(1));
    }

    /// But not for ever. An advertisement older than any receiver may still hold
    /// is a hash nobody can legitimately name, and keeping every copy a
    /// long-lived table ever signed is an unbounded list fed by a timer.
    #[test]
    fn a_copy_older_than_any_receiver_holds_is_forgotten() {
        let (mut t, _, a, hash) = found(6, 2);
        let table_id = t.founder.table_id();
        let (_, request) = Formation::join(
            key(2),
            a,
            hash,
            table_id,
            peer(2),
            "two".into(),
            1_000,
            None,
            None,
            [2u8; 32],
            NOW,
            None,
        )
        .unwrap();

        let stale = NOW + crate::protocol::constants::MAX_AD_LIFETIME_MS + 1_000;
        t.founder.readvertise(stale, 90_000).unwrap();

        let out = t.founder.on_join_request(&request, &peer(2), false, stale).unwrap();
        let Send::Reply(reply) = &out[0] else {
            panic!("a refusal is a reply")
        };
        let (_, reason, _, _) = joinwire::receive_join_reject(reply).unwrap();
        assert_eq!(reason, RejectReason::AdvertExpired.code());
    }

    /// A re-broadcast moves the timestamp and the seat count and **nothing
    /// else**: the timestamps are what §7.2 rule 6 compares and the parameters
    /// are what rule 7 compares, so a re-broadcast that changed one would mark
    /// this table unjoinable at every receiver that had it.
    #[test]
    fn a_rebroadcast_changes_only_what_it_may() {
        let (mut t, _, _, _) = found(6, 2);
        let before = t.founder.under_params();
        t.founder.readvertise(NOW + 30_000, 90_000).unwrap();
        assert_eq!(
            before,
            t.founder.under_params(),
            "a re-broadcast changed a parameter and unjoined its own table"
        );
    }

    /// A ratification that overtakes the roster it ratifies is held and judged
    /// when the roster arrives, not refused.
    ///
    /// This is the second defect the two-instance run found, and it is a
    /// property of the medium rather than of anybody's code: GossipSub does not
    /// order two messages against each other, and the founder sends the roster
    /// and its own ratification of it in the same breath. Refusing the early one
    /// left both sides waiting for each other for ever.
    #[test]
    fn a_ratification_that_arrives_first_is_kept() {
        let (mut t, _, a, hash) = found(6, 2);
        let table_id = t.founder.table_id();

        let (mut j, request) = Formation::join(
            key(2),
            a,
            hash,
            table_id,
            peer(2),
            "two".into(),
            1_000,
            None,
            None,
            [2u8; 32],
            NOW,
            None,
        )
        .unwrap();

        // `D-083`: the seating draw runs first, and its last step is the one
        // this test is about -- the founder says the re-seated roster and
        // ratifies it in one breath. Those two are held back here.
        let out = t.founder.on_join_request(&request, &peer(2), false, NOW).unwrap();
        let mut queue: VecDeque<(usize, Vec<u8>)> = broadcasts(out, 0, Some(&mut j), NOW).into();
        let (mut list, mut ready) = (None, None);
        while let Some((from, bytes)) = queue.pop_front() {
            if from == 0 {
                let complete = joinwire::receive_player_list(&bytes)
                    .is_ok_and(|(l, _)| l.draw.is_some_and(|d| d.openings.len() == d.lots.len()));
                if complete {
                    list = Some(bytes);
                    continue;
                }
                if joinwire::receive_table_ready(&bytes, &table_id, &t.founder.genesis()).is_ok() {
                    ready = Some(bytes);
                    continue;
                }
            }
            let who: &mut Formation = if from == 0 { &mut j } else { &mut t.founder };
            for s in deliver_one(who, &bytes, NOW) {
                if let Send::Broadcast(b) = s {
                    queue.push_back((1 - from, b));
                }
            }
        }
        let list = list.expect("a re-seated roster is announced");
        let ready = ready.expect("the founder ratifies its own roster");

        // The wrong way round, on purpose.
        j.on_table_ready(&ready)
            .expect("an early ratification is not an error");
        assert_eq!(j.session(), None, "nothing is settled without a roster");

        for send in j.on_player_list(&list, NOW).unwrap() {
            if let Send::Broadcast(b) = send {
                t.founder.on_table_ready(&b).unwrap();
            }
        }

        assert!(
            j.session().is_some(),
            "the held ratification was never judged"
        );
        assert_eq!(
            j.session(),
            t.founder.session(),
            "the two seats disagree about the session"
        );
    }

    /// A `JOIN_ACCEPT` overtaken by a newer roster must not rewind it.
    ///
    /// The founder answers a join with `roster_so_far` — the table as it stood
    /// when it said yes — and broadcasts a `PLAYER_LIST` in the same turn. Their
    /// order at the joiner is not fixed, and `Formation::table` says so. When a
    /// later seat's list wins the race, the reply that follows it is *older*
    /// than what the joiner already holds, and it carries no `list_serial` to
    /// say so.
    ///
    /// This is written as the failing peer saw it: adopt the full roster, then
    /// deliver the stale accept. Before the fix the roster fell to `my_seat + 1`
    /// and stayed there, because `serial` was untouched and every rebroadcast of
    /// the founder's last list was then refused as `NotNewer`.
    #[test]
    fn a_late_accept_does_not_rewind_a_roster() {
        let (mut t, _, a, hash) = found(6, 2);
        let table_id = t.founder.table_id();

        let join_as = |seed: u8, name: &str| {
            Formation::join(
                key(seed),
                a.clone(),
                hash,
                table_id,
                peer(seed),
                name.into(),
                1_000,
                None,
                None,
                [seed; 32],
                NOW,
                None,
            )
            .unwrap()
        };

        let (mut two, req2) = join_as(2, "two");
        let (_three, req3) = join_as(3, "three");

        // The founder seats `two`. Its reply is held back, unsent, exactly as a
        // slow round trip holds it.
        let mut stale_accept = None;
        for send in t.founder.on_join_request(&req2, &peer(2), false, NOW).unwrap() {
            if let Send::Reply(b) = send {
                stale_accept = Some(b);
            }
        }
        let stale_accept = stale_accept.expect("the founder answers a join");

        // Meanwhile `three` is seated too, so the founder's next list is newer
        // and larger than the snapshot inside that unsent reply.
        let mut newer_list = None;
        for send in t.founder.on_join_request(&req3, &peer(3), false, NOW).unwrap() {
            if let Send::Broadcast(b) = send {
                if joinwire::receive_player_list(&b).is_ok() {
                    newer_list = Some(b);
                }
            }
        }
        let newer_list = newer_list.expect("a roster is announced for the second joiner");

        // The list wins the race, which is the ordinary case on a fast network.
        two.on_player_list(&newer_list, NOW).unwrap();
        let full = two.roster().seats().len();
        assert_eq!(full, 3, "the founder and both joiners");

        // And only now the reply arrives, carrying the two-seat table.
        two.on_join_answer(&stale_accept, NOW)
            .expect("a late accept is not an error");

        assert_eq!(
            two.roster().seats().len(),
            full,
            "the late accept rewound the roster to its own snapshot"
        );
        assert_eq!(
            two.roster().seat_of(&key(3).verifying_key().to_bytes()),
            Some(2),
            "the seat that was seated after the accept was dropped from the roster"
        );
    }

    /// What is held is bounded, and what has been judged is not held again.
    #[test]
    fn nothing_is_held_for_ever() {
        let (mut t, _, _, _) = found(6, 2);
        // Twenty ratifications of a roster that will never exist.
        let junk = {
            let ready = TableReady {
                roster_hash: [1u8; 32],
                list_serial: 99,
                table_params_hash: t.founder.under_params(),
                my_seat: 3,
                capability_set: vec![],
                draw: [0x5d; 32],
            };
            joinwire::publish_table_ready(
                &ready,
                &t.founder.table_id(),
                &t.founder.genesis(),
                &key(5),
                NOW,
                0,
            )
            .unwrap()
        };
        for _ in 0..20 {
            t.founder.on_table_ready(&junk).unwrap();
        }
        assert!(t.founder.held_early() <= MAX_SEATS as usize);
    }

    /// Below the minimum nobody ratifies, so there is no session and no table.
    /// A client that started dealing on a two-seat roster advertised as needing
    /// three would be dealing a game nobody agreed to.
    #[test]
    fn a_table_below_its_minimum_never_becomes_real() {
        let (mut t, _, a, hash) = found(6, 3);
        let table_id = t.founder.table_id();

        let (mut j, request) = Formation::join(
            key(2),
            a,
            hash,
            table_id,
            peer(2),
            "player 2".into(),
            1_000,
            None,
            None,
            [2u8; 32],
            NOW,
            None,
        )
        .unwrap();
        for s in t.founder.on_join_request(&request, &peer(2), false, NOW).unwrap() {
            match s {
                Send::Reply(b) => {
                    j.on_join_answer(&b, NOW).unwrap();
                }
                Send::Broadcast(b) => {
                    if joinwire::receive_player_list(&b).is_ok() {
                        assert!(
                            j.on_player_list(&b, NOW).unwrap().is_empty(),
                            "a seat ratified a roster too small to start"
                        );
                    }
                }
            }
        }
        assert_eq!(t.founder.roster().len(), 2);
        assert_eq!(t.founder.session(), None);
        assert_eq!(j.session(), None);
    }

    /// One node may not hold two seats at one table — `U17`, checked against
    /// what the transport authenticated rather than against what the request
    /// claims.
    /// A seat given back before the first hand is free for somebody else.
    ///
    /// **`S1-M`, and it is the owner's case.** A player sits down at a
    /// tournament table and leaves before it fills. Measured before this
    /// existed: six seats, one leaving cleanly at 90 s, the replacement told
    /// *"the table is full"* at 95 s and again at 106 s, and the founder then
    /// dealing hand 1 to all six including the one that had gone.
    ///
    /// The four things that have to be true, and each of them was false:
    /// the seat leaves the roster, the serial moves so every prior ratification
    /// is void, a fresh list says so, and somebody else can take the number.
    /// `S1-T`: a stranger cannot join a table that has dealt a hand, and a seat
    /// that is already on it still can be answered.
    ///
    /// `STATE_MACHINE.md` §9.4 forbids adding a seat after `Seating` and
    /// §3.1 fixes the roster for the life of the table, and **nothing enforced
    /// either**. The only barrier was `TableFull`, which never fires on a table
    /// that started at `min_players_to_start` below `max_players` — the shape
    /// this client's own founder path creates by default. Accepting a late join
    /// runs `serial += 1; ratified.clear(); session = None`, so a tournament in
    /// progress loses its ratification and its session identity.
    ///
    /// **Both halves are asserted**, because refusing everybody would have been
    /// the easy version and would have silently undone `S1-J`: a seat already on
    /// the roster is answered with `AlreadySeated` and a fresh list, exactly as
    /// before.
    #[test]
    fn a_stranger_cannot_join_a_table_that_has_started_and_a_seat_still_can() {
        let (mut t, _, a, hash) = found(6, 2);
        let table_id = t.founder.table_id();

        // A seat takes its place while the table is still forming.
        let (_, early) = Formation::join(
            key(2),
            a.clone(),
            hash,
            table_id,
            peer(2),
            "early".into(),
            1_000,
            None,
            None,
            [2u8; 32],
            NOW,
            None,
        )
        .unwrap();
        t.founder
            .on_join_request(&early, &peer(2), false, NOW)
            .expect("a forming table seats it");
        let seated = t.founder.roster().len();
        let serial = t.founder.serial();

        // The table deals. A stranger asks anyway — its advert is stale but it
        // is well formed, which is exactly the case that had no barrier.
        let (_, stranger) = Formation::join(
            key(7),
            a.clone(),
            hash,
            table_id,
            peer(7),
            "stranger".into(),
            1_000,
            None,
            None,
            [7u8; 32],
            NOW + 1,
            None,
        )
        .unwrap();
        let out = t
            .founder
            .on_join_request(&stranger, &peer(7), true, NOW + 1)
            .expect("it is answered rather than dropped");
        assert!(
            out.iter().all(|s| matches!(s, Send::Reply(_))),
            "a refusal is a reply and nothing else; a broadcast here would be a roster change"
        );
        assert_eq!(
            t.founder.roster().len(),
            seated,
            "the roster did not move"
        );
        assert_eq!(
            t.founder.serial(),
            serial,
            "and neither did the serial, so nothing was un-ratified"
        );

        // The seat that is already there is not a stranger, and `S1-J`'s answer
        // still reaches it.
        let (_, again) = Formation::join(
            key(2),
            a.clone(),
            hash,
            table_id,
            peer(2),
            "early".into(),
            1_000,
            None,
            None,
            [2u8; 32],
            NOW + 2,
            None,
        )
        .unwrap();
        let out = t
            .founder
            .on_join_request(&again, &peer(2), true, NOW + 2)
            .expect("a seated peer is answered");
        assert!(
            out.iter().any(|s| matches!(s, Send::Broadcast(b) if joinwire::receive_player_list(b).is_ok())),
            "with a freshly signed roster, which is what S1-J exists for"
        );
    }

    /// `S1-P`: the repeat has to be **new bytes**, or GossipSub will not carry
    /// it.
    ///
    /// `publish` takes its message id from the content and holds it for 120
    /// seconds, so a byte-identical repeat is refused on the sender's side for
    /// the whole window a late-arriving peer needs it in. Measured before the
    /// fix, on every node of a run: `said 0 message(s) again, and Duplicate`.
    ///
    /// The assertion is on the **bytes**, not on the content: it is the bytes
    /// GossipSub hashes, and a version of this that compared the decoded rosters
    /// would pass while the mechanism stayed dead.
    #[test]
    fn a_founder_signs_the_roster_again_rather_than_repeating_it() {
        let (mut t, _, a, hash) = found(6, 2);
        let table_id = t.founder.table_id();
        let (mut joiner, req) = Formation::join(
            key(2),
            a.clone(),
            hash,
            table_id,
            peer(2),
            "joiner".into(),
            1_000,
            None,
            None,
            [2u8; 32],
            NOW,
            None,
        )
        .unwrap();
        let out = t
            .founder
            .on_join_request(&req, &peer(2), false, NOW)
            .expect("the seat is given");
        deliver(&mut t, &mut joiner, out, table_id, NOW);

        let first = t.founder.say_again(NOW + 1);
        let again = t.founder.say_again(NOW + 2);
        let list_one = first.first().expect("a founder says the roster");
        let list_two = again.first().expect("and says it again");
        assert!(
            joinwire::receive_player_list(list_one).is_ok()
                && joinwire::receive_player_list(list_two).is_ok(),
            "both are lists"
        );
        assert_ne!(
            list_one, list_two,
            "signed again at the moment of saying, so GossipSub sees a message it has not carried"
        );

        // And a seat that is not the founder cannot do this: a `PLAYER_LIST` is
        // signed under the table key. Its repeat is the stored bytes, which is
        // why `S1-P` stays open for the ratification half.
        let joiner_says = joiner.say_again(NOW + 3);
        assert!(
            !joiner_says.is_empty(),
            "the joiner has something to repeat: it took the roster and ratified it"
        );
        assert_eq!(
            joiner.say_again(NOW + 4),
            joiner_says,
            "a seat with no table key repeats what it has, byte for byte"
        );
    }

    /// **A seat repeats every ratification it holds, not only its own.**
    ///
    /// `S1-P`'s residue. A `TABLE_READY` must arrive verbatim or not at all —
    /// `emitted_at_unix_ms` is inside `EventBody`, so a re-signature is a
    /// different `event_hash` and a different `session_id` — and GossipSub
    /// refuses a byte-identical repeat for 120 s. So the one repair a seat can
    /// make is the one the mesh will not carry, and until this the *only* peer
    /// that could re-send seat 3's ratification was seat 3.
    ///
    /// That was the shape measured in `run205233-10`: seat 9 held
    /// `ratified 1/10` — one, its own — for six and a half minutes while the
    /// table played twenty-seven hands without it.
    ///
    /// Now every seat that heard a ratification carries it, and D-019's
    /// amendment of 2026-09-02 puts these on the group, where no duplicate
    /// cache applies. The assertion is that the founder's repeat contains the
    /// **joiner's** bytes: a copy the founder did not author and could not
    /// re-sign.
    #[test]
    fn a_seat_repeats_a_ratification_it_did_not_make() {
        // Three seats, the draw and all (`D-083`: nobody sits down after it, so
        // the seat to be repaired is at the table from the start); the second
        // joiner never hears the first joiner's ratification.
        let (mut t, _, a, hash) = found(6, 3);
        let table_id = t.founder.table_id();
        for (n, seed) in [(2u8, 2u8), (3, 3)] {
            let (mut j, request) = Formation::join(
                key(seed),
                a.clone(),
                hash,
                table_id,
                peer(n),
                format!("player {n}"),
                1_000,
                None,
                None,
                [seed; 32],
                NOW,
                None,
            )
            .unwrap();
            let out = t.founder.on_join_request(&request, &peer(n), false, NOW).expect("the seat is given");
            let first = broadcasts(out, 0, Some(&mut j), NOW);
            t.joiners.push(j);
            let mut everyone: Vec<&mut Formation> = vec![&mut t.founder];
            everyone.extend(t.joiners.iter_mut());
            spread_losing(&mut everyone, first, NOW, &|from, to, b| {
                from == 1 && to == 2 && kind(b) == Some(crate::protocol::messages::EventType::TableReady)
            });
        }
        let session = t.founder.session().expect("the founder heard every seat");
        assert_eq!(t.joiners[0].session(), Some(session));
        assert_eq!(t.joiners[1].session(), None, "the second joiner misses one ratification");
        let theirs = t.joiners[0].my_ratification().expect("the joiner ratified").to_vec();

        let repeat = t.founder.say_again(NOW + 1);
        assert!(
            repeat.iter().any(|b| b == &theirs),
            "the founder must repeat the joiner's ratification byte for byte; it holds {} message(s)",
            repeat.len()
        );

        // **Byte for byte, and that is the whole point.** A re-signature would
        // be a different `event_hash`, so the far end would compute a different
        // `session_id` from everybody else and `take_ratification` would name an
        // honest seat as `RatifiedTwice`.
        let carried = repeat.iter().find(|b| *b == &theirs).unwrap();
        assert_eq!(carried, &theirs);

        // And the seat that never heard the joiner is repaired by it: a relayed
        // ratification is accepted like any other, since it is signed by its
        // author, and the seat settles on the table's session.
        assert!(t.joiners[1].on_table_ready(&theirs).is_ok());
        assert_eq!(t.joiners[1].session(), Some(session), "repaired by the founder's copy");
    }

    /// `D-083`: the ratifications a client carries are those of the roster it
    /// holds -- a roster that changes takes them all with it. Kept, a seat's old
    /// copy outlived the player in it, and a client the draw put in that seat
    /// said another player's word on an older roster as its own.
    #[test]
    fn a_ratification_of_an_older_roster_is_never_said_again() {
        let (mut t, _, _) = form_three();
        let old: Vec<Vec<u8>> = t
            .founder
            .say_again(NOW)
            .into_iter()
            .filter(|b| kind(b) == Some(crate::protocol::messages::EventType::TableReady))
            .collect();
        assert_eq!(old.len(), 3, "every seat's ratification of the set roster");
        t.founder.release_seat_before_the_first_hand(&peer(3), NOW + 1_000).expect("given back");
        let said = t.founder.say_again(NOW + 1_001);
        assert!(said.iter().all(|b| !old.contains(b)), "a ratification of the roster before was said again");
    }

    #[test]
    fn a_seat_given_back_before_the_first_hand_can_be_taken_by_somebody_else() {
        let (mut t, _, a, hash) = found(6, 2);
        let table_id = t.founder.table_id();

        let (_, req) = Formation::join(
            key(2),
            a.clone(),
            hash,
            table_id,
            peer(2),
            "leaver".into(),
            1_000,
            None,
            None,
            [2u8; 32],
            NOW,
            None,
        )
        .unwrap();
        t.founder
            .on_join_request(&req, &peer(2), false, NOW)
            .expect("the seat is given");
        let took = t.founder.roster().len();
        let serial_before = t.founder.serial();
        let seat = t
            .founder
            .roster()
            .seats()
            .iter()
            .find(|e| e.peer_id == peer(2))
            .map(|e| e.seat)
            .expect("the leaver is seated");

        let out = t
            .founder
            .release_seat_before_the_first_hand(&peer(2), NOW + 1)
            .expect("the founder may give a seat back");

        assert_eq!(t.founder.roster().len(), took - 1, "the seat left the roster");
        assert!(
            t.founder.serial() > serial_before,
            "the serial moves, so every ratification naming the old one is void"
        );
        assert!(
            out.iter().any(|s| matches!(s, Send::Broadcast(b) if joinwire::receive_player_list(b).is_ok())),
            "and a fresh list says so"
        );

        // The number is free, and the point of freeing it is that somebody
        // else can have it.
        let (_, replacement) = Formation::join(
            key(7),
            a,
            hash,
            table_id,
            peer(7),
            "replacement".into(),
            1_000,
            // The seat the leaver gave back, asked for by number.
            Some(seat),
            None,
            [7u8; 32],
            NOW + 2,
            None,
        )
        .unwrap();
        t.founder
            .on_join_request(&replacement, &peer(7), false, NOW + 2)
            .expect("the freed seat is given to the next player");
        assert_eq!(t.founder.roster().len(), took, "the table is full again");
        assert!(
            t.founder
                .roster()
                .seats()
                .iter()
                .any(|e| e.peer_id == peer(7) && e.seat == seat),
            "and the replacement has the number the leaver gave back"
        );
    }

    /// `S1-GB`: a list the founder said before a joiner sat down, carried to it
    /// late behind its acceptance, is newer than the nothing an acceptance holds
    /// and does not name it -- and gives nothing back.
    #[test]
    fn a_list_said_before_a_joiner_sat_down_is_no_seat_given_back() {
        let (mut t, _, a, hash) = found(6, 3);
        let table_id = t.founder.table_id();
        let mut lists = Vec::new();
        let mut answers = Vec::new();
        for (n, seed) in [(2u8, 2u8), (3, 3)] {
            let (j, request) = Formation::join(
                key(seed),
                a.clone(),
                hash,
                table_id,
                peer(n),
                format!("player {n}"),
                1_000,
                None,
                None,
                [seed; 32],
                NOW,
                None,
            )
            .unwrap();
            for s in t.founder.on_join_request(&request, &peer(n), false, NOW).expect("seated") {
                match s {
                    Send::Reply(bytes) => answers.push(bytes),
                    Send::Broadcast(bytes) if joinwire::receive_player_list(&bytes).is_ok() => lists.push(bytes),
                    Send::Broadcast(_) => {}
                }
            }
            t.joiners.push(j);
        }
        // The second joiner hears its acceptance, then the list said before it
        // sat down, carried late, then its own.
        let mut second = t.joiners.pop().expect("the second joiner");
        second.on_join_answer(&answers[1], NOW).expect("the acceptance holds");
        second.on_player_list(&lists[0], NOW + 1).expect("an older list is newer than nothing here");
        assert!(!second.released_before_the_first_hand(), "a list from before its sitting gives nothing back");
        second.on_player_list(&lists[1], NOW + 2).expect("its own list");
        assert!(!second.released_before_the_first_hand());
        assert!(second.roster().seat_of(&key(3).verifying_key().to_bytes()).is_some());
        assert!(second.seated_by_acceptance(), "S1-GI: its founder admitted it");
    }

    /// `S1-GI`: a seat learnt from a roster said to a seat already on it is not
    /// one its founder admitted -- a founder says that roster at a table in play
    /// too -- and a seat its founder accepted is.
    #[test]
    fn a_seat_known_from_a_roster_is_not_one_its_founder_admitted() {
        let (mut t, _, a, hash) = found(6, 3);
        let table_id = t.founder.table_id();
        let ask = |at: u64| {
            Formation::join(key(2), a.clone(), hash, table_id, peer(2), "player 2".into(), 1_000, None, None, [2; 32], at, None)
                .unwrap()
        };
        let (mut first, request) = ask(NOW);
        let sends = t.founder.on_join_request(&request, &peer(2), false, NOW).expect("seated");
        let accept = sends.iter().find_map(|s| match s {
            Send::Reply(bytes) => Some(bytes.clone()),
            _ => None,
        });
        first.on_join_answer(&accept.expect("an answer"), NOW).expect("the acceptance holds");
        assert!(first.seated_by_acceptance());

        // The same player asks again from a fresh client: already seated, and
        // the roster, from which it knows its seat.
        let (mut again, request) = ask(NOW + 10);
        let sends = t.founder.on_join_request(&request, &peer(2), false, NOW + 10).expect("answered");
        let mut list = None;
        for s in sends {
            match s {
                Send::Reply(bytes) => {
                    assert!(matches!(again.on_join_answer(&bytes, NOW + 10), Err(Failed::Refused { .. })), "already seated");
                }
                Send::Broadcast(bytes) if joinwire::receive_player_list(&bytes).is_ok() => list = Some(bytes),
                Send::Broadcast(_) => {}
            }
        }
        again.on_player_list(&list.expect("the roster again"), NOW + 11).expect("the roster names it");
        assert_eq!(again.my_seat(), first.my_seat());
        assert!(!again.seated_by_acceptance(), "a roster is no admission");
    }

    /// `S1-DV`: a joiner whose seat the founder gave back learns it from the
    /// roster said again -- named by the one it held, not by the next -- and
    /// the node takes it to the lobby. A first list that does not name a
    /// joiner still joining says nothing, as before.
    #[test]
    fn a_joiner_given_back_before_the_first_hand_knows_it_from_the_next_roster() {
        let (mut t, _, a, hash) = found(6, 2);
        let table_id = t.founder.table_id();
        let (mut j, req) = Formation::join(
            key(2),
            a.clone(),
            hash,
            table_id,
            peer(2),
            "leaver".into(),
            1_000,
            None,
            None,
            [2u8; 32],
            NOW,
            None,
        )
        .unwrap();
        let out = t
            .founder
            .on_join_request(&req, &peer(2), false, NOW)
            .expect("the seat is given");
        let mut list = None;
        for s in out {
            match s {
                Send::Reply(bytes) => {
                    j.on_join_answer(&bytes, NOW).expect("the acceptance holds");
                }
                Send::Broadcast(bytes) => {
                    if joinwire::receive_player_list(&bytes).is_ok() {
                        list = Some(bytes);
                    }
                }
            }
        }
        let list = list.expect("a roster change is announced");
        j.on_player_list(&list, NOW).expect("the list holds");
        assert!(j.my_seat().is_some(), "seated");
        assert!(!j.released_before_the_first_hand(), "named by this roster");
        assert_eq!(j.founder_peer_id(), peer(1).as_slice(), "the founder is the advert's");

        // A list arriving at a joiner that is still joining, which does not
        // name it: nothing to complain about.
        let (mut late, _) = Formation::join(
            key(3),
            a.clone(),
            hash,
            table_id,
            peer(3),
            "late".into(),
            1_000,
            None,
            None,
            [3u8; 32],
            NOW,
            None,
        )
        .unwrap();
        late.on_player_list(&list, NOW + 1)
            .expect("a list naming a table this client is still joining holds");
        assert!(!late.released_before_the_first_hand(), "never named, so never given back");

        // The founder gives the seat back, and the roster it says again drops it.
        let out = t
            .founder
            .release_seat_before_the_first_hand(&peer(2), NOW + 2)
            .expect("the founder may give a seat back");
        let again = out
            .into_iter()
            .find_map(|s| match s {
                Send::Broadcast(b) if joinwire::receive_player_list(&b).is_ok() => Some(b),
                _ => None,
            })
            .expect("a fresh list says so");
        j.on_player_list(&again, NOW + 2).expect("the list holds");
        assert!(j.released_before_the_first_hand(), "named before and not now: given back");
    }

    /// The founder's answer to one step, delivered: the reply to `j`, the list
    /// to every joiner and to `j`, and every ratification to every other seat.
    /// `D-083`: one broadcast into one client, by the door its type names -- a
    /// roster, a ratification, a sealed lot or an opening. A refusal is dropped:
    /// tests that care about one ask for it themselves.
    fn deliver_one(who: &mut Formation, bytes: &[u8], now: u64) -> Vec<Send> {
        use crate::protocol::messages::EventType;
        match crate::net::chained::peek(bytes, 65_536).map(|(k, _, _)| k) {
            Ok(EventType::PlayerList) => who.on_player_list(bytes, now).unwrap_or_default(),
            Ok(EventType::TableReady) => who.on_table_ready(bytes).unwrap_or_default(),
            Ok(EventType::RngCommit) => who.on_lot(bytes, now, true).unwrap_or_default(),
            Ok(EventType::RngReveal) => who.on_opening(bytes, now).unwrap_or_default(),
            _ => vec![],
        }
    }

    /// `D-083`: every broadcast reaches every other client, and whatever they
    /// say in answer goes round too, until nothing more is said; between rounds
    /// the founder -- `everyone[0]` -- ticks its draw, which seals the lots and
    /// completes the draw when it can. A table forms in here the way it forms on
    /// the wire, draw and all.
    fn spread(everyone: &mut [&mut Formation], first: Vec<(usize, Vec<u8>)>, now: u64) {
        spread_losing(everyone, first, now, &|_, _, _| false)
    }

    /// The type of a signed word, for a test that loses some of them.
    fn kind(bytes: &[u8]) -> Option<crate::protocol::messages::EventType> {
        crate::net::chained::peek(bytes, 65_536).ok().map(|(k, _, _)| k)
    }

    /// `spread`, with every word `lost(from, to, bytes)` names never reaching
    /// `to` -- a seat that does not hear another, or a player that says nothing.
    fn spread_losing(
        everyone: &mut [&mut Formation],
        first: Vec<(usize, Vec<u8>)>,
        now: u64,
        lost: &dyn Fn(usize, usize, &[u8]) -> bool,
    ) {
        let mut queue: VecDeque<(usize, Vec<u8>)> = first.into();
        let mut steps = 0u32;
        loop {
            while let Some((from, bytes)) = queue.pop_front() {
                for (i, who) in everyone.iter_mut().enumerate() {
                    if i == from || lost(from, i, &bytes) {
                        continue;
                    }
                    for s in deliver_one(who, &bytes, now) {
                        if let Send::Broadcast(b) = s {
                            queue.push_back((i, b));
                        }
                    }
                }
                steps += 1;
                assert!(steps < 10_000, "the formation never went quiet");
            }
            let ticked = everyone[0].draw_tick(now, true);
            if ticked.is_empty() {
                break;
            }
            for s in ticked {
                if let Send::Broadcast(b) = s {
                    queue.push_back((0, b));
                }
            }
        }
    }

    /// The broadcasts of `out`, from client `from`, with any reply handed to `j`.
    fn broadcasts(out: Vec<Send>, from: usize, j: Option<&mut Formation>, now: u64) -> Vec<(usize, Vec<u8>)> {
        let mut first = Vec::new();
        let mut j = j;
        for s in out {
            match s {
                Send::Reply(bytes) => {
                    if let Some(j) = j.as_deref_mut() {
                        let _ = j.on_join_answer(&bytes, now);
                    }
                }
                Send::Broadcast(b) => first.push((from, b)),
            }
        }
        first
    }

    /// The founder's answer `out` to `j`'s request, carried: the reply to `j`,
    /// and every broadcast to every seat -- the draw's words and rosters among
    /// them -- until the table goes quiet (`spread`).
    fn deliver(t: &mut Table, j: &mut Formation, out: Vec<Send>, table_id: Hash, now: u64) {
        let _ = table_id;
        let first = broadcasts(out, 0, Some(j), now);
        let mut everyone: Vec<&mut Formation> = vec![&mut t.founder];
        everyone.extend(t.joiners.iter_mut());
        everyone.push(j);
        spread(&mut everyone, first, now);
    }

    /// `D-083`: a table of the founder and the joiners `2..2 + n`, each seated in
    /// turn and every word carried to every client -- except the words `lost`
    /// names. Client `i` is the founder for 0 and player `i + 1` after it.
    fn seat_all(seats: u8, min: u8, n: u8, lost: &dyn Fn(usize, usize, &[u8]) -> bool) -> Table {
        let (mut t, _, a, hash) = found(seats, min);
        let table_id = t.founder.table_id();
        for p in 2..2 + n {
            let (mut j, request) = Formation::join(
                key(p),
                a.clone(),
                hash,
                table_id,
                peer(p),
                format!("player {p}"),
                1_000,
                None,
                None,
                [p; 32],
                NOW,
                None,
            )
            .unwrap();
            let out = t.founder.on_join_request(&request, &peer(p), false, NOW).expect("seated");
            let first = broadcasts(out, 0, Some(&mut j), NOW);
            t.joiners.push(j);
            carry_from(&mut t, first, NOW, lost);
        }
        t
    }

    /// Carry `first` -- (client, word) -- to every client of `t` until the table
    /// is quiet, losing what `lost` names.
    fn carry_from(t: &mut Table, first: Vec<(usize, Vec<u8>)>, now: u64, lost: &dyn Fn(usize, usize, &[u8]) -> bool) {
        let mut everyone: Vec<&mut Formation> = vec![&mut t.founder];
        everyone.extend(t.joiners.iter_mut());
        spread_losing(&mut everyone, first, now, lost);
    }

    /// Words of type `k` said by client `i` are lost on the way to everybody.
    fn silent(i: usize, k: crate::protocol::messages::EventType) -> impl Fn(usize, usize, &[u8]) -> bool {
        move |from, _, b| from == i && kind(b) == Some(k)
    }

    fn pk(p: u8) -> [u8; 32] {
        key(p).verifying_key().to_bytes()
    }

    /// `D-083`, the owner's word: a player that gives no lot never holds a table
    /// up. The founder's judgement reads what the round owes, the player is
    /// given back, and the table forms without it.
    #[test]
    fn a_player_that_gives_no_lot_is_given_back_and_the_table_forms_without_it() {
        use crate::protocol::messages::EventType;
        let lost = silent(2, EventType::RngCommit);
        let mut t = seat_all(6, 3, 3, &lost);
        assert_eq!(t.founder.roster().len(), 4);
        assert_eq!(t.founder.session(), None, "the round waits on a lot");
        assert!(!t.founder.draw_state().1, "and nothing is sealed without it");
        assert_eq!(t.founder.draw_owed(), vec![(pk(3), Owed::Lot)], "what the round waits on, and from whom");

        let out = t.founder.release_seats_before_the_first_hand(&[peer(3)], NOW + 31_000).expect("given back");
        carry_from(&mut t, broadcasts(out, 0, None, NOW + 31_000), NOW + 31_000, &lost);
        let session = t.founder.session().expect("set without the player that gave no lot");
        assert_eq!(t.founder.roster().len(), 3);
        assert!(t.founder.roster().seat_of(&pk(3)).is_none());
        for (i, j) in t.joiners.iter().enumerate() {
            if i == 1 {
                assert!(j.released_before_the_first_hand(), "the silent player learns it is given back");
            } else {
                assert_eq!(j.session(), Some(session), "player {} agrees", i + 2);
            }
        }
    }

    /// And several of them at once: every player that owes the round its lot is
    /// given back in one roster, and the rest form the table.
    #[test]
    fn several_players_that_give_no_lot_are_given_back_in_one_roster() {
        use crate::protocol::messages::EventType;
        let lost = move |from: usize, _: usize, b: &[u8]| (from == 2 || from == 4) && kind(b) == Some(EventType::RngCommit);
        let mut t = seat_all(6, 3, 5, &lost);
        assert_eq!(t.founder.roster().len(), 6);
        let mut owed = t.founder.draw_owed();
        owed.sort();
        let mut want = vec![(pk(3), Owed::Lot), (pk(5), Owed::Lot)];
        want.sort();
        assert_eq!(owed, want);

        let serial = t.founder.serial();
        let out = t.founder.release_seats_before_the_first_hand(&[peer(3), peer(5)], NOW + 31_000).expect("given back");
        assert_eq!(t.founder.serial(), serial + 1, "one roster for both");
        carry_from(&mut t, broadcasts(out, 0, None, NOW + 31_000), NOW + 31_000, &lost);
        let session = t.founder.session().expect("set at four");
        assert_eq!(t.founder.roster().len(), 4);
        for i in [0usize, 2, 4] {
            assert_eq!(t.joiners[i].session(), Some(session));
        }
    }

    /// A player that gives its lot and does not open it is given back once the
    /// lots are sealed; the round starts again without it -- a round run again
    /// for a player that did not open is one every member takes -- and its lot is
    /// in no draw of the table.
    #[test]
    fn a_player_that_does_not_open_is_given_back_and_the_round_starts_again() {
        use crate::protocol::messages::EventType;
        let lost = silent(2, EventType::RngReveal);
        // At four, so that every player is seated before the lots are sealed.
        let mut t = seat_all(6, 4, 3, &lost);
        assert!(t.founder.draw_state().1, "the lots are sealed");
        assert_eq!(t.founder.draw_owed(), vec![(pk(3), Owed::Opening)]);

        let out = t.founder.release_seats_before_the_first_hand(&[peer(3)], NOW + 31_000).expect("given back");
        carry_from(&mut t, broadcasts(out, 0, None, NOW + 31_000), NOW + 31_000, &lost);
        let session = t.founder.session().expect("set without it");
        assert_eq!(t.joiners[0].session(), Some(session));
        assert_eq!(t.joiners[2].session(), Some(session));
        let list = joinwire::receive_player_list(&t.founder.say_again(NOW + 32_000)[0]).unwrap().0;
        let d = list.draw.expect("the complete draw");
        assert_eq!(d.lots.len(), 3, "three lots, the silent player's in none of them");
    }

    /// A player given back after it opened keeps its lot in the draw and leaves
    /// its drawn seat empty -- nothing is drawn again -- and when a player that
    /// did not open is given back after it, the round runs again without both.
    #[test]
    fn a_player_given_back_after_it_opened_leaves_its_seat_empty() {
        use crate::protocol::messages::EventType;
        // Player 4 (client 3) opens late.
        let late = silent(3, EventType::RngReveal);
        let mut t = seat_all(6, 4, 3, &late);
        assert_eq!(t.founder.draw_owed(), vec![(pk(4), Owed::Opening)]);
        let sealed = t.founder.draw_state().0;

        // Player 3, which opened, leaves; the round goes on.
        let out = t.founder.release_seats_before_the_first_hand(&[peer(3)], NOW + 1_000).expect("given back");
        carry_from(&mut t, broadcasts(out, 0, None, NOW + 1_000), NOW + 1_000, &late);
        assert_eq!(t.founder.draw_state().0, sealed, "the same round");
        assert_eq!(t.joiners[0].draw_state().0, sealed, "at every member");

        // Player 4's opening arrives: the draw is complete, player 3's seat empty.
        let words = t.joiners[2].draw_tick(NOW + 6_000, false);
        let first: Vec<(usize, Vec<u8>)> = words
            .into_iter()
            .filter_map(|s| match s {
                Send::Broadcast(b) => Some((3, b)),
                Send::Reply(_) => None,
            })
            .collect();
        carry_from(&mut t, first, NOW + 6_000, &|_, _, _| false);
        let session = t.founder.session().expect("set at three");
        assert_eq!(t.joiners[0].session(), Some(session));
        assert_eq!(t.joiners[2].session(), Some(session));
        let list = joinwire::receive_player_list(&t.founder.say_again(NOW + 7_000)[0]).unwrap().0;
        assert_eq!(list.draw.expect("complete").lots.len(), 4, "the lot of the player that left is in the draw");
        assert_eq!(t.founder.roster().len(), 3);
    }

    /// The same, the other way: the player that opened is given back first, and
    /// then the one that never opens. The round runs again without both, and
    /// every member takes it -- a player that opened and left is nobody's lever.
    #[test]
    fn a_round_runs_again_without_a_player_that_left_and_one_that_did_not_open() {
        use crate::protocol::messages::EventType;
        let lost = silent(3, EventType::RngReveal);
        let mut t = seat_all(6, 5, 4, &lost);
        assert_eq!(t.founder.draw_owed(), vec![(pk(4), Owed::Opening)]);
        let out = t.founder.release_seats_before_the_first_hand(&[peer(3)], NOW + 1_000).expect("given back");
        carry_from(&mut t, broadcasts(out, 0, None, NOW + 1_000), NOW + 1_000, &lost);
        let out = t.founder.release_seats_before_the_first_hand(&[peer(4)], NOW + 31_000).expect("given back");
        carry_from(&mut t, broadcasts(out, 0, None, NOW + 31_000), NOW + 31_000, &lost);
        let session = t.founder.session().expect("set at three");
        assert_eq!(t.founder.roster().len(), 3);
        assert_eq!(t.joiners[0].session(), Some(session));
        assert_eq!(t.joiners[3].session(), Some(session));
    }

    /// A founder whose players are all gone before the first hand keeps no draw:
    /// a new player sits down, and a table forms with it.
    #[test]
    fn a_founder_left_alone_seats_new_players() {
        let mut t = seat_all(3, 2, 1, &|_, _, _| false);
        assert!(t.founder.session().is_some(), "set heads-up");
        let out = t.founder.release_seats_before_the_first_hand(&[peer(2)], NOW + 1_000).expect("given back");
        carry_from(&mut t, broadcasts(out, 0, None, NOW + 1_000), NOW + 1_000, &|_, _, _| false);
        assert_eq!(t.founder.roster().len(), 1);
        assert_eq!(t.founder.draw_state(), (None, false, false), "no draw kept");

        let (a, hash, table_id) = (t.founder.ad().clone(), t.founder.advert_hash(), t.founder.table_id());
        let (mut j, request) =
            Formation::join(key(7), a, hash, table_id, peer(7), "new".into(), 1_000, None, None, [7; 32], NOW + 2_000, None)
                .unwrap();
        let out = t.founder.on_join_request(&request, &peer(7), false, NOW + 2_000).expect("answered");
        let first = broadcasts(out, 0, Some(&mut j), NOW + 2_000);
        assert!(j.my_seat().is_some(), "seated, not refused");
        t.joiners = vec![j];
        carry_from(&mut t, first, NOW + 2_000, &|_, _, _| false);
        let session = t.founder.session().expect("a table again");
        assert_eq!(t.joiners[0].session(), Some(session));
    }

    /// The second refutation's `H7`: a founder left alone fills its table as a new
    /// one -- no draw kept, and no *set to start* either, so a table advertised
    /// for three does not start at two with the next player that sits down.
    #[test]
    fn a_founder_left_alone_fills_as_a_new_table() {
        let mut t = seat_all(6, 3, 2, &|_, _, _| false);
        assert!(t.founder.session().is_some(), "set at three");
        let out = t.founder.release_seats_before_the_first_hand(&[peer(2), peer(3)], NOW + 1_000).expect("given back");
        carry_from(&mut t, broadcasts(out, 0, None, NOW + 1_000), NOW + 1_000, &|_, _, _| false);
        assert_eq!(t.founder.roster().len(), 1);
        let (a, hash, table_id) = (t.founder.ad().clone(), t.founder.advert_hash(), t.founder.table_id());
        let mut fresh = Vec::new();
        for p in [7u8, 8] {
            let (mut j, request) =
                Formation::join(key(p), a.clone(), hash, table_id, peer(p), format!("player {p}"), 1_000, None, None, [p; 32], NOW + 2_000, None)
                    .unwrap();
            let out = t.founder.on_join_request(&request, &peer(p), false, NOW + 2_000).expect("answered");
            let first = broadcasts(out, 0, Some(&mut j), NOW + 2_000);
            fresh.push(j);
            t.joiners = std::mem::take(&mut fresh);
            carry_from(&mut t, first, NOW + 2_000, &|_, _, _| false);
            fresh = std::mem::take(&mut t.joiners);
            if p == 7 {
                assert_eq!(t.founder.draw_state(), (None, false, false), "two of three: no draw, as at a new table");
                assert!(t.founder.session().is_none());
            }
        }
        t.joiners = fresh;
        let session = t.founder.session().expect("three again, and set");
        assert!(t.joiners.iter().all(|j| j.session() == Some(session)));
    }

    /// `H4`: the founder notes when each member last gave the draw a word, so a
    /// member keeping up with rounds begun again by others is never judged silent.
    #[test]
    fn the_founder_notes_each_members_last_word_to_the_draw() {
        let t = seat_all(6, 3, 2, &|_, _, _| false);
        assert!(t.founder.last_draw_word_ms(&pk(2)).is_some());
        assert!(t.founder.last_draw_word_ms(&pk(3)).is_some());
        assert_eq!(t.founder.last_draw_word_ms(&pk(9)), None);
    }

    /// Once the lots are sealed nobody sits down; a player of a complete draw
    /// given back comes back to the seat the draw gave it, and to no other -- and
    /// not once the founder has ratified, which sets the table (`S1-HE`): a seat
    /// added under the set would split the table between two rosters. The
    /// founder ratifies last here, as its node has it do.
    #[test]
    fn nobody_sits_down_once_the_lots_are_sealed() {
        use crate::protocol::messages::EventType;
        let lost = silent(1, EventType::RngReveal);
        let mut t = seat_all(6, 3, 2, &lost);
        t.founder.hold_ratification(true);
        assert!(t.founder.draw_state().1, "sealed");
        let (a, hash, table_id) = (t.founder.ad().clone(), t.founder.advert_hash(), t.founder.table_id());
        let ask = |p: u8, at: u64| {
            Formation::join(key(p), a.clone(), hash, table_id, peer(p), format!("player {p}"), 1_000, None, None, [p; 32], at, None)
                .unwrap()
        };
        let refused = |out: &[Send]| {
            out.iter().find_map(|s| match s {
                Send::Reply(b) => joinwire::receive_join_reject(b).ok().map(|(_, reason, _, _)| reason),
                Send::Broadcast(_) => None,
            })
        };
        let (_, stranger) = ask(7, NOW + 1_000);
        let out = t.founder.on_join_request(&stranger, &peer(7), false, NOW + 1_000).unwrap();
        assert_eq!(refused(&out), Some(RejectReason::TableFull.code()), "sealed: no stranger");

        // The opening arrives and the draw completes; player 2 is given back.
        let words = t.joiners[0].draw_tick(NOW + 6_000, false);
        let first: Vec<(usize, Vec<u8>)> =
            words.into_iter().filter_map(|s| if let Send::Broadcast(b) = s { Some((1, b)) } else { None }).collect();
        carry_from(&mut t, first, NOW + 6_000, &|_, _, _| false);
        assert!(t.founder.draw_state().2, "complete");
        assert!(t.founder.session().is_none(), "the founder has not ratified yet");
        let drawn = t.founder.roster().seat_of(&pk(2)).expect("seated by the draw");
        let out = t.founder.release_seats_before_the_first_hand(&[peer(2)], NOW + 7_000).unwrap();
        carry_from(&mut t, broadcasts(out, 0, None, NOW + 7_000), NOW + 7_000, &|_, _, _| false);
        let (_, stranger) = ask(8, NOW + 8_000);
        let out = t.founder.on_join_request(&stranger, &peer(8), false, NOW + 8_000).unwrap();
        assert_eq!(refused(&out), Some(RejectReason::TableFull.code()), "a complete draw: no stranger either");
        // Player 2 comes back from a fresh client, asking for another seat.
        let (mut back, request) = Formation::join(
            key(2), a.clone(), hash, table_id, peer(2), "player 2".into(), 1_000, Some((drawn + 1) % 6), None, [22; 32], NOW + 9_000, None,
        )
        .unwrap();
        let out = t.founder.on_join_request(&request, &peer(2), false, NOW + 9_000).unwrap();
        assert_eq!(refused(&out), None, "a player of the draw is seated again");
        let first = broadcasts(out, 0, Some(&mut back), NOW + 9_000);
        assert_eq!(back.my_seat(), Some(drawn), "at the seat the draw gave it");
        t.joiners[0] = back;
        carry_from(&mut t, first, NOW + 9_000, &|_, _, _| false);
        // The founder says it is ready last, and the table is set.
        let out = t.founder.ratify_now(NOW + 10_000).unwrap();
        carry_from(&mut t, broadcasts(out, 0, None, NOW + 10_000), NOW + 10_000, &|_, _, _| false);
        let session = t.founder.session().expect("set");
        assert_eq!(t.founder.roster().len(), 3);
        assert!(t.joiners.iter().all(|j| j.session() == Some(session)));

        // Given back once more, it may not come back under a set table.
        let out = t.founder.release_seats_before_the_first_hand(&[peer(2)], NOW + 11_000).unwrap();
        carry_from(&mut t, broadcasts(out, 0, None, NOW + 11_000), NOW + 11_000, &|_, _, _| false);
        let out = t.founder.ratify_now(NOW + 11_500).unwrap();
        carry_from(&mut t, broadcasts(out, 0, None, NOW + 11_500), NOW + 11_500, &|_, _, _| false);
        assert!(t.founder.session().is_some(), "set again at two");
        let (_, request) = Formation::join(
            key(2), a.clone(), hash, table_id, peer(2), "player 2".into(), 1_000, None, None, [23; 32], NOW + 12_000, None,
        )
        .unwrap();
        let out = t.founder.on_join_request(&request, &peer(2), false, NOW + 12_000).unwrap();
        assert_eq!(refused(&out), Some(RejectReason::TableFull.code()), "not under a table the founder set");
    }

    /// The refutation's finding 3: a player given back for not opening cannot sit
    /// down again and open the round run again instead -- its veto costs it its
    /// seat until the table is set.
    #[test]
    fn a_player_given_back_for_not_opening_cannot_sit_down_again() {
        use crate::protocol::messages::EventType;
        let lost = silent(2, EventType::RngReveal);
        let mut t = seat_all(6, 4, 3, &lost);
        assert_eq!(t.founder.draw_owed(), vec![(pk(3), Owed::Opening)]);
        let out = t.founder.release_seats_before_the_first_hand(&[peer(3)], NOW + 31_000).expect("given back");
        let first = broadcasts(out, 0, None, NOW + 31_000);
        // It asks again at once, before the round run again is sealed.
        let (a, hash, table_id) = (t.founder.ad().clone(), t.founder.advert_hash(), t.founder.table_id());
        let (_, again) =
            Formation::join(key(3), a, hash, table_id, peer(3), "player 3".into(), 1_000, None, None, [33; 32], NOW + 31_100, None)
                .unwrap();
        let out = t.founder.on_join_request(&again, &peer(3), false, NOW + 31_100).unwrap();
        let reason = out.iter().find_map(|s| match s {
            Send::Reply(b) => joinwire::receive_join_reject(b).ok().map(|(_, reason, _, _)| reason),
            Send::Broadcast(_) => None,
        });
        assert_eq!(reason, Some(RejectReason::TableFull.code()));
        carry_from(&mut t, first, NOW + 31_000, &lost);
        let session = t.founder.session().expect("set without it");
        assert_eq!(t.joiners[0].session(), Some(session));
        assert!(t.founder.roster().seat_of(&pk(3)).is_none());
    }

    /// The refutation's finding 1: a roster that does not seat its founder is
    /// refused -- a founder never gives its own seat back, and one that left
    /// itself out of a round could run it again for free.
    #[test]
    fn a_roster_without_its_founder_is_refused() {
        let t = seat_all(6, 3, 2, &|_, _, _| false);
        let j = &t.joiners[0];
        let without: Vec<SeatEntry> = j.roster().seats().iter().filter(|e| e.app_public_key != pk(1)).cloned().collect();
        let list = PlayerList {
            roster: without,
            table_params_hash: j.under_params(),
            list_serial: j.serial() + 1,
            started: true,
            draw: None,
        };
        let bytes = joinwire::publish_player_list(&list, &key(200), NOW + 1_000).unwrap();
        let mut j = t.joiners.into_iter().next().unwrap();
        assert!(matches!(
            j.on_player_list(&bytes, NOW + 1_000),
            Err(Failed::Seating(SeatingRefused::FounderNotSeated))
        ));
    }

    /// `D-060`: a seat held from ratifying says nothing when the roster comes, and
    /// ratifies when its node says it hears every seat -- once; and the table is
    /// set only when the last held seat has.
    #[test]
    fn a_held_seat_ratifies_when_told_and_the_table_waits_for_the_last() {
        let (mut t, _, a, hash) = found(3, 3);
        t.founder.hold_ratification(true);
        let table_id = t.founder.table_id();
        for (n, seed) in [(2u8, 2u8), (3, 3)] {
            let (mut j, request) = Formation::join(
                key(seed),
                a.clone(),
                hash,
                table_id,
                peer(n),
                format!("player {n}"),
                1_000,
                None,
                None,
                [seed; 32],
                NOW,
                None,
            )
            .unwrap();
            j.hold_ratification(true);
            let out = t.founder.on_join_request(&request, &peer(n), false, NOW).expect("seated");
            deliver(&mut t, &mut j, out, table_id, NOW);
            t.joiners.push(j);
        }
        assert_eq!(t.founder.roster().len(), 3);
        assert!(t.founder.may_start());
        assert!(!t.founder.ready_sent() && t.joiners.iter().all(|j| !j.ready_sent()), "nobody ratified while held");

        fn spread(t: &mut Table, sends: Vec<Send>, table_id: Hash) {
            for s in sends {
                let Send::Broadcast(bytes) = s else { continue };
                let (r, sender, _) = joinwire::receive_table_ready(&bytes, &table_id, &t.founder.genesis()).unwrap();
                let mut everyone: Vec<&mut Formation> = vec![&mut t.founder];
                everyone.extend(t.joiners.iter_mut());
                for who in everyone {
                    if who.roster().seat_of(&sender) == Some(r.my_seat) && who.my_seat() != Some(r.my_seat) {
                        who.on_table_ready(&bytes).expect("a ratification holds");
                    }
                }
            }
        }
        let sends = t.founder.ratify_now(NOW + 1).unwrap();
        assert!(!sends.is_empty() && t.founder.ready_sent());
        assert!(t.founder.ratify_now(NOW + 2).unwrap().is_empty(), "once");
        spread(&mut t, sends, table_id);
        let sends = t.joiners[0].ratify_now(NOW + 3).unwrap();
        spread(&mut t, sends, table_id);
        assert!(t.founder.session().is_none(), "one seat has not said it is ready");
        assert_eq!(t.founder.ratified_seats().len(), 2);
        let sends = t.joiners[1].ratify_now(NOW + 4).unwrap();
        spread(&mut t, sends, table_id);
        let session = t.founder.session().expect("set once every seat said it is ready");
        assert!(t.joiners.iter().all(|j| j.session() == Some(session)), "and every seat agrees");
    }

    /// `D-060`: a seat back at a table whose roster it had ratified, with nothing
    /// of its own kept, takes its own ratification from another seat's copy and
    /// computes the table's session -- it never ratifies a second time.
    #[test]
    fn a_seat_back_takes_its_own_ratification_and_the_tables_session() {
        let (mut t, _, a, hash) = found(3, 3);
        let table_id = t.founder.table_id();
        for (n, seed) in [(2u8, 2u8), (3, 3)] {
            let (mut j, request) = Formation::join(
                key(seed),
                a.clone(),
                hash,
                table_id,
                peer(n),
                format!("player {n}"),
                1_000,
                None,
                None,
                [seed; 32],
                NOW,
                None,
            )
            .unwrap();
            let out = t.founder.on_join_request(&request, &peer(n), false, NOW).expect("seated");
            deliver(&mut t, &mut j, out, table_id, NOW);
            t.joiners.push(j);
        }
        let session = t.founder.session().expect("set");

        // The second joiner's client comes back with nothing of its own.
        let (mut back, _) = Formation::join(
            key(3),
            a.clone(),
            hash,
            table_id,
            peer(3),
            "player 3".into(),
            1_000,
            None,
            None,
            [33u8; 32],
            NOW + 10,
            None,
        )
        .unwrap();
        back.hold_ratification(true);
        let said = t.founder.say_again(NOW + 10);
        for bytes in &said {
            if joinwire::receive_player_list(bytes).is_ok() {
                back.on_player_list(bytes, NOW + 10).expect("the list holds");
            }
        }
        for bytes in &said {
            if joinwire::receive_player_list(bytes).is_err() {
                back.on_table_ready(bytes).expect("a ratification holds");
            }
        }
        assert!(back.ready_sent(), "its own ratification, carried back, is its word");
        assert_eq!(back.session(), Some(session), "the table's session, and no other");
        assert!(back.ratify_now(NOW + 11).unwrap().is_empty(), "never a second ratification");
    }

    /// `D-044`, the owner's ruling: a table set to start goes on with the seats
    /// that remain, two at the least, when a seat is given back before the
    /// first hand. Three seats on a table that starts at three; one given
    /// back; the other two ratify the roster of two and agree on one session.
    /// A table never set stays short: nothing starts below the minimum that
    /// was not at it once.
    #[test]
    fn a_table_set_to_start_goes_on_with_the_seats_that_remain() {
        let (mut t, _, a, hash) = found(6, 3);
        let table_id = t.founder.table_id();
        for (n, seed) in [(2u8, 2u8), (3, 3)] {
            let (mut j, request) = Formation::join(
                key(seed),
                a.clone(),
                hash,
                table_id,
                peer(n),
                format!("player {n}"),
                1_000,
                None,
                None,
                [seed; 32],
                NOW,
                None,
            )
            .unwrap();
            let out = t
                .founder
                .on_join_request(&request, &peer(n), false, NOW)
                .expect("seated");
            deliver(&mut t, &mut j, out, table_id, NOW);
            t.joiners.push(j);
        }
        let session = t.founder.session().expect("set at three");
        assert!(t.joiners.iter().all(|j| j.session() == Some(session)), "all three agree");

        // The second joiner's client is gone before the first hand; the founder
        // gives its seat back, and the roster said again holds two.
        let out = t
            .founder
            .release_seat_before_the_first_hand(&peer(3), NOW + 5)
            .expect("given back");
        let _gone = t.joiners.pop().expect("the second joiner");
        let mut stay = t.joiners.pop().expect("the first joiner");
        deliver(&mut t, &mut stay, out, table_id, NOW + 5);
        assert_eq!(t.founder.roster().len(), 2, "two seats remain");
        let again = t.founder.session().expect("set again, at two");
        assert_ne!(again, session, "a new roster is a new session");
        assert_eq!(stay.session(), Some(again), "the joiner that stayed agrees");
    }

    /// And a table that was never set does not start short: two seats on a
    /// table that starts at three ratify nothing.
    #[test]
    fn a_table_never_set_does_not_start_below_its_minimum() {
        let (mut t, _, a, hash) = found(6, 3);
        let table_id = t.founder.table_id();
        let (mut j, request) = Formation::join(
            key(2),
            a,
            hash,
            table_id,
            peer(2),
            "player 2".into(),
            1_000,
            None,
            None,
            [2u8; 32],
            NOW,
            None,
        )
        .unwrap();
        let out = t
            .founder
            .on_join_request(&request, &peer(2), false, NOW)
            .expect("seated");
        deliver(&mut t, &mut j, out, table_id, NOW);
        assert_eq!(t.founder.roster().len(), 2);
        assert!(t.founder.session().is_none(), "two of three: not set");
        assert!(j.session().is_none());
    }

    /// The founder never gives away its own seat, and an unknown peer is a no-op.
    ///
    /// A table whose founder released its own seat is a table with an advert and
    /// nobody behind it. The second half matters as much: the sweep that calls
    /// this runs every thirty seconds and will name a peer it has already
    /// released, so a second call has to be silent rather than an error.
    #[test]
    fn the_founder_keeps_its_own_seat_and_an_unknown_peer_changes_nothing() {
        let (mut t, _, _, _) = found(6, 2);
        let mine = t.founder.roster().len();
        let serial = t.founder.serial();

        let own = t
            .founder
            .roster()
            .seats()
            .iter()
            .find(|e| Some(e.seat) == t.founder.my_seat())
            .map(|e| e.peer_id.clone())
            .expect("the founder has a seat");

        let out = t
            .founder
            .release_seat_before_the_first_hand(&own, NOW + 1)
            .expect("asking is not an error");
        assert!(out.is_empty(), "the founder does not give away its own seat");
        assert_eq!(t.founder.roster().len(), mine);
        assert_eq!(t.founder.serial(), serial, "and nothing moved");

        let out = t
            .founder
            .release_seat_before_the_first_hand(&peer(200), NOW + 2)
            .expect("asking about a stranger is not an error");
        assert!(out.is_empty(), "a peer that holds no seat frees none");
        assert_eq!(t.founder.serial(), serial);
    }

    #[test]
    fn one_node_cannot_take_two_seats() {
        let (mut t, _, a, hash) = found(6, 2);
        let table_id = t.founder.table_id();

        let (_, first) = Formation::join(
            key(2),
            a.clone(),
            hash,
            table_id,
            peer(2),
            "player 2".into(),
            1_000,
            None,
            None,
            [2u8; 32],
            NOW,
            None,
        )
        .unwrap();
        t.founder
            .on_join_request(&first, &peer(2), false, NOW)
            .expect("the first seat is given");

        // A second application key, from the same node.
        let (_, second) = Formation::join(
            key(9),
            a,
            hash,
            table_id,
            peer(2),
            "player 2 again".into(),
            1_000,
            None,
            None,
            [9u8; 32],
            NOW,
            None,
        )
        .unwrap();
        let out = t
            .founder
            .on_join_request(&second, &peer(2), false, NOW)
            .expect("a refusal is still an answer");
        // **A refusal changes no roster, and it repeats one.** The roster
        // assertion below is the one that carries the rule; the send count used
        // to stand in for it and stopped being able to when the repeat was
        // added. A join request from a seat that is already seated is proof
        // that its roster is stale - measured at nine seats, three joiners
        // stuck at four seated for a whole run - so the answer carries the
        // list as well as the refusal.
        assert_eq!(t.founder.roster().len(), 2, "a refusal changes no roster");
        assert_eq!(out.len(), 2, "the refusal, and the roster the asker is missing");
        assert!(
            matches!(&out[1], Send::Broadcast(b) if joinwire::receive_player_list(b).is_ok()),
            "the second send is the player list, re-published as it stands"
        );

        // **And it is signed again, not repeated.** `on_player_list` refuses a
        // list older than `LIST_MAX_AGE_MS`, so re-broadcasting the bytes from
        // when the roster last changed helps only for the first ninety seconds
        // and is discarded after that — which is exactly the case this exists
        // for, a seat that joined and then waited while the table filled.
        //
        // Asked at `NOW + LIST_MAX_AGE_MS * 2`: the answer must still be
        // admissible at that moment, which the stored bytes could not be.
        let much_later = NOW + crate::protocol::constants::LIST_MAX_AGE_MS * 2;
        let out = t
            .founder
            .on_join_request(&second, &peer(2), false, much_later)
            .expect("a refusal is still an answer");
        let list = match &out[1] {
            Send::Broadcast(b) => b.clone(),
            _ => panic!("the second send is the list"),
        };
        let (_, _, emitted) = joinwire::receive_player_list_at(&list).expect("a readable list");
        assert_eq!(
            emitted, much_later,
            "the list is signed at the moment it is asked for, not at the moment the roster changed"
        );
        match &out[0] {
            Send::Reply(bytes) => {
                let (_, reason, _, _) = joinwire::receive_join_reject(bytes).unwrap();
                assert_eq!(reason, RejectReason::AlreadySeated.code());
            }
            _ => panic!("a refusal is a reply"),
        }
    }

    /// A request whose payload key is not the key that signed it gets **no
    /// answer at all**. Replying would confirm to a forger that its message
    /// reached a live founder.
    #[test]
    fn a_forged_request_is_not_answered() {
        let (mut t, _, a, hash) = found(6, 2);
        let table_id = t.founder.table_id();
        let (_, request) = Formation::join(
            key(2),
            a,
            hash,
            table_id,
            peer(2),
            "player 2".into(),
            1_000,
            None,
            None,
            [2u8; 32],
            NOW,
            None,
        )
        .unwrap();

        // The same bytes, arriving on a connection belonging to somebody else.
        assert_eq!(
            t.founder.on_join_request(&request, &peer(7), false, NOW),
            Err(Failed::Join(JoinRefused::PeerIdIsNotTheConnection))
        );
        assert_eq!(t.founder.roster().len(), 1);
    }

    /// A joiner that is refused learns the founder's claim and nothing more. A
    /// reason is never proof: the founder may simply not want this player.
    #[test]
    fn a_refusal_reaches_the_joiner_as_a_claim() {
        let (mut t, _, a, hash) = found(2, 2);
        let table_id = t.founder.table_id();

        let (_, first) = Formation::join(
            key(2),
            a.clone(),
            hash,
            table_id,
            peer(2),
            "two".into(),
            1_000,
            None,
            None,
            [2u8; 32],
            NOW,
            None,
        )
        .unwrap();
        t.founder.on_join_request(&first, &peer(2), false, NOW).unwrap();

        let (mut third, request) = Formation::join(
            key(3),
            a,
            hash,
            table_id,
            peer(3),
            "three".into(),
            1_000,
            None,
            None,
            [3u8; 32],
            NOW,
            None,
        )
        .unwrap();
        let out = t.founder.on_join_request(&request, &peer(3), false, NOW).unwrap();
        let Send::Reply(bytes) = &out[0] else {
            panic!("a refusal is a reply")
        };
        assert_eq!(
            third.on_join_answer(bytes, NOW),
            Err(Failed::Refused {
                reason: RejectReason::TableFull.code(),
                retry_after_ms: 0
            })
        );
        assert_eq!(third.my_seat(), None);
    }

    /// The password gate, both ways round.
    #[test]
    fn a_password_table_needs_the_password() {
        let app = key(1);
        let table = key(200);
        let mut a = ad(&app, 6, 2);
        a.password_required = true;
        let event = super::super::advert::publish(&a, &table).unwrap();
        let hash = crate::protocol::transcript::event_hash(
            &crate::protocol::serialization::from_canonical::<
                crate::protocol::messages::SignedEvent,
            >(&event, 8_192)
            .unwrap()
            .body,
        );
        let mut f = Formation::found(
            app,
            table.clone(),
            a.clone(),
            event,
            hash,
            Some(b"otevri se".to_vec()),
            peer(1),
            "Alice".into(),
            1_000,
        )
        .unwrap();
        let table_id = table.verifying_key().to_bytes();

        // No password at all: refused before a request is even built.
        assert_eq!(
            Formation::join(
                key(2),
                a.clone(),
                hash,
                table_id,
                peer(2),
                "two".into(),
                1_000,
                None,
                None,
                [2u8; 32],
                NOW,
                None,
            )
            .err(),
            Some(Failed::Join(JoinRefused::BadPassword))
        );

        // The wrong one: refused by the founder, as a claim.
        let (_, wrong) = Formation::join(
            key(2),
            a.clone(),
            hash,
            table_id,
            peer(2),
            "two".into(),
            1_000,
            None,
            Some(b"neotevri"),
            [2u8; 32],
            NOW,
            None,
        )
        .unwrap();
        let out = f.on_join_request(&wrong, &peer(2), false, NOW).unwrap();
        let Send::Reply(bytes) = &out[0] else {
            panic!()
        };
        let (_, reason, _, _) = joinwire::receive_join_reject(bytes).unwrap();
        assert_eq!(reason, RejectReason::BadPassword.code());
        assert_eq!(f.roster().len(), 1);

        // And the right one.
        let (_, right) = Formation::join(
            key(2),
            a,
            hash,
            table_id,
            peer(2),
            "two".into(),
            1_000,
            None,
            Some(b"otevri se"),
            [2u8; 32],
            NOW,
            None,
        )
        .unwrap();
        f.on_join_request(&right, &peer(2), false, NOW).unwrap();
        assert_eq!(f.roster().len(), 2);
    }

    /// A ratification names the serial it ratifies, so a seat joining after
    /// somebody has ratified starts a fresh round rather than being added to a
    /// roster half the table has already signed.
    #[test]
    fn a_new_seat_starts_the_ratification_again() {
        let (mut t, _, a, hash) = found(6, 2);
        let table_id = t.founder.table_id();

        let (_, first) = Formation::join(
            key(2),
            a.clone(),
            hash,
            table_id,
            peer(2),
            "two".into(),
            1_000,
            None,
            None,
            [2u8; 32],
            NOW,
            None,
        )
        .unwrap();
        t.founder.on_join_request(&first, &peer(2), false, NOW).unwrap();
        let after_first = t.founder.serial();

        let (_, second) = Formation::join(
            key(3),
            a,
            hash,
            table_id,
            peer(3),
            "three".into(),
            1_000,
            None,
            None,
            [3u8; 32],
            NOW,
            None,
        )
        .unwrap();
        t.founder.on_join_request(&second, &peer(3), false, NOW).unwrap();

        assert!(
            t.founder.serial() > after_first,
            "the serial must increase when the roster changes"
        );
        assert_eq!(
            t.founder.session(),
            None,
            "the founder still holds a session from the smaller roster"
        );
    }

    /// A table of three, formed as `three_clients_form_one_table` forms it.
    fn form_three() -> (Table, TableAd, Hash) {
        let (mut t, _event, a, hash) = found(6, 3);
        let table_id = t.founder.table_id();
        for (n, seed) in [(2u8, 2u8), (3, 3)] {
            let (mut j, request) = Formation::join(
                key(seed),
                a.clone(),
                hash,
                table_id,
                peer(n),
                format!("player {n}"),
                1_000,
                None,
                None,
                [seed; 32],
                NOW,
                None,
            )
            .expect("the request builds");
            let out = t
                .founder
                .on_join_request(&request, &peer(n), false, NOW)
                .expect("the founder seats an honest joiner");
            deliver(&mut t, &mut j, out, table_id, NOW);
            t.joiners.push(j);
        }
        (t, a, hash)
    }

    /// `D-037`: the founder, restarted, is rebuilt from what it recorded --
    /// the table key, the advertisement, its last list -- says the recorded
    /// ratification byte for byte, computes the table's session from the
    /// members' copies, and answers a seated peer as the founder again.
    #[test]
    fn the_founder_comes_back_from_its_own_record() {
        let (t, a, hash) = form_three();
        let table_id = t.founder.table_id();
        let session = t.founder.session().expect("the table settled");
        let seed = t.founder.table_seed().expect("the founder holds the table key");
        assert_eq!(seed, [200u8; 32], "the seed the fixture founded with");
        let list = t.founder.my_list().expect("the founder signed a list").to_vec();
        let recorded = t.founder.my_ratification().expect("and ratified").to_vec();
        let ad = t.founder.ad().clone();
        let advert_hash = t.founder.advert_hash();
        assert_eq!(advert_hash, hash);
        let others: Vec<Vec<u8>> = t.joiners.iter().flat_map(|j| j.say_again(NOW)).collect();
        let later = NOW + 60_000;

        let (mut back, said) = Formation::found_back(
            key(1),
            seed,
            ad,
            advert_hash,
            &list,
            Some(recorded.clone()),
            later,
        )
        .expect("rebuilt from the record");
        assert!(back.is_founder(), "the founder again");
        assert_eq!(back.my_seat(), t.founder.my_seat(), "at the seat the draw gave it");
        assert_eq!(back.table_id(), table_id);
        assert_eq!(
            said,
            vec![Send::Broadcast(recorded.clone())],
            "what it says is the recorded ratification, byte for byte"
        );
        assert!(!back.recorded_ratification_refused());
        for b in others {
            if joinwire::receive_table_ready(&b, &table_id, &back.genesis()).is_ok() {
                let _ = back.on_table_ready(&b);
            }
        }
        assert_eq!(back.session(), Some(session), "the same session identity as the table's");

        // A seated peer asking again is answered as the founder always did:
        // already seated, and the roster said again.
        let (_, again) = Formation::join(
            key(2), a.clone(), hash, table_id, peer(2), "player 2".into(), 1_000, None, None, [9u8; 32], later, None,
        )
        .unwrap();
        let out = back
            .on_join_request(&again, &peer(2), true, later)
            .expect("answered by the founder");
        assert!(
            out.iter().any(|s| matches!(s, Send::Broadcast(b) if joinwire::receive_player_list(b).is_ok())),
            "the roster said again: serial {}, sent {:?}",
            back.serial(),
            out.iter().map(|s| match s { Send::Reply(b) => format!("reply {} B", b.len()), Send::Broadcast(b) => format!("broadcast {} B", b.len()) }).collect::<Vec<_>>()
        );
    }

    /// `S1-CR`: a seat that restarts says the ratification it recorded,
    /// **verbatim**, and computes the table's own session identity.
    ///
    /// `run202634-3`: the returning seat ratified anew -- a new timestamp, a
    /// new event hash -- and settled on a session nobody else had (`ed73f924`
    /// against the table's `9718345e`); every deck key of the hand it adopted
    /// was then refused, because the key's ownership proof is bound to the
    /// session id. The members held its original ratification all along. The
    /// second half is that measurement: a formation given nothing ratifies
    /// anew and lands on another session.
    #[test]
    fn a_seat_that_restarts_ratifies_with_the_bytes_it_recorded() {
        let (mut t, a, hash) = form_three();
        let table_id = t.founder.table_id();
        let session = t.founder.session().expect("the table settled");
        let gone = t.joiners.remove(0);
        let seat = gone.my_seat().expect("seated");
        let recorded = gone
            .my_ratification()
            .expect("a settled seat holds its own ratification")
            .to_vec();
        drop(gone);
        let later = NOW + 60_000;

        // The table has dealt. The seat's process is new and asks again from
        // its record, which is the node's `ResumeSession` road; the founder
        // answers *already seated* and says the roster again.
        let (j, again) = Formation::join(
            key(2), a.clone(), hash, table_id, peer(2), "player 2".into(), 1_000, None, None, [9u8; 32], later, None,
        )
        .unwrap();
        let mut j = j.with_recorded_ratification(recorded.clone());
        let out = t
            .founder
            .on_join_request(&again, &peer(2), true, later)
            .expect("a seated peer is answered");
        let list = out
            .into_iter()
            .find_map(|s| match s {
                Send::Broadcast(b) if joinwire::receive_player_list(&b).is_ok() => Some(b),
                _ => None,
            })
            .expect("the founder says the roster again");
        let said = j.on_player_list(&list, later).expect("the list holds");
        assert_eq!(
            said,
            vec![Send::Broadcast(recorded.clone())],
            "what it says is the recorded ratification, byte for byte, not a new one"
        );
        assert_eq!(j.my_seat(), Some(seat));
        assert!(!j.recorded_ratification_refused());
        // The members' answer is everything they hold (`say_again`), the
        // seat's own original among it -- a byte-identical repeat, not a
        // second ratification.
        for b in t.founder.say_again(later) {
            if joinwire::receive_table_ready(&b, &table_id, &j.genesis()).is_ok() {
                j.on_table_ready(&b).expect("a ratification the table settled on holds");
            }
        }
        assert_eq!(j.session(), Some(session), "the same session identity as the table's");

        // With nothing recorded it is what the run measured: a new
        // ratification, the original refused as a second one, another session.
        let (mut fresh, again) = Formation::join(
            key(2), a.clone(), hash, table_id, peer(2), "player 2".into(), 1_000, None, None, [10u8; 32], later + 1, None,
        )
        .unwrap();
        let out = t
            .founder
            .on_join_request(&again, &peer(2), true, later + 1)
            .expect("answered");
        let list = out
            .into_iter()
            .find_map(|s| match s {
                Send::Broadcast(b) if joinwire::receive_player_list(&b).is_ok() => Some(b),
                _ => None,
            })
            .expect("the roster again");
        let said = fresh.on_player_list(&list, later + 1).expect("the list holds");
        assert_ne!(said, vec![Send::Broadcast(recorded.clone())], "a fresh formation ratifies anew");
        for b in t.founder.say_again(later + 1) {
            if joinwire::receive_table_ready(&b, &table_id, &fresh.genesis()).is_ok() {
                let _ = fresh.on_table_ready(&b);
            }
        }
        assert_ne!(fresh.session(), Some(session), "and that is a session nobody else has -- the run's ed73f924");
    }
}
