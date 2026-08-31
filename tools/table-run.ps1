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
    [switch]$KeepLogs
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
Write-Host "work   $work"
Write-Host ''

# --- start them ------------------------------------------------------------
# Every node is a job that timestamps each line as it arrives. `Start-Process
# -RedirectStandardOutput` would be shorter and would lose the timing, which is
# the half of the measurement that says WHERE a hand's time goes.
$jobs = @()
for ($i = 0; $i -lt $Seats; $i++) {
    $profileDir = Join-Path $work "n$i"
    New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
    $log = Join-Path $work "n$i.log"

    $nodeArgs = @('--headless', '--autoplay', '--for', "$Seconds", '--profile', $profileDir)
    if ($i -eq 0) {
        $nodeArgs += @('--host', $table, '--seats', "$Seats")
    } else {
        $nodeArgs += @('--join', $table)
    }

    $jobs += Start-Job -Name "n$i" -ArgumentList $Exe, $nodeArgs, $log -ScriptBlock {
        param($exe, $nodeArgs, $log)
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
    if ($i -eq 0) { Start-Sleep -Seconds 3 } else { Start-Sleep -Milliseconds 400 }
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
$nodes = @()
for ($i = 0; $i -lt $Seats; $i++) {
    $log = Join-Path $work "n$i.log"
    if (-not (Test-Path $log)) {
        $nodes += [pscustomobject]@{ Node = "n$i"; Group = $null; Opens = @(); Overs = 0; Formed = $false }
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
    $nodes += [pscustomobject]@{ Node = "n$i"; Group = $group; Opens = $opens; Overs = $overs; Formed = $formed }
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
$never = @($nodes | Where-Object { $null -eq $_.Group })
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
