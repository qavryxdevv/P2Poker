# A table across two machines on two subnets.
#
# `table-run.ps1` starts every seat on one box, which is why every failure this
# project has measured is a LAN failure — including `S1-AA` shape (i), whose
# diagnosis turns on the founder having no TCP relays attached and the friend's
# DHT entry never being populated. Neither is necessarily true when the peers
# are not on one wire, and that is what this measures.
#
#   founder  192.168.1.20  (this machine)
#   joiner   172.16.0.20   (far-machine, over ssh)
#
# Usage:  twomachine.ps1 [-Stall <seconds>] [-Seconds <n>]

param(
    [int]$Stall = 0,
    [int]$Seconds = 240
)

$ErrorActionPreference = 'Continue'

$repo   = 'X:\src\p2p-poker'
$key    = Join-Path $env:TEMP 'id_second'
$ssh    = "$env:SystemRoot\System32\OpenSSH\ssh.exe"
$sshOpt = @('-b','192.168.1.20','-i',$key,'-o','StrictHostKeyChecking=no',
            '-o','UserKnownHostsFile=NUL','-o','ConnectTimeout=20','-o','BatchMode=yes',
            'user@172.16.0.20')

$stamp = (Get-Date).ToString('HHmmss')
$table = "two$stamp"
$work  = Join-Path $env:TEMP "p2p-two\$table"
New-Item -ItemType Directory -Force -Path $work | Out-Null

$hostProfile = Join-Path $work 'host'
New-Item -ItemType Directory -Force -Path $hostProfile | Out-Null
# Seed the founder's node list from the shared cache, as table-run.ps1 does, so
# the run is not also a measurement of nodes.tox.chat.
$shared = Join-Path $env:TEMP 'p2p-table-run\tox-nodes.json'
if (Test-Path $shared) { Copy-Item $shared (Join-Path $hostProfile 'tox-nodes.json') -Force }

Write-Host "table   $table"
Write-Host "seats   2 (one per machine)"
Write-Host "for     $Seconds s"
if ($Stall -gt 0) { Write-Host "stall   the joiner starves its group handshake for $Stall s" }
Write-Host "work    $work"
Write-Host ""

# --- the founder, here ------------------------------------------------------
$hostLog = Join-Path $work 'founder.log'
$hostJob = Start-Job -Name 'founder' -ArgumentList $repo, $table, $Seconds, $hostProfile, $hostLog -ScriptBlock {
    param($repo, $table, $secs, $prof, $log)
    $exe = Join-Path $repo 'target\release\p2p-poker.exe'
    $start = Get-Date
    $inv = [System.Globalization.CultureInfo]::InvariantCulture
    & $exe --headless --autoplay --for "$secs" --profile $prof --host $table --seats 2 2>&1 |
        ForEach-Object {
            $t = ((Get-Date) - $start).TotalSeconds
            ($t.ToString('F1', $inv)).PadLeft(7) + '  ' + $_
        } | Out-File -FilePath $log -Encoding utf8
}

Start-Sleep -Seconds 8

# --- the joiner, there ------------------------------------------------------
# A fresh profile per run, so a stale group membership from a previous run
# cannot be mistaken for this one succeeding.
$remoteDir = "C:/p2p-poker-test/run-$table"
$env:P2P_TWO_TABLE = $table
$remote = @"
`$ErrorActionPreference='Continue'
New-Item -ItemType Directory -Force -Path '$remoteDir' | Out-Null
Copy-Item 'C:/p2p-poker-test/profile/tox-nodes.json' '$remoteDir/tox-nodes.json' -Force -ErrorAction SilentlyContinue
if ($Stall -gt 0) { `$env:P2P_POKER_STALL_JOIN = '$Stall' }
`$start = Get-Date
`$inv = [System.Globalization.CultureInfo]::InvariantCulture
& 'C:/p2p-poker-test/p2p-poker-new.exe' --headless --autoplay --for '$Seconds' --profile '$remoteDir' --join '$table' 2>&1 |
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

Write-Host ""
Write-Host "--- founder ---"
if (Test-Path $hostLog) {
    Get-Content $hostLog | Select-String 'seats on the line|opens at genesis|TABLE FORMED|NO TABLE' | Select-Object -Last 6
}
Write-Host ""
Write-Host "--- joiner ---"
if (Test-Path $joinLog) {
    Get-Content $joinLog | Select-String 'seats on the line|opens at genesis|TABLE FORMED|NO TABLE|toxcore\[' | Select-Object -Last 10
}
Write-Host ""
Write-Host "logs: $work"
