//! `D-083` (`S1-B`, `S1-AD`): the seating draw -- who sits where at a table, and
//! who holds its first button, decided by lots its members draw and nobody can
//! choose.
//!
//! # Why a draw at all
//!
//! Before it, the founder seated players in the order they arrived, a joiner
//! could name the seat it wanted (`JOIN_REQUEST n(4)`), and the first button was
//! read off `session_id` -- a hash the last seat to ratify could re-sign until it
//! came out as it liked, in under a hundred tries at six seats (`S1-B`). Two
//! players working together could sit side by side for a whole tournament
//! (`S1-AD`).
//!
//! # How
//!
//! A round of the draw is for one roster's members and is numbered by that
//! roster's serial. Every member draws a secret lot -- `r` and `salt`, 32 bytes
//! each from the operating system -- and first says only a commitment to it,
//! bound to the table, the round, the membership and itself. The founder fixes
//! the set of commitments in a roster it says again, with its own lot opened
//! beside them; only then does any other member open, and its opening names the
//! sealed set it opened under. The seed is a hash over every opened lot, so it is
//! uniform as long as one member drew honestly, and no member could choose its
//! lot after seeing another's. From the seed come the seating and the first
//! button, by the rules below, which every client computes alike.
//!
//! # What is here and what is not
//!
//! This module is the arithmetic and the wire shape of the draw: the membership
//! digest, the commitment, the seed, the seating, the button, and the checks a
//! receiver runs on a roster that carries a draw. Who says what when, and who is
//! given back for not saying it, is the formation's (`net::formation`,
//! `net::run`) -- `PROTOCOL.md` §4.4 carries the rules and `DECISIONS.md` `D-083`
//! the reasons.

use crate::poker::state::{Hash, SeatIdx};
use crate::protocol::signatures::{hash, Domain};

/// The bytes of a lot's secret, and of its salt.
pub const LOT_BYTES: usize = 32;

/// The digest of a membership: the table, and its members' application keys in
/// ascending order.
///
/// Keys and not seats: the seats are what the draw decides, so they cannot be
/// what it is keyed on.
pub fn members_digest(table_id: &Hash, keys: &[[u8; 32]]) -> Hash {
    let keys = sorted_keys(keys);
    let mut parts: Vec<&[u8]> = Vec::with_capacity(keys.len() + 1);
    parts.push(table_id);
    for key in &keys {
        parts.push(key);
    }
    hash(Domain::RngMembers, &parts)
}

/// `PROTOCOL.md` §4.4's commitment to a lot: bound to the table, the round, the
/// membership and the member, so a lot can be carried to no other table, no
/// other round -- a membership can come round again, a round never does -- and
/// no other player.
pub fn commitment(
    table_id: &Hash,
    round: u64,
    members: &Hash,
    app_key: &[u8; 32],
    r: &[u8; LOT_BYTES],
    salt: &[u8; LOT_BYTES],
) -> Hash {
    hash(Domain::RngCommit, &[table_id, &round.to_be_bytes(), members, app_key, r, salt])
}

/// A member's own lot, kept until it is opened.
#[derive(Clone, PartialEq, Eq)]
pub struct Lot {
    pub r: [u8; LOT_BYTES],
    pub salt: [u8; LOT_BYTES],
}

impl std::fmt::Debug for Lot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A lot is a secret until it is opened, and a log line is not the
        // place it is opened in.
        f.write_str("Lot(..)")
    }
}

impl Lot {
    /// A fresh lot from the operating system's generator (`SPEC_CS.md` §7).
    pub fn draw() -> Result<Lot, getrandom::Error> {
        Ok(Lot {
            r: crate::security::rng::array()?,
            salt: crate::security::rng::array()?,
        })
    }

    /// The commitment this member says before anybody opens anything.
    pub fn commitment(&self, table_id: &Hash, round: u64, members: &Hash, app_key: &[u8; 32]) -> Hash {
        commitment(table_id, round, members, app_key, &self.r, &self.salt)
    }
}

/// The digest of a round's sealed lots, in the order the roster carries them.
/// Every opening names it: a member opens under one set of lots and no other.
pub fn lots_digest(lots: &[Vec<u8>]) -> Hash {
    let parts: Vec<&[u8]> = lots.iter().map(|l| l.as_slice()).collect();
    hash(Domain::RngLots, &parts)
}

/// The seed: the table, the round, the membership, the sealed lots, and every
/// member's `r` in ascending order of the members' keys.
///
/// `openings` pairs each member's key with its `r`; the order it arrives in
/// does not matter. The `r` values alone would do -- each is bound to the rest by
/// its commitment -- and the rest is here so that the seed says by itself which
/// draw it is.
pub fn seed(
    table_id: &Hash,
    round: u64,
    members: &Hash,
    lots: &Hash,
    openings: &[([u8; 32], [u8; LOT_BYTES])],
) -> Hash {
    let mut sorted = openings.to_vec();
    sorted.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    let round = round.to_be_bytes();
    let mut parts: Vec<&[u8]> = vec![table_id, &round, members, lots];
    parts.extend(sorted.iter().map(|(_, r)| r.as_slice()));
    hash(Domain::RngBeacon, &parts)
}

/// What the seed says about one question of the draw, as a number.
///
/// `role` 1 is a step of the seating's shuffle, `role` 2 the first button. The
/// modulo that follows at each use is biased by less than one part in 10^18 at
/// ten seats, which is accepted rather than rejected-and-redrawn: a rule with a
/// loop in it is one more thing two implementations can write differently.
fn pick(seed: &Hash, role: u8, index: u8) -> u64 {
    let out = hash(Domain::SeatDraw, &[seed, &[role], &[index]]);
    u64::from_be_bytes([out[0], out[1], out[2], out[3], out[4], out[5], out[6], out[7]])
}

/// The seating: `members`' keys in seat order, seat `i` holding entry `i`.
///
/// The members are sorted by key and shuffled by Fisher-Yates -- for `i` from
/// `n - 1` down to `1`, entry `i` swaps with entry `pick(seed, 1, i) mod (i +
/// 1)` -- so every seating is equally likely whatever order anybody arrived in.
/// The drawn table is compact: `n` members sit at seats `0` to `n - 1`.
pub fn seating(seed: &Hash, members: &[[u8; 32]]) -> Vec<[u8; 32]> {
    let mut order = sorted_keys(members);
    for i in (1..order.len()).rev() {
        let j = (pick(seed, 1, i as u8) % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order
}

/// The first button's seat on a drawn table of `n` members.
pub fn button(seed: &Hash, n: usize) -> SeatIdx {
    debug_assert!(n >= 1);
    (pick(seed, 2, 0) % n.max(1) as u64) as SeatIdx
}

/// The first button at the table that is set: the drawn seat when a player
/// still sits there, and otherwise the first occupied seat above it -- ascending
/// seat numbers, wrapping past the last to the first, which is the ring the
/// dealing order goes round.
///
/// A seat given back after the draw leaves everybody else where the draw put
/// them, and moves the button only if it was the button's own seat, and then by
/// the ordinary rule of the ring.
pub fn first_button(drawn: SeatIdx, occupied: &[SeatIdx]) -> Option<SeatIdx> {
    let mut seats = occupied.to_vec();
    seats.sort_unstable();
    seats.dedup();
    if seats.contains(&drawn) {
        return Some(drawn);
    }
    seats.iter().copied().find(|&s| s > drawn).or_else(|| seats.first().copied())
}

fn sorted_keys(keys: &[[u8; 32]]) -> Vec<[u8; 32]> {
    let mut keys = keys.to_vec();
    keys.sort_unstable();
    keys.dedup();
    keys
}

/// The draw as a roster carries it (`PLAYER_LIST n(4)`, `PROTOCOL.md` §4.3).
///
/// `round` is the serial of the roster whose members are drawn. `lots` is every
/// member's signed `RNG_COMMIT` for that round, whole, ascending by the member's
/// key. `openings` is signed `RNG_REVEAL`s, ascending likewise: the founder's
/// alone when the lots are sealed -- the founder opens first, so it can never be
/// the one that sees every other lot before it opens its own -- and every
/// member's once the draw is complete. A roster with every opening is the whole
/// draw: a receiver checks it from the roster alone, needing no word it may have
/// missed, and nothing in it is checked against a clock.
#[derive(Debug, Clone, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct DrawWire {
    #[n(0)]
    pub round: u64,
    #[cbor(n(1), with = "crate::protocol::serialization::byte_strings")]
    pub lots: Vec<Vec<u8>>,
    #[cbor(n(2), with = "crate::protocol::serialization::byte_strings")]
    pub openings: Vec<Vec<u8>>,
}

/// The digest `TABLE_READY` binds: the draw as the ratified roster carried it.
pub fn draw_digest(draw: &DrawWire) -> Hash {
    let bytes = crate::protocol::serialization::to_canonical(draw).unwrap_or_default();
    hash(Domain::RngDraw, &[&bytes])
}

/// A lot as a checked `RNG_COMMIT` says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaidLot {
    /// The member's application key, which signed the word.
    pub key: [u8; 32],
    pub round: u64,
    /// The membership the lot was drawn for.
    pub members: Hash,
    pub commitment: Hash,
}

/// An opening as a checked `RNG_REVEAL` says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaidOpening {
    pub key: [u8; 32],
    pub round: u64,
    /// The sealed lots the member opened under ([`lots_digest`]).
    pub lots: Hash,
    pub r: [u8; LOT_BYTES],
    pub salt: [u8; LOT_BYTES],
}

/// Why a roster's draw is not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawRefused {
    /// A lot that is not a checked `RNG_COMMIT` of this table.
    BadLot(&'static str),
    /// An opening that is not a checked `RNG_REVEAL` of this table.
    BadOpening(&'static str),
    /// Two lots, or two openings, signed by one member.
    TwoFromOneMember,
    /// Lots or openings not in ascending order of their members' keys.
    OutOfOrder,
    /// A lot or an opening of another round than the draw's.
    AnotherRound,
    /// A lot drawn for another membership than the draw's.
    AnotherMembership,
    /// The lots' signers are not the membership they say they were drawn for.
    NotTheMembership,
    /// An opening made under another set of lots than these.
    NotTheseLots,
    /// An opening by a key that drew no lot here.
    NotAMember,
    /// An opening that does not open its lot.
    DoesNotOpen,
    /// No lots, or more than a table has seats.
    Size,
}

/// A draw that has been checked, with what it decides once it is opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub round: u64,
    /// The membership, ascending by key.
    pub members: Vec<[u8; 32]>,
    /// Its digest, which every lot names.
    pub digest: Hash,
    /// The sealed lots' digest, which every opening names.
    pub lots: Hash,
    /// Each member's commitment, in the members' order.
    pub commitments: Vec<Hash>,
    /// The members whose lots are opened here, and their `r`.
    pub opened: Vec<([u8; 32], [u8; LOT_BYTES])>,
    /// Present once every lot is opened.
    pub drawn: Option<Drawn>,
}

/// What an opened draw decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drawn {
    pub seed: Hash,
    /// Keys in seat order: seat `i` holds `seating[i]`.
    pub seating: Vec<[u8; 32]>,
    /// The first button's seat on the drawn table.
    pub button: SeatIdx,
}

impl Drawn {
    /// The seat the draw gave `key`, if it is a member.
    pub fn seat_of(&self, key: &[u8; 32]) -> Option<SeatIdx> {
        self.seating.iter().position(|k| k == key).map(|i| i as SeatIdx)
    }
}

/// Check a draw as a roster carries it.
///
/// `open_lot` and `open_opening` take one word's bytes to what it says --
/// signature, type and table -- and are the caller's, because a word is checked
/// where every other word of the table is (`net::tabletalk`). What is checked
/// here is what makes the words one draw: one lot a member, ascending, all of one
/// round and one membership whose digest is the lots' own signers; one opening a
/// member, of that round, naming these lots, each opening its lot.
pub fn check(
    table_id: &Hash,
    draw: &DrawWire,
    max_members: usize,
    open_lot: impl Fn(&[u8]) -> Result<SaidLot, &'static str>,
    open_opening: impl Fn(&[u8]) -> Result<SaidOpening, &'static str>,
) -> Result<Checked, DrawRefused> {
    if draw.lots.is_empty() || draw.lots.len() > max_members || draw.openings.len() > draw.lots.len() {
        return Err(DrawRefused::Size);
    }
    let mut said: Vec<SaidLot> = Vec::with_capacity(draw.lots.len());
    for lot in &draw.lots {
        let s = open_lot(lot).map_err(DrawRefused::BadLot)?;
        if s.round != draw.round {
            return Err(DrawRefused::AnotherRound);
        }
        said.push(s);
    }
    ascending(said.iter().map(|s| &s.key))?;
    let members: Vec<[u8; 32]> = said.iter().map(|s| s.key).collect();
    let digest = members_digest(table_id, &members);
    if said.iter().any(|s| s.members != digest) {
        return Err(if said.iter().all(|s| s.members == said[0].members) {
            DrawRefused::NotTheMembership
        } else {
            DrawRefused::AnotherMembership
        });
    }
    let lots = lots_digest(&draw.lots);
    let commitments: Vec<Hash> = said.iter().map(|s| s.commitment).collect();

    let mut opened: Vec<([u8; 32], [u8; LOT_BYTES])> = Vec::with_capacity(draw.openings.len());
    let mut opening_keys: Vec<[u8; 32]> = Vec::with_capacity(draw.openings.len());
    for bytes in &draw.openings {
        let o = open_opening(bytes).map_err(DrawRefused::BadOpening)?;
        if o.round != draw.round {
            return Err(DrawRefused::AnotherRound);
        }
        if o.lots != lots {
            return Err(DrawRefused::NotTheseLots);
        }
        let lot = said.iter().find(|s| s.key == o.key).ok_or(DrawRefused::NotAMember)?;
        if commitment(table_id, draw.round, &digest, &o.key, &o.r, &o.salt) != lot.commitment {
            return Err(DrawRefused::DoesNotOpen);
        }
        opening_keys.push(o.key);
        opened.push((o.key, o.r));
    }
    ascending(opening_keys.iter())?;

    let drawn = (opened.len() == said.len()).then(|| {
        let seed = seed(table_id, draw.round, &digest, &lots, &opened);
        let seating = seating(&seed, &members);
        let button = button(&seed, members.len());
        Drawn { seed, seating, button }
    });
    Ok(Checked { round: draw.round, members, digest, lots, commitments, opened, drawn })
}

fn ascending<'a>(keys: impl Iterator<Item = &'a [u8; 32]>) -> Result<(), DrawRefused> {
    let keys: Vec<&[u8; 32]> = keys.collect();
    for pair in keys.windows(2) {
        match pair[0].cmp(pair[1]) {
            std::cmp::Ordering::Less => {}
            std::cmp::Ordering::Equal => return Err(DrawRefused::TwoFromOneMember),
            std::cmp::Ordering::Greater => return Err(DrawRefused::OutOfOrder),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: Hash = [7u8; 32];
    const ROUND: u64 = 5;

    fn key(n: u8) -> [u8; 32] {
        [n; 32]
    }

    fn lot(n: u8) -> Lot {
        Lot { r: [n.wrapping_mul(3); 32], salt: [n.wrapping_add(100); 32] }
    }

    /// Stand-ins for the word checks: a lot is `key ‖ round ‖ members ‖
    /// commitment` and an opening `key ‖ round ‖ lots ‖ r ‖ salt`, unsigned,
    /// which is all `check` reads of them. The real words are the last test's.
    fn lot_bytes(s: &SaidLot) -> Vec<u8> {
        [&s.key[..], &s.round.to_be_bytes(), &s.members, &s.commitment].concat()
    }

    fn open_lot(bytes: &[u8]) -> Result<SaidLot, &'static str> {
        if bytes.len() != 104 {
            return Err("not a stand-in lot");
        }
        let mut s = SaidLot { key: [0; 32], round: 0, members: [0; 32], commitment: [0; 32] };
        s.key.copy_from_slice(&bytes[..32]);
        s.round = u64::from_be_bytes(bytes[32..40].try_into().unwrap());
        s.members.copy_from_slice(&bytes[40..72]);
        s.commitment.copy_from_slice(&bytes[72..]);
        Ok(s)
    }

    fn opening_bytes(o: &SaidOpening) -> Vec<u8> {
        [&o.key[..], &o.round.to_be_bytes(), &o.lots, &o.r, &o.salt].concat()
    }

    fn open_opening(bytes: &[u8]) -> Result<SaidOpening, &'static str> {
        if bytes.len() != 136 {
            return Err("not a stand-in opening");
        }
        let mut o = SaidOpening { key: [0; 32], round: 0, lots: [0; 32], r: [0; 32], salt: [0; 32] };
        o.key.copy_from_slice(&bytes[..32]);
        o.round = u64::from_be_bytes(bytes[32..40].try_into().unwrap());
        o.lots.copy_from_slice(&bytes[40..72]);
        o.r.copy_from_slice(&bytes[72..104]);
        o.salt.copy_from_slice(&bytes[104..]);
        Ok(o)
    }

    /// The draw of `members` at `ROUND`, opened by `opening` of them (all when
    /// `None`).
    fn draw_of(members: &[u8], opening: Option<&[u8]>) -> DrawWire {
        let keys: Vec<[u8; 32]> = members.iter().map(|&n| key(n)).collect();
        let digest = members_digest(&TABLE, &keys);
        let mut sorted: Vec<u8> = members.to_vec();
        sorted.sort_unstable();
        let lots: Vec<Vec<u8>> = sorted
            .iter()
            .map(|&n| {
                let c = lot(n).commitment(&TABLE, ROUND, &digest, &key(n));
                lot_bytes(&SaidLot { key: key(n), round: ROUND, members: digest, commitment: c })
            })
            .collect();
        let d = lots_digest(&lots);
        let who: Vec<u8> = match opening {
            Some(some) => {
                let mut s = some.to_vec();
                s.sort_unstable();
                s
            }
            None => sorted.clone(),
        };
        let openings = who
            .iter()
            .map(|&n| {
                let l = lot(n);
                opening_bytes(&SaidOpening { key: key(n), round: ROUND, lots: d, r: l.r, salt: l.salt })
            })
            .collect();
        DrawWire { round: ROUND, lots, openings }
    }

    fn checked(draw: &DrawWire) -> Result<Checked, DrawRefused> {
        check(&TABLE, draw, 10, open_lot, open_opening)
    }

    #[test]
    fn the_membership_digest_is_the_set_and_not_its_order() {
        let a = members_digest(&TABLE, &[key(1), key(2), key(3)]);
        let b = members_digest(&TABLE, &[key(3), key(1), key(2)]);
        assert_eq!(a, b, "the same players in another order are the same membership");
        assert_ne!(a, members_digest(&TABLE, &[key(1), key(2)]), "a player fewer is another membership");
        assert_ne!(a, members_digest(&[8u8; 32], &[key(1), key(2), key(3)]), "another table is another membership");
    }

    #[test]
    fn a_commitment_is_bound_to_table_round_membership_member_and_lot() {
        let m = members_digest(&TABLE, &[key(1), key(2)]);
        let base = commitment(&TABLE, 3, &m, &key(1), &[1; 32], &[2; 32]);
        assert_ne!(base, commitment(&[8u8; 32], 3, &m, &key(1), &[1; 32], &[2; 32]), "table");
        assert_ne!(base, commitment(&TABLE, 4, &m, &key(1), &[1; 32], &[2; 32]), "round");
        assert_ne!(base, commitment(&TABLE, 3, &[9u8; 32], &key(1), &[1; 32], &[2; 32]), "membership");
        assert_ne!(base, commitment(&TABLE, 3, &m, &key(2), &[1; 32], &[2; 32]), "member");
        assert_ne!(base, commitment(&TABLE, 3, &m, &key(1), &[3; 32], &[2; 32]), "r");
        assert_ne!(base, commitment(&TABLE, 3, &m, &key(1), &[1; 32], &[4; 32]), "salt");
    }

    #[test]
    fn the_seed_does_not_depend_on_the_order_the_openings_came_in() {
        let m = [1u8; 32];
        let l = [2u8; 32];
        let a = seed(&TABLE, ROUND, &m, &l, &[(key(1), [1; 32]), (key(2), [2; 32]), (key(3), [3; 32])]);
        let b = seed(&TABLE, ROUND, &m, &l, &[(key(3), [3; 32]), (key(1), [1; 32]), (key(2), [2; 32])]);
        assert_eq!(a, b);
        assert_ne!(a, seed(&TABLE, ROUND, &m, &l, &[(key(1), [1; 32]), (key(2), [2; 32]), (key(3), [4; 32])]), "one lot moves it");
        assert_ne!(a, seed(&TABLE, ROUND + 1, &m, &l, &[(key(1), [1; 32]), (key(2), [2; 32]), (key(3), [3; 32])]), "the round moves it");
    }

    #[test]
    fn the_seating_seats_every_member_once_whatever_order_they_came_in() {
        let s = [5u8; 32];
        let members: Vec<[u8; 32]> = (1..=10).map(key).collect();
        let mut reversed = members.clone();
        reversed.reverse();
        let seated = seating(&s, &members);
        assert_eq!(seated, seating(&s, &reversed), "arrival order decides nothing");
        let mut sorted = seated.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, members, "every member seated exactly once");
    }

    /// Every seat is equally likely for every member, within what a thousand
    /// seeds can show: at ten seats each (member, seat) pair expects 100, and a
    /// fair shuffle keeps every one inside 50..=150 with overwhelming margin.
    /// A shuffle that swapped with `i` instead of `i + 1` values, the classic
    /// slip, never leaves a member where it started and fails at once.
    #[test]
    fn the_seating_is_not_lopsided() {
        let members: Vec<[u8; 32]> = (1..=10).map(key).collect();
        let mut count = [[0u32; 10]; 10];
        for i in 0..1000u32 {
            let s = hash(Domain::RngBeacon, &[&i.to_be_bytes()]);
            for (seat, k) in seating(&s, &members).iter().enumerate() {
                count[(k[0] - 1) as usize][seat] += 1;
            }
        }
        for (member, seats) in count.iter().enumerate() {
            for (seat, &n) in seats.iter().enumerate() {
                assert!((50..=150).contains(&n), "member {member} at seat {seat}: {n} of 1000");
            }
        }
    }

    #[test]
    fn the_button_is_a_seat_of_the_drawn_table_and_every_seat_can_have_it() {
        let mut seen = [false; 6];
        for i in 0..200u32 {
            let s = hash(Domain::RngBeacon, &[&i.to_be_bytes()]);
            let b = button(&s, 6);
            assert!(b < 6);
            seen[b as usize] = true;
        }
        assert!(seen.iter().all(|&x| x), "every seat held the button at least once in 200 draws");
    }

    #[test]
    fn a_vacated_button_seat_passes_the_button_clockwise() {
        assert_eq!(first_button(3, &[0, 1, 3, 5]), Some(3), "occupied: stays");
        assert_eq!(first_button(3, &[0, 1, 5]), Some(5), "vacated: the next seat up");
        assert_eq!(first_button(7, &[0, 2, 5]), Some(0), "past the last seat: wraps");
        assert_eq!(first_button(2, &[]), None);
    }

    #[test]
    fn a_draw_is_checked_whole_and_decides_the_same_for_everybody() {
        let draw = draw_of(&[4, 2, 9], None);
        let one = checked(&draw).expect("an honest draw");
        assert_eq!(one, checked(&draw.clone()).expect("an honest draw"));
        let drawn = one.drawn.expect("opened");
        assert_eq!(drawn.seating.len(), 3);
        assert!(drawn.button < 3);
        for k in [key(2), key(4), key(9)] {
            assert!(drawn.seat_of(&k).is_some());
        }
    }

    #[test]
    fn sealed_lots_with_the_founders_opening_decide_nothing_yet() {
        let c = checked(&draw_of(&[1, 2, 3], Some(&[2]))).expect("sealed lots, one opened");
        assert_eq!(c.drawn, None);
        assert_eq!(c.commitments.len(), 3);
        assert_eq!(c.opened.len(), 1);
    }

    #[test]
    fn a_wrong_opening_is_refused() {
        let mut draw = draw_of(&[1, 2, 3], None);
        let last = draw.openings[1].len() - 1;
        draw.openings[1][last] ^= 1;
        assert_eq!(checked(&draw), Err(DrawRefused::DoesNotOpen));
    }

    /// `B1` of the refutation: an opening names the sealed lots it was made
    /// under, so a founder that swaps its own lot after seeing the others'
    /// openings holds openings of another set -- and is refused.
    #[test]
    fn an_opening_under_other_lots_is_refused() {
        let mut draw = draw_of(&[1, 2, 3], None);
        let swapped = {
            let keys: Vec<[u8; 32]> = [1u8, 2, 3].iter().map(|&n| key(n)).collect();
            let digest = members_digest(&TABLE, &keys);
            let other = Lot { r: [0xAB; 32], salt: [0xCD; 32] };
            let c = other.commitment(&TABLE, ROUND, &digest, &key(1));
            lot_bytes(&SaidLot { key: key(1), round: ROUND, members: digest, commitment: c })
        };
        draw.lots[0] = swapped;
        assert!(matches!(checked(&draw), Err(DrawRefused::NotTheseLots)));
    }

    #[test]
    fn lots_or_openings_out_of_order_or_doubled_are_refused() {
        let mut draw = draw_of(&[1, 2, 3], Some(&[]));
        draw.lots.swap(0, 1);
        assert_eq!(checked(&draw), Err(DrawRefused::OutOfOrder));
        let mut draw = draw_of(&[1, 2, 3], Some(&[]));
        let first = draw.lots[0].clone();
        draw.lots.insert(1, first);
        assert_eq!(checked(&draw), Err(DrawRefused::TwoFromOneMember));
        let mut draw = draw_of(&[1, 2, 3], None);
        draw.openings.swap(0, 2);
        assert_eq!(checked(&draw), Err(DrawRefused::OutOfOrder));
    }

    /// A lot of another round, or drawn for the membership with a player who
    /// has since gone, is not a lot of this draw: a new round has fresh lots,
    /// so a lot seen opened in one round decides nothing in the next.
    #[test]
    fn a_lot_of_another_round_or_membership_is_refused() {
        let mut draw = draw_of(&[1, 2, 3], Some(&[]));
        draw.lots.pop();
        assert_eq!(checked(&draw), Err(DrawRefused::NotTheMembership));
        let mut mixed = draw_of(&[1, 2], Some(&[]));
        let other = draw_of(&[1, 2, 3], Some(&[]));
        mixed.lots[1] = other.lots[1].clone();
        assert_eq!(checked(&mixed), Err(DrawRefused::AnotherMembership));
        let mut later = draw_of(&[1, 2], Some(&[]));
        later.round = ROUND + 1;
        assert_eq!(checked(&later), Err(DrawRefused::AnotherRound));
    }

    #[test]
    fn an_opening_by_a_stranger_is_refused() {
        let mut draw = draw_of(&[1, 2, 3], Some(&[1]));
        let d = lots_digest(&draw.lots);
        draw.openings[0] = opening_bytes(&SaidOpening { key: key(9), round: ROUND, lots: d, r: [0; 32], salt: [0; 32] });
        assert_eq!(checked(&draw), Err(DrawRefused::NotAMember));
    }

    #[test]
    fn a_draw_larger_than_the_table_is_refused() {
        let draw = draw_of(&[1, 2, 3], Some(&[]));
        assert_eq!(check(&TABLE, &draw, 2, open_lot, open_opening), Err(DrawRefused::Size));
        let empty = DrawWire { round: ROUND, lots: vec![], openings: vec![] };
        assert_eq!(checked(&empty), Err(DrawRefused::Size));
    }

    /// The wire shape is pinned: a round, then lots and openings as arrays of
    /// byte strings.
    #[test]
    fn the_draw_encodes_as_byte_strings() {
        let draw = DrawWire { round: 3, lots: vec![vec![1u8, 2, 3]], openings: vec![] };
        let bytes = minicbor::to_vec(&draw).unwrap();
        assert_eq!(bytes, vec![0x83, 0x03, 0x81, 0x43, 1, 2, 3, 0x80], "array(3), 3, array(1) of bytes(3), array(0)");
        let back: DrawWire = minicbor::decode(&bytes).unwrap();
        assert_eq!(back, draw);
        assert_ne!(draw_digest(&draw), draw_digest(&DrawWire { round: 4, ..draw.clone() }));
    }

    #[test]
    fn a_lot_prints_no_secret() {
        assert_eq!(format!("{:?}", lot(1)), "Lot(..)");
    }

    /// The whole road with real words: three members sign their sealed lots,
    /// a roster carries them with their signed openings, and the draw checks
    /// and decides. A lot signed by one member and claimed for another is
    /// refused by its signature, not by trust.
    #[test]
    fn a_draw_of_signed_words_checks_and_a_borrowed_lot_does_not() {
        use crate::net::tabletalk::{lot_word, open_lot, open_opening, opening_word};
        use ed25519_dalek::SigningKey;
        const NOW: u64 = 1_700_000_000_000;
        let keys: Vec<SigningKey> = (1..=3u8).map(|n| SigningKey::from_bytes(&[n; 32])).collect();
        let pks: Vec<[u8; 32]> = keys.iter().map(|k| k.verifying_key().to_bytes()).collect();
        let digest = members_digest(&TABLE, &pks);
        let mut order: Vec<usize> = (0..3).collect();
        order.sort_by_key(|&i| pks[i]);
        let lots: Vec<Lot> = (0..3u8).map(lot).collect();
        let lot_words: Vec<Vec<u8>> = order
            .iter()
            .map(|&i| {
                let c = lots[i].commitment(&TABLE, ROUND, &digest, &pks[i]);
                lot_word(&keys[i], &TABLE, ROUND, &digest, &c, NOW).unwrap()
            })
            .collect();
        let d = lots_digest(&lot_words);
        let draw = DrawWire {
            round: ROUND,
            lots: lot_words.clone(),
            openings: order.iter().map(|&i| opening_word(&keys[i], &TABLE, ROUND, &d, &lots[i], NOW).unwrap()).collect(),
        };
        let lot_of = |b: &[u8]| open_lot(b, &TABLE).map_err(|_| "not a lot of this table");
        let opening_of = |b: &[u8]| open_opening(b, &TABLE).map_err(|_| "not an opening of this table");
        let c = check(&TABLE, &draw, 10, lot_of, opening_of).expect("signed words");
        let drawn = c.drawn.expect("opened");
        let rs: Vec<([u8; 32], [u8; 32])> = (0..3).map(|i| (pks[i], lots[i].r)).collect();
        assert_eq!(drawn.seed, seed(&TABLE, ROUND, &digest, &d, &rs), "the seed is every member's r");

        // A member's lot re-signed by another key is that key's lot, and the
        // draw then has one member twice and another not at all.
        let mut borrowed = draw.clone();
        let c1 = lots[order[1]].commitment(&TABLE, ROUND, &digest, &pks[order[1]]);
        borrowed.lots[1] = lot_word(&keys[order[0]], &TABLE, ROUND, &digest, &c1, NOW).unwrap();
        assert!(check(&TABLE, &borrowed, 10, lot_of, opening_of).is_err());
    }
}
