//! `D-083`: the seating draw of one forming table, as one client runs it.
//!
//! [`crate::table::draw`] is the arithmetic and the wire; this is who says what
//! and when, and -- the half that makes the draw worth having -- what a member
//! refuses. The founder assembles the draw, and the founder is exactly the party
//! a draw must not trust: a founder that could swap its own lot after seeing the
//! others', seal a lot a member never drew, re-run a round whose outcome it did
//! not like, or place players by hand would choose the seating it wanted, and
//! the draw would dress that up as chance. So every member checks the founder's
//! rosters against what it drew, sealed, opened and heard itself, and refuses a
//! roster that does any of those things.
//!
//! # The rounds
//!
//! A round is for the members of one roster at or above the table's floor, and
//! is numbered by that roster's serial. Every member draws one lot for it and
//! says its commitment ([`crate::net::tabletalk::lot_word`]). The founder seals
//! the lots in a roster it says again, with its own lot opened beside them; each
//! member checks its own lot is sealed unchanged, and opens under that set of
//! lots and no other. The founder then says the roster a third time, re-seated,
//! with every opening: the draw is complete, and from then on every roster of the
//! table carries it unchanged until the table is set.
//!
//! A member that does not give its lot or its opening in time is given back by
//! the founder (`net::run`), and the round starts again without it -- unless the
//! member had already opened, in which case its lot stays in the draw and its
//! drawn seat stays empty. Nothing about a round waits on any word a member
//! chooses whether to say, which is what makes the draw unable to freeze a
//! table.

use std::collections::BTreeMap;

use ed25519_dalek::SigningKey;

use crate::net::tabletalk;
use crate::poker::state::Hash;
use crate::protocol::constants::MAX_SEATS;
use crate::table::draw::{self, Checked, DrawRefused, DrawWire, Drawn, Lot, SaidLot, SaidOpening};
use crate::table::formation::Roster;

/// Why a roster's draw -- or a roster without one -- is not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatingRefused {
    /// The draw itself does not check.
    Draw(DrawRefused),
    /// A round older than the one this client is in.
    StaleRound { got: u64, held: u64 },
    /// A roster without the draw this client holds complete, or with another.
    DrawDiscarded,
    /// A new round while this client holds every opening of its round, or one
    /// that leaves out no member whose opening this client never heard -- a
    /// round the founder ran again for an outcome it may have seen.
    RoundAbandoned,
    /// A player added to a round whose lots are sealed.
    AddedAfterSealing,
    /// Not the lots this client sealed and opened under.
    LotsChanged,
    /// This client's own lot, as the roster seals it, is not the lot it drew.
    NotMyLot,
    /// A player of the roster sits somewhere the draw did not put it, or is no
    /// member of the draw at all.
    NotTheDrawnSeating,
    /// Lots sealed without the founder's own opening beside them, or without the
    /// founder's lot at all. The founder sees every lot, so it opens first: a
    /// founder that did not could learn every opening before committing its own
    /// and run the round again by leaving itself out of it.
    FounderDidNotOpen,
    /// A roster that does not seat its founder. A founder never gives its own seat
    /// back, and one that left itself out could run a round again for free.
    FounderNotSeated,
    /// A lot in an unfinished draw whose player is not at the table and whose
    /// opening the roster does not carry -- a player nobody can hear open, whose
    /// lot only the founder may know.
    UnseatedLot,
}

/// What a member owes the round, for the founder's judgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Owed {
    /// Its sealed lot.
    Lot,
    /// Its opening, the lots being sealed.
    Opening,
}

/// A complete draw, as the roster carries it and as it decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    pub wire: DrawWire,
    /// What `TABLE_READY` binds.
    pub digest: Hash,
    pub drawn: Drawn,
    /// The draw's members, ascending by key.
    pub members: Vec<[u8; 32]>,
}

impl Done {
    /// Whether a roster sits as this draw put it: every player a member, at the
    /// seat the draw gave it. A member given back leaves its seat empty.
    pub fn fits(&self, roster: &Roster) -> bool {
        roster.seats().iter().all(|e| self.drawn.seat_of(&e.app_public_key) == Some(e.seat))
    }
}

/// What taking a roster's draw asks of this client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Taken {
    /// This client's lot is sealed and it should open now.
    pub open_now: bool,
    /// The draw became complete with this roster.
    pub completed: bool,
}

#[derive(Debug, Clone)]
struct Sealed {
    wire: DrawWire,
    lots: Hash,
    commitments: BTreeMap<[u8; 32], Hash>,
}

#[derive(Debug, Clone)]
struct Round {
    number: u64,
    members: Vec<[u8; 32]>,
    digest: Hash,
    lot: Option<Lot>,
    commitment: Option<Hash>,
    sealed: Option<Sealed>,
    /// Lots heard for this round, the newest a member said: (said at, word, commitment).
    lots_in: BTreeMap<[u8; 32], (u64, Vec<u8>, Hash)>,
    /// Openings heard for the sealed lots, one a member.
    openings_in: BTreeMap<[u8; 32], Vec<u8>>,
}

/// One client's part in the seating draw of one forming table.
#[derive(Debug, Clone)]
pub struct Seating {
    table_id: Hash,
    me: [u8; 32],
    /// The founder's application key, from the advert this table was formed
    /// under: the one member whose opening every sealing carries.
    founder: [u8; 32],
    round: Option<Round>,
    done: Option<Done>,
    /// **Who may still be in a round, once one was sealed.** From the first
    /// sealing until the table is set nobody new sits down, and a round run again
    /// is for the players of the round before it that remain. A player given back
    /// for not opening therefore cannot sit down again and open the next round
    /// instead: its veto costs it its seat for real.
    allowed: Option<Vec<[u8; 32]>>,
}

impl Seating {
    pub fn new(table_id: Hash, me: [u8; 32], founder: [u8; 32]) -> Seating {
        Seating { table_id, me, founder, round: None, done: None, allowed: None }
    }

    /// Whether a round of this table was ever sealed: from then on nobody new
    /// sits down until the table is set.
    pub fn frozen(&self) -> bool {
        self.allowed.is_some()
    }

    /// Whether `key`'s lot is sealed in the current round -- a member whose
    /// opening counts wherever it comes from, seated or given back since.
    pub fn is_sealed_member(&self, key: &[u8; 32]) -> bool {
        self.round.as_ref().and_then(|r| r.sealed.as_ref()).is_some_and(|s| s.commitments.contains_key(key))
    }

    /// Whether a lot of `key`'s for the current round is already held here.
    pub fn has_lot(&self, key: &[u8; 32]) -> bool {
        self.round.as_ref().is_some_and(|r| r.lots_in.contains_key(key))
    }

    /// The complete draw, once there is one.
    pub fn done(&self) -> Option<&Done> {
        self.done.as_ref()
    }

    /// The round this client is in.
    pub fn round(&self) -> Option<u64> {
        self.round.as_ref().map(|r| r.number)
    }

    /// Whether the lots of the current round are sealed, or the draw is complete:
    /// from then on no stranger sits down.
    pub fn sealed(&self) -> bool {
        self.done.is_some() || self.round.as_ref().is_some_and(|r| r.sealed.is_some())
    }

    /// The digest a ratification of `roster` binds: the complete draw's, when the
    /// roster sits as it put it.
    pub fn digest_for(&self, roster: &Roster) -> Option<Hash> {
        self.done.as_ref().filter(|d| d.fits(roster)).map(|d| d.digest)
    }

    /// The seat a complete draw gave `key`, for a member coming back.
    pub fn drawn_seat(&self, key: &[u8; 32]) -> Option<u8> {
        self.done.as_ref().and_then(|d| d.drawn.seat_of(key))
    }

    /// Whether `key` opened its lot in the current round -- a member given back
    /// after that keeps its lot in the draw.
    pub fn opened_by(&self, key: &[u8; 32]) -> bool {
        self.done.as_ref().is_some_and(|d| d.members.contains(key))
            || self.round.as_ref().is_some_and(|r| r.openings_in.contains_key(key))
    }

    /// The guards on a roster without a draw, before it is taken.
    pub fn check_plain(&self, members: &[[u8; 32]]) -> Result<(), SeatingRefused> {
        if self.done.is_some() {
            return Err(SeatingRefused::DrawDiscarded);
        }
        if let Some(allowed) = &self.allowed {
            if members.iter().any(|m| !allowed.contains(m)) {
                return Err(SeatingRefused::AddedAfterSealing);
            }
        }
        self.may_leave_round(members)
    }

    /// A roster without a draw, at or above the floor: a round of its members
    /// begins, numbered by the roster's serial. Returns this client's commitment
    /// when it is a member and drew a lot.
    pub fn begin(&mut self, serial: u64, members: &[[u8; 32]]) -> Option<Hash> {
        let mut members = members.to_vec();
        members.sort_unstable();
        members.dedup();
        if self.round.as_ref().is_some_and(|r| r.number == serial && r.members == members) {
            return self.round.as_ref().and_then(|r| r.commitment);
        }
        // Past the first sealing a new round is for the players that remain.
        if let Some(allowed) = self.allowed.as_mut() {
            allowed.retain(|k| members.contains(k));
        }
        let digest = draw::members_digest(&self.table_id, &members);
        let mine = members.contains(&self.me);
        let lot = if mine { Lot::draw().ok() } else { None };
        let commitment = lot.as_ref().map(|l| l.commitment(&self.table_id, serial, &digest, &self.me));
        self.round = Some(Round {
            number: serial,
            members,
            digest,
            lot,
            commitment,
            sealed: None,
            lots_in: BTreeMap::new(),
            openings_in: BTreeMap::new(),
        });
        commitment
    }

    /// A roster below the floor: no round runs, and the one there was is over.
    pub fn end_round(&mut self) {
        if self.done.is_none() {
            self.round = None;
        }
    }

    /// Check a roster's draw against what this client drew, sealed, opened and
    /// heard -- without taking it.
    pub fn check_draw(&self, d: &DrawWire, roster: &Roster) -> Result<Checked, SeatingRefused> {
        if let Some(done) = &self.done {
            if &done.wire != d {
                return Err(SeatingRefused::DrawDiscarded);
            }
            if !done.fits(roster) {
                return Err(SeatingRefused::NotTheDrawnSeating);
            }
        }
        if let Some(r) = &self.round {
            if d.round < r.number {
                return Err(SeatingRefused::StaleRound { got: d.round, held: r.number });
            }
        }
        let table = self.table_id;
        let checked = draw::check(
            &self.table_id,
            d,
            usize::from(MAX_SEATS),
            |b| tabletalk::open_lot(b, &table).map_err(|_| "not a sealed lot of this table"),
            |b| tabletalk::open_opening(b, &table).map_err(|_| "not an opened lot of this table"),
        )
        .map_err(SeatingRefused::Draw)?;
        // Every player of the roster is a member of the draw: nobody sits down
        // once the lots are sealed.
        if roster.seats().iter().any(|e| !checked.members.contains(&e.app_public_key)) {
            return Err(SeatingRefused::NotTheDrawnSeating);
        }
        if let Some(allowed) = &self.allowed {
            if checked.members.iter().any(|m| !allowed.contains(m)) {
                return Err(SeatingRefused::AddedAfterSealing);
            }
        }
        // The founder is a member and opened first; and a lot whose player is not
        // at the table is one only its opening, carried here, can vouch for.
        if !checked.members.contains(&self.founder) {
            return Err(SeatingRefused::FounderDidNotOpen);
        }
        if checked.drawn.is_none() {
            if !checked.opened.iter().any(|(k, _)| *k == self.founder) {
                return Err(SeatingRefused::FounderDidNotOpen);
            }
            let seated = |k: &[u8; 32]| roster.seats().iter().any(|e| e.app_public_key == *k);
            if checked.members.iter().any(|m| !seated(m) && !checked.opened.iter().any(|(k, _)| k == m)) {
                return Err(SeatingRefused::UnseatedLot);
            }
        }
        let same_round = self.round.as_ref().filter(|r| r.number == d.round);
        match same_round {
            Some(r) => {
                if let Some(s) = &r.sealed {
                    if s.lots != checked.lots {
                        return Err(SeatingRefused::LotsChanged);
                    }
                }
                if checked.members.contains(&self.me) {
                    let sealed_mine = checked
                        .members
                        .iter()
                        .position(|k| k == &self.me)
                        .map(|i| checked.commitments[i]);
                    // A lot sealed under this client's key that is not the one it
                    // drew: it cannot open it, and a roster that says otherwise is
                    // one the founder made up. A complete draw carries this
                    // client's own signed opening, which settles it the other way.
                    if checked.drawn.is_none() && (r.commitment.is_none() || sealed_mine != r.commitment) {
                        return Err(SeatingRefused::NotMyLot);
                    }
                }
            }
            None => {
                // A round this client never saw begin. It may leave its own for
                // it on the same terms as for any new round; and it can take part
                // only where its opening is already in, since it drew no lot.
                self.may_leave_round(&checked.members)?;
                if checked.drawn.is_none() && checked.members.contains(&self.me) {
                    return Err(SeatingRefused::NotMyLot);
                }
            }
        }
        if let Some(drawn) = &checked.drawn {
            if roster.seats().iter().any(|e| drawn.seat_of(&e.app_public_key) != Some(e.seat)) {
                return Err(SeatingRefused::NotTheDrawnSeating);
            }
        }
        Ok(checked)
    }

    /// Take a roster's draw that [`check_draw`](Self::check_draw) passed.
    pub fn take_draw(&mut self, d: &DrawWire, checked: Checked) -> Taken {
        if self.done.is_some() {
            return Taken::default();
        }
        if self.round.as_ref().is_none_or(|r| r.number != d.round) {
            self.round = Some(Round {
                number: d.round,
                members: checked.members.clone(),
                digest: checked.digest,
                lot: None,
                commitment: None,
                sealed: None,
                lots_in: BTreeMap::new(),
                openings_in: BTreeMap::new(),
            });
        }
        let table = self.table_id;
        let first_sealing = self.round.as_ref().is_some_and(|r| r.sealed.is_none());
        if first_sealing {
            let sealed_members: Vec<[u8; 32]> = checked.members.clone();
            match self.allowed.as_mut() {
                Some(allowed) => allowed.retain(|k| sealed_members.contains(k)),
                None => self.allowed = Some(sealed_members),
            }
            let Some(r) = self.round.as_mut() else { return Taken::default() };
            r.sealed = Some(Sealed {
                wire: d.clone(),
                lots: checked.lots,
                commitments: checked.members.iter().copied().zip(checked.commitments.iter().copied()).collect(),
            });
        }
        let Some(r) = self.round.as_mut() else { return Taken::default() };
        // Every opening the roster carries is one heard.
        for bytes in &d.openings {
            if let Ok(o) = tabletalk::open_opening(bytes, &table) {
                r.openings_in.entry(o.key).or_insert_with(|| bytes.clone());
            }
        }
        if let Some(drawn) = checked.drawn {
            self.done = Some(Done { wire: d.clone(), digest: draw::draw_digest(d), drawn, members: checked.members });
            return Taken { open_now: false, completed: true };
        }
        let open_now = r.lot.is_some() && !r.openings_in.contains_key(&self.me);
        Taken { open_now, completed: false }
    }

    /// This client's sealed lot for the current round, signed afresh -- said
    /// until the lots are sealed. `None` when there is nothing to say.
    pub fn lot_word(&self, key: &SigningKey, now_ms: u64) -> Option<Vec<u8>> {
        let r = self.round.as_ref().filter(|r| r.sealed.is_none() && self.done.is_none())?;
        let c = r.commitment?;
        tabletalk::lot_word(key, &self.table_id, r.number, &r.digest, &c, now_ms).ok()
    }

    /// This client's opening, signed afresh -- said once its lot is sealed and
    /// until the draw is complete.
    pub fn opening_word(&self, key: &SigningKey, now_ms: u64) -> Option<Vec<u8>> {
        if self.done.is_some() {
            return None;
        }
        let r = self.round.as_ref()?;
        let s = r.sealed.as_ref()?;
        let lot = r.lot.as_ref()?;
        if s.commitments.get(&self.me) != r.commitment.as_ref() {
            return None;
        }
        tabletalk::opening_word(key, &self.table_id, r.number, &s.lots, lot, now_ms).ok()
    }

    /// A member's sealed lot, heard: kept for the current round, the newest a
    /// member said, until the lots are sealed. Returns whether it was kept.
    pub fn hear_lot(&mut self, said: &SaidLot, word: &[u8], said_ms: u64) -> bool {
        let Some(r) = self.round.as_mut() else { return false };
        if r.sealed.is_some() || said.round != r.number || said.members != r.digest || !r.members.contains(&said.key) {
            return false;
        }
        match r.lots_in.get(&said.key) {
            Some((at, _, _)) if *at >= said_ms => false,
            _ => {
                r.lots_in.insert(said.key, (said_ms, word.to_vec(), said.commitment));
                true
            }
        }
    }

    /// A member's opening, heard: kept when it opens that member's sealed lot
    /// under the sealed set. Returns whether it was kept.
    pub fn hear_opening(&mut self, said: &SaidOpening, word: &[u8]) -> bool {
        let table = self.table_id;
        let Some(r) = self.round.as_mut() else { return false };
        let Some(s) = r.sealed.as_ref() else { return false };
        if said.round != r.number || said.lots != s.lots || r.openings_in.contains_key(&said.key) {
            return false;
        }
        let Some(c) = s.commitments.get(&said.key) else { return false };
        if draw::commitment(&table, r.number, &r.digest, &said.key, &said.r, &said.salt) != *c {
            return false;
        }
        r.openings_in.insert(said.key, word.to_vec());
        true
    }

    /// The founder's: the lots of every member are in, so they can be sealed --
    /// with the founder's own opening beside them, since the founder opens first.
    pub fn sealable(&self, key: &SigningKey, now_ms: u64) -> Option<DrawWire> {
        let r = self.round.as_ref().filter(|r| r.sealed.is_none() && self.done.is_none())?;
        if r.members.iter().any(|m| !r.lots_in.contains_key(m)) {
            return None;
        }
        let lots: Vec<Vec<u8>> = r.members.iter().map(|m| r.lots_in[m].1.clone()).collect();
        // The founder's own lot in the set must be the one it drew, or it could
        // not open it.
        if r.lots_in.get(&self.me).map(|(_, _, c)| *c) != r.commitment {
            return None;
        }
        let lots_hash = draw::lots_digest(&lots);
        let lot = r.lot.as_ref()?;
        let opening = tabletalk::opening_word(key, &self.table_id, r.number, &lots_hash, lot, now_ms).ok()?;
        Some(DrawWire { round: r.number, lots, openings: vec![opening] })
    }

    /// The founder's: every sealed lot is opened, so the draw is complete.
    pub fn drawable(&self) -> Option<DrawWire> {
        if self.done.is_some() {
            return None;
        }
        let r = self.round.as_ref()?;
        let s = r.sealed.as_ref()?;
        if s.commitments.keys().any(|m| !r.openings_in.contains_key(m)) {
            return None;
        }
        let openings: Vec<Vec<u8>> = s.commitments.keys().map(|m| r.openings_in[m].clone()).collect();
        Some(DrawWire { round: r.number, lots: s.wire.lots.clone(), openings })
    }

    /// The draw a roster carries now: the complete draw once there is one, and
    /// while the round is under way its sealed lots exactly as they were sealed,
    /// with the founder's opening -- never an opening more, since a roster whose
    /// draw is complete must be re-seated, which only the complete draw's own
    /// roster is. `None` before the lots are sealed.
    ///
    /// **And every opening the founder holds**, ascending by key, while one is
    /// still missing: a player given back after it opened keeps its lot in the
    /// draw, and a member takes an unfinished draw only where every lot whose
    /// player is not at the table is opened in it -- which is what keeps a
    /// founder from sealing the lot of a player nobody sees.
    pub fn carried(&self) -> Option<DrawWire> {
        if let Some(d) = &self.done {
            return Some(d.wire.clone());
        }
        let r = self.round.as_ref()?;
        let s = r.sealed.as_ref()?;
        if s.commitments.keys().all(|m| r.openings_in.contains_key(m)) {
            return Some(s.wire.clone());
        }
        let openings: Vec<Vec<u8>> = s.commitments.keys().filter_map(|m| r.openings_in.get(m).cloned()).collect();
        Some(DrawWire { round: s.wire.round, lots: s.wire.lots.clone(), openings })
    }

    /// Every other member's word this client holds for the round as it waits:
    /// their sealed lots until the lots are sealed, then their openings until the
    /// draw is complete. A member carries them to the founder with its own
    /// (`Formation::draw_tick`), so that a founder that cannot hear one member
    /// directly still gets its word through another -- rather than giving back a
    /// member the others heard, whose opening they would then hold against the
    /// round that followed.
    pub fn held_words(&self) -> Vec<Vec<u8>> {
        if self.done.is_some() {
            return Vec::new();
        }
        let Some(r) = self.round.as_ref() else { return Vec::new() };
        match &r.sealed {
            None => r.lots_in.iter().filter(|(k, _)| **k != self.me).map(|(_, (_, w, _))| w.clone()).collect(),
            Some(_) => r.openings_in.iter().filter(|(k, _)| **k != self.me).map(|(_, w)| w.clone()).collect(),
        }
    }

    /// What each member still owes the round, for the founder's judgement.
    pub fn owed(&self) -> Vec<([u8; 32], Owed)> {
        if self.done.is_some() {
            return Vec::new();
        }
        let Some(r) = self.round.as_ref() else { return Vec::new() };
        match &r.sealed {
            None => r.members.iter().filter(|m| !r.lots_in.contains_key(*m)).map(|m| (*m, Owed::Lot)).collect(),
            Some(s) => s
                .commitments
                .keys()
                .filter(|m| !r.openings_in.contains_key(*m))
                .map(|m| (*m, Owed::Opening))
                .collect(),
        }
    }

    /// Whether this client may leave its round for a new one of `members`.
    ///
    /// **A sealed round is run again only for a member that did not open**, and
    /// that member is not in the new round. Whoever holds the last unopened lot
    /// knows the outcome before anybody else, and a founder working with it
    /// could otherwise re-run every round whose outcome they did not like: with
    /// the same members, at no cost, or by giving back a member that opened
    /// while the one that knows keeps its seat. So the new round must leave out
    /// a member whose opening this client never heard, and that is the price of
    /// every re-run -- a seat. Members given back that did open may go with it:
    /// a member that opened, left and is no longer at the table is nobody's
    /// lever (`release_seats_before_the_first_hand`).
    fn may_leave_round(&self, members: &[[u8; 32]]) -> Result<(), SeatingRefused> {
        let Some(r) = &self.round else { return Ok(()) };
        let Some(s) = &r.sealed else { return Ok(()) };
        let sealed: Vec<&[u8; 32]> = s.commitments.keys().collect();
        if sealed.iter().all(|m| r.openings_in.contains_key(*m)) {
            return Err(SeatingRefused::RoundAbandoned);
        }
        if members.iter().any(|m| !s.commitments.contains_key(m)) {
            return Err(SeatingRefused::AddedAfterSealing);
        }
        // This client itself left out is this client given back, which it
        // learns from the roster whatever it holds.
        if !sealed
            .iter()
            .any(|m| **m != self.me && **m != self.founder && !members.contains(m) && !r.openings_in.contains_key(*m))
        {
            return Err(SeatingRefused::RoundAbandoned);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::lobby::{BlindSchedule, Mode, TableAd, DECK_SUITE_V1};
    use crate::table::formation::SeatEntry;

    const TABLE: Hash = [7u8; 32];
    const NOW: u64 = 1_700_000_000_000;

    fn sk(n: u8) -> SigningKey {
        SigningKey::from_bytes(&[n; 32])
    }

    fn pk(n: u8) -> [u8; 32] {
        sk(n).verifying_key().to_bytes()
    }

    fn ad(seats: u8) -> TableAd {
        TableAd {
            game: 1,
            mode: Mode::TournamentSngPlayMoney.code(),
            preset_id: "CUSTOM".into(),
            table_name: "t".into(),
            small_blind: 50,
            big_blind: 100,
            ante: 0,
            min_buyin: 10_000,
            max_buyin: 10_000,
            start_stack: 10_000,
            players: 1,
            max_players: seats,
            min_players_to_start: seats,
            blind_schedule: BlindSchedule { mode: 1, every_n_hands: 11, first_small_blind: 50, small_blind_cap: 50_000 },
            action_timeout_ms: 20_000,
            action_grace_ms: 5_000,
            crypto_step_timeout_ms: 30_000,
            hand_deadline_ms: 3_300_000,
            join_deadline_ms: 120_000,
            hand_delay_ms: 7_000,
            time_bank_ms: 0,
            button_rule: 1,
            odd_chip_rule: 1,
            showdown_policy: 1,
            password_required: false,
            deck_suite: DECK_SUITE_V1.into(),
            founder_app_key: pk(1),
            founder_peer_id: vec![1],
            timestamp_unix_ms: NOW,
            expires_at_unix_ms: NOW + 60_000,
            founder_tox_key: None,
            tox_chat_id: None,
        }
    }

    /// A roster of the keys `ns` at seats 0.. in that order.
    fn roster(ns: &[u8], seats: &[u8]) -> Roster {
        let mut entries: Vec<SeatEntry> = ns
            .iter()
            .zip(seats)
            .map(|(&n, &s)| SeatEntry {
                seat: s,
                app_public_key: pk(n),
                peer_id: vec![n],
                display_name: format!("p{n}"),
                buyin: 10_000,
                tox_key: None,
            })
            .collect();
        entries.sort_by_key(|e| e.seat);
        Roster::form(entries, &ad(10), false).unwrap()
    }

    /// A table of `ns` run to the end: the founder (the first) and every member
    /// through one round, returning every client's seating.
    struct Table {
        ns: Vec<u8>,
        clients: Vec<Seating>,
    }

    impl Table {
        fn new(ns: &[u8]) -> Table {
            Table { ns: ns.to_vec(), clients: ns.iter().map(|&n| Seating::new(TABLE, pk(n), pk(ns[0]))).collect() }
        }

        fn keys(&self) -> Vec<[u8; 32]> {
            self.ns.iter().map(|&n| pk(n)).collect()
        }

        /// Every client begins round `serial`; every lot reaches the founder.
        fn lots(&mut self, serial: u64) {
            let keys = self.keys();
            let mut words = Vec::new();
            for (i, c) in self.clients.iter_mut().enumerate() {
                c.begin(serial, &keys);
                if let Some(w) = c.lot_word(&sk(self.ns[i]), NOW) {
                    words.push(w);
                }
            }
            for w in words {
                let said = tabletalk::open_lot(&w, &TABLE).unwrap();
                self.clients[0].hear_lot(&said, &w, NOW);
            }
        }

        fn take(&mut self, i: usize, d: &DrawWire, r: &Roster) -> Result<Taken, SeatingRefused> {
            let checked = self.clients[i].check_draw(d, r)?;
            Ok(self.clients[i].take_draw(d, checked))
        }
    }

    #[test]
    fn a_round_runs_to_a_draw_every_member_agrees_on() {
        let mut t = Table::new(&[1, 2, 3, 4]);
        t.lots(5);
        let pre = roster(&[1, 2, 3, 4], &[0, 1, 2, 3]);
        let sealed = t.clients[0].sealable(&sk(1), NOW).expect("every lot is in");
        assert_eq!(sealed.openings.len(), 1, "the founder opens first");
        let mut openings = Vec::new();
        for i in 0..4 {
            let taken = t.take(i, &sealed, &pre).expect("sealed lots, each member's own");
            if i > 0 {
                assert!(taken.open_now, "member {i} opens");
                openings.push(t.clients[i].opening_word(&sk(t.ns[i]), NOW).unwrap());
            }
        }
        for w in &openings {
            let said = tabletalk::open_opening(w, &TABLE).unwrap();
            assert!(t.clients[0].hear_opening(&said, w));
        }
        let full = t.clients[0].drawable().expect("every lot opened");
        // Re-seat as the draw says, and every client takes it.
        let probe = draw::check(&TABLE, &full, 10, |b| tabletalk::open_lot(b, &TABLE).map_err(|_| "x"), |b| {
            tabletalk::open_opening(b, &TABLE).map_err(|_| "x")
        })
        .unwrap();
        let drawn = probe.drawn.unwrap();
        let seats: Vec<u8> = t.ns.iter().map(|&n| drawn.seat_of(&pk(n)).unwrap()).collect();
        let seated = roster(&[1, 2, 3, 4], &seats);
        for i in 0..4 {
            assert!(t.take(i, &full, &seated).expect("the draw").completed);
        }
        let digests: Vec<Hash> = t.clients.iter().map(|c| c.digest_for(&seated).unwrap()).collect();
        assert!(digests.windows(2).all(|w| w[0] == w[1]), "one draw, one digest, everywhere");
        // The roster as it stood before the draw does not fit it.
        if seats != vec![0, 1, 2, 3] {
            assert_eq!(t.clients[1].digest_for(&pre), None);
        }
    }

    /// `B1`: a founder that seals its own lot, sees every opening, and then
    /// says the draw with a different lot of its own is refused by every member
    /// that opened -- its opening names the lots it opened under.
    #[test]
    fn a_founder_that_swaps_its_own_lot_after_the_openings_is_refused() {
        let mut t = Table::new(&[1, 2, 3]);
        t.lots(5);
        let pre = roster(&[1, 2, 3], &[0, 1, 2]);
        let sealed = t.clients[0].sealable(&sk(1), NOW).unwrap();
        for i in 0..3 {
            t.take(i, &sealed, &pre).unwrap();
        }
        // A fresh lot for the founder, the same round and membership.
        let keys = t.keys();
        let digest = draw::members_digest(&TABLE, &keys);
        let other = Lot { r: [0xEE; 32], salt: [0xEF; 32] };
        let c = other.commitment(&TABLE, 5, &digest, &pk(1));
        let swapped = tabletalk::lot_word(&sk(1), &TABLE, 5, &digest, &c, NOW).unwrap();
        let mut lots = sealed.lots.clone();
        let at = keys.iter().copied().collect::<std::collections::BTreeSet<_>>().iter().position(|k| *k == pk(1)).unwrap();
        lots[at] = swapped;
        // With the founder's opening of its new lot beside them, as a sealing
        // must carry.
        let opening = tabletalk::opening_word(&sk(1), &TABLE, 5, &draw::lots_digest(&lots), &other, NOW).unwrap();
        let forged = DrawWire { round: 5, lots, openings: vec![opening] };
        assert_eq!(
            t.clients[1].check_draw(&forged, &pre).err(),
            Some(SeatingRefused::LotsChanged),
            "another set of lots in the round this member opened in"
        );
    }

    /// The refutation's finding 1: a founder that seals the lots without its own
    /// opening could learn every member's before committing its own, and run the
    /// round again by leaving itself out. Every member refuses such a sealing,
    /// and a draw the founder has no lot in.
    #[test]
    fn a_sealing_without_the_founders_opening_is_refused() {
        let mut t = Table::new(&[1, 2, 3]);
        t.lots(5);
        let pre = roster(&[1, 2, 3], &[0, 1, 2]);
        let mut sealed = t.clients[0].sealable(&sk(1), NOW).unwrap();
        sealed.openings.clear();
        assert_eq!(t.clients[1].check_draw(&sealed, &pre).err(), Some(SeatingRefused::FounderDidNotOpen));
        // Nor a draw of the members alone.
        let mut without = Table::new(&[2, 3]);
        without.lots(5);
        let lots_only = without.clients[0].sealable(&sk(2), NOW).unwrap();
        let two = Seating::new(TABLE, pk(2), pk(1));
        assert_eq!(two.check_draw(&lots_only, &roster(&[2, 3], &[1, 2])).err(), Some(SeatingRefused::FounderDidNotOpen));
    }

    /// The refutation's finding 4: a lot in an unfinished draw whose player is
    /// not at the table is one only the founder may know the opening of -- a
    /// last opener nobody can see. Refused unless the roster carries its opening.
    #[test]
    fn a_lot_of_a_player_not_at_the_table_counts_only_opened() {
        let mut t = Table::new(&[1, 2, 3]);
        t.lots(5);
        let sealed = t.clients[0].sealable(&sk(1), NOW).unwrap();
        // Player 3 sealed but not seated, and not opened: refused.
        let two = roster(&[1, 2], &[0, 1]);
        assert_eq!(t.clients[1].check_draw(&sealed, &two).err(), Some(SeatingRefused::UnseatedLot));
        // With its opening carried, it is a player given back after it opened.
        let all = roster(&[1, 2, 3], &[0, 1, 2]);
        t.take(2, &sealed, &all).unwrap();
        let three = t.clients[2].opening_word(&sk(3), NOW).unwrap();
        let mut carried = sealed.clone();
        carried.openings.push(three);
        carried.openings.sort_by_key(|b| tabletalk::open_opening(b, &TABLE).unwrap().key);
        assert!(t.clients[1].check_draw(&carried, &two).is_ok());
    }

    /// The refutation's finding 3: from the first sealing until the table is
    /// set nobody new sits down -- neither a stranger nor a player the round was
    /// run again without, which would make a withheld opening a free veto.
    #[test]
    fn nobody_new_is_in_a_round_once_one_was_sealed() {
        let mut t = Table::new(&[1, 2, 3, 4]);
        t.lots(5);
        let pre = roster(&[1, 2, 3, 4], &[0, 1, 2, 3]);
        let sealed = t.clients[0].sealable(&sk(1), NOW).unwrap();
        for i in 0..4 {
            t.take(i, &sealed, &pre).unwrap();
        }
        assert!(t.clients[1].frozen());
        // Player 4 does not open; the round runs again without it.
        assert_eq!(t.clients[1].check_plain(&[pk(1), pk(2), pk(3)]), Ok(()));
        t.clients[1].begin(6, &[pk(1), pk(2), pk(3)]);
        // Player 4 back, or a stranger: refused.
        assert_eq!(t.clients[1].check_plain(&[pk(1), pk(2), pk(3), pk(4)]), Err(SeatingRefused::AddedAfterSealing));
        assert_eq!(t.clients[1].check_plain(&[pk(1), pk(2), pk(3), pk(9)]), Err(SeatingRefused::AddedAfterSealing));
        assert_eq!(t.clients[1].check_plain(&[pk(1), pk(2)]), Ok(()), "fewer, as players go");
    }

    /// `B2`: a lot of one round is no lot of the next, even for the same
    /// players -- a member refuses a sealing of lots it never drew.
    #[test]
    fn a_member_refuses_lots_sealed_from_an_earlier_round() {
        let mut t = Table::new(&[1, 2, 3]);
        t.lots(5);
        let old = t.clients[0].sealable(&sk(1), NOW).unwrap();
        // The table goes round again before the sealing: round 9, same players.
        t.lots(9);
        let pre = roster(&[1, 2, 3], &[0, 1, 2]);
        assert_eq!(
            t.clients[2].check_draw(&old, &pre).err(),
            Some(SeatingRefused::StaleRound { got: 5, held: 9 })
        );
        let mut replayed = old.clone();
        replayed.round = 9;
        assert!(t.clients[2].check_draw(&replayed, &pre).is_err(), "the round is inside every lot");
    }

    /// `B3`: once the lots are sealed nobody sits down -- a roster with a player
    /// the draw does not have is refused.
    #[test]
    fn a_player_added_after_the_sealing_is_refused() {
        let mut t = Table::new(&[1, 2, 3]);
        t.lots(5);
        let pre = roster(&[1, 2, 3], &[0, 1, 2]);
        let sealed = t.clients[0].sealable(&sk(1), NOW).unwrap();
        t.take(1, &sealed, &pre).unwrap();
        let grown = roster(&[1, 2, 3, 4], &[0, 1, 2, 3]);
        assert_eq!(t.clients[1].check_draw(&sealed, &grown).err(), Some(SeatingRefused::NotTheDrawnSeating));
        assert_eq!(
            t.clients[1].check_plain(&[pk(1), pk(2), pk(3), pk(4)]),
            Err(SeatingRefused::AddedAfterSealing)
        );
    }

    /// `B4`, `B5`: a sealed round is run again only without a member that did
    /// not open. Whoever holds the last unopened lot knows the outcome first, so
    /// a new round with the same players, or one that leaves out only players
    /// who opened, is a re-run a founder chose for free -- refused; and one once
    /// every lot is opened is a draw whose outcome everybody has seen.
    #[test]
    fn a_round_is_run_again_only_without_a_member_that_did_not_open() {
        let mut t = Table::new(&[1, 2, 3, 4]);
        t.lots(5);
        let pre = roster(&[1, 2, 3, 4], &[0, 1, 2, 3]);
        let sealed = t.clients[0].sealable(&sk(1), NOW).unwrap();
        for i in 0..4 {
            t.take(i, &sealed, &pre).unwrap();
        }
        let three = t.clients[2].opening_word(&sk(3), NOW).unwrap();
        let said = tabletalk::open_opening(&three, &TABLE).unwrap();
        assert!(t.clients[1].hear_opening(&said, &three), "member 2 hears member 3 open");
        // Client 1 (member 2) holds the founder's opening and 3's; 4 has not
        // opened, and neither has member 2 itself.
        let c = &t.clients[1];
        assert_eq!(c.check_plain(&[pk(1), pk(2), pk(3), pk(4)]), Err(SeatingRefused::RoundAbandoned), "the same players again");
        assert_eq!(c.check_plain(&[pk(1), pk(2), pk(4)]), Err(SeatingRefused::RoundAbandoned), "without one that opened");
        assert_eq!(c.check_plain(&[pk(1), pk(2), pk(3)]), Ok(()), "without the one that did not open");
        assert_eq!(c.check_plain(&[pk(1), pk(2)]), Ok(()), "and one that opened may go with it");
        // Every lot opened: no new round at all.
        for n in [2u8, 4] {
            let i = usize::from(n - 1);
            let w = t.clients[i].opening_word(&sk(n), NOW).unwrap();
            let said = tabletalk::open_opening(&w, &TABLE).unwrap();
            assert!(t.clients[1].hear_opening(&said, &w));
        }
        assert_eq!(t.clients[1].check_plain(&[pk(1), pk(2), pk(3)]), Err(SeatingRefused::RoundAbandoned));
    }

    #[test]
    fn a_complete_draw_is_never_discarded() {
        let mut t = Table::new(&[1, 2]);
        t.lots(5);
        let pre = roster(&[1, 2], &[0, 1]);
        let sealed = t.clients[0].sealable(&sk(1), NOW).unwrap();
        t.take(1, &sealed, &pre).unwrap();
        let w = t.clients[1].opening_word(&sk(2), NOW).unwrap();
        let said = tabletalk::open_opening(&w, &TABLE).unwrap();
        let founders = t.clients[0].check_draw(&sealed, &pre).unwrap();
        t.clients[0].take_draw(&sealed, founders);
        assert!(t.clients[0].hear_opening(&said, &w));
        let full = t.clients[0].drawable().unwrap();
        let probe = draw::check(&TABLE, &full, 10, |b| tabletalk::open_lot(b, &TABLE).map_err(|_| "x"), |b| {
            tabletalk::open_opening(b, &TABLE).map_err(|_| "x")
        })
        .unwrap();
        let d = probe.drawn.unwrap();
        let seated = roster(&[1, 2], &[d.seat_of(&pk(1)).unwrap(), d.seat_of(&pk(2)).unwrap()]);
        assert!(t.take(1, &full, &seated).unwrap().completed);
        assert_eq!(t.clients[1].check_plain(&[pk(1), pk(2)]), Err(SeatingRefused::DrawDiscarded));
        let mut other = full.clone();
        other.openings.pop();
        assert_eq!(t.clients[1].check_draw(&other, &seated).err(), Some(SeatingRefused::DrawDiscarded));
        // A member given back leaves its seat empty and the draw stands.
        let one = roster(&[2], &[d.seat_of(&pk(2)).unwrap()]);
        assert!(t.clients[1].check_draw(&full, &one).is_ok());
        // And comes back only to the seat the draw gave it.
        assert_eq!(t.clients[1].drawn_seat(&pk(1)), d.seat_of(&pk(1)));
    }

    /// A member whose lot the roster seals is not the one it drew -- a restart
    /// lost the first, or the founder made one up -- refuses: it could not
    /// open, and taking it would only have it given back later.
    #[test]
    fn a_member_refuses_a_sealing_of_a_lot_it_did_not_draw() {
        let mut t = Table::new(&[1, 2]);
        t.lots(5);
        let pre = roster(&[1, 2], &[0, 1]);
        let sealed = t.clients[0].sealable(&sk(1), NOW).unwrap();
        // Member 2 restarts: a fresh client, a fresh lot for the same round.
        let mut restarted = Seating::new(TABLE, pk(2), pk(1));
        restarted.begin(5, &t.keys());
        assert_eq!(restarted.check_draw(&sealed, &pre).err(), Some(SeatingRefused::NotMyLot));
    }

    #[test]
    fn the_founder_owes_nothing_and_the_rest_owe_what_the_round_waits_on() {
        let mut t = Table::new(&[1, 2, 3]);
        let keys = t.keys();
        for c in t.clients.iter_mut() {
            c.begin(5, &keys);
        }
        let own = t.clients[0].lot_word(&sk(1), NOW).unwrap();
        let said = tabletalk::open_lot(&own, &TABLE).unwrap();
        t.clients[0].hear_lot(&said, &own, NOW);
        let mut owed = t.clients[0].owed();
        owed.sort();
        let mut want = vec![(pk(2), Owed::Lot), (pk(3), Owed::Lot)];
        want.sort();
        assert_eq!(owed, want);
    }

    /// The newest lot a member said is the one kept, so a lot replayed from
    /// before a restart does not displace the one the member can open.
    #[test]
    fn the_founder_keeps_a_members_newest_lot() {
        let mut t = Table::new(&[1, 2]);
        t.lots(5);
        let keys = t.keys();
        let digest = draw::members_digest(&TABLE, &keys);
        let stale = tabletalk::lot_word(&sk(2), &TABLE, 5, &digest, &[9u8; 32], NOW - 5_000).unwrap();
        let said = tabletalk::open_lot(&stale, &TABLE).unwrap();
        assert!(!t.clients[0].hear_lot(&said, &stale, NOW - 5_000), "older than the lot held");
    }
}
