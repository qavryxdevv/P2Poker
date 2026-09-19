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
| `table-run-split.ps1` | Play one table with its seats split across two machines, so *ten seats* and *ten instances on one box* stop being the same experiment. Carries every fault knob: `-DropConfirm`, `-Stall`, `-MuteAt`/`-MuteFor`, `-LinkDown*`, `-DelayCerts*`, `-Deaf*`, `-Think`. **`-Stall` freezes a joiner's event loop and models a frozen process, not a slow network** -- with it firing once, the client's own recovery works (`runs/split182313-9`), so no `-Stall` run is evidence about a healthy-but-unjoined seat. `-DropConfirm` is the knob for that: the founder withholds its first invite confirmations while every loop keeps running. **`-LinkDown*` discards what the transport already delivered and so models a peer whose transport forgets, not a cut line** (`S1-CM`); a brief real outage on the downlink is `-Deaf*` under 58 s, where the lossless ring replays every missed packet when the window ends, and over 58 s it is patch 0015's re-handshake case. **PowerShell 7 only, and refusing is the point.** |
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
| `read-hour-key.py` | One run through `D-070`'s lines, seat by seat: when the hour's record went out and when a node returned it, the sizes of the answers about an hour's key beside those about the lobby's own, and how many of the poker clients a seat met had been named to it by an hour's key. A log with no `D-070` line is said as a client built before it, not counted as a seat the key failed. | `S1-AI`, `D-070` |
| `fold-ring-leftovers.py` | Did the carrier's receive ring replay a leftover, and did a hand stall on it? Per run: `REPLAYED` (upstream's own *Wrap-around on message N*, N above zero -- before patch 0039 each is a message a lossless stream lost to one receiver), `OVERTAKEN` and `CLEARED` (patch 0039's two lines: how often the race is run, and whether the drain's check had to fire), and the hands that stalled with *my clock has run out* grouped by hand and by the seat named -- seven seats saying it of one seat is one incident, not seven. A build before 0039 is said as an absent instrument, not as a zero. | `S1-II` |
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
| `check-portable.ps1` | Is the built client actually portable? By measurement, not by reading the build configuration. |
| `deploy.ps1` | Build and put the client where it is played from, without copying the profile. |
| `clean.ps1` | Sweep `target/` of what cargo never deletes. |
| `msvc-shim/` | Toolchain shim; not run by hand. |
