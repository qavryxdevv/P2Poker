<#
.SYNOPSIS
    One table across two machines on two subnets: a founder here, a joiner there.

.DESCRIPTION
    **`table-run.ps1` starts every seat on one box, so every failure it has ever
    measured is a LAN failure.** That matters more than it sounds. `S1-AA` shape
    (i) — a seat that holds a group number and never becomes a member — was
    diagnosed to `handle_gc_invite_confirmed_packet` returning `-5` because
    *both* of its sources of connection info were empty: the founder had no TCP
    relays attached to that group connection, and `friend_conn_get_dht_ip_port`
    was unset because friendships on one wire come up through LAN discovery and
    need never populate a DHT entry. Neither is necessarily true off the LAN, and
    a single-machine harness cannot ask.

    Measured on the first cross-network run, 2026-09-02: the table forms, the
    joiner finds it over the lobby, both peers open hand 1 at the same genesis,
    and seven hands played with the boundary checkpoint agreeing at each.
    **Entering the Tox group took about ninety seconds against ten to twenty on
    the LAN** — and `GC_UNCONFIRMED_PEER_TIMEOUT` is twelve, so a handshake that
    crosses networks has far less margin, not more.

    # Reaching the second machine takes three corrections

    None is obvious and each looks like a different fault:

    * **`-b <local address>`** — a machine with a Hyper-V switch and a handful of
      `169.254.*` addresses lets `ssh` pick the wrong source and report
      `Connection timed out` while a raw TCP socket to port 22 connects at once.
      `Test-NetConnection -Port 22` tells the two apart; ignore `PingSucceeded`,
      which is false either way because ICMP is blocked.
    * **A key with tight permissions.** A key on a FAT USB volume is world
      readable and `ssh` refuses it. Copy it and
      `icacls <file> /inheritance:r` then `/grant:r "$env:USERNAME:(R)"`.
    * **`-o UserKnownHostsFile=NUL`** — without it `ssh` fails with *the system
      cannot find the path specified*, which also reads as a routing failure.

    Git Bash's `ssh` did not work even with all three; `System32\OpenSSH\ssh.exe`
    did.

.PARAMETER Remote
    `user@host` of the second machine.

.PARAMETER LocalAddress
    The address `ssh` must bind, and the one this machine is reachable at.

.PARAMETER Key
    Private key for the second machine. Copied to a locked-down temporary file
    before use, because a key on a USB volume has permissions `ssh` refuses.

.PARAMETER RemoteExe
    The client on the second machine. It is **not** built there — there is no
    cargo — so copy one over first:
    `scp target\release\p2p-poker.exe user@host:C:/p2p-poker-test/p2p-poker.exe`

.PARAMETER Stall
    Seconds the joiner starves its own group handshake, right after accepting
    the invitation. Above `GC_UNCONFIRMED_PEER_TIMEOUT` = 12 this reproduces
    `S1-AA` shape (i). Needs a binary built with `--features fault-harness` at
    **both** ends.

.EXAMPLE
    powershell -File tools\two-machine-run.ps1
    powershell -File tools\two-machine-run.ps1 -Stall 15 -Seconds 240
#>
[CmdletBinding()]
param(
    [string]$Remote = 'user@172.16.0.20',
    [string]$LocalAddress = '192.168.1.20',
    [string]$Key = 'X:\keys\far-machine-key',
    [string]$RemoteDir = 'C:/p2p-poker-test',
    [string]$RemoteExe = 'C:/p2p-poker-test/p2p-poker-new.exe',
    [string]$Exe,
    [ValidateRange(0, 300)][int]$Stall = 0,
    [ValidateRange(60, 3600)][int]$Seconds = 240
)

$ErrorActionPreference = 'Continue'

if (-not $Exe) {
    $repo = Split-Path -Parent $PSScriptRoot
    $Exe = Join-Path $repo 'target\release\p2p-poker.exe'
}
if (-not (Test-Path $Exe)) { throw "no binary at $Exe. Build one: cargo build --release" }
if (-not (Test-Path $Key)) { throw "no key at $Key. The USB drive may not be plugged in." }

# A copy with permissions `ssh` will accept. The original is usually on a FAT
# volume, where every file is world readable.
$keyCopy = Join-Path $env:TEMP 'two-machine-run.key'
Copy-Item $Key $keyCopy -Force
icacls $keyCopy /inheritance:r  | Out-Null
icacls $keyCopy /grant:r "$($env:USERNAME):(R)" | Out-Null

$ssh    = "$env:SystemRoot\System32\OpenSSH\ssh.exe"
$sshOpt = @('-b', $LocalAddress, '-i', $keyCopy,
            '-o', 'StrictHostKeyChecking=no',
            '-o', 'UserKnownHostsFile=NUL',
            '-o', 'ConnectTimeout=20',
            '-o', 'BatchMode=yes',
            $Remote)

$stamp = (Get-Date).ToString('HHmmss')
$table = "two$stamp"
$work  = Join-Path $env:TEMP "p2p-two\$table"
New-Item -ItemType Directory -Force -Path $work | Out-Null

$hostProfile = Join-Path $work 'host'
New-Item -ItemType Directory -Force -Path $hostProfile | Out-Null
# Seed the node list from the shared cache, as `table-run.ps1` does, so the run
# is not also a measurement of `nodes.tox.chat`.
$shared = Join-Path $env:TEMP 'p2p-table-run\tox-nodes.json'
if (Test-Path $shared) { Copy-Item $shared (Join-Path $hostProfile 'tox-nodes.json') -Force }

Write-Host "table   $table"
Write-Host "seats   2, one per machine ($LocalAddress and $Remote)"
Write-Host "for     $Seconds s"
if ($Stall -gt 0) {
    Write-Host "stall   the joiner starves its group handshake for $Stall s (S1-AA shape (i))"
}
Write-Host "work    $work"
Write-Host ""

$hostLog = Join-Path $work 'founder.log'
$hostJob = Start-Job -Name 'founder' -ArgumentList $Exe, $table, $Seconds, $hostProfile, $hostLog -ScriptBlock {
    param($exe, $table, $secs, $prof, $log)
    $start = Get-Date
    $inv = [System.Globalization.CultureInfo]::InvariantCulture
    & $exe --headless --autoplay --for "$secs" --profile $prof --host $table --seats 2 2>&1 |
        ForEach-Object {
            $t = ((Get-Date) - $start).TotalSeconds
            ($t.ToString('F1', $inv)).PadLeft(7) + '  ' + $_
        } | Out-File -FilePath $log -Encoding utf8
}

# The founder must be advertising before the joiner looks.
Start-Sleep -Seconds 8

# A fresh profile per run, so a group membership left by a previous one cannot
# be mistaken for this one succeeding.
$runDir = "$RemoteDir/run-$table"
$remote = @"
`$ErrorActionPreference='Continue'
New-Item -ItemType Directory -Force -Path '$runDir' | Out-Null
Copy-Item '$RemoteDir/profile/tox-nodes.json' '$runDir/tox-nodes.json' -Force -ErrorAction SilentlyContinue
if ($Stall -gt 0) { `$env:P2P_POKER_STALL_JOIN = '$Stall' }
`$start = Get-Date
`$inv = [System.Globalization.CultureInfo]::InvariantCulture
& '$RemoteExe' --headless --autoplay --for '$Seconds' --profile '$runDir' --join '$table' 2>&1 |
  ForEach-Object { `$t = ((Get-Date) - `$start).TotalSeconds; (`$t.ToString('F1', `$inv)).PadLeft(7) + '  ' + `$_ }
"@
$b64 = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($remote))

$joinLog = Join-Path $work 'joiner.log'
$joinJob = Start-Job -Name 'joiner' -ArgumentList $ssh, $sshOpt, $b64, $joinLog -ScriptBlock {
    param($ssh, $sshOpt, $b64, $log)
    & $ssh @sshOpt "powershell -NoProfile -EncodedCommand $b64" 2>&1 | Out-File -FilePath $log -Encoding utf8
}

Write-Host "both started; waiting up to $($Seconds + 90) s"
$null = Wait-Job -Job $hostJob, $joinJob -Timeout ($Seconds + 90)
Stop-Job -Job $hostJob, $joinJob -ErrorAction SilentlyContinue
Receive-Job -Job $hostJob, $joinJob -ErrorAction SilentlyContinue | Out-Null
Remove-Job -Job $hostJob, $joinJob -Force -ErrorAction SilentlyContinue

# **Counted, not eyeballed.** A cross-network run is slow and easy to read
# charitably; the two numbers that matter are how many hands each side opened
# and how many it finished, and they must agree between the two machines.
function Count-Log {
    param([string]$Path)
    if (-not (Test-Path $Path)) { return $null }
    $l = Get-Content $Path
    [pscustomobject]@{
        Opened   = @($l | Select-String 'hand #\d+ opens at genesis').Count
        Finished = @($l | Select-String 'hand #\d+ is over').Count
        Group    = ($l | Select-String 'seats on the line' | Select-Object -Last 1)
        Toxcore  = @($l | Select-String 'toxcore\[').Count
    }
}

$h = Count-Log $hostLog
$j = Count-Log $joinLog
Write-Host ""
'{0,-9} {1,7} {2,9} {3,9}' -f 'side', 'opened', 'finished', 'tox lines'
'{0,-9} {1,7} {2,9} {3,9}' -f 'founder', $h.Opened, $h.Finished, $h.Toxcore
'{0,-9} {1,7} {2,9} {3,9}' -f 'joiner', $j.Opened, $j.Finished, $j.Toxcore
Write-Host ""
if ($h.Group) { Write-Host "founder: $($h.Group)" }
if ($j.Group) { Write-Host "joiner : $($j.Group)" }
Write-Host ""
if ($j.Opened -eq 0) {
    Write-Warning "the joiner never opened a hand - it did not get into the table"
} elseif ($h.Opened -ne $j.Opened) {
    Write-Warning "the two sides opened different numbers of hands ($($h.Opened) and $($j.Opened))"
}
Write-Host "logs: $work"
