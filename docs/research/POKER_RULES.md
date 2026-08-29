# POKER_RULES.md — normative rules for the deterministic engine

Phase 0 research output. Covers SPEC_CS.md §11 (deterministic poker engine) and
§4 (tournament preset derived from PokerTH's rated Sit-and-Go).

This document is written to be implementable twice, independently, with
**byte-identical** resulting state. Every rule below is stated as a computable
predicate over named state variables. Where a real-world rulebook leaves
discretion to a human floor person, this document removes the discretion and
states one machine rule, and says so explicitly.

---

## 0. Sources and verification method

Every factual claim in this document is tagged with how it was checked. Two
methods are used, per the Phase 0 rules:

* **(compiled)** — code using the API was compiled and executed successfully.
* **(source)** — the actual crate/program source on disk was read.
* **(primary text)** — the rulebook PDF was downloaded and its text extracted
  locally; quotations are verbatim from that extraction.

Primary rules source:

> **Poker Tournament Directors Assn., 2024 Rules Version 1.0, Oct 9, 2024,
> Longform Version (includes Recommended Procedures and Illustration Addendum).**
> Downloaded from `https://holdemhive.com/TDA%20Rules-2024-V1.pdf`
> (310.7 KB PDF, 22 pages), text extracted locally with `pypdf`.
> Verification: (primary text). Extracted text kept at
> `<scratchpad>`.
> Note: `pokertda.com` itself refused the connection (`ECONNREFUSED
> 18.236.22.145:443`) at the time of writing; the mirror above was used and its
> header self-identifies as the 2024 v1.0 official longform text.

Secondary source for the preset: the PokerTH checkout at
`~/src/pokerth`. All PokerTH claims cite `file:line`.
Verification: (source).

Probe crate used for the evaluator section (all results reproducible):
`<scratchpad>`

Where this document deliberately departs from TDA, the departure is marked
**[OUR CHOICE]** with the reason. Where a value could not be found in the source
it is marked **[NOT IN POKERTH]**.

---

# PART A — No-Limit Texas Hold'em, normative rules

## A0. Units, types, and global invariants

* All chip amounts are **non-negative integers**. There is exactly one
  denomination; the smallest unit is `1`. No fractional chips ever exist.
  Rationale: TDA rule 20 begins "First, odd chips will be broken into the
  smallest denomination in play"; with a single denomination of 1 that step is a
  no-op and the odd-chip rule reduces to integer remainder distribution.
  Verification: (primary text).
* Seats are a fixed ring `0..=N-1` with `N = max_players`. Clockwise = index+1
  mod N. A seat is either empty or holds one player for the whole tournament.
* `succ(s)` = next seat index clockwise; `succ_active(s)` = first seat strictly
  clockwise from `s` holding a player who is still in the tournament (stack > 0
  at hand start); `succ_live(s)` = first seat strictly clockwise from `s` holding
  a player who is still in *this hand* (not folded).

Global invariants the engine MUST assert after every transition (these are the
property-test invariants required by SPEC_CS.md §26):

1. `sum(stacks) + sum(committed_this_hand) == total_chips_in_play`, constant for
   the whole tournament.
2. `sum(pot_sizes) + sum(returned_uncalled) == sum(committed_this_hand)`.
3. No card value appears twice across `board` and all dealt hole cards.
4. `len(board) <= 5`, and `len(board) ∈ {0,3,4,5}` at street boundaries.
5. `committed_this_round[p] <= committed_this_hand[p] <= start_stack[p]`.
6. A folded player is in no pot's eligible set.
7. Every pot's eligible set is a subset of its contributor set.
8. `player_to_act` is live, not all-in, and it is that player's turn.

## A1. Blinds, button and heads-up

### A1.1 Blind posting

At the start of every hand, with `SB` and `BB = 2*SB` the current level's amounts:

1. The small-blind seat posts `min(SB, stack)`. If `stack <= SB` that player is
   **all-in** immediately, before any voluntary action.
2. The big-blind seat posts `min(BB, stack)`. If `stack <= BB` that player is
   **all-in** immediately.
3. `current_bet := BB` — the **nominal** big blind, *regardless* of whether the
   big-blind player was able to post it in full.
4. `last_full_raise := BB`.
5. Posting a blind is **not an action**. `acted_this_round[p] := false` for
   every player, including both blinds. This is what gives the big blind the
   "option" (see A2).

Rule 3 is the one implementers most often get wrong. Worked example: blinds
50/100, the BB player has only 60 chips. They post 60 and are all-in.
`current_bet` is still **100**, not 60. An under-the-gun player who wants to call
must put in 100, and a raise must be to at least `100 + 100 = 200`.

> Verification: (source) PokerTH does exactly this —
> `src/engine/local_engine/localberopreflop.cpp:44` `setHighestSet(2*getSmallBlind());`
> unconditionally, while
> `src/engine/local_engine/localhand.cpp:490-529` (`LocalHand::setBlinds`) posts only `getMyCash()` when
> `getMyCash() <= 2*smallBlind` and flags `PLAYER_ACTION_ALLIN`.

There are **no antes** in this game. Verification: (source) `grep -ri '\bante\b'`
over `pokerth/src/engine/`, `src/gamedata.h` and `src/game_defs.h` returns
nothing; PokerTH's engine has no ante concept at all.

### A1.2 Heads-up button and blind rule

With exactly two live players:

> **TDA 34-B, verbatim:** "Heads-up, the small blind is the button, is dealt the
> last card, and acts first pre-flop and last on all other betting rounds.
> Starting heads-up play, the button may need to be adjusted to ensure no player
> has the big blind twice in a row."
>
> Verification: (primary text).

Normatively for the engine:

* `sb_seat == button_seat`, `bb_seat == the other seat`.
* Pre-flop first to act = `button_seat` (the small blind).
* Post-flop (flop, turn, river) first to act = `bb_seat`; the button acts last.
* The button alternates every hand.

Note this is the exact inverse of the 3+-handed post-flop order, which is the
usual source of bugs. Do not implement post-flop order as "first live seat
clockwise from the button" — heads-up that formula yields the BB, which happens
to be correct, but implement it as an explicit two-player branch anyway and unit
test both.

> Verification: (source) PokerTH implements the same inversion explicitly in
> `src/engine/local_engine/localhand.cpp:435-476` — the comment reads
> "assign Small Blind next to dealer. ATTENTION: in heads up it is big blind",
> and the code branches on `activePlayerList->size() > 2`, setting button code 3
> (big blind) on the seat after the dealer and button code 2 (small blind) on the
> dealer when only two players remain.

### A1.3 Button rotation between hands, and busted players

**TDA 32, verbatim:** "Tournament play will use a dead button."
Verification: (primary text).

The *dead button* rule means the blind positions advance by one seat position
each hand independent of who is still alive, so that no player ever pays the big
blind twice in a row and no player ever skips the big blind. Concretely:

```
next_bb_seat     = succ_active(bb_seat)          // next surviving player, clockwise
next_sb_position = bb_seat                       // the previous BB position
next_button_pos  = sb_position                   // the previous SB position
```

`next_sb_position` and `next_button_pos` are *positions*, not necessarily
occupied seats:

* If `next_button_pos` is now empty, the button is **dead** — it sits on an empty
  seat. No one is "on the button"; the button is only a position marker for
  action order and odd-chip assignment.
* If `next_sb_position` is now empty, the small blind is **dead** — it is simply
  not posted, and the pot is one small blind lighter that hand.
* The big blind is always posted by a live player; the BB position never dies.

When the field reaches two players, apply the TDA 34-B adjustment: if applying
the rotation above would give one player the big blind twice in a row, the button
is adjusted so it does not.

**PokerTH does NOT implement the dead button.** Verification: (source)
`src/engine/game.cpp:191-210` shifts `dealerPosition` to the next entry found in
`activePlayerList` — i.e. the next *surviving player*, skipping busted seats
entirely. The source even carries the comment
`// shifting dealer button -> TODO exception-rule !!!` at
`src/engine/game.cpp:191`. Consequence of PokerTH's simpler rule: when the player
seated between the current SB and BB busts, a player can be forced to post the
big blind on two consecutive hands.

**[OUR CHOICE]** Adopt the **TDA dead button** (the block above), not PokerTH's
moving button, even though the preset in Part B is PokerTH-derived. Reasons:
(a) it is the tournament standard and is what a player expects; (b) it is
strictly *more* deterministic — it is a pure function of the previous hand's
`(button_pos, sb_pos, bb_seat)` and the set of survivors, with no dependence on
list iteration order; (c) the alternative measurably changes EV for the player
adjacent to a bust-out, which is exactly the kind of silent unfairness a
decentralised game cannot arbitrate away. The divergence from PokerTH must be
recorded in `docs/DECISIONS.md` since Part B otherwise follows PokerTH.

A player whose stack reaches `0` is eliminated at the **end** of the hand and is
not dealt in to the next one. Verification: (source) PokerTH does this in
`src/engine/game.cpp:171-181` (`initHand()` sets players with `getMyCash() == 0`
inactive and erases them from `activePlayerList`), i.e. at the start of the next
hand, before the button shift — equivalent.

Finishing order for the tournament result: players eliminated in the same hand
are ranked by their **stack at the start of that hand**, larger stack finishing
higher. If two eliminated players had identical starting stacks, rank them by
seat order clockwise from the button, earlier seat finishing higher.
**[OUR CHOICE]** — TDA does not settle simultaneous bust-outs mechanically and
PokerTH stores only a finishing place (`src/net/servergame.cpp:475-488`); a
decentralised game needs a total order with no floor person, and this one is a
pure function of public state.

## A2. Betting rounds and action order

Streets in order: `PreFlop → Flop → Turn → River → Showdown`. The board has
`0, 3, 4, 5` cards respectively.

**First to act:**

| Street | 2 live players | 3+ live players |
|---|---|---|
| Pre-flop | `button_seat` (= SB) | `succ_live(bb_seat)` (UTG) |
| Flop / Turn / River | `bb_seat` (non-button) | `succ_live(button_pos)` |

For 3+ players post-flop, `succ_live(button_pos)` is computed from the button
*position*, which may be a dead (empty) seat — this is why the button position
must be retained even when nobody occupies it.

Action then proceeds clockwise, skipping folded and all-in players.

**A betting round ends** when both of the following hold:

1. Every live, non-all-in player has `acted_this_round == true`, **and**
2. Every live, non-all-in player has `committed_this_round == current_bet`
   (or is all-in for less).

The big blind's **option**: pre-flop, because posting a blind sets
`acted_this_round = false` (A1.1 step 5), condition 1 is not satisfied until the
big blind has actually acted, even if everyone merely called. So an unraised pot
returns to the big blind, who may check or raise. Same for the small blind
heads-up, where the SB/button acts first and the BB closes the action.

If at most one live player is not all-in, no further betting is possible: skip
directly to dealing the remaining board cards and then to showdown. The hand is
**not** abandoned — remaining streets are still dealt because they decide the
pots. (SPEC_CS.md §10: each board card is still cryptographically revealed at
its own street; the reveals happen back to back with no action between them.)

## A3. Legal actions

For the player to act, with `to_call = current_bet - committed_this_round[p]` and
`stack = p.stack`:

| Action | Legal when | Effect |
|---|---|---|
| `Fold` | always | `folded[p] = true`. Chips already committed stay in the pot. |
| `Check` | `to_call == 0` | nothing moves |
| `Call` | `to_call > 0` | move `min(to_call, stack)` into the pot. If `stack <= to_call`, this is an **all-in call for less** (A6). |
| `Bet(n)` | `current_bet == 0` | see A4 |
| `Raise(to)` | `current_bet > 0` and `can_reopen(p)` (A5) | see A4 |

`Bet`/`Raise` amounts are always stated as the player's **total commitment for
the round** (`raise_to`), never as an increment. TDA 43-B, verbatim: "Without
other clarifying information, declaring raise and an amount is the total bet."
Verification: (primary text). Using a total, not an increment, removes an entire
class of ambiguity from the wire protocol; the protocol message MUST carry
`raise_to`.

Every action also sets `acted_this_round[p] = true`.

The engine MUST re-validate every incoming action against this table locally and
reject anything else, per SPEC_CS.md §11 ("Poker engine nesmí důvěřovat tomu, že
protistrana posílá legální akce"). An action that fails validation is a protocol
violation attributable to its signer, not a state transition.

## A4. Minimum bet, minimum raise, and `last_full_raise`

State variables:

* `current_bet` — the highest `committed_this_round` at the table.
* `last_full_raise` — the size of the largest *full* bet or raise **increment**
  so far in this betting round.

Initialisation:

* Pre-flop: `current_bet = BB`, `last_full_raise = BB`.
* Flop/Turn/River: `current_bet = 0`, `last_full_raise = BB`.

**Minimum bet** (when `current_bet == 0`): `n >= BB`, or `n == stack` (all-in).

**Minimum raise**:

> **TDA 43-A, verbatim:** "A raise must be at least equal to the largest prior
> full bet or raise of the current betting round."
>
> **TDA 47-A, verbatim:** "In no-limit and pot limit, an all-in wager (or
> cumulative multiple short all-ins) totaling less than a full bet or raise will
> not reopen betting for players who have already acted and are not facing at
> least a full bet or raise when the action returns to them. If multiple short
> all-ins re-open the betting, the minimum raise is always the last full valid
> bet or raise of the round (See also Rule 43)."
>
> Verification: (primary text).

So:

```
min_raise_to = current_bet + last_full_raise

legal Raise(to)  ⇔  to >= min_raise_to
                 ∨  to == committed_this_round[p] + stack[p]     // all-in
```

**Updating `last_full_raise`** after a legal `Bet(n)` or `Raise(to)`:

```
increment = to - current_bet                  // for Bet: increment = n - 0 = n
if increment >= last_full_raise:
    last_full_raise = increment               // a FULL raise
// else: a short all-in — last_full_raise is NOT changed
current_bet = max(current_bet, to)
```

The `else` branch is the whole point of TDA 47-A's second sentence: short all-ins
never raise the minimum-raise yardstick, even when several of them stack up.

`last_full_raise` is monotonically non-decreasing within a round, so "largest
prior full raise" (43-A) and "last full valid raise" (47-A) always coincide; the
engine only needs the one variable.

**Worked check against TDA's own Illustration Addendum, Rule 47 Example 2**
(verbatim from the primary text): "NLHE, Blinds 50-100. Post-flop A opens for
300, B pushes all-in for 500 total, C goes all-in for 650 total, D goes all-in
for 800 total, E calls 800. What is the min raise for Player F? The opening bet
(300) sets the initial min raise. Because no single player was all-in for more
than 300, the min raise for F remains 300. F can either smooth call 800 or raise
to at least 1100."

Trace with the rules above:

| step | action | increment | `last_full_raise` | `current_bet` |
|---|---|---|---|---|
| init | flop | — | 100 (= BB) | 0 |
| A | Bet 300 | 300 | 300 ≥ 100 → **300** | 300 |
| B | all-in 500 | 200 | 200 < 300 → stays 300 | 500 |
| C | all-in 650 | 150 | 150 < 300 → stays 300 | 650 |
| D | all-in 800 | 150 | 150 < 300 → stays 300 | 800 |
| E | call 800 | — | 300 | 800 |
| F | `min_raise_to` | — | `800 + 300 = 1100` ✔ | |

Matches the published answer exactly.

## A5. The incomplete all-in raise and reopening the betting

This is the single subtlest rule in hold'em. Adopted rule, derived directly from
TDA 47-A quoted above:

```
can_reopen(p)  ⇔  !acted_this_round[p]
               ∨  (current_bet - committed_this_round[p]) >= last_full_raise
```

If `can_reopen(p)` is false, `p`'s only legal actions are `Fold` and `Call`
(and `Call` may be an all-in call for less). `p` may **not** raise, not even
all-in for more — an attempted raise is an illegal action and is rejected.

Note what the predicate is *not*: it does not track "who was the last aggressor",
and it does not need a per-player "facing amount at last action" field. Because
every completed voluntary action leaves `committed_this_round[p] == current_bet`
(or leaves `p` all-in and unable to act again), the quantity
`current_bet - committed_this_round[p]` **is** exactly the increment `p` has
faced since `p` last acted. That is the cumulative-short-all-in case of TDA 47-A
handled for free.

The `!acted_this_round[p]` disjunct is what preserves the big blind's option and
the raising rights of anyone yet to speak: posting a blind is not acting (A1.1),
and a player who has not yet acted always has full raising rights.

### Worked example 1 — short all-in does NOT reopen

TDA Illustration Addendum, Rule 47 Example 3-A, reproduced with our variables.
NLHE, blinds 2000/4000, four players A, B, SB, BB.

Start: `current_bet = 4000`, `last_full_raise = 4000`, all `acted = false`.

| # | player | action | committed | `current_bet` | `last_full_raise` |
|---|---|---|---|---|---|
| 1 | A | Call 4000 | A: 4000, `acted=true` | 4000 | 4000 |
| 2 | B | Fold | — | 4000 | 4000 |
| 3 | C | all-in **7500** | C: 7500 | 7500 | increment 3500 < 4000 → **stays 4000** |
| 4 | SB | Fold | SB forfeits 2000 | 7500 | 4000 |
| 5 | BB | ? | BB: 4000, `acted=false` | | |

At step 5, `can_reopen(BB)` is true via the `!acted` disjunct. `min_raise_to =
7500 + 4000 = 11500`. BB may fold, call 3500 more, or raise to ≥ 11500.
TDA: "The BB can fold, smooth call the 3500, or raise by at least 4000 for a
total of 11,500." ✔

BB smooth-calls to 7500. Action returns to A:

`can_reopen(A)`: `acted_this_round[A] == true`, and
`current_bet - committed[A] = 7500 - 4000 = 3500`, and `3500 < last_full_raise =
4000` → **false**. A may only fold or call 3500.
TDA: "A has already acted and is facing 3500 which is not a full raise.
Therefore, A can only fold or call the 3500, he cannot raise." ✔

Chip counts for the example: say A started with 40 000, C with 7 500, BB with
60 000. After A calls, committed are A 7500, C 7500 (all-in), BB 7500, plus SB's
dead 2000. Pot = 24 500, of which 2000 came from a folded player.

### Worked example 2 — the same all-in DOES reopen once a full raise lands

TDA Illustration Addendum, Rule 47 Example 3-B. Same setup through step 4, but at
step 5 the BB **raises to 11 500** (increment 4000 ≥ `last_full_raise` 4000, so
it is a full raise; `last_full_raise` stays 4000, `current_bet` becomes 11 500).

Action returns to A: `acted = true`, but
`current_bet - committed[A] = 11500 - 4000 = 7500 >= 4000` → `can_reopen(A)` is
**true**. A may fold, call 7500 more, or re-raise to at least
`11500 + 4000 = 15500`.
TDA: "because 7500 is more than a full minimum raise, betting is now re-opened
for A who can fold, call, or re-raise." ✔

### Worked example 3 — cumulative short all-ins reopen for one player but not another

TDA Illustration Addendum, Rule 47 Examples 1 and 1-A. NLHE, blinds 50/100,
post-flop (so `current_bet = 0`, `last_full_raise = 100`). Players A, B, C, D, E
with stacks A 5000, B 125, C 5000, D 200, E 5000.

| # | player | action | committed | `current_bet` | `last_full_raise` |
|---|---|---|---|---|---|
| 1 | A | Bet 100 | A: 100 | 100 | inc 100 ≥ 100 → **100** |
| 2 | B | all-in 125 | B: 125 | 125 | inc 25 < 100 → stays 100 |
| 3 | C | Call 125 | C: 125 | 125 | 100 |
| 4 | D | all-in 200 | D: 200 | 200 | inc 75 < 100 → stays 100 |
| 5 | E | Call 200 | E: 200 | 200 | 100 |
| 6 | A | ? | A: 100, acted | | |

`can_reopen(A)`: `200 - 100 = 100 >= 100` → **true**. A may raise to ≥ 300.
TDA: "Action returns to A who is facing a total raise of 100. Since 100 is a full
raise, the betting is re-opened for A... Note that neither B's increment of 25 or
D's increment of 75 is by itself a full raise, but when added together they total
a full raise." ✔

Now A merely calls to 200. Action passes to C (B and D are all-in and skipped):

`can_reopen(C)`: `200 - 125 = 75 < 100` → **false**. C may only call 75 more or
fold.
TDA Example 1-A: "C must face at least 225 total to re-open betting. Because 75
is not a full raise, betting for C is not re-opened and C can either call with 75
more or fold, he cannot raise." ✔ — and indeed our predicate flips exactly at
`current_bet = 225`.

Variant (TDA Example 1-B): if instead A had min-raised to 300, then for C
`300 - 125 = 175 >= 100` → reopened. ✔

These three examples together exercise every branch of `can_reopen`. They MUST
become unit tests in `src/poker/engine.rs`.

## A6. All-in for less than a call; over-calling

* **All-in call for less.** If `stack < to_call`, `Call` moves the whole stack.
  The player is all-in, `committed_this_round[p] < current_bet`, and the player
  takes no further action this hand. `current_bet` is unchanged, `acted` is set.
  The shortfall creates a side-pot boundary (A7).
* **All-in raise for less** (the incomplete raise). If a player raises all-in
  with `increment < last_full_raise`, the raise is legal (all-in is always
  legal), `current_bet` rises to their total, but `last_full_raise` is unchanged
  and betting is not reopened for players already acted per A5.
* **Over-calling.** A player behind an all-in-for-less must still call the full
  `current_bet`, not the short all-in amount. Example: `current_bet = 200`, D is
  all-in for 200, and earlier B went all-in for 125. E to act must call 200,
  not 125. This falls straight out of `to_call = current_bet -
  committed_this_round[p]`; there is no special case.
* **Uncalled excess is returned.** If, when the hand ends, the top layer of
  commitment has exactly one contributor, that layer is returned to its
  contributor before pots are awarded, never formed into a pot. See A7; the
  layering algorithm does this automatically.

## A7. Pot and side-pot construction

Pots are derived, never incrementally mutated. At the moment they are needed
(hand end, or for display), compute them as a pure function of
`committed_this_hand[]` and `folded[]`. This is the property that makes two
independent implementations agree bit-for-bit, and it makes SPEC_CS.md §15's
`STATE_HASH` well-defined.

```
fn build_pots(committed: &[u64], folded: &[bool]) -> (Vec<Pot>, Vec<(Seat,u64)>) {
    let mut levels: Vec<u64> = committed.iter().copied().filter(|&c| c > 0).collect();
    levels.sort_unstable();
    levels.dedup();

    let mut pots = Vec::new();
    let mut refunds = Vec::new();
    let mut prev = 0u64;

    for lvl in levels {
        let contributors: Vec<Seat> =
            seats().filter(|&s| committed[s] > prev).collect();
        let amount_each = lvl - prev;
        let size = amount_each * contributors.len() as u64;

        if contributors.len() == 1 {
            // nobody could contest this layer -> uncalled, give it back
            refunds.push((contributors[0], size));
        } else {
            let eligible: Vec<Seat> =
                contributors.iter().copied().filter(|&s| !folded[s]).collect();
            pots.push(Pot { size, eligible });
        }
        prev = lvl;
    }
    (pots, refunds)
}
```

Properties, all of which must be asserted:

* `sum(pots.size) + sum(refunds.1) == sum(committed)`. Exact, by construction.
* Pot `i` is the "main pot"; pots `1..` are side pots in ascending order of the
  all-in level that created them.
* A folded player's chips **are** in the pots (they contributed), but the folded
  player is in no `eligible` set.
* `eligible` is never empty for a legally reachable state: the last player to
  put chips into a layer voluntarily is by definition not folding at that point.
  The engine MUST still assert non-emptiness and, if it is ever violated (only
  reachable through a byzantine peer), split that pot pro rata among its
  contributors rather than losing chips.
* TDA 21, verbatim: "Each side pot will be split separately."
  Verification: (primary text). The algorithm satisfies this by construction:
  each pot is awarded independently to the best hand among *its own* eligible set.

### Four-player worked example, three different all-in amounts

NLHE, blinds 50/100. Seats clockwise: `S1, S2, S3, S4`. Button = `S1`, so
`S2 = SB`, `S3 = BB`, `S4 = UTG`.

Starting stacks: `S1 = 1000`, `S2 = 300`, `S3 = 600`, `S4 = 200`.
Total chips at the table = 2100.

Pre-flop. After blinds: `S2` committed 50, `S3` committed 100,
`current_bet = 100`, `last_full_raise = 100`, nobody has acted.

| # | player | action | commit | `current_bet` | `last_full_raise` | note |
|---|---|---|---|---|---|---|
| 1 | S4 | all-in **200** | 200 | 200 | inc 100 ≥ 100 → **100** | a full raise |
| 2 | S1 | Call 200 | 200 | 200 | 100 | 800 behind |
| 3 | S2 | all-in **300** | 300 | 300 | inc 100 ≥ 100 → **100** | a full raise |
| 4 | S3 | all-in **600** | 600 | 600 | inc 300 ≥ 100 → **300** | a full raise |
| 5 | S1 | `can_reopen`? | `600 - 200 = 400 ≥ 300` → **true** | | | may re-raise to ≥ 900 |
| 6 | S1 | Call 600 | 600 | 600 | 300 | 400 behind |

Betting is closed: only `S1` is not all-in, and one non-all-in player cannot bet.
The flop, turn and river are dealt with no further action.

Final `committed_this_hand`: `S1 = 600`, `S2 = 300`, `S3 = 600`, `S4 = 200`.
Sum = 1700. `S1` has 400 behind. 1700 + 400 = 2100 ✔.

Distinct levels ascending: `200, 300, 600`.

| Layer | range | contributors (`committed > prev`) | size | eligible |
|---|---|---|---|---|
| **Main pot** | 0 → 200 | S1, S2, S3, S4 | `200 × 4 = ` **800** | S1, S2, S3, S4 |
| **Side pot 1** | 200 → 300 | S1, S2, S3 | `100 × 3 = ` **300** | S1, S2, S3 |
| **Side pot 2** | 300 → 600 | S1, S3 | `300 × 2 = ` **600** | S1, S3 |

`800 + 300 + 600 = 1700` ✔, no refunds (every layer had ≥ 2 contributors).

Note that S4, the shortest stack, is eligible for the main pot only; S2 is
eligible for the main pot and side pot 1 but not side pot 2; S1 and S3 are
eligible for all three.

Award, assuming showdown strength `S4 > S3 > S1 > S2`:

* Main pot 800 → best among {S1,S2,S3,S4} = **S4** → S4 finishes with 800.
* Side pot 1 300 → best among {S1,S2,S3} = **S3**.
* Side pot 2 600 → best among {S1,S3} = **S3** → S3 finishes with 900.
* S1 keeps the 400 behind; S2 is eliminated with 0.

`800 + 900 + 400 + 0 = 2100` ✔.

### Uncalled-excess example

Same table, heads-up between `S1` (1000) and `S3` (500). On the river `S1` bets
800 and `S3` calls all-in for 500. `committed = {S1: 800, S3: 500}`.

Levels: `500, 800`.

| Layer | contributors | size | outcome |
|---|---|---|---|
| 0 → 500 | S1, S3 | 1000 | pot, eligible {S1, S3} |
| 500 → 800 | S1 only | 300 | **refund 300 to S1** |

No player may win more than they can be called for; the single-contributor test
in `build_pots` is that rule. It also handles the "bet, everyone folds" case
correctly, and the "A bets 300, B raises all-in to 900, A folds" case: layers are
`0→300` (contributors A, B; eligible {B} because A folded) and `300→900`
(contributor B alone → refunded), so B wins A's 300 and gets 600 back.

## A8. Showdown, split pots, and odd chips

### Awarding a pot

For each pot in order (main first), find `best = max` hand rank (A10) over its
`eligible` set. Winners `W` = all eligible players tied at `best`.

`|W| == 1`: the whole pot goes to that player.

`|W| > 1`: split.

```
share     = pot.size / |W|            // integer division
remainder = pot.size % |W|            // 0 <= remainder < |W|
```

Each winner receives `share`. The `remainder` odd chips are handed out one each,
in **clockwise order starting from the first seat left of the button position**,
to the first `remainder` winners in that order.

> **TDA 20, verbatim:** "First, odd chips will be broken into the smallest
> denomination in play. A) Board games with 2 or more high or low hands: the odd
> chip goes to the first seat left of the button."
>
> Verification: (primary text).

We adopt convention (A) — *first seat left of the button* — and generalise it
from one odd chip to `remainder` odd chips by continuing clockwise. Why this
convention and not "high card by suit":

1. It is the TDA rule for board games (hold'em is a board game); high-card-by-suit
   is TDA 20-B, explicitly scoped to stud/razz.
2. It is a pure function of `(button_position, winner set)` — public state that
   every peer already agrees on and that is already in the hash chain. The suit
   convention would require agreeing on a suit ordering *and* on which five cards
   constitute "the winning hand", which is not uniquely defined when a player has
   two equally-good best-fives.
3. Ordering by seat needs no card data at all, so it cannot leak anything and
   cannot disagree with the evaluator.

Note the ordering starts from the **button position**, which under the dead-button
rule (A1.3) may be an empty seat. That is fine — it is still a well-defined index.

Worked example: main pot 800 from the four-player example above, but S1, S2 and
S4 tie for best hand (S3 is beaten). `|W| = 3`, `share = 266`, `remainder = 2`.
Button is `S1`, so clockwise order from `succ(S1)` is `S2, S3, S4, S1`; restricted
to `W` that is `S2, S4, S1`. The 2 odd chips go to `S2` and `S4`.
Result: `S2 = 267`, `S4 = 267`, `S1 = 266`. Sum `= 800` ✔.

### Showdown order and who must show

If all but one player folds, there is no showdown: the last live player wins
every pot they are eligible for and never reveals.

Otherwise:

* **If at least one player is all-in and all betting is complete** —
  > **TDA 16, verbatim:** "All hands will be tabled without delay once a player
  > is all-in and all betting action by all other players in the hand is
  > complete. No player who is either all-in or has called all betting action may
  > muck their hand without tabling. All hands in both the main and side pot(s)
  > must be tabled and are live."
  >
  > Verification: (primary text).

  Everyone still live must reveal. No mucking.

* **Non-all-in showdown** —
  > **TDA 17-A, verbatim:** "In a non all-in showdown, if cards are not
  > spontaneously tabled or discarded, the TD may enforce an order of show. The
  > last aggressive player on the final betting round (final street) must table
  > first. If there was no final round bet, the player who would act first in a
  > final betting round must table first (i.e. first seat left of the button in
  > flop games...)."
  >
  > **TDA 17-B, verbatim:** "A non all-in showdown is uncontested if all but one
  > player mucks face down without tabling. The last player with live cards wins
  > and is not required to table the cards."
  >
  > Verification: (primary text).

  So: `show_first = last player to bet or raise on the river`, or if the river
  was checked through, `show_first = ` the player who acts first on the river
  (A2), then clockwise. Each subsequent player may either reveal or muck.

### Design question (NOT decided here): mucking in a verifiable transcript

There is a real conflict between two SPEC_CS.md requirements and it must be
resolved deliberately, in `docs/PROTOCOL.md`, before the engine is written.

* §13 requires that after the hand anyone can verify "that the winner and the pot
  size were computed correctly".
* §9/§35 require that a player's hole cards stay undecryptable by anyone else
  until the rules force a reveal.
* Live poker's mucking right exists *because* hiding a losing hand has strategic
  value, and PokerTH honours it.

If a beaten player is allowed to muck, the transcript no longer contains the
information needed to check the award independently — a verifier can only confirm
"the revealed hands were ranked correctly", not "no unrevealed hand was better".
That is a genuine weakening: a colluding pair could have one player muck a
winner. If instead every showdown participant must reveal, verification is total
but the game leaks strictly more information than real poker does, which is
itself an exploitable edge over the long run.

Three candidate resolutions, stated without choosing:

1. **Mandatory universal reveal at showdown.** Simplest, fully verifiable,
   changes the game.
2. **TDA-faithful mucking with a binding forfeiture.** A muck is a signed
   `SHOWDOWN_MUCK` event that irrevocably forfeits all claim to every pot. The
   transcript then verifies "the award was correct *given the set of players who
   did not forfeit*", which is a weaker but precisely stateable claim. The
   mucked hole cards are additionally revealed to nobody but are committed to, so
   an end-of-tournament audit can optionally open them.
3. **Delayed reveal.** Mucked hands stay sealed during play and their decryption
   shares are published at the end of the *tournament*, so live strategic value
   is preserved but the full transcript eventually becomes checkable. Requires
   that shares be escrowed in a way a disconnect cannot destroy — which reopens
   SPEC_CS.md §19.

Each option changes the cryptographic protocol, not just the engine, so the
decision belongs to Phase 1 and must be recorded in `docs/DECISIONS.md`.

## A9. Automatic progression to the next hand

SPEC_CS.md §4 requires the next hand to start automatically and deterministically,
"ne řízený tím, kdo první klikne". Normatively: `HAND_COMPLETE` for hand `h` is
itself the trigger for `HAND_INIT` of hand `h+1`. The next hand's
`(button_pos, sb_pos, bb_seat, level, SB, BB)` are a pure function of hand `h`'s
end state (A1.3, and Part B §B2 for the level), so every peer derives them
independently and no one announces them.

## A10. Hand evaluation — 7-card best-five

A hand is exactly **five** cards chosen from the player's 2 hole cards and the 5
board cards — that is, the best 5 of 7. A player may use both, one, or neither
hole card ("playing the board").

Category order, weakest to strongest:

1. High card
2. One pair
3. Two pair
4. Three of a kind
5. Straight
6. Flush
7. Full house
8. Four of a kind
9. Straight flush

Tie-breaking, in every case comparing exactly five cards:

| Category | Compare in this order |
|---|---|
| High card | the five ranks, descending |
| One pair | pair rank, then the 3 kickers descending |
| Two pair | higher pair, lower pair, then the 1 kicker |
| Three of a kind | trips rank, then the 2 kickers descending |
| Straight | rank of the highest card of the straight |
| Flush | the five ranks, descending |
| Full house | trips rank, then pair rank |
| Four of a kind | quads rank, then the 1 kicker |
| Straight flush | rank of the highest card |

Hard rules that catch the usual bugs:

* **Suits never break a tie.** Two hands with identical ranks are exactly equal
  and split the pot.
* **The wheel.** `A-2-3-4-5` is a straight (and a straight flush if suited); the
  ace plays **low** and it is the **lowest** straight, below `2-3-4-5-6`.
* **No wrap-around.** `Q-K-A-2-3` is not a straight. The ace is either high
  (`10-J-Q-K-A`) or low (`A-2-3-4-5`), never both in one hand.
* **Only five cards count.** With 7 cards, a sixth or seventh card can never
  break a tie. `A♣A♦K♣K♦Q♣Q♦2♠` is two pair aces-and-kings with a queen kicker;
  the third pair is irrelevant and it ties with `A♣A♦K♣K♦Q♣7♦2♠`.
  Verification: (compiled) asserted in the probe as
  `assert_eq!(three_pair, two_pair_ref)`.
* **Playing the board.** If both players' best five is the board, they tie
  exactly. Verification: (compiled) asserted in the probe.
* From 7 cards the best five must be selected across *all* categories — a hand
  that contains both a flush and a straight is a flush.

There are exactly **7462** distinct five-card hand strengths.
Verification: (compiled) the probe enumerates all `C(52,5) = 2 598 960` hands and
observes exactly 7462 distinct rank values.

---

# PART A′ — Rust hand-evaluator crates

Everything in this section was checked against the crates.io API (with the
required `User-Agent`) and against the unpacked crate sources under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`, and the
three shortlisted crates were compiled and executed on
`rustc 1.95.0 (59807616e 2026-04-14)` / `cargo 1.95.0`, target
`x86_64-pc-windows-msvc`.

## A′1. Survey

| Crate | Max stable | Licence | Downloads | Last update | 7-card? | Status here |
|---|---|---|---|---|---|---|
| `rs_poker` | 5.1.0 | Apache-2.0 | 90 624 | 2026-08-13 | yes | **5.1.0 does not build on stable 1.95.0**; `=5.0.0` works |
| `aya_poker` | 0.1.0 | Zlib OR Apache-2.0 OR MIT | 5 822 | 2023-10-31 | yes | builds, but its `std` feature is **broken** |
| `ckc-rs` | 0.1.18 | Apache-2.0 | 28 600 | 2025-06-24 | **no** (5-card only) | builds, correct |
| `poker` | 0.7.0 | MIT | 22 536 | 2025-06-15 | **no** (3- and 5-card only) | not usable directly for hold'em |
| `rust_poker` | 0.1.14 | MIT | 20 911 | 2021-03-04 | yes | stale (5 years), build-script table generation |
| `pokers` | 0.10.0 | MIT | 15 571 | 2026-03-29 | — | simulator-oriented, not probed |
| `pkcore` | 0.9.0 | MIT OR Apache-2.0 | 3 372 | 2026-08-26 | — | self-described "Prototype", MSRV 1.94.1, 5.2 MB crate |
| `espada` | 0.5.2 | **non-standard** | 5 528 | 2026-08-26 | — | licence field is `non-standard` on crates.io — a compliance risk, not probed |
| `holdem-hand-evaluator` | — | — | — | — | — | **does not exist on crates.io** (404) |
| `mental-poker` | 0.1.0 | — | — | 2022 | — | out of scope here (crypto, not evaluation) |

Verification for the whole table: crates.io API `GET /api/v1/crates/<name>` with
`User-Agent: p2p-poker-research (<your-email>)`; licence taken from the
newest non-yanked version record.

## A′2. Verified negative results

**`rs_poker` 5.1.0 does not compile on stable Rust 1.95.0.** It calls an unstable
library method:

```
error[E0658]: use of unstable library feature `isolate_most_least_significant_one`
  --> .../rs_poker-5.1.0/src/core/card_bit_set.rs:49:27
   |
49 |         let lowest = mask.isolate_lowest_one();
   |                           ^^^^^^^^^^^^^^^^^^
   = note: see issue #136909 <https://github.com/rust-lang/rust/issues/136909>
```

The call sits in `pdep_fallback`, the software fallback for the BMI2 `_pdep_u64`
path. It is not behind any cargo feature, so `--no-default-features` does not
help. Verification: (compiled — failed) and (source): the file and line above
were read directly. **`=5.0.0` compiles cleanly** and was used for everything
below.

**`aya_poker`'s `std` feature is broken.** With `--features std`:

```
error[E0433]: cannot find module or crate `std` in this scope
  --> .../aya_base-0.1.0/src/card.rs:88:6
   |
88 | impl std::error::Error for ParseError {}
```

Root cause, read in the source: `aya_base-0.1.0/src/lib.rs:1` is

```rust
#![cfg_attr(not(any(std, test)), no_std)]
```

— a bare `std` cfg predicate, which is never set, instead of `feature = "std"`.
So the crate is always `no_std`, while `card.rs:88` is gated on
`#[cfg(feature = "std")]` and references `std`. The two gates disagree.
`aya_poker` is usable **only** with default features (no `std`), and it has had no
release since 2023-10-31. Verification: (compiled — failed) then (source).

**`poker` 0.7.0 cannot evaluate a 7-card hold'em hand.** Its only entry points
are `evaluate_three` and `evaluate_five` (`src/evaluate/evaluation.rs:19,43`,
`src/evaluate/evaluator.rs:91,127`, `src/evaluate/static_lookup.rs:135,141`).
Using it would mean iterating the 21 five-card subsets by hand. Verification:
(source). `ckc-rs` has the same limitation: `evaluate::five_cards([CKCNumber; 5])`
only (`src/lib.rs:332`).

## A′3. Verified positive results

All from the probe crate, `cargo build --release` + `cargo test --release`:

* **`rs_poker` `=5.0.0`, `default-features = false`** evaluates 7 cards via
  `rs_poker::core::Rankable::rank()`, which is implemented for `FlatHand`,
  `Hand`, `CardBitSet`, `Vec<Card>` and `[Card]` (`src/core/rank.rs:461-495`).
  `Rank` is a `#[repr(transparent)] u16` laid out as `(category << 12) | subrank`
  and derives `Ord`, so comparison is one integer compare. Its build script
  reimplements zekyll's OMPEval (MIT) and asserts the 7462 count at build time
  (`build.rs` header). Minimal compiling snippet:

  ```rust
  use rs_poker::core::{CoreRank, FlatHand, Rankable};

  let quads = FlatHand::new_from_str("7c7d7h7s2c3d4h").unwrap().rank();
  let boat  = FlatHand::new_from_str("AcAdAhKcKd2h3s").unwrap().rank();
  assert_eq!(quads.category(), CoreRank::FourOfAKind);
  assert!(quads > boat);
  ```

  Verification: (compiled and run).

* **14 known-hand assertions pass**: royal flush from 7 cards, wheel straight
  flush is the weakest straight flush, quads beat a boat, a 7-card hand with both
  a flush and a straight ranks as a flush, kicker discrimination on identical two
  pair, a third pair is ignored, playing the board ties exactly, the wheel is the
  lowest straight, `Q-K-A-2-3` is only a high card, and rank is independent of
  card order. Verification: (compiled and run).

* **Exhaustive class count**: all `C(52,5) = 2 598 960` hands map onto exactly
  **7462** distinct `Rank` values. Verification: (compiled and run).

* **Three-way exhaustive cross-validation.** Over all 2 598 960 five-card hands,
  `rs_poker 5.0.0`, `aya_poker 0.1.0` and `ckc-rs 0.1.18` induce the **same total
  order**: the maps `rs → aya` and `rs → ckc` are bijections on 7462 classes,
  strictly monotone (increasing and decreasing respectively — `ckc-rs` ranks
  1 = best), with `rs`'s weakest hand ↔ `ckc` 7462 and `rs`'s best ↔ `ckc` 1.
  Zero disagreements. Verification: (compiled and run), test
  `three_way::rs_poker_aya_poker_and_ckc_rs_all_agree_on_all_c52_5`.

  **Caveat on independence:** `rs_poker`'s tables and `aya_poker` are *both*
  derived from zekyll's OMPEval, so those two agreeing is weak evidence.
  `ckc-rs` is Cactus-Kev lineage and genuinely independent, so the three-way
  agreement is the result that actually counts.

* **7-card random cross-check**: 300 000 random 7-vs-7 comparisons between
  `rs_poker` and `aya_poker`, **0 disagreements**. (`ckc-rs` was excluded here
  because it has no 7-card entry point.) Verification: (compiled and run).

* **Throughput**, release build, 200 000 pre-parsed 7-card hands, single thread,
  6 runs on this machine (an untuned in-process `Instant` measurement on a busy
  desktop, **not** a `criterion` benchmark — treat it as an order of magnitude,
  not a number):
  * `rs_poker` 5.0.0: 42.1, 45.2, 50.3, 52.8, 106.6, 111.7 ns/eval — **~40–110 ns**
  * `aya_poker` 0.1.0: 4.7, 4.9, 5.4, 6.4, 6.5, 7.1 ns/eval — **~5–7 ns**

  Verification: (compiled and run, 6 repetitions). `rs_poker`'s spread is wide
  because the loop also walks a 200 000-entry `Vec<FlatHand>`; `aya_poker`'s
  `Hand` is a single `u64`-backed value and stays in cache. The ranking result is
  stable across every run: `aya_poker` is roughly an order of magnitude faster.

  This does not decide anything. Both are several orders of magnitude faster than
  the mental-poker reveal that precedes them — one hand needs on the order of ten
  evaluations, against several elliptic-curve operations per card — so evaluator
  speed is **not** a design constraint for this project, and it must not be
  allowed to outweigh maintenance status or licence.

* **Dependency weight** (`cargo tree`, verification: (compiled)):
  * `rs_poker 5.0.0 --no-default-features` → `rand`, `thiserror` (+ proc-macro
    chain). Its **default** features (`arena`, `omaha`, `open-hand-history`,
    `serde`) drag in `tokio`, `chrono`, `serde_json`, `tracing`, `parking_lot` —
    96 packages in the probe's initial lock file. Always disable them.
  * `aya_poker 0.1.0` → `aya_base` → `fastrand`, plus `quickdiv`. Three crates
    total; by far the lightest.
  * `ckc-rs 0.1.18` → pulls `serde` and `strum` (+ `syn`), heavier than it looks
    for a 5-card evaluator.

## A′4. Recommendation

**Use `rs_poker = { version = "=5.0.0", default-features = false }` for the MVP,
behind our own façade, and keep the three-way exhaustive test as a permanent
regression gate.**

Reasons: it is the only shortlisted crate that is both actively maintained
(2026-08) and has a real 7-card entry point; Apache-2.0 is compatible with the
project; the `--no-default-features` dependency set is two crates; and its
correctness is now backed by an exhaustive proof against an independent lineage.

Three architectural constraints that follow from the decentralised setting and
matter more than the crate choice:

1. **Never put a third-party evaluator's raw score into the hash-chained state.**
   `Rank`'s numeric encoding is an internal detail of one crate version; a table
   regeneration or a version bump would change `STATE_HASH` on some peers and
   manufacture false disputes (SPEC_CS.md §15). Only the *derived* result — the
   set of winners per pot and the chip deltas — may enter the canonical state.
   Wrap the crate in `src/poker/evaluator.rs` as
   `fn evaluate7(cards: &[Card; 7]) -> HandStrength` where `HandStrength` is our
   own opaque `Ord` newtype that is explicitly **not** serialised.
2. **Pin exactly (`=5.0.0`) and commit `Cargo.lock`** (SPEC_CS.md §28). A
   semver-compatible auto-upgrade to 5.1.0 would break the build outright on this
   toolchain, which at least fails loudly — but a future 5.x could silently
   change the encoding, which would not.
3. **Watch the `unsafe` BMI2 path.** `rs_poker`'s `CardBitSet::pdep` selects
   between `unsafe { core::arch::x86_64::_pdep_u64(..) }` and a software loop via
   a **compile-time** `#[cfg(all(target_arch = "x86_64", target_feature = "bmi2"))]`
   (`rs_poker-5.0.0/src/core/card_bit_set.rs:31-57`). Verification: (source).
   This is a compile-time cfg, not runtime dispatch, so the *same source* can
   produce two different code paths on two peers depending on `RUSTFLAGS` /
   `-C target-cpu`. Both compute the same pure function, so determinism holds —
   but that is a property to *test*, not assume. Ship a release profile with a
   fixed baseline target and run the exhaustive evaluator test on every target we
   publish binaries for.

   Incidentally, this exact function is what breaks 5.1.0: 5.0.0's fallback uses
   the stable `mask & mask.wrapping_neg()` (line 49), and 5.1.0 replaced that one
   expression with the unstable `mask.isolate_lowest_one()`. Verification:
   (source), both files read.

**Fallback if the crate ever becomes a problem:** write our own. A Cactus-Kev
style 7-card evaluator is roughly 200 lines plus a generated table, the algorithm
is public and well documented, and we now have an exhaustive oracle
(`ckc-rs` + `aya_poker`) to validate it against in a test-only dev-dependency.
That is a realistic option precisely because the correctness bar is a finite,
fully enumerable 2 598 960-case check — do not treat "write our own evaluator" as
risky the way "write our own crypto" is.

---

# PART B — the `RATED_SNG_POKERTH_V1` preset

SPEC_CS.md §4 requires one named preset with fixed seat count, equal starting
stacks, a fixed blind schedule that raises after a given number of hands, and
action/hand time limits, with the values derived from PokerTH's default rated
Sit-and-Go. PokerTH calls this game type `GAME_TYPE_RANKING`.

The authoritative definition in PokerTH is `ServerGame::CheckSettings`, which
*rejects* any rated game whose settings differ from the constants. That is the
strongest possible evidence of what "the rated preset" means: these are not
defaults a user can change, they are enforced.

`~/src/pokerth/src/net/servergame.cpp:1258-1273`:

```cpp
if (mode != SERVER_MODE_LAN) {
    if (data.playerActionTimeoutSec < 5) { retVal = false; }
}
if (data.gameType == GAME_TYPE_RANKING) {
    if ((data.startMoney != RANKING_GAME_START_CASH)
            || (data.maxNumberOfPlayers != RANKING_GAME_NUMBER_OF_PLAYERS)
            || (data.firstSmallBlind != RANKING_GAME_START_SBLIND)
            || (data.raiseIntervalMode != RAISE_ON_HANDNUMBER)
            || (data.raiseMode != DOUBLE_BLINDS)
            || (data.raiseSmallBlindEveryHandsValue != RANKING_GAME_RAISE_EVERY_HAND)
            || (!password.empty())
            || (!data.allowSpectators)) {
        retVal = false;
    }
}
```

## B1. Values enforced by PokerTH's rated game

| Parameter | Value | Source file:line |
|---|---|---|
| Seats (`max_players`) | **10** | `src/game_defs.h:80` — `#define RANKING_GAME_NUMBER_OF_PLAYERS 10` |
| Starting stack (all players) | **10 000** | `src/game_defs.h:79` — `#define RANKING_GAME_START_CASH 10000` |
| First small blind | **50** | `src/game_defs.h:81` — `#define RANKING_GAME_START_SBLIND 50` |
| Big blind | **2 × small blind** = 100 | `src/engine/local_engine/localberopreflop.cpp:44` (`setHighestSet(2*getSmallBlind())`), `src/engine/local_engine/localhand.cpp:490-529` |
| Ante | **none** | no ante concept exists in `src/engine/` (grep) |
| Blind raise trigger | by **hand number** (`RAISE_ON_HANDNUMBER`) | `src/net/servergame.cpp:1266`; enum at `src/gamedata.h:57-60` |
| Raise every N hands | **11** | `src/game_defs.h:82` — `#define RANKING_GAME_RAISE_EVERY_HAND 11` |
| Raise mode | **double the blinds** (`DOUBLE_BLINDS`) | `src/net/servergame.cpp:1267`; enum at `src/gamedata.h:62-65`; applied at `src/engine/game.cpp:302` |
| Small-blind ceiling | `min(SB, max_players × start_stack / 2)` = **50 000** | `src/engine/game.cpp:318` — `currentSmallBlind = min(currentSmallBlind, startQuantityPlayers*startCash/2);` |
| Table password | must be **empty** | `src/net/servergame.cpp:1269` |
| Spectators | must be **allowed** | `src/net/servergame.cpp:1270` |
| Table size limits (general) | min 2, max 10 | `src/game_defs.h:34-35` |
| Payout structure | **none** — only the finishing place is recorded | `src/net/servergame.cpp:475-488` (`StoreAndResetRanking` → `SetGamePlayerPlace`) |

## B2. The blind schedule

The raise test is at `src/engine/game.cpp:286-291`:

```cpp
if (myGameData.raiseIntervalMode == RAISE_ON_HANDNUMBER) {
    if (lastHandBlindsRaised + myGameData.raiseSmallBlindEveryHandsValue <= currentHandID) {
        raiseBlinds = true;
        lastHandBlindsRaised = currentHandID;
    }
}
```

with `lastHandBlindsRaised` initialised to `1` (`src/engine/game.cpp:52`) and
`currentHandID` initialised to `0` (`src/engine/game.cpp:51`) and incremented at
the top of `initHand()` before `raiseBlinds()` is called
(`src/engine/game.cpp:158-161`). Hands are therefore numbered from 1, and the
first raise fires on hand `1 + 11 = 12`. Each level lasts exactly 11 hands.

| Level | Hands | Small blind | Big blind |
|---:|---|---:|---:|
| 1 | 1 – 11 | 50 | 100 |
| 2 | 12 – 22 | 100 | 200 |
| 3 | 23 – 33 | 200 | 400 |
| 4 | 34 – 44 | 400 | 800 |
| 5 | 45 – 55 | 800 | 1 600 |
| 6 | 56 – 66 | 1 600 | 3 200 |
| 7 | 67 – 77 | 3 200 | 6 400 |
| 8 | 78 – 88 | 6 400 | 12 800 |
| 9 | 89 – 99 | 12 800 | 25 600 |
| 10 | 100 – 110 | 25 600 | 51 200 |
| 11 | 111 – 121 | **50 000** (capped) | 100 000 |
| 12+ | 122 – … | 50 000 (stays capped) | 100 000 |

Closed form for hands, with `level(h) = 1 + floor((h - 1) / 11)`:

```
small_blind(h) = min(50 * 2^(level(h) - 1),  50_000)
big_blind(h)   = 2 * small_blind(h)
```

The cap is `max_players * start_stack / 2 = 10 * 10000 / 2 = 50 000`, i.e. half
of all the chips in the tournament; level 11 would be 51 200 and is clamped. In
practice a 10-handed 10 000-chip Sit-and-Go is over long before level 11.

Note the cap uses `startQuantityPlayers` — the number of players the game
*started* with, not the number still alive — so it is a constant for the whole
tournament and is safe to precompute.

## B3. Values the rated game does NOT fix

`CheckSettings` deliberately leaves the two timing values free. The only
constraint is a floor of **5 s** on the action timeout for non-LAN servers
(`src/net/servergame.cpp:1257-1260`). PokerTH's *application* defaults are
therefore the best available guidance, and they are not unanimous:

| Value | PokerTH number | Source file:line |
|---|---|---|
| Action timeout, `GameData` struct default | 20 s | `src/gamedata.h:80` |
| Action timeout, saved-config default `NetTimeOutPlayerAction` | 20 s | `src/config/configfile.cpp:233` |
| Action timeout, QML client hard-code | 20 s | `src/gui/qt6-qml/cpp/createlocalgameviewimpl.cpp:149` |
| Server grace added on top of the action timeout | +2 s | `src/net/servergamestate.cpp:80`, used at `src/net/servergamestate.cpp:1600` |
| `0` means unlimited thinking time | — | `src/net/servergamestate.cpp:1596` |
| Delay between hands, `GameData` struct default | 6 s | `src/gamedata.h:80` |
| Delay between hands, saved-config default `NetDelayBetweenHands` | **7 s** | `src/config/configfile.cpp:232` |
| Delay between hands, QML client hard-code | 7 s | `src/gui/qt6-qml/cpp/createlocalgameviewimpl.cpp:148` |
| Autostart delay once the table is full | 6 s | `src/net/servergamestate.cpp:84`, triggered at `src/net/servergamestate.cpp:576-581` |
| Start-game timeout | 20 s | `src/net/servergamestate.cpp:83` |
| AFK warning / kick | 1200 s / 1260 s | `src/net/servergamestate.cpp:87-88` |
| Per-hand time limit | **[NOT IN POKERTH]** | no such concept exists |

The 6 vs 7 discrepancy on the hand delay is real: the C++ struct default is 6
(`src/gamedata.h:80`) but every path that actually creates a game reads 7 from
config or hard-codes 7. A game created through the UI gets 7.

## B4. `RATED_SNG_POKERTH_V1` — the preset as we will ship it

Values marked **[OUR CHOICE]** are not PokerTH's; everything else is B1/B2.

```
preset_id                  = "RATED_SNG_POKERTH_V1"
game                       = NLHE
mode                       = TOURNAMENT_SNG_PLAY_MONEY
seats                      = 10                    // PokerTH RANKING_GAME_NUMBER_OF_PLAYERS
min_players_to_start       = 10                    // [OUR CHOICE] SNG starts when full
start_stack                = 10000                 // PokerTH RANKING_GAME_START_CASH
first_small_blind          = 50                    // PokerTH RANKING_GAME_START_SBLIND
big_blind                  = 2 * small_blind
ante                       = 0                     // PokerTH has no ante
blind_raise_mode           = DOUBLE_EVERY_N_HANDS
blind_raise_every_hands    = 11                    // PokerTH RANKING_GAME_RAISE_EVERY_HAND
small_blind_cap            = 50000                 // PokerTH seats*stack/2
password                   = none                  // PokerTH forbids it for rated games
button_rule                = DEAD_BUTTON           // [OUR CHOICE] TDA 32, not PokerTH
odd_chip_rule              = FIRST_SEAT_LEFT_OF_BUTTON   // TDA 20-A
action_timeout_sec         = 20                    // PokerTH app default
action_timeout_grace_sec   = 5                     // [OUR CHOICE] was 2 in PokerTH
hand_delay_sec             = 7                     // PokerTH config default
hand_deadline_sec          = 600                   // [OUR CHOICE] no PokerTH analogue
join_deadline_sec          = 120                   // [OUR CHOICE] no PokerTH analogue
payouts                    = none (finishing place only)   // PokerTH records place only
```

Justification for the four **[OUR CHOICE]** timing values, which SPEC_CS.md §4
and §19 both require and PokerTH does not supply:

* `min_players_to_start = 10`: PokerTH autostarts a rated game the moment the
  table is full (`src/net/servergamestate.cpp:576-581`), which for a 10-seat
  rated game means 10 players. Making it explicit removes any ambiguity about
  whether a short-handed rated game is legal. It is not.
* `action_timeout_grace_sec = 5` rather than PokerTH's 2. PokerTH's grace only
  has to cover one client→server hop on a trusted clock. Here every peer times
  out independently, with no shared clock, and the grace must absorb P2P round
  trip, relay hops for CGNAT peers (SPEC_CS.md §1), and signature verification.
  Every peer MUST use the identical constant or peers will disagree about whether
  a timeout fired — which is a consensus fault, not a UX detail.
* `hand_deadline_sec = 600`: SPEC_CS.md §4 asks for a time limit "na akci a na
  handu", and §19 requires a hand abort with attributable evidence when a peer
  stops cooperating with the mental-poker protocol. A per-action timeout does not
  cover a peer that stalls *inside* a cryptographic step, which is exactly the
  abort attack. 600 s comfortably exceeds any legitimate hand.
* `join_deadline_sec = 120`: bounds the seating phase so an advertised table
  cannot be pinned open forever by a peer that joins and never readies.

All of these are part of the signed table advertisement (SPEC_CS.md §4), so every
joining player sees them before the first hand and all participants agree on them
before any cards exist.

## B5. What is deliberately not taken from PokerTH

* **Button rotation.** PokerTH shifts the button to the next *surviving player*
  (`src/engine/game.cpp:191-210`, with its own `TODO exception-rule !!!`
  comment), so a player can post the big blind twice in a row after a bust-out.
  We use the TDA dead button (A1.3).
* **Rated-game identity and ranking.** PokerTH's rated game exists to write
  finishing places into a central SQLite/MySQL database
  (`src/net/servergame.cpp:475-488`). There is no such database here
  (SPEC_CS.md §34), so `RATED_SNG_POKERTH_V1` borrows only the *structure* of the
  game, not its ranking. Any reputation layer is a separate concern
  (SPEC_CS.md §18, §19).
* **Spectators.** PokerTH requires rated games to allow spectators. Spectators in
  a mental-poker protocol are a non-trivial cryptographic question (who holds
  decryption shares?), so the preset ships with no spectator support in the MVP
  and the field is simply absent rather than set to `true`.

---

## Appendix — mapping to SPEC_CS.md §11's required state

| §11 name | Type | Defined in |
|---|---|---|
| `table_id` | 32-byte id | protocol layer |
| `hand_id` | `u64`, from 1 | A9, B2 |
| `button` | seat **position** (may be empty) | A1.3 |
| `small blind` / `big blind` | `u64`, `BB = 2*SB` | B2 |
| `player order` | fixed seat ring `0..N-1` | A0 |
| `stack` | `u64` per seat | A0 |
| `current bet` | `u64` | A4 |
| `pot` / `side pots` | derived, `Vec<Pot { size, eligible }>` | A7 |
| `street` | `PreFlop\|Flop\|Turn\|River\|Showdown` | A2 |
| `player_to_act` | seat | A2 |
| `minimum_raise` | derived: `current_bet + last_full_raise` | A4 |
| `last_full_raise` | `u64` | A4 |
| `all-in states` | `all_in[]: bool` | A6 |
| `fold states` | `folded[]: bool` | A3 |
| `board` | `Vec<Card>`, len ∈ {0,3,4,5} | A2 |
| `betting history` | ordered `Vec<ActionRecord>` | A3, §13 transcript |

Additional state this document requires that §11 does not name explicitly, and
without which the rules above cannot be evaluated:

* `committed_this_round[]: u64` — needed by `to_call` and by `can_reopen` (A5).
* `committed_this_hand[]: u64` — needed by `build_pots` (A7).
* `acted_this_round[]: bool` — needed by round termination (A2) and `can_reopen`
  (A5); this is what encodes the big blind's option.
* `sb_position`, `bb_seat` as separate fields from `button` — needed by the dead
  button rotation (A1.3).
