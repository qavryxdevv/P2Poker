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
    # `--features divergence-harness`; without it the environment variable is
    # not read and the run is an ordinary one.
    [ValidateRange(0, 100000)][int]$DivergeAt = 0,
    # Which node tells the lie. Its own log is where the OTHER nodes' complaints
    # are not: a peer that diverges does not know it, which is the whole reason
    # everybody else compares.
    [ValidateRange(0, 32)][int]$DivergeNode = 1,
    [ValidateRange(0, 3600)][int]$DropAt = 0,
    [ValidateRange(0, 600)][int]$DropFor = 20,
    # A seat slower than this to enter the Tox group keeps the logs, however
    # well the rest of the run went. Sixty seconds is far outside the ordinary
    # spread, which has been 10-25 s in every run measured.
    [int]$LateGroupSeconds = 60
)

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
$stamp = (Get-Date).ToString('HHmmss')
$table = "run$stamp-$Seats"

$work = Join-Path $env:TEMP "p2p-table-run\$table"
New-Item -ItemType Directory -Force -Path $work | Out-Null

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
for ($i = 0; $i -lt $Seats; $i++) {
    $profileDir = Join-Path $work "n$i"
    New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
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
    $dropping = ($DropAt -gt 0 -and $i -eq 1 -and -not $leaving)
    if ($dropping) { $mine = $DropAt }
    $nodeArgs = @('--headless', '--autoplay', '--for', "$mine", '--profile', $profileDir)
    if ($i -eq 0) {
        $nodeArgs += @('--host', $table, '--seats', "$Seats")
    } else {
        $nodeArgs += @('--join', $table)
    }

    $diverge = if ($DivergeAt -gt 0 -and $i -eq $DivergeNode) { $DivergeAt } else { 0 }
    $jobs += Start-Job -Name "n$i" -ArgumentList $Exe, $nodeArgs, $log, $diverge -ScriptBlock {
        param($exe, $nodeArgs, $log, $diverge)
        if ($diverge -gt 0) { $env:P2P_POKER_DIVERGE_AT_HAND = "$diverge" }
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
    Write-Host "    (needs a binary built with --features divergence-harness)"
}
if ($DropAt -gt 0 -and $LeaverSeconds -eq 0) {
    Write-Host "==> n1 drops at $DropAt s and returns $DropFor s later, same profile"
    $return = Start-Job -ArgumentList $Exe, $work, $table, $Seconds, $DropAt, $DropFor -ScriptBlock {
        param($exe, $work, $table, $seconds, $dropAt, $dropFor)
        Start-Sleep -Seconds ($dropAt + $dropFor)
        # **The same profile, deliberately.** It carries the identity and the
        # application key, so this is the seat coming back rather than a new
        # player taking one.
        $p = Join-Path $work 'n1'
        $log = Join-Path $work 'n1-again.log'
        $start = Get-Date
        $inv = [System.Globalization.CultureInfo]::InvariantCulture
        $left = $seconds - $dropAt - $dropFor
        if ($left -lt 30) { $left = 30 }
        & $exe --headless --autoplay --for "$left" --profile $p --join $table 2>&1 |
            ForEach-Object { ((((Get-Date) - $start).TotalSeconds).ToString('F1', $inv)).PadLeft(7) + '  ' + $_ } |
            Out-File -FilePath $log -Encoding utf8
    }
    $jobs += $return
}

Write-Host "$Seats nodes started; waiting up to $($Seconds + 60) s"
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
for ($i = 0; $i -lt $Seats; $i++) { $logs += @{ Name = "n$i"; Path = (Join-Path $work "n$i.log") } }
if ($LeaverSeconds -gt 0) { $logs += @{ Name = 'nR'; Path = (Join-Path $work 'nR.log') } }
if ($DropAt -gt 0 -and $LeaverSeconds -eq 0) { $logs += @{ Name = 'n1-again'; Path = (Join-Path $work 'n1-again.log') } }

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
    foreach ($l in $lines) {
        if ($l -match "^\s*([0-9.]+)\s+in the table's Tox group") {
            if ($null -eq $group) { $group = [double]::Parse($Matches[1], $inv) }
        }
        if ($l -match '^\s*([0-9.]+)\s+hand #(\d+) opens at genesis (\w+) with seats \[([^\]]*)\]') {
            $opens += [pscustomobject]@{
                At      = [double]::Parse($Matches[1], $inv)
                Hand    = [int]$Matches[2]
                Genesis = $Matches[3]
                Seats   = $Matches[4]
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
            $ref = $founder.Opens | Where-Object { $_.Hand -eq $o.Hand } | Select-Object -First 1
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
        $ref = $founder.Opens | Where-Object { $_.Hand -eq $o.Hand } | Select-Object -First 1
        ($null -ne $ref) -and ($ref.Genesis -ne $o.Genesis)
    }).Count -gt 0
})

Write-Host ''
Write-Host '--- result ---'
if ($formed) {
    Write-Host ($formed | Select-Object -Last 1)
} else {
    Write-Host 'NO TABLE - the seats never all arrived'
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

# A stalled run is the one whose logs are wanted.
if ($KeepLogs -or -not $formed) {
    Write-Host ''
    Write-Host "logs kept: $work"
} else {
    Remove-Item -Recurse -Force $work
}
