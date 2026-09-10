//! Protocol constants (`PROTOCOL.md` §13).
//!
//! Wire-visible values and the bounds a receiver enforces. Everything here is
//! transcribed from §13, which owns them (D-011); a value that appears in a
//! second document is a defect, and has twice been one.
//!
//! # Why the deadline bounds are functions and not numbers
//!
//! `hand_deadline_ms` was a single figure, `600_000`, and it was below its own
//! floor at **every** seat count. Legal play at six seats and up aborted
//! itself, with `cause = 1` and nobody at fault.
//!
//! What makes it worth a note here is that **no timing test in this project
//! could have caught it**. A protocol-paced table finishes a hand in a couple
//! of seconds at ten seats, hundreds of times inside 600 000 ms, so every
//! healthy-table walk passed. What the deadline has to pay for is the *human*
//! action term — forty actions at twenty-five seconds — which the walk never
//! measures. So the bound is derived here rather than written down, and a bare
//! literal deadline is a defect on sight.

use crate::poker::state::Chips;

// ---------------------------------------------------------------------------
// Identity and protocol strings
// ---------------------------------------------------------------------------

pub const PROTOCOL_VERSION: u16 = 1;
pub const PROTOCOL_MAJOR: u16 = 1;

/// The libp2p identify protocol name.
///
/// Held here and used from `net::swarm`, which defined its own copy of this
/// string under the name `PROTOCOL_VERSION` — a third meaning for a name that
/// already had two.
pub const IDENTIFY_PROTOCOL: &str = "/p2p-poker/1";
pub const LOBBY_TOPIC: &str = "/p2p-poker/lobby/1";
pub const LOBBY_CHAT_TOPIC: &str = "/p2p-poker/lobby-chat/1";
pub const SNAPSHOT_PROTOCOL: &str = "/p2p-poker/lobby-snapshot/1";
pub const JOIN_PROTOCOL: &str = "/p2p-poker/join/1";
pub const TABLE_PROTOCOL: &str = "/p2p-poker/table/1";

// **The two Mainline infohashes and their derivation strings are gone.**
// `c7e6317` took BitTorrent out of the binary and the lobby became a libp2p
// Kademlia provider record; `net::run::lobby_namespace` and `relay_namespace`
// are where a client announces now. The constants outlived the crate by some
// months, guarded by two compile-time assertions and a test that compared them
// with each other - all three passed, none of them reached anything, and a
// reader had four public constants saying the lobby was somewhere it was not.

// ---------------------------------------------------------------------------
// Table and chain shape
// ---------------------------------------------------------------------------

pub const MAX_SEATS: u8 = 10;
pub const MAX_STAGES_PER_HAND: u64 = 2_048;

/// A boundary event of chain `k` carries `sequence = BOUNDARY_SEQUENCE_BASE + seat`.
pub const BOUNDARY_SEQUENCE_BASE: u64 = 4_096;

/// The boundary checkpoint's own sequence base (§4.9).
pub const BOUNDARY_CHECKPOINT_BASE: u64 = 8_192;
/// The return band (`S1-BM`): a `RETURN_VOTE` and a `RETURN_CERT` about seat
/// `s` are sealed at `RETURN_SEQUENCE_BASE + s`, parented on `TERMINAL(k)`.
/// Above the last reconciliation ack (8 207) with room to spare, so the
/// three boundary bands -- the seat window, the checkpoint rounds and this
/// -- never meet; `returnwire::tests` holds that.
pub const RETURN_SEQUENCE_BASE: u64 = 8_224;
/// How long a client holds the next deal at a boundary while a return is in
/// flight (`S1-BM`, D-028): a seat outside the roster asked to sit in -- this
/// client, or one whose request the window took -- and no certificate about
/// it has banked here yet. A client liveness parameter, not a wire rule: past
/// it the table deals on and the seat asks again at the next boundary.
/// Measured `run164337-3`: without it the request went out in the very tick
/// the voters dealt hand k+1, and a heads-up hand leaves stage 0 in a tenth
/// of a second, so the late-roster repair had nothing left to re-open.
pub const RETURN_GRACE_MS: u64 = 6_000;

/// How many reconciliation rounds the checkpoint band has room for.
///
/// `PROTOCOL.md` §4.9: round `r` takes `BOUNDARY_CHECKPOINT_BASE + 2r` for its
/// `STATE_HASH` and `+ 2r + 1` for its `STATE_ACK`, with **`1 <= r <= 7`** — so
/// the band is the sixteen values `8 192 … 8 207` and a checkpoint-8 event
/// outside it is a stage violation at §4.0 step 12.
pub const MAX_RECONCILIATION_ROUNDS: u16 = 7;

/// How many past hands a receiver keeps a record of, for the stale-event test.
pub const MAX_RETAINED_HAND_RECORDS: usize = 4_096;

pub const MAX_CBOR_NESTING_DEPTH: usize = 8;
pub const MAX_DISPUTES_PER_SENDER_PER_HAND: usize = 8;
/// How many certificates against one seat make it sit out.
///
/// `PROTOCOL.md` §8.3: after three, the seat is marked sitting out at the next
/// hand boundary — it keeps its stack, posts dead money, takes no cards and
/// drains. That is the tournament's dead seat, and it exists only where
/// `|V| >= 2`, because below the floor no certificate has any effect and the
/// counter never increments.
pub const MAX_CONSECUTIVE_AUTO_ACTIONS: u8 = 3;

// ---------------------------------------------------------------------------
// Size caps (`SPEC_CS.md` §17, §27: every message and every collection bounded)
// ---------------------------------------------------------------------------

pub const GOSSIP_MAX_TRANSMIT: usize = 65_536;
pub const LOBBY_MSG_MAX: usize = 8_192;
pub const TABLE_AD_MAX: usize = 1_024;
pub const TABLE_AD_SIGNED_MAX: usize = 1_536;
pub const LOBBY_CHAT_MAX: usize = 2_048;
pub const SNAPSHOT_REQ_MAX: usize = 1_024;
pub const SNAPSHOT_RESP_MAX: usize = 262_144;
pub const SNAPSHOT_MAX_ADS: usize = 128;
pub const JOIN_REQ_MAX: usize = 4_096;
pub const JOIN_RESP_MAX: usize = 16_384;

// ---------------------------------------------------------------------------
// §9.3's per-message payload caps
// ---------------------------------------------------------------------------
//
// **§9.3 publishes a cap for every message and the code enforced one shared
// number for the whole join family.** `JOIN_REQUEST` is published at 512 and
// was checked at `JOIN_REQ_MAX` = 4 096; `JOIN_ACCEPT` at 8 192, `JOIN_REJECT`
// at 128, `PLAYER_LIST` at 2 048 and `TABLE_READY` at 1 024 were all checked at
// `JOIN_RESP_MAX` = 16 384. So four published bounds were enforced by nothing,
// and a client built to §9.3 would refuse messages this one considers legal.
// `S1-W`.
//
// The typical sizes §9.3 gives alongside them — ~180, ~1 800, ~45, ~1 200,
// ~200 — are all comfortably inside, which is why nothing ever noticed.

/// §9.3: `JOIN_REQUEST`, typical ~180.
pub const JOIN_REQUEST_MAX: usize = 512;
/// §9.3: `JOIN_ACCEPT`, typical ~1 800. It carries the advert verbatim.
pub const JOIN_ACCEPT_MAX: usize = 8_192;
/// §9.3: `JOIN_REJECT`, typical ~45.
pub const JOIN_REJECT_MAX: usize = 128;
/// §9.3: `PLAYER_LIST`, typical ~1 200 at a full roster.
pub const PLAYER_LIST_MAX: usize = 2_048;
/// §9.3: `TABLE_READY`, typical ~200.
///
/// **1 024 until 2026-09-02, and 1 024 cannot hold a `TABLE_READY` the corpus
/// calls legal.** `S1-W`'s second half asked which of two bounds was wrong and
/// answered it from the wrong end. The per-capability length was never missing:
/// §1.3, §4.2 and §9.4 all publish 32 B. What was missing was the arithmetic —
/// a maximal set is 32 names of 32 B, and as CBOR byte strings that is
/// 32 × (2 + 32) = 1 088 B for the array alone, before `roster_hash` (34),
/// `table_params_hash` (34), `list_serial`, `my_seat` and two array headers.
/// About 1 160 B of body, against a published payload cap of 1 024.
///
/// So the cap is raised to 1 536 rather than a per-capability bound invented,
/// which is also the corpus's own method: §9.4 derives a container from its
/// collection bound, and `TABLE_AD_SIGNED_MAX` is the same 1 536.
///
/// Nothing has ever come near either number — the one capability in use is
/// `deck/bs-bg12-secp256k1/1` at 24 bytes — so this decides what is *refused*,
/// not what is sent. It mattered anyway, because a conforming peer that filled
/// its capability set would have been refused by this client for sending
/// exactly what §1.3 permits.
pub const TABLE_READY_MAX: usize = 1_536;
pub const TABLE_FRAME_MAX: usize = 262_144;
pub const MAX_EMBEDDED_EVENT: usize = 32_768;

/// Everything a `HAND_ABORT` carries except `n(3) evidence`.
///
/// `attributed` is at most `MAX_SEATS` keys of 32 B, `cert_hash` is 33, and
/// `deltas` and `final_stacks` are one number per seat each. Two kilobytes is
/// several times that and is deliberately loose: it is subtracted from a
/// transmit budget, so an over-estimate costs a little evidence room and an
/// under-estimate would let a legal abort be built that cannot be sent.
pub const ABORT_FIXED_MAX: usize = 2_048;

/// The largest embedded `SignedEvent` a `HAND_ABORT` may carry **on this
/// transport**, and it is smaller than the protocol's own bound.
///
/// # `PROTOCOL.md` §4.10's bound does not fit in the frame that carries it
///
/// §4.10's field table bounds `n(3) evidence` at *two `SignedEvent`s, each
/// ≤ `MAX_EMBEDDED_EVENT` = 32 768 B*. Two of those is **65 536 bytes, which is
/// `GOSSIP_MAX_TRANSMIT` exactly** — before the envelope, before the signature,
/// before `attributed`, `cert_hash`, `deltas` and `final_stacks`, and before
/// CBOR's own framing. A conforming peer can therefore build a `HAND_ABORT` that
/// this corpus calls legal and that the table's transport cannot carry: the
/// sender's own GossipSub refuses it at `publish`, so the hand it was meant to
/// end runs to its deadline instead and the message is never seen by anybody.
///
/// The corpus is not self-contradictory about this so much as split across two
/// documents: `PROTOCOL.md` §13 sizes the evidence against `TABLE_FRAME_MAX`
/// (262 144 B, the **table stream**), and `NETWORK_STACK.md` §6.2 caps
/// GossipSub at 65 536. A table whose traffic is on the stream — or on the Tox
/// group D-019 moves it to — has the room. This client publishes hand events on
/// GossipSub, so this client does not.
///
/// # What this constant does about it
///
/// It states the transport-honest bound and proves it fits, below. This client
/// emits no evidence larger than this and this is what its abort decoder
/// allows, which is **tighter than §4.10 and therefore refuses nothing §4.10
/// permits that could have arrived** — a larger one could not have been sent.
///
/// It is not a fix for the contradiction, which is `PROTOCOL.md`'s to make: it
/// is either a smaller per-element bound in §4.10's table, or hand traffic on
/// the stream. Both are wire decisions and neither is one an implementation may
/// take on its own.
///
/// The real sizes are far below either bound — a `SHUFFLE_STEP` is about 9 KB
/// and a `SHUFFLE_PROOF` about 5.6 KB — so no cause-2 abort this client builds
/// comes near it. The bound matters for what is *refused*, not for what is sent.
pub const ABORT_EVIDENCE_MAX: usize = 24_576;

/// The whole of a `HAND_ABORT` at its largest: the fixed part and two evidence
/// entries.
pub const HAND_ABORT_MAX: usize = 2 * ABORT_EVIDENCE_MAX + ABORT_FIXED_MAX;

// ---------------------------------------------------------------------------
// Lobby and transport timing
// ---------------------------------------------------------------------------

pub const AD_TTL_MS: u64 = 90_000;
pub const AD_REBROADCAST_MS: u64 = 30_000;

/// How old a `PLAYER_LIST` may be before a client refuses it.
///
/// A founder re-sends its list as the roster changes and a joiner needs one
/// promptly, so this is generous rather than tight: what it has to exclude is a
/// genuine list from long ago being replayed at a client that has no serial of
/// its own to compare it with. Three re-broadcast intervals, on the same
/// reasoning as `AD_TTL_MS`.
pub const LIST_MAX_AGE_MS: u64 = 90_000;
pub const MAX_AD_LIFETIME_MS: u64 = 300_000;
pub const MAX_CLOCK_SKEW_MS: u64 = 120_000;
pub const PRESENCE_TTL_MS: u64 = 120_000;
pub const PRESENCE_HEARTBEAT_MS: u64 = 40_000;

/// How often the DHT announce is repeated. A legitimate `600_000`, and the
/// only one in the protocol: a Mainline record is short-lived and the announce
/// must be renewed.
pub const REANNOUNCE_INTERVAL_MS: u64 = 600_000;

pub const IDLE_CONNECTION_TIMEOUT_MS: u64 = 60_000;
pub const MDNS_QUERY_INTERVAL_MS: u64 = 15_000;
pub const HANDSHAKE_DEADLINE_MS: u64 = 15_000;
pub const SNAPSHOT_PEER_COUNT: usize = 4;

pub const MAX_TRACKED_TABLES: usize = 4_096;
pub const MAX_TRACKED_PRESENCE: usize = 8_192;
pub const MAX_ADS_PER_TABLE_KEY_PER_MIN: u32 = 4;
pub const MAX_ADS_PER_PEER_PER_MIN: u32 = 20;
pub const MAX_PRESENCE_PER_PEER_PER_MIN: u32 = 4;

// ---------------------------------------------------------------------------
// The deadline bounds (`PROTOCOL.md` §8.2)
// ---------------------------------------------------------------------------

/// The largest whole-hand deadline any advert may carry.
pub const HAND_DEADLINE_CAP_MS: u64 = 3_600_000;

/// What one reopening raise costs: it entitles up to `n - 1` further actions.
pub const fn reopening_cost_ms(seats: u8, action_timeout_ms: u64, action_grace_ms: u64) -> u64 {
    let n = seats as u64;
    if n < 2 {
        return 0;
    }
    (n - 1) * (action_timeout_ms + action_grace_ms)
}

/// The smallest deadline at which a hand with no reopening raise can finish.
///
/// A hand is `6n + 20` round trips; `4n` of them are betting actions and the
/// other `2n + 23` are cryptographic steps.
///
/// `time_bank_ms` is the per-hand thinking reserve every seat may spend on top
/// of `action_timeout_ms`, and it enters as `n * time_bank_ms` — once per seat
/// per hand, which is exactly what the reserve is. Without that term a table
/// that offers a bank abandons the hand under the first table where everybody
/// uses it, which is the one failure a bank must not cause.
pub const fn hand_deadline_floor_ms(
    seats: u8,
    action_timeout_ms: u64,
    action_grace_ms: u64,
    crypto_step_timeout_ms: u64,
    hand_delay_ms: u64,
    time_bank_ms: u64,
) -> u64 {
    let n = seats as u64;
    hand_delay_ms
        + (2 * n + 23) * crypto_step_timeout_ms
        + 4 * n * (action_timeout_ms + action_grace_ms)
        + n * time_bank_ms
}

/// The **admitted** minimum: the floor plus one reopening.
///
/// Between the floor and this, a table buys the walk and no reopening at all,
/// so the very first re-raise reaches the same abort the floor was raised to
/// prevent. A joiner rejects any advert below this, and does **not** join and
/// substitute its own bound — two peers running different whole-hand deadlines
/// disagree about whether a hand aborted, which is a consensus fault.
pub const fn hand_deadline_min_ms(
    seats: u8,
    action_timeout_ms: u64,
    action_grace_ms: u64,
    crypto_step_timeout_ms: u64,
    hand_delay_ms: u64,
    time_bank_ms: u64,
) -> u64 {
    hand_deadline_floor_ms(
        seats,
        action_timeout_ms,
        action_grace_ms,
        crypto_step_timeout_ms,
        hand_delay_ms,
        time_bank_ms,
    ) + reopening_cost_ms(seats, action_timeout_ms, action_grace_ms)
}

/// The largest per-hand thinking reserve a table may advertise.
///
/// **Derived, not chosen.** The reserve enters the whole-hand floor `n` times,
/// so at ten seats every second of reserve costs ten seconds of deadline, and
/// [`HAND_DEADLINE_CAP_MS`] is what a hand may not exceed. This is exactly the
/// headroom the largest table has, shared out one share per seat — a number
/// picked by hand drifts from the cap the moment any other term moves, which
/// the test below caught it doing on the first attempt.
///
/// It is stated against the default timings. A table with a longer
/// `action_timeout_ms` has less headroom, and what enforces payability there is
/// not this ceiling but the joiner's own floor check, which runs on the
/// advert's own numbers.
pub const TIME_BANK_CAP_MS: u32 = {
    let headroom = HAND_DEADLINE_CAP_MS
        - hand_deadline_min_ms(MAX_SEATS, 20_000, 5_000, 30_000, 7_000, 0);
    (headroom / MAX_SEATS as u64) as u32
};

/// What a table offers unless its founder says otherwise.
///
/// Zero for `RATED_SNG_POKERTH_V1`, whose whole-hand deadline §13 fixes to the
/// millisecond and which therefore has no room for one. Every other table gets
/// half a minute, which is a decision a human can actually use and which the
/// floor absorbs at any seat count this protocol admits.
pub const fn default_time_bank_ms(seats: u8) -> u32 {
    if seats == MAX_SEATS {
        0
    } else {
        30_000
    }
}

// ---------------------------------------------------------------------------
// Preset identity (`PROTOCOL.md` §7.2 rule 3)
// ---------------------------------------------------------------------------

/// The two byte strings `preset_id` may carry, and nothing else.
///
/// A preset name **asserts** the configuration's values, so an unrecognised
/// name is a table whose identity cannot be checked: the name asserts values by
/// that rule, the advert asserts values in its fields, and nothing says the two
/// agree. Two clients shipping different tables under one unknown name is an
/// identity failure that has already happened twice in this project, so a third
/// value is rejected on sight whether or not this client knows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PresetId {
    /// Implies §13's exact values, or the advert is rejected.
    RatedSngPokerthV1,
    /// Implies nothing; every value is carried in the advert's own fields.
    Custom,
}

impl PresetId {
    pub const fn as_str(self) -> &'static str {
        match self {
            PresetId::RatedSngPokerthV1 => "RATED_SNG_POKERTH_V1",
            PresetId::Custom => "CUSTOM",
        }
    }

    /// Parse a `preset_id` from the wire, rejecting any third value.
    pub fn parse(s: &str) -> Option<PresetId> {
        match s {
            "RATED_SNG_POKERTH_V1" => Some(PresetId::RatedSngPokerthV1),
            "CUSTOM" => Some(PresetId::Custom),
            _ => None,
        }
    }
}

/// The rated preset's whole-hand deadline, the one value §13 fixes directly.
pub const RATED_HAND_DEADLINE_MS: u64 = 3_300_000;

/// Chips the rated preset starts every seat with.
pub const RATED_START_STACK: Chips = 10_000;

/// The rated preset's opening small blind. The big blind is twice it, by §7.2,
/// and the schedule's first level **is** it, also by §7.2.
pub const RATED_SMALL_BLIND: Chips = 50;

/// How many hands the rated preset plays before the blinds double.
pub const RATED_BLIND_EVERY_N_HANDS: u16 = 11;

/// Where the doubling stops: the whole table's chips, halved.
///
/// `RATED_SEATS * RATED_START_STACK / 2`, which is a small blind no hand can be
/// played past — every seat is all in before it is posted. Stated as the product
/// rather than as 50 000 so that changing a seat count or a stack cannot leave a
/// cap that means something different.
pub const RATED_SMALL_BLIND_CAP: Chips = RATED_SEATS as Chips * RATED_START_STACK / 2;

/// How many reopening raises a Sit-and-Go's whole-hand deadline buys.
///
/// §13 does not state this as a number — it states the rated deadline as
/// 3 300 000 ms and then *derives* that the surplus over the floor buys four
/// reopening raises. Four is therefore the property the value was chosen for,
/// and it is the property to preserve at any other seat count.
pub const SNG_REOPENINGS: u64 = 4;

/// A Sit-and-Go's whole-hand deadline, for a table of `seats`.
///
/// The floor plus [`SNG_REOPENINGS`] reopenings, which is the guarantee §13's
/// number was picked to give — except at [`RATED_SEATS`], where the number is
/// **pinned** by the preset and is not ours to derive. That exception is not
/// tidiness: `RATED_SNG_POKERTH_V1` asserts 3 300 000 exactly, a derived
/// 3 197 000 would be a different `table_params_hash`, and a client computing it
/// could not join a rated table. The preset owns its own numbers; everything
/// else is derived the way the preset's own were.
pub const fn sng_hand_deadline_ms(seats: u8) -> u64 {
    if seats == RATED_SEATS {
        return RATED_HAND_DEADLINE_MS;
    }
    // With the reserve this table will actually advertise, or the joiner's own
    // floor check refuses the founder's advert on the founder's own numbers.
    let floor = hand_deadline_floor_ms(
        seats,
        20_000,
        5_000,
        30_000,
        7_000,
        default_time_bank_ms(seats) as u64,
    );
    let surplus = SNG_REOPENINGS * reopening_cost_ms(seats, 20_000, 5_000);
    let want = floor + surplus;
    if want > HAND_DEADLINE_CAP_MS {
        HAND_DEADLINE_CAP_MS
    } else {
        want
    }
}

/// A Sit-and-Go's blind cap, for a table of `seats`.
///
/// The whole table's chips, halved: a small blind no hand can be played past,
/// because every seat is already all in before it is posted. §13 annotates the
/// rated cap as `n(11)*n(9)/2`, so this is that formula rather than a second
/// number that happens to agree with it at ten seats.
pub const fn sng_small_blind_cap(seats: u8) -> Chips {
    seats as Chips * RATED_START_STACK / 2
}

/// Seats at a rated table, and also the number needed to start one.
///
/// Ten of ten: a rated table deals its first hand when it is full and not
/// before, which is what makes every rated table the same game.
pub const RATED_SEATS: u8 = 10;


// ---------------------------------------------------------------------------
// Compile-time relationships between the constants
// ---------------------------------------------------------------------------
//
// These hold by construction rather than by test, so a value edited into an
// inconsistent state fails the build instead of a test run. They are the
// relationships a reader would otherwise have to re-derive by eye.

/// The caps nest the way the transports do: an advert fits its signed form,
/// which fits a lobby message, which fits a GossipSub frame.
const _: () = assert!(TABLE_AD_MAX < TABLE_AD_SIGNED_MAX);
const _: () = assert!(TABLE_AD_SIGNED_MAX <= LOBBY_MSG_MAX);
const _: () = assert!(LOBBY_MSG_MAX <= GOSSIP_MAX_TRANSMIT);
const _: () = assert!(LOBBY_CHAT_MAX <= LOBBY_MSG_MAX);
const _: () = assert!(MAX_EMBEDDED_EVENT <= TABLE_FRAME_MAX);

/// The abort must fit in the frame that carries it. This is the assertion the
/// corpus does not make and whose absence let §4.10's bound stand: it fails the
/// build rather than the hand.
const _: () = assert!(HAND_ABORT_MAX <= GOSSIP_MAX_TRANSMIT);
/// And the reason this client's bound is its own rather than §4.10's: two of
/// §4.10's would not fit. Written as an assertion so that a later pass which
/// raises `ABORT_EVIDENCE_MAX` to `MAX_EMBEDDED_EVENT` — which looks like
/// bringing the client into line with the protocol — breaks the build instead
/// of shipping aborts nobody can send.
const _: () = assert!(2 * MAX_EMBEDDED_EVENT + ABORT_FIXED_MAX > GOSSIP_MAX_TRANSMIT);
const _: () = assert!(ABORT_EVIDENCE_MAX <= MAX_EMBEDDED_EVENT);
const _: () = assert!(SNAPSHOT_RESP_MAX <= TABLE_FRAME_MAX);

/// **Two** lost rebroadcasts must not expire an advert, and two lost heartbeats
/// must not drop a player from the lobby.
///
/// `NETWORK_STACK.md` \u{a7}10.3 states the relation as **3x** for both rows and
/// these constants satisfy it exactly. The assertion said `2 *` for a long time,
/// which is a weaker claim than the corpus makes and than the comment above it
/// made: it would have admitted a pair that tolerates one loss, not two.
/// **The carrier's own blind repair ladder, mirrored here because a Rust
/// constant is measured against it.**
///
/// `gcc_resend_packets` retries an unacked send-array entry only when `delta`
/// reaches a power of two in whole seconds, and `create_array_entry` stamps
/// whole seconds — so the attempts fall in the seconds **T+3, T+5, T+9, T+17
/// and T+33** after the message was queued. T+65 does not exist: the head check
/// drops the peer at `GC_CONFIRMED_PEER_TIMEOUT` = `GC_PING_TIMEOUT * 4 + 10` =
/// 58 s first.
///
/// This number lives in C and nothing in a Rust build can notice it changing,
/// so `tools/build-tox.ps1` carries the ladder's own expression as a checked
/// patch marker. If that marker is ever gone, this constant is stale and must
/// be re-derived before it is trusted.
pub const CARRIER_LADDER_LAST_MS: u32 = 33_000;

/// **How long a client may hear nothing from a table before it stops saying
/// it is playing at one (`S1-BH`).**
///
/// Derived rather than picked. `CARRIER_GIVES_UP_MS` = 58 000 is the point at
/// which the carrier itself drops a confirmed peer, `CARRIER_LADDER_LAST_MS` =
/// 33 000 is its last blind repair, and housekeeping reads the group at its own
/// interval — so a reading may be that stale on top. Two minutes is past all
/// three with room, which matters because the cost of being wrong here is
/// telling a player who is fine that they are not.
pub const DEAF_MS: u64 = 120_000;

/// When the carrier itself concludes a confirmed peer is gone.
///
/// `GC_CONFIRMED_PEER_TIMEOUT` in `group_chats.h`. Past it there is nothing
/// left to wait for: the peer is no longer in the group.
pub const CARRIER_GIVES_UP_MS: u32 = 58_000;

/// **The shortest stage budget a table may advertise.**
///
/// `S1-BK`: a stage that must finish in less time than the carrier needs to
/// repair one dropped datagram votes out seats for messages that are in flight
/// and would have arrived. §7.2 admitted anything from **one second**, which is
/// thirty-three times shorter than the blind ladder.
///
/// The floor is the ladder rather than something larger because `patches/0011`
/// removes the ladder from the common path: a stage that is waiting asks the
/// seats it is waiting for to re-send, and that request is answered by an
/// immediate retransmission — one round trip, 36 to 326 ms measured. The blind
/// ladder is what remains when the request itself is lost, and a table that
/// wants to survive *that* may advertise more. It may not advertise less.
pub const CRYPTO_STEP_MIN_MS: u32 = 30_000;

const _: () = assert!(AD_REBROADCAST_MS * 3 <= AD_TTL_MS);

/// **The two numbers meet here, which is what `S1-BK` says nothing did.**
const _: () = assert!(
    CRYPTO_STEP_MIN_MS < CARRIER_GIVES_UP_MS,
    "a stage budget past the carrier's own patience waits for a peer that is already gone"
);
const _: () = assert!(
    CRYPTO_STEP_MIN_MS >= CARRIER_LADDER_LAST_MS - 3_000,
    "the floor must leave the fast repair room to land inside one stage"
);
const _: () = assert!(PRESENCE_HEARTBEAT_MS * 3 <= PRESENCE_TTL_MS);

/// The boundary window sits above the stage numbers and below the boundary
/// checkpoint, with room for every seat.
const _: () = assert!(BOUNDARY_SEQUENCE_BASE > MAX_STAGES_PER_HAND);
const _: () = assert!(BOUNDARY_CHECKPOINT_BASE > BOUNDARY_SEQUENCE_BASE + MAX_SEATS as u64);
/// The checkpoint band ends above every ordinary stage and above the boundary
/// window, which is the constraint §4.9 argues the base from: a reconciliation
/// round extends **upwards**, so a base low enough for the band to reach the
/// window would let a disputed checkpoint collide with a seat's `PLAYER_SIT_IN`.
const _: () = assert!(
    BOUNDARY_CHECKPOINT_BASE > BOUNDARY_SEQUENCE_BASE + MAX_SEATS as u64
        && BOUNDARY_CHECKPOINT_BASE > MAX_STAGES_PER_HAND
);

/// The rated deadline is inside the range every advert must satisfy.
const _: () = assert!(RATED_HAND_DEADLINE_MS <= HAND_DEADLINE_CAP_MS);

/// The rated preset seats no more than the protocol allows, and starts full.
const _: () = assert!(RATED_SEATS <= MAX_SEATS);
/// The cap is a small blind that cannot be posted: every seat is already all in.
const _: () = assert!(RATED_SMALL_BLIND_CAP == 50_000);
/// The general formulae agree with the preset's own numbers at ten seats, or
/// one of them is wrong.
const _: () = assert!(sng_small_blind_cap(RATED_SEATS) == RATED_SMALL_BLIND_CAP);
const _: () = assert!(sng_hand_deadline_ms(RATED_SEATS) == RATED_HAND_DEADLINE_MS);
const _: () = assert!(RATED_SMALL_BLIND_CAP > RATED_START_STACK);
const _: () = assert!(
    RATED_HAND_DEADLINE_MS >= hand_deadline_min_ms(MAX_SEATS, 20_000, 5_000, 30_000, 7_000, 0)
);

#[cfg(test)]
mod tests {
    use super::*;

    /// §8.2's published table, extended to `n = 8` which it omits.
    #[test]
    fn the_floor_matches_the_published_derivation() {
        // The rated preset's timing: 20 s action, 5 s grace, 30 s crypto step,
        // 7 s hand delay.
        let f = |n: u8| hand_deadline_floor_ms(n, 20_000, 5_000, 30_000, 7_000, 0);
        assert_eq!(f(2), 1_017_000);
        assert_eq!(f(4), 1_337_000);
        assert_eq!(f(6), 1_657_000);
        assert_eq!(f(8), 1_977_000);
        assert_eq!(f(10), 2_297_000);
    }

    #[test]
    fn the_admitted_minimum_is_the_floor_plus_one_reopening() {
        for n in [2u8, 4, 6, 8, 10] {
            let floor = hand_deadline_floor_ms(n, 20_000, 5_000, 30_000, 7_000, 0);
            let cost = reopening_cost_ms(n, 20_000, 5_000);
            let min = hand_deadline_min_ms(n, 20_000, 5_000, 30_000, 7_000, 0);
            assert_eq!(min, floor + cost, "at {n} seats");
            assert!(min > floor, "the minimum must leave room for one reopening");
        }
        assert_eq!(hand_deadline_min_ms(10, 20_000, 5_000, 30_000, 7_000, 0), 2_522_000);
    }

    /// The reserve buys thinking time for **every** seat, once per hand, or a
    /// table that offers one abandons the first hand where everybody uses it.
    #[test]
    fn the_thinking_reserve_enters_the_floor_once_per_seat() {
        for n in 2..=MAX_SEATS {
            let without = hand_deadline_floor_ms(n, 20_000, 5_000, 30_000, 7_000, 0);
            let with = hand_deadline_floor_ms(n, 20_000, 5_000, 30_000, 7_000, 30_000);
            assert_eq!(
                with - without,
                n as u64 * 30_000,
                "at {n} seats the reserve must be budgeted for every seat"
            );
        }
    }

    /// A table may not offer a reserve it cannot afford: the term enters the
    /// floor `n` times, and the floor may not pass the cap.
    #[test]
    fn the_largest_admitted_reserve_still_fits_under_the_cap() {
        let floor = hand_deadline_min_ms(
            MAX_SEATS,
            20_000,
            5_000,
            30_000,
            7_000,
            TIME_BANK_CAP_MS as u64,
        );
        assert!(
            floor <= HAND_DEADLINE_CAP_MS,
            "the cap admits a reserve the deadline cannot pay for: {floor} > {HAND_DEADLINE_CAP_MS}"
        );
    }

    /// The shipped defaults have to be payable by the deadline the same
    /// defaults advertise, or a founder's own advert fails the joiner's floor.
    #[test]
    fn every_default_table_can_pay_for_the_reserve_it_offers() {
        for n in 2..=MAX_SEATS {
            let bank = default_time_bank_ms(n) as u64;
            let minimum = hand_deadline_min_ms(n, 20_000, 5_000, 30_000, 7_000, bank);
            let advertised = sng_hand_deadline_ms(n);
            assert!(
                advertised >= minimum,
                "at {n} seats the advert offers {bank} ms of reserve and only {advertised} ms of deadline, against a floor of {minimum}"
            );
        }
    }

    /// The regression that no timing test could see. 600 000 ms is below the
    /// floor at every seat count, and a protocol-paced walk passes at it
    /// because what it cannot pay for is the human action term.
    #[test]
    fn six_hundred_thousand_is_below_the_floor_everywhere() {
        for n in [2u8, 4, 6, 8, 10] {
            assert!(
                hand_deadline_floor_ms(n, 20_000, 5_000, 30_000, 7_000, 0) > 600_000,
                "600 000 ms would be legal at {n} seats"
            );
        }
    }

    /// §8.2 derives four reopening raises at the rated value; the code asserts
    /// the consequence rather than the inequality, because an inequality once
    /// passed at a value that falsified this very sentence.
    #[test]
    fn the_rated_deadline_buys_exactly_four_reopenings() {
        let floor = hand_deadline_floor_ms(MAX_SEATS, 20_000, 5_000, 30_000, 7_000, 0);
        let cost = reopening_cost_ms(MAX_SEATS, 20_000, 5_000);
        assert_eq!((RATED_HAND_DEADLINE_MS - floor) / cost, 4);
    }

    #[test]
    fn a_one_seat_table_costs_nothing_to_reopen() {
        // Guarded so the n - 1 cannot underflow on a malformed advert.
        assert_eq!(reopening_cost_ms(1, 20_000, 5_000), 0);
        assert_eq!(reopening_cost_ms(0, 20_000, 5_000), 0);
    }

    #[test]
    fn preset_ids_round_trip_and_a_third_name_is_refused() {
        for p in [PresetId::RatedSngPokerthV1, PresetId::Custom] {
            assert_eq!(PresetId::parse(p.as_str()), Some(p));
        }
        for unknown in ["HEADS_UP_PLAY_MONEY_V1", "", "rated_sng_pokerth_v1", "TURBO"] {
            assert_eq!(
                PresetId::parse(unknown),
                None,
                "an unrecognised preset name asserts values nothing checks"
            );
        }
    }

    /// The lobby presence pair is `PROTOCOL.md` \u{a7}12's, and it was not.
    ///
    /// **What this pins, and why a compile-time assertion did not.** These two
    /// constants and the assertion beside them were correct and **dead**:
    /// `net::lobbytalk` carried a second `PRESENCE_TTL_MS` at `90_000` with a
    /// `PRESENCE_EVERY_MS` at `30_000`, and those were what the client sent and
    /// expired on. The assertion guarded numbers nothing reached, so it read as
    /// protection and protected nothing.
    ///
    /// The drift has a clean explanation. `AD_TTL_MS` / `AD_REBROADCAST_MS` are
    /// `90_000` / `30_000`, and `lobbytalk`'s comment said presence held *"the
    /// same relationship the table advertisements have with their own TTL"*.
    /// The relationship is 3x; what was copied was the values.
    ///
    /// **The failure it caused is only visible against a conforming peer**,
    /// which is why our own two-node runs never showed it. A conforming client
    /// sends presence every `40_000`. A client expiring at `90_000` drops it
    /// after two consecutive losses - 80 000 ms is inside, 120 000 ms is not -
    /// so a player sitting in the lobby vanishes and comes back. That is exactly
    /// the property `lobbytalk`'s comment claimed to have.
    #[test]
    fn the_presence_pair_is_the_one_the_corpus_publishes() {
        assert_eq!(PRESENCE_TTL_MS, 120_000, "PROTOCOL.md \u{a7}12 and its register");
        assert_eq!(PRESENCE_HEARTBEAT_MS, 40_000, "PROTOCOL.md \u{a7}12");
        assert_eq!(
            PRESENCE_TTL_MS,
            3 * PRESENCE_HEARTBEAT_MS,
            "NETWORK_STACK.md \u{a7}10.3 puts presence in the same 3\u{d7} relation as \
             the advert row, and the whole point of the ratio is that two lost \
             heartbeats do not remove somebody who is sitting there"
        );
    }
}
