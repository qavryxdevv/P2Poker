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

# **TCP, not ICMP.** The first version tested with `Test-Connection` and would
# have refused to run against the very machine this was written for: its
# firewall drops ping and permits port 22, so `ping` said unreachable while ssh
# worked. A reachability test has to probe the port the work goes over.
$probe  = Test-NetConnection -ComputerName $hostPart -Port 22 -WarningAction SilentlyContinue
$open   = $probe.TcpTestSucceeded
$pinged = Test-Connection -ComputerName $hostPart -Count 1 -Quiet -ErrorAction SilentlyContinue
$route  = Find-NetRoute -RemoteIPAddress $hostPart -ErrorAction SilentlyContinue | Select-Object -First 1
$iface  = if ($route) { (Get-NetAdapter -InterfaceIndex $route.InterfaceIndex -ErrorAction SilentlyContinue).Name } else { $null }

Note "tcp/22: $open   (icmp: $pinged - blocked by a firewall is normal and not a failure)"
Note "route: $(if ($iface) { $iface } else { 'none' })"

if (-not $open) {
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
    Fail "$hostPart does not accept TCP/22 and no tunnel adapter explains it. Check the far machine is on and its firewall admits ssh."
}

# A private far address is a routed boundary, not a NAT. This is the caveat most
# likely to be dropped when the result is repeated to somebody else, so it is
# said at the top of the run and not only in the notes: a success here proves
# discovery, relay and forwarding across a real boundary, and says nothing at
# all about hole punching, which needs an endpoint outside this building.
if ($hostPart -match '^(10)\.|^(192)\.(168)\.|^(172)\.(1[6-9]|2[0-9]|3[01])\.|^(169)\.(254)\.') {
    Warn "$hostPart is a private address: this is two subnets with a router between them, which is a real boundary but NOT a NAT to traverse. A success here does not prove hole punching."
}

if ($iface -match 'OpenVPN|TAP|Wintun|WireGuard') {
    Warn "the far end is reached THROUGH $iface. A tunnel is one more network with no NAT in the middle, so a success here does not prove NAT traversal — say so when reporting the result."
}

# --- the key, on terms Windows OpenSSH will accept ---------------------------
# The key lives on removable media, whose permissions are wide open, and
# Windows OpenSSH REFUSES a private key others can read — "bad permissions",
# then "Permission denied (publickey)", which reads like a rejected key rather
# than an unread one. Git's ssh is more forgiving, so this only bites here.
#
# So: a copy in this user's temp, its inheritance broken and its ACL cut to
# this account alone, removed again in the `finally` at the end.
Step "preparing a copy of the key Windows OpenSSH will load"
$privKey = Join-Path $env:TEMP ('twonet-' + [IO.Path]::GetFileName($KeyPath))
Copy-Item -Path $KeyPath -Destination $privKey -Force
$acl = Get-Acl $privKey
$acl.SetAccessRuleProtection($true, $false)
$acl.Access | ForEach-Object { [void]$acl.RemoveAccessRule($_) }
$me = [Security.Principal.WindowsIdentity]::GetCurrent().Name
$rule = New-Object System.Security.AccessControl.FileSystemAccessRule($me, 'FullControl', 'Allow')
$acl.AddAccessRule($rule)
Set-Acl -Path $privKey -AclObject $acl
Note "using $privKey (removed when this finishes)"

try {

# --- what is over there ------------------------------------------------------
Step "asking the far end what it is"
$ssh = @('-i', $privKey, '-o', 'StrictHostKeyChecking=accept-new', '-o', 'ConnectTimeout=10', '-o', 'BatchMode=yes', $Target)
$uname = & ssh @ssh 'uname -s -m 2>/dev/null || ver' 2>&1
if ($LASTEXITCODE -ne 0) {
    Fail "ssh to $Target failed: $uname"
}
Note "far end: $uname"

# Not `$isWindows`: PowerShell 7 owns `$IsWindows` as a read-only automatic
# variable and variable names are case-insensitive, so assigning to it is a
# hard error — and one that fires only on PS7, after the ssh round trip.
$farIsWindows = $uname -match 'Windows|Microsoft'
$remoteDir = if ($farIsWindows) { 'C:\p2p-poker-test' } else { '~/p2p-poker-test' }

# --- the binary --------------------------------------------------------------
if ($farIsWindows) {
    if (-not (Test-Path $Binary)) { Fail "no binary at $Binary. cargo build --release" }
    Step "copying the Windows binary over"
    # cmd's `md` fails when the directory is already there, which is fine.
    & ssh @ssh "md $remoteDir" 2>$null | Out-Null
    & scp -i $privKey -o StrictHostKeyChecking=accept-new $Binary "${Target}:$remoteDir/p2p-poker.exe"
    if ($LASTEXITCODE -ne 0) { Fail "scp failed" }
    $remoteExe = "$remoteDir/p2p-poker.exe"
} else {
    Step "checking the far end can build for itself"
    $cargo = & ssh @ssh 'command -v cargo || true' 2>&1
    if (-not $cargo) {
        $why = "the far end runs $uname and has no cargo, and this machine cannot build for it: "
        $why += "only x86_64-pc-windows-msvc is installed, there is no cross-linker, and WSL has no distro. "
        $why += "Install Rust on the far end, or a Linux target and linker here."
        Fail $why
    }
    Note "cargo: $cargo — building there from a copy of the tree"
    & ssh @ssh "mkdir -p '$remoteDir'" | Out-Null
    Fail "copying the source tree is not implemented: it needs a decision about what to send (the tree is large and vendored). Do it once by hand, then re-run with -Binary pointing at the far end's build."
}

# --- run both ends at once ---------------------------------------------------
Step "running both ends for $Seconds s"
$localLog       = Join-Path $env:TEMP 'twonet-local.log'
$remoteLogLocal = Join-Path $env:TEMP 'twonet-remote.log'

# **Run synchronously over the ssh session and capture here.** The first
# version started the far node with `Start-Process -RedirectStandardOutput`,
# which produced a zero-byte log and a node that looked like it had crashed —
# it had not; nothing was captured. Holding the session open and reading its
# stdout is both simpler and the only version that produced a result.
#
# **`--no-mdns` on both ends.** Two machines on different subnets cannot find
# each other by multicast anyway, so leaving it on would not have changed the
# outcome - but it would have left the result arguable, and the whole point of
# reaching across the boundary is to say the DHT lobby and the relay are what
# did the finding. With it off there is nothing else it could have been.
$remoteCmd = if ($farIsWindows) {
    "cd $remoteDir && rmdir /s /q profile 2>nul & p2p-poker.exe --headless --no-mdns --profile $remoteDir\profile --join $TableName --for $Seconds"
} else {
    "cd '$remoteDir' && rm -rf profile && ./p2p-poker --headless --no-mdns --profile ./profile --join $TableName --for $Seconds"
}
$far = Start-Job -ScriptBlock {
    param($ssh, $cmd, $out)
    & ssh @ssh $cmd 2>&1 | Set-Content -Path $out
} -ArgumentList (,$ssh), $remoteCmd, $remoteLogLocal

# A fresh profile at this end as well. Without it the second run of this script
# comes back as the first run's player, with its keys and its history, and a
# result that depends on what an earlier run left behind is not a measurement.
Remove-Item -Recurse -Force "$env:TEMP\twonet-profile" -ErrorAction SilentlyContinue

# This end hosts the table, so the far end has something to look for.
$here = Start-Process -FilePath $Binary -PassThru -NoNewWindow -RedirectStandardOutput $localLog `
    -ArgumentList @('--headless', '--no-mdns', '--profile', "$env:TEMP\twonet-profile",
                    '--host', $TableName, '--seats', '2', '--min', '2', '--for', "$Seconds")

Wait-Process -Id $here.Id -Timeout ($Seconds + 60) -ErrorAction SilentlyContinue
Receive-Job $far -Wait -AutoRemoveJob | Out-Null

# --- what actually happened --------------------------------------------------
Step "reading both logs"
$remoteText = Get-Content $remoteLogLocal -ErrorAction SilentlyContinue
$localText  = Get-Content $localLog -ErrorAction SilentlyContinue

function Count($text, $pattern) { ($text | Select-String -Pattern $pattern -AllMatches).Count }

Write-Host ""
Write-Host "  here  : peers $(Count $localText 'another poker client'), table set $(Count $localText 'the table is set'), hands $(Count $localText 'opens at genesis')"
Write-Host "  there : peers $(Count $remoteText 'another poker client'), saw table $(Count $remoteText 'table [0-9a-f]'), hands $(Count $remoteText 'opens at genesis')"
Write-Host ""
Write-Host "  relay reservations here : $(Count $localText 'reservation')"
Write-Host "  relay reservations there: $(Count $remoteText 'reservation')"
# **Both sides, and the reservation's verdict.** The first version counted direct
# connections at the far end only, and that is the number this test turns on: a
# run where the far peer never hole-punches plays no hand to the end, because
# every public relay found so far offers 128 KB / 120 s and the client itself
# calls that not enough to carry one. A run that reports "a table formed" and
# nothing about how it was carried invites the wrong conclusion.
Write-Host "  direct connections here  : $(Count $localText 'is now a direct connection')"
Write-Host "  direct connections there : $(Count $remoteText 'is now a direct connection')"
Write-Host "  peers left on the relay  : here $(Count $localText 'stays relayed'), there $(Count $remoteText 'stays relayed')"
$thin = (Count $localText 'NOT enough to carry a hand') + (Count $remoteText 'NOT enough to carry a hand')
if ($thin -gt 0) {
    Write-Host "  relay reservations too small to carry a hand: $thin" -ForegroundColor Yellow
}
$stuck = (Count $localText 'NoPeersSubscribedToTopic') + (Count $remoteText 'NoPeersSubscribedToTopic')
if ($stuck -gt 0) {
    Write-Host "  publishes with nobody subscribed: $stuck  (a circuit that closed mid-hand looks like this)" -ForegroundColor Yellow
}
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

Note "logs: $localLog and $remoteLogLocal"

# The genesis hashes are the only thing that says the two really played the same
# hand rather than two hands with the same name.
#
# **Wrapped in `@()` because a run where one end opened no hand made this throw
# and turned a successful test into exit 1.** `Select-String` with no match
# yields nothing, `.Matches` on nothing is `$null`, and `ForEach-Object` over
# `$null.Groups` is *Cannot index into a null array*. Measured: a run that
# reported `RESULT a table formed across the boundary` and then failed here,
# which reads as a failed test to anybody who checks the exit code.
function Get-Genesis($text) {
    $m = @($text | Select-String -Pattern 'opens at genesis (\S+)' -AllMatches)
    if (-not $m) { return @() }
    @($m.Matches | ForEach-Object { $_.Groups[1].Value })
}
$hereG  = Get-Genesis $localText
$thereG = Get-Genesis $remoteText
$shared = @($hereG | Where-Object { $thereG -contains $_ })
Note "hands here $($hereG.Count), there $($thereG.Count), at the same genesis $($shared.Count)"
if ($shared.Count -gt 0) {
    $list = $shared -join ', '
    Write-Host "        agreed on: $list" -ForegroundColor Green
}

}
finally {
    # The key copy never outlives the run, whatever happened during it.
    Remove-Item $privKey -Force -ErrorAction SilentlyContinue
}
