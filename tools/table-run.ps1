<#
.SYNOPSIS
    Play a table of N seats on this machine and report what a hand costs.

.DESCRIPTION
    The seat-count curve in NEXT.md stops at six because six clients were
    started by hand. `MAX_SEATS` is ten, so four of the ten legal sizes had
    never been played at all. This starts them.

    One founder and N-1 joiners, every one of them `--headless --autoplay`, each
    in its own profile directory because two copies of one profile are one
    identity and §4.3 correctly refuses the second a seat. Output is timestamped
    as it arrives, so the log says when a hand opened rather than only that it
    did.

    What it reports is the number NEXT.md's table reports: seconds per hand in
    steady state, taken from the founder's log, with the first hand excluded.
    **The first hand of a table is always the slowest** — the founder opens it
    the moment the roster ratifies, before the joiners are in the group — and
    averaging it in measures formation, not play.

.PARAMETER Seats
    2 to 10. The table is a Sit-and-Go of exactly this many seats and does not
    start until they are all there.

.PARAMETER Seconds
    Wall clock per node. Needs to be long enough for the table to form and play
    several hands: at ten seats formation alone can take a minute.

.PARAMETER Exe
    Which client to run. Tox or libp2p is a **build** choice, not a runtime one:
    `cargo build --release` gives the Tox path (D-019, the default feature) and
    `cargo build --release --no-default-features` gives libp2p. Both are worth a
    curve and they are not the same shape, so point this at the one you built.

.EXAMPLE
    tools\table-run.ps1 -Seats 8 -Seconds 420

.NOTES
    A run leaves its profiles and logs behind on failure and deletes them on
    success, because a run that stalled is the one whose logs are wanted.
#>
[CmdletBinding()]
param(
    [ValidateRange(2, 10)][int]$Seats = 6,
    # **The lobby measurement (D-040).** `-HostSeats <n>` is the founder's own
    # `--seats` when it should differ from the number of nodes; `-NoJoin` makes
    # every node but the founder a WATCHER -- it joins nothing and only watches
    # the lobby, so the table stays open (*waiting for players*) for the whole
    # run; `-Watchers <k>` adds k more such watchers, numbered after the seats;
    # `-LeaveTableAt <s>` has the founder leave its table at that second, as the
    # window's button would (`P2P_POKER_LEAVE_TABLE_AT`, needs `--features
    # fault-harness`). A watcher's log says when the table reached it (*table X
    # (name)*, and *heard by asking* when the question carried it rather than the
    # mesh) and when it went (*withdrawn: its founder no longer offers it*, or
    # *expired*). `tools/fold-lobby.py` reads those lines.
    [ValidateRange(0, 10)][int]$HostSeats = 0,
    [switch]$NoJoin,
    [ValidateRange(0, 8)][int]$Watchers = 0,
    [ValidateRange(0, 3600)][int]$LeaveTableAt = 0,
    # `-LeaveTableNode <i>`: which node leaves at `-LeaveTableAt` (n0 unless said).
    [ValidateRange(0, 9)][int]$LeaveTableNode = 0,
    # `-KillAt <s>` and `-KillNode <i>`: the node's process is killed outright
    # at that second -- no part message, no clean exit -- which is what a client
    # that died or lost its internet looks like to the others (S1-DT). `-DropAt`
    # is a clean stop by `--for`, which says goodbye to the group.
    [ValidateRange(0, 3600)][int]$KillAt = 0,
    [ValidateRange(0, 9)][int]$KillNode = 1,
    # `-ThinkMs <ms>`: every seat waits that long before it acts (`--autoplay <ms>`);
    # above the table's own thirty seconds the seat's OWN clock acts first, which
    # is how the check/fold's timing is measured from the other seats' side
    # (D-034). A slow table, not comparable with the rest of the corpus.
    [ValidateRange(0, 120000)][int]$ThinkMs = 0,
    # `-RehostAt <s>`: at that second the founder leaves its table and hosts a
    # second one ("<table>-2"), and five seconds later every joiner leaves and
    # looks for it -- a second table in the same processes (S1-DL: after a
    # finished tournament the players could not sit at a new table without a
    # restart). The second table's logs follow the first's in the same files.
    [ValidateRange(0, 3600)][int]$RehostAt = 0,
    # `-StartStack <chips>`: the founder's Sit & Go starts every seat with that
    # many chips instead of the preset's 10 000 (`P2P_POKER_START_STACK`, a
    # fault-harness knob), so a tournament ENDS inside a run -- at 300 chips
    # against blinds of 50/100 in a few hands. For D-042's end of tournament:
    # the group left ten seconds after the last hand, the friends after.
    [ValidateRange(0, 1000000)][int]$StartStack = 0,
    # `-TwoTables`: a second table "<table>-B" (three seats) hosted by node
    # $Seats, joined by node $Seats+1, and joined AS WELL by n1 at `-AlsoAt`
    # seconds without leaving the first -- one client at two tables (D-043).
    # n1's log then carries both tables' hands; read it with fold-tables.py.
    [switch]$TwoTables,
    [ValidateRange(0, 3600)][int]$AlsoAt = 60,
    [ValidateRange(30, 3600)][int]$Seconds = 300,
    [string]$Exe,
    [switch]$KeepLogs,
    # **How long the joiners take to arrive, in seconds, spread evenly.**
    #
    # Zero starts them within a second of each other, which is the easy case and
    # not the real one: players arrive irregularly and a tournament table has to
    # stay open until it fills. Everything measured before this parameter
    # existed measured the easy case, and a table that forms when six clients
    # start at once has not been shown to form when the sixth arrives ten
    # minutes after the first.
    #
    # `-Seconds` must cover the stagger and still leave time to play; a
    # combination that cannot is refused rather than run.
    [ValidateRange(0, 3600)][int]$StaggerSeconds = 0,
    # **One seat sits down, waits this long, and leaves before the table is
    # full.** A replacement then joins for the seat it freed.
    #
    # A player who takes a seat at a tournament and then leaves before it starts
    # is ordinary, and no run before this one covered it: they all measure a
    # roster that only ever grows. This exercises one that shrinks and grows
    # again — the serial moving twice, every prior ratification cleared twice,
    # and the freed seat taken by somebody new.
    #
    # Zero is off.
    #
    # **The leaver is the FIRST joiner, not the last, and that is the whole
    # point.** The case is a seat given up *before the table fills*, and a
    # leaver that arrives last has by definition filled it: the first attempt at
    # this put the leaver at the end, the table filled in ten seconds, dealt at
    # thirty, and the departure at ninety was an ordinary mid-tournament one
    # that D-022 already handles. Use `-StaggerSeconds` with this so the table
    # is still filling when the seat is given back.
    #
    # It stops on its own `--for`, so `Drop` runs and the client exits the way a
    # player closing the window does rather than the way a crash does.
    [ValidateRange(0, 3600)][int]$LeaverSeconds = 0,
    # **A seat that drops mid-tournament and comes back**, which is the case
    # D-022 exists for and which nothing had ever exercised.
    #
    # `-DropAt` is when n1's client stops; `-DropFor` is how long it stays down
    # before starting again **with the same profile** — the same application
    # key, the same peer identity, the same seat. That is a client that crashed
    # or a link that went, not a player who left.
    #
    # D-022 gives every seat `GRACE_HANDS = 2`: back inside two hands and it is
    # dealt in again; longer and the allowance is spent and the blinds eat the
    # stack while it sits out. At roughly ten seconds a hand, a twenty-second
    # outage is the boundary.
    # `-DivergeAt <hand>` makes n1 compute a WRONG boundary-checkpoint value for
    # that hand, so that section 6.3's answer to a divergence can be measured
    # rather than reasoned about. It needs a binary built with
    # `--features fault-harness`; without it the environment variable is
    # not read and the run is an ordinary one.
    [ValidateRange(0, 100000)][int]$DivergeAt = 0,
    # Which node tells the lie. Its own log is where the OTHER nodes' complaints
    # are not: a peer that diverges does not know it, which is the whole reason
    # everybody else compares.
    [ValidateRange(0, 32)][int]$DivergeNode = 1,
    # `-LinkDownAt <s> -LinkDownFor <s>` takes one node's LINE away without
    # killing it. The process, its Hand, its chain position and its keys all
    # survive; only the table messages stop, in both directions.
    #
    # This is a different test from `-DropAt`, which kills the process, and the
    # difference is the whole question: after a brief outage a client needs only
    # the messages it missed, while after a restart it has lost the state as
    # well. A test where both fail at once cannot say which one you fixed.
    #
    # Needs a binary built with `--features fault-harness`.
    [ValidateRange(0, 100000)][int]$LinkDownAt = 0,
    [ValidateRange(0, 3600)][int]$LinkDownFor = 15,
    [ValidateRange(0, 32)][int]$LinkDownNode = 1,
    # `-StallJoin <seconds> -StallJoinNode <n>` starves one joiner's **group
    # handshake** for that long, right after it accepts the invitation.
    #
    # This is `S1-AA` shape (i) on demand. toxcore gives the handshake four
    # attempts at three seconds inside `GC_UNCONFIRMED_PEER_TIMEOUT` = 12 s;
    # past that the inviter's address-less peer entry is reaped, no `peer_exit`
    # fires, nothing is logged, and there is no path back — a fresh invitation
    # is refused inside `Messenger.c` because a chat with that id already
    # exists. Seven runs of 134 hit it and nothing could make it happen.
    #
    # **Different from `-LinkDownAt`, and that is the point.** A link outage
    # stops the *table's* messages at the application layer while toxcore keeps
    # handshaking underneath; this stops the handshake itself and leaves the
    # driver believing it holds a group, which is exactly what the seven
    # measured victims believed.
    #
    # Use a value above 12 (25 or 30 is comfortable) and expect the seat to
    # leave and rejoin: `JOIN_GRACE` = 25 s, `MAX_REJOINS` = 3, and the status
    # line counts both.
    #
    # Needs a binary built with `--features fault-harness`.
    [ValidateRange(0, 300)][int]$StallJoin = 0,
    [ValidateRange(0, 32)][int]$StallJoinNode = 1,
    # `-MuteAt <s> -MuteFor <s> -MuteNode <n>` makes one node drop every HAND
    # message it would SEND for that long while it hears everything -- the
    # split harness's `-MuteSeat`, on one machine. `-MuteOnTurn` measures the
    # window from the node's first ACTION at or after `-MuteAt` instead of
    # from the wall clock, so the seat goes quiet at a decision and the table
    # certifies it out (`-MuteFor` above the 30 s decision deadline). The node
    # keeps following every hand, which is what `S1-BM`'s return needs: a seat
    # outside the roster that holds the settled terminal and its checkpoint
    # asks to sit in at that boundary and is dealt into the hand after.
    #
    # Needs a binary built with `--features fault-harness`.
    [ValidateRange(0, 3600)][int]$MuteAt = 0,
    [ValidateRange(0, 3600)][int]$MuteFor = 0,
    [ValidateRange(0, 32)][int]$MuteNode = 1,
    [switch]$MuteOnTurn,
    [ValidateRange(0, 3600)][int]$DropAt = 0,
    # `-DropOnTurn`: the dropper stops at its first own turn at or after `-DropAt`
    # rather than at the second itself, so the table plays past the death by a
    # fold-effect certificate; a death mid-shuffle stalls the table to the hand
    # deadline (D-015), which is longer than a run. The return waits for the stop.
    # Needs a binary built with `--features fault-harness`.
    [switch]$DropOnTurn,
    # `-DropAtOpen`: the dropper stops at its first hand open at or after `-DropAt`:
    # a seat gone while the next hand waits at stage 0, which is the shape the
    # two-seat rule (D-031) is about. Needs a binary built with `--features fault-harness`.
    [switch]$DropAtOpen,
    # `-DropAtHand <k>`: the droppers stop at the open of hand #k, whichever
    # second that is, so that several of them stop at the SAME hand. `-DropAtOpen`
    # counts seconds from each process's own start, and a hand that opens near
    # that mark straddles their clocks: in `run180731-4` one dropper stopped at
    # hand #1 and the other at hand #2, and the table certified them one at a
    # time instead of facing both gone at once. `-DropAt` is not needed with it.
    # Needs a binary built with `--features fault-harness`.
    [ValidateRange(0, 100000)][int]$DropAtHand = 0,
    [ValidateRange(0, 600)][int]$DropFor = 20,
    # `-DropNodes`: which joiners drop (node numbers, comma-separated). One by
    # default; several stop at the same moment -- D-036's shape, where the
    # table certifies them together -- and each comes back on its own return.
    # A return that would start after the run is over is not started: the seat
    # stays away.
    [string]$DropNodes = '1',
    # A seat slower than this to enter the Tox group keeps the logs, however
    # well the rest of the run went. Sixty seconds is far outside the ordinary
    # spread, which has been 10-25 s in every run measured.
    [int]$LateGroupSeconds = 60
)
# Comma or space: `-DropNodes 1,2` can reach a [string] parameter as "1 2".
$droppers = @("$DropNodes" -split '[,\s]+' | Where-Object { $_ -ne '' } | ForEach-Object { [int]$_ })
# Not `$hostSeats`: PowerShell variable names are case-insensitive and that
# would BE the parameter.
$founderSeats = if ($HostSeats -gt 0) { $HostSeats } else { $Seats }
$nodeCount = $Seats + $Watchers + $(if ($TwoTables) { 2 } else { 0 })

$ErrorActionPreference = 'Stop'

# --- the binary ------------------------------------------------------------
if (-not $Exe) {
    $repo = Split-Path -Parent $PSScriptRoot
    $Exe = Join-Path $repo 'target\release\p2p-poker.exe'
}
if (-not (Test-Path $Exe)) {
    throw "no binary at $Exe. Build one: cargo build --release"
}
$Exe = (Resolve-Path $Exe).Path

# A table name that cannot collide with another run on this machine. The name is
# display data and never an identifier (PROTOCOL.md 4.3); it is how a scripted
# joiner recognises the table it was told to sit at, and nothing more.
# **Where the logs go, and it is not the temp directory (2026-09-06).**
#
# Storage Sense is on for this account with temp-file cleanup enabled, and it
# deleted the whole of the old work root -- 76 runs, 277 MB, including every run
# the register cites by name -- between one read of a log and the next. They came
# back from the recycle bin, but nothing warned, and a register row that says
# *measured in `split092359-10`* is worth exactly as much as the log it points
# at. The repository's own `runs/` is ignored by git and untouched by Windows.
$repoRoot = Split-Path -Parent $PSScriptRoot

$stamp = (Get-Date).ToString('HHmmss')
$table = "run$stamp-$Seats"

$work = Join-Path $repoRoot (Join-Path 'runs' $table)
New-Item -ItemType Directory -Force -Path $work | Out-Null

# **One shared Tox node list for every run on this machine.**
#
# Each node gets a fresh profile, finds no `tox-nodes.json`, and fetches
# `https://nodes.tox.chat/json` on start. Measured on 2026-09-01: 88 run
# directories and 432 of those files in one evening - 432 requests to one public
# endpoint from one address, which is a load nobody should put on a volunteer
# service and is a variable in every measurement taken afterwards. That evening's
# runs did fail, repeatedly and inexplicably, with `tox self tcp, tox friends up
# 0` and no group; the cause was never proven, and this removes the most likely
# candidate from the next attempt.
#
# The client refreshes at most daily and merges the cache with its compiled-in
# list, so seeding a profile from a shared copy is exactly what a second run on
# the same day would have done for itself.
$shared = Join-Path $repoRoot (Join-Path 'runs' 'tox-nodes.json')
$seed = (Test-Path $shared) -and
        ((Get-Date) - (Get-Item $shared).LastWriteTime).TotalHours -lt 24
if ($seed) { Write-Host "nodes  seeded from the shared cache, no fetch needed" }

Write-Host "table  $table"
Write-Host "seats  $Seats"
Write-Host "for    $Seconds s"
if ($StaggerSeconds -gt 0) {
    if ($StaggerSeconds -ge $Seconds - 60) {
        throw "a stagger of $StaggerSeconds s leaves nothing of a $Seconds s run to play in"
    }
    Write-Host ("join   over {0} s, one every {1:F1} s" -f $StaggerSeconds, ($StaggerSeconds / [Math]::Max(1, $Seats - 1)))
}
Write-Host "work   $work"
Write-Host ''

# --- start them ------------------------------------------------------------
# Every node is a job that timestamps each line as it arrives. `Start-Process
# -RedirectStandardOutput` would be shorter and would lose the timing, which is
# the half of the measurement that says WHERE a hand's time goes.
$t0 = Get-Date
$jobs = @()
for ($i = 0; $i -lt $nodeCount; $i++) {
    $profileDir = Join-Path $work "n$i"
    New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
        # **A stable identity per seat, because a fresh one is a permanent
    # ghost.** An empty profile makes `load_or_create_identity`
    # (`src/storage/profile.rs:55`) mint a new 32-byte Ed25519 seed, and that
    # brand-new peer id announces itself on the public lobby key. Kademlia
    # has no unprovide and `stop_providing` is local only, so the record
    # outlives the process by the storing node's TTL -- 48 h. One seat-start
    # was one permanent ghost.
    #
    # Measured before this existed: 1020 identities of ours on the lobby key,
    # of which a run saw 598 and could reach none. See `DECISIONS.md` S1-AI.
    # Reusing one seed per seat index turns that into 4-20 recurring ids and
    # lets the graveyard drain itself inside two days.
    #
    # The file is 32 raw bytes with no header, which is exactly what the
    # client reads; a file it cannot parse is refused rather than
    # overwritten, so a damaged seed fails loudly.
    #
    # Cost: two runs can no longer be told apart by peer id -- use the seat
    # names -- and two concurrent runs on this machine would collide on a
    # seat index. This harness starts one table at a time.
    $seatKeys = Join-Path $env:LOCALAPPDATA 'p2p-poker-test-seats'
    # `$seedKey`, not `$seed`: `$seed` is the shared-cache flag set above, and
    # reusing its name here made `if ($seed)` below always true, so a run on a
    # machine without the cache died copying a file that was not there
    # (2026-09-10, `run163537-3`).
    $seedKey = Join-Path $seatKeys "local-n$i.key"
    if (-not (Test-Path $seedKey)) {
        New-Item -ItemType Directory -Force -Path $seatKeys | Out-Null
        $b = New-Object byte[] 32
        # `::Fill` is .NET Core only; Windows PowerShell 5.1 runs on .NET
        # Framework and has only the instance API. Measured: the run died
        # with *does not contain a method named 'Fill'*.
        $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
        try { $rng.GetBytes($b) } finally { $rng.Dispose() }
        [System.IO.File]::WriteAllBytes($seedKey, $b)
    }
    Copy-Item $seedKey (Join-Path $profileDir 'identity.key') -Force
    if ($seed) { Copy-Item $shared (Join-Path $profileDir 'tox-nodes.json') -Force }
    $log = Join-Path $work "n$i.log"

    # **Every node stops at the same wall-clock moment, not after the same
    # duration.** A joiner started two hundred seconds late and given
    # `--for $Seconds` would outlive the founder by two hundred seconds and
    # spend the last of them as the only seat at the table.
    $mine = $Seconds - [int]([Math]::Round(((Get-Date) - $t0).TotalSeconds))
    if ($mine -lt 30) { $mine = 30 }
    $leaving = ($LeaverSeconds -gt 0 -and $i -eq 1)
    if ($leaving) { $mine = $LeaverSeconds }
    # The dropper stops at `-DropAt` and is started again below. Its own `--for`
    # is cut to the outage's start; the second half gets the rest of the run.
    $dropping = (($DropAt -gt 0 -or $DropAtHand -gt 0) -and $droppers -contains $i -and -not $leaving)
    if ($dropping) { $mine = if ($DropAtHand -gt 0) { $Seconds } elseif ($DropOnTurn -or $DropAtOpen) { $DropAt + 150 } else { $DropAt } }
    $stopOnTurn = if ($dropping -and $DropOnTurn) { $DropAt } else { 0 }
    $stopAtOpen = if ($dropping -and $DropAtOpen) { $DropAt } else { 0 }
    $stopAtHand = if ($dropping -and $DropAtHand -gt 0) { $DropAtHand } else { 0 }
    $nodeArgs = @('--headless', '--autoplay')
    if ($ThinkMs -gt 0) { $nodeArgs += "$ThinkMs" }
    $nodeArgs += @('--for', "$mine", '--profile', $profileDir)
    if ($i -eq 0) {
        $nodeArgs += @('--host', $table, '--seats', "$founderSeats")
        if ($RehostAt -gt 0) { $nodeArgs += @('--then-host', "$table-2", '--then-at', "$RehostAt") }
    } elseif ($TwoTables -and $i -eq $Seats + $Watchers) {
        $nodeArgs += @('--host', "$table-B", '--seats', '3')
    } elseif ($TwoTables -and $i -eq $Seats + $Watchers + 1) {
        $nodeArgs += @('--join', "$table-B")
    } elseif ($i -lt $Seats -and -not $NoJoin) {
        $nodeArgs += @('--join', $table)
        if ($RehostAt -gt 0) { $nodeArgs += @('--then-join', "$table-2", '--then-at', "$($RehostAt + 5)") }
        if ($TwoTables -and $i -eq 1) { $nodeArgs += @('--also-join', "$table-B", '--also-at', "$AlsoAt") }
    }
    # Otherwise a watcher: the lobby only, joining nothing.
    $leaveAt = if ($LeaveTableAt -gt 0 -and $i -eq $LeaveTableNode) { $LeaveTableAt } else { 0 }

    $diverge = if ($DivergeAt -gt 0 -and $i -eq $DivergeNode) { $DivergeAt } else { 0 }
    $downAt = if ($LinkDownAt -gt 0 -and $i -eq $LinkDownNode) { $LinkDownAt } else { 0 }
    $stall = if ($StallJoin -gt 0 -and $i -eq $StallJoinNode) { $StallJoin } else { 0 }
    $mute = if ($MuteFor -gt 0 -and $i -eq $MuteNode) { $MuteFor } else { 0 }
    # Not `$startStack`: PowerShell's names are case-insensitive and that IS
    # the parameter. The founder alone; the advert carries it to the joiners.
    $stackForNode = if ($StartStack -gt 0 -and $i -eq 0) { $StartStack } else { 0 }
    $jobs += Start-Job -Name "n$i" -ArgumentList $Exe, $nodeArgs, $log, $diverge, $downAt, $LinkDownFor, $stall, $mute, $MuteAt, [bool]$MuteOnTurn, $stopOnTurn, $stopAtOpen, $stopAtHand, $leaveAt, $stackForNode -ScriptBlock {
        param($exe, $nodeArgs, $log, $diverge, $downAt, $downFor, $stall, $mute, $muteAt, $muteOnTurn, $stopOnTurn, $stopAtOpen, $stopAtHand, $leaveAt, $stack)
        if ($diverge -gt 0) { $env:P2P_POKER_DIVERGE_AT_HAND = "$diverge" }
        if ($downAt -gt 0) {
            $env:P2P_POKER_LINK_DOWN_AT = "$downAt"
            $env:P2P_POKER_LINK_DOWN_FOR = "$downFor"
        }
        if ($stall -gt 0) { $env:P2P_POKER_STALL_JOIN = "$stall" }
        if ($stopOnTurn -gt 0) { $env:P2P_POKER_STOP_ON_TURN_AFTER = "$stopOnTurn" }
        if ($stopAtOpen -gt 0) { $env:P2P_POKER_STOP_AT_OPEN_AFTER = "$stopAtOpen" }
        if ($stopAtHand -gt 0) { $env:P2P_POKER_STOP_AT_HAND = "$stopAtHand" }
        if ($leaveAt -gt 0) { $env:P2P_POKER_LEAVE_TABLE_AT = "$leaveAt" }
        if ($stack -gt 0) { $env:P2P_POKER_START_STACK = "$stack" }
        if ($mute -gt 0) {
            $env:P2P_POKER_MUTE_AT = "$muteAt"
            $env:P2P_POKER_MUTE_FOR = "$mute"
            if ($muteOnTurn) { $env:P2P_POKER_MUTE_ON_TURN = '1' }
        }
        $start = Get-Date
        $inv = [System.Globalization.CultureInfo]::InvariantCulture
        & $exe @nodeArgs 2>&1 | ForEach-Object {
            $t = ((Get-Date) - $start).TotalSeconds
            # Invariant, not the machine's locale. A Czech Windows writes "5,4"
            # and the first version of this script then read zero hands out of a
            # log full of them.
            ($t.ToString('F1', $inv)).PadLeft(7) + '  ' + $_
        } | Out-File -FilePath $log -Encoding utf8
    }

    # The founder needs to be advertising before a joiner looks for its name.
    # Joiners are staggered so ten of them do not all dial in one instant, which
    # is a thundering herd this measurement is not about.
    # The founder first and then a gap, so it is advertising before anybody
    # looks. After that, either a token stagger or the real one.
    if ($i -eq 0) {
        Start-Sleep -Seconds 3
    } elseif ($StaggerSeconds -gt 0) {
        Start-Sleep -Seconds ([Math]::Round($StaggerSeconds / [Math]::Max(1, $Seats - 1)))
    } else {
        Start-Sleep -Milliseconds 400
    }
}

if ($LeaverSeconds -gt 0) {
    # The replacement, started once the leaver has gone. It is given the rest of
    # the run, and the table is one seat short in between - which is the state
    # the whole exercise is about.
    Write-Host "==> n1 leaves at $LeaverSeconds s; a replacement follows"
    $replacement = Start-Job -ArgumentList $Exe, $work, $table, $Seats, $Seconds, $LeaverSeconds -ScriptBlock {
        param($exe, $work, $table, $seats, $seconds, $leaverSeconds)
        Start-Sleep -Seconds ($leaverSeconds + 5)
        $p = Join-Path $work 'nR'
        New-Item -ItemType Directory -Force -Path $p | Out-Null
        $log = Join-Path $work 'nR.log'
        $start = Get-Date
        $inv = [System.Globalization.CultureInfo]::InvariantCulture
        $left = $seconds - $leaverSeconds - 5
        if ($left -lt 30) { $left = 30 }
        & $exe --headless --autoplay --for "$left" --profile $p --join $table 2>&1 |
            ForEach-Object { ((((Get-Date) - $start).TotalSeconds).ToString('F1', $inv)).PadLeft(7) + '  ' + $_ } |
            Out-File -FilePath $log -Encoding utf8
    }
    $jobs += $replacement
}

if ($DivergeAt -gt 0) {
    Write-Host "==> n$DivergeNode will hold a wrong end-of-hand state for hand $DivergeAt"
    Write-Host "    (needs a binary built with --features fault-harness)"
}
if ($StallJoin -gt 0) {
    Write-Host "==> n$StallJoinNode starves its group handshake for $StallJoin s after accepting the invite"
    Write-Host "    (S1-AA shape (i): past 12 s the inviter entry is reaped and there is no way back;"
    Write-Host "     expect a leave-and-rejoin, counted on the status line. Needs --features fault-harness)"
}
if ($LinkDownAt -gt 0) {
    Write-Host "==> n$LinkDownNode loses its LINE at $LinkDownAt s for $LinkDownFor s; the process lives on"
    Write-Host "    (needs a binary built with --features fault-harness)"
}
if ($MuteFor -gt 0) {
    Write-Host "==> n$MuteNode sends no hand message for $MuteFor s $(if ($MuteOnTurn) { "from its first action at or after $MuteAt s" } else { "from $MuteAt s" }) and hears everything"
    if ($MuteFor -le 30) { Write-Host "    WARNING: under the 30 s decision deadline, so the table will not vote it out" }
    Write-Host "    (S1-BM: expect the seat certified out, then asking to sit in at the next settled boundary, then dealt in; needs --features fault-harness)"
}
if (($DropAt -gt 0 -or $DropAtHand -gt 0) -and $LeaverSeconds -eq 0) {
    foreach ($dn in $droppers) {
        if ($DropAtHand -gt 0) {
            Write-Host "==> n$dn stops at the open of hand #$DropAtHand and returns $DropFor s after that, same profile"
            $null = Wait-Job -Job $jobs[$dn] -Timeout $Seconds
            $stoppedAt = [int](((Get-Date) - $t0).TotalSeconds)
            Write-Host "==> n$dn stopped at $stoppedAt s; back in $DropFor s"
            $delay = $DropFor
            $spent = $stoppedAt + $DropFor
        } elseif ($DropOnTurn -or $DropAtOpen) {
            Write-Host "==> n$dn stops at its first $(if ($DropAtOpen) { 'hand open' } else { 'turn' }) at or after $DropAt s and returns $DropFor s after that, same profile"
            # The main thread has nothing else to do until the run ends, so it waits
            # for the dropper's process here and starts the return when it is gone.
            $null = Wait-Job -Job $jobs[$dn] -Timeout ($DropAt + 200)
            $stoppedAt = [int](((Get-Date) - $t0).TotalSeconds)
            Write-Host "==> n$dn stopped at $stoppedAt s; back in $DropFor s"
            $delay = $DropFor
            $spent = $stoppedAt + $DropFor
        } else {
            Write-Host "==> n$dn drops at $DropAt s and returns $DropFor s later, same profile"
            $delay = $DropAt + $DropFor
            $spent = $DropAt + $DropFor
        }
        # A return that would start after the run is over is no return: the
        # seat stays away, which is what `-DropFor 600` is for.
        if ($spent + 30 -gt $Seconds) {
            Write-Host "==> n$dn stays away: its return would fall after the run"
            continue
        }
        $return = Start-Job -ArgumentList $Exe, $work, $table, $Seconds, $delay, $spent, $dn -ScriptBlock {
            param($exe, $work, $table, $seconds, $delay, $spent, $dn)
            Start-Sleep -Seconds $delay
            # **The same profile, deliberately.** It carries the identity and the
            # application key, so this is the seat coming back rather than a new
            # player taking one.
            $p = Join-Path $work "n$dn"
            $log = Join-Path $work "n$dn-again.log"
            $start = Get-Date
            $inv = [System.Globalization.CultureInfo]::InvariantCulture
            $left = $seconds - $spent
            if ($left -lt 30) { $left = 30 }
            # `--resume` (S1-CR): the returning process rejoins from its own session record;
            # `--join` stays as the name it would otherwise look for.
            & $exe --headless --autoplay --for "$left" --profile $p --join $table --resume 2>&1 |
                ForEach-Object { ((((Get-Date) - $start).TotalSeconds).ToString('F1', $inv)).PadLeft(7) + '  ' + $_ } |
                Out-File -FilePath $log -Encoding utf8
        }
        $jobs += $return
    }
}

if ($NoJoin -or $Watchers -gt 0) {
    Write-Host "==> $($nodeCount - 1 - $(if ($NoJoin) { 0 } else { $Seats - 1 })) watcher(s) join nothing and only watch the lobby; the founder offers $founderSeats seat(s)"
}
if ($LeaveTableAt -gt 0) { Write-Host "==> n$LeaveTableNode leaves its table at $LeaveTableAt s" }
if ($KillAt -gt 0) {
    Write-Host "==> n$KillNode is killed outright at $KillAt s"
    $killProfile = Join-Path $work "n$KillNode"
    $null = Start-Job -Name 'killer' -ArgumentList $KillAt, $killProfile -ScriptBlock {
        param($at, $profile)
        Start-Sleep -Seconds $at
        Get-CimInstance Win32_Process -Filter "Name = 'p2p-poker.exe'" |
            Where-Object { $_.CommandLine -like "*$profile*" } |
            ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
    }
}
if ($RehostAt -gt 0) { Write-Host "==> at $RehostAt s n0 leaves and hosts $table-2; the joiners follow five seconds later" }
if ($StartStack -gt 0) { Write-Host "==> every seat starts with $StartStack chips, so the tournament ends inside the run" }
if ($TwoTables) { Write-Host "==> a second table $table-B: hosted by n$($Seats + $Watchers), joined by n$($Seats + $Watchers + 1), and by n1 as well at $AlsoAt s" }
if ($ThinkMs -gt 0) { Write-Host "==> every seat waits $ThinkMs ms before it acts; past the table's own clock, the seat's client acts for it" }
Write-Host "$nodeCount nodes started; waiting up to $($Seconds + 60) s"
$null = Wait-Job -Job $jobs -Timeout ($Seconds + 60)

$stillRunning = @($jobs | Where-Object { $_.State -eq 'Running' })
if ($stillRunning.Count -gt 0) {
    Write-Warning "$($stillRunning.Count) node(s) did not stop on their own; killing"
    $stillRunning | Stop-Job
}
$jobs | Remove-Job -Force

# --- read it back ----------------------------------------------------------
#
# **Per node, not only the founder.** The first version of this script read n0's
# log and reported a clean three-seat table. What had actually happened was that
# seat 2 was never invited into the Tox group, opened its own hands on a genesis
# nobody else held, and played nothing for the whole run - and it still printed
# `TABLE FORMED session=<the same one> seats=3`, because that line reports the
# roster a peer holds and a peer that heard no hand still holds it.
#
# So the scripted pass condition every test in this project reads back cannot
# tell a seat that played from one that did not, and this table is what says so.
$inv = [System.Globalization.CultureInfo]::InvariantCulture
$logs = @()
for ($i = 0; $i -lt $nodeCount; $i++) { $logs += @{ Name = "n$i"; Path = (Join-Path $work "n$i.log") } }
if ($LeaverSeconds -gt 0) { $logs += @{ Name = 'nR'; Path = (Join-Path $work 'nR.log') } }
if (($DropAt -gt 0 -or $DropAtHand -gt 0) -and $LeaverSeconds -eq 0) {
    foreach ($dn in $droppers) { $logs += @{ Name = "n$dn-again"; Path = (Join-Path $work "n$dn-again.log") } }
}

$nodes = @()
foreach ($entry in $logs) {
    $log = $entry.Path
    if (-not (Test-Path $log)) {
        $nodes += [pscustomobject]@{ Node = $entry.Name; Group = $null; Opens = @(); Overs = 0; Formed = $false }
        continue
    }
    $lines = Get-Content $log
    $group = $null
    $opens = @()
    $overs = 0
    $formed = $false
    # Which table a hand belongs to: a `-RehostAt` run holds two in one log,
    # each with its own hand #1 on its own genesis, and comparing across them
    # read a clean run as forked (run212040-3).
    $tableNo = 0
    foreach ($l in $lines) {
        if ($l -match '^\s*[0-9.]+\s+left the table') { $tableNo++ }
        if ($l -match "^\s*([0-9.]+)\s+in the table's Tox group") {
            if ($null -eq $group) { $group = [double]::Parse($Matches[1], $inv) }
        }
        if ($l -match '^\s*([0-9.]+)\s+hand #(\d+) opens at genesis (\w+) with seats \[([^\]]*)\]') {
            $opens += [pscustomobject]@{
                At      = [double]::Parse($Matches[1], $inv)
                Hand    = [int]$Matches[2]
                Genesis = $Matches[3]
                Seats   = $Matches[4]
                Table   = $tableNo
            }
        }
        if ($l -match 'hand #\d+ is over') { $overs++ }
        if ($l -match 'TABLE FORMED session=(\w+) seats=(\d+)') { $formed = $true }
    }
    $nodes += [pscustomobject]@{ Node = $entry.Name; Group = $group; Opens = $opens; Overs = $overs; Formed = $formed }
}

$founder = $nodes[0]
$lines = Get-Content (Join-Path $work 'n0.log')
$formed = $lines | Where-Object { $_ -match 'TABLE FORMED session=(\w+) seats=(\d+)' }
$opens = $founder.Opens
$overs = $founder.Overs

Write-Host ''
Write-Host '--- per node ---'
# **`finished` is the participation column; `same genesis` is not.**
# `genesis_hand` is a hash of the table id, the hand number, the session, the
# roster hash, the terminal-zero and the ratifiers - every one of which a peer
# holds the moment the roster ratifies. Two peers therefore agree on hand N's
# genesis without exchanging a single hand message, and a seat that never
# entered the group was observed agreeing on all five hands it opened while
# finishing none of them. Opening a hand is local. Finishing one is not.
Write-Host ('{0,-4} {1,10} {2,7} {3,8}  {4}' -f 'node', 'in group', 'opened', 'finished', 'same genesis (see note)')
foreach ($n in $nodes) {
    $g = if ($null -eq $n.Group) { 'never' } else { $n.Group.ToString('F1', $inv) + ' s' }
    # A node agrees if every hand it opened carries the genesis the founder gave
    # that hand number. A forked seat opens the same NUMBER on its own hash.
    #
    # **Only hands the founder also opened are compared.** The joiners start a
    # few seconds before the founder does - it is staggered so it is advertising
    # before anybody looks - so they outlive it by that much and open one more
    # hand at the end, together and in agreement. Counting those as
    # disagreements made every clean run report two forked seats.
    $agree = 'no hands'
    if ($n.Opens.Count -gt 0) {
        $bad = 0
        $beyond = 0
        foreach ($o in $n.Opens) {
            $ref = $founder.Opens | Where-Object { $_.Hand -eq $o.Hand -and $_.Table -eq $o.Table } | Select-Object -First 1
            if ($null -eq $ref) { $beyond++ } elseif ($ref.Genesis -ne $o.Genesis) { $bad++ }
        }
        $compared = $n.Opens.Count - $beyond
        $agree = if ($bad -eq 0) {
            if ($beyond -gt 0) { "yes ($compared compared, $beyond after the founder stopped)" } else { 'yes' }
        } else {
            "NO - $bad of $compared on a different genesis"
        }
    }
    Write-Host ('{0,-4} {1,10} {2,7} {3,8}  {4}' -f $n.Node, $g, $n.Opens.Count, $n.Overs, $agree)
}

# A seat that started hands and finished none heard nobody, whatever its
# genesis column says. This is the check that catches what the genesis one
# cannot.
$deaf = @($nodes | Where-Object { $_.Opens.Count -gt 0 -and $_.Overs -eq 0 })
# **A build with no Tox has no group, and that is not a fault.** Every node
# reporting `never` means this is the libp2p path (`--no-default-features`),
# which is a curve worth measuring against D-019's. One node missing a group
# while others have one is the fault.
$onTox = @($nodes | Where-Object { $null -ne $_.Group }).Count -gt 0
$never = if ($onTox) { @($nodes | Where-Object { $null -eq $_.Group }) } else { @() }
if (-not $onTox) { Write-Host 'path   libp2p (no node reported a Tox group)' }
$forked = @($nodes | Where-Object { $_.Opens.Count -gt 0 -and $_.Node -ne 'n0' } | Where-Object {
    $n = $_
    ($n.Opens | Where-Object {
        $o = $_
        $ref = $founder.Opens | Where-Object { $_.Hand -eq $o.Hand -and $_.Table -eq $o.Table } | Select-Object -First 1
        ($null -ne $ref) -and ($ref.Genesis -ne $o.Genesis)
    }).Count -gt 0
})

Write-Host ''
Write-Host '--- result ---'
if ($formed) {
    Write-Host ($formed | Select-Object -Last 1)
} else {
    # **Two different failures, and they were the same sentence.** Since
    # `S1-BH` a client that formed a table and then heard nobody from it says
    # `NOT PLAYING` rather than claiming a seat it no longer has, so the
    # absence of `TABLE FORMED` no longer means only that nobody arrived.
    $deaf = $lines | Where-Object { $_ -match 'NOT PLAYING session=(\w+)' }
    if ($deaf) {
        Write-Host ($deaf | Select-Object -Last 1)
        Write-Host 'the table formed and this client could not hear it - not the same as never forming'
    } else {
        Write-Host 'NO TABLE - the seats never all arrived'
    }
}
Write-Host "hands opened   $($opens.Count)  (founder)"
Write-Host "hands finished $overs  (founder)"
if ($never.Count -gt 0) {
    Write-Warning "$($never.Count) seat(s) never entered the Tox group: $($never.Node -join ', ')"
    $KeepLogs = $true
}
if ($forked.Count -gt 0) {
    Write-Warning "$($forked.Count) seat(s) opened a hand on a genesis the founder never had: $($forked.Node -join ', ')"
    $KeepLogs = $true
}
if ($deaf.Count -gt 0) {
    Write-Warning "$($deaf.Count) seat(s) opened hands and finished none - they heard nobody: $($deaf.Node -join ', ')"
    $KeepLogs = $true
}

# **A seat that took a long time to get into the group is worth keeping even
# when the run is otherwise perfect.** A nine-seat run played ten hands with
# zero settlement disagreements and every seat dealt in throughout - and one
# seat had entered the group at 125 s. The run counted as a success, the logs
# were deleted, and the question it could have answered (a friend connection
# that had not come up, or an invitation refused?) went with them.
$slow = @($nodes | Where-Object { $null -ne $_.Group -and $_.Group -gt $LateGroupSeconds })
if (-not $onTox) { $slow = @() }
if ($slow.Count -gt 0) {
    Write-Warning ("{0} seat(s) took longer than {1} s to enter the group: {2}" -f $slow.Count, $LateGroupSeconds, (($slow | ForEach-Object { '{0} at {1} s' -f $_.Node, $_.Group.ToString('F1', $inv) }) -join ', '))
    $KeepLogs = $true
}

# **The shapes this summary cannot see, from the script that can.**
#
# Everything above counts hands. Three failures measured on 2026-09-02 do not
# show up in a hand count at all: a seat that never entered the group opens
# nothing and contributes a quiet zero; a seat whose stage stalled leaves the
# others waiting while its own line says it is present; and a seat one
# ratification short of a full set (`S1-P`) never computes a session, never
# opens a hand, and is invisible from every side. The last of those sat at
# `ratified 1/10` for six and a half minutes of a ten-seat run whose summary
# looked healthy, and the logs were deleted.
#
# So the classifier runs before the cleanup decision and its verdict keeps the
# logs. It is a separate script because it must also be usable on runs already
# on disk, including ones from before it existed.
$classifier = Join-Path $PSScriptRoot 'classify-run.ps1'
if (Test-Path $classifier) {
    $verdict = & powershell -NoProfile -File $classifier -Dir $work 2>&1 | Out-String
    if ($verdict -notmatch 'verdict: clean') {
        Write-Host ''
        Write-Host ($verdict.Trim())
        $KeepLogs = $true
    }
}

# Seconds per hand from consecutive opens, first interval dropped. See the note
# in the header: hand 1 measures formation.
if ($opens.Count -ge 3) {
    $gaps = @()
    for ($i = 2; $i -lt $opens.Count; $i++) {
        $gaps += ($opens[$i].At - $opens[$i - 1].At)
    }
    $mean = ($gaps | Measure-Object -Average).Average
    Write-Host ''
    Write-Host ("seats {0}: {1} s per hand, steady state, over {2} interval(s)" -f $Seats, $mean.ToString('F1', $inv), $gaps.Count)
    Write-Host ("  first hand took {0} s from the founder's start, and is excluded" -f $opens[0].At.ToString('F1', $inv))
} else {
    Write-Host ''
    Write-Host "not enough hands for a steady-state figure (need 3 opens, got $($opens.Count))."
    Write-Host "Logs are in $work"
    $KeepLogs = $true
}

# Keep whatever the founder fetched, so the next run on this machine seeds from
# it instead of asking `nodes.tox.chat` again. Outside the `KeepLogs` branch on
# purpose: a run whose logs are thrown away still fetched a list, and throwing
# that away too is what made 432 requests out of one evening.
$fetched = Join-Path $work (Join-Path 'n0' 'tox-nodes.json')
if ((Test-Path $fetched) -and (-not $seed)) {
    Copy-Item $fetched $shared -Force
    Write-Host "nodes  cached for the next run"
}

# A stalled run is the one whose logs are wanted.
# **Every run's logs are kept (2026-09-10).** The branch below used to delete
# the work directory of a formed run unless `-KeepLogs` was given, and the
# first S1-BM measurement -- 28 hands, one genesis, the whole return road --
# was deleted the moment it finished. A run is evidence; disk is cheap; the
# switch is kept so old command lines still parse.
$KeepLogs = $true
if ($KeepLogs -or -not $formed) {
    Write-Host ''
    Write-Host "logs kept: $work"
} else {
    Remove-Item -Recurse -Force $work
}
