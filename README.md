# p2p-poker

Decentralised, serverless No-Limit Texas Hold'em. No house, no dealer, no
trusted third party — the players *are* the dealer, and the deal is proved
rather than promised.

Written in Rust. One portable executable.

> **Status: in progress.** The poker engine, the protocol layer and the mental
> poker construction are complete and tested. Two clients discover each other,
> exchange signed table advertisements, and **form a table** — a roster every
> seat has ratified, ending in a session identity all of them compute
> identically. What is not yet wired is the hand itself: the engine deals one
> between three peers in memory, and nothing yet carries the deck messages
> between two processes. See [`NEXT.md`](NEXT.md).

---

## Why this exists

Online poker asks you to trust a server with the deck. That server knows every
hole card, and you are asked to believe it does not act on them. This project
removes the server instead of trusting it.

A hand is dealt by **mental poker**: every player shuffles the deck under
encryption, proves in zero knowledge that they shuffled it honestly and did not
substitute a card, and no card can be opened without a share from *every* seat.
A player who leaves cannot be dealt around; a player who cheats cannot do so
undetectably. There is no point in the protocol at which any participant, or any
observer, can see a card they are not entitled to.

## How it works

| Layer | What it does |
|---|---|
| **Mental poker** | Barnett–Smart with Bayer–Groth shuffle proofs over secp256k1. `n`-of-`n` threshold ElGamal: every seat holds a share, and a card opens only when all of them are given. |
| **Transcript** | Every event is canonical CBOR, signed Ed25519, and hash-chained. Two players who disagree about what happened can prove which of them is wrong. |
| **Discovery** | BitTorrent Mainline DHT for finding strangers on the open internet; libp2p (QUIC, TCP, Noise/TLS, GossipSub, Kademlia) for talking to them; mDNS for the ones on your own network. |
| **NAT** | AutoNAT v2 decides whether this client is reachable, Circuit Relay v2 carries a hand when it is not, and DCUtR upgrades a relayed connection to a direct one. Relay capacity is judged against **measured** per-hand bytes rather than against a guess. |
| **Poker** | A complete No-Limit Hold'em engine: blinds, dead button, side pots, TDA reopening rules. Around 120 000 random hands run in the test suite. |

Nothing is a service. Every one of those runs inside the same executable on
every player's machine.

## Running it

```bash
cargo build --release
```

```bash
./target/release/p2p-poker
```

The window opens on the lobby. **Create table** advertises one; **Join table**
sits down at somebody else's. The table itself opens in a window of its own,
beside the lobby, and the other players appear in it as they sit down.

A new table is a **rated Sit-and-Go** by default: ten seats, 10 000 chips each,
blinds 50/100 doubling every eleven hands, and it deals when all ten are in.
Those numbers are not a choice — they are `RATED_SNG_POKERTH_V1`, read out of
PokerTH's own `RANKING_GAME_*` constants, and every client derives the same
parameters from the name alone. That is what lets two people who have never
spoken agree on the game before either sits down. A custom cash table is one
click away for everything else.

Two clients on one machine are two players only if they keep two profiles — the
protocol refuses a second seat to the same node at the same table, deliberately:

```bash
./target/release/p2p-poker --headless --profile ./A --host Riverside --for 45
```

```bash
./target/release/p2p-poker --headless --profile ./B --join Riverside --for 40
```

Both print `TABLE FORMED session=…` with the same session identity.

| Flag | |
|---|---|
| `--headless` | the node without a window; a scripted run, or a volunteer relay |
| `--profile DIR` | keep the profile somewhere other than beside the binary |
| `--host NAME` | advertise a table called `NAME` — a rated Sit-and-Go |
| `--seats N` | make it a custom table of `N` seats instead (a rated one needs all ten) |
| `--min N` | how many of them it starts with |
| `--join NAME` | sit down at the first table called `NAME` |
| `--table` | open on the table rather than the lobby |
| `--for N` | stop after `N` seconds |

## It is portable, and that is checked

One executable, around 21 MB, statically linked against the C runtime. Copy it
into an empty folder and it runs, creating exactly `profile/identity.key` and
`profile/player.key` beside itself — two keys, because the protocol keeps the
network identity and the player identity apart. Two folders are two players.

`tools/check-portable.ps1` verifies all of that by measurement, including that
the binary imports no C runtime and that the two identities really are two
different secrets.

## Testing

```bash
cargo test --release -- --test-threads=19
```

Around 600 tests. Some of what they cover:

- **the engine**, against a reference over ~120 000 random hands;
- **the deck**, end to end — shuffle, prove, verify, open, with the proof sizes
  and timings measured rather than assumed;
- **two nodes in one process**, with a real QUIC handshake and a real GossipSub
  mesh, and a signed advertisement crossing it;
- **a relay circuit** that actually carries traffic;
- **a whole table formed over a real connection**, from the first join request
  to a session identity both sides compute identically;
- **the table geometry** — that the player is always at the bottom, that chips
  are never covered by a card, and that nothing is drawn outside the window — at
  every seat count from two to ten and five window sizes.

Two habits this project holds itself to, because both have paid:

**Every guard is verified to bite.** Disable the check, run the test that claims
to cover it, require a failure, restore it. A test that passes whether or not
the thing it guards exists is worse than no test.

**A measurement beats a recollection.** The relay defaults, the proof sizes, the
import table — measured, and several of them disagreed with the text written
around them.

## Documentation

The design is written down before it is built, and the documents are normative:

- [`docs/PROTOCOL.md`](docs/PROTOCOL.md) — the wire protocol, message by message
- [`docs/STATE_MACHINE.md`](docs/STATE_MACHINE.md) — the hand, as a state machine
- [`docs/CRYPTOGRAPHY.md`](docs/CRYPTOGRAPHY.md) — the constructions and why they are sound
- [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) — what a fully modified client can and cannot do
- [`docs/DECISIONS.md`](docs/DECISIONS.md) — every decision, including the ones that were withdrawn
- [`NEXT.md`](NEXT.md) — where the work is now

## What this is not

**It is not a gambling product.** It plays for play money. There is no cashier,
no custody of funds, and no mechanism for settling a real-money debt — putting
one in would be a different project with a different set of legal obligations.

**It is not anonymous.** A client that multi-tables under one application key
links those tables to anyone watching the lobby. Using a distinct key per table
unlinks them and costs nothing.

**It does not solve Sybil.** One key per seat and one node per seat are
enforceable and enforced; one *person* per seat is not, and is not claimed.

## Licence

Not yet chosen.
