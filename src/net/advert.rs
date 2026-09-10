//! Putting a table on the wire, and taking one off it.
//!
//! The last link of D-003. Everything either side of this module existed and was
//! tested — the canonical encoding, the signed envelope, §7.2's admission rules,
//! the lobby store — and none of it was joined up, so two clients could find each
//! other and still not see each other's tables.
//!
//! # The order a received advert is put through, and why it is this one
//!
//! Each step is cheaper than the one after it, and each is a gate the next one
//! would otherwise have to trust:
//!
//! 1. **Rate limit the sending peer.** Free, and it runs **before** the
//!    signature — a limiter that ran after verification would let a peer spend
//!    this client's CPU at will, which is the thing it exists to prevent. The
//!    peer identity is safe to charge here because the transport authenticated
//!    it.
//!    Then, **after step 3 and not before**, rate limit the **table key**.
//!    Before the signature that key is a *claim*, and charging a budget against
//!    a claim let anyone evict any table from every lobby for four unsigned
//!    messages a minute. See [`lobby::RateLimiter::admit_table`].
//! 2. **Decode**, canonically. The bytes are re-encoded and compared, so one
//!    advert is one byte string and the duplicate cache and the message id mean
//!    what they say.
//! 3. **`verify_strict`**, never `verify` — for a reason narrower than the
//!    folklore, and the narrower reason is the measured one. See
//!    `what_the_two_signature_checks_actually_do`.
//! 4. **§7.2 rules 2 to 5**, which is [`lobby::admit`].
//! 5. **§7.2 rules 6 and 7**, which need the advert already held and are
//!    [`super::lobby::LobbyStore`]'s.
//!
//! # The table's identity is the key that signed it
//!
//! There is no `table_id` in the advert body and the envelope's is the unchained
//! sentinel. A body that carried an identity could disagree with the key that
//! signed it, and then two receivers reading the same bytes would file the same
//! table under two names.

use ed25519_dalek::{Signature, VerifyingKey};
use minicbor::{Decode, Encode};

use super::lobby::{self, AdRejected, BlindSchedule, LobbyStore, NotTaken, RateLimiter, TableAd};
use crate::poker::state::Hash;
use crate::protocol::constants::{LOBBY_MSG_MAX, TABLE_AD_MAX};
use crate::protocol::messages::{EventBody, EventType, SignedEvent, ZERO32};
use crate::protocol::serialization::{from_canonical, h, to_canonical};
use crate::protocol::signatures::{to_be_signed, Domain};

/// The advert body, in its wire form.
///
/// A separate type from [`TableAd`] on purpose. This one is the CBOR array
/// §7.2 specifies and exists to be encoded; that one is what the admission rules
/// read.
///
/// **Twenty-nine fields, `n(0)` to `n(28)`, with `n(13)` a nested array.** The
/// first version of this struct flattened `blind_schedule` into four top-level
/// fields and shifted every later index by three, which is a silent wire
/// incompatibility: two clients would each decode the other's advert into
/// well-formed nonsense. And `preset_id`, `table_name` and `deck_suite` are CBOR
/// **bytes**, not text — the protocol types all three as `bytes`, and the
/// difference is a major-type byte in the preimage of every hash over them.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct WireSchedule {
    #[n(0)]
    pub mode: u16,
    #[n(1)]
    pub every_n_hands: u16,
    #[n(2)]
    pub first_small_blind: u64,
    #[n(3)]
    pub small_blind_cap: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct AdBody {
    #[n(0)]
    pub game: u16,
    #[n(1)]
    pub mode: u16,
    #[cbor(n(2), with = "minicbor::bytes")]
    pub preset_id: Vec<u8>,
    #[cbor(n(3), with = "minicbor::bytes")]
    pub table_name: Vec<u8>,
    #[n(4)]
    pub small_blind: u64,
    #[n(5)]
    pub big_blind: u64,
    #[n(6)]
    pub ante: u64,
    #[n(7)]
    pub min_buyin: u64,
    #[n(8)]
    pub max_buyin: u64,
    #[n(9)]
    pub start_stack: u64,
    #[n(10)]
    pub players: u8,
    #[n(11)]
    pub max_players: u8,
    #[n(12)]
    pub min_players_to_start: u8,
    #[n(13)]
    pub blind_schedule: WireSchedule,
    #[n(14)]
    pub action_timeout_ms: u32,
    #[n(15)]
    pub action_grace_ms: u32,
    #[n(16)]
    pub crypto_step_timeout_ms: u32,
    #[n(17)]
    pub hand_deadline_ms: u32,
    #[n(18)]
    pub join_deadline_ms: u32,
    #[n(19)]
    pub hand_delay_ms: u32,
    /// The per-hand thinking reserve every seat may spend on top of
    /// `action_timeout_ms`. **Inside `table_params_hash`**, because every peer
    /// adds it to a betting stage's deadline before voting a seat late, and two
    /// peers using different values would certify a player who was still
    /// legitimately thinking.
    #[n(29)]
    pub time_bank_ms: u32,
    #[n(20)]
    pub button_rule: u16,
    #[n(21)]
    pub odd_chip_rule: u16,
    #[n(22)]
    pub showdown_policy: u16,
    #[n(23)]
    pub password_required: bool,
    #[cbor(n(24), with = "minicbor::bytes")]
    pub deck_suite: Vec<u8>,
    #[cbor(n(25), with = "minicbor::bytes")]
    pub founder_app_key: [u8; 32],
    #[cbor(n(26), with = "minicbor::bytes")]
    pub founder_peer_id: Vec<u8>,
    #[n(27)]
    pub timestamp_unix_ms: u64,
    #[n(28)]
    pub expires_at_unix_ms: u64,
    /// The founder's **Tox** public key, and the chat id of the group that
    /// carries this table's traffic (D-019).
    ///
    /// # What they are for, and they are not the same thing
    ///
    /// The **key** is how a joiner is reached. A Tox group invitation takes a
    /// friend number, not an address, so the founder and the joiner must be Tox
    /// friends before an invitation is possible — and `tox_friend_add_norequest`
    /// makes that automatic from a public key, with no request and nothing for
    /// anybody to accept. Without this field there is no route to an
    /// invitation, and the only alternative is joining by `chat_id` through
    /// Tox's DHT, which is the path measured to work only while a group is new.
    ///
    /// The **chat id** is not how anybody joins. It is what a joiner compares
    /// the group it was invited into **against**: an invitation says nothing
    /// about which group it is for, so a founder could invite a player into a
    /// different one — and a table whose traffic is on a group nobody
    /// advertised is a table nobody else can audit. Publishing it turns that
    /// from unnoticeable into a check.
    ///
    /// # Both are `Option`, and both are outside `table_params_hash`
    ///
    /// `Option`, because a build without `--features tox` advertises a table it
    /// carries on the swarm and must not be forced to invent a key it does not
    /// have. Absent means *this table is not on Tox*.
    ///
    /// **Outside the digest**, for exactly the reason `founder_peer_id` is
    /// (§3.1): identity and routing. A founder that restarts comes back with a
    /// different Tox identity and a different group, which is legitimate — and
    /// if these were hashed, §7.2 rule 7 would see changed parameters and every
    /// client holding the table would mark it permanently unjoinable. That is
    /// `table_name`'s bug with a different field in it.
    #[cbor(n(30), with = "minicbor::bytes")]
    pub founder_tox_key: Option<[u8; 32]>,
    #[cbor(n(31), with = "minicbor::bytes")]
    pub tox_chat_id: Option<[u8; 32]>,
}

impl From<&TableAd> for AdBody {
    fn from(a: &TableAd) -> Self {
        AdBody {
            game: a.game,
            mode: a.mode,
            preset_id: a.preset_id.clone().into_bytes(),
            table_name: a.table_name.clone().into_bytes(),
            small_blind: a.small_blind,
            big_blind: a.big_blind,
            ante: a.ante,
            min_buyin: a.min_buyin,
            max_buyin: a.max_buyin,
            start_stack: a.start_stack,
            players: a.players,
            max_players: a.max_players,
            min_players_to_start: a.min_players_to_start,
            blind_schedule: WireSchedule {
                mode: a.blind_schedule.mode,
                every_n_hands: a.blind_schedule.every_n_hands,
                first_small_blind: a.blind_schedule.first_small_blind,
                small_blind_cap: a.blind_schedule.small_blind_cap,
            },
            action_timeout_ms: a.action_timeout_ms,
            action_grace_ms: a.action_grace_ms,
            crypto_step_timeout_ms: a.crypto_step_timeout_ms,
            hand_deadline_ms: a.hand_deadline_ms,
            join_deadline_ms: a.join_deadline_ms,
            hand_delay_ms: a.hand_delay_ms,
            time_bank_ms: a.time_bank_ms,
            button_rule: a.button_rule,
            odd_chip_rule: a.odd_chip_rule,
            showdown_policy: a.showdown_policy,
            password_required: a.password_required,
            deck_suite: a.deck_suite.clone().into_bytes(),
            founder_app_key: a.founder_app_key,
            founder_peer_id: a.founder_peer_id.clone(),
            timestamp_unix_ms: a.timestamp_unix_ms,
            expires_at_unix_ms: a.expires_at_unix_ms,
            founder_tox_key: a.founder_tox_key,
            tox_chat_id: a.tox_chat_id,
        }
    }
}

/// The three display fields are bytes on the wire and text to the rules.
///
/// A body whose bytes are not UTF-8 is refused rather than lossily converted:
/// §7.2 requires `table_name` to be well-formed UTF-8, and a lossy conversion
/// would turn a malformed advert into a well-formed one with a different name at
/// every receiver that used a different lossy rule.
impl TryFrom<AdBody> for TableAd {
    type Error = NotAccepted;

    fn try_from(b: AdBody) -> Result<Self, NotAccepted> {
        let text = |v: Vec<u8>, what: &'static str| {
            String::from_utf8(v).map_err(|_| NotAccepted::Malformed(what))
        };
        Ok(TableAd {
            game: b.game,
            mode: b.mode,
            preset_id: text(b.preset_id, "preset_id is not UTF-8")?,
            table_name: text(b.table_name, "table_name is not UTF-8")?,
            small_blind: b.small_blind,
            big_blind: b.big_blind,
            ante: b.ante,
            min_buyin: b.min_buyin,
            max_buyin: b.max_buyin,
            start_stack: b.start_stack,
            players: b.players,
            max_players: b.max_players,
            min_players_to_start: b.min_players_to_start,
            blind_schedule: BlindSchedule {
                mode: b.blind_schedule.mode,
                every_n_hands: b.blind_schedule.every_n_hands,
                first_small_blind: b.blind_schedule.first_small_blind,
                small_blind_cap: b.blind_schedule.small_blind_cap,
            },
            action_timeout_ms: b.action_timeout_ms,
            action_grace_ms: b.action_grace_ms,
            crypto_step_timeout_ms: b.crypto_step_timeout_ms,
            hand_deadline_ms: b.hand_deadline_ms,
            join_deadline_ms: b.join_deadline_ms,
            hand_delay_ms: b.hand_delay_ms,
            time_bank_ms: b.time_bank_ms,
            button_rule: b.button_rule,
            odd_chip_rule: b.odd_chip_rule,
            showdown_policy: b.showdown_policy,
            password_required: b.password_required,
            deck_suite: text(b.deck_suite, "deck_suite is not UTF-8")?,
            founder_app_key: b.founder_app_key,
            founder_peer_id: b.founder_peer_id,
            timestamp_unix_ms: b.timestamp_unix_ms,
            expires_at_unix_ms: b.expires_at_unix_ms,
            founder_tox_key: b.founder_tox_key,
            tox_chat_id: b.tox_chat_id,
        })
    }
}

/// `table_params_hash`, exactly as `PROTOCOL.md` §3.1's normative box writes it.
///
/// **Twenty-five parts, in that order**, each length-prefixed by §2.8's
/// constructor: `preset_id` and `deck_suite` as their raw payload bytes, and
/// every other part in the fixed-width big-endian form. The order is §7.2's
/// ascending field order with `BlindSchedule` expanded in place.
///
/// # What is excluded, and why the first version of this function was a live bug
///
/// The first version hashed the **encoded body** with three fields zeroed, on
/// the reasoning that a field-by-field enumeration is a second copy of the list
/// and those drift. The spec rejects that position outright — §13 publishes both
/// preset blocks as twenty-five indexed lines *so that they can be diffed field
/// by field* — and the shortcut was wrong in five independent ways at once: one
/// length prefix instead of twenty-five, CBOR shortest-form integers instead of
/// fixed-width, CBOR type headers in the preimage, and four fields §3.1 excludes
/// **by name** hashed live.
///
/// One of those four made it a bug with no adversary in it. `table_name` is
/// display text, and §3.1 excludes it for exactly this reason: a founder who
/// fixes a typo in the table's name changes the digest, §7.2 rule 7 sees changed
/// parameters, and every client that holds the table marks it **permanently
/// unjoinable**. The other three are `players` (advisory), `password_required`
/// (an admission gate, spent at `JOIN_REQUEST`), and `founder_app_key` /
/// `founder_peer_id` (identity and routing, whose special position ends at
/// `TABLE_READY`).
///
/// `protocol_version` and `table_id` are excluded because `GENESIS(0)` already
/// carries both as separate parts.
pub fn table_params_hash(ad: &TableAd) -> Hash {
    let game = ad.game.to_be_bytes();
    let mode = ad.mode.to_be_bytes();
    let small_blind = ad.small_blind.to_be_bytes();
    let big_blind = ad.big_blind.to_be_bytes();
    let ante = ad.ante.to_be_bytes();
    let min_buyin = ad.min_buyin.to_be_bytes();
    let max_buyin = ad.max_buyin.to_be_bytes();
    let start_stack = ad.start_stack.to_be_bytes();
    let max_players = [ad.max_players];
    let min_players = [ad.min_players_to_start];
    let sched_mode = ad.blind_schedule.mode.to_be_bytes();
    let every_n = ad.blind_schedule.every_n_hands.to_be_bytes();
    let first_sb = ad.blind_schedule.first_small_blind.to_be_bytes();
    let sb_cap = ad.blind_schedule.small_blind_cap.to_be_bytes();
    let action_timeout = ad.action_timeout_ms.to_be_bytes();
    let action_grace = ad.action_grace_ms.to_be_bytes();
    let crypto_step = ad.crypto_step_timeout_ms.to_be_bytes();
    let hand_deadline = ad.hand_deadline_ms.to_be_bytes();
    let join_deadline = ad.join_deadline_ms.to_be_bytes();
    let hand_delay = ad.hand_delay_ms.to_be_bytes();
    let time_bank = ad.time_bank_ms.to_be_bytes();
    let button_rule = ad.button_rule.to_be_bytes();
    let odd_chip_rule = ad.odd_chip_rule.to_be_bytes();
    let showdown = ad.showdown_policy.to_be_bytes();

    h(
        Domain::TableParams.context(),
        &[
            &game,                      // n(0)
            &mode,                      // n(1)
            ad.preset_id.as_bytes(),    // n(2), payload bytes verbatim
            &small_blind,               // n(4)
            &big_blind,                 // n(5)
            &ante,                      // n(6)
            &min_buyin,                 // n(7)
            &max_buyin,                 // n(8)
            &start_stack,               // n(9)
            &max_players,               // n(11)
            &min_players,               // n(12)
            &sched_mode,                // n(13) BlindSchedule n(0)
            &every_n,                   //                     n(1)
            &first_sb,                  //                     n(2)
            &sb_cap,                    //                     n(3)
            &action_timeout,            // n(14)
            &action_grace,              // n(15)
            &crypto_step,               // n(16)
            &hand_deadline,             // n(17)
            &join_deadline,             // n(18)
            &hand_delay,                // n(19)
            &button_rule,               // n(20)
            &odd_chip_rule,             // n(21)
            &showdown,                  // n(22)
            ad.deck_suite.as_bytes(),   // n(24), payload bytes verbatim
            &time_bank,                 // n(29)
        ],
    )
}

/// Why a received advert did not reach the lobby.
#[derive(Debug, Clone, PartialEq)]
pub enum NotAccepted {
    /// The sender or the table key is over its per-minute allowance.
    RateLimited,
    /// The envelope or the body did not decode, or is not canonical.
    Malformed(&'static str),
    /// The envelope is well formed but is not a lobby advert.
    WrongType,
    /// `verify_strict` did not pass under the signing key.
    ///
    /// Always `verify_strict`, because `PROTOCOL.md` requires it by name — not
    /// because the permissive form is measurably weaker in the version pinned
    /// here. See the test of that name.
    BadSignature,
    /// §7.2 rules 2 to 5.
    Refused(AdRejected),
    /// §7.2 rules 6 and 7, or the lobby is full.
    NotTaken(NotTaken),
}

/// Publish this client's own table.
///
/// The body is signed under the **table** key, which is the table's whole
/// identity. The envelope carries the unchained sentinels, because an identity
/// in two places is an identity two receivers can disagree about — and there is
/// no `sequence` parameter for the same reason: an unchained event has none, and
/// offering one would have been offering a way to be wrong.
pub fn publish(
    ad: &TableAd,
    table_key: &ed25519_dalek::SigningKey,
) -> Result<Vec<u8>, &'static str> {
    let body_bytes = to_canonical(&AdBody::from(ad)).map_err(|_| "the advert does not encode")?;
    if body_bytes.len() > TABLE_AD_MAX {
        return Err("the advert is over its cap");
    }

    // Built by the one constructor that knows the sentinels, so the publisher
    // cannot disagree with the checker about them.
    let envelope = EventBody::unchained(
        EventType::LobbyTableAd,
        table_key.verifying_key().to_bytes(),
        body_bytes,
        ad.timestamp_unix_ms,
    )
    .ok_or("a lobby advert is an unchained event")?;

    let envelope_bytes = to_canonical(&envelope).map_err(|_| "the envelope does not encode")?;
    let signature = {
        use ed25519_dalek::Signer;
        table_key.sign(&to_be_signed(&envelope_bytes))
    };

    to_canonical(&SignedEvent {
        body: envelope_bytes,
        signature: signature.to_bytes(),
    })
    .map_err(|_| "the signed event does not encode")
}

/// Take an advert off the wire.
///
/// `from_peer` is the GossipSub source, which is **not** the table key: a peer
/// relaying somebody else's advert is the ordinary case, and the two are rate
/// limited separately because they answer different questions.
pub fn receive(
    bytes: &[u8],
    from_peer: [u8; 32],
    now_ms: u64,
    limits: &mut RateLimiter,
    store: &mut LobbyStore,
) -> Result<[u8; 32], NotAccepted> {
    // Each decode carries its own cap, and the caps nest the way the transports
    // do — advert inside signed event inside lobby message. A decoder given no
    // bound is a decoder an attacker chooses the working set of.
    let signed: SignedEvent = from_canonical(bytes, LOBBY_MSG_MAX)
        .map_err(|_| NotAccepted::Malformed("not a canonical signed event"))?;
    let envelope: EventBody = from_canonical(&signed.body, LOBBY_MSG_MAX)
        .map_err(|_| NotAccepted::Malformed("not a canonical envelope"))?;

    let table_key = envelope.sender_public_key;

    // The **peer's** budget, before the signature: the transport authenticated
    // this identity, and the signature is the expensive part.
    if !limits.admit_peer(from_peer, now_ms) {
        return Err(NotAccepted::RateLimited);
    }

    match envelope.check_envelope() {
        Ok(EventType::LobbyTableAd) => {}
        Ok(_) => return Err(NotAccepted::WrongType),
        Err(_) => return Err(NotAccepted::Malformed("the envelope is not conforming")),
    }
    if envelope.table_id != ZERO32 {
        return Err(NotAccepted::Malformed(
            "a lobby advert carries the unchained sentinel",
        ));
    }

    let verifying = VerifyingKey::from_bytes(&table_key).map_err(|_| NotAccepted::BadSignature)?;
    let signature = Signature::from_bytes(&signed.signature);
    verifying
        .verify_strict(&to_be_signed(&signed.body), &signature)
        .map_err(|_| NotAccepted::BadSignature)?;

    // The **table's** budget, now that the key is more than a claim. Charging it
    // earlier let anyone name any table and burn its allowance four messages a
    // minute, after which the real founder's re-broadcast was rate limited out
    // and the table vanished from every lobby that had heard the forgeries.
    if !limits.admit_table(table_key, now_ms) {
        return Err(NotAccepted::RateLimited);
    }

    let body: AdBody = from_canonical(&envelope.payload, TABLE_AD_MAX)
        .map_err(|_| NotAccepted::Malformed("not a canonical advert body"))?;
    let ad: TableAd = body.try_into()?;

    lobby::admit(&ad, now_ms).map_err(NotAccepted::Refused)?;

    let params = table_params_hash(&ad);
    // The hash of the bytes that arrived, not of a re-encoding: it is what a
    // `JOIN_REQUEST` names, and the founder looks it up against an advert it
    // signed itself.
    let advert_hash = crate::protocol::transcript::event_hash(&signed.body);
    store
        .offer(table_key, ad, params, advert_hash, now_ms)
        .map_err(NotAccepted::NotTaken)?;
    Ok(table_key)
}

/// Re-verify an advertisement handed over **outside** the lobby.
///
/// §4.3 has `JOIN_ACCEPT` repeat the whole `SignedEvent` of the advert verbatim,
/// for one reason: the joiner must not be relying on a gossip copy or on the
/// founder's word for what it is sitting down to. So this is not a convenience
/// wrapper on [`receive`] — it is the second, independent check, and it
/// deliberately shares only the parts that are about the bytes:
///
/// * it takes **no rate limiter and no store**. Those answer lobby questions —
///   who may spend this client's CPU, which advert is the current one — and
///   neither is a question about an advert somebody handed us directly.
/// * it runs **no §7.2 admission**. `lobby::admit` decides what belongs in a
///   list of tables; an accepted echo has already been decided on.
///
/// What it does do is everything the signature covers: canonical decode,
/// envelope catalogue, the unchained sentinel, `verify_strict` under the key in
/// the envelope, and a canonical body. It returns the advert and its
/// `event_hash`, which is what §4.3's comparison against `advert_hash` needs.
/// The advert's body as canonical bytes, for the session record (`S1-CR`):
/// a restarted client rebuilds its `JOIN_REQUEST` from the advert it joined
/// by, and a table that has started is not advertised any more.
pub fn to_body_bytes(ad: &TableAd) -> Result<Vec<u8>, &'static str> {
    to_canonical(&AdBody::from(ad)).map_err(|_| "the advert does not encode")
}

/// The inverse, with the body's own consistency rules and none of the
/// lobby's freshness rules: the record is this client's own file, and the
/// advert in it is stale by construction.
pub fn from_body_bytes(bytes: &[u8]) -> Result<TableAd, NotAccepted> {
    let body: AdBody = from_canonical(bytes, TABLE_AD_MAX)
        .map_err(|_| NotAccepted::Malformed("not a canonical advert body"))?;
    body.try_into()
}

pub fn verify_echoed(bytes: &[u8]) -> Result<(TableAd, [u8; 32]), NotAccepted> {
    let signed: SignedEvent = from_canonical(bytes, LOBBY_MSG_MAX)
        .map_err(|_| NotAccepted::Malformed("not a canonical signed event"))?;
    let envelope: EventBody = from_canonical(&signed.body, LOBBY_MSG_MAX)
        .map_err(|_| NotAccepted::Malformed("not a canonical envelope"))?;

    match envelope.check_envelope() {
        Ok(EventType::LobbyTableAd) => {}
        Ok(_) => return Err(NotAccepted::WrongType),
        Err(_) => return Err(NotAccepted::Malformed("the envelope is not conforming")),
    }
    if envelope.table_id != ZERO32 {
        return Err(NotAccepted::Malformed(
            "a lobby advert carries the unchained sentinel",
        ));
    }

    let verifying = VerifyingKey::from_bytes(&envelope.sender_public_key)
        .map_err(|_| NotAccepted::BadSignature)?;
    let signature = Signature::from_bytes(&signed.signature);
    verifying
        .verify_strict(&to_be_signed(&signed.body), &signature)
        .map_err(|_| NotAccepted::BadSignature)?;

    let body: AdBody = from_canonical(&envelope.payload, TABLE_AD_MAX)
        .map_err(|_| NotAccepted::Malformed("not a canonical advert body"))?;
    let ad: TableAd = body.try_into()?;
    Ok((ad, crate::protocol::transcript::event_hash(&signed.body)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::constants::hand_deadline_min_ms;
    use ed25519_dalek::SigningKey;

    const NOW: u64 = 1_700_000_000_000;

    /// One named edit to an otherwise legal advert.
    type Change = (&'static str, Box<dyn Fn(&mut TableAd)>);

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn ad() -> TableAd {
        let mut a = TableAd {
            game: 1,
            mode: 1,
            preset_id: "CUSTOM".into(),
            table_name: "Riverside".into(),
            small_blind: 10,
            big_blind: 20,
            ante: 0,
            min_buyin: 200,
            max_buyin: 2_000,
            start_stack: 0,
            players: 1,
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
            hand_deadline_ms: 0,
            join_deadline_ms: 120_000,
            hand_delay_ms: 7_000,
            time_bank_ms: 0,
            button_rule: 1,
            odd_chip_rule: 1,
            showdown_policy: 1,
            password_required: false,
            deck_suite: lobby::DECK_SUITE_V1.into(),
            founder_app_key: [7u8; 32],
            founder_peer_id: b"12D3KooWfake".to_vec(),
            timestamp_unix_ms: NOW,
            expires_at_unix_ms: NOW + 90_000,
            founder_tox_key: None,
            tox_chat_id: None,
        };
        a.hand_deadline_ms = hand_deadline_min_ms(
            a.max_players,
            a.action_timeout_ms as u64,
            a.action_grace_ms as u64,
            a.crypto_step_timeout_ms as u64,
            a.hand_delay_ms as u64,
        0,
        ) as u32;
        a
    }

    /// The whole loop, which is what D-003 needs and what did not exist: a table
    /// is signed, put on the wire, and arrives at another client as a table.
    /// The hash the lobby keeps is the hash a joiner puts in its
    /// `JOIN_REQUEST`.
    ///
    /// Without it the lobby was a complete list of tables and no way to sit down
    /// at one: §4.3's `n(0) advert_hash` names the advert being joined, and the
    /// store had thrown the bytes away. A gap that only appears when the two
    /// halves are finally joined up.
    #[test]
    fn the_store_keeps_the_hash_a_join_request_needs() {
        let table = key(7);
        let wire = publish(&ad(), &table).unwrap();
        let mut store = LobbyStore::new();
        let mut limits = RateLimiter::new();
        let seen = receive(&wire, [9u8; 32], NOW, &mut limits, &mut store).unwrap();

        let signed: SignedEvent = from_canonical(&wire, LOBBY_MSG_MAX).unwrap();
        let expected = crate::protocol::transcript::event_hash(&signed.body);
        assert_eq!(store.get(&seen).unwrap().advert_hash, expected);

        // And it is the hash of the copy this client accepted: a re-broadcast
        // carries a later timestamp and therefore a different hash, which is
        // exactly why the *parameters* hash is the one joiners must agree on.
        let mut later = ad();
        later.timestamp_unix_ms += 30_000;
        later.expires_at_unix_ms += 30_000;
        let again = publish(&later, &table).unwrap();
        receive(&again, [9u8; 32], NOW + 30_000, &mut limits, &mut store).unwrap();
        let held = store.get(&seen).unwrap();
        assert_ne!(held.advert_hash, expected, "the hash follows the copy held");
        assert_eq!(held.params_hash, table_params_hash(&ad()), "the parameters do not");
    }

    /// The echoed advert in a `JOIN_ACCEPT` is verified from its own bytes, and
    /// one altered byte anywhere in it is refused. A joiner that took the
    /// founder's word here would be sitting down to whatever the founder said.
    #[test]
    fn an_echoed_advert_is_verified_from_its_own_bytes() {
        let table = key(7);
        let wire = publish(&ad(), &table).unwrap();
        let (back, hash) = verify_echoed(&wire).expect("an honest echo verifies");
        assert_eq!(back.table_name, "Riverside");
        assert_ne!(hash, ZERO32);

        let mut refused = 0;
        for i in 0..wire.len() {
            let mut bad = wire.clone();
            bad[i] ^= 0x01;
            if verify_echoed(&bad).is_err() {
                refused += 1;
            }
        }
        assert_eq!(refused, wire.len(), "an altered echo was accepted");
    }

    /// And its hash is the same one the lobby computes for the same bytes, or
    /// §4.3's comparison against `advert_hash` compares two different things.
    #[test]
    fn the_echoed_hash_is_the_lobbys_hash() {
        let table = key(7);
        let wire = publish(&ad(), &table).unwrap();
        let (_, echoed) = verify_echoed(&wire).unwrap();
        let signed: SignedEvent = from_canonical(&wire, LOBBY_MSG_MAX).unwrap();
        assert_eq!(
            echoed,
            crate::protocol::transcript::event_hash(&signed.body)
        );
    }

    #[test]
    fn a_table_travels_from_one_client_to_another() {
        let table = key(1);
        let wire = publish(&ad(), &table).expect("an advert publishes");

        let mut store = LobbyStore::new();
        let mut limits = RateLimiter::new();
        let seen = receive(&wire, [9u8; 32], NOW, &mut limits, &mut store)
            .expect("an honest advert arrives");

        assert_eq!(seen, table.verifying_key().to_bytes());
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(&seen).unwrap().ad.table_name, "Riverside");
        assert_eq!(store.joinable().count(), 1);
    }

    /// The identity is the key that signed it and nothing else, so two receivers
    /// reading the same bytes file the same table under the same name.
    #[test]
    fn the_table_is_named_by_its_key() {
        let wire_a = publish(&ad(), &key(1)).unwrap();
        let wire_b = publish(&ad(), &key(2)).unwrap();

        let mut store = LobbyStore::new();
        let mut limits = RateLimiter::new();
        receive(&wire_a, [9u8; 32], NOW, &mut limits, &mut store).unwrap();
        receive(&wire_b, [9u8; 32], NOW, &mut limits, &mut store).unwrap();
        assert_eq!(store.len(), 2, "one key, one table");
    }

    /// A flipped byte anywhere in the advert fails the signature. `verify_strict`
    /// and never `verify`: the permissive form accepts small-order and
    /// non-canonical keys, and a signature two implementations disagree about is
    /// worse than one that is plainly invalid.
    #[test]
    fn a_tampered_advert_does_not_verify() {
        let wire = publish(&ad(), &key(1)).unwrap();

        let mut broken = 0;
        for i in (0..wire.len()).step_by(7) {
            let mut bad = wire.clone();
            bad[i] ^= 0x01;
            let mut store = LobbyStore::new();
            let mut limits = RateLimiter::new();
            if receive(&bad, [9u8; 32], NOW, &mut limits, &mut store).is_err() {
                broken += 1;
            }
            assert!(store.is_empty(), "a tampered advert reached the lobby");
        }
        assert!(broken > 5, "the corpus did not exercise the check");
    }


    /// What the two signature checks actually do in `ed25519-dalek` 3.0.0.
    ///
    /// **This test exists because the comment it replaces was wrong.** That
    /// comment said `verify_strict` rejects small-order keys *that the
    /// permissive one accepts*, which is the folklore and is the classic
    /// universal forgery: the all-zero key is the identity, of small order, and
    /// under the cofactored equation `[8s]B = [8]R + [8k]A` a zero signature
    /// satisfies it for any message, with no secret and no computation.
    ///
    /// **In this version both checks reject it**, measured here rather than
    /// assumed. `is_weak()` is true for the identity and `verify` refuses it
    /// too, so the attack does not reproduce against `ed25519-dalek` 3.0.0.
    ///
    /// `verify_strict` stays, for two reasons that survive the correction.
    /// `PROTOCOL.md` requires it by name, so using anything else would make this
    /// client non-conforming whatever the library does. And a property that
    /// holds because a dependency currently chooses to be strict is not a
    /// property to rest on: the strict form is the one whose contract says so.
    ///
    /// If a future version loosens `verify`, this test starts failing and says
    /// which half changed.
    #[test]
    fn what_the_two_signature_checks_actually_do() {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};

        let identity = VerifyingKey::from_bytes(&[0u8; 32])
            .expect("the identity is a well-formed point encoding");
        let zero_sig = Signature::from_bytes(&[0u8; 64]);

        assert!(identity.is_weak(), "the identity is of small order");
        assert!(
            identity.verify(b"any message at all", &zero_sig).is_err(),
            "measured: 3.0.0's permissive check refuses this too"
        );
        assert!(identity
            .verify_strict(b"any message at all", &zero_sig)
            .is_err());

        // And the forgery does not reach the lobby through `receive`, which is
        // the property that matters whichever check the library uses.
        let body = to_canonical(&AdBody::from(&ad())).unwrap();
        let envelope =
            EventBody::unchained(EventType::LobbyTableAd, [0u8; 32], body, NOW).unwrap();
        let wire = to_canonical(&SignedEvent {
            body: to_canonical(&envelope).unwrap(),
            signature: [0u8; 64],
        })
        .unwrap();

        let mut store = LobbyStore::new();
        let mut limits = RateLimiter::new();
        assert_eq!(
            receive(&wire, [9u8; 32], NOW, &mut limits, &mut store),
            Err(NotAccepted::BadSignature)
        );
        assert!(store.is_empty(), "a table nobody signed reached the lobby");
    }


    /// The attack the split fixes, end to end on the wire.
    ///
    /// A hostile peer names somebody else's table key in an advert it cannot
    /// sign. Before the split, that spent the real table's four-per-minute
    /// budget, so the founder's own re-broadcast was refused, the held advert
    /// expired, and the table left every lobby that had heard the forgeries —
    /// for the price of four unsigned messages a minute.
    #[test]
    fn a_forged_key_cannot_spend_the_real_tables_budget() {
        let real = key(1);
        let real_key = real.verifying_key().to_bytes();
        let mut store = LobbyStore::new();
        let mut limits = RateLimiter::new();

        // The attacker signs with its own key and claims to be the table by
        // putting the victim's key in the envelope. That fails `verify_strict`.
        let honest = publish(&ad(), &real).unwrap();
        let mut forged: SignedEvent = from_canonical(&honest, LOBBY_MSG_MAX).unwrap();
        forged.signature[0] ^= 0x01;
        let forged = to_canonical(&forged).unwrap();

        for i in 0..20u32 {
            // A fresh peer each time, so the per-peer limit is not what stops it.
            let mut attacker = [0u8; 32];
            attacker[..4].copy_from_slice(&i.to_be_bytes());
            assert_eq!(
                receive(&forged, attacker, NOW, &mut limits, &mut store),
                Err(NotAccepted::BadSignature),
                "the forgery is refused, which was never in doubt"
            );
        }

        // And the real founder can still advertise, which is the point.
        let mut later = ad();
        later.timestamp_unix_ms += 1_000;
        let fresh = publish(&later, &real).unwrap();
        assert_eq!(
            receive(&fresh, [0xAA; 32], NOW, &mut limits, &mut store),
            Ok(real_key),
            "twenty forgeries must not have spent the real table's allowance"
        );
    }

    /// The rate limit runs before the signature, so a flood costs this client
    /// nothing but a map lookup.
    #[test]
    fn a_flood_is_stopped_before_any_signature_is_checked() {
        let wire = publish(&ad(), &key(1)).unwrap();
        let mut store = LobbyStore::new();
        let mut limits = RateLimiter::new();

        let mut refused = 0;
        for _ in 0..50 {
            if let Err(NotAccepted::RateLimited) =
                receive(&wire, [9u8; 32], NOW, &mut limits, &mut store)
            {
                refused += 1;
            }
        }
        assert!(refused > 0, "the limiter never fired");
    }

    /// The six things section 3.1 excludes **by name**, each checked separately.
    ///
    /// The Tox fields survive the wire in both states, and `on_tox` is what
    /// puts them there.
    ///
    /// Both are `Option` because a build without `--features tox` advertises a
    /// table it carries on the swarm and must not invent a key it does not
    /// have. Absent means *this table is not on Tox*, and that has to stay
    /// distinguishable from thirty-two zero bytes, which is a key.
    #[test]
    fn the_tox_fields_survive_the_wire_in_both_states() {
        let plain = ad();
        assert_eq!(
            plain.founder_tox_key, None,
            "a table is not on Tox unless it says so"
        );
        assert_eq!(plain.tox_chat_id, None);

        let key = [0xA1u8; 32];
        let chat = [0xB2u8; 32];
        let on_tox = plain.clone().on_tox(key, chat);
        assert_eq!(on_tox.founder_tox_key, Some(key));
        assert_eq!(on_tox.tox_chat_id, Some(chat));

        for a in [&plain, &on_tox] {
            let body = AdBody::from(a);
            let bytes = minicbor::to_vec(&body).expect("it encodes");
            let back: AdBody = minicbor::decode(&bytes).expect("and decodes");
            let round: TableAd = back.try_into().expect("into an advert");
            assert_eq!(&round, a, "and nothing moved on the way");
        }

        // An advert naming a Tox group is still under the lobby's cap: the two
        // fields are sixty-six bytes and there is not much room to spare.
        let bytes = minicbor::to_vec(AdBody::from(&on_tox)).expect("it encodes");
        assert!(
            bytes.len() <= crate::protocol::constants::TABLE_AD_MAX,
            "an advert on Tox is {} bytes, over the {} cap",
            bytes.len(),
            crate::protocol::constants::TABLE_AD_MAX
        );
    }

    /// The first version of `table_params_hash` hashed the encoded body with
    /// three of them zeroed, and `table_name` was not one of the three. That was
    /// a live bug with no adversary in it: a founder who fixes a typo in the
    /// table's name changes the digest, rule 7 sees changed parameters, and
    /// every client holding the table marks it permanently unjoinable. Section
    /// 3.1 excludes it for exactly this reason and says so.
    #[test]
    fn the_excluded_fields_are_excluded() {
        let base = ad();
        let excluded: Vec<Change> = vec![
            (
                "table_name",
                Box::new(|a: &mut TableAd| a.table_name = "Riverside (opraveno)".into()),
            ),
            ("players", Box::new(|a: &mut TableAd| a.players = 4)),
            (
                "password_required",
                Box::new(|a: &mut TableAd| a.password_required = true),
            ),
            (
                "founder_app_key",
                Box::new(|a: &mut TableAd| a.founder_app_key = [9u8; 32]),
            ),
            (
                "founder_peer_id",
                Box::new(|a: &mut TableAd| a.founder_peer_id = b"somewhere else".to_vec()),
            ),
            (
                // **The pair that would have been the same bug again.** A
                // founder that restarts comes back with a different Tox
                // identity and a different group, which is legitimate - and if
                // either were hashed, rule 7 would see changed parameters and
                // every client holding the table would mark it permanently
                // unjoinable. That is `table_name`'s defect with a different
                // field in it, which is why both sit beside `founder_peer_id`
                // among section 3.1's exclusions rather than inside the digest.
                "founder_tox_key",
                Box::new(|a: &mut TableAd| a.founder_tox_key = Some([7u8; 32])),
            ),
            (
                "tox_chat_id",
                Box::new(|a: &mut TableAd| a.tox_chat_id = Some([8u8; 32])),
            ),
            (
                "the timestamps",
                Box::new(|a: &mut TableAd| {
                    a.timestamp_unix_ms += 30_000;
                    a.expires_at_unix_ms += 30_000;
                }),
            ),
        ];

        for (what, mutate) in excluded {
            let mut other = base.clone();
            mutate(&mut other);
            assert_eq!(
                table_params_hash(&base),
                table_params_hash(&other),
                "{what} is excluded by section 3.1 and must not move the digest"
            );
        }
    }

    /// And every part that IS in the box moves it, one at a time.
    ///
    /// Twenty-five parts means twenty-five ways for two clients to be playing
    /// different games, and a part that silently failed to enter would be a rule
    /// nobody was comparing.
    #[test]
    fn every_included_part_moves_the_digest() {
        let base = ad();
        let included: Vec<Change> = vec![
            ("game", Box::new(|a: &mut TableAd| a.game = 2)),
            ("mode", Box::new(|a: &mut TableAd| a.mode = 2)),
            ("preset_id", Box::new(|a: &mut TableAd| a.preset_id = "RATED_SNG_POKERTH_V1".into())),
            ("small_blind", Box::new(|a: &mut TableAd| a.small_blind = 25)),
            ("big_blind", Box::new(|a: &mut TableAd| a.big_blind = 40)),
            ("ante", Box::new(|a: &mut TableAd| a.ante = 1)),
            ("min_buyin", Box::new(|a: &mut TableAd| a.min_buyin = 300)),
            ("max_buyin", Box::new(|a: &mut TableAd| a.max_buyin = 3_000)),
            ("start_stack", Box::new(|a: &mut TableAd| a.start_stack = 500)),
            ("max_players", Box::new(|a: &mut TableAd| a.max_players = 9)),
            ("min_players_to_start", Box::new(|a: &mut TableAd| a.min_players_to_start = 3)),
            ("schedule mode", Box::new(|a: &mut TableAd| a.blind_schedule.mode = 2)),
            ("schedule interval", Box::new(|a: &mut TableAd| a.blind_schedule.every_n_hands = 21)),
            ("schedule first level", Box::new(|a: &mut TableAd| a.blind_schedule.first_small_blind = 11)),
            ("schedule cap", Box::new(|a: &mut TableAd| a.blind_schedule.small_blind_cap = 2_000)),
            ("action_timeout_ms", Box::new(|a: &mut TableAd| a.action_timeout_ms = 25_000)),
            ("action_grace_ms", Box::new(|a: &mut TableAd| a.action_grace_ms = 6_000)),
            ("crypto_step_timeout_ms", Box::new(|a: &mut TableAd| a.crypto_step_timeout_ms = 31_000)),
            ("hand_deadline_ms", Box::new(|a: &mut TableAd| a.hand_deadline_ms += 1_000)),
            ("join_deadline_ms", Box::new(|a: &mut TableAd| a.join_deadline_ms = 130_000)),
            ("hand_delay_ms", Box::new(|a: &mut TableAd| a.hand_delay_ms = 8_000)),
            ("button_rule", Box::new(|a: &mut TableAd| a.button_rule = 2)),
            ("odd_chip_rule", Box::new(|a: &mut TableAd| a.odd_chip_rule = 2)),
            ("showdown_policy", Box::new(|a: &mut TableAd| a.showdown_policy = 2)),
            ("deck_suite", Box::new(|a: &mut TableAd| a.deck_suite = "bs-bg12-secp256k1/2".into())),
            ("time_bank_ms", Box::new(|a: &mut TableAd| a.time_bank_ms = 30_000)),
        ];
        assert_eq!(included.len(), 26, "the box has twenty-six parts");

        let base_hash = table_params_hash(&base);
        for (what, mutate) in included {
            let mut other = base.clone();
            mutate(&mut other);
            assert_ne!(
                base_hash,
                table_params_hash(&other),
                "{what} is in the box and must move the digest"
            );
        }
    }

    /// The body is the array section 7.2 specifies: twenty-nine fields with a
    /// nested schedule at n(13). The first version flattened the schedule and
    /// shifted every later index by three, which two clients would each decode
    /// into well-formed nonsense.
    #[test]
    fn the_body_is_the_arrangement_the_protocol_specifies() {
        let body = AdBody::from(&ad());
        let bytes = to_canonical(&body).unwrap();

        // A CBOR array of 30 elements: major type 4, count in one extra byte.
        // Thirty because `time_bank_ms` is `n(29)`; the derive encodes indices
        // `0..=29` positionally, so the count **is** the highest index plus
        // one and a field added anywhere but the end would move every later
        // one. That is what this assertion is here to catch.
        assert_eq!(
            &bytes[..2],
            &[0x98, 0x1E],
            "thirty fields, and each at the index the protocol gives it"
        );
        // And n(13) is itself an array of four.
        assert!(bytes.contains(&0x84), "the schedule is nested, not flattened");

        let back: AdBody = from_canonical(&bytes, TABLE_AD_MAX).unwrap();
        assert_eq!(back, body);
    }

    /// Bytes, not text, for the three display fields. The difference is a
    /// major-type byte in the preimage of every hash over them, and it is the
    /// protocol that chooses which.
    #[test]
    fn the_display_fields_are_bytes_on_the_wire() {
        let bytes = to_canonical(&AdBody::from(&ad())).unwrap();
        let as_byte_string = [0x46u8, b'C', b'U', b'S', b'T', b'O', b'M'];
        let as_text = [0x66u8, b'C', b'U', b'S', b'T', b'O', b'M'];
        assert!(
            bytes.windows(7).any(|w| w == as_byte_string),
            "preset_id is a byte string"
        );
        assert!(
            !bytes.windows(7).any(|w| w == as_text),
            "and not a text string"
        );
    }

    /// A body whose display bytes are not UTF-8 is refused rather than lossily
    /// converted: a lossy conversion turns a malformed advert into a well-formed
    /// one with a different name at every receiver that used a different rule.
    #[test]
    fn a_display_field_that_is_not_utf8_is_refused() {
        let mut body = AdBody::from(&ad());
        body.table_name = vec![0xFF, 0xFE, 0xFD];
        assert!(matches!(
            TableAd::try_from(body),
            Err(NotAccepted::Malformed(_))
        ));
    }

    /// The whole of rule 7, end to end on the wire: a founder re-signing with a
    /// different blind hands two joiners two rule sets, so the table is marked
    /// unjoinable and the changed advert is discarded.
    #[test]
    fn a_founder_who_changes_the_game_loses_the_table() {
        let table = key(1);
        let mut store = LobbyStore::new();
        let mut limits = RateLimiter::new();

        let first = publish(&ad(), &table).unwrap();
        let seen = receive(&first, [9u8; 32], NOW, &mut limits, &mut store).unwrap();
        assert_eq!(store.joinable().count(), 1);

        let mut changed = ad();
        changed.timestamp_unix_ms += 1_000;
        changed.small_blind = 25;
        changed.big_blind = 50;
        changed.blind_schedule.first_small_blind = 25;
        let second = publish(&changed, &table).unwrap();

        assert_eq!(
            receive(&second, [8u8; 32], NOW, &mut limits, &mut store),
            Err(NotAccepted::NotTaken(NotTaken::ParametersChanged))
        );
        assert!(store.get(&seen).unwrap().unjoinable);
        assert_eq!(store.get(&seen).unwrap().ad.small_blind, 10);
    }

    /// A hostile advert is refused at the joiner, which is where §7.2 puts the
    /// check: the value is the founder's, and a founder who wants a table where
    /// nothing can ever be won needs only a small number.
    #[test]
    fn a_hostile_advert_is_refused_on_arrival() {
        let mut hostile = ad();
        hostile.hand_deadline_ms = 5_000;
        let wire = publish(&hostile, &key(1)).unwrap();

        let mut store = LobbyStore::new();
        let mut limits = RateLimiter::new();
        assert!(matches!(
            receive(&wire, [9u8; 32], NOW, &mut limits, &mut store),
            Err(NotAccepted::Refused(AdRejected::DeadlineTooShort { .. }))
        ));
        assert!(store.is_empty());
    }

    /// An advert fits its cap. The caps nest — advert inside signed event inside
    /// lobby message inside gossip frame — and that nesting is asserted at
    /// compile time in `constants`; this checks the first one against a real
    /// advert rather than against a guess.
    #[test]
    fn a_real_advert_fits_the_cap() {
        let body = to_canonical(&AdBody::from(&ad())).unwrap();
        assert!(
            body.len() <= TABLE_AD_MAX,
            "an advert body is {} bytes against a cap of {TABLE_AD_MAX}",
            body.len()
        );
        let wire = publish(&ad(), &key(1)).unwrap();
        assert!(
            wire.len() <= crate::protocol::constants::TABLE_AD_SIGNED_MAX,
            "a signed advert is {} bytes",
            wire.len()
        );
    }

    /// One advert is one byte string, so the duplicate cache and the message id
    /// mean what they say.
    #[test]
    fn one_advert_is_one_encoding() {
        let a = publish(&ad(), &key(1)).unwrap();
        let b = publish(&ad(), &key(1)).unwrap();
        assert_eq!(a, b, "signing is deterministic and so is the encoding");
    }
}
