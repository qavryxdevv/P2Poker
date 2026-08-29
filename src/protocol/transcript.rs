//! The hash chain (`PROTOCOL.md` §3.1, §3.2).
//!
//! A table is a sequence of chains: a setup chain, then one per hand. Each
//! opens at a genesis derived from the previous chain's terminal value, so
//! every event of hand `k` chains transitively to `GENESIS(k)` and a component
//! two honest receivers compute differently does not cost them one field — it
//! costs them **every event of the hand**, because neither will verify a single
//! one of the other's. That failure is total and silent until the first event
//! arrives, which is why D-012 forbids deriving any of this from a per-receiver
//! quantity.
//!
//! Everything here goes through [`crate::protocol::serialization::h`], so every
//! part is length-prefixed and every hash is domain-separated.

use crate::poker::state::{Hash, PlayerId, SeatIdx};
use crate::protocol::constants::PROTOCOL_VERSION;
use crate::protocol::messages::ZERO32;
use crate::protocol::signatures::{hash, Domain};

/// One seat's contribution to the roster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RosterSeat {
    pub seat: SeatIdx,
    pub app_public_key: PlayerId,
    /// The stack this seat holds when the hand starts.
    pub stack_at_hand_start: u64,
}

/// One seat's contribution to a collective stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageEmitter {
    pub seat: SeatIdx,
    pub event_hash: Hash,
}

/// The hash of one event.
///
/// **The signature is deliberately excluded.** Ed25519 is deterministic per RFC
/// 8032, but a malicious signer can pick a different nonce and produce a second
/// valid signature over the same body. Hashing only the body means that cannot
/// fork the chain: two signatures over one body are **one event**.
pub fn event_hash(body_bytes: &[u8]) -> Hash {
    hash(Domain::Transcript, &[body_bytes])
}

/// The stage hash of a single-writer stage.
pub fn stage_hash_single(sequence: u64, stage_type: u16, writer_seat: SeatIdx, event: Hash) -> Hash {
    hash(
        Domain::Stage,
        &[
            &sequence.to_be_bytes(),
            &stage_type.to_be_bytes(),
            &[writer_seat],
            &event,
        ],
    )
}

/// The stage hash of a collective stage.
///
/// `emitters` must be the required set in ascending seat order; each seat
/// contributes one part, its index concatenated with its event hash. The caller
/// sorts, because the required set is a fixed function of state and sorting it
/// here would hide a caller that had the wrong set.
pub fn stage_hash_collective(sequence: u64, stage_type: u16, emitters: &[StageEmitter]) -> Hash {
    debug_assert!(
        emitters.windows(2).all(|w| w[0].seat < w[1].seat),
        "the required set must be in ascending seat order, without repeats"
    );

    let mut parts: Vec<Vec<u8>> = Vec::with_capacity(emitters.len() + 2);
    parts.push(sequence.to_be_bytes().to_vec());
    parts.push(stage_type.to_be_bytes().to_vec());
    for e in emitters {
        let mut part = Vec::with_capacity(1 + 32);
        part.push(e.seat);
        part.extend_from_slice(&e.event_hash);
        parts.push(part);
    }
    let refs: Vec<&[u8]> = parts.iter().map(|p| p.as_slice()).collect();
    hash(Domain::Stage, &refs)
}

/// The roster of a hand: the seated identities and their start-of-hand stacks,
/// **and nothing else**.
///
/// A `seat_flags` component once stood here. It appeared exactly once in the
/// whole corpus, was never defined, and fed the genesis — so two implementers
/// had to guess it and two guesses share no verifying event at all. It is
/// deleted rather than defined, because anything mutable is either already in
/// the chain, in which case hashing it again buys nothing, or it is a local
/// view, in which case hashing it forks the genesis (D-012).
pub fn roster_hash(seats: &[RosterSeat]) -> Hash {
    // A `debug_assert` stood here, and a roster arrives from the network: in a
    // release build the check was absent on exactly the path that needs it.
    // `table::formation::Roster` refuses an unsorted roster before it can reach
    // this function, so this is the second gate rather than the only one — but
    // an order-dependent hash whose order is unchecked is two peers hashing two
    // values from the same members, and that must not be reachable at all.
    assert!(
        seats.windows(2).all(|w| w[0].seat < w[1].seat),
        "the roster must be in ascending seat order, without repeats"
    );

    let parts: Vec<Vec<u8>> = seats
        .iter()
        .map(|s| {
            let mut part = Vec::with_capacity(1 + 32 + 8);
            part.push(s.seat);
            part.extend_from_slice(&s.app_public_key);
            part.extend_from_slice(&s.stack_at_hand_start.to_be_bytes());
            part
        })
        .collect();
    let refs: Vec<&[u8]> = parts.iter().map(|p| p.as_slice()).collect();
    hash(Domain::Roster, &refs)
}

/// The genesis of the setup chain.
///
/// Slot 4 is the zero sentinel: there is no session yet, since the session id
/// is derived from the `TABLE_READY` events of the very chain this opens. Slot
/// 6 is zero for the same shape of reason — there is no previous terminal.
///
/// Both genesis forms keep six parts under one domain string on purpose. `h`
/// carries no arity, so two part-lists of different lengths under one domain
/// would be two preimages a future editor has to reason about.
pub fn genesis_setup(table_id: &Hash, table_params_hash: &Hash) -> Hash {
    hash(
        Domain::Genesis,
        &[
            &PROTOCOL_VERSION.to_be_bytes(),
            table_id,
            &0u64.to_be_bytes(),
            &ZERO32,
            table_params_hash,
            &ZERO32,
        ],
    )
}

/// The genesis of hand `k`, for `k >= 1`.
pub fn genesis_hand(
    table_id: &Hash,
    hand_id: u64,
    session_id: &Hash,
    roster: &Hash,
    previous_terminal: &Hash,
) -> Hash {
    debug_assert!(hand_id >= 1, "hand 0 is the setup chain; use genesis_setup");
    hash(
        Domain::Genesis,
        &[
            &PROTOCOL_VERSION.to_be_bytes(),
            table_id,
            &hand_id.to_be_bytes(),
            session_id,
            roster,
            previous_terminal,
        ],
    )
}

/// The terminal value of an **aborted** hand.
///
/// A function of the hand's own genesis and of nothing else, which is the whole
/// point: `GENESIS(k+1)` depends on `TERMINAL(k)`, so if two peers derived
/// different terminals for an aborted hand they could never speak again. Being
/// witness-independent means peers that saw *different prefixes* of the aborted
/// hand still agree on where the next one starts.
pub fn abort_terminal(table_id: &Hash, hand_id: u64, genesis: &Hash) -> Hash {
    hash(
        Domain::AbortTerminal,
        &[
            &PROTOCOL_VERSION.to_be_bytes(),
            table_id,
            &hand_id.to_be_bytes(),
            genesis,
        ],
    )
}

/// The session identifier, derived from the roster's ratification.
///
/// Every later `GENESIS(k)` carries it, so every hand event is bound to this
/// exact ratification.
///
/// # Where the uniqueness actually comes from
///
/// From `table_id`, which is the table's public key and is fresh for every
/// table. **Not** from any nonce, and this is worth stating because both this
/// comment and `PROTOCOL.md` §4.3 previously said otherwise: *"the `HELLO`
/// nonces feed the connections and the `join_nonce`s feed the join requests
/// whose hashes are in the roster chain"*.
///
/// They do not. The four inputs are `table_id`, `table_params_hash`,
/// `roster_hash(0)` — seat index, application key and starting stack, per §3.1 —
/// and the `TABLE_READY` bodies, which carry a roster hash, a serial, a
/// parameters hash, a seat and a capability set. No nonce is in any of them.
/// `join_nonce` reaches `JOIN_REQUEST` alone, which is unchained and enters
/// nothing downstream.
///
/// The claim was wrong in the dangerous direction: an editor who believed the
/// nonces carried the uniqueness could weaken the freshness of `table_id` on the
/// grounds that something else covered it, and nothing does.
pub fn session_id(
    table_id: &Hash,
    table_params_hash: &Hash,
    roster_zero: &Hash,
    table_ready_hashes: &[Hash],
) -> Hash {
    let mut parts: Vec<&[u8]> = Vec::with_capacity(3 + table_ready_hashes.len());
    parts.push(table_id);
    parts.push(table_params_hash);
    parts.push(roster_zero);
    for h in table_ready_hashes {
        parts.push(h);
    }
    hash(Domain::Session, &parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: Hash = [0x11; 32];
    const PARAMS: Hash = [0x22; 32];

    fn roster(stacks: &[(SeatIdx, u64)]) -> Vec<RosterSeat> {
        stacks
            .iter()
            .map(|&(seat, stack)| RosterSeat {
                seat,
                app_public_key: [seat.wrapping_add(0x40); 32],
                stack_at_hand_start: stack,
            })
            .collect()
    }

    /// Two signatures over one body are one event, so the signature is outside
    /// the hash. Otherwise a malicious signer picking a second nonce could fork
    /// the chain without changing anything anyone agreed to.
    #[test]
    fn the_event_hash_covers_the_body_and_nothing_else() {
        let a = event_hash(b"the very same body");
        let b = event_hash(b"the very same body");
        assert_eq!(a, b);
        assert_ne!(a, event_hash(b"a different body"));
    }

    #[test]
    fn a_single_writer_stage_depends_on_all_four_parts() {
        let base = stage_hash_single(5, 0x0305, 2, [7u8; 32]);
        assert_ne!(base, stage_hash_single(6, 0x0305, 2, [7u8; 32]));
        assert_ne!(base, stage_hash_single(5, 0x0306, 2, [7u8; 32]));
        assert_ne!(base, stage_hash_single(5, 0x0305, 3, [7u8; 32]));
        assert_ne!(base, stage_hash_single(5, 0x0305, 2, [8u8; 32]));
    }

    #[test]
    fn a_collective_stage_depends_on_who_emitted_and_on_what() {
        let two = [
            StageEmitter { seat: 0, event_hash: [1u8; 32] },
            StageEmitter { seat: 2, event_hash: [2u8; 32] },
        ];
        let base = stage_hash_collective(4, 0x0303, &two);

        // A different seat in the set.
        let other_seats = [
            StageEmitter { seat: 0, event_hash: [1u8; 32] },
            StageEmitter { seat: 3, event_hash: [2u8; 32] },
        ];
        assert_ne!(base, stage_hash_collective(4, 0x0303, &other_seats));

        // The same seats, one different event.
        let other_event = [
            StageEmitter { seat: 0, event_hash: [1u8; 32] },
            StageEmitter { seat: 2, event_hash: [9u8; 32] },
        ];
        assert_ne!(base, stage_hash_collective(4, 0x0303, &other_event));

        // A third emitter.
        let three = [
            StageEmitter { seat: 0, event_hash: [1u8; 32] },
            StageEmitter { seat: 2, event_hash: [2u8; 32] },
            StageEmitter { seat: 4, event_hash: [3u8; 32] },
        ];
        assert_ne!(base, stage_hash_collective(4, 0x0303, &three));
    }

    /// Length prefixing does the work here: a seat index and its event hash are
    /// one part, so two seats cannot be re-cut into a different pair with the
    /// same bytes.
    #[test]
    fn the_emitters_cannot_be_recut() {
        let a = [
            StageEmitter { seat: 1, event_hash: [0u8; 32] },
            StageEmitter { seat: 2, event_hash: [0u8; 32] },
        ];
        let b = [StageEmitter { seat: 1, event_hash: [0u8; 32] }];
        assert_ne!(
            stage_hash_collective(1, 1, &a),
            stage_hash_collective(1, 1, &b)
        );
    }

    #[test]
    fn the_roster_depends_on_every_seat_its_key_and_its_stack() {
        let base = roster_hash(&roster(&[(0, 10_000), (1, 10_000)]));

        assert_ne!(base, roster_hash(&roster(&[(0, 10_000), (1, 9_999)])), "a stack");
        assert_ne!(base, roster_hash(&roster(&[(0, 10_000), (2, 10_000)])), "a seat");
        assert_ne!(
            base,
            roster_hash(&roster(&[(0, 10_000), (1, 10_000), (2, 10_000)])),
            "an extra seat"
        );

        let mut different_key = roster(&[(0, 10_000), (1, 10_000)]);
        different_key[1].app_public_key = [0xFF; 32];
        assert_ne!(base, roster_hash(&different_key), "an identity");
    }

    #[test]
    fn the_two_genesis_forms_differ_and_each_depends_on_its_inputs() {
        let setup = genesis_setup(&TABLE, &PARAMS);
        let session = [0x33; 32];
        let roster = [0x44; 32];
        let terminal = [0x55; 32];
        let hand = genesis_hand(&TABLE, 1, &session, &roster, &terminal);
        assert_ne!(setup, hand);

        assert_ne!(setup, genesis_setup(&TABLE, &[0x99; 32]), "the parameters");
        assert_ne!(setup, genesis_setup(&[0x99; 32], &PARAMS), "the table");

        assert_ne!(hand, genesis_hand(&TABLE, 2, &session, &roster, &terminal));
        assert_ne!(hand, genesis_hand(&TABLE, 1, &[0x99; 32], &roster, &terminal));
        assert_ne!(hand, genesis_hand(&TABLE, 1, &session, &[0x99; 32], &terminal));
        assert_ne!(hand, genesis_hand(&TABLE, 1, &session, &roster, &[0x99; 32]));
    }

    /// The property the whole abort design rests on: two peers that saw
    /// **different prefixes** of an aborted hand still agree where the next one
    /// starts. If they did not, they could never speak again, because
    /// `GENESIS(k+1)` depends on `TERMINAL(k)`.
    #[test]
    fn an_aborted_hands_terminal_is_witness_independent() {
        let genesis = genesis_hand(&TABLE, 4, &[0x33; 32], &[0x44; 32], &[0x55; 32]);

        // Two peers, having accepted entirely different events of hand 4,
        // compute the terminal from the genesis alone.
        let peer_a = abort_terminal(&TABLE, 4, &genesis);
        let peer_b = abort_terminal(&TABLE, 4, &genesis);
        assert_eq!(peer_a, peer_b);

        // It is still bound to the hand and the table.
        assert_ne!(peer_a, abort_terminal(&TABLE, 5, &genesis));
        assert_ne!(peer_a, abort_terminal(&[0x99; 32], 4, &genesis));
        assert_ne!(peer_a, abort_terminal(&TABLE, 4, &[0x99; 32]));
    }

    #[test]
    fn the_session_id_binds_the_roster_and_every_ratification() {
        let roster0 = roster_hash(&roster(&[(0, 10_000), (1, 10_000)]));
        let ready = [[0xA1; 32], [0xA2; 32]];
        let base = session_id(&TABLE, &PARAMS, &roster0, &ready);

        assert_eq!(base, session_id(&TABLE, &PARAMS, &roster0, &ready));
        assert_ne!(base, session_id(&TABLE, &PARAMS, &roster0, &[[0xA1; 32], [0xFF; 32]]));
        assert_ne!(base, session_id(&TABLE, &PARAMS, &[0x99; 32], &ready));
        assert_ne!(base, session_id(&TABLE, &[0x99; 32], &roster0, &ready));

        // Two tables with the same participants get different sessions, because
        // the ratification events carry the handshake and join nonces.
        let other_ready = [[0xB1; 32], [0xB2; 32]];
        assert_ne!(base, session_id(&TABLE, &PARAMS, &roster0, &other_ready));
    }

    /// Every construction in this module is domain-separated, so the same parts
    /// under two constructions never collide.
    #[test]
    fn the_constructions_do_not_collide_with_each_other() {
        let same: Hash = [0x77; 32];
        let a = event_hash(&same);
        let b = roster_hash(&[RosterSeat {
            seat: 0,
            app_public_key: same,
            stack_at_hand_start: 0,
        }]);
        let c = abort_terminal(&same, 0, &same);
        let d = genesis_setup(&same, &same);
        let all = [a, b, c, d];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j], "constructions {i} and {j} collided");
            }
        }
    }
}
