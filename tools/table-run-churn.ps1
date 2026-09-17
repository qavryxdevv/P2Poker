<#
.SYNOPSIS
    A table that forms while its players come and go -- arrivals, lost lines,
    crashes, leaves on purpose -- over this machine and the far one.

.DESCRIPTION
    The owner's goal for a forming table (2026-09-16, after S1-FY): filter out
    the seats whose internet is unreliable without the algorithm freezing and
    without closing a healthy seat's window, and start the table once it is full
    of healthy seats, waiting for an unhealthy one as little as measurement
    allows.

    This starts a founder and Seats-1 joiners and gives every node a plan of
    LIVES, drawn from -Seed: when a joiner arrives; whether its line goes away
    once and for how long (-Outages); whether its client is killed outright and
    started again (-Crashes); whether its player leaves the table on purpose and
    comes back (-Leaves); and whether the founder's line goes away (-FounderOutFor).
    Every fault falls before -ChurnUntil, and from then on every node is present
    with a healthy line: a table that has not started some time after that is
    the finding. The plan is written to plan.json beside the logs, so a run can
    be read, and repeated, from its own directory.

    Every node is a window's client (P2P_POKER_STAYS=1, S1-FY): it never leaves
    a table by itself. -There joiners run on the far machine; every log line
    carries the UTC time and the seconds since the run's own start, the same
    clock on both machines. tools\fold-churn.py reads a run.

    **The far machine may be the owner's.** Only processes whose image lives
    under -FarDir are ever stopped there -- never a client the owner runs.

    Needs a binary built with --features fault-harness, which the build step
    below makes, and PowerShell 7.
#>
#Requires -Version 7.0

[CmdletBinding()]
param(
    [ValidateRange(3, 10)][int]$Seats = 10,
    # Joiners on the far machine. Four logical cores there: three is the most that
    # does not measure starvation once hands are dealt (S1-AY).
    [ValidateRange(0, 9)][int]$There = 3,
    [ValidateRange(120, 3600)][int]$Seconds = 480,
    # 0 draws a seed and prints it.
    [int]$Seed = 0,
    # Joiners arrive over this many seconds from the founder's start.
    [ValidateRange(0, 600)][int]$ArriveOver = 60,
    # Every fault ends before this second; from it on every node is present.
    [ValidateRange(60, 3000)][int]$ChurnUntil = 180,
    [ValidateRange(0, 9)][int]$Outages = 3,
    [ValidateRange(0, 9)][int]$Crashes = 2,
    [ValidateRange(0, 9)][int]$Leaves = 2,
    # The founder's line goes away once, for this long (0: never).
    [ValidateRange(0, 600)][int]$FounderOutFor = 0,
    # ...from this second (0: drawn before -ChurnUntil). D-061 needs the cut
    # before the table fills; a formation that heals fast sets a table first.
    [ValidateRange(0, 3600)][int]$FounderOutAt = 0,
    # The founder's player leaves the table at this second (0: never) -- D-061's
    # continuation, at once.
    [ValidateRange(0, 3600)][int]$FounderLeaveAt = 0,
    # The owner's second phase: these nodes (numbers, comma-separated) meet
    # -AtSetFault the moment their table is set, before hand #1 -- `leave` (the
    # player leaves), `crash` (the client dies) or `cut:<s>` (the line goes away).
    [string]$AtSetNodes = '',
    [ValidatePattern('^(|leave|crash|cut:[0-9]+)$')][string]$AtSetFault = '',
    # Faults during play, after the table is set -- the owner's third phase:
    # `node:kind@at[+for]` entries, comma-separated. `3:cut@200+60` cuts n3's line
    # at 200 s for 60 s; `5:crash@210+40` kills n5 at 210 s and starts it again
    # 40 s later with its record; `7:leave@215+30` has n7's player leave at 215 s
    # and ask for a seat again 30 s later. `for` is 30 s when left out. The
    # founder is n0. A node named here draws no churn role, and its fault may
    # fall before the set as well: the schedule is the schedule.
    [string]$PlayFaults = '',
    # Which joiners run on the far machine (numbers, comma-separated); drawn from
    # -Seed when left out. -There is then their count.
    [string]$FarNodes = '',
    [string]$Target = '',
    [string]$KeyPath = '',
    [string]$Exe,
    [string]$FarDir = 'C:\p2ptest\churn',
    [switch]$NoBuild
)

$ErrorActionPreference = 'Stop'
trap {
    Write-Host "the run did not complete: $($_.Exception.Message)"
    exit 1
}
$inv = [System.Globalization.CultureInfo]::InvariantCulture
$root = Split-Path -Parent $PSScriptRoot
if (-not $FarNodes -and $There -gt $Seats - 1) { throw "-There ${There}: the table has only $($Seats - 1) joiners" }

# --- the binary -------------------------------------------------------------
if (-not $Exe) { $Exe = Join-Path $root 'target\release\p2p-poker.exe' }
if (-not $NoBuild) {
    $cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
    if (-not $cargo) { $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
    Write-Host '==> cargo build --release --features fault-harness'
    $was = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    & $cargo build --release --features fault-harness --manifest-path (Join-Path $root 'Cargo.toml') 2>&1 |
        Where-Object { $_ -match '^error|Finished' } | ForEach-Object { Write-Host "    $_" }
    $built = $LASTEXITCODE
    $ErrorActionPreference = $was
    if ($built -ne 0) { throw 'the build failed; nothing was measured' }
}
if (-not (Test-Path $Exe)) { throw "no binary at $Exe" }
$Exe = (Resolve-Path $Exe).Path
# The feature's presence, read off the bytes (see table-run-split.ps1).
if (-not [System.Text.Encoding]::ASCII.GetString([System.IO.File]::ReadAllBytes($Exe)).Contains('P2P_POKER_STAYS')) {
    throw "$Exe was built without --features fault-harness: every knob of this run would be read by nothing"
}
$newest = Get-ChildItem -Path (Join-Path $root 'src'), (Join-Path $root 'vendor\c-toxcore\toxcore') -Recurse -File |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if ($newest.LastWriteTime -gt (Get-Item $Exe).LastWriteTime) {
    throw "the binary is older than $($newest.Name): build it, or drop -NoBuild"
}

# --- the plan ---------------------------------------------------------------
if ($Seed -eq 0) { $Seed = Get-Random -Minimum 1 -Maximum 1000000 }
$rng = [System.Random]::new($Seed)
function Draw([int]$lo, [int]$hi) {
    if ($hi -le $lo) { return $lo }
    return $lo + $rng.Next($hi - $lo + 1)
}
$joiners = 1..($Seats - 1)
$shuffled = @($joiners | Sort-Object { $rng.Next() })
$farList = @("$FarNodes" -split '[,\s]+' | Where-Object { $_ -ne '' } | ForEach-Object { [int]$_ })
foreach ($f in $farList) { if ($joiners -notcontains $f) { throw "-FarNodes ${f}: not a joiner of a table of $Seats" } }
$far = if ($farList.Count -gt 0) { $farList } else { @($shuffled | Select-Object -First $There) }
$There = $far.Count
$playList = @()
foreach ($p in @("$PlayFaults" -split '[,\s]+' | Where-Object { $_ -ne '' })) {
    if ($p -notmatch '^(\d+):(cut|crash|leave)@(\d+)(\+(\d+))?$') { throw "-PlayFaults entry '$p': expected node:kind@at[+for]" }
    $playList += [pscustomobject]@{
        Node = [int]$Matches[1]; Kind = $Matches[2]; At = [int]$Matches[3]
        For = $(if ($Matches[5]) { [int]$Matches[5] } else { 30 })
    }
}
foreach ($p in $playList) { if ($p.Node -ne 0 -and $joiners -notcontains $p.Node) { throw "-PlayFaults n$($p.Node): not a node of a table of $Seats" } }
$playNodes = @($playList | ForEach-Object Node)
$roles = @{}
$order = @($joiners | Where-Object { $playNodes -notcontains $_ } | Sort-Object { $rng.Next() })
$k = 0
foreach ($r in @(@('outage', $Outages), @('crash', $Crashes), @('leave', $Leaves))) {
    for ($j = 0; $j -lt $r[1] -and $k -lt $order.Count; $j++) { $roles["$($order[$k])"] = $r[0]; $k++ }
}
$lives = [System.Collections.Generic.List[object]]::new()
$atSetList = @("$AtSetNodes" -split '[,\s]+' | Where-Object { $_ -ne '' } | ForEach-Object { [int]$_ })
function Add-Life($node, $life, $start, $stop, $end, $leaveAt, $offAt, $offFor, $resume) {
    $lives.Add([pscustomobject]@{
        Node = $node; Life = $life; Far = ($far -contains $node); Start = $start; Stop = $stop; End = $end
        LeaveAt = $leaveAt; OfflineAt = $offAt; OfflineFor = $offFor; Resume = $resume
        AtSet = $(if ($AtSetFault -and $life -eq 0 -and $atSetList -contains $node) { $AtSetFault } else { '' })
    })
}
# A fault during play, as -PlayFaults asked: the node's whole life with its line
# cut once; or killed and started again with its record; or its player leaving
# and asking for a seat again.
function Add-PlayLives($n, $a, $pf) {
    switch ($pf.Kind) {
        'cut' { Add-Life $n 0 $a $Seconds 'close' 0 ($pf.At - $a) $pf.For $false }
        'crash' {
            Add-Life $n 0 $a $pf.At 'kill' 0 0 0 $false
            Add-Life $n 1 ($pf.At + $pf.For) $Seconds 'close' 0 0 0 $true
        }
        'leave' {
            Add-Life $n 0 $a ($pf.At + 4) 'kill' ($pf.At - $a) 0 0 $false
            Add-Life $n 1 ($pf.At + $pf.For) $Seconds 'close' 0 0 0 $false
        }
    }
}
# The founder: a whole run, its line cut once when asked.
$fOffAt = 0
if ($FounderOutFor -gt 0) { $fOffAt = if ($FounderOutAt -gt 0) { $FounderOutAt } else { Draw 30 ([Math]::Max(30, $ChurnUntil - $FounderOutFor - 10)) } }
$fPlay = @($playList | Where-Object { $_.Node -eq 0 } | Select-Object -First 1)
if ($fPlay.Count -gt 0) { Add-PlayLives 0 0 $fPlay[0] } else { Add-Life 0 0 0 $Seconds 'close' $FounderLeaveAt $fOffAt $FounderOutFor $false }
foreach ($n in $joiners) {
    $a = Draw 3 ([Math]::Max(3, $ArriveOver))
    $pf = @($playList | Where-Object { $_.Node -eq $n } | Select-Object -First 1)
    if ($pf.Count -gt 0) { Add-PlayLives $n $a $pf[0]; continue }
    switch ($roles["$n"]) {
        'outage' {
            $t = Draw ($a + 10) ([Math]::Max($a + 10, $ChurnUntil - 70))
            $d = Draw 15 ([Math]::Max(15, [Math]::Min(60, $ChurnUntil - $t - 5)))
            Add-Life $n 0 $a $Seconds 'close' 0 ($t - $a) $d $false
        }
        'crash' {
            $kill = Draw ($a + 10) ([Math]::Max($a + 10, $ChurnUntil - 45))
            $back = $kill + (Draw 15 40)
            Add-Life $n 0 $a $kill 'kill' 0 0 0 $false
            Add-Life $n 1 $back $Seconds 'close' 0 0 0 $true
        }
        'leave' {
            $leave = Draw ($a + 10) ([Math]::Max($a + 10, $ChurnUntil - 45))
            $back = $leave + (Draw 15 40)
            Add-Life $n 0 $a ($leave + 4) 'kill' ($leave - $a) 0 0 $false
            Add-Life $n 1 $back $Seconds 'close' 0 0 0 $false
        }
        default { Add-Life $n 0 $a $Seconds 'close' 0 0 0 $false }
    }
}

$stamp = (Get-Date).ToString('HHmmss')
$table = "churn$stamp-$Seats"
$work = Join-Path $root (Join-Path 'runs' $table)
New-Item -ItemType Directory -Force -Path $work | Out-Null
[pscustomobject]@{
    table = $table; seats = $Seats; seconds = $Seconds; seed = $Seed; churn_until = $ChurnUntil
    far = $far; roles = $roles; lives = $lives; play = $playList
} | ConvertTo-Json -Depth 5 | Out-File (Join-Path $work 'plan.json') -Encoding utf8

$header = @(
    "table  $table"
    "seats  $Seats  (founder here; $($Seats - 1 - $There) joiner(s) here, $There on the far machine)"
    "for    $Seconds s, seed $Seed, faults before $ChurnUntil s"
    "roles  $(($roles.GetEnumerator() | Sort-Object Name | ForEach-Object { "n$($_.Name) $($_.Value)" }) -join ', ')"
    "atset  $(if ($AtSetFault) { "n$($atSetList -join ', n') meet '$AtSetFault' as the table is set" } else { 'no fault at the set' })"
    "founder$(if ($FounderOutFor -gt 0) { " line cut at $fOffAt s for $FounderOutFor s" } else { ' healthy' })$(if ($FounderLeaveAt -gt 0) { ", leaves its table at $FounderLeaveAt s" })"
    "play   $(if ($playList.Count -gt 0) { ($playList | ForEach-Object { "n$($_.Node) $($_.Kind) at $($_.At) s for $($_.For) s" }) -join ', ' } else { 'no fault during play' })"
    "work   $work"
)
$header | ForEach-Object { Write-Host $_ }
$header | Out-File (Join-Path $work 'run.txt') -Encoding utf8
foreach ($l in ($lives | Sort-Object Start)) {
    $what = switch ($l.End) { 'kill' { if ($l.LeaveAt -gt 0) { "leaves at $($l.Start + $l.LeaveAt) s, gone at $($l.Stop) s" } else { "killed at $($l.Stop) s" } } default { 'to the end' } }
    $cut = if ($l.OfflineFor -gt 0) { ", line cut $($l.Start + $l.OfflineAt)-$($l.Start + $l.OfflineAt + $l.OfflineFor) s" } else { '' }
    Write-Host ("  n{0}.{1} {2,-5} from {3,4} s, {4}{5}" -f $l.Node, $l.Life, $(if ($l.Far) { 'far' } else { 'here' }), $l.Start, $what, $cut)
}

# --- the far machine ------------------------------------------------------
$farLives = @($lives | Where-Object Far)
$ssh = $null
$privKey = $null
if ($farLives.Count -gt 0) {
    . (Join-Path $PSScriptRoot 'machine.ps1')
    if (-not $Target) { $Target = Get-MachineValue 'FarTarget' }
    if (-not $KeyPath) { $KeyPath = Get-MachineValue 'FarKeyPath' }
    if (-not $Target -or -not $KeyPath) { throw 'the far machine is not named: set FarTarget and FarKeyPath in tools\machine.local.psd1' }
    if (-not (Test-Path $KeyPath)) { throw "no key at $KeyPath" }
    $privKey = Join-Path $env:TEMP ('churn-' + [IO.Path]::GetFileName($KeyPath))
    Copy-Item $KeyPath $privKey -Force
    $me = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    & icacls "$privKey" /inheritance:r /grant:r "${me}:(F)" | Out-Null
    $probe = Test-NetConnection -ComputerName (($Target -split '@')[-1]) -Port 22 -WarningAction SilentlyContinue
    if (-not $probe.TcpTestSucceeded) { throw 'nothing answers on the far machine port 22' }
    $ssh = @('-i', $privKey, '-o', "BindAddress=$($probe.SourceAddress.IPAddress)", '-o', 'StrictHostKeyChecking=accept-new',
             '-o', 'UserKnownHostsFile=/dev/null', '-o', 'LogLevel=ERROR', '-o', 'BatchMode=yes')
}
function Invoke-Far([string]$Script) {
    $enc = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($Script))
    (& ssh @ssh $Target "powershell -NoProfile -EncodedCommand $enc" 2>&1) -join "`n"
}

try {
    if ($farLives.Count -gt 0) {
        Write-Host '==> the far machine: stopping only what runs from the bed''s own folder'
        $null = Invoke-Far @"
New-Item -ItemType Directory -Force -Path '$FarDir' | Out-Null
Get-CimInstance Win32_Process -Filter "Name = 'p2p-poker.exe'" |
    Where-Object { `$_.ExecutablePath -and `$_.ExecutablePath.StartsWith('$FarDir\', [StringComparison]::OrdinalIgnoreCase) } |
    ForEach-Object { Stop-Process -Id `$_.ProcessId -Force }
"@
        Start-Sleep -Milliseconds 800
        & scp @ssh $Exe "$($Target):$($FarDir -replace '\\','/')/p2p-poker.exe" | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "could not copy the binary to the far machine (scp exited $LASTEXITCODE)" }
        $localHash = (Get-FileHash $Exe -Algorithm SHA256).Hash
        $farHash = (Invoke-Far "(Get-FileHash '$FarDir\p2p-poker.exe' -Algorithm SHA256).Hash").Trim()
        if ($farHash -ne $localHash) { throw 'the far machine does not hold the binary just built' }
        Write-Host "    far binary verified, sha256 $($localHash.Substring(0, 16))..."
    }

    # One clock for the whole run: its start, in UTC ticks, handed to both machines.
    $t0 = (Get-Date).ToUniversalTime().AddSeconds(8)
    $t0Ticks = $t0.Ticks

    if ($farLives.Count -gt 0) {
        $plan = ($farLives | ConvertTo-Json -Depth 3 -AsArray) -replace "'", "''"
        $farScript = @"
`$ErrorActionPreference = 'Continue'
`$dir = '$FarDir'
`$exe = Join-Path `$dir 'p2p-poker.exe'
`$t0 = [DateTime]::new($t0Ticks, [DateTimeKind]::Utc)
`$lives = '$plan' | ConvertFrom-Json
`$seatKeys = Join-Path `$env:LOCALAPPDATA 'p2p-poker-test-seats'
New-Item -ItemType Directory -Force -Path `$seatKeys | Out-Null
foreach (`$n in (`$lives | ForEach-Object Node | Sort-Object -Unique)) {
    `$p = Join-Path `$dir "n`$n"
    if (Test-Path `$p) { Remove-Item -Recurse -Force `$p }
    New-Item -ItemType Directory -Force -Path `$p | Out-Null
    `$seed = Join-Path `$seatKeys "churn-far-n`$n.key"
    if (-not (Test-Path `$seed)) {
        `$b = New-Object byte[] 32
        `$r = [System.Security.Cryptography.RandomNumberGenerator]::Create()
        try { `$r.GetBytes(`$b) } finally { `$r.Dispose() }
        [System.IO.File]::WriteAllBytes(`$seed, `$b)
    }
    Copy-Item `$seed (Join-Path `$p 'identity.key') -Force
}
`$events = @()
foreach (`$l in `$lives) {
    `$events += [pscustomobject]@{ At = [double]`$l.Start; Kind = 'start'; L = `$l }
    if (`$l.End -eq 'kill') { `$events += [pscustomobject]@{ At = [double]`$l.Stop; Kind = 'kill'; L = `$l } }
}
`$jobs = @()
foreach (`$e in (`$events | Sort-Object At)) {
    `$wait = `$e.At - ((Get-Date).ToUniversalTime() - `$t0).TotalSeconds
    if (`$wait -gt 0) { Start-Sleep -Milliseconds ([int](`$wait * 1000)) }
    `$l = `$e.L
    `$p = Join-Path `$dir "n`$(`$l.Node)"
    if (`$e.Kind -eq 'kill') {
        Get-CimInstance Win32_Process -Filter "Name = 'p2p-poker.exe'" |
            Where-Object { `$_.ExecutablePath -and `$_.ExecutablePath.StartsWith("`$dir\", [StringComparison]::OrdinalIgnoreCase) -and `$_.CommandLine -like "* `$p *" } |
            ForEach-Object { Stop-Process -Id `$_.ProcessId -Force }
        continue
    }
    `$log = Join-Path `$dir ("n{0}-{1}.log" -f `$l.Node, `$l.Life)
    `$for = [int](`$l.Stop - `$l.Start) + 5
    `$a = @('--headless', '--autoplay', '--no-mdns', '--for', "`$for", '--profile', `$p, '--join', '$table')
    if (`$l.Resume) { `$a += '--resume' }
    `$knobs = @{ P2P_POKER_STAYS = '1' }
    if (`$l.OfflineFor -gt 0) { `$knobs['P2P_POKER_OFFLINE_AT'] = "`$(`$l.OfflineAt)"; `$knobs['P2P_POKER_OFFLINE_FOR'] = "`$(`$l.OfflineFor)" }
    if (`$l.LeaveAt -gt 0) { `$knobs['P2P_POKER_LEAVE_TABLE_AT'] = "`$(`$l.LeaveAt)" }
    if (`$l.AtSet) { `$knobs['P2P_POKER_AT_SET'] = "`$(`$l.AtSet)" }
    `$jobs += Start-Job -ArgumentList `$exe, `$a, `$log, `$knobs, `$t0 -ScriptBlock {
        param(`$exe, `$a, `$log, `$knobs, `$t0)
        foreach (`$k in `$knobs.Keys) { Set-Item -Path "env:`$k" -Value `$knobs[`$k] }
        `$inv = [System.Globalization.CultureInfo]::InvariantCulture
        & `$exe @a 2>&1 | ForEach-Object {
            `$now = (Get-Date).ToUniversalTime()
            `$now.ToString('HH:mm:ss.fff', `$inv) + ' ' + ((`$now - `$t0).TotalSeconds.ToString('F1', `$inv)).PadLeft(7) + '  ' + `$_
        } | Out-File -FilePath `$log -Encoding utf8
    }
}
`$left = $Seconds + 30 - ((Get-Date).ToUniversalTime() - `$t0).TotalSeconds
if (`$left -gt 0) { `$null = Wait-Job -Job `$jobs -Timeout ([int]`$left) }
Get-CimInstance Win32_Process -Filter "Name = 'p2p-poker.exe'" |
    Where-Object { `$_.ExecutablePath -and `$_.ExecutablePath.StartsWith("`$dir\", [StringComparison]::OrdinalIgnoreCase) } |
    ForEach-Object { Stop-Process -Id `$_.ProcessId -Force }
`$jobs | Remove-Job -Force
'far end done'
"@
        $farFile = Join-Path $work 'far.ps1'
        Set-Content -Path $farFile -Value $farScript -Encoding UTF8
        & scp @ssh $farFile "$($Target):$($FarDir -replace '\\','/')/far.ps1" | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'could not copy the far script' }
        $farJob = Start-Job -ArgumentList $ssh, $Target, $FarDir -ScriptBlock {
            param($ssh, $Target, $FarDir)
            & ssh @ssh $Target "powershell -NoProfile -ExecutionPolicy Bypass -File $FarDir\far.ps1" 2>&1
        }
    }

    # --- this machine ------------------------------------------------------
    $seatKeys = Join-Path $env:LOCALAPPDATA 'p2p-poker-test-seats'
    New-Item -ItemType Directory -Force -Path $seatKeys | Out-Null
    $shared = Join-Path $root 'runs\tox-nodes.json'
    foreach ($n in (@($lives | Where-Object { -not $_.Far }) | ForEach-Object Node | Sort-Object -Unique)) {
        $p = Join-Path $work "n$n"
        New-Item -ItemType Directory -Force -Path $p | Out-Null
        $seedFile = Join-Path $seatKeys "churn-here-n$n.key"
        if (-not (Test-Path $seedFile)) {
            $b = New-Object byte[] 32
            $r = [System.Security.Cryptography.RandomNumberGenerator]::Create()
            try { $r.GetBytes($b) } finally { $r.Dispose() }
            [System.IO.File]::WriteAllBytes($seedFile, $b)
        }
        Copy-Item $seedFile (Join-Path $p 'identity.key') -Force
        if (Test-Path $shared) { Copy-Item $shared (Join-Path $p 'tox-nodes.json') -Force }
    }
    $events = @()
    foreach ($l in @($lives | Where-Object { -not $_.Far })) {
        $events += [pscustomobject]@{ At = [double]$l.Start; Kind = 'start'; L = $l }
        if ($l.End -eq 'kill') { $events += [pscustomobject]@{ At = [double]$l.Stop; Kind = 'kill'; L = $l } }
    }
    $jobs = @()
    Write-Host "==> the run starts at $($t0.ToString('HH:mm:ss', $inv)) UTC"
    foreach ($e in ($events | Sort-Object At)) {
        $wait = $e.At - ((Get-Date).ToUniversalTime() - $t0).TotalSeconds
        if ($wait -gt 0) { Start-Sleep -Milliseconds ([int]($wait * 1000)) }
        $l = $e.L
        $p = Join-Path $work "n$($l.Node)"
        if ($e.Kind -eq 'kill') {
            Get-CimInstance Win32_Process -Filter "Name = 'p2p-poker.exe'" |
                Where-Object { $_.CommandLine -like "* $p *" } |
                ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
            Write-Host ("    {0,5:F0} s  n{1}.{2} killed" -f ((Get-Date).ToUniversalTime() - $t0).TotalSeconds, $l.Node, $l.Life)
            continue
        }
        $log = Join-Path $work ("n{0}-{1}.log" -f $l.Node, $l.Life)
        $for = [int]($l.Stop - $l.Start) + 5
        if ($l.Node -eq 0) { $for = $Seconds }
        $a = @('--headless', '--autoplay', '--no-mdns', '--for', "$for", '--profile', $p)
        if ($l.Node -eq 0) { $a += @('--host', $table, '--seats', "$Seats") } else { $a += @('--join', $table) }
        if ($l.Resume) { $a += '--resume' }
        $knobs = @{ P2P_POKER_STAYS = '1' }
        if ($l.OfflineFor -gt 0) { $knobs['P2P_POKER_OFFLINE_AT'] = "$($l.OfflineAt)"; $knobs['P2P_POKER_OFFLINE_FOR'] = "$($l.OfflineFor)" }
        if ($l.LeaveAt -gt 0) { $knobs['P2P_POKER_LEAVE_TABLE_AT'] = "$($l.LeaveAt)" }
        if ($l.AtSet) { $knobs['P2P_POKER_AT_SET'] = "$($l.AtSet)" }
        $jobs += Start-Job -ArgumentList $Exe, $a, $log, $knobs, $t0 -ScriptBlock {
            param($exe, $a, $log, $knobs, $t0)
            foreach ($k in $knobs.Keys) { Set-Item -Path "env:$k" -Value $knobs[$k] }
            $inv = [System.Globalization.CultureInfo]::InvariantCulture
            & $exe @a 2>&1 | ForEach-Object {
                $now = (Get-Date).ToUniversalTime()
                $now.ToString('HH:mm:ss.fff', $inv) + ' ' + (($now - $t0).TotalSeconds.ToString('F1', $inv)).PadLeft(7) + '  ' + $_
            } | Out-File -FilePath $log -Encoding utf8
        }
        Write-Host ("    {0,5:F0} s  n{1}.{2} started" -f ((Get-Date).ToUniversalTime() - $t0).TotalSeconds, $l.Node, $l.Life)
    }
    $left = $Seconds + 40 - ((Get-Date).ToUniversalTime() - $t0).TotalSeconds
    Write-Host "==> waiting up to $([int]$left) s"
    $all = @($jobs) + @($(if ($farLives.Count -gt 0) { $farJob }))
    if ($left -gt 0) { $null = Wait-Job -Job $all -Timeout ([int]$left) }
    Get-CimInstance Win32_Process -Filter "Name = 'p2p-poker.exe'" |
        Where-Object { $_.CommandLine -like "*$work*" } |
        ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
    $all | Where-Object { $_.State -eq 'Running' } | Stop-Job
    $all | Remove-Job -Force

    if ($farLives.Count -gt 0) {
        Write-Host '==> collecting the far logs'
        foreach ($l in $farLives) {
            $name = "n$($l.Node)-$($l.Life).log"
            & scp @ssh "$($Target):$($FarDir -replace '\\','/')/$name" (Join-Path $work $name) 2>&1 | Out-Null
            if ($LASTEXITCODE -ne 0) { Write-Warning "far life n$($l.Node).$($l.Life) left no log" }
        }
    }
    Write-Host ''
    Write-Host "logs: $work"
    Write-Host "read it with: python tools\fold-churn.py $work"
}
finally {
    if ($privKey) { Remove-Item -Force $privKey -ErrorAction SilentlyContinue }
}
