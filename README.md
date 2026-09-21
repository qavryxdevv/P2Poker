# p2p-poker

Decentralised, serverless No-Limit Texas Hold'em. No house, no dealer, no
trusted third party — the players *are* the dealer, and the deal is proved
rather than promised.

Written in Rust. One portable executable.

> ## [Download P2Poker](https://github.com/qavryxdevv/P2Poker/releases/latest)
>
> **Windows** — one file, 45 MB: **[p2p-poker.exe](https://github.com/qavryxdevv/P2Poker/releases/latest/download/p2p-poker.exe)**.
> Start it and it offers to install itself — a copy in your user folder and a shortcut on the desktop, no
> administrator rights, nothing in the registry — or runs from the folder it is in. A beta; **play money only**.
>
> **Linux** — x86_64 with glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 and later): an **AppImage** for
> any distribution, a **.deb** for Ubuntu, Debian and Mint, and an **.rpm** for Fedora and openSUSE, on the
> [release page](https://github.com/qavryxdevv/P2Poker/releases/latest). The player profile is kept in
> `~/.local/share/p2poker/profile`. Since 0.1.4 the sounds play on Linux too, through ALSA, which PipeWire and
> PulseAudio both serve.
>
> Every release is built by GitHub from this source and GitHub signs a statement of it:
> [how to check a download](#and-a-download-can-be-checked-against-this-source). It is not signed with a Windows
> certificate, so SmartScreen asks before the first start (*More info*, then *Run anyway*).
>
> Questions — real money, fairness without a server, the Windows warning, Mac and Linux — are answered in the
> **[FAQ](FAQ.md)**.

![The table window with all ten seats taken, in PokerTH's Green Casino style](docs/images/table-ten-seats.png)

*The table window with all ten seats taken, drawn from the client's built-in
sample hand (`--table-preview --preview-seats 10 --preview-odds --preview-chat
--preview-showcase`). The look follows PokerTH's Green Casino table; see
[`assets/pokerth/PROVENANCE.md`](assets/pokerth/PROVENANCE.md).*

> **Status: a playable beta.** Players find each other in a public lobby with no
> server behind it, sit down at Sit-and-Go tables of two to ten, and play whole
> tournaments: every deck is shuffled by all the seats together and every card
> is proved, a seat that goes silent is timed out by a certificate the others
> sign, and a player whose client or line dropped comes back to the same game.
> *Find a game* seats you automatically; an album of cards and quests keeps
> score of nothing but play money. What is open, owed or the owner's to decide is
> kept honestly in the register of [`docs/DECISIONS.md`](docs/DECISIONS.md)
> (`S1-*`), and [`NEXT.md`](NEXT.md) is the working log it grew out of.

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
| **Discovery** | The public libp2p Kademlia DHT: every client announces itself a provider of one agreed key and asks who else is. Relays are found the same way, under `/libp2p/relay`, which is where go-libp2p's own AutoRelay looks. mDNS for players on your own network. |
| **NAT** | Three ways to open a port: UPnP IGD, and PCP and NAT-PMP for the routers that speak those instead. AutoNAT v2 then decides whether this client is *actually* reachable — a router's confirmation is not the same claim. Circuit Relay v2 carries a hand when nothing opens, and DCUtR upgrades a relayed connection to a direct one. Relay capacity is judged against **measured** per-hand bytes rather than against a guess. |
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

**Find a game** does the looking for you: choose a format (heads-up, six seats,
the full ring of ten, or whatever fills first) and how many games you want at once, and
the client reserves seats at the tables closest to starting, founds one of its
own when nothing is on offer, lets that one start with the players who came,
and gives every other seat back the moment a game starts. The window shows the
clock, the estimate, how many others are searching and where seats are held,
and one button, which cancels at once.

A new table is a **rated Sit-and-Go** by default: ten seats, 10 000 chips each,
blinds 50/100 doubling every eleven hands, and it deals when all ten are in.
Those numbers are not a choice — they are `RATED_SNG_POKERTH_V1`, read out of
PokerTH's own `RANKING_GAME_*` constants, and every client derives the same
parameters from the name alone. That is what lets two people who have never
spoken agree on the game before either sits down. Any other number of seats is
the same game with its numbers spelled out in the advert, one slider away. The
cash game is deactivated in this build: every table this client founds, and
every table it will sit down at, is a Sit-and-Go.

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
| `--search hu` \| `6` \| `10` \| `auto` | find a game automatically, from the start |
| `--search-tables N` | how many games at once the search aims at, 1 to 4 |
| `--search-again` | search again when the game the search found ends |
| `--table` | open on the table rather than the lobby |
| `--for N` | stop after `N` seconds |
| `--renderer gl` \| `software` | pin the renderer instead of letting it choose |
| `--install` | offer to install on this computer, from wherever this copy is |
| `--portable` | never offer: run from the folder it is in |
| `--no-mdns` | do not look for players by multicast — how the DHT path gets tested |

### On a machine with no graphics driver

The window draws with OpenGL, and a virtual machine without graphics
acceleration has only the OpenGL 1.1 that Windows ships, where the window needs
2.0. The client does not stop there. It starts itself again on Direct3D 12,
which with no driver present resolves to **WARP** — `Microsoft Basic Render
Driver`, a software rasteriser that is part of Windows rather than of any
driver — and prints which adapter it ended up on:

```text
the window could not open: egui_glow: OpenGL: egui_glow requires opengl 2.0+.

No OpenGL 2.0 on this machine. Starting again in software.
drawing  Microsoft Basic Render Driver (Cpu)
window   open (software)
```

Nothing to install, and nothing to type: `--renderer` exists to override the
choice, not to make it. It is a second process because a process gets one event
loop and no more, so a renderer cannot be retried in place.

`window   open (…)` is printed from inside a window that exists, so it is the
line a script waits for. A run that never got that far says why and **exits
non-zero** — 1 when the window could not open, 2 for an argument it does not
understand. Every failure used to exit 0, including a profile that could not be
created, which meant a client that never started was indistinguishable from one
that ran and found nothing.

## It is portable, and that is checked

One executable, around 45 MB, statically linked against the C runtime. Five of
those megabytes are the second renderer, which is what makes it start on a
machine with no graphics driver at all. Copy it
into an empty folder and it runs, creating exactly `profile/identity.key` and
`profile/player.key` beside itself — two keys, because the protocol keeps the
network identity and the player identity apart. Two folders are two players.

`tools/check-portable.ps1` verifies all of that by measurement, including that
the binary imports no C runtime and that the two identities really are two
different secrets.

### And on a first run it offers itself a home

Double-click a fresh download — no arguments, no profile beside it — and the
client asks one question before it does anything else: **Install**, **Run
without installing**, or **Quit**. Install copies the program to your user
folder (`%LOCALAPPDATA%\Programs\P2Poker`), puts a shortcut on the desktop and
in the Start menu, tells you where both are, and hands over to the installed
copy as it closes — so there are never two clients running.

It is still the same portable folder, program and profile side by side, and it
needs no administrator rights. **Nothing goes into the registry, nothing starts
by itself, and nothing is deleted**: the copy is hashed as it is written and
hashed again from the disk before it takes the program's name; a shortcut of
your own is never overwritten; a newer installed version is never replaced by
an older download; and a program that is running is not replaced under a game.
To remove it, delete that folder and the shortcuts — your player profile is in
that folder, so make a backup first (Settings, Profile).

Anything started with arguments — a script, a relay, a test bed — is never
asked, and neither is a folder that already has a player in it. The About tab
says where this copy lives, and offers to install a portable one with its
profile moved along, so you stay the same player. `D-073` has the reasoning.

### Download

The newest release is always at
<https://github.com/qavryxdevv/P2Poker/releases/latest>, and the file itself at
<https://github.com/qavryxdevv/P2Poker/releases/latest/download/p2p-poker.exe>.

### And a download can be checked against this source

Releases are not built on anybody's own machine. A tag starts this repository's
workflow on one of GitHub's, which builds the client from exactly that commit
and has GitHub sign a statement of it -- through Sigstore, with a certificate
that lives for minutes and is bound to this workflow in this repository. Nobody
holds a signing key, so there is none to steal. To check a file you downloaded:

```bash
gh attestation verify p2p-poker.exe --repo qavryxdevv/P2Poker
```

Its SHA-256 is on the release page and beside it as `p2p-poker.exe.sha256`.
That proves *where the file came from* -- this source, that tag -- and not that
the source is good, which is what reading it is for. It is also **not** a
Windows code-signing certificate, so SmartScreen still asks before the first
start. `D-074` has the reasoning.

The client asks GitHub for the list of releases **when its window opens**, and
again when you press Settings, About, *Check for a new version*. It sends
nothing about you and downloads nothing itself. While P2Poker is a beta its
protocol changes from one version to the next, so **a newer release is
required**: an older client says which version is out, hands its download to
your browser, and does not sit down at a table until the new one is started. A
client that cannot reach GitHub plays (`D-075`, `D-077`).

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

## Tools

| | |
|---|---|
| `tools/deploy.ps1` | test, build, copy to where it is played from — never touching the profile |
| `tools/clean.ps1 -Deep` | sweep the build directory of what cargo never deletes. It reached 5 GB; 2.7 of that was dead |
| `tools/check-portable.ps1` | prove the binary is still one portable file with two separate identities |
| `tools/two-network-test.ps1` | one node here, one in a VM on another subnet |

## Documentation

For players, [`FAQ.md`](FAQ.md) answers the questions people ask first. The design is written down before it is
built, and the documents are normative:

- [`docs/PROTOCOL.md`](docs/PROTOCOL.md) — the wire protocol, message by message
- [`docs/STATE_MACHINE.md`](docs/STATE_MACHINE.md) — the hand, as a state machine
- [`docs/CRYPTOGRAPHY.md`](docs/CRYPTOGRAPHY.md) — the constructions and why they are sound
- [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) — what a fully modified client can and cannot do
- [`docs/DECISIONS.md`](docs/DECISIONS.md) — every decision, including the ones that were withdrawn
- [`NEXT.md`](NEXT.md) — where the work is now

## Supporting it

There is no house here to take a rake, no ads and nothing to buy. If the client gave you a good evening at
the tables, [`DONATE.md`](DONATE.md) is how it keeps going: two addresses, each with its QR code, and the
lobby's *Support the project* button opens the same page. A donation is a gift -- it buys no chips, no
rank and no advantage at any table.

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

**GPL-3.0-or-later.** The full text is in [`LICENSE`](LICENSE).

It is not a preference. A table's game traffic rides a Tox group (D-019),
`c-toxcore` declares `GPL-3.0-or-later` in every source file, and linking it
makes this whole client that. `DECISIONS.md`'s D-019 records the trade under
"The price" and calls it a one-way door: MIT, Apache-2.0 and the dual form are
foreclosed, and the open item *"the project licence has never been chosen"* is
closed by a transport decision rather than by a licensing one.

The table window's look, its sounds, icons and font come from PokerTH, which is
AGPL-3.0-or-later; GPLv3 section 13 permits the combination, and AGPLv3 section
13's requirement about users interacting over a network applies to it as such.
[`assets/pokerth/PROVENANCE.md`](assets/pokerth/PROVENANCE.md) names every file,
its source and its terms.

The released binary always carries Tox, so the licence is unconditional.
`--no-default-features` builds without it and is a development convenience — a
contributor with no C toolchain, or a test run with no business opening a socket
— never a release.

    This program is free software: you can redistribute it and/or modify it
    under the terms of the GNU General Public License as published by the Free
    Software Foundation, either version 3 of the License, or (at your option)
    any later version.

    This program is distributed in the hope that it will be useful, but WITHOUT
    ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
    FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for
    more details.

    You should have received a copy of the GNU General Public License along
    with this program. If not, see <https://www.gnu.org/licenses/>.

### Building it

The default build needs the vendored C fetched once, at pinned commits:

    pwsh tools/build-tox.ps1
    cargo build --release
