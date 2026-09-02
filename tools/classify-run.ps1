<#
.SYNOPSIS
    Read a kept table-run log directory and say what shape of failure, if any, it holds.

.DESCRIPTION
    **The harness reports hands; the failures this project keeps finding are not
    about hands.** `table-run.ps1`'s summary counts hands opened and finished per
    node and warns when a node opened some and finished none. Three real failures
    measured on 2026-09-02 do not show up in that number at all:

      * a seat that never entered the Tox group (`group 0/N`) opens no hands, so
        it contributes a quiet `0` to a table that otherwise looks healthy;
      * a seat that entered the group and then stopped being heard leaves the
        others at `checkpoint hand K waiting for [s]` while its own line says it
        is present and 0 ms away;
      * a seat one ratification short of a full set (`S1-P`) never computes a
        session, never opens a hand, and is invisible from every side - the
        founder counts it, it holds the roster, and no number says the session
        was never computed.

    An A/B experiment run the same day used a classifier that looked only for
    `group 0/`, so two of those three would have been scored as clean runs. This
    exists so a measurement cannot quietly answer a narrower question than the
    one that was asked.

    # Two rules, and the first draft of this script broke both of them

    **A message that every healthy run prints is not a finding.** The first draft
    reported `FLOOR-HELD` on all four nodes of an eighteen-hand run that was
    perfect: every node legitimately holds `group 0/N` for the first ten to
    twenty seconds, says so, and then fills. Read against the last line, not the
    first occurrence.

    **A run stopped by its own `--for` is always mid-hand.** The first draft
    reported `STAGE-STALLED` on every node of every completed run, because the
    last status line before a deliberate shutdown is naturally waiting for
    somebody. A stall is only a stall if it does not move: the same hand across
    at least two consecutive status lines, which is thirty seconds apart.

    Both are the same error the `S1-P` row records in the register - a classifier
    that counts an ordinary transient as a failure - and they are written down
    here because it has now been made three times.

    It reads only the logs a run leaves behind, so it can be pointed at anything
    already on disk, including runs from before it existed.

.PARAMETER Dir
    A run's work directory - the path `table-run.ps1 -KeepLogs` prints. Defaults
    to the most recent one under $env:TEMP\p2p-table-run.

.PARAMETER All
    Classify every run directory on this machine and print one line each.

.EXAMPLE
    powershell -File tools\classify-run.ps1
    powershell -File tools\classify-run.ps1 -All
#>
[CmdletBinding()]
param(
    [string]$Dir,
    [switch]$All,
    # Print the evidence behind every verdict rather than the verdict alone.
    [switch]$Why
)

$ErrorActionPreference = 'Stop'
$root = Join-Path $env:TEMP 'p2p-table-run'

function Get-RunVerdict {
    param([string]$RunDir)

    $logs = Get-ChildItem -Path $RunDir -Filter 'n*.log' -ErrorAction SilentlyContinue | Sort-Object Name
    if (-not $logs) { return $null }

    $nodes = @()
    foreach ($f in $logs) {
        $name  = [IO.Path]::GetFileNameWithoutExtension($f.Name)
        $lines = Get-Content $f.FullName
        $status = @($lines | Select-String 'seats on the line')
        $last   = if ($status) { "$($status[-1])" } else { '' }
        $opened   = @($lines | Select-String 'hand #\d+ opens at genesis').Count
        $finished = @($lines | Select-String 'hand #\d+ is over').Count

        $verdicts = @()
        $why      = @()

        # **A node that opened a hand computed a session, whatever the status
        # line says.** The status line prints every thirty seconds; a table that
        # settles between the last line and the end of the run leaves a final
        # `ratified n/m` behind that is stale rather than true. Reading it as a
        # verdict is the third repetition of the same mistake and it cost a run
        # in `run095807-6`, where the table filled after 92.3 s and played.
        #
        # So `opened` is the discriminator for anything that claims formation
        # never completed. It is a fact about what the node did, not about what
        # it last said.
        $formed = $opened -gt 0

        # -- never entered the group, and never went on to play -----------------
        # The `group 0/N` line is normal for the first ten to twenty seconds of
        # every run.
        if ($last -match 'group 0/([1-9])' -and -not $formed) {
            $verdicts += 'ISOLATED'
            $why += "never entered the Tox group and never dealt; last line says $($Matches[0])"
        }
        elseif ($last -match 'group (\d+)/(\d+)' -and [int]$Matches[1] -lt [int]$Matches[2] -and -not $formed) {
            $verdicts += 'PARTIAL-GROUP'
            $why += "group never filled and this node never dealt: $($Matches[0])"
        }

        # -- S1-P: a ratification set that never completed ------------------------
        # `ratified n/m` prints only while a table has no session. On the last
        # line of a node that never dealt, the session really was never computed.
        if ($last -match 'ratified (\d+)/(\d+)' -and -not $formed) {
            $verdicts += 'NO-SESSION'
            $why += "the session was never computed and no hand was ever opened: ratified $($Matches[1])/$($Matches[2]) (S1-P)"
        }

        # -- a stage that stopped moving ------------------------------------------
        # Persistence is the whole test. A run killed by `--for` is mid-hand by
        # construction, so one line proves nothing; the same hand on two
        # consecutive status lines is thirty seconds of no progress.
        $hands = @()
        foreach ($s in $status) {
            if ("$s" -match 'checkpoint hand (\d+) waiting for \[([^\]]+)\]') {
                $hands += [pscustomobject]@{ Hand = $Matches[1]; Seats = $Matches[2] }
            } else {
                $hands += $null
            }
        }
        $stuck = $false
        for ($i = 1; $i -lt $hands.Count; $i++) {
            if ($hands[$i] -and $hands[$i-1] -and $hands[$i].Hand -eq $hands[$i-1].Hand) { $stuck = $true; $stalledAt = $hands[$i] }
        }
        if ($stuck) {
            $verdicts += 'STAGE-STALLED'
            $why += "hand $($stalledAt.Hand) waited for seat(s) $($stalledAt.Seats) across two status lines - thirty seconds with no progress"
        }

        # -- the dealing floor as the LAST word ------------------------------------
        # Not merely that it fired: every node fires it while the group fills.
        if ($opened -eq 0 -and ($lines | Select-String -Quiet 'This client is isolated and the other seats will certify it out')) {
            $verdicts += 'NEVER-DEALT'
            $why += "opened no hand at all; the S1-AA floor held it back and the group never filled"
        }

        # -- opened hands and finished none: it heard nobody ------------------------
        # `table-run.ps1` warns on this and it is kept here because the two
        # scripts must not disagree, and because the correction above -- treating
        # any node that opened a hand as formed -- would otherwise hide it. A
        # node can compute a session, open a hand, and still be alone.
        #
        # **Two, not one.** A table that opens its first hand a few seconds
        # before `--for` expires shows `opened 1, finished 0` and is not broken -
        # it simply ran out of clock, and `run095807-6` is exactly that. A node
        # that opened a SECOND hand abandoned the first at its own deadline,
        # which is the shape that means nobody answered.
        if ($opened -ge 2 -and $finished -eq 0) {
            $verdicts += 'HEARD-NOBODY'
            $why += "opened $opened hands and finished none - each was abandoned at its own deadline"
        }

        # -- a seat the table left behind -------------------------------------------
        $out = @($lines | Select-String 'this client is out: the table is at hand')
        if ($out.Count -gt 0) {
            $verdicts += 'ADRIFT'
            $why += ("$($out[-1])" -replace '^\s*[\d.]+\s+', '')
        }

        if (-not $verdicts) { $verdicts += 'ok' }

        $nodes += [pscustomobject]@{
            Node    = $name
            Opened  = $opened
            Verdict = ($verdicts -join ',')
            Why     = ($why -join '; ')
        }
    }

    [pscustomobject]@{
        Run   = Split-Path -Leaf $RunDir
        Nodes = $nodes
        Bad   = @($nodes | Where-Object { $_.Verdict -ne 'ok' })
    }
}

if ($All) {
    if (-not (Test-Path $root)) { throw "no runs under $root" }
    foreach ($d in (Get-ChildItem -Path $root -Directory | Sort-Object LastWriteTime)) {
        $v = Get-RunVerdict -RunDir $d.FullName
        if (-not $v) { continue }
        $shapes = if ($v.Bad.Count) { (($v.Bad | ForEach-Object { "$($_.Node):$($_.Verdict)" }) -join ' ') } else { 'clean' }
        '{0,-16} {1,2} nodes  {2}' -f $v.Run, $v.Nodes.Count, $shapes
    }
    return
}

if (-not $Dir) {
    if (-not (Test-Path $root)) { throw "no runs under $root" }
    $Dir = (Get-ChildItem -Path $root -Directory | Sort-Object LastWriteTime -Descending | Select-Object -First 1).FullName
}
if (-not (Test-Path $Dir)) { throw "no such directory: $Dir" }

$v = Get-RunVerdict -RunDir $Dir
if (-not $v) { throw "no n*.log files in $Dir" }

Write-Host ""
Write-Host "run    $($v.Run)"
Write-Host "nodes  $($v.Nodes.Count)"
Write-Host ""
$v.Nodes | Format-Table -AutoSize Node, Opened, Verdict

if ($Why) {
    foreach ($x in $v.Bad) { Write-Host ""; Write-Host "$($x.Node): $($x.Why)" }
}

# **A table can be healthy while a seat is lost, and that is the whole point.**
# The founder's own numbers look right in every one of these failures, so the
# verdict is taken over the nodes rather than from the founder.
Write-Host ""
if ($v.Bad.Count -eq 0) {
    Write-Host "verdict: clean - every node held a session, filled its group, dealt, and stalled on nobody"
} else {
    Write-Host "verdict: $($v.Bad.Count) of $($v.Nodes.Count) node(s) did not come through:"
    foreach ($x in $v.Bad) { Write-Host "  $($x.Node)  $($x.Verdict)  -  $($x.Why)" }
}
Write-Host ""
