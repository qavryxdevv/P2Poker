<#
.SYNOPSIS
    Runs one p2p-poker node on this machine and one inside a Hyper-V VM, on a
    different subnet, and reports what actually happened between them.

.DESCRIPTION
    What this proves, and what it does not, stated first so the result is not
    read as more than it is.

    PROVES:
      * Two nodes on DIFFERENT SUBNETS, neither on the other's LAN broadcast
        domain, so mDNS cannot be what finds them.
      * The Mainline announce and lookup work from both, independently.
      * A dial across the subnet boundary completes a libp2p handshake.
      * A signed table advert crosses GossipSub and is admitted under the
        section 7.2 rules at the far end.
      * If the host volunteers as a relay, the VM reserves a circuit and READS
        THE RETURNED LIMIT - which is the relay path carrying something for the
        first time.

    DOES NOT PROVE:
      * NAT traversal between two peers that cannot reach each other directly.
        The VM can reach the host directly across the Hyper-V switch, so no hole
        needs punching. A real test of that needs ONE ENDPOINT OUTSIDE THIS
        HOUSE - a VPS, or the VM bridged to a phone hotspot - and nothing on one
        physical machine can substitute for it.
      * Anything about hairpinning. Both nodes share one external address, so
        DHT-discovered dials between them fail here for the same reason they
        failed in the single-machine run, and that is expected rather than a
        fault.

.NOTES
    Must be run ELEVATED. Hyper-V cmdlets return nothing under a UAC-filtered
    token, which looks exactly like "there are no VMs".

    You will be prompted once for the VM's own Windows credentials. They go to
    PowerShell Direct and to nothing else.
#>

[CmdletBinding()]
param(
    [string] $VMName = 'p2p-poker-test',
    [int]    $Seconds = 240,
    [string] $Binary  = "$PSScriptRoot\..\target\release\p2p-poker.exe"
)

$ErrorActionPreference = 'Stop'

function Fail($msg) { Write-Host "FAIL  $msg" -ForegroundColor Red; exit 1 }
function Note($msg) { Write-Host "      $msg" -ForegroundColor DarkGray }
function Step($msg) { Write-Host "==>   $msg" -ForegroundColor Cyan }

# --- elevation --------------------------------------------------------------
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Fail "not elevated. Hyper-V cmdlets return an EMPTY LIST under a filtered token, which reads as 'no VMs' - so this exits rather than reporting that."
}

if (-not (Test-Path $Binary)) {
    Fail "no binary at $Binary. Build it first: cargo build --release"
}

# --- the VM -----------------------------------------------------------------
Step "looking for the VM"
$vm = Get-VM -Name $VMName -ErrorAction SilentlyContinue
if (-not $vm) {
    $names = (Get-VM | Select-Object -ExpandProperty Name) -join ', '
    Fail "no VM called '$VMName'. Present: $names"
}
Note "state: $($vm.State)"

if ($vm.State -ne 'Running') {
    Step "starting it"
    Start-VM -Name $VMName
    Note "waiting for the guest services to answer"
    $deadline = (Get-Date).AddMinutes(5)
    while ((Get-Date) -lt $deadline) {
        $heartbeat = (Get-VMIntegrationService -VMName $VMName -Name Heartbeat -ErrorAction SilentlyContinue).PrimaryStatusDescription
        if ($heartbeat -eq 'OK') { break }
        Start-Sleep -Seconds 5
    }
}

# The file copy needs the Guest Service Interface, which is off by default.
$gsi = Get-VMIntegrationService -VMName $VMName -Name 'Guest Service Interface'
if (-not $gsi.Enabled) {
    Step "enabling the Guest Service Interface (needed to copy the binary in)"
    Enable-VMIntegrationService -VMName $VMName -Name 'Guest Service Interface'
}

# --- the subnets, which are the point --------------------------------------
Step "checking that these really are two networks"
$hostIPs = Get-NetIPAddress -AddressFamily IPv4 |
    Where-Object { $_.IPAddress -notlike '127.*' } |
    Select-Object -ExpandProperty IPAddress
Note "host addresses: $($hostIPs -join ', ')"

$vmNet = Get-VMNetworkAdapter -VMName $VMName
Note "VM switch: $($vmNet.SwitchName)"
Note "VM addresses: $($vmNet.IPAddresses -join ', ')"

$vmIPv4 = $vmNet.IPAddresses | Where-Object { $_ -match '^\d+\.\d+\.\d+\.\d+$' } | Select-Object -First 1
if (-not $vmIPv4) {
    Fail "the VM reports no IPv4 address yet. Give it a moment after boot and re-run."
}

$hostPrefix = ($hostIPs | ForEach-Object { ($_ -split '\.')[0..2] -join '.' })
$vmPrefix = ($vmIPv4 -split '\.')[0..2] -join '.'
if ($hostPrefix -contains $vmPrefix) {
    Write-Host "WARN  the VM is on the SAME /24 as this host ($vmPrefix). mDNS may find them, which is not what this test is for - put the VM on the Default Switch for a NAT'd subnet." -ForegroundColor Yellow
} else {
    Note "different subnets: host $($hostPrefix -join '/'), VM $vmPrefix - mDNS cannot cross this"
}

# --- credentials, entered by you and never seen by anything else -----------
Step "the VM's own Windows credentials (for PowerShell Direct)"
$cred = Get-Credential -Message "Sign in to $VMName"

$session = New-PSSession -VMName $VMName -Credential $cred
try {
    # --- copy the binary in -------------------------------------------------
    Step "copying the binary into the VM"
    $remoteDir = 'C:\p2p-poker-test'
    Invoke-Command -Session $session -ScriptBlock {
        param($d)
        if (-not (Test-Path $d)) { New-Item -ItemType Directory -Path $d | Out-Null }
        # A fresh profile, so the VM node is a different client from this host.
        $p = Join-Path $d 'profile'
        if (Test-Path $p) { Remove-Item $p -Recurse -Force }
    } -ArgumentList $remoteDir
    Copy-Item -Path $Binary -Destination "$remoteDir\p2p-poker.exe" -ToSession $session -Force

    # --- run both, at the same time ----------------------------------------
    Step "running both nodes for $Seconds s"
    $hostLog = Join-Path $env:TEMP 'p2p-host.log'
    $vmLogRemote = "$remoteDir\vm.log"

    # The VM watches; this host hosts a table AND is directly reachable from the
    # VM, so it is also the relay candidate.
    #
    # Both `--headless`, and it matters on each side for a different reason.
    # In the VM this runs inside a WinRM job - a non-interactive session with no
    # desktop - so a window could not open there even on a machine that had a
    # graphics driver, and the client would now spend the run re-launching
    # itself into the software renderer instead of talking to anybody. On this
    # host a window is simply not what is being measured. The exit code is kept:
    # a client that never started used to look exactly like one that started and
    # found nothing.
    $vmJob = Invoke-Command -Session $session -AsJob -ScriptBlock {
        param($dir, $secs, $log)
        Set-Location $dir
        & "$dir\p2p-poker.exe" --headless --for $secs *> $log
        if ($LASTEXITCODE -ne 0) { "EXIT=$LASTEXITCODE" | Out-File -Append -Encoding utf8 $log }
    } -ArgumentList $remoteDir, $Seconds, $vmLogRemote

    $hostProc = Start-Process -FilePath $Binary -ArgumentList @('--headless', '--host', 'HyperV-test', '--for', "$Seconds") `
        -RedirectStandardOutput $hostLog -NoNewWindow -PassThru

    Wait-Process -Id $hostProc.Id -Timeout ($Seconds + 60)
    Wait-Job $vmJob -Timeout 120 | Out-Null

    # --- collect ------------------------------------------------------------
    Step "collecting"
    $vmLog = Invoke-Command -Session $session -ScriptBlock {
        param($log)
        if (Test-Path $log) { Get-Content $log -Raw } else { '' }
    } -ArgumentList $vmLogRemote

    $hostText = if (Test-Path $hostLog) { Get-Content $hostLog -Raw } else { '' }

    # --- report -------------------------------------------------------------
    Write-Host ''
    Write-Host '================ what happened ================' -ForegroundColor White

    function Check($label, $hostPattern, $vmPattern) {
        $h = $hostText -match $hostPattern
        $v = $vmLog -match $vmPattern
        $mark = if ($h -and $v) { 'BOTH ' } elseif ($h) { 'host ' } elseif ($v) { 'VM   ' } else { 'NO   ' }
        $colour = if ($h -and $v) { 'Green' } elseif ($h -or $v) { 'Yellow' } else { 'Red' }
        Write-Host ("{0} {1}" -f $mark, $label) -ForegroundColor $colour
    }

    # First, because every line below it is meaningless if a client never ran -
    # and a client that failed to start produces the same empty log as one that
    # started and found nobody.
    Check 'the client started at all'          '^peer id ' '^peer id '
    Check 'announced under the lobby infohash' 'announce udp' 'announce udp'
    Check 'discovered peers through the DHT'   'discover \d+ usable' 'discover \d+ usable'
    Check 'completed a libp2p handshake'       '^connect ' '^connect '
    Check 'reserved a relay circuit'           '^relay .*limit' '^relay .*limit'
    Check 'a table advert crossed'             '^publish ' '^table '

    Write-Host ''
    Write-Host '--- host log (first 40 lines) ---' -ForegroundColor DarkGray
    ($hostText -split "`n" | Select-Object -First 40) -join "`n"
    Write-Host ''
    Write-Host '--- VM log (first 40 lines) ---' -ForegroundColor DarkGray
    ($vmLog -split "`n" | Select-Object -First 40) -join "`n"

    Write-Host ''
    Write-Host 'Read the result against the header of this script.' -ForegroundColor White
    Write-Host 'A missing hole punch here is EXPECTED: the VM can reach this host' -ForegroundColor DarkGray
    Write-Host 'directly, so there is no hole to punch. That case needs an endpoint' -ForegroundColor DarkGray
    Write-Host 'outside this network and nothing on one machine can stand in for it.' -ForegroundColor DarkGray
}
finally {
    Remove-PSSession $session
}
