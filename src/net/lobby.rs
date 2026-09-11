//! The lobby: signed table advertisements, and what a receiver does with one.
//!
//! `PROTOCOL.md` §7. This is the half of discovery that answers **what tables
//! are open**; the Kademlia provider records in [`super::run`] answer *where to
//! try*, and neither does the other's job.
//!
//! # The admission rules are the joiner's, and that is the point
//!
//! Every value in an advert is the **founder's**, signed into
//! `table_params_hash`. A founder who wants a table where nothing can ever be
//! won needs no attack — only a small number in one field. So the checks below
//! run at the joiner, before the advert is shown to a user or stored, and a
//! client that joins anyway and applies its own bound locally has made two peers
//! disagree about whether a hand aborted, which §8.2 classes as a consensus
//! fault.
//!
//! # Rule 3, and why an unknown preset name is refused rather than ignored
//!
//! `preset_id` is a **closed two-value enum**. `RATED_SNG_POKERTH_V1` asserts
//! §13's exact values or the advert is a lie about what game is being offered;
//! `CUSTOM` asserts nothing and carries every value in its own fields.
//!
//! **Any third value is rejected on sight, whether or not this client recognises
//! the name.** A receiver that accepts an unknown name has accepted a table
//! whose identity it cannot check: the name asserts values by this very rule,
//! the advert asserts values in its fields, and nothing says the two agree. Two
//! clients shipping different tables under one name is a defect this project has
//! already produced twice — `hand_deadline_ms` alone is signed into
//! `table_params_hash`, so they cannot join each other and neither can say why.
//!
//! # Rule 7, and the difference between a stale advert and a changed table
//!
//! Rule 6 compares two adverts on their timestamp and on nothing else, which
//! blunts replay. It does not stop a founder re-signing with a different
//! `small_blind` and handing two joiners two **rule sets** — and that forks
//! `HAND_INIT`, a collective stage whose bodies must be byte-identical.
//!
//! So an advert whose parameters changed under a live one is discarded **and the
//! table is marked unjoinable**. A founder who wants to change the game forms a
//! new table under a new key, which is what changing the game means.

use std::collections::BTreeMap;

use crate::poker::state::Hash;
use crate::protocol::constants::{
    hand_deadline_min_ms, PresetId, AD_TTL_MS, HAND_DEADLINE_CAP_MS, MAX_ADS_PER_PEER_PER_MIN,
    MAX_ADS_PER_TABLE_KEY_PER_MIN, MAX_AD_LIFETIME_MS, MAX_CLOCK_SKEW_MS, MAX_SEATS,
    sng_hand_deadline_ms, sng_small_blind_cap, MAX_TRACKED_TABLES, RATED_BLIND_EVERY_N_HANDS,
    RATED_HAND_DEADLINE_MS, RATED_SEATS, RATED_SMALL_BLIND, RATED_SMALL_BLIND_CAP,
    RATED_START_STACK,
};

/// The deck suite version 1 speaks, and the only one.
pub const DECK_SUITE_V1: &str = "bs-bg12-secp256k1/1";

/// What kind of game an advert offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    CashPlayMoney,
    TournamentSngPlayMoney,
}

impl Mode {
    pub const fn code(self) -> u16 {
        match self {
            Mode::CashPlayMoney => 1,
            Mode::TournamentSngPlayMoney => 2,
        }
    }

    pub fn parse(code: u16) -> Option<Mode> {
        match code {
            1 => Some(Mode::CashPlayMoney),
            2 => Some(Mode::TournamentSngPlayMoney),
            _ => None,
        }
    }

    /// A tournament pays every entrant the same stack, so the buy-in **is** the
    /// stack.
    pub const fn is_tournament(self) -> bool {
        matches!(self, Mode::TournamentSngPlayMoney)
    }
}

/// §7.2's blind schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlindSchedule {
    /// `1` = `DOUBLE_EVERY_N_HANDS`, and nothing else in version 1.
    pub mode: u16,
    pub every_n_hands: u16,
    pub first_small_blind: u64,
    pub small_blind_cap: u64,
}

/// A table advertisement, as §7.2 defines it.
///
/// The envelope's `table_id` is the unchained sentinel; the advert's identity is
/// its **signing key**, which is the table's public key. That is why there is no
/// `table_id` field here: an advert that carried one could disagree with the key
/// that signed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableAd {
    pub game: u16,
    pub mode: u16,
    pub preset_id: String,
    pub table_name: String,
    pub small_blind: u64,
    pub big_blind: u64,
    pub ante: u64,
    pub min_buyin: u64,
    pub max_buyin: u64,
    pub start_stack: u64,
    pub players: u8,
    pub max_players: u8,
    pub min_players_to_start: u8,
    pub blind_schedule: BlindSchedule,
    pub action_timeout_ms: u32,
    pub action_grace_ms: u32,
    pub crypto_step_timeout_ms: u32,
    pub hand_deadline_ms: u32,
    pub join_deadline_ms: u32,
    pub hand_delay_ms: u32,
    /// The per-hand thinking reserve, on top of `action_timeout_ms`. A table
    /// parameter and inside `table_params_hash`: every peer adds it to a
    /// betting stage's deadline before it will vote that a seat is late.
    pub time_bank_ms: u32,
    pub button_rule: u16,
    pub odd_chip_rule: u16,
    pub showdown_policy: u16,
    pub password_required: bool,
    pub deck_suite: String,
    pub founder_app_key: [u8; 32],
    pub founder_peer_id: Vec<u8>,
    pub timestamp_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    /// The founder's **Tox** public key: how a joiner is reached so that an
    /// invitation into the table's group is possible at all (D-019). `None`
    /// means this table's traffic is not on Tox.
    ///
    /// Outside `table_params_hash`, like `founder_peer_id` and for the same
    /// reason — identity and routing. See `advert::AdBody` for the full
    /// argument and for why hashing it would make a restarted founder's table
    /// permanently unjoinable.
    pub founder_tox_key: Option<[u8; 32]>,
    /// The chat id of the group that carries this table.
    ///
    /// **Not how anybody joins** — members arrive by invitation, because
    /// joining by chat id goes through Tox's DHT and that path decays. It is
    /// what a joiner compares the group it was invited into against, so that a
    /// founder cannot quietly put the table somewhere nobody advertised.
    pub tox_chat_id: Option<[u8; 32]>,
}

/// Why an advert was not admitted.
///
/// Each variant names the §7.2 rule it comes from, because the rules are cited
/// by number across the corpus and a message that says only "invalid" costs the
/// next reader the walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdRejected {
    /// Rule 2: a numeric range, `big_blind == 2 * small_blind`, the buy-in
    /// ordering, the seat bounds, or the tournament buy-in identity.
    Range(&'static str),
    /// Rule 2a: below the **derived** whole-hand deadline minimum.
    ///
    /// The one range check that is not a literal. Below the floor, every legal
    /// hand at this table aborts on its own deadline with nobody named; between
    /// the floor and the minimum the table buys the walk and no reopening, so the
    /// **first re-raise** reaches that same abort.
    DeadlineTooShort { given: u32, minimum: u64 },
    /// Rule 3: a `preset_id` that is neither of the two, or a named preset whose
    /// values are not the preset's.
    Preset(&'static str),
    /// Rule 4: a deck suite this client does not speak.
    UnknownDeckSuite,
    /// Rule 5: an expiry beyond the lifetime cap, or a timestamp too far ahead.
    Timing(&'static str),
    /// The name or another display field is not something this client will show.
    Display(&'static str),
}

/// §7.2 rules 2, 2a, 3, 4 and 5.
///
/// Rule 1 — `verify_strict` under the sending key — belongs to the signature
/// layer and has run before this is called. Rules 6 and 7 need the advert this
/// client already holds and are [`LobbyStore`]'s.
pub fn admit(ad: &TableAd, now_unix_ms: u64) -> Result<(), AdRejected> {
    // ---- rule 3 first, because it decides what the rest must equal ---------
    let preset = PresetId::parse(&ad.preset_id).ok_or(AdRejected::Preset(
        "preset_id is a closed two-value enum; a third name is rejected on sight",
    ))?;

    // ---- rule 4 ------------------------------------------------------------
    if ad.deck_suite != DECK_SUITE_V1 {
        return Err(AdRejected::UnknownDeckSuite);
    }

    // ---- display bounds ----------------------------------------------------
    if ad.table_name.len() > 64 {
        return Err(AdRejected::Display("table_name is over 64 bytes"));
    }
    if ad.table_name.chars().any(|c| c.is_control()) {
        return Err(AdRejected::Display("table_name holds a control character"));
    }
    if ad.founder_peer_id.len() > 42 {
        return Err(AdRejected::Display("founder_peer_id is over 42 bytes"));
    }

    // ---- rule 2 ------------------------------------------------------------
    let mode = Mode::parse(ad.mode).ok_or(AdRejected::Range("mode is not a version 1 mode"))?;
    if ad.game != 1 {
        return Err(AdRejected::Range("game is not NLHE"));
    }
    if ad.small_blind < 1 {
        return Err(AdRejected::Range("small_blind is below 1"));
    }
    if ad.big_blind != 2 * ad.small_blind {
        return Err(AdRejected::Range("big_blind is not twice small_blind"));
    }
    if ad.ante != 0 {
        return Err(AdRejected::Range("ante is not 0 in version 1"));
    }
    if ad.small_blind != ad.blind_schedule.first_small_blind {
        return Err(AdRejected::Range(
            "small_blind is not the schedule's first level",
        ));
    }
    if ad.min_buyin < ad.big_blind {
        return Err(AdRejected::Range("min_buyin is below one big blind"));
    }
    if ad.max_buyin < ad.min_buyin {
        return Err(AdRejected::Range("max_buyin is below min_buyin"));
    }
    if !(2..=MAX_SEATS).contains(&ad.max_players) {
        return Err(AdRejected::Range("max_players is outside 2 to 10"));
    }
    if ad.min_players_to_start < 2 || ad.min_players_to_start > ad.max_players {
        return Err(AdRejected::Range(
            "min_players_to_start is outside 2 to max_players",
        ));
    }
    if ad.players > ad.max_players {
        return Err(AdRejected::Range("players exceeds max_players"));
    }
    if mode.is_tournament() {
        // A tournament pays every entrant the same stack, so the buy-in is the
        // stack. Without this the two buy-in fields are free parts of
        // `table_params_hash` that a named configuration has to pin one at a
        // time, which is how `G7-S3` happened.
        if ad.min_buyin != ad.start_stack || ad.max_buyin != ad.start_stack {
            return Err(AdRejected::Range(
                "a tournament buy-in is the start stack, both bounds",
            ));
        }
    } else if ad.start_stack != 0 {
        return Err(AdRejected::Range("start_stack is set on a cash table"));
    }
    if !(5_000..=300_000).contains(&ad.action_timeout_ms) {
        return Err(AdRejected::Range("action_timeout_ms is outside 5s to 300s"));
    }
    if ad.action_grace_ms > 30_000 {
        return Err(AdRejected::Range("action_grace_ms is over 30s"));
    }
    // **Floored at the carrier, not at one second.** `S1-BK`: this admitted
    // 1_000, and a stage that must close in a second cannot survive a carrier
    // whose blind repair ladder reaches T+33 — every seat whose datagram the
    // wire refused is voted out for a message that was on its way. The ceiling
    // is the carrier's own patience: past `CARRIER_GIVES_UP_MS` the peer is no
    // longer in the group, so a longer budget waits for nobody.
    if !(crate::protocol::constants::CRYPTO_STEP_MIN_MS
        ..=crate::protocol::constants::CARRIER_GIVES_UP_MS)
        .contains(&ad.crypto_step_timeout_ms)
    {
        return Err(AdRejected::Range(
            "crypto_step_timeout_ms is outside the carrier's repair window",
        ));
    }
    if ad.join_deadline_ms as u64 > HAND_DEADLINE_CAP_MS {
        return Err(AdRejected::Range("join_deadline_ms is over an hour"));
    }
    if ad.hand_delay_ms > 60_000 {
        return Err(AdRejected::Range("hand_delay_ms is over 60s"));
    }
    if ad.button_rule != 1 {
        return Err(AdRejected::Range("button_rule is not DEAD_BUTTON"));
    }
    if ad.odd_chip_rule != 1 {
        return Err(AdRejected::Range(
            "odd_chip_rule is not FIRST_SEAT_LEFT_OF_BUTTON",
        ));
    }
    if !(1..=2).contains(&ad.showdown_policy) {
        return Err(AdRejected::Range("showdown_policy is not 1 or 2"));
    }
    if ad.blind_schedule.mode != 1 {
        return Err(AdRejected::Range(
            "blind schedule mode is not DOUBLE_EVERY_N_HANDS",
        ));
    }
    if ad.blind_schedule.small_blind_cap < ad.blind_schedule.first_small_blind {
        return Err(AdRejected::Range("the blind cap is below the first level"));
    }

    // ---- rule 2a: the derived bound ---------------------------------------
    if ad.hand_deadline_ms as u64 > HAND_DEADLINE_CAP_MS {
        return Err(AdRejected::Range("hand_deadline_ms is over an hour"));
    }
    let minimum = hand_deadline_min_ms(
        ad.max_players,
        ad.action_timeout_ms as u64,
        ad.action_grace_ms as u64,
        ad.crypto_step_timeout_ms as u64,
        ad.hand_delay_ms as u64,
        ad.time_bank_ms as u64,
    );
    if (ad.hand_deadline_ms as u64) < minimum {
        return Err(AdRejected::DeadlineTooShort {
            given: ad.hand_deadline_ms,
            minimum,
        });
    }

    // ---- rule 3's second half: a name asserts values -----------------------
    if preset == PresetId::RatedSngPokerthV1 {
        rated_values_match(ad)?;
    }

    // ---- rule 5 ------------------------------------------------------------
    if ad.expires_at_unix_ms <= ad.timestamp_unix_ms {
        return Err(AdRejected::Timing("expires_at is not after timestamp"));
    }
    if ad.expires_at_unix_ms > now_unix_ms.saturating_add(MAX_AD_LIFETIME_MS) {
        // Without this bound a malicious peer pins a table into every lobby
        // forever, which is free spam.
        return Err(AdRejected::Timing(
            "expires_at is more than the lifetime cap ahead of local time",
        ));
    }
    // **And the other end, which was open.** Only the future was bounded, so an
    // advert that had already expired was admitted — anybody who had ever seen
    // one could re-inject it and put a dead table back into every lobby it
    // reached, with no key and no signature of their own. `expire` drops it on
    // the next sweep using this exact comparison, so admitting it was taking in
    // something already known to be rubbish. Same predicate, one moment
    // earlier, and the clock is the same local view in both places (D-012).
    if ad.expires_at_unix_ms <= now_unix_ms {
        return Err(AdRejected::Timing("expires_at has already passed"));
    }
    if ad.timestamp_unix_ms > now_unix_ms.saturating_add(MAX_CLOCK_SKEW_MS) {
        return Err(AdRejected::Timing("timestamp is too far in the future"));
    }

    Ok(())
}

/// The named preset asserts §13's values, so they are checked.
///
/// A preset name that does not carry the preset's values is a lie about what
/// game is being offered, and it is the failure this project has shipped twice.
fn rated_values_match(ad: &TableAd) -> Result<(), AdRejected> {
    // Every part of `table_params_hash` that §13 pins and rule 2 leaves free.
    //
    // **Twenty-five parts, and this list plus rule 2 must cover all of them.**
    // Two did not: `join_deadline_ms` (n(18)) and `showdown_policy` (n(22)) are
    // both parts of the hash, both fixed by §13's block, and rule 2 gives each
    // only a range — so two clients both correctly implementing this preset
    // could pick 60 000 and 120 000, or policy 1 and policy 2, compute two
    // different `table_params_hash` values, and be unable to join each other's
    // table with **neither of them wrong**. That is `G7-S3` exactly, a fourth
    // time, and this time in the code rather than in the document.
    //
    // The audit is the same one §13 prescribes for itself: diff this list
    // against §3.1's twenty-five parts and require every part to be pinned
    // either here or by rule 2.
    // `every_part_of_the_hash_is_pinned_by_the_rated_name` does it.
    let expected: [(&'static str, u64, u64); 13] = [
        ("mode", ad.mode as u64, Mode::TournamentSngPlayMoney.code() as u64),
        ("max_players", ad.max_players as u64, RATED_SEATS as u64),
        (
            "min_players_to_start",
            ad.min_players_to_start as u64,
            RATED_SEATS as u64,
        ),
        ("start_stack", ad.start_stack, RATED_START_STACK),
        ("small_blind", ad.small_blind, RATED_SMALL_BLIND),
        ("ante", ad.ante, 0),
        (
            "every_n_hands",
            ad.blind_schedule.every_n_hands as u64,
            RATED_BLIND_EVERY_N_HANDS as u64,
        ),
        (
            "small_blind_cap",
            ad.blind_schedule.small_blind_cap,
            RATED_SMALL_BLIND_CAP,
        ),
        ("action_timeout_ms", ad.action_timeout_ms as u64, 20_000),
        ("action_grace_ms", ad.action_grace_ms as u64, 5_000),
        (
            "hand_deadline_ms",
            ad.hand_deadline_ms as u64,
            RATED_HAND_DEADLINE_MS,
        ),
        ("join_deadline_ms", ad.join_deadline_ms as u64, 120_000),
        ("showdown_policy", ad.showdown_policy as u64, 1),
    ];
    for (name, got, want) in expected {
        if got != want {
            return Err(AdRejected::Preset(match name {
                "mode" => "the rated preset is a tournament",
                "max_players" => "the rated preset seats ten",
                "min_players_to_start" => "the rated preset starts at ten",
                "start_stack" => "the rated preset starts every seat with 10 000",
                "small_blind" => "the rated preset opens at 50",
                "ante" => "the rated preset has no ante",
                "every_n_hands" => "the rated preset raises every 11 hands",
                "small_blind_cap" => "the rated preset caps the small blind at 50 000",
                "action_timeout_ms" => "the rated preset gives 20 s to act",
                "action_grace_ms" => "the rated preset gives 5 s of grace",
                "join_deadline_ms" => "the rated preset gives 120 s to form",
                "showdown_policy" => "the rated preset reveals at showdown",
                _ => "the rated preset's whole-hand deadline is 3 300 000 ms",
            }));
        }
    }
    if ad.crypto_step_timeout_ms != 30_000 {
        return Err(AdRejected::Preset(
            "the rated preset gives 30 s to a cryptographic step",
        ));
    }
    if ad.hand_delay_ms != 7_000 {
        return Err(AdRejected::Preset("the rated preset pauses 7 s between hands"));
    }
    // §13: `n(23) password_required = false`, stated there only so its absence
    // is not read as an omission — and enforced nowhere until now. PokerTH
    // refuses a ranked game with a password for the same reason: a rated table
    // is one anybody may sit down at, and a password is the opposite of that.
    //
    // **This is the only rated check on a field that is not part of
    // `table_params_hash`**, so unlike the other thirteen it cannot make two
    // honest clients derive different digests. It is a policy check, and it is
    // here because a password-gated table calling itself rated is a claim about
    // what it is that happens not to be true.
    if ad.password_required {
        return Err(AdRejected::Preset("a rated table has no password"));
    }
    Ok(())
}

/// What kind of game a founder is starting.
///
/// A client concept, not a protocol one: the protocol has a `mode` and a
/// `preset_id`, and this is the choice a person makes that settles both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableKind {
    /// Everybody starts together with the same stack, and it deals when the
    /// table is full.
    SitAndGo,
    /// Deals with two, and people arrive and leave between hands.
    Cash,
}

impl TableAd {
    /// A Sit-and-Go of `seats` seats, with the rated structure.
    ///
    /// **Ten seats is `RATED_SNG_POKERTH_V1`; anything else is `CUSTOM`.** That
    /// is not a limitation of this function, it is what a preset name means:
    /// §7.2 rule 3 says the name asserts §13's exact values, and §13 says ten.
    /// A six-handed table carrying the rated name would be refused by every
    /// receiver — including this client — so it carries `CUSTOM` and its own
    /// numbers instead, which is exactly what `CUSTOM` is for.
    ///
    /// Everything else is the rated structure: 10 000 chips, blinds 50/100
    /// doubling every eleven hands, and it deals when every seat is full. The
    /// two values that depend on the seat count — the blind cap and the
    /// whole-hand deadline — are derived by §13's own formulae rather than
    /// invented, so a nine-handed Sit-and-Go is the same game with one fewer
    /// chair.
    pub fn sng(
        seats: u8,
        table_name: String,
        founder_app_key: [u8; 32],
        founder_peer_id: Vec<u8>,
        now_ms: u64,
    ) -> TableAd {
        let seats = seats.clamp(2, MAX_SEATS);
        TableAd {
            game: 1,
            mode: Mode::TournamentSngPlayMoney.code(),
            preset_id: if seats == RATED_SEATS {
                PresetId::RatedSngPokerthV1.as_str().into()
            } else {
                PresetId::Custom.as_str().into()
            },
            table_name,
            small_blind: RATED_SMALL_BLIND,
            big_blind: RATED_SMALL_BLIND * 2,
            ante: 0,
            // A Sit-and-Go's buy-in **is** its starting stack: every entrant
            // gets an equal stack, so the two bounds and the stack are one
            // number. Rule 2 enforces it in tournament modes.
            min_buyin: RATED_START_STACK,
            max_buyin: RATED_START_STACK,
            start_stack: RATED_START_STACK,
            players: 1,
            max_players: seats,
            // Full, and not before. That is what makes it a Sit-and-Go rather
            // than a cash table that happens to pay equal stacks.
            min_players_to_start: seats,
            blind_schedule: BlindSchedule {
                mode: 1,
                every_n_hands: RATED_BLIND_EVERY_N_HANDS,
                first_small_blind: RATED_SMALL_BLIND,
                small_blind_cap: sng_small_blind_cap(seats),
            },
            // `D-034`: thirty seconds to decide, three for the network, no
            // reserve -- on every table this client hosts. The rated preset
            // alone keeps §13's numbers; this constructor served both, and
            // the owner's Sit & Go tables were dealt 20 s plus a 30 s reserve,
            // so the first slow decision took 50 s at every seat (S1-DM,
            // `split201510-3`).
            action_timeout_ms: if seats == RATED_SEATS { 20_000 } else { crate::protocol::constants::DECISION_MS },
            action_grace_ms: if seats == RATED_SEATS { 5_000 } else { crate::protocol::constants::DECISION_GRACE_MS },
            crypto_step_timeout_ms: 30_000,
            hand_deadline_ms: sng_hand_deadline_ms(seats) as u32,
            join_deadline_ms: 120_000,
            hand_delay_ms: 7_000,
            time_bank_ms: if seats == RATED_SEATS { crate::protocol::constants::default_time_bank_ms(seats) } else { 0 },
            button_rule: 1,
            odd_chip_rule: 1,
            showdown_policy: 1,
            password_required: false,
            deck_suite: DECK_SUITE_V1.into(),
            founder_app_key,
            founder_peer_id,
            timestamp_unix_ms: now_ms,
            expires_at_unix_ms: now_ms + AD_TTL_MS,
            // Filled by [`TableAd::on_tox`] once the group exists. It cannot be
            // filled here: the table is decided before the group is created,
            // and a builder that took a chat id would have to be handed one
            // that does not exist yet.
            founder_tox_key: None,
            tox_chat_id: None,
        }
    }

    /// Say that this table's traffic rides a Tox group (D-019).
    ///
    /// Called after the group is created, on the advert about to be published.
    /// Neither field enters `table_params_hash`, so adding them does not make
    /// this a different table — which is what lets the founder advertise first
    /// and say where the traffic is a moment later, and what lets a restarted
    /// founder come back on a new group without every client marking the table
    /// permanently unjoinable.
    #[must_use]
    pub fn on_tox(mut self, founder_tox_key: [u8; 32], chat_id: [u8; 32]) -> Self {
        self.founder_tox_key = Some(founder_tox_key);
        self.tox_chat_id = Some(chat_id);
        self
    }

    /// The rated Sit-and-Go, exactly as `PROTOCOL.md` §13 fixes it.
    ///
    /// Every one of `table_params_hash`'s twenty-five parts is settled here and
    /// **none of them is a choice**: a `preset_id` is a claim about values, and
    /// §7.2 rule 3 rejects an advert carrying this name with any other value in
    /// it. A founder who wants different numbers wants a `CUSTOM` table.
    ///
    /// The four arguments are the four things §13 does not pin, and none of them
    /// is a part of the hash: the name is display data, the founder's key and
    /// peer id are identity and routing, and the timestamps are what make the
    /// advertisement fresh rather than what make the game.
    ///
    /// # What "rated" buys, and what it does not
    ///
    /// It buys the thing a lobby full of strangers needs: **every client derives
    /// the same `table_params_hash` from the same name**, so two players who have
    /// never spoken agree on the game before either sits down. It does not buy a
    /// ranking, a ladder or a record — there is no such thing in this protocol
    /// and the name is inherited from the configuration it copies.
    pub fn rated_sng(
        table_name: String,
        founder_app_key: [u8; 32],
        founder_peer_id: Vec<u8>,
        now_ms: u64,
    ) -> TableAd {
        // The general one at ten seats, and there is a test that the two agree
        // field for field. Two constructors that were supposed to produce the
        // same table would be two places for it to change.
        TableAd::sng(
            RATED_SEATS,
            table_name,
            founder_app_key,
            founder_peer_id,
            now_ms,
        )
    }
}

/// One advertisement this client is holding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    pub ad: TableAd,
    /// The parameter hash rule 7 compares against.
    pub params_hash: Hash,
    /// The `event_hash` of the advertisement this record came from.
    ///
    /// Kept because **a joiner cannot build a `JOIN_REQUEST` without it**:
    /// §4.3's `n(0) advert_hash` names the advert being joined, and it is the
    /// hash of bytes the store had already thrown away. The lobby was complete
    /// as a list of tables and unusable as a way to sit down at one, which is
    /// the kind of gap that only appears when the two halves are finally joined.
    ///
    /// Unlike `params_hash` this is **per re-broadcast** — every re-broadcast
    /// carries a strictly greater timestamp and therefore hashes differently —
    /// so it is the hash of the copy this client accepted and of no other. That
    /// is exactly right for `JOIN_REQUEST`, which the founder answers by looking
    /// up an advert it signed, and exactly wrong for anything that must be
    /// agreed between joiners, which is why D-013 put `table_params_hash` in
    /// those places instead.
    pub advert_hash: Hash,
    /// When this client accepted it, by its own clock. A local view, never
    /// canonical state.
    pub received_at_ms: u64,
    /// The founder changed the game under a live advert. The table stays visible
    /// and is not joinable.
    pub unjoinable: bool,
}

/// Why a re-broadcast was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotTaken {
    /// Rule 6: not newer than the advert already held.
    NotNewer,
    /// Rule 7: the parameters changed under a live advert. The table is now
    /// marked unjoinable, and this is not a rejection of the advert so much as a
    /// finding about the table.
    ParametersChanged,
    /// The lobby is full. A bound, because the advert stream is open to
    /// strangers.
    LobbyFull,
}

/// One row of the lobby, as a caller reads it.
///
/// A named pair rather than a tuple, because the key is the table's identity and
/// a caller that had to remember which half of a tuple that was would eventually
/// get it wrong.
#[derive(Debug, Clone, Copy)]
pub struct Listing<'a> {
    pub key: &'a [u8; 32],
    pub held: &'a Held,
}

/// The table adverts this client holds.
#[derive(Debug, Clone, Default)]
pub struct LobbyStore {
    tables: BTreeMap<[u8; 32], Held>,
}

impl LobbyStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take an advert that has passed [`admit`] and its signature check.
    ///
    /// `table_key` is the key that signed it, which **is** the table's identity.
    pub fn offer(
        &mut self,
        table_key: [u8; 32],
        ad: TableAd,
        params_hash: Hash,
        advert_hash: Hash,
        now_ms: u64,
    ) -> Result<(), NotTaken> {
        match self.tables.get_mut(&table_key) {
            Some(held) => {
                // Rule 7 **first**, and the order is the fix. Rule 6 used to run
                // ahead of it and return `NotNewer` for anything not strictly
                // later — so a founder that signed two adverts with the SAME
                // timestamp and different parameters had the second silently
                // dropped, and the table was never marked. Two joiners who saw
                // them in different orders then held two rule sets and neither
                // knew.
                //
                // An equivocation is a finding about the table whatever order it
                // arrives in, so it is tested whatever the timestamps say.
                if params_hash != held.params_hash {
                    held.unjoinable = true;
                    return Err(NotTaken::ParametersChanged);
                }
                // Rule 6, now that the parameters are known to agree.
                if ad.timestamp_unix_ms <= held.ad.timestamp_unix_ms {
                    return Err(NotTaken::NotNewer);
                }
                held.ad = ad;
                // The hash follows the copy actually held, because that is what
                // a `JOIN_REQUEST` names and the founder looks up. Leaving the
                // first one behind would have every joiner naming an advert the
                // founder had already replaced.
                held.advert_hash = advert_hash;
                held.received_at_ms = now_ms;
                Ok(())
            }
            None => {
                if self.tables.len() >= MAX_TRACKED_TABLES {
                    // A hard refusal here was a **lockout**: the first 4 096
                    // table keys to arrive kept their slots for ever, and every
                    // honest table advertised afterwards was invisible. The
                    // bound was real and the policy behind it was
                    // first-come-keeps-it, which hands a squatter the whole
                    // lobby for the price of 4 096 signatures.
                    //
                    // So room is made, in two steps. Anything already dead goes
                    // first — a store that is full of expired adverts is not
                    // full. If every slot is still live, the one that expires
                    // soonest is displaced, because it is the entry with the
                    // least left to lose and because a squatter must then keep
                    // re-signing to hold ground rather than claiming it once.
                    //
                    // **What this does not fix**: a peer that keeps re-signing
                    // 4 096 tables holds them legitimately, and no eviction rule
                    // reaches that. It is Sybil without an identity layer, which
                    // `SPEC_CS.md` §18 places outside the threat model. What is
                    // fixed is the version that cost one message per slot and
                    // then nothing at all.
                    self.expire(now_ms);
                    if self.tables.len() >= MAX_TRACKED_TABLES {
                        let victim = self
                            .tables
                            .iter()
                            .min_by_key(|(_, h)| h.ad.expires_at_unix_ms)
                            .map(|(k, _)| *k);
                        match victim {
                            Some(k) => {
                                self.tables.remove(&k);
                            }
                            None => return Err(NotTaken::LobbyFull),
                        }
                    }
                }
                self.tables.insert(
                    table_key,
                    Held {
                        ad,
                        params_hash,
                        advert_hash,
                        received_at_ms: now_ms,
                        unjoinable: false,
                    },
                );
                Ok(())
            }
        }
    }

    /// Drop adverts that have gone quiet or expired.
    ///
    /// Two clocks, deliberately: the founder's `expires_at`, which is what the
    /// table itself claims, and this client's own TTL since it last heard a
    /// re-broadcast. A table whose founder went away without withdrawing its
    /// advert is caught by the second even though the first has not lapsed.
    pub fn expire(&mut self, now_ms: u64) -> usize {
        let before = self.tables.len();
        self.tables.retain(|_, h| {
            let claimed_alive = h.ad.expires_at_unix_ms > now_ms;
            let heard_recently = now_ms.saturating_sub(h.received_at_ms) < AD_TTL_MS;
            claimed_alive && heard_recently
        });
        before - self.tables.len()
    }

    /// Withdraw a table, on a `LOBBY_TABLE_REMOVE` from its own key.
    pub fn remove(&mut self, table_key: &[u8; 32]) -> bool {
        self.tables.remove(table_key).is_some()
    }

    pub fn get(&self, table_key: &[u8; 32]) -> Option<&Held> {
        self.tables.get(table_key)
    }

    /// Every table, in key order.
    pub fn tables(&self) -> impl Iterator<Item = Listing<'_>> {
        self.tables.iter().map(|(k, h)| Listing { key: k, held: h })
    }

    /// The tables a user may actually sit at.
    pub fn joinable(&self) -> impl Iterator<Item = Listing<'_>> {
        self.tables()
            .filter(|listing| !listing.held.unjoinable)
    }

    pub fn len(&self) -> usize {
        self.tables.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }
}

/// §7.6's per-peer rate limits, as a sliding count over one minute.
///
/// Two separate limits because they answer different questions: how often one
/// **peer** may speak at all, and how often one **table key** may re-advertise.
/// A peer relaying a busy lobby is legitimate; a table key re-signing four times
/// a minute is not.
#[derive(Debug, Clone, Default)]
pub struct RateLimiter {
    per_peer: BTreeMap<[u8; 32], Window>,
    /// The same charge for the **chat** topic, in its own map.
    ///
    /// **`NETWORK_STACK.md` §6 promises this and the code did not do it.** The
    /// document says chat is a separate topic *“so that chat volume can never
    /// crowd out table discovery … and so its scoring and rate limits are
    /// independent”*. They were one `BTreeMap`: `lobbytalk::receive` and
    /// `advert::receive` were handed the same `RateLimiter` and both charged
    /// `admit_peer`, so presence and chat spent the advert budget.
    ///
    /// That matters more than it looks, because the charge is against the
    /// **forwarding neighbour** and not the author — `run.rs` says so in terms:
    /// *“a peer relaying somebody else's advert is the ordinary case”*. On a
    /// gossip mesh that makes it a throughput ceiling for the whole topic
    /// rather than a fairness rule, and presence is unconditional at one every
    /// forty seconds while an advert is only sent by a founder of a table that
    /// has not yet dealt. So the traffic that would have been squeezed out
    /// first is exactly table discovery.
    per_peer_talk: BTreeMap<[u8; 32], Window>,
    per_table: BTreeMap<[u8; 32], Window>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Window {
    started_ms: u64,
    count: u32,
}

impl Window {
    fn admit(&mut self, now_ms: u64, cap: u32) -> bool {
        if now_ms.saturating_sub(self.started_ms) >= 60_000 {
            self.started_ms = now_ms;
            self.count = 0;
        }
        if self.count >= cap {
            return false;
        }
        self.count += 1;
        true
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether an advert from this **peer** may be processed at all.
    ///
    /// Charged **before** the signature, which is the expensive part: a limiter
    /// that ran after verification would let a peer spend this client's CPU at
    /// will, which is the thing the limit exists to prevent.
    ///
    /// The peer identity is safe to charge this early because the transport
    /// authenticated it. The **table** key is not, and is charged separately.
    pub fn admit_peer(&mut self, peer: [u8; 32], now_ms: u64) -> bool {
        self.per_peer
            .entry(peer)
            .or_default()
            .admit(now_ms, MAX_ADS_PER_PEER_PER_MIN)
    }

    /// Whether a **chat-topic** message from this peer may be processed.
    ///
    /// The same cap as [`admit_peer`](Self::admit_peer) and deliberately the
    /// same number: this change is about the budgets being **separate**, which
    /// is what the corpus promises, and not about what either budget is worth.
    /// Moving a threshold and un-sharing a map in one edit would leave neither
    /// measured.
    ///
    /// **`MAX_PRESENCE_PER_PEER_PER_MIN` is still unused, and that is on
    /// purpose.** It is 4, which is a sane per-*author* rate against a
    /// heartbeat of one per forty seconds — and this charge is per *forwarding
    /// neighbour*, which relays for its whole mesh. Wiring the constant in here
    /// would drop almost all presence. It is left declared and unread until
    /// something charges the author, which nothing does yet.
    pub fn admit_peer_talk(&mut self, peer: [u8; 32], now_ms: u64) -> bool {
        self.per_peer_talk
            .entry(peer)
            .or_default()
            .admit(now_ms, MAX_ADS_PER_PEER_PER_MIN)
    }

    /// Whether this **table key** may advertise again.
    ///
    /// **Charged only after the signature verifies**, and the first version got
    /// this wrong in a way that handed away the lobby. It read `table_key` out
    /// of the envelope and charged it eighteen lines before `verify_strict` — so
    /// anyone could name any table, four messages a minute, and burn that
    /// table's whole allowance. The real founder's re-broadcast was then rate
    /// limited out, the advert expired, and the table vanished from every lobby
    /// that had heard the forgeries. It cost the attacker four unsigned messages
    /// a minute per table.
    ///
    /// A key that has not been verified is a claim, and a claim must not spend a
    /// budget that belongs to whoever actually holds it.
    pub fn admit_table(&mut self, table_key: [u8; 32], now_ms: u64) -> bool {
        self.per_table
            .entry(table_key)
            .or_default()
            .admit(now_ms, MAX_ADS_PER_TABLE_KEY_PER_MIN)
    }

    /// Forget windows nothing has used for a while, so the limiter is not itself
    /// a growth surface.
    pub fn sweep(&mut self, now_ms: u64) {
        let stale = |w: &Window| now_ms.saturating_sub(w.started_ms) >= 120_000;
        self.per_peer.retain(|_, w| !stale(w));
        // Swept with the others. A map added to this struct and forgotten here
        // is an unbounded one, and this one is keyed by every neighbour that
        // ever relayed a chat message.
        self.per_peer_talk.retain(|_, w| !stale(w));
        self.per_table.retain(|_, w| !stale(w));
    }

    /// Windows held, as `(peers, tables)`.
    ///
    /// The peer figure is the **larger** of the two peer maps rather than their
    /// sum, because this is a memory watch and the two are keyed by the same
    /// neighbours: summing would report a doubling that has not happened.
    pub fn tracked(&self) -> (usize, usize) {
        (
            self.per_peer.len().max(self.per_peer_talk.len()),
            self.per_table.len(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_700_000_000_000;

    /// `NETWORK_STACK.md` §6 promises chat cannot crowd out table discovery.
    ///
    /// It said chat is a separate topic *“so that chat volume can never crowd
    /// out table discovery … and so its scoring and rate limits are
    /// independent”*, and until 2026-09-02 the two paths charged **one**
    /// `BTreeMap` through `admit_peer`. Both charge the *forwarding neighbour*
    /// — `run.rs`: *“a peer relaying somebody else's advert is the ordinary
    /// case”* — so on a gossip mesh a chatty neighbour spent the budget this
    /// client needed to hear about tables, and presence is unconditional while
    /// an advert is not.
    ///
    /// A promise a document makes and the code does not keep is the class this
    /// register calls a defect, so it is a test rather than a comment.
    #[test]
    fn chat_volume_cannot_crowd_out_table_discovery() {
        let mut limits = RateLimiter::new();
        let peer = [7u8; 32];

        for i in 0..MAX_ADS_PER_PEER_PER_MIN {
            assert!(
                limits.admit_peer_talk(peer, NOW),
                "chat message {i} should be admitted"
            );
        }
        assert!(
            !limits.admit_peer_talk(peer, NOW),
            "the chat bucket must fill at its cap"
        );

        for i in 0..MAX_ADS_PER_PEER_PER_MIN {
            assert!(
                limits.admit_peer(peer, NOW),
                "advert {i} must not have paid for that chat"
            );
        }
    }

    /// The second map is swept like the first, or it grows without bound.
    #[test]
    fn the_chat_bucket_is_swept_too() {
        let mut limits = RateLimiter::new();
        for i in 0..100u32 {
            let mut peer = [0u8; 32];
            peer[..4].copy_from_slice(&i.to_be_bytes());
            assert!(limits.admit_peer_talk(peer, NOW));
        }
        assert_eq!(limits.tracked().0, 100);
        limits.sweep(NOW + 120_000);
        assert_eq!(limits.tracked(), (0, 0));
    }

    /// One named change to an otherwise legal advert.
    type Mutation = (&'static str, Box<dyn Fn(&mut TableAd)>);

    fn custom_ad() -> TableAd {
        TableAd {
            game: 1,
            mode: Mode::CashPlayMoney.code(),
            preset_id: "CUSTOM".into(),
            table_name: "Kitchen table".into(),
            small_blind: 10,
            big_blind: 20,
            ante: 0,
            min_buyin: 200,
            max_buyin: 2_000,
            start_stack: 0,
            players: 2,
            max_players: 6,
            min_players_to_start: 2,
            blind_schedule: BlindSchedule {
                mode: 1,
                every_n_hands: 20,
                first_small_blind: 10,
                small_blind_cap: 1_000,
            },
            action_timeout_ms: 20_000,
            action_grace_ms: 5_000,
            crypto_step_timeout_ms: 30_000,
            hand_deadline_ms: 0, // filled in below
            join_deadline_ms: 120_000,
            hand_delay_ms: 7_000,
            time_bank_ms: 0,
            button_rule: 1,
            odd_chip_rule: 1,
            showdown_policy: 1,
            password_required: false,
            deck_suite: DECK_SUITE_V1.into(),
            founder_app_key: [7u8; 32],
            founder_peer_id: vec![1, 2, 3],
            timestamp_unix_ms: NOW,
            expires_at_unix_ms: NOW + 90_000,
            founder_tox_key: None,
            tox_chat_id: None,
        }
    }

    /// A `CUSTOM` advert with a legal deadline for its own shape.
    fn legal_custom() -> TableAd {
        let mut ad = custom_ad();
        ad.hand_deadline_ms = hand_deadline_min_ms(
            ad.max_players,
            ad.action_timeout_ms as u64,
            ad.action_grace_ms as u64,
            ad.crypto_step_timeout_ms as u64,
            ad.hand_delay_ms as u64,
            ad.time_bank_ms as u64,
        ) as u32;
        ad
    }

    fn rated_ad() -> TableAd {
        let mut ad = legal_custom();
        ad.preset_id = "RATED_SNG_POKERTH_V1".into();
        ad.mode = Mode::TournamentSngPlayMoney.code();
        ad.max_players = 10;
        ad.min_players_to_start = 10;
        ad.players = 0;
        ad.start_stack = RATED_START_STACK;
        ad.min_buyin = RATED_START_STACK;
        ad.max_buyin = RATED_START_STACK;
        ad.small_blind = 50;
        ad.big_blind = 100;
        ad.blind_schedule = BlindSchedule {
            mode: 1,
            every_n_hands: 11,
            first_small_blind: 50,
            small_blind_cap: 50_000,
        };
        ad.action_timeout_ms = 20_000;
        ad.action_grace_ms = 5_000;
        ad.crypto_step_timeout_ms = 30_000;
        ad.hand_delay_ms = 7_000;
        ad.hand_deadline_ms = RATED_HAND_DEADLINE_MS as u32;
        ad
    }

    /// **Every one of `table_params_hash`'s twenty-five parts is pinned by the
    /// rated preset**, and this test is the audit §13 prescribes for itself.
    ///
    /// A `preset_id` is a claim about values: two clients that both implement
    /// this name must derive the *same* `table_params_hash`, or they cannot join
    /// each other's table and §7.2 rule 3 has nothing to imply. A part the name
    /// leaves free breaks that with **neither client wrong**, and it is invisible
    /// by reading, because a missing check looks like an omission rather than a
    /// contradiction.
    ///
    /// Two were free when this was written — `join_deadline_ms` and
    /// `showdown_policy`, both parts of the hash and both fixed by §13 — and
    /// rule 2 gave each only a range. That is `G7-S3` a fourth time, in the code
    /// instead of the document.
    ///
    /// The test mutates each part in turn and requires the advert to be
    /// **refused**. A part that survives its mutation is a part nothing pins.
    #[test]
    fn every_part_of_the_hash_is_pinned_by_the_rated_name() {
        let base = TableAd::rated_sng("Rated".into(), [1u8; 32], vec![2u8; 38], NOW);
        assert_eq!(admit(&base, NOW), Ok(()), "the preset itself must be legal");

        // One mutation per part of §3.1's box, in its order. Each moves the part
        // to a value that is legal on its own terms, so the only thing that can
        // refuse it is the preset.
        let parts: Vec<Mutation> = vec![
            ("n(0) game", Box::new(|a: &mut TableAd| a.game = 2)),
            (
                "n(1) mode",
                Box::new(|a: &mut TableAd| {
                    a.mode = Mode::CashPlayMoney.code();
                    a.start_stack = 0;
                }),
            ),
            (
                "n(4) small_blind",
                Box::new(|a: &mut TableAd| {
                    a.small_blind = 25;
                    a.big_blind = 50;
                    a.blind_schedule.first_small_blind = 25;
                }),
            ),
            (
                "n(5) big_blind",
                Box::new(|a: &mut TableAd| a.big_blind = 150),
            ),
            ("n(6) ante", Box::new(|a: &mut TableAd| a.ante = 5)),
            (
                "n(7) min_buyin",
                Box::new(|a: &mut TableAd| a.min_buyin = 5_000),
            ),
            (
                "n(8) max_buyin",
                Box::new(|a: &mut TableAd| a.max_buyin = 20_000),
            ),
            (
                "n(9) start_stack",
                Box::new(|a: &mut TableAd| {
                    a.start_stack = 20_000;
                    a.min_buyin = 20_000;
                    a.max_buyin = 20_000;
                }),
            ),
            (
                "n(11) max_players",
                Box::new(|a: &mut TableAd| a.max_players = 9),
            ),
            (
                "n(12) min_players_to_start",
                Box::new(|a: &mut TableAd| a.min_players_to_start = 2),
            ),
            (
                "n(13.0) schedule mode",
                Box::new(|a: &mut TableAd| a.blind_schedule.mode = 2),
            ),
            (
                "n(13.1) every_n_hands",
                Box::new(|a: &mut TableAd| a.blind_schedule.every_n_hands = 20),
            ),
            (
                "n(13.2) first_small_blind",
                Box::new(|a: &mut TableAd| a.blind_schedule.first_small_blind = 25),
            ),
            (
                "n(13.3) small_blind_cap",
                Box::new(|a: &mut TableAd| a.blind_schedule.small_blind_cap = 60_000),
            ),
            (
                "n(14) action_timeout_ms",
                Box::new(|a: &mut TableAd| a.action_timeout_ms = 30_000),
            ),
            (
                "n(15) action_grace_ms",
                Box::new(|a: &mut TableAd| a.action_grace_ms = 10_000),
            ),
            (
                "n(16) crypto_step_timeout_ms",
                Box::new(|a: &mut TableAd| a.crypto_step_timeout_ms = 60_000),
            ),
            (
                "n(17) hand_deadline_ms",
                Box::new(|a: &mut TableAd| a.hand_deadline_ms = 3_400_000),
            ),
            (
                "n(18) join_deadline_ms",
                Box::new(|a: &mut TableAd| a.join_deadline_ms = 60_000),
            ),
            (
                "n(19) hand_delay_ms",
                Box::new(|a: &mut TableAd| a.hand_delay_ms = 10_000),
            ),
            (
                "n(20) button_rule",
                Box::new(|a: &mut TableAd| a.button_rule = 2),
            ),
            (
                "n(21) odd_chip_rule",
                Box::new(|a: &mut TableAd| a.odd_chip_rule = 2),
            ),
            (
                "n(22) showdown_policy",
                Box::new(|a: &mut TableAd| a.showdown_policy = 2),
            ),
            (
                "n(24) deck_suite",
                Box::new(|a: &mut TableAd| a.deck_suite = "bs-bg12-secp256k1/2".into()),
            ),
        ];

        // Twenty-four mutations for twenty-five parts, because `n(2) preset_id`
        // is not pinned by a **rule** — it is pinned by being the name. It is
        // itself a part of the hash, so an advert carrying a different name is a
        // different game by construction and there is nothing for a receiver to
        // check. Changing it to `CUSTOM` produces a perfectly legal custom
        // table, which is the right answer and not a hole; what must hold is
        // that the hash moves with it, and the assertion below is that.
        assert_eq!(parts.len(), 24, "twenty-four rules for twenty-five parts");

        let mut renamed = base.clone();
        renamed.preset_id = "CUSTOM".into();
        assert_eq!(admit(&renamed, NOW), Ok(()), "it is a legal custom table");
        assert_ne!(
            crate::net::advert::table_params_hash(&renamed),
            crate::net::advert::table_params_hash(&base),
            "n(2) preset_id is in the hash, so a different name is a different game"
        );

        for (name, change) in parts {
            let mut ad = base.clone();
            change(&mut ad);
            assert_ne!(
                ad, base,
                "{name}: the mutation changed nothing, so the test proves nothing"
            );
            assert!(
                admit(&ad, NOW).is_err(),
                "{name} is not pinned: two clients implementing this preset could \
                 choose differently, derive different table_params_hash values, and \
                 fail to join each other with neither of them wrong"
            );
        }
    }

    /// A Sit-and-Go of any size is a legal table, and the same game with a
    /// different number of chairs.
    ///
    /// Everything that does not depend on the seat count is identical across
    /// them; the two that do — the blind cap and the whole-hand deadline — are
    /// derived by §13's own formulae rather than invented.
    #[test]
    fn a_sit_and_go_of_any_size_is_admissible() {
        for seats in 2..=MAX_SEATS {
            let ad = TableAd::sng(seats, "SNG".into(), [1u8; 32], vec![2u8; 38], NOW);
            assert_eq!(
                admit(&ad, NOW),
                Ok(()),
                "a {seats}-seat Sit-and-Go was refused"
            );

            assert_eq!(ad.mode, Mode::TournamentSngPlayMoney.code());
            assert_eq!(ad.max_players, seats);
            assert_eq!(
                ad.min_players_to_start, seats,
                "a Sit-and-Go deals when it is full and not before"
            );
            // A tournament pays every entrant the same stack, so the buy-in is
            // the stack — rule 2 refuses anything else in a tournament mode.
            assert_eq!(ad.start_stack, RATED_START_STACK);
            assert_eq!(ad.min_buyin, ad.start_stack);
            assert_eq!(ad.max_buyin, ad.start_stack);
            // The structure, unchanged by the seat count.
            assert_eq!((ad.small_blind, ad.big_blind), (50, 100));
            assert_eq!(ad.blind_schedule.every_n_hands, RATED_BLIND_EVERY_N_HANDS);
            assert_eq!(ad.ante, 0);
            // And the two that follow from it.
            assert_eq!(
                ad.blind_schedule.small_blind_cap,
                seats as u64 * RATED_START_STACK / 2,
                "the cap is the table's chips, halved"
            );
            let minimum = crate::protocol::constants::hand_deadline_min_ms(
                seats, 20_000, 5_000, 30_000, 7_000, 0,
            );
            assert!(
                ad.hand_deadline_ms as u64 >= minimum,
                "{seats} seats: the deadline is below its own floor"
            );
        }
    }

    /// **Ten seats is the rated preset; anything else is `CUSTOM`.**
    ///
    /// That is what a preset name means: §7.2 rule 3 says the name asserts
    /// §13's exact values and §13 says ten. A six-handed table carrying the
    /// rated name would be refused by every receiver, this client included, so
    /// it carries its own numbers instead.
    #[test]
    fn only_a_ten_seat_sit_and_go_carries_the_rated_name() {
        for seats in 2..=MAX_SEATS {
            let ad = TableAd::sng(seats, "SNG".into(), [1u8; 32], vec![2u8; 38], NOW);
            if seats == RATED_SEATS {
                assert_eq!(ad.preset_id, "RATED_SNG_POKERTH_V1");
            } else {
                assert_eq!(ad.preset_id, "CUSTOM", "{seats} seats claimed the name");
                // And claiming it would be refused, which is the point.
                let mut lying = ad.clone();
                lying.preset_id = "RATED_SNG_POKERTH_V1".into();
                assert!(
                    admit(&lying, NOW).is_err(),
                    "{seats} seats carried the rated name and was accepted"
                );
            }
        }
    }

    /// The two constructors produce the same table at ten seats, field for
    /// field. Two that were supposed to agree would be two places for it to
    /// change.
    #[test]
    fn the_rated_table_is_the_general_one_at_ten_seats() {
        assert_eq!(
            TableAd::rated_sng("SNG".into(), [1u8; 32], vec![2u8; 38], NOW),
            TableAd::sng(RATED_SEATS, "SNG".into(), [1u8; 32], vec![2u8; 38], NOW),
        );
    }

    /// Two Sit-and-Gos of **different** sizes are different games, and the
    /// digest says so. A player choosing six seats is not joinable by a client
    /// that chose nine, and both are right.
    #[test]
    fn two_sizes_are_two_games() {
        let mut seen = std::collections::BTreeSet::new();
        for seats in 2..=MAX_SEATS {
            let ad = TableAd::sng(seats, "SNG".into(), [1u8; 32], vec![2u8; 38], NOW);
            assert!(
                seen.insert(crate::net::advert::table_params_hash(&ad)),
                "two seat counts produced one digest"
            );
        }
        assert_eq!(seen.len(), (MAX_SEATS - 1) as usize);
    }

    /// A rated table has no password, which §13 states and PokerTH enforces.
    ///
    /// The **only** rated check on a field that is not part of
    /// `table_params_hash`, so it cannot make two honest clients disagree about
    /// the digest. It is a policy check: a password-gated table calling itself
    /// rated is making a claim about itself that is not true.
    #[test]
    fn a_rated_table_has_no_password() {
        let mut ad = TableAd::rated_sng("Rated".into(), [1u8; 32], vec![2u8; 38], NOW);
        assert_eq!(admit(&ad, NOW), Ok(()));
        ad.password_required = true;
        assert!(admit(&ad, NOW).is_err(), "a rated table demanded a password");

        // And it really is outside the digest, or the claim above is wrong.
        let mut open = TableAd::rated_sng("Rated".into(), [1u8; 32], vec![2u8; 38], NOW);
        let with = {
            let mut a = open.clone();
            a.password_required = true;
            a
        };
        assert_eq!(
            crate::net::advert::table_params_hash(&open),
            crate::net::advert::table_params_hash(&with),
            "password_required is not a part of table_params_hash"
        );
        open.password_required = false;
    }

    /// The four things the constructor takes are the four §13 does not pin, and
    /// **none of them is a part of the hash** — so two rated tables founded by
    /// two different people, at two different moments, under two different names
    /// are still provably the same game.
    #[test]
    fn what_the_founder_chooses_is_not_part_of_the_game() {
        let a = TableAd::rated_sng("Riverside".into(), [1u8; 32], vec![2u8; 38], NOW);
        let b = TableAd::rated_sng(
            "Somewhere else".into(),
            [9u8; 32],
            vec![7u8; 20],
            NOW + 45_000,
        );
        assert_ne!(a, b, "they are different advertisements");
        assert_eq!(
            crate::net::advert::table_params_hash(&a),
            crate::net::advert::table_params_hash(&b),
            "two rated tables are the same game or the name means nothing"
        );
        assert_eq!(admit(&b, NOW + 45_000), Ok(()));
    }

    /// The numbers, written out, so a change to one of them is a change to this
    /// test and therefore a decision rather than a slip.
    #[test]
    fn the_rated_numbers_are_the_ones_written_down() {
        let a = TableAd::rated_sng("R".into(), [0u8; 32], vec![0u8; 4], NOW);
        assert_eq!(a.start_stack, 10_000, "every seat starts with 10 000");
        assert_eq!((a.small_blind, a.big_blind), (50, 100), "blinds 50/100");
        assert_eq!(a.min_buyin, a.start_stack, "the buy-in is the stack");
        assert_eq!(a.max_buyin, a.start_stack);
        assert_eq!(a.max_players, 10);
        assert_eq!(a.min_players_to_start, 10, "ten of ten");
        assert_eq!(a.blind_schedule.first_small_blind, a.small_blind);
        assert_eq!(a.blind_schedule.small_blind_cap, 50_000);
        assert_eq!(a.ante, 0);
        assert_eq!(a.preset_id, "RATED_SNG_POKERTH_V1");
    }


    #[test]
    fn an_honest_advert_is_admitted() {
        assert_eq!(admit(&legal_custom(), NOW), Ok(()));
        assert_eq!(admit(&rated_ad(), NOW), Ok(()));
    }

    /// Rule 3, and the half that matters: a **third** name is rejected on sight,
    /// whether or not this client recognises it. A receiver that accepted one
    /// would have accepted a table whose identity it cannot check.
    #[test]
    fn a_third_preset_name_is_refused_even_if_it_looks_familiar() {
        for name in [
            "HEADS_UP_CUSTOM_2P", // a name this very repository uses
            "RATED_SNG_POKERTH_V2",
            "rated_sng_pokerth_v1", // case matters
            "",
        ] {
            let mut ad = legal_custom();
            ad.preset_id = name.into();
            assert!(
                matches!(admit(&ad, NOW), Err(AdRejected::Preset(_))),
                "{name} was admitted"
            );
        }
    }

    /// Rule 3's other half: a named preset that does not carry the preset's
    /// values is a lie about what game is being offered. `hand_deadline_ms`
    /// alone is signed into `table_params_hash`, so two clients disagreeing
    /// about it cannot join each other and neither can say why.
    #[test]
    fn a_rated_advert_must_carry_the_rated_values() {
        let mutations: Vec<Mutation> = vec![
            (
                "hand_deadline_ms",
                Box::new(|a: &mut TableAd| a.hand_deadline_ms = 2_700_000),
            ),
            (
                // Both, so the advert stays internally consistent and only the
                // preset rule can catch it. Changing one alone trips rule 2
                // first and would have tested the wrong thing.
                "max_players",
                Box::new(|a: &mut TableAd| {
                    a.max_players = 6;
                    a.min_players_to_start = 6;
                }),
            ),
            (
                "small_blind",
                Box::new(|a: &mut TableAd| {
                    a.small_blind = 25;
                    a.big_blind = 50;
                    a.blind_schedule.first_small_blind = 25;
                }),
            ),
            (
                "every_n_hands",
                Box::new(|a: &mut TableAd| a.blind_schedule.every_n_hands = 10),
            ),
            (
                "start_stack",
                Box::new(|a: &mut TableAd| {
                    a.start_stack = 5_000;
                    a.min_buyin = 5_000;
                    a.max_buyin = 5_000;
                }),
            ),
            (
                // In range and not the preset's, which is what this row tests.
                // It was 20_000, and since `S1-BK` floored §7.2 at the
                // carrier's repair ladder that is refused as `Range` before it
                // can reach the preset check — a green test that had stopped
                // testing the thing it names.
                "crypto_step_timeout_ms",
                Box::new(|a: &mut TableAd| a.crypto_step_timeout_ms = 45_000),
            ),
        ];
        for (what, mutate) in mutations {
            let mut ad = rated_ad();
            mutate(&mut ad);
            assert!(
                matches!(admit(&ad, NOW), Err(AdRejected::Preset(_))),
                "a rated advert with the wrong {what} was admitted"
            );
        }
    }

    /// Rule 2a, the one derived bound. Below it every legal hand at this table
    /// aborts on its own deadline with nobody named — a founder who wants a table
    /// where nothing can ever be won needs no attack, only a small number.
    #[test]
    fn a_deadline_below_the_derived_minimum_is_refused() {
        let mut ad = legal_custom();
        let minimum = hand_deadline_min_ms(
            ad.max_players,
            ad.action_timeout_ms as u64,
            ad.action_grace_ms as u64,
            ad.crypto_step_timeout_ms as u64,
            ad.hand_delay_ms as u64,
            ad.time_bank_ms as u64,
        );
        assert!(minimum > 0);

        ad.hand_deadline_ms = (minimum - 1) as u32;
        assert_eq!(
            admit(&ad, NOW),
            Err(AdRejected::DeadlineTooShort {
                given: (minimum - 1) as u32,
                minimum
            })
        );

        ad.hand_deadline_ms = minimum as u32;
        assert_eq!(admit(&ad, NOW), Ok(()), "and exactly the minimum is legal");
    }

    /// The bound is a function of the advert's own shape, not a constant. A
    /// value legal at two seats is not legal at ten, which is the whole reason
    /// it is derived.
    #[test]
    fn the_minimum_moves_with_the_table() {
        let mut small = legal_custom();
        small.max_players = 2;
        small.min_players_to_start = 2;
        let at_two = hand_deadline_min_ms(2, 20_000, 5_000, 30_000, 7_000, 0);
        let at_ten = hand_deadline_min_ms(10, 20_000, 5_000, 30_000, 7_000, 0);
        assert!(at_ten > at_two, "more seats, more time");

        small.hand_deadline_ms = at_two as u32;
        assert_eq!(admit(&small, NOW), Ok(()));

        let mut big = legal_custom();
        big.max_players = 10;
        big.hand_deadline_ms = at_two as u32;
        assert!(matches!(
            admit(&big, NOW),
            Err(AdRejected::DeadlineTooShort { .. })
        ));
    }

    /// Rule 5's first half. Without it a malicious peer pins a table into every
    /// lobby forever, which is free spam.
    #[test]
    fn an_advert_cannot_pin_itself_into_the_lobby() {
        let mut ad = legal_custom();
        ad.expires_at_unix_ms = NOW + MAX_AD_LIFETIME_MS + 1;
        assert!(matches!(admit(&ad, NOW), Err(AdRejected::Timing(_))));

        ad.expires_at_unix_ms = NOW + MAX_AD_LIFETIME_MS;
        assert_eq!(admit(&ad, NOW), Ok(()));

        ad.expires_at_unix_ms = ad.timestamp_unix_ms;
        assert!(matches!(admit(&ad, NOW), Err(AdRejected::Timing(_))));
    }

    #[test]
    fn an_advert_from_the_future_is_refused() {
        let mut ad = legal_custom();
        ad.timestamp_unix_ms = NOW + MAX_CLOCK_SKEW_MS + 1;
        ad.expires_at_unix_ms = ad.timestamp_unix_ms + 1;
        assert!(matches!(admit(&ad, NOW), Err(AdRejected::Timing(_))));
    }

    /// Rule 2's arithmetic, one field at a time. Each of these is a table that
    /// could not be played.
    #[test]
    fn the_numeric_rules_hold() {
        let cases: Vec<Mutation> = vec![
            ("big blind", Box::new(|a: &mut TableAd| a.big_blind = 30)),
            ("zero small blind", Box::new(|a: &mut TableAd| { a.small_blind = 0; a.big_blind = 0; a.blind_schedule.first_small_blind = 0; })),
            ("schedule disagrees", Box::new(|a: &mut TableAd| a.blind_schedule.first_small_blind = 5)),
            ("buyin order", Box::new(|a: &mut TableAd| a.max_buyin = 100)),
            ("eleven seats", Box::new(|a: &mut TableAd| a.max_players = 11)),
            ("one seat", Box::new(|a: &mut TableAd| { a.max_players = 1; a.min_players_to_start = 1; })),
            ("start above max", Box::new(|a: &mut TableAd| a.min_players_to_start = 7)),
            ("players above max", Box::new(|a: &mut TableAd| a.players = 7)),
            ("an ante", Box::new(|a: &mut TableAd| a.ante = 5)),
            ("not NLHE", Box::new(|a: &mut TableAd| a.game = 2)),
            ("stack on cash", Box::new(|a: &mut TableAd| a.start_stack = 1_000)),
            ("a four second clock", Box::new(|a: &mut TableAd| a.action_timeout_ms = 4_999)),
            ("a live button rule", Box::new(|a: &mut TableAd| a.button_rule = 2)),
            ("an odd chip rule", Box::new(|a: &mut TableAd| a.odd_chip_rule = 2)),
            ("a third showdown policy", Box::new(|a: &mut TableAd| a.showdown_policy = 3)),
            ("a schedule mode", Box::new(|a: &mut TableAd| a.blind_schedule.mode = 2)),
            ("a cap below the first level", Box::new(|a: &mut TableAd| a.blind_schedule.small_blind_cap = 1)),
        ];
        for (what, mutate) in cases {
            let mut ad = legal_custom();
            mutate(&mut ad);
            assert!(admit(&ad, NOW).is_err(), "{what} was admitted");
        }
    }

    /// A tournament pays every entrant the same stack, so the buy-in **is** the
    /// stack. Without the rule the two buy-in fields are free parts of
    /// `table_params_hash` that a named configuration has to pin one at a time.
    #[test]
    fn a_tournament_buyin_is_the_stack() {
        let mut ad = legal_custom();
        ad.mode = Mode::TournamentSngPlayMoney.code();
        ad.start_stack = 5_000;
        ad.min_buyin = 5_000;
        ad.max_buyin = 5_000;
        assert_eq!(admit(&ad, NOW), Ok(()));

        ad.max_buyin = 6_000;
        assert!(matches!(admit(&ad, NOW), Err(AdRejected::Range(_))));
    }

    #[test]
    fn an_unknown_deck_suite_is_refused() {
        let mut ad = legal_custom();
        ad.deck_suite = "bs-bg12-secp256k1/2".into();
        assert_eq!(admit(&ad, NOW), Err(AdRejected::UnknownDeckSuite));
    }

    #[test]
    fn a_display_field_is_bounded_and_printable() {
        let mut ad = legal_custom();
        ad.table_name = "x".repeat(65);
        assert!(matches!(admit(&ad, NOW), Err(AdRejected::Display(_))));

        let mut ad = legal_custom();
        ad.table_name = "line\u{0}break".into();
        assert!(matches!(admit(&ad, NOW), Err(AdRejected::Display(_))));

        let mut ad = legal_custom();
        ad.founder_peer_id = vec![0u8; 43];
        assert!(matches!(admit(&ad, NOW), Err(AdRejected::Display(_))));
    }

    // -- the store ----------------------------------------------------------

    fn h(b: u8) -> Hash {
        [b; 32]
    }

    /// Rule 6: a stale advert is not a re-broadcast.
    #[test]
    fn an_older_advert_does_not_replace_a_newer_one() {
        let mut store = LobbyStore::new();
        let key = [1u8; 32];
        let mut ad = legal_custom();
        ad.timestamp_unix_ms = NOW + 1_000;
        store.offer(key, ad.clone(), h(9), [0u8; 32], NOW).unwrap();

        ad.timestamp_unix_ms = NOW;
        assert_eq!(
            store.offer(key, ad.clone(), h(9), [0u8; 32], NOW),
            Err(NotTaken::NotNewer)
        );
        assert_eq!(
            store.get(&key).unwrap().ad.timestamp_unix_ms,
            NOW + 1_000
        );
    }

    /// Rule 7, which is J1(b). Rule 6 compares two adverts on their timestamp
    /// and nothing else, so nothing stopped a founder re-signing with a
    /// different `small_blind` and handing two joiners two rule sets — which
    /// forks `HAND_INIT`, a collective stage whose bodies must be byte-identical.
    #[test]
    fn a_table_whose_parameters_changed_becomes_unjoinable() {
        let mut store = LobbyStore::new();
        let key = [1u8; 32];
        let mut ad = legal_custom();
        store.offer(key, ad.clone(), h(1), [0u8; 32], NOW).unwrap();
        assert_eq!(store.joinable().count(), 1);

        ad.timestamp_unix_ms += 1;
        ad.small_blind = 25;
        assert_eq!(
            store.offer(key, ad, h(2), [0u8; 32], NOW),
            Err(NotTaken::ParametersChanged)
        );

        assert!(store.get(&key).unwrap().unjoinable);
        assert_eq!(store.joinable().count(), 0, "and it stays visible");
        assert_eq!(
            store.get(&key).unwrap().ad.small_blind,
            10,
            "the changed advert was discarded, not applied"
        );
    }


    /// The equivocation rule 6 used to hide.
    ///
    /// A founder signs two adverts with the **same timestamp** and different
    /// parameters. Rule 6 ran first and returned `NotNewer` for the second, so
    /// nothing compared the parameters and nothing was marked — and two joiners
    /// who saw the pair in different orders held two different rule sets, each
    /// believing it held the only one.
    #[test]
    fn an_equal_timestamp_equivocation_is_caught_and_not_dropped() {
        let mut store = LobbyStore::new();
        let key = [1u8; 32];

        let first = legal_custom();
        store.offer(key, first.clone(), h(1), [0u8; 32], NOW).unwrap();

        // Same timestamp, different parameters.
        let mut second = first;
        second.small_blind = 25;
        assert_eq!(
            store.offer(key, second, h(2), [0u8; 32], NOW),
            Err(NotTaken::ParametersChanged),
            "not NotNewer: the timestamps are equal and the game is not"
        );
        assert!(store.get(&key).unwrap().unjoinable);
    }

    /// And an OLDER advert with different parameters is equivocation too. The
    /// order a receiver happens to see them in is not a property of the founder.
    #[test]
    fn an_older_advert_with_other_parameters_is_still_equivocation() {
        let mut store = LobbyStore::new();
        let key = [1u8; 32];

        let mut newer = legal_custom();
        newer.timestamp_unix_ms = NOW + 10_000;
        store.offer(key, newer, h(1), [0u8; 32], NOW).unwrap();

        let mut older = legal_custom();
        older.small_blind = 25;
        assert_eq!(
            store.offer(key, older, h(2), [0u8; 32], NOW),
            Err(NotTaken::ParametersChanged)
        );
        assert!(store.get(&key).unwrap().unjoinable);
    }

    /// A legitimate re-broadcast refreshes the advert and the clock.
    #[test]
    fn a_rebroadcast_refreshes_the_table() {
        let mut store = LobbyStore::new();
        let key = [1u8; 32];
        let mut ad = legal_custom();
        store.offer(key, ad.clone(), h(1), [0u8; 32], NOW).unwrap();

        ad.timestamp_unix_ms += 30_000;
        ad.players = 4;
        store.offer(key, ad, h(1), [0u8; 32], NOW + 30_000).unwrap();

        let held = store.get(&key).unwrap();
        assert_eq!(held.ad.players, 4);
        assert_eq!(held.received_at_ms, NOW + 30_000);
        assert!(!held.unjoinable);
    }

    /// Two clocks: what the table claims, and how long since this client heard
    /// from it. A founder that went away without withdrawing is caught by the
    /// second even though the first has not lapsed.
    #[test]
    fn a_table_that_went_quiet_expires_even_before_it_claims_to() {
        let mut store = LobbyStore::new();
        let mut ad = legal_custom();
        ad.expires_at_unix_ms = NOW + MAX_AD_LIFETIME_MS;
        store.offer([1u8; 32], ad, h(1), [0u8; 32], NOW).unwrap();

        assert_eq!(store.expire(NOW + AD_TTL_MS - 1), 0);
        assert_eq!(store.expire(NOW + AD_TTL_MS), 1, "no re-broadcast heard");
        assert!(store.is_empty());
    }

    /// **An advert that has already expired is not admitted in the first
    /// place**, and this is the other half of the sweep below.
    ///
    /// Only the future was bounded: `expires_at` had to be after `timestamp`
    /// and within the lifetime cap ahead of local time, and nothing said it had
    /// to be ahead of local time at all. So anybody who had ever seen an advert
    /// could re-inject it after it lapsed and put a dead table back into every
    /// lobby it reached — no key, no signature of their own, and the table's own
    /// signature still verifying, because the bytes are genuine.
    ///
    /// **To make this fail:** remove the `expires_at_unix_ms <= now_unix_ms`
    /// arm from `admit`.
    #[test]
    fn an_advert_that_has_already_expired_is_refused_rather_than_swept_later() {
        let mut ad = legal_custom();
        ad.timestamp_unix_ms = NOW - 200_000;
        ad.expires_at_unix_ms = NOW - 100_000;
        assert_eq!(
            admit(&ad, NOW),
            Err(AdRejected::Timing("expires_at has already passed")),
            "a lapsed advert was taken in, to be dropped by the next sweep"
        );

        // And one that is still alive by a second is still admitted, so this is
        // a bound and not a narrowing.
        ad.expires_at_unix_ms = NOW + 1_000;
        assert_eq!(admit(&ad, NOW), Ok(()));
    }

    #[test]
    fn an_expired_advert_goes_even_if_it_was_just_heard() {
        let mut store = LobbyStore::new();
        let mut ad = legal_custom();
        ad.expires_at_unix_ms = NOW + 1_000;
        store.offer([1u8; 32], ad, h(1), [0u8; 32], NOW).unwrap();
        assert_eq!(store.expire(NOW + 2_000), 1);
    }

    /// The advert stream is open to strangers, so the lobby is bounded — and the
    /// bound makes room rather than locking the door.
    ///
    /// The first version refused every new key once full, which is not a bound
    /// but a **lockout**: the first 4 096 keys to arrive kept their slots for
    /// ever and every honest table advertised afterwards was invisible. A
    /// squatter bought the whole lobby for 4 096 signatures and then paid
    /// nothing.
    #[test]
    fn a_full_lobby_makes_room_rather_than_locking_the_door() {
        let mut store = LobbyStore::new();
        for i in 0..MAX_TRACKED_TABLES as u32 {
            let mut key = [0u8; 32];
            key[..4].copy_from_slice(&i.to_be_bytes());
            assert_eq!(store.offer(key, legal_custom(), h(1), [0u8; 32], NOW), Ok(()));
        }
        assert_eq!(store.len(), MAX_TRACKED_TABLES);

        // An honest table arriving now is seen, and the store stays bounded.
        let mut honest = legal_custom();
        honest.table_name = "arrived late".into();
        honest.expires_at_unix_ms = NOW + MAX_AD_LIFETIME_MS;
        assert_eq!(store.offer([0xFF; 32], honest, h(1), [0u8; 32], NOW), Ok(()));
        assert_eq!(store.len(), MAX_TRACKED_TABLES);
        assert!(
            store.get(&[0xFF; 32]).is_some(),
            "the late arrival is in the lobby, which is the whole point"
        );
    }

    /// And what it displaces is the entry with the least left to lose.
    #[test]
    fn the_soonest_to_expire_is_the_one_displaced() {
        let mut store = LobbyStore::new();
        for i in 0..MAX_TRACKED_TABLES as u32 {
            let mut key = [0u8; 32];
            key[..4].copy_from_slice(&i.to_be_bytes());
            let mut ad = legal_custom();
            // Table 0 expires first by a whole second.
            ad.expires_at_unix_ms = NOW + 60_000 + i as u64;
            assert_eq!(store.offer(key, ad, h(1), [0u8; 32], NOW), Ok(()));
        }

        let mut fresh = legal_custom();
        fresh.expires_at_unix_ms = NOW + MAX_AD_LIFETIME_MS;
        store.offer([0xFF; 32], fresh, h(1), [0u8; 32], NOW).unwrap();

        assert!(
            store.get(&[0u8; 32]).is_none(),
            "the one closest to expiry went"
        );
        let mut second = [0u8; 32];
        second[..4].copy_from_slice(&1u32.to_be_bytes());
        assert!(store.get(&second).is_some(), "and only that one");
    }

    /// A store full of dead adverts is not full. Expiry runs before anything is
    /// displaced, so a live table is never evicted while a corpse holds a slot.
    #[test]
    fn dead_adverts_are_cleared_before_a_live_one_is_displaced() {
        let mut store = LobbyStore::new();
        for i in 0..MAX_TRACKED_TABLES as u32 {
            let mut key = [0u8; 32];
            key[..4].copy_from_slice(&i.to_be_bytes());
            assert_eq!(store.offer(key, legal_custom(), h(1), [0u8; 32], NOW), Ok(()));
        }

        // Long enough that every held advert has expired on its own terms.
        let later = NOW + 200_000;
        let mut fresh = legal_custom();
        fresh.timestamp_unix_ms = later;
        fresh.expires_at_unix_ms = later + 90_000;
        store.offer([0xFF; 32], fresh, h(1), [0u8; 32], later).unwrap();

        assert_eq!(store.len(), 1, "the corpses went, not a live table");
        assert!(store.get(&[0xFF; 32]).is_some());
    }

    #[test]
    fn a_table_can_withdraw_itself() {
        let mut store = LobbyStore::new();
        store.offer([1u8; 32], legal_custom(), h(1), [0u8; 32], NOW).unwrap();
        assert!(store.remove(&[1u8; 32]));
        assert!(!store.remove(&[1u8; 32]));
        assert!(store.is_empty());
    }

    // -- the rate limits ----------------------------------------------------

    /// Charged before the signature check, which is the expensive part. A
    /// limiter that ran after verification would let a peer spend this client's
    /// CPU at will, which is the thing it exists to prevent.
    #[test]
    fn a_peer_cannot_flood_the_lobby() {
        let mut rl = RateLimiter::new();
        let peer = [1u8; 32];
        let mut admitted = 0;
        for i in 0..100u32 {
            let mut table = [0u8; 32];
            table[..4].copy_from_slice(&i.to_be_bytes());
            if rl.admit_peer(peer, NOW) && rl.admit_table(table, NOW) {
                admitted += 1;
            }
        }
        assert_eq!(
            admitted, MAX_ADS_PER_PEER_PER_MIN,
            "and a fresh table key each time does not buy more"
        );
    }

    /// A peer relaying a busy lobby is legitimate; one table key re-signing five
    /// times a minute is not. Two limits, two questions.
    #[test]
    fn one_table_key_cannot_resign_faster_than_the_cap() {
        let mut rl = RateLimiter::new();
        let table = [9u8; 32];
        let mut admitted = 0;
        for i in 0..20u32 {
            let mut peer = [0u8; 32];
            peer[..4].copy_from_slice(&i.to_be_bytes());
            if rl.admit_peer(peer, NOW) && rl.admit_table(table, NOW) {
                admitted += 1;
            }
        }
        assert_eq!(
            admitted, MAX_ADS_PER_TABLE_KEY_PER_MIN,
            "and relaying through fresh peers does not buy more"
        );
    }

    #[test]
    fn the_window_reopens_after_a_minute() {
        let mut rl = RateLimiter::new();
        let peer = [1u8; 32];
        let table = [2u8; 32];
        assert!(rl.admit_peer(peer, NOW) && rl.admit_table(table, NOW));
        for _ in 0..MAX_ADS_PER_TABLE_KEY_PER_MIN {
            rl.admit_table(table, NOW);
        }
        assert!(!rl.admit_table(table, NOW));
        assert!(rl.admit_table(table, NOW + 60_000));
    }

    /// The limiter must not itself be a growth surface.
    #[test]
    fn the_limiter_forgets_what_stopped_speaking() {
        let mut rl = RateLimiter::new();
        for i in 0..1_000u32 {
            let mut peer = [0u8; 32];
            peer[..4].copy_from_slice(&i.to_be_bytes());
            rl.admit_peer(peer, NOW);
            rl.admit_table([9u8; 32], NOW);
        }
        assert_eq!(rl.tracked().0, 1_000);
        rl.sweep(NOW + 120_000);
        assert_eq!(rl.tracked(), (0, 0));
    }
}
