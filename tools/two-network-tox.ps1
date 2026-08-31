<#
.SYNOPSIS
    Does a Tox group carry traffic between two machines on two networks?
    This is the measurement D-019 rests on.

.DESCRIPTION
    `tools/two-network-ssh.ps1` measured the libp2p path and found what D-019
    exists to answer: where DCUtR hole-punched, five hands played and both ends
    agreed on all five genesis hashes; where it did not, the hand reached the
    deal, the relay circuit hit its 128 KiB limit, and every publish afterwards
    answered `NoPeersSubscribedToTopic`.

    This script measures the replacement. It runs `examples/tox_link` at both
    ends: the near one makes a private Tox group and prints its public key, the
    far one adds that key as a friend and waits to be invited, and once it is in
    the group both push packets the size of a `SHUFFLE_STEP` fragment until the
    time runs out.

    WHAT A PASSING RUN PROVES

      * An invitation crossed a real network boundary, with no libp2p relay and
        no DHT search for the group — the group is created PRIVATE and its chat
        id is never looked up, which is the whole reason the invitation route
        was chosen (a public NGC group is findable only while it is new).
      * Lossless packets flowed afterwards, both ways, with no per-circuit byte
        cap to run into.

    WHAT IT DOES NOT PROVE

      * NAT traversal, unless the two machines are on different NATs. The same
        caveat as the libp2p script, for the same reason, and it is printed for
        the same reason: it is the first thing lost when a result is repeated.

.NOTES
    Needs `cargo build --release --features tox --example tox_link` to have run,
    which needs `tools/build-tox.ps1` to have run once before that.
#>

[CmdletBinding()]
param(
    [string] $Target  = 'user@172.16.0.20',
    [string] $KeyPath = 'X:\keys\far-machine-key',
    [int]    $Seconds = 180,
    [string] $Binary  = "$PSScriptRoot\..\target\release\examples\tox_link.exe",
    # Forces both ends through a TCP relay. Not a preference: two hosts behind
    # one router on different subnets cannot reach each other over Tox's UDP
    # path, because the DHT publishes both under one public address and the
    # hole punch asks the router to hairpin a packet back to itself. See the
    # D-019 notes in NEXT.md; a relay is a third party with an address of its
    # own, so nothing has to hairpin.
    [switch] $TcpOnly
)

$ErrorActionPreference = 'Stop'

function Fail($msg) { Write-Host "FAIL  $msg" -ForegroundColor Red; exit 1 }
function Note($msg) { Write-Host "      $msg" -ForegroundColor DarkGray }
function Step($msg) { Write-Host "==>   $msg" -ForegroundColor Cyan }
function Warn($msg) { Write-Host "WARN  $msg" -ForegroundColor Yellow }

$hostPart = ($Target -split '@')[-1]

if (-not (Test-Path $Binary)) {
    Fail "no probe at $Binary. Run tools/build-tox.ps1 once, then: cargo build --release --features tox --example tox_link"
}

Step "checking the far end is up before spending two minutes on SSH"
$probe = Test-NetConnection -ComputerName $hostPart -Port 22 -WarningAction SilentlyContinue
if (-not $probe.TcpTestSucceeded) {
    Fail "$hostPart does not accept TCP/22. Check the far machine is on and its firewall admits ssh."
}
Note "tcp/22: True"

if ($hostPart -match '^(10)\.|^(192)\.(168)\.|^(172)\.(1[6-9]|2[0-9]|3[01])\.|^(169)\.(254)\.') {
    Warn "$hostPart is a private address: two subnets with a router between them is a real boundary but NOT a NAT to traverse. A success here does not prove hole punching."
}

# Windows OpenSSH refuses a private key whose file others can read, and reports
# it as `Permission denied (publickey)` — which reads like a rejected key rather
# than an unread one. The key lives on removable media, so a copy is made with
# its ACL cut to this account and removed in the `finally` at the end.
Step "preparing a copy of the key Windows OpenSSH will load"
$privKey = Join-Path $env:TEMP ('twonettox-' + [IO.Path]::GetFileName($KeyPath))
Copy-Item -Path $KeyPath -Destination $privKey -Force
$acl = Get-Acl $privKey
$acl.SetAccessRuleProtection($true, $false)
$acl.Access | ForEach-Object { [void]$acl.RemoveAccessRule($_) }
$me = [Security.Principal.WindowsIdentity]::GetCurrent().Name
$rule = New-Object System.Security.AccessControl.FileSystemAccessRule($me, 'FullControl', 'Allow')
$acl.AddAccessRule($rule)
Set-Acl -Path $privKey -AclObject $acl

try {

$ssh = @('-i', $privKey, '-o', 'StrictHostKeyChecking=accept-new', '-o', 'ConnectTimeout=10', '-o', 'BatchMode=yes', $Target)
$remoteDir = 'C:\p2p-poker-test'

Step "copying the probe over"
& ssh @ssh "md $remoteDir" 2>$null | Out-Null
& scp -i $privKey -o StrictHostKeyChecking=accept-new $Binary "${Target}:$remoteDir/tox_link.exe"
if ($LASTEXITCODE -ne 0) { Fail "scp failed" }

$localLog  = Join-Path $env:TEMP 'twonettox-local.log'
$remoteLog = Join-Path $env:TEMP 'twonettox-remote.log'

# **Both keys are worked out before either end starts, because a Tox friendship
# is two-sided.** `tox_friend_add_norequest` on one side alone establishes
# nothing - the other end has never heard of the caller and does not answer it.
# The first version of this script had the joiner add the host and not the
# reverse, and the run was a hundred and fifty seconds of silence that read
# exactly like an unreachable machine.
#
# In the client both ends take both keys off the ratified roster. Here the
# identities are seeded, so `--print-key` computes each one locally and offline
# before anything is launched. The seeds are fixed and public on purpose: these
# are disposable measurement identities, not a player's.
$hostSeed = '01' * 32
$joinSeed = '02' * 32

Step "working out both identities"
$hostKey = (& $Binary --seed $hostSeed --print-key).Trim()
$joinKey = (& $Binary --seed $joinSeed --print-key).Trim()
if ($hostKey -notmatch '^[0-9A-F]{64}$' -or $joinKey -notmatch '^[0-9A-F]{64}$') {
    Fail "the probe did not print a key: got '$hostKey' and '$joinKey'"
}
Note "host $hostKey"
Note "join $joinKey"

Step "starting the host end"
Remove-Item $localLog -ErrorAction SilentlyContinue
$here = Start-Process -FilePath $Binary -PassThru -NoNewWindow `
    -RedirectStandardOutput $localLog `
    -ArgumentList (@('--seed', $hostSeed, '--peer', $joinKey, '--host', '--for', "$Seconds") +
                   $(if ($TcpOnly) { @('--tcp-only') } else { @() }))

Step "running both ends for $Seconds s"
$far = Start-Job -ScriptBlock {
    param($ssh, $cmd, $out)
    & ssh @ssh $cmd 2>&1 | Set-Content -Path $out
} -ArgumentList (,$ssh), ("cd $remoteDir && tox_link.exe --seed $joinSeed --peer $hostKey --for $Seconds" +
    $(if ($TcpOnly) { ' --tcp-only' } else { '' })), $remoteLog

Wait-Process -Id $here.Id -Timeout ($Seconds + 60) -ErrorAction SilentlyContinue
Receive-Job $far -Wait -AutoRemoveJob | Out-Null

Step "reading both logs"
$hereText  = Get-Content $localLog  -ErrorAction SilentlyContinue
$thereText = Get-Content $remoteLog -ErrorAction SilentlyContinue

function Line($text, $pattern) {
    ($text | Select-String -Pattern $pattern | Select-Object -First 1).Line
}

Write-Host ""
Write-Host "  here : $(Line $hereText 'RESULT self')"
Write-Host "  there: $(Line $thereText 'RESULT self')"
Write-Host ""
foreach ($p in 'friend up after', 'joined the group after', 'FIRST PACKET') {
    $h = Line $hereText $p
    $t = Line $thereText $p
    if ($h) { Write-Host "  here : $h" }
    if ($t) { Write-Host "  there: $t" }
}
Write-Host ""

$crossed = ($thereText | Select-String -Pattern 'the group carried traffic').Count +
           ($hereText  | Select-String -Pattern 'the group carried traffic').Count
if ($crossed -ge 2) {
    Write-Host "RESULT  the group carried traffic BOTH ways across the boundary" -ForegroundColor Green
} elseif ($crossed -eq 1) {
    Write-Host "RESULT  traffic crossed one way only - which end received is above" -ForegroundColor Yellow
} elseif ($thereText | Select-String -Pattern 'joined the group') {
    Write-Host "RESULT  the invitation crossed and no packet did - the group is where to look" -ForegroundColor Yellow
} elseif ($thereText | Select-String -Pattern 'friend .* connection [12]') {
    Write-Host "RESULT  the friend connection came up and no invitation was accepted" -ForegroundColor Yellow
} else {
    Write-Host "RESULT  the two never became friends. Discovery is the whole finding." -ForegroundColor Red
}
Note "logs: $localLog and $remoteLog"

}
finally {
    Remove-Item $privKey -Force -ErrorAction SilentlyContinue
}
