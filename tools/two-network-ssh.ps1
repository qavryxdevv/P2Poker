<#
.SYNOPSIS
    Runs one p2p-poker node here and one on a machine reached over SSH, and
    reports what actually crossed between them.

.DESCRIPTION
    This is the test `two-network-test.ps1` says it cannot be. That one puts the
    second node in a Hyper-V VM, and its own notes are explicit: both endpoints
    share one external address, so nothing needs to punch a hole and nothing
    about NAT is proven. Here the far end is a machine on somebody else's
    subnet, reached over SSH, and that is the only shape in which the DHT, the
    relay reservation and DCUtR are exercised at all.

    WHAT IT PROVES, if it completes:
      * Two nodes with no LAN in common. mDNS cannot be what finds them, so
        discovery is the public DHT lobby or nothing.
      * A dial across the boundary completes a libp2p handshake, or a relay
        circuit carries one that cannot.
      * A signed advert crosses GossipSub and is admitted at the far end under
        the section 7.2 rules.
      * With three nodes (two here, one there), whether an event that must
        travel THROUGH a peer arrives — which is the forwarding property no
        in-process harness can see, because a harness delivers to everybody.

    WHAT IT DOES NOT PROVE:
      * Anything, if the far end is reached over a VPN that also carries the
        traffic. A tunnel between the two machines is one more network with no
        NAT in the middle. Check `-Route` in the output: if the far address is
        reached through a tun/tap adapter, say so when reporting the result.

.NOTES
    Reachability is checked FIRST and the script stops there if the far end is
    not up, because an SSH that hangs for two minutes and then fails reads as a
    broken test rather than a closed tunnel.

    The far end needs the binary. If it runs Windows this copies the local
    `p2p-poker.exe`. If it runs Linux it needs a Linux build, which this machine
    cannot produce — no Linux target and no cross-linker are installed — so the
    script says what to do rather than pretending.
#>

[CmdletBinding()]
param(
    [string] $Target  = 'user@172.16.0.20',
    [string] $KeyPath = 'X:\keys\far-machine-key',
    [int]    $Seconds = 240,
    [string] $Binary  = "$PSScriptRoot\..\target\release\p2p-poker.exe",
    [string] $TableName = 'TwoNet'
)

$ErrorActionPreference = 'Stop'

function Fail($msg) { Write-Host "FAIL  $msg" -ForegroundColor Red; exit 1 }
function Note($msg) { Write-Host "      $msg" -ForegroundColor DarkGray }
function Step($msg) { Write-Host "==>   $msg" -ForegroundColor Cyan }
function Warn($msg) { Write-Host "WARN  $msg" -ForegroundColor Yellow }

$hostPart = ($Target -split '@')[-1]

# --- is it even reachable, and how ------------------------------------------
Step "checking the far end is up before spending two minutes on SSH"
if (-not (Test-Path $KeyPath)) {
    Fail "no key at $KeyPath. It lives on the USB volume; plug it in or pass -KeyPath."
}

$pinged = Test-Connection -ComputerName $hostPart -Count 2 -Quiet -ErrorAction SilentlyContinue
$route  = Find-NetRoute -RemoteIPAddress $hostPart -ErrorAction SilentlyContinue | Select-Object -First 1
$iface  = if ($route) { (Get-NetAdapter -InterfaceIndex $route.InterfaceIndex -ErrorAction SilentlyContinue).Name } else { $null }

Note "ping: $pinged"
Note "route: $(if ($iface) { $iface } else { 'none' })"

if (-not $pinged) {
    $vpn = Get-NetAdapter | Where-Object {
        $_.InterfaceDescription -match 'OpenVPN|TAP|Wintun|WireGuard'
    }
    $down = $vpn | Where-Object { $_.Status -ne 'Up' } | Select-Object -ExpandProperty Name
    if ($down) {
        Fail @"
$hostPart does not answer, and these tunnel adapters are not connected:
  $($down -join ', ')
That address is a private range on the far side of one of them. Bring the
tunnel up and run this again — nothing else here can reach it.
"@
    }
    Fail "$hostPart does not answer and no tunnel adapter explains it. Check the far machine is on."
}

if ($iface -match 'OpenVPN|TAP|Wintun|WireGuard') {
    Warn "the far end is reached THROUGH $iface. A tunnel is one more network with no NAT in the middle, so a success here does not prove NAT traversal — say so when reporting the result."
}

# --- what is over there ------------------------------------------------------
Step "asking the far end what it is"
$ssh = @('-i', $KeyPath, '-o', 'StrictHostKeyChecking=accept-new', '-o', 'ConnectTimeout=10', '-o', 'BatchMode=yes', $Target)
$uname = & ssh @ssh 'uname -s -m 2>/dev/null || ver' 2>&1
if ($LASTEXITCODE -ne 0) {
    Fail "ssh to $Target failed: $uname"
}
Note "far end: $uname"

$isWindows = $uname -match 'Windows|Microsoft'
$remoteDir = if ($isWindows) { 'C:\p2p-poker-test' } else { '~/p2p-poker-test' }

# --- the binary --------------------------------------------------------------
if ($isWindows) {
    if (-not (Test-Path $Binary)) { Fail "no binary at $Binary. cargo build --release" }
    Step "copying the Windows binary over"
    & ssh @ssh "mkdir -p '$remoteDir' 2>/dev/null || md `"$remoteDir`"" | Out-Null
    & scp -i $KeyPath -o StrictHostKeyChecking=accept-new $Binary "${Target}:$remoteDir/p2p-poker.exe"
    if ($LASTEXITCODE -ne 0) { Fail "scp failed" }
    $remoteExe = "$remoteDir/p2p-poker.exe"
} else {
    Step "checking the far end can build for itself"
    $cargo = & ssh @ssh 'command -v cargo || true' 2>&1
    if (-not $cargo) {
        Fail @"
the far end runs $uname and has no cargo, and THIS machine cannot build for it:
`rustup target list --installed` holds only x86_64-pc-windows-msvc and there is
no cross-linker (no cc, clang or zig). One of these, and the script says which
rather than guessing:
  * install Rust on the far end and re-run, or
  * install a Linux target and linker here, or
  * build in WSL — which is also not installed on this machine.
"@
    }
    Note "cargo: $cargo — building there from a copy of the tree"
    & ssh @ssh "mkdir -p '$remoteDir'" | Out-Null
    Fail "copying the source tree is not implemented: it needs a decision about what to send (the tree is large and vendored). Do it once by hand, then re-run with -Binary pointing at the far end's build."
}

# --- run both ends at once ---------------------------------------------------
Step "running both ends for $Seconds s"
$localLog  = Join-Path $env:TEMP 'twonet-local.log'
$remoteLog = "$remoteDir/twonet-remote.log"

$remoteCmd = "cd '$remoteDir' && ./p2p-poker.exe --headless --profile ./profile --join $TableName --for $Seconds > '$remoteLog' 2>&1"
$far = Start-Job -ScriptBlock {
    param($ssh, $cmd)
    & ssh @ssh $cmd 2>&1
} -ArgumentList (,$ssh), $remoteCmd

# This end hosts the table, so the far end has something to look for.
$here = Start-Process -FilePath $Binary -PassThru -NoNewWindow -RedirectStandardOutput $localLog `
    -ArgumentList @('--headless', '--profile', "$env:TEMP\twonet-profile", '--host', $TableName,
                    '--seats', '2', '--min', '2', '--for', "$Seconds")

Wait-Process -Id $here.Id -Timeout ($Seconds + 60) -ErrorAction SilentlyContinue
Receive-Job $far -Wait -AutoRemoveJob | Out-Null

# --- what actually happened --------------------------------------------------
Step "reading both logs"
$remoteText = & ssh @ssh "cat '$remoteLog'" 2>&1
$localText  = Get-Content $localLog -ErrorAction SilentlyContinue

function Count($text, $pattern) { ($text | Select-String -Pattern $pattern -AllMatches).Count }

Write-Host ""
Write-Host "  here  : peers $(Count $localText 'another poker client'), table set $(Count $localText 'the table is set'), hands $(Count $localText 'opens at genesis')"
Write-Host "  there : peers $(Count $remoteText 'another poker client'), saw table $(Count $remoteText 'table [0-9a-f]'), hands $(Count $remoteText 'opens at genesis')"
Write-Host ""
Write-Host "  relay reservations here : $(Count $localText 'reservation')"
Write-Host "  relay reservations there: $(Count $remoteText 'reservation')"
Write-Host "  direct connections there: $(Count $remoteText 'is now a direct connection')"
Write-Host ""

if ((Count $remoteText 'the table is set') -gt 0) {
    Write-Host "RESULT  a table formed across the boundary" -ForegroundColor Green
} elseif ((Count $remoteText 'table [0-9a-f]') -gt 0) {
    Write-Host "RESULT  the advert crossed but the table did not form — the join path is where to look" -ForegroundColor Yellow
} elseif ((Count $remoteText 'another poker client') -gt 0) {
    Write-Host "RESULT  the two met and no advert crossed — GossipSub, not discovery" -ForegroundColor Yellow
} else {
    Write-Host "RESULT  they never met. Discovery across the boundary is the whole finding." -ForegroundColor Red
}

Note "logs: $localLog and ${Target}:$remoteLog"
