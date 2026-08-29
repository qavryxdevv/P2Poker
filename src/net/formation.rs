//! Forming a table: the dance, as a state machine with no network in it.
//!
//! Every rule was written and tested in [`table::join`](crate::table::join) and
//! [`table::formation`](crate::table::formation); every message got a wire form
//! in [`joinwire`](super::joinwire). What was missing between them is the part
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

use std::collections::BTreeMap;

use ed25519_dalek::SigningKey;

use super::joinwire::{self, WireError};
use super::lobby::TableAd;
use crate::poker::state::Hash;
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
}

impl From<WireError> for Failed {
    fn from(e: WireError) -> Self {
        Failed::Wire(e)
    }
}

/// The founder's extra half.
struct FounderPart {
    /// The table's identity. Nothing else is.
    key: SigningKey,
    /// The signed advert, echoed verbatim into every acceptance so a joiner
    /// re-verifies the table key's own signature instead of trusting a gossip
    /// copy.
    advert_event: Vec<u8>,
    /// The table password, if it has one.
    password: Option<Vec<u8>>,
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
    sent_ready: bool,
    session: Option<Hash>,
    capabilities: Vec<Vec<u8>>,
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
        };
        // Through the same gate as anybody else's seat. A founder that seated
        // itself outside the rules would be the one entry no joiner could have
        // refused.
        let roster =
            Roster::form(vec![me], &under.ad, false).map_err(|e| Failed::List(ListRefused::Roster(e)))?;

        Ok(Formation {
            app,
            founder: Some(FounderPart {
                key: table_key,
                advert_event,
                password,
            }),
            under,
            roster,
            serial: 0,
            my_seat: Some(0),
            my_buyin,
            pending: None,
            ratified: BTreeMap::new(),
            sent_ready: false,
            session: None,
            capabilities: vec![DECK_CAPABILITY.to_vec()],
        })
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
                sent_ready: false,
                session: None,
                capabilities: vec![DECK_CAPABILITY.to_vec()],
            },
            bytes,
        ))
    }

    /// `GENESIS(0)` — the setup chain's genesis, which every `TABLE_READY` is
    /// chained to and which is therefore where a parameter fork is caught.
    pub fn genesis(&self) -> Hash {
        genesis_setup(&self.under.table_id, &self.under.params)
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

    /// The founder's answer to a join request.
    ///
    /// `connection_peer_id` is what the **transport** authenticated, not what
    /// the request claims: the whole of `U17` rests on those being compared.
    pub fn on_join_request(
        &mut self,
        bytes: &[u8],
        connection_peer_id: &[u8],
        now_ms: u64,
    ) -> Result<Vec<Send>, Failed> {
        let f = self.founder.as_ref().ok_or(Failed::NotTheFounder)?;
        let (req, sender, request_hash) = joinwire::receive_join_request(bytes)?;

        match admit_join(
            &req,
            &sender,
            connection_peer_id,
            &self.under,
            &self.roster,
            f.password.as_deref(),
        ) {
            Ok(entry) => {
                let mut seats = self.roster.seats().to_vec();
                seats.push(entry.clone());
                seats.sort_by_key(|e| e.seat);
                self.roster = Roster::form(seats, &self.under.ad, false)
                    .map_err(|e| Failed::List(ListRefused::Roster(e)))?;

                let accept = JoinAccept {
                    request_hash,
                    seat: entry.seat,
                    advert_event: f.advert_event.clone(),
                    roster_so_far: self.roster.seats().to_vec(),
                };
                let reply = joinwire::publish_join_accept(&accept, &f.key, now_ms)?;

                // The roster changed, so every previous ratification named a
                // serial that no longer describes this table.
                self.serial += 1;
                self.ratified.clear();
                self.sent_ready = false;
                self.session = None;

                let list = PlayerList {
                    roster: self.roster.seats().to_vec(),
                    table_params_hash: self.under.params,
                    list_serial: self.serial,
                };
                let list_bytes = joinwire::publish_player_list(&list, &f.key, now_ms)?;

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
                Ok(vec![Send::Reply(reply)])
            }
        }
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
        self.my_seat = Some(accept.seat);
        self.roster = roster;
        let _ = now_ms;
        Ok(vec![])
    }

    /// A `PLAYER_LIST` from the founder — a proposal, checked every time.
    pub fn on_player_list(&mut self, bytes: &[u8], now_ms: u64) -> Result<Vec<Send>, Failed> {
        let (list, sender) = joinwire::receive_player_list(bytes)?;
        let held = if self.serial == 0 { None } else { Some(self.serial) };
        let roster = admit_list(&list, &sender, held, &self.under).map_err(Failed::List)?;
        self.roster = roster;
        self.adopt(&list, now_ms)
    }

    /// Take a checked list as this client's own, and ratify it if it is
    /// ratifiable.
    fn adopt(&mut self, list: &PlayerList, now_ms: u64) -> Result<Vec<Send>, Failed> {
        if list.list_serial != self.serial {
            self.serial = list.list_serial;
            self.ratified.clear();
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

        if self.sent_ready || self.roster.len() < self.under.ad.min_players_to_start as usize {
            return Ok(vec![]);
        }

        let ready = TableReady {
            roster_hash: self.roster.hash_at_zero(),
            list_serial: self.serial,
            table_params_hash: self.under.params,
            my_seat: seat,
            capability_set: self.capabilities.clone(),
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
        self.settle();
        Ok(vec![Send::Broadcast(bytes)])
    }

    /// Somebody else's ratification.
    pub fn on_table_ready(&mut self, bytes: &[u8]) -> Result<Vec<Send>, Failed> {
        let (ready, sender, event_hash) =
            joinwire::receive_table_ready(bytes, &self.under.table_id, &self.genesis())?;
        admit_ready(&ready, &sender, &self.roster, self.serial, &self.under)
            .map_err(Failed::Ready)?;
        self.ratified.insert(ready.my_seat, event_hash);
        self.settle();
        Ok(vec![])
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
            ) as u32,
            join_deadline_ms: 120_000,
            hand_delay_ms: delay,
            button_rule: 1,
            odd_chip_rule: 1,
            showdown_policy: 1,
            password_required: false,
            deck_suite: DECK_SUITE_V1.into(),
            founder_app_key: founder.verifying_key().to_bytes(),
            founder_peer_id: peer(1),
            timestamp_unix_ms: NOW,
            expires_at_unix_ms: NOW + 90_000,

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
            )
            .expect("the request builds");

            let out = t
                .founder
                .on_join_request(&request, &peer(n), NOW)
                .expect("the founder seats an honest joiner");

            // The reply first, then the list to everybody — including the
            // joiners already seated.
            let mut list_bytes = None;
            let mut ready_from_founder = vec![];
            for s in out {
                match s {
                    Send::Reply(bytes) => {
                        j.on_join_answer(&bytes, NOW).expect("the acceptance holds");
                    }
                    Send::Broadcast(bytes) => {
                        if joinwire::receive_player_list(&bytes).is_ok() {
                            list_bytes = Some(bytes);
                        } else {
                            ready_from_founder.push(bytes);
                        }
                    }
                }
            }

            let list = list_bytes.expect("a roster change is announced");
            let mut new_readies = vec![];
            for other in t.joiners.iter_mut() {
                for s in other.on_player_list(&list, NOW).expect("the list holds") {
                    if let Send::Broadcast(b) = s {
                        new_readies.push(b);
                    }
                }
            }
            for s in j.on_player_list(&list, NOW).expect("the list holds") {
                if let Send::Broadcast(b) = s {
                    new_readies.push(b);
                }
            }
            new_readies.extend(ready_from_founder);

            // Every ratification reaches every other seat.
            let mut everyone: Vec<&mut Formation> = vec![&mut t.founder];
            everyone.extend(t.joiners.iter_mut());
            everyone.push(&mut j);
            for bytes in &new_readies {
                let (r, sender, _) =
                    joinwire::receive_table_ready(bytes, &table_id, &everyone[0].genesis())
                        .unwrap();
                for who in everyone.iter_mut() {
                    if who.roster().seat_of(&sender) == Some(r.my_seat)
                        && who.my_seat() != Some(r.my_seat)
                    {
                        who.on_table_ready(bytes).expect("a ratification holds");
                    }
                }
            }

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
        )
        .unwrap();
        for s in t.founder.on_join_request(&request, &peer(2), NOW).unwrap() {
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
        )
        .unwrap();
        t.founder
            .on_join_request(&first, &peer(2), NOW)
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
        )
        .unwrap();
        let out = t
            .founder
            .on_join_request(&second, &peer(2), NOW)
            .expect("a refusal is still an answer");
        assert_eq!(out.len(), 1, "a refusal changes no roster");
        assert_eq!(t.founder.roster().len(), 2);
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
        )
        .unwrap();

        // The same bytes, arriving on a connection belonging to somebody else.
        assert_eq!(
            t.founder.on_join_request(&request, &peer(7), NOW),
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
        )
        .unwrap();
        t.founder.on_join_request(&first, &peer(2), NOW).unwrap();

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
        )
        .unwrap();
        let out = t.founder.on_join_request(&request, &peer(3), NOW).unwrap();
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
        )
        .unwrap();
        let out = f.on_join_request(&wrong, &peer(2), NOW).unwrap();
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
        )
        .unwrap();
        f.on_join_request(&right, &peer(2), NOW).unwrap();
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
        )
        .unwrap();
        t.founder.on_join_request(&first, &peer(2), NOW).unwrap();
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
        )
        .unwrap();
        t.founder.on_join_request(&second, &peer(3), NOW).unwrap();

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
}
