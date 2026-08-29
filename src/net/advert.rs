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
//! 1. **Rate limit**, by sending peer and by table key. Free, and it runs
//!    **before** the signature — a limiter that ran after verification would let
//!    a peer spend this client's CPU at will, which is the thing it exists to
//!    prevent.
//! 2. **Decode**, canonically. The bytes are re-encoded and compared, so one
//!    advert is one byte string and the duplicate cache and the message id mean
//!    what they say.
//! 3. **`verify_strict`**, never `verify` — for a reason narrower than the
//!    folklore, and the narrower reason is the measured one. See
//!    [`what_the_two_signature_checks_actually_do`](tests::what_the_two_signature_checks_actually_do).
//! 4. **§7.2 rules 2 to 5**, which is [`lobby::admit`].
//! 5. **§7.2 rules 6 and 7**, which need the advert already held and are
//!    [`LobbyStore`]'s.
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
/// A separate type from [`TableAd`] on purpose. This one is arrays and integers
/// and exists to be encoded; that one is what the admission rules read. Keeping
/// them apart is what stops a decoding convenience from quietly becoming a
/// protocol change.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cbor(array)]
pub struct AdBody {
    #[n(0)]
    pub game: u16,
    #[n(1)]
    pub mode: u16,
    #[n(2)]
    pub preset_id: String,
    #[n(3)]
    pub table_name: String,
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
    pub schedule_mode: u16,
    #[n(14)]
    pub every_n_hands: u16,
    #[n(15)]
    pub first_small_blind: u64,
    #[n(16)]
    pub small_blind_cap: u64,
    #[n(17)]
    pub action_timeout_ms: u32,
    #[n(18)]
    pub action_grace_ms: u32,
    #[n(19)]
    pub crypto_step_timeout_ms: u32,
    #[n(20)]
    pub hand_deadline_ms: u32,
    #[n(21)]
    pub join_deadline_ms: u32,
    #[n(22)]
    pub hand_delay_ms: u32,
    #[n(23)]
    pub button_rule: u16,
    #[n(24)]
    pub odd_chip_rule: u16,
    #[n(25)]
    pub showdown_policy: u16,
    #[n(26)]
    pub password_required: bool,
    #[n(27)]
    pub deck_suite: String,
    #[cbor(n(28), with = "minicbor::bytes")]
    pub founder_app_key: [u8; 32],
    #[cbor(n(29), with = "minicbor::bytes")]
    pub founder_peer_id: Vec<u8>,
    #[n(30)]
    pub timestamp_unix_ms: u64,
    #[n(31)]
    pub expires_at_unix_ms: u64,
}

impl From<&TableAd> for AdBody {
    fn from(a: &TableAd) -> Self {
        AdBody {
            game: a.game,
            mode: a.mode,
            preset_id: a.preset_id.clone(),
            table_name: a.table_name.clone(),
            small_blind: a.small_blind,
            big_blind: a.big_blind,
            ante: a.ante,
            min_buyin: a.min_buyin,
            max_buyin: a.max_buyin,
            start_stack: a.start_stack,
            players: a.players,
            max_players: a.max_players,
            min_players_to_start: a.min_players_to_start,
            schedule_mode: a.blind_schedule.mode,
            every_n_hands: a.blind_schedule.every_n_hands,
            first_small_blind: a.blind_schedule.first_small_blind,
            small_blind_cap: a.blind_schedule.small_blind_cap,
            action_timeout_ms: a.action_timeout_ms,
            action_grace_ms: a.action_grace_ms,
            crypto_step_timeout_ms: a.crypto_step_timeout_ms,
            hand_deadline_ms: a.hand_deadline_ms,
            join_deadline_ms: a.join_deadline_ms,
            hand_delay_ms: a.hand_delay_ms,
            button_rule: a.button_rule,
            odd_chip_rule: a.odd_chip_rule,
            showdown_policy: a.showdown_policy,
            password_required: a.password_required,
            deck_suite: a.deck_suite.clone(),
            founder_app_key: a.founder_app_key,
            founder_peer_id: a.founder_peer_id.clone(),
            timestamp_unix_ms: a.timestamp_unix_ms,
            expires_at_unix_ms: a.expires_at_unix_ms,
        }
    }
}

impl From<AdBody> for TableAd {
    fn from(b: AdBody) -> Self {
        TableAd {
            game: b.game,
            mode: b.mode,
            preset_id: b.preset_id,
            table_name: b.table_name,
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
                mode: b.schedule_mode,
                every_n_hands: b.every_n_hands,
                first_small_blind: b.first_small_blind,
                small_blind_cap: b.small_blind_cap,
            },
            action_timeout_ms: b.action_timeout_ms,
            action_grace_ms: b.action_grace_ms,
            crypto_step_timeout_ms: b.crypto_step_timeout_ms,
            hand_deadline_ms: b.hand_deadline_ms,
            join_deadline_ms: b.join_deadline_ms,
            hand_delay_ms: b.hand_delay_ms,
            button_rule: b.button_rule,
            odd_chip_rule: b.odd_chip_rule,
            showdown_policy: b.showdown_policy,
            password_required: b.password_required,
            deck_suite: b.deck_suite,
            founder_app_key: b.founder_app_key,
            founder_peer_id: b.founder_peer_id,
            timestamp_unix_ms: b.timestamp_unix_ms,
            expires_at_unix_ms: b.expires_at_unix_ms,
        }
    }
}

/// `table_params_hash` of `PROTOCOL.md` §3.1, over the parameters rule 7
/// compares.
///
/// Over the advert's **canonical bytes** minus the two timestamps, so a
/// re-broadcast that changed nothing but the clock hashes the same and one that
/// changed a blind does not. It is computed from the encoded body rather than
/// field by field, because a field-by-field version is a second enumeration of
/// the same list and those drift.
pub fn table_params_hash(ad: &TableAd) -> Hash {
    let mut stable = AdBody::from(ad);
    stable.players = 0;
    stable.timestamp_unix_ms = 0;
    stable.expires_at_unix_ms = 0;
    let bytes = to_canonical(&stable).expect("an advert body encodes");
    h(Domain::TableParams.context(), &[&bytes])
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

    // Before the signature, which is the expensive part.
    if !limits.admit_ad(from_peer, table_key, now_ms) {
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

    let body: AdBody = from_canonical(&envelope.payload, TABLE_AD_MAX)
        .map_err(|_| NotAccepted::Malformed("not a canonical advert body"))?;
    let ad: TableAd = body.into();

    lobby::admit(&ad, now_ms).map_err(NotAccepted::Refused)?;

    let params = table_params_hash(&ad);
    store
        .offer(table_key, ad, params, now_ms)
        .map_err(NotAccepted::NotTaken)?;
    Ok(table_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::constants::hand_deadline_min_ms;
    use ed25519_dalek::SigningKey;

    const NOW: u64 = 1_700_000_000_000;

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
            button_rule: 1,
            odd_chip_rule: 1,
            showdown_policy: 1,
            password_required: false,
            deck_suite: lobby::DECK_SUITE_V1.into(),
            founder_app_key: [7u8; 32],
            founder_peer_id: b"12D3KooWfake".to_vec(),
            timestamp_unix_ms: NOW,
            expires_at_unix_ms: NOW + 90_000,
        };
        a.hand_deadline_ms = hand_deadline_min_ms(
            a.max_players,
            a.action_timeout_ms as u64,
            a.action_grace_ms as u64,
            a.crypto_step_timeout_ms as u64,
            a.hand_delay_ms as u64,
        ) as u32;
        a
    }

    /// The whole loop, which is what D-003 needs and what did not exist: a table
    /// is signed, put on the wire, and arrives at another client as a table.
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

    /// Rule 7's comparison needs a hash that moves with the parameters and not
    /// with the clock, or every honest re-broadcast would mark its own table
    /// unjoinable.
    #[test]
    fn the_params_hash_ignores_the_clock_and_the_seat_count() {
        let base = ad();
        let mut later = base.clone();
        later.timestamp_unix_ms += 30_000;
        later.expires_at_unix_ms += 30_000;
        later.players = 4;
        assert_eq!(
            table_params_hash(&base),
            table_params_hash(&later),
            "a re-broadcast that changed nothing but time and seats"
        );

        let mut changed = base.clone();
        changed.small_blind = 25;
        assert_ne!(
            table_params_hash(&base),
            table_params_hash(&changed),
            "and one that changed the game"
        );
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
