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
    [bool]$NoMdns = $true
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
$acl = Get-Acl $privKey
$acl.SetAccessRuleProtection($true, $false)
$acl.Access | ForEach-Object { [void]$acl.RemoveAccessRule($_) }
$me = [Security.Principal.WindowsIdentity]::GetCurrent().Name
$acl.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule($me, 'FullControl', 'Allow')))
Set-Acl -Path $privKey -AclObject $acl

$ssh = @('-i', $privKey, '-o', 'StrictHostKeyChecking=accept-new', '-o', 'BatchMode=yes')

try {
    Write-Host '==> copying the binary to the far end'
    & ssh @ssh $Target "if not exist $FarDir mkdir $FarDir" | Out-Null
    & scp @ssh $Exe "$($Target):$($FarDir -replace '\\','/')/p2p-poker.exe" | Out-Null

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
    `$jobs += Start-Job -ArgumentList `$p, `$log -ScriptBlock {
        param(`$p, `$log)
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

    $far = Start-Job -ArgumentList $privKey, $Target, $FarDir -ScriptBlock {
        param($privKey, $Target, $FarDir)
        & ssh -i $privKey -o StrictHostKeyChecking=accept-new -o BatchMode=yes $Target `
            "powershell -NoProfile -ExecutionPolicy Bypass -File $FarDir\far.ps1" 2>&1
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
