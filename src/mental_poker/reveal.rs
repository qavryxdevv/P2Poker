//! Decryption shares: private hole cards, and the board opened one street at a
//! time and never before.
//!
//! # What this module owns, and why it is not in the crypto boundary
//!
//! The review was explicit that no `ctx` field and no library parameter can
//! express whether index 12 is due now, or whether seat 0 may publish a token
//! for index 0. A cheat would simply compute the "correct" context for the stage
//! it is cheating in. Gating is irreducibly the protocol layer's, and putting a
//! stage parameter into [`DeckCrypto`](super::protocol::DeckCrypto) would
//! suggest otherwise.
//!
//! So the trait takes a deck and an index and will happily mint a river token
//! pre-flop. **This module is what stops it**, under two rules:
//!
//! - A board index opens when its street is reached, to everybody.
//! - A hole index of seat `X` opens **to `X` alone**, from every seat but `X` —
//!   and at showdown, to everybody, but only for the seats that must show.
//!
//! ## What these rules are, and what they are not
//!
//! They are correctness rules for a **conforming client**: they say what an
//! honest implementation issues and what it accepts, and
//! [`entitlement`] is the one place either question is answered so that no two
//! callers answer it differently.
//!
//! **They are not a defence against collusion, and no rule here could be.** The
//! attack they appear to stop is seat `Y` handing `Z` a token for `X`'s hole
//! card. Nothing in this protocol can stop that: the signed envelope
//! (`PROTOCOL.md` §2.7) carries no addressee, so a token sent to the wrong seat
//! is byte-identical to one sent to the right one — and even if it did, `Y`
//! could send the token, or simply the card, over any channel at all.
//!
//! That is collusion by out-of-band channel, and `SPEC_CS.md` §18 places it
//! **outside** what this construction fixes. It is stated here rather than left
//! implied, because a rule that looks like a privacy defence and is not one is
//! worse than no rule: it invites a reader to stop looking.
//!
//! What [`token_addressed_to`] does buy is real but narrower — an honest client
//! does not act on a token that was not for it, so a lazy or buggy peer is
//! caught rather than tolerated, and a hole card is never opened by accident.
//!
//! # A full token set that does not open a card is a soundness fault (C-10)
//!
//! `n`-of-`n` threshold ElGamal: with one verified token per key in the
//! aggregate, the card comes out. If every token verified and the product is
//! still not a card of the open deck, then something in the argument's soundness
//! has failed — nobody in particular did anything, and there is nothing to
//! retry.
//!
//! It is therefore **not** a verification failure and **not** an abort:
//! [`SoundnessFault`] is a distinct, loud, non-resumable outcome that is **never
//! attributed to a peer**. Treating it as an abort would be worse than useless,
//! because an abort has a blame edge and this has none.

use std::collections::BTreeMap;

use crate::poker::state::{SeatIdx, Street};

use super::deck::{CardIndex, DeckIndexMap, Role};

/// Who a reveal token is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    /// Every seat at the table. Board cards, and hole cards at showdown.
    Broadcast,
    /// One seat, and no other. A hole card belongs to its owner until showdown.
    Only(SeatIdx),
}

/// Why a token may not be issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotEntitled {
    /// The index has no role this hand, so no token for it is ever legal.
    UnusedIndex { index: u8 },
    /// The street has not been reached.
    StreetNotReached { needs: Street, at: Street },
    /// A seat cannot contribute a token towards its own hole card before
    /// showdown: it holds the last share itself, and publishing it would open
    /// its own card to the table.
    OwnHoleCard { seat: SeatIdx },
    /// The seat is not showing, so its hole cards are never opened.
    NotShowing { seat: SeatIdx },
}

/// Where the hand is, as far as revealing is concerned.
///
/// Distinct from [`Street`] because showdown is not a street: it is what happens
/// after the last one, and it is the only point at which a hole card becomes
/// public.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevealStage {
    /// Betting on `street`. Only board cards up to that street may open.
    Betting(Street),
    /// Showdown. The board is complete, and the hole cards of the seats in
    /// `showing` open to the table.
    Showdown { showing: Vec<SeatIdx> },
}

/// When each index may be opened and to whom.
///
/// The two rules of this module in one function, so that no caller re-derives
/// them and no two callers derive them differently.
pub fn entitlement(
    map: &DeckIndexMap,
    index: CardIndex,
    from: SeatIdx,
    stage: &RevealStage,
) -> Result<Audience, NotEntitled> {
    let role = map.role(index.get()).ok_or(NotEntitled::UnusedIndex {
        index: index.get(),
    })?;

    let street_now = match stage {
        RevealStage::Betting(s) => *s,
        RevealStage::Showdown { .. } => Street::River,
    };

    match role {
        Role::Flop | Role::Turn | Role::River => {
            let needs = match role {
                Role::Flop => Street::Flop,
                Role::Turn => Street::Turn,
                _ => Street::River,
            };
            if (street_now as u8) < (needs as u8) {
                return Err(NotEntitled::StreetNotReached {
                    needs,
                    at: street_now,
                });
            }
            Ok(Audience::Broadcast)
        }
        Role::HoleFirst(owner) | Role::HoleSecond(owner) => match stage {
            RevealStage::Betting(_) => {
                if from == owner {
                    // The owner holds the last share. It never publishes it,
                    // and it never needs to: it decrypts with its own key.
                    Err(NotEntitled::OwnHoleCard { seat: owner })
                } else {
                    Ok(Audience::Only(owner))
                }
            }
            RevealStage::Showdown { showing } => {
                if showing.contains(&owner) {
                    Ok(Audience::Broadcast)
                } else {
                    Err(NotEntitled::NotShowing { seat: owner })
                }
            }
        },
    }
}


/// Why a received token should not have arrived here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Misdirected {
    /// The token was for another seat's eyes.
    ///
    /// Dropped rather than used. It is **not** evidence of cheating: the
    /// envelope carries no addressee, so the same bytes sent to the right seat
    /// would be correct, and a colluding sender would not have used this channel
    /// anyway (`SPEC_CS.md` §18).
    ForAnotherSeat { intended: SeatIdx },
    /// The token was not due at all.
    NotDue(NotEntitled),
}

/// Whether a token that arrived here is one this seat should act on.
///
/// The receiving half of [`entitlement`]. An honest client checks it so that a
/// hole card is never opened by accident and a peer that publishes what it
/// should have addressed is caught rather than silently followed.
pub fn token_addressed_to(
    map: &DeckIndexMap,
    index: CardIndex,
    from: SeatIdx,
    me: SeatIdx,
    stage: &RevealStage,
) -> Result<(), Misdirected> {
    match entitlement(map, index, from, stage) {
        Err(e) => Err(Misdirected::NotDue(e)),
        Ok(Audience::Broadcast) => Ok(()),
        Ok(Audience::Only(x)) if x == me => Ok(()),
        Ok(Audience::Only(x)) => Err(Misdirected::ForAnotherSeat { intended: x }),
    }
}

/// A full verified token set failed to open a card.
///
/// Never attributed to a peer, never retried, and not an abort. Every token in
/// the set verified against this peer's own final deck, so if the product is not
/// a card then the failure is in the construction, not in anybody's behaviour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundnessFault {
    /// What was being opened.
    pub index: u8,
    /// One line, for the log and for the window the player sees.
    pub what: &'static str,
}

impl core::fmt::Display for SoundnessFault {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "soundness fault opening index {}: {} — this is not attributable to any peer",
            self.index, self.what
        )
    }
}

impl std::error::Error for SoundnessFault {}

/// Why a token was not added to a set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetError {
    /// A token for a different index.
    WrongIndex { expected: u8, got: u8 },
    /// A seat that owes no token towards this card.
    NotExpected { seat: SeatIdx },
    /// That seat already gave one. A second is a protocol violation rather than
    /// an update, because a set that could be overwritten is a set whose
    /// contents depend on arrival order.
    AlreadyGiven { seat: SeatIdx },
}

/// The tokens gathered for one card.
///
/// `n`-of-`n`: the set is complete only when every expected seat has given one,
/// counted here. The library sums whatever it is handed and never learns how
/// many players exist, so the count is ours to keep.
#[derive(Debug, Clone)]
pub struct TokenSet<T> {
    index: CardIndex,
    expected: Vec<SeatIdx>,
    given: BTreeMap<SeatIdx, T>,
}

impl<T> TokenSet<T> {
    /// Open a set for one card, over the seats whose share it needs.
    ///
    /// **Every seat, always — a hole card included.** Decryption is `n`-of-`n`
    /// over the aggregate key, so the owner's own share is one of the `n` and the
    /// set is not complete without it. What is different about a hole card is
    /// only *where the owner's share comes from*: the owner computes it locally
    /// and never publishes it, which is why nobody else can complete the set and
    /// why the card is private.
    ///
    /// Getting this wrong is worse than it looks, and the first draft of this
    /// module did: a set sized `n - 1` calls itself complete one share early,
    /// `open` then fails, and C-10 says a **complete** verified set that fails to
    /// open is a soundness fault — distinct, loud and non-resumable. A mis-sized
    /// set turns an ordinary "not yet" into exactly the fault that must never be
    /// raised without cause.
    pub fn awaiting(index: CardIndex, expected: Vec<SeatIdx>) -> Self {
        TokenSet {
            index,
            expected,
            given: BTreeMap::new(),
        }
    }

    /// The seats this set is waiting on, in seat order.
    pub fn outstanding(&self) -> Vec<SeatIdx> {
        self.expected
            .iter()
            .copied()
            .filter(|s| !self.given.contains_key(s))
            .collect()
    }

    /// Add one **verified** token.
    ///
    /// The type parameter is the boundary's `Verified<RevealToken>`, so a token
    /// that has not been checked cannot be put in a set — and completeness then
    /// means what C-10 needs it to mean.
    pub fn add(&mut self, seat: SeatIdx, index: CardIndex, token: T) -> Result<(), SetError> {
        if index != self.index {
            return Err(SetError::WrongIndex {
                expected: self.index.get(),
                got: index.get(),
            });
        }
        if !self.expected.contains(&seat) {
            return Err(SetError::NotExpected { seat });
        }
        if self.given.contains_key(&seat) {
            return Err(SetError::AlreadyGiven { seat });
        }
        self.given.insert(seat, token);
        Ok(())
    }

    pub fn is_complete(&self) -> bool {
        self.given.len() == self.expected.len()
    }

    pub fn index(&self) -> CardIndex {
        self.index
    }

    /// The tokens, in seat order, or `None` if the set is short.
    ///
    /// The only way to get at them, so a caller cannot try to open a card from a
    /// partial set and then read the failure as a soundness fault — which would
    /// turn an ordinary "not yet" into a non-resumable one.
    pub fn complete(&self) -> Option<Vec<&T>> {
        if !self.is_complete() {
            return None;
        }
        Some(self.given.values().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(seats: usize, button: SeatIdx) -> DeckIndexMap {
        DeckIndexMap::build(&vec![true; seats], button, 52).unwrap()
    }

    #[test]
    fn a_board_card_opens_on_its_own_street_and_not_before() {
        let m = map(6, 0);
        let flop = m.flop()[0];

        for (at, ok) in [
            (Street::PreFlop, false),
            (Street::Flop, true),
            (Street::Turn, true),
            (Street::River, true),
        ] {
            let got = entitlement(&m, flop, 3, &RevealStage::Betting(at));
            assert_eq!(
                got.is_ok(),
                ok,
                "flop token at {at:?} should be {}",
                if ok { "allowed" } else { "refused" }
            );
            if ok {
                assert_eq!(got.unwrap(), Audience::Broadcast);
            }
        }

        assert_eq!(
            entitlement(&m, m.river(), 3, &RevealStage::Betting(Street::Turn)),
            Err(NotEntitled::StreetNotReached {
                needs: Street::River,
                at: Street::Turn
            })
        );
        assert_eq!(
            entitlement(&m, m.turn(), 3, &RevealStage::Betting(Street::Turn)),
            Ok(Audience::Broadcast)
        );
    }

    /// The rule the game's privacy rests on: a token for somebody else's hole
    /// card goes to its owner and to nobody else. The ciphertext is public, so a
    /// broadcast token is the whole of the secret.
    #[test]
    fn a_hole_card_token_is_addressed_to_its_owner_alone() {
        let m = map(6, 0);
        let owner = m.deal_order()[2];
        let [first, second] = m.hole_cards(owner).unwrap();

        for index in [first, second] {
            for from in 0..6u8 {
                let got = entitlement(&m, index, from, &RevealStage::Betting(Street::PreFlop));
                if from == owner {
                    assert_eq!(got, Err(NotEntitled::OwnHoleCard { seat: owner }));
                } else {
                    assert_eq!(
                        got,
                        Ok(Audience::Only(owner)),
                        "seat {from} must send to {owner} and not broadcast"
                    );
                }
            }
        }
    }

    /// And it stays that way on every street, because there is no street at
    /// which a hole card becomes public. Only showdown does that.
    #[test]
    fn no_street_makes_a_hole_card_public() {
        let m = map(4, 1);
        let owner = m.deal_order()[0];
        let [first, _] = m.hole_cards(owner).unwrap();

        for at in [Street::PreFlop, Street::Flop, Street::Turn, Street::River] {
            assert_eq!(
                entitlement(&m, first, owner + 1, &RevealStage::Betting(at)),
                Ok(Audience::Only(owner)),
                "still private at {at:?}"
            );
        }
    }

    /// At showdown the seats that show open to the table, and the seats that do
    /// not are never opened at all — a mucked hand stays mucked.
    #[test]
    fn showdown_opens_only_the_hands_that_show() {
        let m = map(4, 0);
        let shows = m.deal_order()[0];
        let mucks = m.deal_order()[1];
        let stage = RevealStage::Showdown {
            showing: vec![shows],
        };

        let [a, _] = m.hole_cards(shows).unwrap();
        assert_eq!(entitlement(&m, a, 2, &stage), Ok(Audience::Broadcast));
        // Even the owner may publish now: the card is public either way.
        assert_eq!(entitlement(&m, a, shows, &stage), Ok(Audience::Broadcast));

        let [b, _] = m.hole_cards(mucks).unwrap();
        assert_eq!(
            entitlement(&m, b, 2, &stage),
            Err(NotEntitled::NotShowing { seat: mucks }),
            "a mucked hand is never opened"
        );
    }

    /// An index with no role has no legal token at any stage, which is the other
    /// half of the map refusing to mint one.
    #[test]
    fn an_unused_index_is_never_opened() {
        let m = map(2, 0);
        // Index 9 is past the river at two seats, so the map will not mint it.
        assert_eq!(m.index_from_wire(9), None);
        // Constructed through a used index and then checked at a stage where it
        // has no role: build a 2-seat map and ask about an index legal at 6.
        let wide = map(6, 0);
        let river_at_six = wide.river();
        assert_eq!(
            entitlement(&m, river_at_six, 0, &RevealStage::Betting(Street::River)),
            Err(NotEntitled::UnusedIndex {
                index: river_at_six.get()
            }),
            "an index from another table shape has no role here"
        );
    }


    /// The receiving half. A token for somebody else's hole card is dropped
    /// rather than used, so an honest client never opens a card by accident.
    #[test]
    fn a_token_meant_for_another_seat_is_dropped() {
        let m = map(4, 0);
        let owner = m.deal_order()[0];
        let other = m.deal_order()[1];
        let [card, _] = m.hole_cards(owner).unwrap();
        let stage = RevealStage::Betting(Street::Flop);

        assert_eq!(token_addressed_to(&m, card, other, owner, &stage), Ok(()));
        assert_eq!(
            token_addressed_to(&m, card, other, m.deal_order()[2], &stage),
            Err(Misdirected::ForAnotherSeat { intended: owner }),
            "seat 2 has no business holding a share of seat 0 card"
        );
    }

    /// A board token is for everybody, so it is never misdirected — and one that
    /// is not due yet is refused for that reason rather than for its audience.
    #[test]
    fn a_board_token_reaches_every_seat_once_it_is_due() {
        let m = map(4, 0);
        let flop = m.flop()[0];
        for me in 0..4u8 {
            assert_eq!(
                token_addressed_to(&m, flop, 1, me, &RevealStage::Betting(Street::Flop)),
                Ok(())
            );
            assert_eq!(
                token_addressed_to(&m, flop, 1, me, &RevealStage::Betting(Street::PreFlop)),
                Err(Misdirected::NotDue(NotEntitled::StreetNotReached {
                    needs: Street::Flop,
                    at: Street::PreFlop
                }))
            );
        }
    }

    #[test]
    fn a_set_is_complete_only_when_every_expected_seat_has_given() {
        let m = map(4, 0);
        let card = m.flop()[0];
        let mut set: TokenSet<u8> = TokenSet::awaiting(card, vec![0, 1, 2, 3]);

        assert!(!set.is_complete());
        assert_eq!(set.outstanding(), vec![0, 1, 2, 3]);
        assert!(set.complete().is_none());

        for seat in [0u8, 1, 2] {
            set.add(seat, card, seat).unwrap();
        }
        assert!(!set.is_complete(), "n-of-n, and one seat is missing");
        assert_eq!(set.outstanding(), vec![3]);
        assert!(
            set.complete().is_none(),
            "a partial set must not be openable, or a `not yet` reads as a fault"
        );

        set.add(3, card, 3).unwrap();
        assert!(set.is_complete());
        assert_eq!(set.complete().unwrap(), vec![&0, &1, &2, &3]);
    }

    /// A hole card needs **every** share, the owner's included. Decryption is
    /// `n`-of-`n`; what makes the card private is that the owner's share is
    /// computed locally and never published, not that it is not needed.
    ///
    /// The first draft of this module sized the set at `n - 1`. It would have
    /// called itself complete one share early, failed to open, and raised a
    /// soundness fault — the one outcome that is meant to be unreachable without
    /// a real failure of the construction.
    #[test]
    fn a_hole_card_needs_every_share_including_its_owners() {
        let m = map(4, 0);
        let owner = m.deal_order()[0];
        let [card, _] = m.hole_cards(owner).unwrap();
        let everyone: Vec<SeatIdx> = (0..4u8).collect();

        let mut set: TokenSet<u8> = TokenSet::awaiting(card, everyone);
        for seat in (0..4u8).filter(|&s| s != owner) {
            set.add(seat, card, seat).unwrap();
        }
        assert!(
            !set.is_complete(),
            "n-1 shares is not n, and calling it complete would raise a fault"
        );
        assert_eq!(set.outstanding(), vec![owner]);

        // The owner's own share, computed locally and never sent anywhere.
        set.add(owner, card, owner).unwrap();
        assert!(set.is_complete());
    }

    /// A seat that is not at the table owes nothing and cannot fill a slot.
    #[test]
    fn a_seat_outside_the_expected_set_cannot_contribute() {
        let m = map(4, 0);
        let card = m.turn();
        let mut set: TokenSet<u8> = TokenSet::awaiting(card, vec![0, 1, 2, 3]);
        assert_eq!(
            set.add(9, card, 9),
            Err(SetError::NotExpected { seat: 9 })
        );
    }

    /// A second token from one seat is a protocol violation, not an update: a
    /// set that could be overwritten has contents that depend on arrival order.
    #[test]
    fn a_seat_cannot_replace_its_own_token() {
        let m = map(4, 0);
        let card = m.turn();
        let mut set: TokenSet<u8> = TokenSet::awaiting(card, vec![0, 1, 2, 3]);
        set.add(1, card, 10).unwrap();
        assert_eq!(
            set.add(1, card, 99),
            Err(SetError::AlreadyGiven { seat: 1 })
        );
        set.add(0, card, 0).unwrap();
        set.add(2, card, 2).unwrap();
        set.add(3, card, 3).unwrap();
        assert_eq!(set.complete().unwrap(), vec![&0, &10, &2, &3]);
    }

    /// A token for another card does not fill this set, however valid it is.
    #[test]
    fn a_token_for_another_card_is_refused() {
        let m = map(4, 0);
        let turn = m.turn();
        let river = m.river();
        let mut set: TokenSet<u8> = TokenSet::awaiting(turn, vec![0, 1]);
        assert_eq!(
            set.add(0, river, 7),
            Err(SetError::WrongIndex {
                expected: turn.get(),
                got: river.get()
            })
        );
        assert!(!set.is_complete());
    }

    /// The fault says what it is and says it is nobody's fault, because the one
    /// thing it must never do is start a blame edge.
    #[test]
    fn a_soundness_fault_names_no_peer() {
        let f = SoundnessFault {
            index: 12,
            what: "a complete verified token set did not decrypt to a card of the open deck",
        };
        let text = f.to_string();
        assert!(text.contains("index 12"));
        assert!(text.contains("not attributable to any peer"));
    }
}
