//! Where the deck becomes cards: the seam between `mental_poker` and `poker`.
//!
//! Everything below this module is either cryptography that does not know what a
//! flop is, or a poker engine that does not know what a ciphertext is. This is
//! the one place both are named, and therefore the one place an integration
//! defect can live.
//!
//! # What it does, and the order it does it in
//!
//! A share arriving from the network passes three gates, cheapest first:
//!
//! 1. **Is it due?** [`entitlement`] — the street has been reached, and the
//!    sender is not publishing its own hole card's share. Free.
//! 2. **Does it verify** against *this* peer's own final deck? ~0.1 ms.
//! 3. **Does the set want it?** One share per seat per card, never an update.
//!
//! Only a complete set opens a card, and a set is complete at `n` shares — every
//! seat, the owner of a hole card included. That last point is the one this
//! module exists to get right: a set sized `n - 1` would call itself complete one
//! share early and turn an ordinary "not yet" into C-10's soundness fault, which
//! is meant to be unreachable without a real failure of the construction.
//!
//! # What it deliberately does not do
//!
//! It does not decide *when* to publish a share, and it does not publish one.
//! [`Dealing::own_share`] computes this peer's own and hands it back; whether it
//! goes on the wire is the protocol layer's. There is no *to whom*: every legal
//! share is broadcast (`PROTOCOL.md` §4.6), every peer keeps every share it
//! receives, and that is what makes a showdown one message per revealing seat.

use std::collections::BTreeMap;

use crate::mental_poker::backend::{
    HandDeck, HandSecret, VerifiedKey, VerifiedToken, WireToken, WireTokenProof,
};
use crate::mental_poker::deck::{CardIndex, DeckIndexMap, Role};
use crate::mental_poker::protocol::{Ciphertext, DeckCtx, Final, VerifyOutcome, Verified};
use crate::mental_poker::reveal::{
    entitlement, NotEntitled, RevealStage, SetError,
    SoundnessFault, TokenSet,
};
use crate::poker::state::{Card, SeatIdx, Street};

/// The final deck, as this module receives it.
pub type FinalDeck = Final<Verified<Vec<Ciphertext>>>;

/// Why a share was not taken.
#[derive(Debug, Clone, PartialEq)]
pub enum Refused {
    /// It was not due: the street has not been reached, the index has no role
    /// this hand, or the sender is the card's owner publishing its own share
    /// before showdown — which is the one rule hole-card privacy rests on.
    NotDue(NotEntitled),
    /// It did not verify against this peer's own final deck, or the check could
    /// not be run at all — [`VerifyOutcome`] carries which, and only the first
    /// is evidence.
    DidNotVerify(VerifyOutcome),
    /// The set already has a share from that seat, or wants none from it.
    NotWanted(SetError),
    /// The seat has no verified key here, so nothing can be checked against it.
    ///
    /// Never evidence: it is a statement about what this peer holds.
    UnknownSeat { seat: SeatIdx },
    /// The index has no role this hand.
    NoSuchCard { index: u8 },
}

/// Why a card could not be opened.
#[derive(Debug, Clone, PartialEq)]
pub enum NotOpenable {
    /// Shares are still missing. Ordinary, and not a fault.
    Waiting { outstanding: Vec<SeatIdx> },
    /// The index has no role this hand.
    NoSuchCard { index: u8 },
    /// Every share is in and verified, and the card still did not come out.
    ///
    /// This is the one that is **not** ordinary. See [`SoundnessFault`].
    Fault(SoundnessFault),
}

/// The three things every share is checked against, which are fixed for a hand.
///
/// Bundled rather than passed one by one — not for tidiness, but because they
/// belong together: a share verified against one peer's deck and another peer's
/// key set is not verified against anything. Keeping them in one value is what
/// makes that combination unwritable.
pub struct Deal<'a> {
    pub hand: &'a HandDeck,
    /// The verified key of each seat, indexed by seat.
    pub keys: &'a [Option<VerifiedKey>],
    /// The last deck of the completed shuffle chain, and no other.
    pub deck: &'a FinalDeck,
}

/// This peer's own seat, key and secret.
///
/// Together, because a share computed with one seat's secret and presented under
/// another's key verifies nowhere and is a mistake that would otherwise compile.
pub struct Identity<'a> {
    pub seat: SeatIdx,
    pub key: &'a VerifiedKey,
    pub secret: &'a HandSecret,
}

/// A share as it arrives from the network.
pub struct Share<'a> {
    pub from: SeatIdx,
    pub index: CardIndex,
    pub token: WireToken,
    pub proof: &'a WireTokenProof,
}

/// One hand's dealing state.
pub struct Dealing {
    map: DeckIndexMap,
    stage: RevealStage,
    /// Every seat whose key is in the aggregate. The share count is `n`, and
    /// this is `n`.
    seats: Vec<SeatIdx>,
    sets: BTreeMap<u8, TokenSet<VerifiedToken>>,
    opened: BTreeMap<u8, Card>,
}

impl Dealing {
    /// Open the dealing state for a hand.
    ///
    /// One set per index the hand uses, each expecting every seat. Indices with
    /// no role get no set, which is the other half of no token for them ever
    /// being legal.
    pub fn new(map: DeckIndexMap, seats: Vec<SeatIdx>) -> Self {
        let mut sets = BTreeMap::new();
        for i in 0..52u8 {
            if let Some(index) = map.index_from_wire(i) {
                sets.insert(i, TokenSet::awaiting(index, seats.clone()));
            }
        }
        Dealing {
            map,
            stage: RevealStage::Betting(Street::PreFlop),
            seats,
            sets,
            opened: BTreeMap::new(),
        }
    }

    pub fn map(&self) -> &DeckIndexMap {
        &self.map
    }

    pub fn stage(&self) -> &RevealStage {
        &self.stage
    }

    /// Move to a later street, or to showdown.
    ///
    /// Monotone in the street: a stage cannot go backwards, because doing so
    /// would make a share that was due stop being due and a card that was
    /// opened stop being openable — and the engine has already acted on it.
    pub fn advance(&mut self, stage: RevealStage) -> bool {
        let now = match &self.stage {
            RevealStage::Betting(s) => *s as u8,
            RevealStage::Showdown { .. } => u8::MAX,
        };
        let next = match &stage {
            RevealStage::Betting(s) => *s as u8,
            RevealStage::Showdown { .. } => u8::MAX,
        };
        if next < now {
            return false;
        }
        self.stage = stage;
        true
    }

    /// Where a share for `index` should be sent, if this peer may issue one.
    ///
    /// The protocol layer's question, answered here so it has one answer.
    pub fn due(&self, index: CardIndex, from: SeatIdx) -> Result<(), NotEntitled> {
        entitlement(&self.map, index, from, &self.stage)
    }

    /// Compute and record **this** peer's own share.
    ///
    /// Never published for a hole card of this seat — it is the share nobody
    /// else can supply, which is the whole of why the card is private — and the
    /// caller is what decides that, from [`due`](Self::due).
    pub fn own_share(
        &mut self,
        deal: &Deal<'_>,
        me: &Identity<'_>,
        index: CardIndex,
        ctx: &DeckCtx,
    ) -> Result<(WireToken, WireTokenProof), Refused> {
        let (token, proof) = deal
            .hand
            .token(me.secret, me.key, deal.deck, index, ctx)
            .map_err(Refused::DidNotVerify)?;

        // Recorded through the same door as everybody else's, so the set cannot
        // be filled by a path that skipped verification.
        let verified = deal
            .hand
            .verify_token(me.key, deal.deck, index, token, &proof, ctx)
            .map_err(Refused::DidNotVerify)?;
        self.set_mut(index)?
            .add(me.seat, index, verified)
            .map_err(Refused::NotWanted)?;
        Ok((token, proof))
    }

    /// Take a share that arrived from another seat.
    pub fn accept(
        &mut self,
        deal: &Deal<'_>,
        me: SeatIdx,
        share: &Share<'_>,
        ctx: &DeckCtx,
    ) -> Result<(), Refused> {
        let _ = me;
        // Steps 1 and 2, which are exactly [`would_verify`] and are called
        // through it rather than repeated here: a second copy of a
        // verification is a second copy that can drift, and the whole value of
        // `HAND_ABORT cause = 3` is that the accused's judge and the accuser
        // ran the same check.
        let verified = self.verified_share(deal, share, ctx)?;

        // 3. One share per seat per card, never an update.
        self.set_mut(share.index)?
            .add(share.from, share.index, verified)
            .map_err(Refused::NotWanted)
    }

    /// Is this share due here, and does its proof hold against this peer's own
    /// final deck? **Changes nothing.**
    ///
    /// The judging half of [`accept`](Self::accept) with the recording half cut
    /// off. It exists for `PROTOCOL.md` §4.10's `cause = 3`, whose acceptance
    /// gate is *the evidence verifies* and which must therefore run the check
    /// over a share that is **not** this receiver's to record: it belongs to a
    /// stage the accused was at, carried inside somebody else's abort, and
    /// recording it would be accepting a contribution that arrived as an
    /// exhibit.
    ///
    /// The distinctions in [`Refused`] are the point of returning it whole. Only
    /// `DidNotVerify(VerifyOutcome::Invalid(_))` is evidence against the signer.
    /// `NotDue`, `UnknownSeat` and `CouldNotVerify` are statements about what
    /// *this* peer holds, and a gate that read them as agreement would let an
    /// accuser end a hand at every receiver that happened to be behind.
    pub fn would_verify(
        &self,
        deal: &Deal<'_>,
        share: &Share<'_>,
        ctx: &DeckCtx,
    ) -> Result<(), Refused> {
        self.verified_share(deal, share, ctx).map(|_| ())
    }

    /// Steps 1 and 2, once, for both callers above.
    fn verified_share(
        &self,
        deal: &Deal<'_>,
        share: &Share<'_>,
        ctx: &DeckCtx,
    ) -> Result<VerifiedToken, Refused> {
        // 1. Free: is it due at all? Not "is it for us" — every legal share is
        // broadcast and every peer keeps every one of them, which is what makes
        // a showdown 2 x 131 bytes instead of a fresh round.
        entitlement(&self.map, share.index, share.from, &self.stage)
            .map_err(Refused::NotDue)?;

        // 2. ~0.1 ms: does it verify against this peer's own final deck?
        let key = deal
            .keys
            .get(share.from as usize)
            .and_then(|k| k.as_ref())
            .ok_or(Refused::UnknownSeat { seat: share.from })?;
        deal.hand
            .verify_token(key, deal.deck, share.index, share.token, share.proof, ctx)
            .map_err(Refused::DidNotVerify)
    }

    /// Open a card, if every share is in.
    pub fn open(&mut self, deal: &Deal<'_>, index: CardIndex) -> Result<Card, NotOpenable> {
        if let Some(card) = self.opened.get(&index.get()) {
            return Ok(*card);
        }
        let set = self
            .sets
            .get(&index.get())
            .ok_or(NotOpenable::NoSuchCard { index: index.get() })?;

        let Some(tokens) = set.complete() else {
            return Err(NotOpenable::Waiting {
                outstanding: set.outstanding(),
            });
        };
        let owned: Vec<VerifiedToken> = tokens.into_iter().copied().collect();

        let card = deal
            .hand
            .open(deal.deck, index, &owned)
            .map_err(NotOpenable::Fault)?;
        self.opened.insert(index.get(), card);
        Ok(card)
    }

    /// A seat's two cards, if both are open here.
    pub fn hole_cards(&self, seat: SeatIdx) -> Option<[Card; 2]> {
        let [a, b] = self.map.hole_cards(seat)?;
        Some([*self.opened.get(&a.get())?, *self.opened.get(&b.get())?])
    }

    /// The board so far, in order, stopping at the first card not yet open.
    pub fn board(&self) -> Vec<Card> {
        let mut out = Vec::with_capacity(5);
        let mut indices: Vec<CardIndex> = self.map.flop().to_vec();
        indices.push(self.map.turn());
        indices.push(self.map.river());
        for i in indices {
            match self.opened.get(&i.get()) {
                Some(c) => out.push(*c),
                None => break,
            }
        }
        out
    }

    /// How many seats the aggregate has, which is how many shares a card needs.
    pub fn share_count(&self) -> usize {
        self.seats.len()
    }

    /// Which seats a card is still waiting on.
    pub fn outstanding(&self, index: CardIndex) -> Option<Vec<SeatIdx>> {
        self.sets.get(&index.get()).map(|s| s.outstanding())
    }

    fn set_mut(&mut self, index: CardIndex) -> Result<&mut TokenSet<VerifiedToken>, Refused> {
        self.sets
            .get_mut(&index.get())
            .ok_or(Refused::NoSuchCard { index: index.get() })
    }

    /// Every index this hand uses, with its role. For a caller driving the
    /// exchange.
    pub fn schedule(&self) -> Vec<(CardIndex, Role)> {
        (0..52u8)
            .filter_map(|i| {
                let index = self.map.index_from_wire(i)?;
                Some((index, self.map.role(i)?))
            })
            .collect()
    }
}
