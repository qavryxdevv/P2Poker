# The tools, and what question each one answers

This index exists because of `S1-BR`. That row's stopping rule is a fold over
the instrument's own log line, and the row said in as many words that it
*"should be re-run against any future stall rather than re-derived"* — while
naming no file, because there was none. It was re-derived four times, and each
time had to rediscover the same correction. A tool that cannot be found is a
tool that gets rewritten, so every fold now lives here and is named from the
row it serves.

Nothing below is a build step. `cargo build` and `cargo test` are the build;
`build-tox.ps1` is the one exception and it says so.

## Taking a measurement

| tool | the question |
| --- | --- |
| `table-run.ps1` | Play N seats on **this** machine and say what a hand costs. |
| `table-run-split.ps1` | Play one table with its seats split across two machines, so *ten seats* and *ten instances on one box* stop being the same experiment. Carries every fault knob: `-DropConfirm`, `-Stall`, `-MuteAt`/`-MuteFor`, `-LinkDown*`, `-DelayCerts*`, `-Deaf*`, `-Think`, `-FarDelay`, `-DeadAnnouncedRelay`, `-NoPatch0040`, `-OldReannounce`, `-OfferEveryAddress`, `-OneWayIn`, `-CapForAll`. **The last three together are the client as it was before `S1-IT`**, from the same binary: every address the DHT offers is dialled, one relay reservation is kept, and no dial is let through a seat's own connection limit. **`-DeadAnnouncedRelay` is `S1-AA`'s join-phase failure on demand** -- the one relay a relay-only member is announced by becomes one no seat can connect to, at both ends -- and `-NoPatch0040` switches patch 0040 off in the same binary, so a control and its treatment differ in nothing else; `-FarDelay` starts the far seats late, which by itself breaks nothing (`runs/split213510-9`). **`-Stall` freezes a joiner's event loop and models a frozen process, not a slow network** -- with it firing once, the client's own recovery works (`runs/split182313-9`), so no `-Stall` run is evidence about a healthy-but-unjoined seat. `-DropConfirm` is the knob for that: the founder withholds its first invite confirmations while every loop keeps running. **`-LinkDown*` discards what the transport already delivered and so models a peer whose transport forgets, not a cut line** (`S1-CM`); a brief real outage on the downlink is `-Deaf*` under 58 s, where the lossless ring replays every missed packet when the window ends, and over 58 s it is patch 0015's re-handshake case. **PowerShell 7 only, and refusing is the point.** |
| `two-network-tox.ps1` | Does a Tox group carry traffic between two machines on two networks? The measurement `D-019` rests on. |
| `two-network-ssh.ps1` | One node here, one over SSH, and what actually crossed between them. |
| `two-network-test.ps1` | The same across a Hyper-V VM on a different subnet. |

**`runs/<name>/run.txt` is the archive's own record**, written before the seats
start and appended to when they finish: the knobs the run was given and the
per-node outcome it reached. It describes the **binary**, probed, and not the
build the command line asked for — see `S1-BB` for why those are different and
what it cost.

## Reading a corpus of runs

Each fold takes run names or, with no arguments, every run under `runs/`.

| tool | the question | the row |
| --- | --- | --- |
| `fold-vote-state.py` | Was a timeout vote ever **owed, eligible, past twice the stage budget, and not cast**? Every per-seat entry falls in one of five buckets and only `UNMET` is the fault. Prints each bucket's longest wait *against the budget that stage actually carried*. | `S1-BR` |
| `fold-forks.py` | How many hands forked, and did **two** branches ever pass stage 0? Carries `--count-aborts-as-advanced` so the trap it dodges stays demonstrable. | `S1-CE`, `S1-CG` |
| `fold-relay-kills.py` | Which relays were killed with a group's slots still on them, by which of the two kill paths, and **what those slots were** — `ONLINE`, `REGISTERED`, or neither. | `S1-AA` |
| `fold-peer-deaths.py` | Which group peer entries were **deleted**, by which exit type, how far each had got, and how many **distinct peers** were lost. Separates the orderly shutdown from the rest first, so it cannot pad a fault count. | `S1-AA` |
| `fold-late-settlements.py` | How often a hand ends by abort and is then **settled late**, and how: on this client's own body, on a peer's, over a dissent, or refused because the peers disagreed and this client had nothing of its own to prefer -- and how often a settlement arrived after the 800 ms the late path is open for, which is a fork this seat is on (`S1-CN`). Close counts exist only from 2026-09-08 on and the tool says so. | `S1-CL`, `S1-BP`, `S1-CN` |
| `read-deaf-run.py` | One `-Deaf` run read against the prediction booked for it: the window computed from the node's join anchor, the node's own timeline around it, the **first hand-level line after the window** (the repair latency, which is what decides the run against the table's 30 s clock), every certificate and abort, and each sender's answers to the deaf node from the window's end on -- ids one apart at offsets a second or more apart is the one-per-second drain `S1-CO` found, a run of consecutive ids at one offset is the burst patch 0024 sends for. Prints the node AND its seat, because the harness numbers nodes and the table numbers seats. | `S1-CM`, `S1-CO` |
| `read-join-phase.py` | One run's **join phase** through the handshake instruments: per node the 12-second reaps (peer hash, confirmed/handshaked, attempts, seconds since the last packet), the handshake attempts by the door they left by (patch 0028), the silent drops with their reason, the completed handshakes and the first full-table heartbeat -- then every reaped hash cross-referenced with what every other node logged about it, so the place an exchange died is on one screen. A run before 0028 shows reaps only and says so. | `S1-AA` |
| `read-hour-key.py` | One run through `D-070`'s lines, seat by seat: when the hour's record went out and when a node returned it, the sizes of the answers about an hour's key beside those about the lobby's own, and how many of the poker clients a seat met had been named to it by an hour's key. A log with no `D-070` line is said as a client built before it, not counted as a seat the key failed. Only a binary built with `fault-harness` says every ANSWER of a lookup (`S1-IV`); a player's build says one line a lookup, and a log that holds only those is read as that -- its lookups counted, the answers' sizes said to be absent -- never as a key that returned nothing. | `S1-AI`, `D-070`, `S1-IV` |
| `fold-ring-leftovers.py` | Did the carrier's receive ring replay a leftover, and did a hand stall on it? Per run: `REPLAYED` (upstream's own *Wrap-around on message N*, N above zero -- before patch 0039 each is a message a lossless stream lost to one receiver), `OVERTAKEN` and `CLEARED` (patch 0039's two lines: how often the race is run, and whether the drain's check had to fire), and the hands that stalled with *my clock has run out* grouped by hand and by the seat named -- seven seats saying it of one seat is one incident, not seven. A build before 0039 is said as an absent instrument, not as a zero. | `S1-II` |
| `read-far-seat-join.py` | How the seats here and the relay-only far seat found each other: per seat when it was TOLD of the far seat, when the search was WIDENED to its own relays (patch 0040), when the two made CONTACT -- read from either side's line, since the answering side never prints *handshaked with* -- how many attempts nothing carried and whether it reaped the entry; then the run against the prediction patch 0040 booked before its first run. | `S1-AA` |
| `donation-page.py` | Are the donation addresses the client's button leads to valid, and do their QR codes say them? `--check` verifies both checksums (bech32 or bech32m for a mainnet native SegWit address, Base58Check for a TRON one), has OpenCV read both QR files back -- whole, and with six modules flipped -- and says whether every file is what `--write` makes of the addresses. `--set` is the one way an address changes: it asks, refuses a bad address in words, shows *was* and *will be*, wants the word typed, commits those paths only, pushes, and asks GitHub whether it holds the commit; anything failing puts everything back. `--selftest` runs the tool's own QR encoder against OpenCV on a text of every length it takes, and gives both checkers an address for every refusal they know, each of which must get its own answer and have its line in the owner's language. Needs OpenCV and NumPy; nothing is written without them. | `D-071` |
| `read-reachability.py` | Did a client's verdict about its own reachability turn over while its way in stood confirmed? Per seat log: the shape (`P` public, `N` behind NAT), the seconds between turns, how many came within thirty seconds of the one before -- nothing about a home connection changes that fast -- and, where the client says how many addresses it holds, how many turns to *behind NAT* were taken while it held one. That last count is the defect itself and must be zero; a log from a build without the count is reported as `NOT TOLD APART`, never as a zero. The control is the same binary under `tools/table-run.ps1 -LastAddressVerdict`. | `S1-IL` |
| `read-first-meetings.py` | Did every pair of seats of a run **meet** over libp2p, how soon, and what did the dials that failed end in? The matrix of first meetings (a pair is met when either seat says so), the founder's column, when each seat sat down, the reservations each seat was given and the ways in that went away, the failed dials of peers that matter and the relays' own by kind of ending -- counted as incidents, a dial once under each kind it shows -- asks whose dial could not start, and what the clients' own connection limit refused. `--pair a b` adds what those two said of each other, relay by relay. The run's `dials` and `waysin` header lines say whether the client as built or the control ran. `ALL MET` / `NOT ALL MET`, exit 0 / 1. A log from before the row holds none of these lines: *none said*, which is an absent instrument and no zero. | `S1-IT` |
| `triage-code-scanning.py` | Which of GitHub's open code-scanning alerts are one of the two kinds that are never a fault here -- a fixed key or password inside **test code** (a file under `tests/`, or a line inside a `#[cfg(test)]` module, read from the file as it was at the alert's commit) and **vendored C that `build.rs` never builds** -- and which are left for a person? Prints the plan and changes nothing; `--apply` dismisses the recognised ones, each with its reason; `--review` runs the same rules over the alerts already dismissed and counts where they disagree with the verdict a person gave (0 of 89 when written). Needs the GitHub CLI signed in. Never by line number, and never an alert in code this client ships. | `S1-IU` |
| `classify-run.ps1` | Read a kept run directory and say what shape of failure, if any, it holds. | — |

**A fold that prints nothing is not a fold that measured zero.** Each of these
says which it is: a run taken before the patch that added its instrument has no
instrument, and the tool prints that rather than a zero. The distinction is not
pedantry — reading an absent instrument as an absence of the thing is a trap
this register has fallen into three times (`S1-AA`'s `no relay carried`, its
`0 online`, and `S1-CG`'s fabricated forks).

## Building and shipping

| tool | the question |
| --- | --- |
| `build-tox.ps1` | Build what `--features tox` needs from the vendored source, and **verify every patch marker is still in the tree** — one or more for every file under `patches/`, checked case-sensitively (the count is the script's own, and a number written here went stale at thirty). A tree that was reverted, half-merged or restored from an unpatched copy would otherwise build and run and be wrong. |
| `build-sodium.sh` | `D-078`: libsodium for a Linux build — static, from the vendored source, set up under `target/` with the system's autotools and nothing fetched from the network. The Linux counterpart of `build-tox.ps1`'s MSVC build; `build.rs` links what it makes. |
| `package-linux.sh` | `D-078`: the Linux packages of a built client — an AppImage, a `.deb`, an `.rpm` and a `.tar.gz` — into `dist/`. The AppImage's tool and runtime are fetched at pinned versions and refused unless their SHA-256 is the one in the script. |
| `check-build-paths.ps1` | `S1-EN`: does a built program name the machine it was built on — the profile, the user, the computer, the cargo home, the repository? Read from the file itself; on Windows and, since `D-078`, on Linux. |
| `check-portable.ps1` | Is the built client actually portable? By measurement, not by reading the build configuration. |
| `deploy.ps1` | Build and put the client where it is played from, without copying the profile. |
| `clean.ps1` | Sweep `target/` of what cargo never deletes. |
| `msvc-shim/` | Toolchain shim; not run by hand. |
