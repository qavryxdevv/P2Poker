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
    pub const ALL: [EventType; 39] = [
        EventType::Hello,
        EventType::Capabilities,
        EventType::LobbyTableAd,
        EventType::LobbyTableRemove,
        EventType::LobbyPlayerPresence,
        EventType::LobbySnapshotRequest,
        EventType::LobbySnapshotResponse,
        EventType::LobbyChat,
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
    /// Thirteen types have no chain scope. Four of them ride the table mesh -
    /// `HELLO`, `CAPABILITIES`, `PLAYER_LIST` and `DISPUTE` - which contradicts
    /// `PROTOCOL.md`'s prose claim that `DISPUTE` is the only one. The §4.11
    /// table is the normative data and this follows it.
    pub const fn chain_scope(self) -> u8 {
        use EventType::*;
        match self {
            Hello | Capabilities | LobbyTableAd | LobbyTableRemove | LobbyPlayerPresence
            | LobbySnapshotRequest | LobbySnapshotResponse | LobbyChat | JoinRequest
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
            | LobbySnapshotRequest | LobbySnapshotResponse | LobbyChat | JoinRequest
            | JoinAccept | JoinReject | PlayerList => StageKind::Unchained,

            // Chained, but legal outside its stage.
            Dispute => StageKind::OutOfStage,

            // Closes a chain alone, from the genesis, so peers with different
            // prefixes reach the same terminal value.
            HandAbort => StageKind::WitnessIndependentTerminal,

            // One named seat writes it.
            ShuffleStep | ShuffleProof | ActionCheck | ActionCall | ActionBet | ActionRaise
            | ActionFold | TimeoutVote | PlayerSitOut | PlayerSitIn | PlayerLeave => {
                StageKind::Single
            }

            // Every required emitter produces a byte-identical body.
            TableReady | RngCommit | RngReveal | HandInit | DeckInit | DeckCommit | DealPrivate
            | BoardReveal | ShowdownReveal | ShowdownMuck | TimeoutCert | StateHash | StateAck
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn the_catalogue_has_exactly_thirty_nine_types() {
        assert_eq!(EventType::ALL.len(), 39, "PROTOCOL.md section 4.11 lists 39");
    }

    #[test]
    fn every_code_is_distinct_and_every_name_is_distinct() {
        let codes: BTreeSet<u16> = EventType::ALL.iter().map(|t| t.code()).collect();
        assert_eq!(codes.len(), 39, "two types share a wire code");
        let names: BTreeSet<&str> = EventType::ALL.iter().map(|t| t.name()).collect();
        assert_eq!(names.len(), 39, "two types share a name");
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
        for code in [0x0000u16, 0x0003, 0x0107, 0x0206, 0x0308, 0x0506, 0x0704, 0x0806, 0x1234] {
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
        assert_eq!(unchained.len(), 13, "got {unchained:?}");

        for t in unchained {
            assert!(!t.is_chained(), "{t}");
            assert!(
                matches!(t.stage_kind(), StageKind::Unchained | StageKind::OutOfStage),
                "{t} has chain_scope 0 but stage kind {:?}",
                t.stage_kind()
            );
        }
    }

    /// Four table-mesh messages carry no chain scope, not one.
    ///
    /// `PROTOCOL.md` prose says *"`DISPUTE` is the one table-mesh message with
    /// `chain_scope = 0`"*, and its own §4.11 table contradicts that four times
    /// over: `HELLO`, `CAPABILITIES` and `PLAYER_LIST` are all listed as table
    /// mesh with `chain_scope = 0`. The table is the normative data, so the code
    /// follows the table; the prose is filed against `PROTOCOL.md`'s owner.
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
