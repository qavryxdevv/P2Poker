# Readmission — the answer to `Q-10`, and what has to exist before it can be built

**Status:** design, not built. Produced 2026-09-05 by four independent designs,
three judges and two adversarial passes, at the project owner's request, after
`S1-BV` measured the door standing open and leading nowhere. Register row
`S1-BM`. Not normative: `PROTOCOL.md` owns the wire and `DECISIONS.md` owns the
decisions, and neither is amended by this file.

---

## 1. The question

A seat certified out of a table can never come back. `next_hand` derives
`R(k+1)` by filtering `R(k)` in both of its branches, so the roster only ever
shrinks — `D-024`, monotone. That makes `D-013`'s promise, *one silent seat
costs exactly one hand*, untrue as shipped.

It was measured on 2026-09-05 in `split125944-9`. A seat lost its link for a few
seconds inside hand #8, was certified out, and then followed the table as a
healthy bystander for six consecutive hands with its checkpoint agreeing every
time. It was never dealt in again. `D-005`'s dead-money rule would have drained
its stack to a bust while it sat there watching.

## 2. The two answers that are cheap, and why the corpus already refused them

The obvious design is to let the returning seat's own signed request add it back:
`PLAYER_SIT_IN` already exists as wire type `0x0804`, is chained, is a boundary
event, and its own doc comment in `src/protocol/messages.rs` calls it *"the sole
route back for a seat that stopped signing (D-013)"*. Nothing consumes it.

That design cannot be built, and the reason is in `PROTOCOL.md` twice, in the
two sections that own the question.

§3.1, on letting a boundary event reach the genesis:

> Letting it reach `roster_hash(k+1)` would derive `GENESIS(k+1)` from *which
> boundary events this receiver happened to hear* — a per-receiver quantity in
> the position D-012 forbids above all others. […] That fork is permanent for
> the *terminal* half and no rule in this document can end it.

§4.10, on why the window's per-receiver close is safe **today**:

> Two receivers can hold different windows […] and nothing collective closes the
> window. That would be fatal if the window fed `GENESIS(k+1)`; §3.1 is what
> stops it […] so a window disagreement reaches canonical state only through
> `HAND_INIT(k+1)`'s **collective, byte-identical** body […] where the two peers
> reject each other's copy and **stage 0 stalls**. That is loud, it is disposed
> of by §8, and both peers still hold the same `GENESIS(k+1)` […] so nothing is
> permanent.

Both passages were read in the tree, not quoted from memory. Together they say
that today's failure of the boundary window is loud, stalled and recoverable,
and that feeding the window into the roster converts it into a silent,
permanent, unrecoverable one. A design that trades the first for the second is
worse than the problem.

**The governing principle, which is the one sentence to keep from this whole
exercise:**

> The grow side of the roster may not introduce a new class of arrival residual.
> It may only reuse the class the project has already accepted.

The shrink side has an arrival residual too — a `TIMEOUT_CERT` that one receiver
gets late — and the project has accepted it, instrumented it and measured it
(`S1-BS`, `D-027`, seen live in `split124308-9`). A certificate is admissible
where a bare request is not, for three reasons that can each be checked:

1. **Redundancy.** A certificate is a collective stage, so as many independent
   byte-identical copies exist as there are healthy seats. A bare request is one
   frame, sent once, by the seat whose link just failed, over a carrier that
   forwards nothing on another client's behalf.
2. **Standalone decidability.** A certificate that carries the subject's own
   signed events can be verified by a receiver that never heard the subject —
   which is exactly the receiver whose link to it is broken. A request can only
   be decided by a receiver that received it.
3. **Standing.** §3.2 admits a set-enlarging event written from an event bound
   to the chain whose set it enlarges. Only the certificate is that.

The checkpoint-band design was refused separately and on its own load-bearing
claim: it rests on `P(k) = R(k)` at the instant `TERMINAL(k)` is fixed, and that
is false in this tree, because a bystander's accepted stage-0 copy sets `signed`
through `note_signed` before the stage test.

## 3. What to build

`R(k+1) = ((R(k) \ OUT(k)) ∪ IN(k)) ∩ ALIVE(k+1)`

where `OUT(k)` is the subject set of complete `TIMEOUT_CERT`s of hand `k` and
`IN(k)` the subject set of complete `RETURN_CERT`s of hand `k`. Both directions
certificate-gated, so neither `P(k)` nor the per-receiver readmission set `A`
ever reaches a required emitter set. Today's code is that expression with
`IN(k)` forced empty, and the corpus routed every reader through that one call
site on purpose: `src/table/hand.rs` says *"which of them `R(k+1)` should be
derived from is `Q-10` … so that when `Q-10` is answered, the edit is one call
site rather than a search."*

A return needs the seat's own signed `PLAYER_SIT_IN` **and** a unanimous
certificate from the seats that would have to wait for it. Everything is
parented on `TERMINAL(k)`, which §4.10 proves agreed on both the settled and the
aborted path, and which is the only anchor in the boundary that survives.

The certificate carries the subject's request **and** the subject's checkpoint
value inside it, so a receiver that never heard the subject can still verify the
whole claim. That is `D-024`'s *the artefact proves its own position*, applied to
the subject rather than to the stage.

**Only on the player's own signed request, never automatically.** In a mesh,
"automatically" can only mean *whoever I have heard from lately*, which is the
forbidden quantity again; it would silently undo the signed intent of a seat that
sat itself out; and the evidence it would use is emitted every hand, so a seat
that is watching would be dealt in against its will. The automation belongs in
the client, which sends the request at every boundary once the predicate holds,
so the player clicks nothing and sees *sitting in at the next hand*. A card room
works exactly this way: a player who steps away is dealt out rather than removed,
and says *deal me in* to come back.

## 4. What has to exist first, and it is not the certificate

**The late-roster repair road is abort-path only.** `late_roster` has one
producer, `src/table/hand.rs`, inside `if self.aborted().is_some()`. A return
certificate is confined to the settled path, because `checkpoint8` is `None` on
the abort path and there is no agreed value for a returning seat to prove it
followed.

So the two do not meet. A `RETURN_CERT` that arrives late would bank, change
`R(k+1)` at whoever received it in time, and notify nobody — and `IN(k)` lands in
`participants`, which §3.1 puts in the permanent-fork class. The design's own
safety argument, *it reuses the accepted class of residual*, fails: it reuses the
class with its repair amputated.

**The prerequisite is therefore a settled-path late-roster repair**, and it is
smaller than the certificate, independently useful, and testable on its own. It
is what `S1-BS`'s road already does after an abort, applied to a hand that
settled.

## 5. Five corrections to `S1-BM` as the register states it

1. **The voter set is `R(k) \ OUT(k)`, before the returns are added.** Written
   from the post-return roster, two simultaneous certificates become mutually
   dependent and two receivers applying them in different orders derive different
   sets.
2. **The settled-path condition is a chain fact, not a local one.** Stated as
   `checkpoint8.is_some()` it comes apart on §4.10's abort-settle race: a peer
   that reached the settlement terminal by the late road holds no checkpoint of
   its own, computes an empty `IN(k)`, and — because the condition is checked at
   verification — refuses the certificate instead of holding it. That is the
   permanent fork with no dissent and no attacker. It must read *`TERMINAL(k)` is
   the `HAND_COMPLETE` stage hash*.
3. **The two-voter floor does not exclude heads-up.** The floor was imported from
   §8.3, where the subject is inside the parent set; here the subject is outside
   `R(k)`, so removing it removes nothing and the floor is satisfied at two
   seats. If heads-up is to be excluded, it has to be excluded by a term that
   says so.
4. **`grace` is an unowned gate between the roster growing and the player
   playing.** A seat can be back in `R(k+1)` and still not dealt in.
5. **A return costs two hands, not one**, and there is no return at a boundary
   the table aborted — which is most of them, because the hand that certifies a
   seat out usually ends in an abort. On a bad carrier the wait is unbounded and
   this design does not bound it.

## 6. The two things a first implementation will get wrong

**Deriving the voter set by generalising `voters()`.** That function reads
`dealt_in`, which reads `grace`, which is a private per-receiver accumulator. A
faithful generalisation would import it into the predicate that decides
`participants` and therefore the genesis. It would pass every unit test, because
`grace` is uniform on a healthy table, and fork the first run where one seat
spends a unit the others did not see it spend. The voter set must be written
fresh, with a test that fails if `dealt_in` appears in it at all.

**Widening `bank`'s settled refusal and stopping there.** It is one line and it
reads like the whole edit. Without the settled-path repair of §4 above, it
produces a certificate that banks, moves the roster at whoever got it in time,
and tells nobody.

## 7. Cost, honestly

About 1 650 lines of source and 700 of tests, across eight corpus documents, and
it is a **major** version item. The vote-and-seal machinery is not duplicated but
generalised, and the late-arrival road already exists. The single largest item
that does not exist at all is §4.10's boundary window for `PLAYER_SIT_IN`: the
wire type is declared, and `Hand::on_event` answers it — and `PLAYER_SIT_OUT` and
`PLAYER_LEAVE` — with *wrong type*. Three declared chained types that the state
machine ignores.

## 8. What is given up

Two hands rather than one. No return at an aborted boundary. One dissenting seat
can keep an honest player out for the life of the table, which is liveness rather
than safety and is exactly what §8.3 already accepts on the shrink side. A
major-version bump before the first release.

## 9. The case for building nothing

`PROTOCOL_MAJOR` has not shipped and there is no population to serve. `D-013`
could be rewritten to say that a seat certified out does not return in version 1,
`Q-10` closed as *not in version 1*, and the four dead mechanisms deleted rather
than made live. The corpus would be smaller and truer.

It loses on the measurement. In `split125944-9` a healthy seat with a working
link, agreeing with the table at six consecutive checkpoints, was locked out of a
tournament it had chips in. Rewriting `D-013` to say *never* makes the protocol's
answer to a four-second disconnection *you forfeit the tournament*, and the
declining pass would not even save the corpus work: the legality gate, the
grace-accrual leak and the three ignored wire types are owed either way.
