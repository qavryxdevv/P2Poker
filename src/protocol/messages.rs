//! The message catalogue: every event type on the wire, and what each one is.
//!
//! Transcribed from `PROTOCOL.md` §4.11, which owns the table (D-011). The
//! codes are wire-visible and permanent: adding one is a minor protocol change,
//! changing or removing one is a major change.
//!
//! # Why this is a closed enum with a checked conversion
//!
//! `SPEC_CS.md` section 17 assumes a peer that emits arbitrary bytes, so an
//! event type arriving from the network is sixteen attacker-chosen bits. A
//! closed enum with [`TryFrom<u16>`] makes an unknown code a rejection at the
//! parser rather than a `match` arm somebody forgot, and the range reserved for
//! future use is refused explicitly rather than by omission.

use core::fmt;

/// Which transport carries a message (`PROTOCOL.md` §4.11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Channel {
    /// Direct streams between the participants of one table, and only them.
    TableMesh,
    /// The GossipSub lobby topic.
    LobbyBroadcast,
    /// Request/response against a lobby peer.
    LobbyRpc,
    /// The lobby chat topic.
    LobbyChat,
    /// Request/response while joining a table.
    JoinRpc,
}

/// How a message participates in a chain stage (`PROTOCOL.md` §4.11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StageKind {
    /// Not part of a stage at all; `chain_scope = 0`.
    Unchained,
    /// Every required emitter must produce a byte-identical body.
    Collective,
    /// One named seat writes it.
    Single,
    /// Chained, but legal outside its stage.
    OutOfStage,
    /// Closes a chain on its own, as a function of the genesis alone, so peers
    /// with different prefixes can still reach the same terminal value.
    WitnessIndependentTerminal,
}

/// Every event type of `protocol_version = 1`.
///
/// The discriminants are the wire codes of `PROTOCOL.md` §4.11.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u16)]
pub enum EventType {
    Hello = 0x0001,
    Capabilities = 0x0002,

    LobbyTableAd = 0x0101,
    LobbyTableRemove = 0x0102,
    LobbyPlayerPresence = 0x0103,
    LobbySnapshotRequest = 0x0104,
    LobbySnapshotResponse = 0x0105,
    LobbyChat = 0x0106,
    /// A line among the seats of one table (`S1-CS`, §7.8).
    TableChat = 0x0107,
    /// `S1-FQ`: a seat's own word that its player left the table (§7.10).
    TableLeave = 0x0108,

    JoinRequest = 0x0201,
    JoinAccept = 0x0202,
    JoinReject = 0x0203,
    PlayerList = 0x0204,
    TableReady = 0x0205,

    RngCommit = 0x0301,
    RngReveal = 0x0302,
    HandInit = 0x0303,
    DeckInit = 0x0304,
    ShuffleStep = 0x0305,
    ShuffleProof = 0x0306,
    DeckCommit = 0x0307,

    DealPrivate = 0x0401,
    BoardReveal = 0x0402,
    ShowdownReveal = 0x0403,
    ShowdownMuck = 0x0404,

    ActionCheck = 0x0501,
    ActionCall = 0x0502,
    ActionBet = 0x0503,
    ActionRaise = 0x0504,
    ActionFold = 0x0505,

    TimeoutVote = 0x0601,
    TimeoutCert = 0x0602,
    /// `S1-BM`: the grow side of the roster, symmetric with the pair above.
    ReturnVote = 0x0603,
    ReturnCert = 0x0604,

    StateHash = 0x0701,
    StateAck = 0x0702,
    Dispute = 0x0703,

    HandComplete = 0x0801,
    HandAbort = 0x0802,
    PlayerSitOut = 0x0803,
    PlayerSitIn = 0x0804,
    PlayerLeave = 0x0805,
}

/// A `u16` that is not a known event type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownEventType {
    /// Not in the catalogue.
    Unknown(u16),
    /// Inside the range held back for future versions.
    Reserved(u16),
}

impl fmt::Display for UnknownEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnknownEventType::Unknown(c) => write!(f, "unknown event type 0x{c:04x}"),
            UnknownEventType::Reserved(c) => write!(f, "reserved event type 0x{c:04x}"),
        }
    }
}

impl std::error::Error for UnknownEventType {}

impl EventType {
    /// The range held back for future protocol versions.
    pub const RESERVED: core::ops::RangeInclusive<u16> = 0xF000..=0xFFFF;

    /// Every type, in wire-code order.
    pub const ALL: [EventType; 43] = [
        EventType::Hello,
        EventType::Capabilities,
        EventType::LobbyTableAd,
        EventType::LobbyTableRemove,
        EventType::LobbyPlayerPresence,
        EventType::LobbySnapshotRequest,
        EventType::LobbySnapshotResponse,
        EventType::LobbyChat,
        EventType::TableChat,
        EventType::TableLeave,
        EventType::JoinRequest,
        EventType::JoinAccept,
        EventType::JoinReject,
        EventType::PlayerList,
        EventType::TableReady,
        EventType::RngCommit,
        EventType::RngReveal,
        EventType::HandInit,
        EventType::DeckInit,
        EventType::ShuffleStep,
        EventType::ShuffleProof,
        EventType::DeckCommit,
        EventType::DealPrivate,
        EventType::BoardReveal,
        EventType::ShowdownReveal,
        EventType::ShowdownMuck,
        EventType::ActionCheck,
        EventType::ActionCall,
        EventType::ActionBet,
        EventType::ActionRaise,
        EventType::ActionFold,
        EventType::TimeoutVote,
        EventType::TimeoutCert,
        EventType::ReturnVote,
        EventType::ReturnCert,
        EventType::StateHash,
        EventType::StateAck,
        EventType::Dispute,
        EventType::HandComplete,
        EventType::HandAbort,
        EventType::PlayerSitOut,
        EventType::PlayerSitIn,
        EventType::PlayerLeave,
    ];

    pub const fn code(self) -> u16 {
        self as u16
    }

    /// Whether the event occupies a place in a table's chain.
    ///
    /// Fourteen types have no chain scope. Five of them ride the table mesh -
    /// `HELLO`, `CAPABILITIES`, `PLAYER_LIST`, `TABLE_CHAT` and `DISPUTE` - which contradicts
    /// `PROTOCOL.md`'s prose claim that `DISPUTE` is the only one. The §4.11
    /// table is the normative data and this follows it.
    pub const fn chain_scope(self) -> u8 {
        use EventType::*;
        match self {
            Hello | Capabilities | LobbyTableAd | LobbyTableRemove | LobbyPlayerPresence
            | LobbySnapshotRequest | LobbySnapshotResponse | LobbyChat | TableChat | TableLeave | JoinRequest
            | JoinAccept | JoinReject | PlayerList | Dispute => 0,
            _ => 1,
        }
    }

    pub const fn channel(self) -> Channel {
        use EventType::*;
        match self {
            LobbyTableAd | LobbyTableRemove | LobbyPlayerPresence => Channel::LobbyBroadcast,
            LobbySnapshotRequest | LobbySnapshotResponse => Channel::LobbyRpc,
            LobbyChat => Channel::LobbyChat,
            JoinRequest | JoinAccept | JoinReject => Channel::JoinRpc,
            _ => Channel::TableMesh,
        }
    }

    pub const fn stage_kind(self) -> StageKind {
        use EventType::*;
        match self {
            // Unchained: no stage at all.
            Hello | Capabilities | LobbyTableAd | LobbyTableRemove | LobbyPlayerPresence
            | LobbySnapshotRequest | LobbySnapshotResponse | LobbyChat | TableChat | TableLeave | JoinRequest
            | JoinAccept | JoinReject | PlayerList => StageKind::Unchained,

            // Chained, but legal outside its stage.
            Dispute => StageKind::OutOfStage,

            // Closes a chain alone, from the genesis, so peers with different
            // prefixes reach the same terminal value.
            HandAbort => StageKind::WitnessIndependentTerminal,

            // One named seat writes it.
            ShuffleStep | ShuffleProof | ActionCheck | ActionCall | ActionBet | ActionRaise
            | ActionFold | TimeoutVote | ReturnVote | PlayerSitOut | PlayerSitIn | PlayerLeave => {
                StageKind::Single
            }

            // Every required emitter produces a byte-identical body.
            TableReady | RngCommit | RngReveal | HandInit | DeckInit | DeckCommit | DealPrivate
            | BoardReveal | ShowdownReveal | ShowdownMuck | TimeoutCert | ReturnCert | StateHash | StateAck
            | HandComplete => StageKind::Collective,
        }
    }

    /// Whether this type may appear in a table's chain.
    pub const fn is_chained(self) -> bool {
        self.chain_scope() == 1
    }

    /// The three seat-boundary events, which a seat emits in the boundary
    /// window of a chain and which carry their own `sequence` rule.
    ///
    /// `PLAYER_SIT_IN` is the one type a seat outside `P(k)` may emit, so it is
    /// the sole route back for a seat that stopped signing (D-013).
    pub const fn is_boundary(self) -> bool {
        matches!(
            self,
            EventType::PlayerSitOut | EventType::PlayerSitIn | EventType::PlayerLeave
        )
    }

    pub const fn name(self) -> &'static str {
        use EventType::*;
        match self {
            Hello => "HELLO",
            Capabilities => "CAPABILITIES",
            LobbyTableAd => "LOBBY_TABLE_AD",
            LobbyTableRemove => "LOBBY_TABLE_REMOVE",
            LobbyPlayerPresence => "LOBBY_PLAYER_PRESENCE",
            LobbySnapshotRequest => "LOBBY_SNAPSHOT_REQUEST",
            LobbySnapshotResponse => "LOBBY_SNAPSHOT_RESPONSE",
            LobbyChat => "LOBBY_CHAT",
            TableChat => "TABLE_CHAT",
            TableLeave => "TABLE_LEAVE",
            JoinRequest => "JOIN_REQUEST",
            JoinAccept => "JOIN_ACCEPT",
            JoinReject => "JOIN_REJECT",
            PlayerList => "PLAYER_LIST",
            TableReady => "TABLE_READY",
            RngCommit => "RNG_COMMIT",
            RngReveal => "RNG_REVEAL",
            HandInit => "HAND_INIT",
            DeckInit => "DECK_INIT",
            ShuffleStep => "SHUFFLE_STEP",
            ShuffleProof => "SHUFFLE_PROOF",
            DeckCommit => "DECK_COMMIT",
            DealPrivate => "DEAL_PRIVATE",
            BoardReveal => "BOARD_REVEAL",
            ShowdownReveal => "SHOWDOWN_REVEAL",
            ShowdownMuck => "SHOWDOWN_MUCK",
            ActionCheck => "ACTION_CHECK",
            ActionCall => "ACTION_CALL",
            ActionBet => "ACTION_BET",
            ActionRaise => "ACTION_RAISE",
            ActionFold => "ACTION_FOLD",
            TimeoutVote => "TIMEOUT_VOTE",
            TimeoutCert => "TIMEOUT_CERT",
            ReturnVote => "RETURN_VOTE",
            ReturnCert => "RETURN_CERT",
            StateHash => "STATE_HASH",
            StateAck => "STATE_ACK",
            Dispute => "DISPUTE",
            HandComplete => "HAND_COMPLETE",
            HandAbort => "HAND_ABORT",
            PlayerSitOut => "PLAYER_SIT_OUT",
            PlayerSitIn => "PLAYER_SIT_IN",
            PlayerLeave => "PLAYER_LEAVE",
        }
    }
}

impl TryFrom<u16> for EventType {
    type Error = UnknownEventType;

    fn try_from(code: u16) -> Result<Self, Self::Error> {
        if EventType::RESERVED.contains(&code) {
            return Err(UnknownEventType::Reserved(code));
        }
        EventType::ALL
            .into_iter()
            .find(|t| t.code() == code)
            .ok_or(UnknownEventType::Unknown(code))
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}


// ---------------------------------------------------------------------------
// The envelope (`PROTOCOL.md` §2.3)
// ---------------------------------------------------------------------------

/// Thirty-two zero bytes: the sentinel an unchained event carries where a
/// chained one carries a table id or a parent hash.
pub const ZERO32: [u8; 32] = [0u8; 32];

/// The `hand_id` an unchained event carries.
pub const UNCHAINED_HAND_ID: u64 = u64::MAX;

/// The protocol version this build speaks.
///
/// **Re-exported, not defined.** This was a second `pub const PROTOCOL_VERSION:
/// u16 = 1` while `protocol::constants` held the first, and the two are read by
/// different halves of the client: this one fills the wire field and validates
/// it on receipt, the register's is hashed into the transcript. They agreed. A
/// version bump is one edit, and it would have moved one of them.
pub use super::constants::PROTOCOL_VERSION;

/// What is signed and hashed (`PROTOCOL.md` §2.3).
///
/// The field numbering is the wire format. This is `SPEC_CS.md` §12's required
/// list plus the two discriminators `chain_scope` and `event_class`, and the
/// spec's single "timestamp/deadline information" is split in two because the
/// halves have opposite trust properties: `emitted_at_unix_ms` comes from a
/// clock nobody shares and is advisory, while `next_deadline_ms` is normative.
#[derive(Clone, Debug, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct EventBody {
    #[n(0)]
    pub protocol_version: u16,
    #[cbor(n(1), with = "minicbor::bytes")]
    pub table_id: [u8; 32],
    #[n(2)]
    pub hand_id: u64,
    #[n(3)]
    pub sequence: u64,
    #[cbor(n(4), with = "minicbor::bytes")]
    pub sender_public_key: [u8; 32],
    #[n(5)]
    pub event_type: u16,
    /// Canonical CBOR of the per-type payload: the third nesting level, gated
    /// independently of this one.
    #[cbor(n(6), with = "minicbor::bytes")]
    pub payload: Vec<u8>,
    #[cbor(n(7), with = "minicbor::bytes")]
    pub previous_event_hash: [u8; 32],
    /// **Advisory only.** From a clock nobody shares; never a rule input.
    #[n(8)]
    pub emitted_at_unix_ms: u64,
    /// **Normative.**
    #[n(9)]
    pub next_deadline_ms: u32,
    /// `1` = occupies a stage slot, `0` = unchained.
    #[n(10)]
    pub chain_scope: u8,
    /// `0` ordinary, `1` `TIMEOUT_VOTE`, `2` `TIMEOUT_CERT`.
    #[n(11)]
    pub event_class: u8,
}

/// What travels on the wire (`PROTOCOL.md` §2.3).
///
/// The body is carried as opaque bytes rather than as a nested struct on
/// purpose: a signature covers bytes, so a verifier must see exactly the bytes
/// the signer signed and never a re-encoding of a decoded value.
#[derive(Clone, Debug, PartialEq, Eq, minicbor::Encode, minicbor::Decode)]
#[cbor(array)]
pub struct SignedEvent {
    #[cbor(n(0), with = "minicbor::bytes")]
    pub body: Vec<u8>,
    #[cbor(n(1), with = "minicbor::bytes")]
    pub signature: [u8; 64],
}

/// Why an envelope was rejected, before anything looked at its payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeError {
    /// A version this build does not speak.
    WrongVersion(u16),
    /// `event_type` is not in the catalogue.
    UnknownType(UnknownEventType),
    /// `chain_scope` disagrees with the catalogue for this type.
    ChainScopeMismatch { declared: u8, expected: u8 },
    /// An unchained event carried something other than the sentinels.
    UnchainedSentinelViolated(&'static str),
    /// `event_class` is not 0, 1 or 2.
    BadEventClass(u8),
    /// `event_class` does not match the type carrying it.
    EventClassMismatch { declared: u8, expected: u8 },
}

impl fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvelopeError::WrongVersion(v) => write!(f, "protocol version {v} is not spoken here"),
            EnvelopeError::UnknownType(e) => write!(f, "{e}"),
            EnvelopeError::ChainScopeMismatch { declared, expected } => {
                write!(f, "chain_scope {declared} but this type is {expected}")
            }
            EnvelopeError::UnchainedSentinelViolated(field) => {
                write!(f, "unchained event carried a non-sentinel {field}")
            }
            EnvelopeError::BadEventClass(c) => write!(f, "event_class {c} is not 0, 1 or 2"),
            EnvelopeError::EventClassMismatch { declared, expected } => {
                write!(f, "event_class {declared} but this type is {expected}")
            }
        }
    }
}

impl std::error::Error for EnvelopeError {}

impl EventBody {
    /// The event type, if the catalogue has it.
    pub fn typed(&self) -> Result<EventType, UnknownEventType> {
        EventType::try_from(self.event_type)
    }

    /// The class an event type must declare.
    pub const fn expected_class(t: EventType) -> u8 {
        match t {
            EventType::TimeoutVote => 1,
            EventType::TimeoutCert => 2,
            // The return pair references the boundary the same way (`S1-BM`).
            EventType::ReturnVote => 1,
            EventType::ReturnCert => 2,
            _ => 0,
        }
    }

    /// Build an **unchained** envelope, with every sentinel §2.3 requires.
    ///
    /// The counterpart of [`check_envelope`](Self::check_envelope), and here so
    /// that the two cannot disagree. A publisher that restated `chain_scope`,
    /// `event_class` and the four sentinels by hand had six chances to differ
    /// from the checker, and the first one written took three of them: it set a
    /// class of 2 where the catalogue says 0, and gave a sequence to an event
    /// that has none.
    ///
    /// Returns `None` for a chained type, because a chained event's envelope is
    /// not this shape and there is no sensible sentinel for it.
    pub fn unchained(
        event_type: EventType,
        sender_public_key: [u8; 32],
        payload: Vec<u8>,
        emitted_at_unix_ms: u64,
    ) -> Option<Self> {
        if event_type.chain_scope() != 0 {
            return None;
        }
        Some(EventBody {
            protocol_version: PROTOCOL_VERSION,
            table_id: ZERO32,
            hand_id: UNCHAINED_HAND_ID,
            sequence: 0,
            sender_public_key,
            event_type: event_type.code(),
            payload,
            previous_event_hash: ZERO32,
            emitted_at_unix_ms,
            next_deadline_ms: 0,
            chain_scope: event_type.chain_scope(),
            event_class: Self::expected_class(event_type),
        })
    }

    /// Check the envelope against the catalogue, before the payload is parsed
    /// and before any signature is checked.
    ///
    /// An unchained event must carry the sentinels of §2.3, because the table
    /// it concerns is named **in the payload** and never in the envelope. A
    /// receiver that indexed on the envelope's `table_id` would be indexing on
    /// an attacker-chosen value.
    pub fn check_envelope(&self) -> Result<EventType, EnvelopeError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(EnvelopeError::WrongVersion(self.protocol_version));
        }
        let t = self.typed().map_err(EnvelopeError::UnknownType)?;

        let expected_scope = t.chain_scope();
        if self.chain_scope != expected_scope {
            return Err(EnvelopeError::ChainScopeMismatch {
                declared: self.chain_scope,
                expected: expected_scope,
            });
        }

        if self.event_class > 2 {
            return Err(EnvelopeError::BadEventClass(self.event_class));
        }
        let expected_class = Self::expected_class(t);
        if self.event_class != expected_class {
            return Err(EnvelopeError::EventClassMismatch {
                declared: self.event_class,
                expected: expected_class,
            });
        }

        if self.chain_scope == 0 {
            if self.table_id != ZERO32 {
                return Err(EnvelopeError::UnchainedSentinelViolated("table_id"));
            }
            if self.hand_id != UNCHAINED_HAND_ID {
                return Err(EnvelopeError::UnchainedSentinelViolated("hand_id"));
            }
            if self.previous_event_hash != ZERO32 {
                return Err(EnvelopeError::UnchainedSentinelViolated("previous_event_hash"));
            }
            if self.sequence != 0 {
                return Err(EnvelopeError::UnchainedSentinelViolated("sequence"));
            }
        }

        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;


    fn chained_body(t: EventType) -> EventBody {
        EventBody {
            protocol_version: PROTOCOL_VERSION,
            table_id: [3u8; 32],
            hand_id: 7,
            sequence: 12,
            sender_public_key: [4u8; 32],
            event_type: t.code(),
            payload: vec![1, 2, 3],
            previous_event_hash: [5u8; 32],
            emitted_at_unix_ms: 1_700_000_000_000,
            next_deadline_ms: 20_000,
            chain_scope: t.chain_scope(),
            event_class: EventBody::expected_class(t),
        }
    }

    fn unchained_body(t: EventType) -> EventBody {
        EventBody {
            protocol_version: PROTOCOL_VERSION,
            table_id: ZERO32,
            hand_id: UNCHAINED_HAND_ID,
            sequence: 0,
            sender_public_key: [4u8; 32],
            event_type: t.code(),
            payload: vec![1, 2, 3],
            previous_event_hash: ZERO32,
            emitted_at_unix_ms: 1_700_000_000_000,
            next_deadline_ms: 0,
            chain_scope: 0,
            event_class: 0,
        }
    }

    #[test]
    fn an_envelope_round_trips_through_canonical_bytes() {
        use crate::protocol::serialization::{from_canonical, to_canonical};
        let body = chained_body(EventType::ActionBet);
        let bytes = to_canonical(&body).unwrap();
        let back: EventBody = from_canonical(&bytes, 8192).unwrap();
        assert_eq!(back, body);
    }

    #[test]
    fn a_signed_event_round_trips() {
        use crate::protocol::serialization::{from_canonical, to_canonical};
        let event = SignedEvent { body: vec![9; 40], signature: [1u8; 64] };
        let bytes = to_canonical(&event).unwrap();
        let back: SignedEvent = from_canonical(&bytes, 8192).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn a_well_formed_envelope_passes_for_every_event_type() {
        for t in EventType::ALL {
            let body = if t.chain_scope() == 0 { unchained_body(t) } else { chained_body(t) };
            assert_eq!(body.check_envelope(), Ok(t), "{t}");
        }
    }

    #[test]
    fn a_version_this_build_does_not_speak_is_rejected() {
        let mut body = chained_body(EventType::ActionFold);
        body.protocol_version = 2;
        assert_eq!(body.check_envelope(), Err(EnvelopeError::WrongVersion(2)));
    }

    #[test]
    fn a_declared_chain_scope_that_contradicts_the_catalogue_is_rejected() {
        let mut body = unchained_body(EventType::Hello);
        body.chain_scope = 1;
        assert_eq!(
            body.check_envelope(),
            Err(EnvelopeError::ChainScopeMismatch { declared: 1, expected: 0 })
        );
    }

    /// The sentinels matter because an unchained event names the table it
    /// concerns in its payload. A receiver that indexed on the envelope's
    /// `table_id` would be indexing on an attacker-chosen value.
    #[test]
    fn an_unchained_event_must_carry_the_sentinels() {
        type Breaker = fn(&mut EventBody);
        let cases: [(&str, Breaker); 4] = [
            ("table_id", |b| b.table_id = [1u8; 32]),
            ("hand_id", |b| b.hand_id = 0),
            ("previous_event_hash", |b| b.previous_event_hash = [1u8; 32]),
            ("sequence", |b| b.sequence = 1),
        ];
        for (field, break_it) in cases {
            let mut body = unchained_body(EventType::LobbyTableAd);
            break_it(&mut body);
            assert_eq!(
                body.check_envelope(),
                Err(EnvelopeError::UnchainedSentinelViolated(field)),
                "a bad {field} must be rejected"
            );
        }
    }

    #[test]
    fn dispute_is_unchained_and_carries_the_sentinels_too() {
        let good = unchained_body(EventType::Dispute);
        assert_eq!(good.check_envelope(), Ok(EventType::Dispute));

        let mut bad = unchained_body(EventType::Dispute);
        bad.sequence = 5;
        assert!(bad.check_envelope().is_err(), "even DISPUTE gets no sequence");
    }

    #[test]
    fn event_class_must_match_the_type_that_carries_it() {
        let mut body = chained_body(EventType::ActionCall);
        body.event_class = 1;
        assert_eq!(
            body.check_envelope(),
            Err(EnvelopeError::EventClassMismatch { declared: 1, expected: 0 })
        );

        let mut vote = chained_body(EventType::TimeoutVote);
        assert_eq!(vote.event_class, 1, "a vote declares class 1");
        vote.event_class = 0;
        assert_eq!(
            vote.check_envelope(),
            Err(EnvelopeError::EventClassMismatch { declared: 0, expected: 1 })
        );
    }

    #[test]
    fn an_out_of_range_event_class_is_rejected() {
        for class in [3u8, 4, 255] {
            let mut body = chained_body(EventType::ActionCheck);
            body.event_class = class;
            assert_eq!(body.check_envelope(), Err(EnvelopeError::BadEventClass(class)));
        }
    }

    #[test]
    fn an_uncatalogued_type_is_rejected_at_the_envelope() {
        let mut body = chained_body(EventType::ActionCheck);
        body.event_type = 0x1234;
        assert_eq!(
            body.check_envelope(),
            Err(EnvelopeError::UnknownType(UnknownEventType::Unknown(0x1234)))
        );
    }

    #[test]
    fn the_catalogue_has_exactly_thirty_nine_types() {
        assert_eq!(EventType::ALL.len(), 43, "PROTOCOL.md section 4.11 lists 43");
    }

    #[test]
    fn every_code_is_distinct_and_every_name_is_distinct() {
        let codes: BTreeSet<u16> = EventType::ALL.iter().map(|t| t.code()).collect();
        assert_eq!(codes.len(), 43, "two types share a wire code");
        let names: BTreeSet<&str> = EventType::ALL.iter().map(|t| t.name()).collect();
        assert_eq!(names.len(), 43, "two types share a name");
    }

    #[test]
    fn every_code_round_trips_through_the_wire_representation() {
        for expected in EventType::ALL {
            let decoded = EventType::try_from(expected.code()).expect("a catalogued code");
            assert_eq!(decoded, expected, "0x{:04x}", expected.code());
        }
    }

    /// Section 17 assumes arbitrary bytes, so an unknown code is a network
    /// condition and must be a rejection rather than a forgotten match arm.
    #[test]
    fn an_uncatalogued_code_is_rejected() {
        for code in [0x0000u16, 0x0003, 0x0109, 0x0206, 0x0308, 0x0506, 0x0704, 0x0806, 0x1234] {
            assert_eq!(
                EventType::try_from(code),
                Err(UnknownEventType::Unknown(code)),
                "0x{code:04x} is not in the catalogue"
            );
        }
    }

    #[test]
    fn the_reserved_range_is_refused_explicitly() {
        for code in [0xF000u16, 0xF001, 0xFABC, 0xFFFF] {
            assert_eq!(
                EventType::try_from(code),
                Err(UnknownEventType::Reserved(code)),
                "0x{code:04x} is held back for future versions"
            );
        }
        assert!(!EventType::RESERVED.contains(&0xEFFF));
    }

    /// `PROTOCOL.md` §2.3 has an exhaustive list of thirteen types with
    /// `chain_scope = 0`. A fourteenth would mean a message occupying no slot
    /// that everyone assumed was chained.
    ///
    /// Twelve of them are `Unchained`; `DISPUTE` is the thirteenth and is
    /// `OutOfStage` — chained in nothing, but a table-mesh message rather than
    /// a lobby or join one.
    #[test]
    fn exactly_thirteen_types_have_no_chain_scope() {
        let unchained: Vec<EventType> = EventType::ALL
            .into_iter()
            .filter(|t| t.chain_scope() == 0)
            .collect();
        assert_eq!(unchained.len(), 15, "got {unchained:?}");

        for t in unchained {
            assert!(!t.is_chained(), "{t}");
            assert!(
                matches!(t.stage_kind(), StageKind::Unchained | StageKind::OutOfStage),
                "{t} has chain_scope 0 but stage kind {:?}",
                t.stage_kind()
            );
        }
    }

    /// Five table-mesh messages carry no chain scope, not one.
    ///
    /// `PROTOCOL.md` prose says *"`DISPUTE` is the one table-mesh message with
    /// `chain_scope = 0`"*, and its own §4.11 table contradicts that: `HELLO`,
    /// `CAPABILITIES`, `PLAYER_LIST` and, since `S1-CS`, `TABLE_CHAT` are all
    /// listed as table mesh with `chain_scope = 0`. The table is the normative
    /// data, so the code follows the table; the prose is filed against
    /// `PROTOCOL.md`'s owner.
    ///
    /// It looks small, but the `DISPUTE` exception has already produced two
    /// defects, and a receiver written from the prose would look for a chain
    /// position on a `HELLO`.
    #[test]
    fn four_table_mesh_types_carry_no_chain_scope() {
        let odd: Vec<EventType> = EventType::ALL
            .into_iter()
            .filter(|t| t.channel() == Channel::TableMesh && t.chain_scope() == 0)
            .collect();
        assert_eq!(
            odd,
            vec![
                EventType::Hello,
                EventType::Capabilities,
                EventType::TableChat,
                EventType::TableLeave,
                EventType::PlayerList,
                EventType::Dispute
            ],
            "the set of unchained table-mesh types changed"
        );
        // Only DISPUTE is out-of-stage; the other three are simply unchained.
        assert_eq!(EventType::Dispute.stage_kind(), StageKind::OutOfStage);
        for t in [EventType::Hello, EventType::Capabilities, EventType::PlayerList] {
            assert_eq!(t.stage_kind(), StageKind::Unchained, "{t}");
        }
    }

    #[test]
    fn only_the_abort_closes_a_chain_on_its_own() {
        let terminal: Vec<EventType> = EventType::ALL
            .into_iter()
            .filter(|t| t.stage_kind() == StageKind::WitnessIndependentTerminal)
            .collect();
        assert_eq!(terminal, vec![EventType::HandAbort]);
    }

    #[test]
    fn the_three_boundary_events_are_single_writer_and_chained() {
        let boundary: Vec<EventType> =
            EventType::ALL.into_iter().filter(|t| t.is_boundary()).collect();
        assert_eq!(
            boundary,
            vec![EventType::PlayerSitOut, EventType::PlayerSitIn, EventType::PlayerLeave]
        );
        for t in boundary {
            assert_eq!(t.stage_kind(), StageKind::Single, "{t}");
            assert!(t.is_chained(), "{t}");
            assert_eq!(t.channel(), Channel::TableMesh, "{t}");
        }
    }

    /// Every unchained type is on a lobby, join or connection channel, and
    /// every chained one is on the table mesh — except `DISPUTE`, checked above.
    #[test]
    fn chained_and_unchained_split_along_the_channels() {
        for t in EventType::ALL {
            if t.is_chained() {
                assert_eq!(t.channel(), Channel::TableMesh, "{t} is chained but off-mesh");
            }
        }
    }

    #[test]
    fn every_type_has_a_stage_kind_consistent_with_its_chain_scope() {
        for t in EventType::ALL {
            match t.stage_kind() {
                StageKind::Unchained => assert_eq!(t.chain_scope(), 0, "{t}"),
                // DISPUTE is chain_scope 0 but out-of-stage rather than
                // unchained: it is a table-mesh message with no slot.
                StageKind::OutOfStage => assert_eq!(t.chain_scope(), 0, "{t}"),
                _ => assert_eq!(t.chain_scope(), 1, "{t}"),
            }
        }
    }

    #[test]
    fn the_five_betting_actions_are_single_writer() {
        for t in [
            EventType::ActionCheck,
            EventType::ActionCall,
            EventType::ActionBet,
            EventType::ActionRaise,
            EventType::ActionFold,
        ] {
            assert_eq!(t.stage_kind(), StageKind::Single, "{t}");
            assert!(t.is_chained(), "{t}");
        }
    }
}
