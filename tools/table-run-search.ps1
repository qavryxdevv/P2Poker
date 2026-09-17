<#
.SYNOPSIS
    Clients that search for a game by themselves (D-064), over this machine and
    the far one, with lines cut, clients killed and searches cancelled while
    they look.

.DESCRIPTION
    The owner's design (2026-09-17): a player presses one button and the client
    finds a Sit & Go -- reserving seats at the tables closest to starting,
    founding one when nothing is on offer, starting its own with the players
    that came, giving every other seat back the moment a game starts.

    This starts -Nodes searching clients, each with a format (-Formats: one for
    all, or a comma-separated list per node: hu, 6, 10, auto) and a number of
    games at once (-Tables, the same way), arriving over -ArriveOver seconds.
    Drawn from -Seed: which nodes have their line cut once while they search
    (-Cuts), which are killed and started again (-Crashes), and which cancel
    the search and start it again twenty seconds later (-Cancels). Every fault
    falls before -ChurnUntil. Tournaments start with -StartStack chips so they
    end inside the run, and -Again has every client search again when its game
    ends -- the owner's bonus, measured.

    Every node is a window's client (P2P_POKER_STAYS=1). -FarNodes run on the far
    machine; every log line carries the UTC time and the seconds since the run's
    own start, one clock on both machines. tools\fold-search.py reads a run.

    **The far machine may be the owner's.** Only processes whose image lives
    under -FarDir are ever stopped there -- never a client the owner runs.

    Needs a binary built with --features fault-harness, which the build step
    below makes, and PowerShell 7.
#>
#Requires -Version 7.0

[CmdletBinding()]
param(
    [ValidateRange(2, 12)][int]$Nodes = 6,
    # Which nodes run on the far machine (numbers, comma-separated); drawn from
    # -Seed when left out, -There of them. Four logical cores there: two at most
    # once hands are dealt.
    [string]$FarNodes = '',
    [ValidateRange(0, 6)][int]$There = 0,
    [ValidateRange(120, 3600)][int]$Seconds = 420,
    # 0 draws a seed and prints it.
    [int]$Seed = 0,
    # Nodes arrive over this many seconds from the run's start.
    [ValidateRange(0, 600)][int]$ArriveOver = 40,
    # hu, 6, 10 or auto: one for every node, or one per node, comma-separated.
    [string]$Formats = 'auto',
    # Games at once, 1..4: one for every node, or one per node.
    [string]$Tables = '1',
    # Faults, all before -ChurnUntil.
    [ValidateRange(0, 9)][int]$Cuts = 0,
    [ValidateRange(0, 9)][int]$Crashes = 0,
    [ValidateRange(0, 9)][int]$Cancels = 0,
    [ValidateRange(60, 3000)][int]$ChurnUntil = 200,
    # Every seat's stack at a table founded here: 300 against blinds of 50/100
    # ends a tournament in a few hands.
    [ValidateRange(0, 100000)][int]$StartStack = 300,
    # --search-again for every node: search again when the game ends.
    [switch]$Again,
    [string]$Target = '',
    [string]$KeyPath = '',
    [string]$Exe,
    [string]$FarDir = 'C:\p2ptest\search',
    [switch]$NoBuild
)

$ErrorActionPreference = 'Stop'
trap {
    Write-Host "the run did not complete: $($_.Exception.Message)"
    exit 1
}
$inv = [System.Globalization.CultureInfo]::InvariantCulture
$root = Split-Path -Parent $PSScriptRoot

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
$nodeList = 0..($Nodes - 1)
$formatList = @("$Formats" -split '[,\s]+' | Where-Object { $_ -ne '' })
$tableList = @("$Tables" -split '[,\s]+' | Where-Object { $_ -ne '' })
function Format-Of([int]$n) { if ($formatList.Count -eq 1) { $formatList[0] } else { $formatList[$n % $formatList.Count] } }
function Tables-Of([int]$n) { if ($tableList.Count -eq 1) { [int]$tableList[0] } else { [int]$tableList[$n % $tableList.Count] } }
foreach ($f in $formatList) { if ($f -notin @('hu', '6', '10', 'auto')) { throw "-Formats '$f': hu, 6, 10 or auto" } }
$farList = @("$FarNodes" -split '[,\s]+' | Where-Object { $_ -ne '' } | ForEach-Object { [int]$_ })
foreach ($f in $farList) { if ($nodeList -notcontains $f) { throw "-FarNodes ${f}: not a node of $Nodes" } }
$shuffled = @($nodeList | Sort-Object { $rng.Next() })
$far = if ($farList.Count -gt 0) { $farList } else { @($shuffled | Select-Object -First $There) }
$There = $far.Count
$roles = @{}
$order = @($nodeList | Sort-Object { $rng.Next() })
$k = 0
foreach ($r in @(@('cut', $Cuts), @('crash', $Crashes), @('cancel', $Cancels))) {
    for ($j = 0; $j -lt $r[1] -and $k -lt $order.Count; $j++) { $roles["$($order[$k])"] = $r[0]; $k++ }
}
$lives = [System.Collections.Generic.List[object]]::new()
function Add-Life($node, $life, $start, $stop, $end, $offAt, $offFor, $cancelAt, $againAt, $resume) {
    $lives.Add([pscustomobject]@{
        Node = $node; Life = $life; Far = ($far -contains $node); Start = $start; Stop = $stop; End = $end
        Format = (Format-Of $node); Tables = (Tables-Of $node)
        OfflineAt = $offAt; OfflineFor = $offFor; CancelAt = $cancelAt; AgainAt = $againAt; Resume = $resume
    })
}
foreach ($n in $nodeList) {
    $a = Draw 3 ([Math]::Max(3, $ArriveOver))
    switch ($roles["$n"]) {
        'cut' {
            $t = Draw ($a + 20) ([Math]::Max($a + 20, $ChurnUntil - 60))
            $d = Draw 15 ([Math]::Max(15, [Math]::Min(45, $ChurnUntil - $t - 5)))
            Add-Life $n 0 $a $Seconds 'close' ($t - $a) $d 0 0 $false
        }
        'crash' {
            $kill = Draw ($a + 20) ([Math]::Max($a + 20, $ChurnUntil - 45))
            $back = $kill + (Draw 15 40)
            Add-Life $n 0 $a $kill 'kill' 0 0 0 0 $false
            Add-Life $n 1 $back $Seconds 'close' 0 0 0 0 $true
        }
        'cancel' {
            $c = Draw ($a + 15) ([Math]::Max($a + 15, $ChurnUntil - 40))
            Add-Life $n 0 $a $Seconds 'close' 0 0 ($c - $a) ($c - $a + 20) $false
        }
        default { Add-Life $n 0 $a $Seconds 'close' 0 0 0 0 $false }
    }
}

$stamp = (Get-Date).ToString('HHmmss')
$table = "search$stamp-$Nodes"
$work = Join-Path $root (Join-Path 'runs' $table)
New-Item -ItemType Directory -Force -Path $work | Out-Null
[pscustomobject]@{
    table = $table; nodes = $Nodes; seconds = $Seconds; seed = $Seed; churn_until = $ChurnUntil
    far = $far; roles = $roles; lives = $lives; formats = $formatList; tables = $tableList
    start_stack = $StartStack; again = [bool]$Again
} | ConvertTo-Json -Depth 5 | Out-File (Join-Path $work 'plan.json') -Encoding utf8

$header = @(
    "run    $table"
    "nodes  $Nodes searching ($($Nodes - $There) here, $There on the far machine); formats $($formatList -join ','); games at once $($tableList -join ',')"
    "for    $Seconds s, seed $Seed, faults before $ChurnUntil s, start stack $StartStack$(if ($Again) { ', search again after the game' })"
    "roles  $(($roles.GetEnumerator() | Sort-Object Name | ForEach-Object { "n$($_.Name) $($_.Value)" }) -join ', ')"
    "work   $work"
)
$header | ForEach-Object { Write-Host $_ }
$header | Out-File (Join-Path $work 'run.txt') -Encoding utf8
foreach ($l in ($lives | Sort-Object Start)) {
    $what = switch ($l.End) { 'kill' { "killed at $($l.Stop) s" } default { 'to the end' } }
    $cut = if ($l.OfflineFor -gt 0) { ", line cut $($l.Start + $l.OfflineAt)-$($l.Start + $l.OfflineAt + $l.OfflineFor) s" } else { '' }
    $can = if ($l.CancelAt -gt 0) { ", cancels at $($l.Start + $l.CancelAt) s and searches again at $($l.Start + $l.AgainAt) s" } else { '' }
    Write-Host ("  n{0}.{1} {2,-5} {3,-4} x{4} from {5,4} s, {6}{7}{8}" -f $l.Node, $l.Life, $(if ($l.Far) { 'far' } else { 'here' }), $l.Format, $l.Tables, $l.Start, $what, $cut, $can)
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
    $privKey = Join-Path $env:TEMP ('search-' + [IO.Path]::GetFileName($KeyPath))
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
    $againFlag = if ($Again) { '1' } else { '0' }

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
    `$seed = Join-Path `$seatKeys "search-far-n`$n.key"
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
    `$a = @('--headless', '--autoplay', '--no-mdns', '--for', "`$for", '--profile', `$p, '--search', "`$(`$l.Format)", '--search-tables', "`$(`$l.Tables)")
    if ('$againFlag' -eq '1') { `$a += '--search-again' }
    if (`$l.Resume) { `$a += '--resume' }
    `$knobs = @{ P2P_POKER_STAYS = '1'; P2P_POKER_START_STACK = '$StartStack' }
    if (`$l.OfflineFor -gt 0) { `$knobs['P2P_POKER_OFFLINE_AT'] = "`$(`$l.OfflineAt)"; `$knobs['P2P_POKER_OFFLINE_FOR'] = "`$(`$l.OfflineFor)" }
    if (`$l.CancelAt -gt 0) { `$knobs['P2P_POKER_CANCEL_SEARCH_AT'] = "`$(`$l.CancelAt)"; `$knobs['P2P_POKER_SEARCH_AGAIN_AT'] = "`$(`$l.AgainAt)" }
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
        $seedFile = Join-Path $seatKeys "search-here-n$n.key"
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
        $a = @('--headless', '--autoplay', '--no-mdns', '--for', "$for", '--profile', $p, '--search', "$($l.Format)", '--search-tables', "$($l.Tables)")
        if ($Again) { $a += '--search-again' }
        if ($l.Resume) { $a += '--resume' }
        $knobs = @{ P2P_POKER_STAYS = '1'; P2P_POKER_START_STACK = "$StartStack" }
        if ($l.OfflineFor -gt 0) { $knobs['P2P_POKER_OFFLINE_AT'] = "$($l.OfflineAt)"; $knobs['P2P_POKER_OFFLINE_FOR'] = "$($l.OfflineFor)" }
        if ($l.CancelAt -gt 0) { $knobs['P2P_POKER_CANCEL_SEARCH_AT'] = "$($l.CancelAt)"; $knobs['P2P_POKER_SEARCH_AGAIN_AT'] = "$($l.AgainAt)" }
        $jobs += Start-Job -ArgumentList $Exe, $a, $log, $knobs, $t0 -ScriptBlock {
            param($exe, $a, $log, $knobs, $t0)
            foreach ($k in $knobs.Keys) { Set-Item -Path "env:$k" -Value $knobs[$k] }
            $inv = [System.Globalization.CultureInfo]::InvariantCulture
            & $exe @a 2>&1 | ForEach-Object {
                $now = (Get-Date).ToUniversalTime()
                $now.ToString('HH:mm:ss.fff', $inv) + ' ' + (($now - $t0).TotalSeconds.ToString('F1', $inv)).PadLeft(7) + '  ' + $_
            } | Out-File -FilePath $log -Encoding utf8
        }
        Write-Host ("    {0,5:F0} s  n{1}.{2} started ({3}, x{4})" -f ((Get-Date).ToUniversalTime() - $t0).TotalSeconds, $l.Node, $l.Life, $l.Format, $l.Tables)
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
    Write-Host "read it with: python tools\fold-search.py $work"
}
finally {
    if ($privKey) { Remove-Item -Force $privKey -ErrorAction SilentlyContinue }
}
