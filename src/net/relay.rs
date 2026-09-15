//! Playing when everybody is behind NAT (D-004).
//!
//! # The shape of the problem, measured rather than assumed
//!
//! `NAT_AND_DISCOVERY.md` measured this network as a **port-restricted cone**:
//! endpoint-independent mapping with the external port equal to the internal
//! one, and address-and-port-dependent filtering. 793 DHT nodes were contacted,
//! 1 361 replies came back, and **zero** packets ever arrived from a stranger.
//!
//! Two consequences, and they pull in opposite directions:
//!
//! * **Seeing the lobby works; appearing in it needs a relay.** Reading the
//!   public DHT is outbound, so the router holds the mapping and the reply comes
//!   back down it. But a provider record carries the swarm's confirmed external
//!   addresses, and a client behind a NAT has none until a relay hands it a
//!   circuit address. Until `56b0b50` this bullet said *discovery works* with no
//!   help at all — true of the Mainline announce it described, which published
//!   an `IP:port` whether or not anything answered there.
//! * **A peer can never be reached cold.** Unsolicited inbound is impossible, so
//!   two clients that have found each other still cannot connect. Hole punching
//!   works *only* because both sides transmit outward first — which is what
//!   DCUtR's synchronised open does, and **DCUtR requires an existing relayed
//!   connection to coordinate through.** It is not a fallback for the relay; it
//!   is built on top of it.
//!
//! So when everybody is behind NAT there must still be a relay, and it cannot
//! come from inside the game — which is why D-002 has publicly reachable clients
//! volunteer, and why they advertise under `/libp2p/relay`, the namespace every
//! libp2p client already looks in, rather than under a swarm of their own.
//!
//! # The default relay limits carry less than one hand
//!
//! `relay::Config::default()` is 120 seconds and 128 KiB per circuit. Those are
//! deliberately hostile to using a relay as a transport: upstream sized them to
//! carry a DCUtR handshake and nothing else.
//!
//! [`per_hand_bytes`] computes what one hand actually costs from the wire sizes
//! this project measured, and at a full table it is **over the whole default
//! budget**. A circuit that dies mid-street is not a performance problem — it is
//! an abort this client engineered against itself, and `SPEC_CS.md` §19 makes a
//! mid-hand disconnection a security question rather than an annoyance.
//!
//! The rule that follows is [`Adequacy`]: read the `Limit` the relay returns,
//! compare it against a real hand, and **refuse to seat the player** rather than
//! starting one that will drop. Do not assume; the protocol reports the server's
//! real limits back, so there is nothing to guess.
//!
//! # The case this cannot fix, said plainly
//!
//! **If nobody anywhere is publicly reachable, there is no game.** Not a slow
//! game or a degraded one — none. Every peer needs a relay to be reached
//! through, DCUtR needs a relayed connection to upgrade, and neither exists in a
//! network where every participant is behind a NAT that admits nothing.
//!
//! D-004 asks that clients play when *they* are all behind NAT, and that is
//! satisfiable: it needs one reachable host somewhere, and it does not have to
//! be a player. What it cannot survive is a network with no reachable host at
//! all. A client in that position must **say so**, because the alternative is a
//! lobby that lists tables nobody can sit at and a user who concludes the
//! software is broken.

use std::time::Duration;

// The wire sizes, **taken from the module that defines them** rather than
// copied. The first version declared its own literals under a comment claiming
// they "move when the cryptography does" — they did not, because they were a
// second copy, and a second copy of a number is a number that drifts. The
// difference is not academic: this file decides whether a relay can carry a
// hand, so a stale figure here is a table that drops mid-street.
use crate::mental_poker::backend::{
    DECK_BYTES, KEY_PROOF_BYTES, POINT as POINT_USIZE, SHUFFLE_PROOF_BYTES, TOKEN_PROOF_BYTES,
};

const POINT: u64 = POINT_USIZE as u64;
const DECK: u64 = DECK_BYTES as u64;
const SHUFFLE_PROOF: u64 = SHUFFLE_PROOF_BYTES as u64;
const DECK_KEY: u64 = POINT;
const KEY_PROOF: u64 = KEY_PROOF_BYTES as u64;
const TOKEN: u64 = POINT;
const TOKEN_PROOF: u64 = TOKEN_PROOF_BYTES as u64;

/// What a signed envelope costs on top of a payload.
///
/// The signature is 64 bytes and the envelope carries two 32-byte hashes, three
/// integers and the sender's key. Rounded up rather than measured to the byte,
/// because it is an overhead estimate and a generous one fails safe: it makes
/// this client refuse a relay that would in fact have been adequate, which costs
/// a player a table and never costs them a hand.
const ENVELOPE: u64 = 220;

/// What one hand of `seats` players costs on the table mesh, in bytes, as seen
/// by one peer.
///
/// Every term comes from `mental_poker::backend`, which is where the wire sizes
/// are declared — so this moves when the cryptography does instead of staying
/// true by luck. It said so before it was true: the terms were a hand copy.
///
/// * `seats` deck keys with ownership proofs — `DECK_INIT`.
/// * `seats` links of the shuffle chain, each a whole deck and its argument.
/// * `2·seats + 5` cards, each needing one decryption share per seat.
///
/// The count is what **one** peer sees: it receives every other seat's
/// contribution and sends its own, and a relayed circuit carries both.
pub const fn per_hand_bytes(seats: u8) -> u64 {
    let n = seats as u64;
    let cards = 2 * n + 5;

    let deck_init = n * (DECK_KEY + KEY_PROOF + ENVELOPE);
    let chain = n * (DECK + SHUFFLE_PROOF + ENVELOPE);
    let reveal = cards * n * (TOKEN + TOKEN_PROOF + ENVELOPE);

    deck_init + chain + reveal
}

/// Whether a relay can carry a hand at this table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adequacy {
    /// The circuit outlasts and outsizes a hand with room to spare.
    Adequate,
    /// The circuit would die during a hand.
    ///
    /// The honest response is to **refuse to seat the player**, with a message
    /// saying why. Starting a hand that will drop mid-street is an abort this
    /// client arranged for itself.
    TooSmall { need_bytes: u64, offered_bytes: u64 },
    /// The circuit would time out during a hand.
    TooShort {
        need: Duration,
        offered: Duration,
    },
    /// The relay declared no limit at all.
    ///
    /// Taken as adequate, because an absent limit is the relay saying it imposes
    /// none — but recorded as its own case, since "no limit stated" and "a limit
    /// large enough" are different claims and only one of them is checkable.
    Unlimited,
}

/// How much headroom a circuit needs beyond one hand.
///
/// A hand is not the unit a circuit has to survive — a table plays many, and
/// re-establishing a circuit between hands is fine while doing it *during* one is
/// not. Three hands of slack means a circuit that is renewed at any point in the
/// gap still carries the hand in progress.
pub const HAND_HEADROOM: u64 = 3;

/// Whether a relay's returned limits can carry play at this table.
///
/// `offered_bytes` and `offered_duration` are what the relay reported in its
/// reservation — **read, not assumed**. The protocol hands the server's real
/// limits back to the client precisely so this decision can be made before
/// committing.
pub fn adequate(
    seats: u8,
    hand_deadline: Duration,
    offered_bytes: Option<u64>,
    offered_duration: Option<Duration>,
) -> Adequacy {
    let need_bytes = per_hand_bytes(seats) * HAND_HEADROOM;

    match offered_bytes {
        Some(offered) if offered < need_bytes => {
            return Adequacy::TooSmall {
                need_bytes,
                offered_bytes: offered,
            }
        }
        _ => {}
    }

    match offered_duration {
        Some(offered) if offered < hand_deadline => {
            return Adequacy::TooShort {
                need: hand_deadline,
                offered,
            }
        }
        _ => {}
    }

    if offered_bytes.is_none() && offered_duration.is_none() {
        return Adequacy::Unlimited;
    }
    Adequacy::Adequate
}

/// The library's defaults, named so a test can compare against them.
///
/// From `libp2p-relay`'s `Default for Config`: 120 seconds and 128 KiB per
/// circuit.
pub const LIBRARY_DEFAULT_CIRCUIT_BYTES: u64 = 128 * 1024;
pub const LIBRARY_DEFAULT_CIRCUIT_DURATION: Duration = Duration::from_secs(120);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::constants::MAX_SEATS;

    /// The number this module exists for.
    ///
    /// One hand at a full table costs more than the library's **entire** default
    /// circuit budget. A client that took the defaults would drop tables
    /// mid-street and the failure would look like a peer disconnecting.
    #[test]
    fn one_hand_at_a_full_table_exceeds_the_default_relay_budget() {
        let ten = per_hand_bytes(MAX_SEATS);
        println!("per hand at {MAX_SEATS} seats: {ten} bytes");
        println!("library default circuit:      {LIBRARY_DEFAULT_CIRCUIT_BYTES} bytes");

        assert!(
            ten > LIBRARY_DEFAULT_CIRCUIT_BYTES,
            "if this ever stops being true the warning in this module can go, \
             but it must be re-measured and not assumed"
        );
    }

    /// And heads-up is under it, which is the case that makes the defaults look
    /// fine to anyone who tests with two players.
    #[test]
    fn heads_up_fits_and_that_is_the_trap() {
        let two = per_hand_bytes(2);
        println!("per hand heads-up: {two} bytes");
        assert!(
            two < LIBRARY_DEFAULT_CIRCUIT_BYTES,
            "heads-up fits, which is exactly why a two-player test proves nothing \
             about a six-player table"
        );
    }

    /// The sizes are the deck module's, not this one's. A copy here would drift,
    /// and the comment on `per_hand_bytes` claimed otherwise before it was true.
    #[test]
    fn the_wire_sizes_are_the_deck_modules() {
        use crate::mental_poker::backend;
        assert_eq!(POINT as usize, backend::POINT);
        assert_eq!(DECK as usize, backend::DECK_BYTES);
        assert_eq!(SHUFFLE_PROOF as usize, backend::SHUFFLE_PROOF_BYTES);
        assert_eq!(KEY_PROOF as usize, backend::KEY_PROOF_BYTES);
        assert_eq!(TOKEN_PROOF as usize, backend::TOKEN_PROOF_BYTES);
    }

    /// The cost is dominated by the shuffle chain, which is `seats` whole decks
    /// and `seats` arguments. That is worth knowing before anybody tries to
    /// economise on the reveal path.
    #[test]
    fn the_chain_is_what_costs() {
        for seats in [2u8, 6, 10] {
            let total = per_hand_bytes(seats);
            let chain = seats as u64 * (DECK + SHUFFLE_PROOF + ENVELOPE);
            println!(
                "{seats} seats: {total} bytes, chain {chain} ({:.0}%)",
                100.0 * chain as f64 / total as f64
            );
            assert!(chain * 2 > total, "the chain is over half of it");
        }
    }

    /// It grows faster than the table does: the reveal term is quadratic in the
    /// seat count, because every seat contributes a share to every card and the
    /// card count itself grows with the seats.
    #[test]
    fn the_cost_grows_faster_than_the_table() {
        let two = per_hand_bytes(2);
        let ten = per_hand_bytes(10);
        assert!(
            ten > 5 * two,
            "five times the seats is more than five times the bytes: {two} to {ten}"
        );
    }

    /// The decision the research asked for by name: read the limit, compare it
    /// against a real hand, and refuse rather than assume.
    #[test]
    fn a_relay_that_cannot_carry_a_hand_is_refused() {
        let deadline = Duration::from_secs(3_300);

        assert_eq!(
            adequate(
                MAX_SEATS,
                deadline,
                Some(LIBRARY_DEFAULT_CIRCUIT_BYTES),
                Some(LIBRARY_DEFAULT_CIRCUIT_DURATION)
            ),
            Adequacy::TooSmall {
                need_bytes: per_hand_bytes(MAX_SEATS) * HAND_HEADROOM,
                offered_bytes: LIBRARY_DEFAULT_CIRCUIT_BYTES
            },
            "the library's own defaults are refused, which is the finding"
        );
    }

    /// A relay big enough but short enough still fails, and says which.
    #[test]
    fn a_relay_that_would_time_out_mid_hand_is_refused() {
        let deadline = Duration::from_secs(600);
        let plenty = per_hand_bytes(6) * 100;

        assert_eq!(
            adequate(6, deadline, Some(plenty), Some(Duration::from_secs(120))),
            Adequacy::TooShort {
                need: deadline,
                offered: Duration::from_secs(120)
            }
        );
        assert_eq!(
            adequate(6, deadline, Some(plenty), Some(Duration::from_secs(601))),
            Adequacy::Adequate
        );
    }

    /// A relay that states no limits is taken at its word, and the case is named
    /// rather than folded into "adequate": "no limit stated" and "a limit large
    /// enough" are different claims, and only the second is checkable.
    #[test]
    fn an_unlimited_relay_is_its_own_answer() {
        assert_eq!(
            adequate(6, Duration::from_secs(600), None, None),
            Adequacy::Unlimited
        );
    }

    /// The headroom is hands, not seconds: a table plays many, and renewing a
    /// circuit between hands is fine while renewing it during one is not.
    #[test]
    fn the_budget_asks_for_more_than_one_hand() {
        let one = per_hand_bytes(6);
        let deadline = Duration::from_secs(600);

        assert_eq!(
            adequate(6, deadline, Some(one), Some(Duration::from_secs(3_600))),
            Adequacy::TooSmall {
                need_bytes: one * HAND_HEADROOM,
                offered_bytes: one
            },
            "exactly one hand of budget is not enough to start a table"
        );
        assert_eq!(
            adequate(
                6,
                deadline,
                Some(one * HAND_HEADROOM),
                Some(Duration::from_secs(3_600))
            ),
            Adequacy::Adequate
        );
    }

    /// The envelope overhead is generous on purpose: over-estimating makes this
    /// client refuse a relay that would have worked, which costs a player a
    /// table. Under-estimating drops them mid-hand, which is the thing
    /// `SPEC_CS.md` §19 calls a security problem.
    #[test]
    fn the_overhead_estimate_fails_in_the_safe_direction() {
        let with = per_hand_bytes(6);
        let signature_and_two_hashes = 64 + 32 + 32;
        assert!(
            ENVELOPE > signature_and_two_hashes,
            "the estimate must exceed the parts it is standing in for"
        );
        assert!(with > 0);
    }
}
