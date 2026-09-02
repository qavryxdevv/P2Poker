<#
.SYNOPSIS
    One table, its seats split across two machines, and what every seat saw.

.DESCRIPTION
    `table-run.ps1` puts every client on one box, and at ten seats that box is
    the suspect: ten toxcore instances all running their own DHT and LAN
    discovery is a plausible limit that has nothing to do with the protocol. Ten
    seats failed the same way on two independent machines, which rules out *this
    machine* and does not rule out *ten instances*.

    This is the experiment that separates them. Five seats here and five on the
    far end is five instances per machine — well inside what both machines play
    at six, seven, eight and nine — so a ten-seat table that still fails is a
    ten-seat problem, and one that works was an instances-per-box problem.

    The far end needs the Windows binary and this script; both are copied.

.PARAMETER Here
    Seats started on this machine, the founder among them.

.PARAMETER There
    Seats started on the far machine. `Here + There` is the table's size.

.NOTES
    **Discovery across the boundary is the DHT lobby, not mDNS.** The two
    machines are on different subnets and multicast does not route, so the far
    seats find the table exactly as a stranger on the internet would. That is
    a second thing under test at the same time, and the per-node report says
    which of the two failed: a seat that never reached `N seated` never found
    the table, and a seat that reached it and never entered the group found it
    and could not join the Tox side.
#>
[CmdletBinding()]
param(
    [ValidateRange(1, 9)][int]$Here = 5,
    [ValidateRange(1, 9)][int]$There = 5,
    [ValidateRange(60, 3600)][int]$Seconds = 420,
    [string]$Target = 'user@172.16.0.20',
    [string]$KeyPath = 'X:\keys\far-machine-key',
    [string]$Exe,
    [string]$FarDir = 'C:\p2ptest',
    # **On by default, because it is the shape that works and the honest one.**
    # Two machines on different subnets cannot find each other by multicast, so
    # the far seats use the public DHT lobby whatever this says. Leaving mDNS on
    # for the local seats makes them a clique that finds itself in a second
    # while the far ones come the long way, and the first run of this harness
    # showed the cost: the far seat reached the lobby, found 34 players in it,
    # and never saw our table. With it off every seat finds every other by the
    # one road, which is what `two-network-ssh.ps1` proved carries a table.
    [bool]$NoMdns = $true,
    # `-Stall <seconds> -StallSeat <n>` starves one far seat's **group
    # handshake** for that long, right after it accepts the invitation.
    #
    # `S1-AA` shape (i) on demand, and the far end is where it belongs: the
    # failure turns on the founder having no TCP relays attached to the group
    # connection and on `copy_friend_ip_port_to_gconn` finding no DHT `IP:port`,
    # and a friendship that crosses subnets is TCP-relayed and has neither.
    # Anything above `GC_UNCONFIRMED_PEER_TIMEOUT` = 12 s reproduces it.
    #
    # Needs binaries built with `--features fault-harness` at both ends.
    [ValidateRange(0, 300)][int]$Stall = 0,
    [ValidateRange(0, 8)][int]$StallSeat = 0
)

$ErrorActionPreference = 'Stop'
$inv = [System.Globalization.CultureInfo]::InvariantCulture

if (-not $Exe) {
    $Exe = Join-Path (Split-Path -Parent $PSScriptRoot) 'target\release\p2p-poker.exe'
}
if (-not (Test-Path $Exe)) { throw "no binary at $Exe. cargo build --release" }
$Exe = (Resolve-Path $Exe).Path
if (-not (Test-Path $KeyPath)) { throw "no key at $KeyPath. Plug the USB volume in or pass -KeyPath." }

$seats = $Here + $There
$stamp = (Get-Date).ToString('HHmmss')
$table = "split$stamp-$seats"
$work = Join-Path $env:TEMP "p2p-table-split\$table"
New-Item -ItemType Directory -Force -Path $work | Out-Null

Write-Host "table  $table"
Write-Host "seats  $seats  ($Here here, $There on $Target)"
Write-Host "for    $Seconds s"
Write-Host "mdns   $(if ($NoMdns) { 'off - every seat finds every other through the public lobby' } else { 'on' })"
Write-Host "work   $work"
Write-Host ''

# Windows OpenSSH refuses a key on removable media and reports it as
# `Permission denied (publickey)`, so the run uses a copy this account alone can
# read, removed in the `finally`.
$privKey = Join-Path $env:TEMP ('split-' + [IO.Path]::GetFileName($KeyPath))
Copy-Item -Path $KeyPath -Destination $privKey -Force

# **`icacls`, not `Set-Acl`, and that is not a style preference.** `Set-Acl`
# writes back the whole security descriptor it was handed by `Get-Acl` --
# including the audit portion -- so Windows demands `SeSecurityPrivilege` for
# it, which an ordinary shell does not hold. It fails with *the process does not
# have the SeSecurityPrivilege required for this operation*, which reads like a
# key problem and is not one; the run dies before a single node starts.
# `icacls` sets only the DACL and needs no such privilege.
#
# `/inheritance:r` drops the inherited rules that make the file group- and
# world-readable, which is the whole reason for the copy: Windows OpenSSH
# refuses a key anyone else can read and reports it as
# `Permission denied (publickey)`.
$me = [Security.Principal.WindowsIdentity]::GetCurrent().Name
& icacls "$privKey" /inheritance:r /grant:r "${me}:(F)" | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw "could not lock the permissions on the key copy at $privKey (icacls exited $LASTEXITCODE). ssh will refuse a key other accounts can read."
}

# **`ssh` must be told which interface to leave by, and finding that out took a
# session.** A machine with a Hyper-V switch and a handful of `169.254.*`
# addresses lets `ssh` choose wrongly and report `Connection timed out` while a
# raw TCP socket to the same port connects at once — which reads as *the far
# machine is down* and is not. `Test-NetConnection` answers both questions: it
# says whether port 22 is open, and it says which source address the OS would
# use to reach it, which is exactly what `-b` wants. Detected rather than
# configured, so this does not carry one machine's address as a constant.
#
# `PingSucceeded` is false either way because ICMP is blocked; read
# `TcpTestSucceeded`.
$targetHost = ($Target -split '@')[-1]
$probe = Test-NetConnection -ComputerName $targetHost -Port 22 -WarningAction SilentlyContinue
if (-not $probe.TcpTestSucceeded) {
    throw "nothing is listening on $targetHost port 22. The far machine is off, or the key is on a drive that is not plugged in."
}
$bind = $probe.SourceAddress.IPAddress

# `UserKnownHostsFile=NUL` because otherwise `ssh` tries to write the file and
# fails with *the system cannot find the path specified*, which also reads as a
# routing failure. The far end is on a private network and is pinned by the key.
#
# **`-o BindAddress=` and not `-b`, because this list is handed to `scp` too.**
# `ssh` takes `-b`; `scp` does not — its `-b` is `sftp`'s batch-file option and
# it exits with *unknown option -- b* before doing anything. That killed every
# copy in a run while the local founder started normally and played alone, so
# the report looked like a far machine that never joined rather than a harness
# that never sent it the binary. Both programs accept the `-o` form.
$ssh = @('-i', $privKey,
         '-o', "BindAddress=$bind",
         '-o', 'StrictHostKeyChecking=accept-new',
         '-o', 'UserKnownHostsFile=NUL',
         '-o', 'BatchMode=yes')

try {
    Write-Host '==> copying the binary to the far end'
    & ssh @ssh $Target "if not exist $FarDir mkdir $FarDir" | Out-Null
    # **A copy that fails must stop the run.** It used to be piped to
    # `Out-Null` and its exit code ignored, so a broken `scp` invocation left
    # the far end with no binary while the local founder started normally and
    # played by itself. The report then read as *the far machine never joined*,
    # which is a protocol conclusion, drawn from a harness fault. One run was
    # spent on it.
    & scp @ssh $Exe "$($Target):$($FarDir -replace '\\','/')/p2p-poker.exe" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "could not copy the binary to $Target (scp exited $LASTEXITCODE). The far seats would have had nothing to run." }

    # --- the far seats, started first so they are looking before the table is
    #     hosted. They join by name, and a name they have not heard of yet is
    #     simply a name they keep waiting for.
    Write-Host "==> starting $There seat(s) on the far end"
    $farScript = @"
`$jobs = @()
for (`$i = 0; `$i -lt $There; `$i++) {
    `$p = "$FarDir\split-n`$i"
    if (Test-Path `$p) { Remove-Item -Recurse -Force `$p }
    New-Item -ItemType Directory -Force -Path `$p | Out-Null
    `$log = "$FarDir\split-n`$i.log"
    `$jobs += Start-Job -ArgumentList `$p, `$log, `$i -ScriptBlock {
        param(`$p, `$log, `$seat)
        if ($Stall -gt 0 -and `$seat -eq $StallSeat) { `$env:P2P_POKER_STALL_JOIN = '$Stall' }
        `$start = Get-Date
        `$inv = [System.Globalization.CultureInfo]::InvariantCulture
        & "$FarDir\p2p-poker.exe" --headless $(if ($NoMdns) { '--no-mdns' }) --autoplay --for $Seconds --profile `$p --join $table 2>&1 |
            ForEach-Object { ((((Get-Date) - `$start).TotalSeconds).ToString('F1', `$inv)).PadLeft(7) + '  ' + `$_ } |
            Out-File -FilePath `$log -Encoding utf8
    }
    Start-Sleep -Milliseconds 400
}
`$null = Wait-Job -Job `$jobs -Timeout ($Seconds + 60)
`$jobs | Where-Object { `$_.State -eq 'Running' } | Stop-Job
`$jobs | Remove-Job -Force
'far end done'
"@
    $farFile = Join-Path $work 'far.ps1'
    Set-Content -Path $farFile -Value $farScript -Encoding UTF8
    & scp @ssh $farFile "$($Target):$($FarDir -replace '\\','/')/far.ps1" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "could not copy the far-end script to $Target (scp exited $LASTEXITCODE)." }

    $far = Start-Job -ArgumentList $ssh, $Target, $FarDir -ScriptBlock {
        param($ssh, $Target, $FarDir)
        & ssh @ssh $Target "powershell -NoProfile -ExecutionPolicy Bypass -File $FarDir\far.ps1" 2>&1
    }

    # --- the local seats, founder first ------------------------------------
    Write-Host "==> starting $Here seat(s) here, founder first"
    $jobs = @()
    for ($i = 0; $i -lt $Here; $i++) {
        $profileDir = Join-Path $work "n$i"
        New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
        $log = Join-Path $work "n$i.log"
        $nodeArgs = @('--headless', '--autoplay', '--for', "$Seconds", '--profile', $profileDir)
        if ($NoMdns) { $nodeArgs += '--no-mdns' }
        if ($i -eq 0) { $nodeArgs += @('--host', $table, '--seats', "$seats") }
        else { $nodeArgs += @('--join', $table) }

        $jobs += Start-Job -Name "n$i" -ArgumentList $Exe, $nodeArgs, $log -ScriptBlock {
            param($exe, $nodeArgs, $log)
            $start = Get-Date
            $inv = [System.Globalization.CultureInfo]::InvariantCulture
            & $exe @nodeArgs 2>&1 | ForEach-Object {
                ((((Get-Date) - $start).TotalSeconds).ToString('F1', $inv)).PadLeft(7) + '  ' + $_
            } | Out-File -FilePath $log -Encoding utf8
        }
        if ($i -eq 0) { Start-Sleep -Seconds 3 } else { Start-Sleep -Milliseconds 400 }
    }

    Write-Host "==> waiting up to $($Seconds + 90) s"
    $null = Wait-Job -Job ($jobs + $far) -Timeout ($Seconds + 90)
    ($jobs + $far) | Where-Object { $_.State -eq 'Running' } | Stop-Job
    ($jobs + $far) | Remove-Job -Force

    Write-Host '==> collecting the far logs'
    for ($i = 0; $i -lt $There; $i++) {
        & scp @ssh "$($Target):$($FarDir -replace '\\','/')/split-n$i.log" (Join-Path $work "far-n$i.log") 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { Write-Warning "far seat $i left no log to collect (scp exited $LASTEXITCODE); its column below is empty because nothing was read, not because nothing happened" }
    }

    # --- read it back -------------------------------------------------------
    function Read-Node($path, $name) {
        if (-not (Test-Path $path)) {
            return [pscustomobject]@{ Node = $name; Seated = 0; Group = $null; Opens = 0; Overs = 0 }
        }
        $lines = Get-Content $path
        $seated = 0
        $group = $null
        $opens = 0
        $overs = 0
        foreach ($l in $lines) {
            if ($l -match '([0-9]+) seated') {
                $n = [int]$Matches[1]
                if ($n -gt $seated) { $seated = $n }
            }
            if ($null -eq $group -and $l -match "^\s*([0-9.]+)\s+in the table's Tox group") {
                $group = [double]::Parse($Matches[1], [System.Globalization.CultureInfo]::InvariantCulture)
            }
            if ($l -match 'opens at genesis') { $opens++ }
            if ($l -match 'hand #\d+ is over') { $overs++ }
        }
        [pscustomobject]@{ Node = $name; Seated = $seated; Group = $group; Opens = $opens; Overs = $overs }
    }

    $nodes = @()
    for ($i = 0; $i -lt $Here; $i++) { $nodes += Read-Node (Join-Path $work "n$i.log") "here-n$i" }
    for ($i = 0; $i -lt $There; $i++) { $nodes += Read-Node (Join-Path $work "far-n$i.log") "far-n$i" }

    Write-Host ''
    Write-Host '--- per node ---'
    Write-Host ('{0,-9} {1,7} {2,10} {3,7} {4,9}' -f 'node', 'seated', 'in group', 'opened', 'finished')
    foreach ($n in $nodes) {
        $g = if ($null -eq $n.Group) { 'never' } else { $n.Group.ToString('F1', $inv) + ' s' }
        Write-Host ('{0,-9} {1,7} {2,10} {3,7} {4,9}' -f $n.Node, $n.Seated, $g, $n.Opens, $n.Overs)
    }

    $full = @($nodes | Where-Object { $_.Seated -ge $seats }).Count
    $inGroup = @($nodes | Where-Object { $null -ne $_.Group }).Count
    $played = @($nodes | Where-Object { $_.Overs -gt 0 }).Count
    Write-Host ''
    Write-Host "seats that saw the whole roster : $full of $seats"
    Write-Host "seats that entered the group    : $inGroup of $seats"
    Write-Host "seats that finished a hand      : $played of $seats"
    Write-Host ''
    Write-Host "logs: $work"
}
finally {
    Remove-Item -Force $privKey -ErrorAction SilentlyContinue
}
