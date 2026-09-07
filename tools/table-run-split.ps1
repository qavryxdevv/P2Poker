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
# **PowerShell 7, and refusing is the point.**
#
# This script sets `$ErrorActionPreference = 'Stop'` and pipes several native
# commands through `2>&1` — cargo, ssh, scp and every seat. Windows PowerShell
# 5.1 turns each stderr line into a terminating `ErrorRecord`; PowerShell 7.4
# and later do not, unless `$PSNativeCommandUseErrorActionPreference` is set. So
# under 5.1 a **successful** build kills the run and the trap reports cargo's
# own *"Finished `release` profile"* as the failure — measured on 2026-09-06.
#
# `#Requires` refuses the wrong shell outright, which is this file's own rule:
# a run that fell over must not be able to look like a run that passed, and a
# run that could not start must not be able to look like one that did.
#Requires -Version 7.0

[CmdletBinding()]
param(
    [ValidateRange(1, 9)][int]$Here = 5,
    [ValidateRange(1, 9)][int]$There = 5,
    # **420 covers everything a run is read for. Longer is habit, not method.**
    #
    # Measured over five ten-seat runs on 2026-09-04: the founder sealed the
    # table between 18.7 s and 153.6 s, and every timeout certificate this
    # register has a timestamp for landed between 289.8 s and 358.2 s. Seven
    # minutes therefore contains formation with a wide margin and the whole of
    # the window where a seat is first voted out, which are the two things a
    # run is looked at for.
    #
    # Fifteen minutes was passed on the command line all day and bought
    # nothing but a slower loop: the last eight minutes of a 900-second run
    # have never yet changed a reading. Ask for longer only when the question
    # is specifically about the long tail -- throughput drift, a late
    # divergence, a peer that flaps -- and say so when you do.
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
    [ValidateRange(0, 8)][int]$StallSeat = 0,
    # Skip the build. The staleness check below still runs, so this only saves
    # the time of a no-op build -- it cannot be used to measure a stale binary.
    # fault-harness: park every TIMEOUT_CERT received by far seat
    # -DelayCertsSeat for -DelayCerts ms (0 = off). Induces the S1-BS shape.
    [ValidateRange(0, 600000)][int]$DelayCerts = 0,
    [ValidateRange(0, 8)][int]$DelayCertsSeat = 0,
    # ... only for frames arriving before this many seconds of that seat's run
    # (0 = the whole run).
    [ValidateRange(0, 3600)][int]$DelayCertsUntil = 0,
    # fault-harness: local seat -LinkDownSeat drops every table message both
    # ways from -LinkDownAt s (of its own start) for -LinkDownFor s (0 = off).
    [ValidateRange(0, 3600)][int]$LinkDownAt = 0,
    [ValidateRange(0, 3600)][int]$LinkDownFor = 0,
    [ValidateRange(0, 8)][int]$LinkDownSeat = 0,
    # fault-harness: local seat -MuteSeat drops every HAND message it would
    # SEND, from -MuteAt s (of its own start) for -MuteFor s (0 = off), and
    # hears everything throughout. One direction, and that is the whole
    # instrument.
    #
    # It is the only way to certify a seat out **without making it fall
    # behind**: every other knob here works by cutting what reaches a seat, so
    # by the time the table has voted the seat is hands behind and is no longer
    # a bystander. `S1-BW`'s fix is about the seat that is certified out and
    # still level, which is where the buffered next hand is worth anything.
    #
    # **-MuteFor must exceed one decision deadline** (30 s) or the table never
    # votes and the knob only delays that seat's stage.
    [ValidateRange(0, 3600)][int]$MuteAt = 0,
    [ValidateRange(0, 3600)][int]$MuteFor = 0,
    [ValidateRange(0, 8)][int]$MuteSeat = 0,
    # `-MuteOnTurn` measures the mute window from the first ACTION that seat
    # would publish at or after -MuteAt, instead of from -MuteAt itself.
    #
    # Four runs of the same wall-clock recipe gave four different outcomes,
    # because a mute in seconds costs the table nothing unless the seat happens
    # to be due to act inside it, and whether it is depends on where the button
    # was when the run started. On the turn it is silent across a turn it
    # certainly owed, which is what the table has to miss for a certificate.
    [switch]$MuteOnTurn,
    # `-DelayCertsHere <n>` puts -DelayCerts on a LOCAL seat instead of a far
    # one. The two knobs are the same instrument; only the node it is set on
    # differs, and the far-only wiring was an accident of where the shape was
    # first needed.
    #
    # It exists so **one** seat can carry the mute and the cert delay together,
    # which is `S1-BW`'s shape and cannot be had any other way: the mute gets
    # the seat certified out without letting it fall behind, and the delay makes
    # it learn that late, so it re-derives and re-opens hand k+1 **after** the
    # table has already dealt it. The frames that arrive in between are exactly
    # what the next-hand buffer exists to keep. -1 = off.
    [ValidateRange(-1, 8)][int]$DelayCertsHere = -1,
    # `-Think <ms>` makes EVERY seat wait that long before it acts, which is
    # `--autoplay <ms>` rather than the bare `--autoplay` this harness has
    # always passed. It is not a fault knob: it is the table's speed.
    #
    # It exists because `S1-BW`'s bystander state is bounded by two windows that
    # do not overlap at full speed. Getting a seat certified out needs about
    # 39 s of silence; the adrift latch tolerates `ADRIFT_MARGIN` = 2 hands,
    # which at the 14.4-second hands of `split130616-9` is 29 s. Slowing every
    # seat's action stretches the second window and leaves the first where it
    # is: at 3 000 ms and nine seats a hand is minutes rather than seconds, so a
    # forty-second mute costs a fraction of one hand instead of five.
    #
    # A run with this set measures a SLOW table and its formation and hand
    # counts are not comparable with the rest of the corpus. Say so when
    # reporting one.
    [ValidateRange(0, 60000)][int]$Think = 0,
    # fault-harness, in the C (patch 0016): local seat -DeafSeat ignores every
    # lossless and lossy group packet from -DeafAt s (of its first group
    # packet) for -DeafFor s (0 = off). It keeps SENDING, so its peers do not
    # time it out while it times them out -- the asymmetric timeout that is the
    # only trigger for the in-place re-handshake patch 0015 clears up after.
    #
    # **-DeafFor must exceed 58 s** (GC_CONFIRMED_PEER_TIMEOUT) or nothing is
    # timed out and the knob does nothing at all.
    [ValidateRange(0, 3600)][int]$DeafAt = 0,
    [ValidateRange(0, 3600)][int]$DeafFor = 0,
    [ValidateRange(0, 8)][int]$DeafSeat = 0,
    [switch]$NoBuild,
    # **Build without `fault-harness`, and therefore without toxcore's log.**
    #
    # `src/tox/mod.rs` registers `tox_options_set_log_callback` only under that
    # feature, so a binary built without it writes NO toxcore line of any level
    # -- not merely no DEBUG. Every "zero drops", "zero retransmits", "zero
    # ring-full refusals" conclusion drawn from such a run is unsupported: the
    # measurement was absent, not the event.
    #
    # That happened. The build step added to this script on 2026-09-03 omitted
    # the feature, and `split173908-10` and `split182531-10` carry zero toxcore
    # lines where `split163641-10` carries 155 on one node alone. Two readings
    # were made against it before the gap was noticed.
    #
    # It is a real trade: at `MIN_LOGGER_LEVEL=DEBUG` the library is loud, and
    # the writing costs something. `-Quiet` is here for a run that is measuring
    # timing rather than diagnosing, and it must be passed deliberately.
    [switch]$Quiet
)

$ErrorActionPreference = 'Stop'

# **A run that fell over must not be able to look like a run that passed.**
# Under `powershell -File`, an uncaught exception prints its message and then
# leaves the exit code at 0. An invocation that died on `cargo is not
# recognized` — before a single seat started — therefore reported success to
# the thing that launched it, and the empty log directory had to be caught by
# eye. Same class as measuring a stale binary: the run did not happen and the
# result said otherwise.
trap {
    Write-Host "the run did not complete: $($_.Exception.Message)"
    exit 1
}

$inv = [System.Globalization.CultureInfo]::InvariantCulture

if (-not $Exe) {
    $Exe = Join-Path (Split-Path -Parent $PSScriptRoot) 'target\release\p2p-poker.exe'
}
if (-not (Test-Path $Exe)) { throw "no binary at $Exe. cargo build --release" }
$Exe = (Resolve-Path $Exe).Path

# **The binary has to be newer than the source, and nothing here used to check.**
#
# The far-versus-local hash check further down proves both machines run the
# SAME build. It says nothing about whether that build contains the change
# being measured, and the difference is not academic: S1-AG withdrew a whole
# measurement made against a binary three days stale, and on 2026-09-03 a
# ten-seat run "verifying" the TIMEOUT_CERT_CAP fix ran a binary compiled
# forty-eight minutes before the fix was written. Both times the hash check
# passed and made it look verified.
#
# So: build, unless the caller says otherwise. A no-op build costs a second.
if (-not $NoBuild) {
    $root = Split-Path -Parent $PSScriptRoot
    $feat = if ($Quiet) { @() } else { @('--features', 'fault-harness') }
    # **Resolve cargo, do not assume the PATH has it.** Launched with
    # `powershell -NoProfile` — which is how a background job starts it — the
    # rustup shim is not on the PATH, `& cargo` raises CommandNotFoundException,
    # and the run dies before it builds anything.
    $cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
    if (-not $cargo) { $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
    if (-not (Test-Path $cargo)) { throw "cargo is neither on the PATH nor at $cargo; nothing can be built." }
    Write-Host "==> cargo build --release $($feat -join ' ')"
    # **`2>&1` on a native command and `ErrorActionPreference = 'Stop'` are a
    # trap, and which shell you are in decides whether it springs.** cargo
    # writes *every* progress line — `Compiling`, `Finished`, and a clean
    # build's whole output — to **stderr**. `2>&1` turns each into an
    # `ErrorRecord`, and under `Stop` an `ErrorRecord` entering the pipeline is
    # terminating. Windows PowerShell 5.1 does that; PowerShell 7.4 and later do
    # not, unless `$PSNativeCommandUseErrorActionPreference` is set.
    #
    # So the same script, the same arguments and the same successful build:
    # under `pwsh` it runs, under `powershell -File` it dies on the trap with
    # *"the run did not complete: Finished `release` profile … in 0.53s"* —
    # the success line reported as the failure. Measured on 2026-09-06, and it
    # cost a run.
    #
    # Suppressed only around the call, and `$LASTEXITCODE` is what decides the
    # outcome — which is the right instrument for a native command's success
    # and always was. The preference is restored immediately, so nothing else
    # in this script loses its `Stop`.
    $wasStop = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    & $cargo build --release @feat --manifest-path (Join-Path $root 'Cargo.toml') 2>&1 |
        Where-Object { $_ -match 'error|warning: unused|Compiling p2p-poker|Finished' } |
        ForEach-Object { Write-Host "    $_" }
    $built = $LASTEXITCODE
    $ErrorActionPreference = $wasStop
    if ($built -ne 0) { throw "the build failed; nothing was measured." }
    $Exe = (Resolve-Path $Exe).Path
}

# Belt and braces: even with -NoBuild, refuse to measure a binary older than
# the newest source file. A stale measurement is worse than no measurement,
# because it gets written down.
# `src` and the vendored C, which are what the binary is built FROM. Not
# `patches/`: those files document what was done to `vendor/`, they are not a
# build input, and including them meant writing one after a successful build
# refused the run that followed. That a patch is actually applied is checked by
# the markers in `tools/build-tox.ps1`, which is the right instrument for it.
$newest = Get-ChildItem -Path (Join-Path (Split-Path -Parent $PSScriptRoot) 'src'),
                              (Join-Path (Split-Path -Parent $PSScriptRoot) 'vendor\c-toxcore\toxcore') `
    -Recurse -File -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
$exeTime = (Get-Item $Exe).LastWriteTime
if ($newest -and $newest.LastWriteTime -gt $exeTime) {
    throw ("the binary is older than the source: $Exe is $exeTime, " +
           "$($newest.Name) is $($newest.LastWriteTime). Every measurement from " +
           "this run would have been about a build that predates the change. " +
           "Drop -NoBuild, or build by hand.")
}
if (-not (Test-Path $KeyPath)) { throw "no key at $KeyPath. Plug the USB volume in or pass -KeyPath." }

# **Where the logs go, and it is not the temp directory (2026-09-06).**
#
# Storage Sense is on for this account with temp-file cleanup enabled, and it
# deleted the whole of the old work root -- 76 runs, 277 MB, including every run
# the register cites by name -- between one read of a log and the next. They came
# back from the recycle bin, but nothing warned, and a register row that says
# *measured in `split092359-10`* is worth exactly as much as the log it points
# at. The repository's own `runs/` is ignored by git and untouched by Windows.
$repoRoot = Split-Path -Parent $PSScriptRoot

$seats = $Here + $There
$stamp = (Get-Date).ToString('HHmmss')
$table = "split$stamp-$seats"
$work = Join-Path $repoRoot (Join-Path 'runs' $table)
New-Item -ItemType Directory -Force -Path $work | Out-Null

# **The header goes to the archive as well as to the terminal, and it is built
# once so the two cannot disagree.**
#
# A run directory used to hold only the logs and `far.ps1`. `far.ps1` records the
# FAR seat's arguments; every fault knob is applied to a LOCAL seat, so nothing
# in `runs/<name>/` said whether a run was clean or muted. Every fold over the
# corpus then has to guess, and a fold that guesses wrong reads a manufactured
# silence as a natural one -- which is exactly the distinction `S1-BB` turns on.
# It cost two readings on 2026-09-07 before anybody noticed the archive could not
# answer the question at all.
$header = @(
    "table  $table"
    "seats  $seats  ($Here here, $There on $Target)"
    "for    $Seconds s"
    "mdns   $(if ($NoMdns) { 'off - every seat finds every other through the public lobby' } else { 'on' })"
    "tox    $(if ($Quiet) { 'log OFF - toxcore writes nothing; do not read a zero as an absence' } else { 'log on (fault-harness)' })"
    "delay  $(if ($DelayCerts -gt 0) { "TIMEOUT_VOTE and TIMEOUT_CERT frames parked $DelayCerts ms on $(if ($DelayCertsHere -ge 0) { "local seat $DelayCertsHere" } else { "far seat $DelayCertsSeat" })$(if ($DelayCertsUntil -gt 0) { " for frames arriving before $DelayCertsUntil s" }) (fault-harness)" } else { 'none' })"
    "link   $(if ($LinkDownFor -gt 0) { "local seat $LinkDownSeat drops every table message from $LinkDownAt s for $LinkDownFor s (fault-harness)" } else { 'no forced outage' })"
    "think  $(if ($Think -gt 0) { "every seat waits ${Think} ms before it acts - a SLOW table, not comparable with the rest of the corpus" } else { 'no delay: seats act at once' })"
    "mute   $(if ($MuteFor -gt 0) { "local seat $MuteSeat sends no hand message for $MuteFor s $(if ($MuteOnTurn) { "from its first action at or after $MuteAt s" } else { "from $MuteAt s" }) and hears everything (fault-harness)$(if ($MuteFor -le 30) { ' - WARNING: under the 30 s decision deadline, so the table will not vote it out' })" } else { 'nobody is muted' })"
    # **`-Stall` was missing from this header, and it was missing on the very
    # first run taken after the header started being archived.** The knob is
    # applied to a FAR seat at line ~506 and had no line here, so `run.txt`
    # would have called a stalled run clean -- the exact reading the file was
    # added to make possible. Caught by looking at the first file it wrote.
    "stall  $(if ($Stall -gt 0) { "far seat $StallSeat starves its group handshake for $Stall s after accepting the invitation (fault-harness; S1-AA shape (i) on demand)" } else { 'no forced handshake stall' })"
    "deaf   $(if ($DeafFor -gt 0) { "local seat $DeafSeat ignores its peers' group packets from $DeafAt s for $DeafFor s, still sending (patch 0016)$(if ($DeafFor -le 58) { ' - WARNING: under the 58 s peer timeout, so nothing will be timed out' })" } else { 'nobody is deaf' })"
    "work   $work"
)
$header | ForEach-Object { Write-Host $_ }
# Written before the seats start, so a run that dies half way still says what it
# was. `-Encoding utf8` to match the logs beside it.
$header | Out-File -FilePath (Join-Path $work 'run.txt') -Encoding utf8
# **`S1-AY`: the far box has four logical processors and the split can starve
# it.** Measured, and the numbers are the row's: with five seats there the far
# half opened 11 to 14 hands where the local half opened 31 and 32, and moving
# to eight here and two there took the same two far seats to 33 and 34 -- level
# with the best local ones -- while peer timeouts went 3 to 0 and settlement
# disagreements 553 to 64. That is CPU starvation reading as a protocol
# regression, and a large part of what this register attributed to the protocol
# across four runs was this.
#
# **Said and not enforced, deliberately.** The lossy two-machine rig is the
# only place packet loss is reproducible here and it must not be "fixed"; which
# split to run is the experiment's design and the owner's (`S1-AY`, and this
# script's parameter defaults are still 5 and 5). What an instrument may do is
# refuse to be silent about a condition it has already measured, so a run that
# will not be comparable says so in its own header rather than in the reading
# of it a week later.
if ($There -gt 2) {
    Write-Host "rig    WARNING: $There seats on a 4-core far box. S1-AY measured this as CPU starvation" -ForegroundColor Yellow
    Write-Host "       that reads as a protocol regression; 2 there is level with the local seats." -ForegroundColor Yellow
}
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

# `UserKnownHostsFile` is pointed at the null device because otherwise `ssh`
# tries to write the file and fails with *the system cannot find the path
# specified*, which also reads as a routing failure. The far end is on a private
# network and is pinned by the key.
#
# **The value is `/dev/null` and not `NUL`, which is what it said until
# 2026-09-06.** Windows OpenSSH does not map the bare `NUL` device name here: it
# opens it as an ordinary relative path and leaves a 94-byte file called `NUL`
# in the working directory — which is the repository root — after every run.
# Git then refuses `git add -A` outright with *short read while indexing NUL*,
# and the file cannot be deleted without the `\?\` prefix. `/dev/null` is
# understood by Windows OpenSSH and leaves nothing behind.
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
         '-o', 'UserKnownHostsFile=/dev/null',
         # A null known-hosts file means the host key is re-added on every call,
         # so `ssh` prints *Permanently added ... to the list of known hosts* to
         # stderr every time. Under `$ErrorActionPreference = 'Stop'` PowerShell
         # turns a native command's stderr into a terminating NativeCommandError,
         # so that warning alone killed a run. Silenced at the source; real
         # errors still print.
         '-o', 'LogLevel=ERROR',
         '-o', 'BatchMode=yes')

try {
    # **Stop any seat left running on the far end before copying over it.**
    #
    # The far seats are started detached, so ending the run here ends the `ssh`
    # and not them. They keep `p2p-poker.exe` open, and the next run's copy
    # fails with `dest open "C:/p2ptest/p2p-poker.exe": Failure` — which reads
    # like a permissions or path problem on a directory that is perfectly fine.
    #
    # Measured, and it had already cost two runs by the time it was seen: five
    # seats from the previous day were still running, holding the previous
    # day's binary, and because the script copy had failed too the far end was
    # obediently running *yesterday's* `far.ps1` — five seats for a two-seat
    # table, against a binary without any of the day's fixes in it.
    # **Remote PowerShell goes over as base64.** The far end's login shell is
    # `cmd.exe`, and it parses the command line before PowerShell ever sees it:
    # a `|` is split (which answered *'Stop-Process' is not recognized as an
    # internal or external command*), and parentheses, `;`, `&` and quotes all
    # have their own meanings. `-EncodedCommand` takes UTF-16 base64, which has
    # none of those characters in it, so nothing has to survive two parsers.
    function Invoke-Far {
        param([string[]]$SshArgs, [string]$Where, [string]$Script)
        $enc = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($Script))
        (& ssh @SshArgs $Where "powershell -NoProfile -EncodedCommand $enc" 2>&1) -join "`n"
    }

    Write-Host '==> stopping any seat left running on the far end'
    Invoke-Far $ssh $Target "Stop-Process -Name p2p-poker -Force -ErrorAction SilentlyContinue" | Out-Null
    Start-Sleep -Milliseconds 800

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

    # **Prove the far end has the bytes that were just built.** A successful
    # `scp` exit code is not the same claim: the copy can be refused by a lock,
    # land somewhere else, or -- as happened for a whole afternoon -- never run
    # at all while the run reported success. Every cross-network measurement of
    # 2026-09-02 was taken against a binary from the previous day because of
    # that, and the reports read as *the far machine never joined*, which is a
    # conclusion about the protocol drawn from a fault in the harness. See
    # `DECISIONS.md` `S1-AG`.
    #
    # A hash is the only check that cannot be satisfied by a stale file.
    $localHash = (Get-FileHash $Exe -Algorithm SHA256).Hash
    $farHash = (Invoke-Far $ssh $Target "(Get-FileHash '$FarDir\p2p-poker.exe' -Algorithm SHA256).Hash").Trim()
    if ($farHash -ne $localHash) {
        throw "the far end is not running the binary that was just built. local $($localHash.Substring(0,16))..., far $(if ($farHash) { $farHash.Substring(0, [Math]::Min(16, $farHash.Length)) + '...' } else { '(nothing)' }). Every measurement from this run would have been about a different build."
    }
    Write-Host "    far binary verified, sha256 $($localHash.Substring(0,16))..." 

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
    # A stable identity per far seat, for the reason given at the local seats
    # above: an empty profile mints a peer id that haunts the lobby key for 48 h.
    `$seatKeys = Join-Path `$env:LOCALAPPDATA 'p2p-poker-test-seats'
    `$seed = Join-Path `$seatKeys "far-n`$i.key"
    if (-not (Test-Path `$seed)) {
        New-Item -ItemType Directory -Force -Path `$seatKeys | Out-Null
        # `::Fill` is .NET Core only; this runs under Windows PowerShell.
        `$b = New-Object byte[] 32
        `$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
        try { `$rng.GetBytes(`$b) } finally { `$rng.Dispose() }
        [System.IO.File]::WriteAllBytes(`$seed, `$b)
    }
    Copy-Item `$seed (Join-Path `$p 'identity.key') -Force
    `$log = "$FarDir\split-n`$i.log"
    `$jobs += Start-Job -ArgumentList `$p, `$log, `$i -ScriptBlock {
        param(`$p, `$log, `$seat)
        if ($Stall -gt 0 -and `$seat -eq $StallSeat) { `$env:P2P_POKER_STALL_JOIN = '$Stall' }
        if ($DelayCerts -gt 0 -and $DelayCertsHere -lt 0 -and `$seat -eq $DelayCertsSeat) { `$env:P2P_POKER_DELAY_CERTS_MS = '$DelayCerts' }
        if ($DelayCerts -gt 0 -and $DelayCertsHere -lt 0 -and $DelayCertsUntil -gt 0 -and `$seat -eq $DelayCertsSeat) { `$env:P2P_POKER_DELAY_CERTS_UNTIL_S = '$DelayCertsUntil' }
        `$start = Get-Date
        `$inv = [System.Globalization.CultureInfo]::InvariantCulture
        & "$FarDir\p2p-poker.exe" --headless $(if ($NoMdns) { '--no-mdns' }) --autoplay $(if ($Think -gt 0) { "$Think" }) --for $Seconds --profile `$p --join $table 2>&1 |
            ForEach-Object { (Get-Date).ToUniversalTime().ToString('HH:mm:ss.fff', `$inv) + ' ' + ((((Get-Date) - `$start).TotalSeconds).ToString('F1', `$inv)).PadLeft(7) + '  ' + `$_ } |
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
        # **A stable identity per seat, because a fresh one is a permanent
        # ghost.** An empty profile makes `load_or_create_identity`
        # (`src/storage/profile.rs:55`) mint a new 32-byte Ed25519 seed, and that
        # brand-new peer id announces itself on the public lobby key. Kademlia
        # has no unprovide and `stop_providing` is local only, so the record
        # outlives the process by the storing node's TTL -- 48 h. One seat-start
        # was one permanent ghost.
        #
        # Measured before this existed: 1020 identities of ours on the lobby key,
        # of which a run saw 598 and could reach none. See `DECISIONS.md` S1-AI.
        # Reusing one seed per seat index turns that into 4-20 recurring ids and
        # lets the graveyard drain itself inside two days.
        #
        # The file is 32 raw bytes with no header, which is exactly what the
        # client reads; a file it cannot parse is refused rather than
        # overwritten, so a damaged seed fails loudly.
        #
        # Cost: two runs can no longer be told apart by peer id -- use the seat
        # names -- and two concurrent runs on this machine would collide on a
        # seat index. This harness starts one table at a time.
        $seatKeys = Join-Path $env:LOCALAPPDATA 'p2p-poker-test-seats'
        $seed = Join-Path $seatKeys "here-n$i.key"
        if (-not (Test-Path $seed)) {
            New-Item -ItemType Directory -Force -Path $seatKeys | Out-Null
            # `::Fill` is .NET Core only; Windows PowerShell 5.1 runs on
            # .NET Framework and has just the instance API. Measured: the run
            # died with *does not contain a method named 'Fill'*.
            $b = New-Object byte[] 32
            $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
            try { $rng.GetBytes($b) } finally { $rng.Dispose() }
            [System.IO.File]::WriteAllBytes($seed, $b)
        }
        Copy-Item $seed (Join-Path $profileDir 'identity.key') -Force
        $log = Join-Path $work "n$i.log"
        $nodeArgs = @('--headless') `
            + $(if ($Think -gt 0) { @('--autoplay', "$Think") } else { @('--autoplay') }) `
            + @('--for', "$Seconds", '--profile', $profileDir)
        if ($NoMdns) { $nodeArgs += '--no-mdns' }
        if ($i -eq 0) { $nodeArgs += @('--host', $table, '--seats', "$seats") }
        else { $nodeArgs += @('--join', $table) }

        $knobs = @{}
        if ($LinkDownFor -gt 0 -and $i -eq $LinkDownSeat) {
            $knobs['P2P_POKER_LINK_DOWN_AT'] = "$LinkDownAt"
            $knobs['P2P_POKER_LINK_DOWN_FOR'] = "$LinkDownFor"
        }
        if ($DelayCerts -gt 0 -and $DelayCertsHere -ge 0 -and $i -eq $DelayCertsHere) {
            $knobs['P2P_POKER_DELAY_CERTS_MS'] = "$DelayCerts"
            if ($DelayCertsUntil -gt 0) { $knobs['P2P_POKER_DELAY_CERTS_UNTIL_S'] = "$DelayCertsUntil" }
        }
        if ($MuteFor -gt 0 -and $i -eq $MuteSeat) {
            $knobs['P2P_POKER_MUTE_AT'] = "$MuteAt"
            $knobs['P2P_POKER_MUTE_FOR'] = "$MuteFor"
            if ($MuteOnTurn) { $knobs['P2P_POKER_MUTE_ON_TURN'] = '1' }
        }
        if ($DeafFor -gt 0 -and $i -eq $DeafSeat) {
            $knobs['P2P_POKER_DEAF_AT'] = "$DeafAt"
            $knobs['P2P_POKER_DEAF_FOR'] = "$DeafFor"
        }
        $jobs += Start-Job -Name "n$i" -ArgumentList $Exe, $nodeArgs, $log, $knobs -ScriptBlock {
            param($exe, $nodeArgs, $log, $knobs)
            foreach ($k in $knobs.Keys) { Set-Item -Path "env:$k" -Value $knobs[$k] }
            # **A wall clock, not an elapsed one, because the columns get
            # compared across nodes.**
            #
            # This was `(Get-Date) - $start` with `$start` captured INSIDE the
            # job, so every log counted from its own zero — and the loop below
            # sleeps 3 s after the founder and 400 ms after each later seat, so
            # the zeros were up to 7 s apart in start order. Subtracting two
            # such columns subtracts nothing, and it produced a clean, stable,
            # entirely fictitious result: node ranks 0.0, 0.8, 1.4, 2.0, 2.6,
            # 3.2, 4.1, 4.4, 5.6, 7.1 s apart, with the same node opening every
            # one of 33 hands first. Reconstructing the origins from message
            # causality alone reproduced those figures to within 0.2 s, and with
            # them removed all ten peers open each hand within **0.2 s** and the
            # order reshuffles every hand.
            #
            # The check that should have caught it needs no arithmetic: every
            # node played exactly 33 hands, so none of them was seven seconds
            # behind.
            #
            # UTC on both hosts, so the two files are directly comparable as
            # long as the machine clocks are. The elapsed figure is kept beside
            # it because a run is read in elapsed terms.
            $start = Get-Date
            $inv = [System.Globalization.CultureInfo]::InvariantCulture
            & $exe @nodeArgs 2>&1 | ForEach-Object {
                (Get-Date).ToUniversalTime().ToString('HH:mm:ss.fff', $inv) + ' ' +
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
        # **Which seat is this, so that `opened` can mean *dealt in*.** A
        # certified-out peer keeps following the table and keeps logging
        # `hand #k opens`, with a seat list that does not contain it — seat 3
        # logged hand 25 with `seats [0, 4, 5, 6, 7, 8, 9]` 167 s after it was
        # struck. Counting those made a seat that had been removed for three
        # minutes look like a seat that was playing, and a first reading of
        # `S1-BH` concluded from exactly that arithmetic that certification was
        # being recovered from. It was not; the peer was watching.
        $mySeat = $null
        $dealtIn = [System.Collections.Generic.HashSet[string]]::new()
        foreach ($l in $lines) {
            if ($null -eq $mySeat -and $l -match 'seat ([0-9]+) at ') { $mySeat = [int]$Matches[1] }
            if ($null -eq $mySeat -and $l -match 'hosting ') { $mySeat = 0 }
        }
        foreach ($l in $lines) {
            if ($l -match '([0-9]+) seated') {
                $n = [int]$Matches[1]
                if ($n -gt $seated) { $seated = $n }
            }
            # **Skip the UTC stamp.** This was anchored on the elapsed figure
            # being the first field on the line. Adding a wall clock in front of
            # it — so that two machines' logs could be compared at all — moved
            # the elapsed figure to second place and this stopped matching, on
            # every node, silently. The column then read `never` for all ten
            # seats, including the nine that were plainly in the group, and it
            # is the ONE column that separates *found the table but could not
            # join the Tox side* from *never found it*. A run where exactly one
            # seat failed that way reported the same thing as a run where none
            # did. The optional `HH:MM:SS.fff` is what makes it read both the
            # old logs and the new.
            if ($null -eq $group -and $l -match "^\s*(?:[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3}\s+)?([0-9.]+)\s+in the table's Tox group") {
                $group = [double]::Parse($Matches[1], [System.Globalization.CultureInfo]::InvariantCulture)
            }
            if ($l -match 'hand #([0-9]+) opens at genesis') {
                # Dealt in, or merely watching? The seat list is on the line.
                $hid = $Matches[1]
                if ($l -match 'with seats \[([0-9, ]*)\]' -and $null -ne $mySeat) {
                    $in = $Matches[1] -split ',' | ForEach-Object { $_.Trim() }
                    if ($in -contains "$mySeat") { $opens++; $null = $dealtIn.Add($hid) }
                } else {
                    $opens++
                    $null = $dealtIn.Add($hid)
                }
            }
            # **The same hands, or the two columns measure different things.**
            # `opened` counting only hands this seat was dealt while `finished`
            # counted every hand it watched produced rows like `opened 8,
            # finished 22`, which reads as a broken client and is really two
            # different questions in one table.
            if ($l -match 'hand #([0-9]+) is over' -and $dealtIn.Contains($Matches[1])) { $overs++ }
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
