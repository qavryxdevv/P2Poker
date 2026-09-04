<#
.SYNOPSIS
    Builds what `--features tox` needs from the vendored source in this
    static libsodium. Run once; `cargo build --features tox` does the rest.

.DESCRIPTION
    D-019 moves a table's game traffic off libp2p and onto a Tox NGC group,
    because a public libp2p relay circuit grants 128 KiB and two minutes and a
    Tox session has no per-circuit byte cap. `c-toxcore` is GPL-3.0, so linking
    it makes this whole client GPL-3.0 — which is why it is behind a cargo
    feature rather than an unconditional dependency.

    WHAT THIS SCRIPT DOES AND DOES NOT DO

    It fetches both trees at **pinned commits** and builds libsodium. It does
    not build toxcore: `build.rs` compiles those sixty C files with the `cc`
    crate, reading the file list out of the vendored `CMakeLists.txt` so a
    version bump cannot leave the two out of step.

    WHY THE SOURCES ARE FETCHED RATHER THAN COMMITTED

    `vendor/ziffle` is committed, so vendoring is this project's habit, and the
    same argument — reproducible, offline, reviewable — applies here. What is
    different is that these two are 12.7 MB and the transport they are for is
    not proven yet: the point of D-019 is a measurement across two networks that
    has not been taken. Pinning by commit hash is reproducible without deciding
    the repository's shape for a dependency that may still change version.

    When the transport is measured and the licence change is made formally, the
    right move is to commit them and delete this argument.

.NOTES
    The MSVC path needs no cmake, no pkg-config and no vcpkg. toxcore's own
    CMake build requires all three on Windows; `build.rs` bypasses it, and
    `tools/msvc-shim/pthread.h` supplies the fifteen pthread calls toxcore uses
    so PThreads4W is not a dependency either.
#>

[CmdletBinding()]
param(
    [switch] $Force
)

$ErrorActionPreference = 'Stop'

# Pinned. A moving branch is a build that changes under you, and this one
# changes the licence of the binary it produces.
$TOXCORE_TAG    = 'v0.2.23'
$TOXCORE_COMMIT = 'd9ca3c577e5abd4303d180eb5270167b4133ea4c'
$CMP_COMMIT     = '52bfcfa17d2eb4322da2037ad625f5575129cece'
$SODIUM_TAG     = '1.0.20-RELEASE'
$SODIUM_COMMIT  = '9511c982fb1d046470a8b42aa36556cdb7da15de'

function Step($m) { Write-Host "==>   $m" -ForegroundColor Cyan }
function Note($m) { Write-Host "      $m" -ForegroundColor DarkGray }
function Fail($m) { Write-Host "FAIL  $m" -ForegroundColor Red; exit 1 }

$root   = Split-Path -Parent $PSScriptRoot
$vendor = Join-Path $root 'vendor'

# **The vendored trees are committed to this repository, so nothing is fetched.**
#
# They used to be cloned here at a pinned commit and patched on the way past.
# That has a failure mode a committed tree does not: a library that looks right,
# links, runs and is missing the fix. This project has already lost a day to a
# defect that only showed under measurement, and it now carries two patches to
# toxcore rather than none.
#
# So this checks that the source is present. The commit ids above are what the
# trees were taken from and are kept for a reader and for a version bump; there
# is no `.git` here to interrogate any more, and that is the point -- the answer
# is in this repository's own history, and `patches/` is the delta against those
# commits.
function Have($name, $probe) {
    $dir = Join-Path $vendor $name
    if (-not (Test-Path (Join-Path $dir $probe))) {
        Fail "$name is missing from vendor/. It is committed to this repository, so this is a partial checkout or a deleted directory - restore it with 'git checkout -- vendor/$name'."
    }
    Note "$name present"
    return $dir
}

$tox = Have 'c-toxcore' 'toxcore/group_chats.c'

# `third_party/cmp` came from a submodule of the original clone. Its
# `cmp.c` is the first entry of `toxcore_SOURCES`, so without this the build
# fails on a missing file rather than on a missing dependency.
# `third_party/cmp` was a submodule of the clone and is committed here with the
# rest of the tree. Its `cmp.c` is the first entry of `toxcore_SOURCES`, so
# without it the build fails on a missing file rather than a missing dependency.
if (-not (Test-Path (Join-Path $tox 'third_party/cmp/cmp.c'))) {
    Fail "third_party/cmp/cmp.c is missing. It is committed to this repository; restore it with 'git checkout -- vendor/c-toxcore'."
}

# --- our patches ------------------------------------------------------------
#
# **They are already in the tree, because the tree is committed.** `patches/`
# is no longer applied here; it is the record of what this repository's copy of
# c-toxcore differs from upstream by, which is what a reader needs and what a
# version bump needs.
#
# What is checked is that the patched lines are still there. A tree that was
# reverted, half-merged or restored from an unpatched copy would otherwise build
# clean and be missing the fix -- the exact failure that cost this project a day
# and the reason the source is committed at all.
#
# Each entry is a marker unique to one patch and a sentence saying what is lost
# without it. Adding a patch means adding a line here; that is deliberate
# friction, because a patch nothing verifies is a patch that can quietly stop
# existing.
$patched = @(
    @{ File   = 'toxcore/onion_client.c'
       Marker = 'set_tcp_onion_status(nc_get_tcp_c(onion_c->c), true)'
       Why    = '0001: without it a UDP-healthy node keeps no TCP relay, so its invite confirmations carry none and a relayed joiner can never complete a group join' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker: ask for the missing message on this path too'
       Why    = '0002: without it a receive ring that has wrapped drops every further packet without ever asking for the message it is missing, and the peer never recovers' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker: every confirmed peer, or this is not a send'
       Why    = '0003: without it a custom packet accepted by one peer of ten is reported to the caller as sent, so the application drops it and the other nine never see it' },
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'p2p-poker: the awaited chunk goes in even if its slot is taken'
       Why    = '0004: without it a fragment sequence blocked on an occupied slot can never be unblocked, because unlike an ordinary packet the awaited chunk must be stored - the peer is lost until the sender times it out' },
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'no room for the first chunk of a fragmented packet'
       Why    = '0005: diagnostic. Without it the fragmenting send path fails silently three times over, and the conservation identity that found S1-AM (array failures = receive drops + send refusals) is left with an unexplained residue' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'is confirmed but cannot be sent to'
       Why    = '0006: corrects 0003. Without it the denominator counts peers send_lossless_group_packet refuses outright - not handshaked, or pending delete - so one peer mid-handshake makes every send fail for ever and one re-handshaking peer stops the whole table' },
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'Failed to create %s array entry'
       Why    = '0007: diagnostic. create_array_entry is called from both add_to_send_array and store_in_recv_array and logged one sentence for both, so every count of it is the SUM of back-pressure and head-of-line blocking - two faults with nothing in common, and a whole day of readings was taken against the sum' },
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'p2p-poker: this id must not be truncated, because it is written back'
       Why    = '0008: without it a fragmented send that fails part-way rewinds gconn->send_message_id to send_message_id % 65536 - clear_send_queue_id_range assigns start_id back - so past the first 65 536 messages to a peer the whole stream jumps backwards by up to 65 535, every later message reuses a consumed id, and that peer is silently broken for the rest of the session' },
    @{ File   = 'toxcore/group_chats.c'
       Marker = 'p2p-poker: receiving a packet is not requesting one'
       Why    = '0009: without it handle_gc_lossless_helper stamps last_requested_packet_time on every successfully handled packet, and that field is the sole gate on GR_ACK_REQ - so a receiver cannot ask for a missing message for the rest of any second in which it handled anything, the fast one-RTT repair path is switched off, and recovery falls to the senders blind retry whose floor is three seconds' },
    @{ File   = 'toxcore/tox.h'
       Marker = 'p2p-poker: ask the peer for the message this stage is waiting on'
       Why    = '0011: without it the application cannot tell the carrier which message a stalled stage is waiting for, so the one-round-trip GR_ACK_REQ repair is only ever reached by accident - when a LATER message happens to arrive and reveal the hole. A stage waiting on a single seat has nothing later to reveal it, so recovery falls to the blind ladder at T+3/+5/+9/+17/+33 and the 30 s stage budget expires first, certifying out a seat whose message was in flight' },
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'p2p-poker: how many messages are stalled behind a hole'
       Why    = '0011: without it the vote cannot tell a peer the carrier is mid-delivery with from a peer that is genuinely silent, and accuses both. recv_array holds messages that arrived out of order, so a non-zero count is positive evidence the peer is talking' },
    # **The retransmit ladder is frozen by this list.** `CARRIER_LADDER_LAST_MS`
    # in src/protocol/constants.rs is derived from `gcc_resend_packets` firing on
    # `delta > 1 && is_power_of_2(delta)`, and nothing in a Rust build can notice
    # if that changes. 0011 deliberately adds a new entry point beside the ladder
    # rather than editing it, and no later patch may edit it either: change the
    # ladder and the constant silently becomes a lie.
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'delta > 1 && is_power_of_2(delta)'
       Why    = 'the retransmit ladder T+3/+5/+9/+17/+33, which src/protocol/constants.rs mirrors as CARRIER_LADDER_LAST_MS and asserts the stage budget against. If this marker is gone the ladder was edited and that constant is stale - re-derive it before anything else' },
    @{ File   = 'toxcore/group_connection.c'
       Marker = 'p2p-poker: read the id BEFORE the entry is wiped'
       Why    = '0010: without it process_recv_array_entry acks every drained message with id 0, because clear_array_entry zeroes the struct before array_entry->message_id is read - so the senders slot is never cleared, its time_added never moves, and gcc_resend_packets drops the peer at 58 s unless a later blind duplicate happens to ack it correctly' }
)

Step 'checking the patches are in the vendored source'
foreach ($p in $patched) {
    $path = Join-Path $tox $p.File
    if (-not (Test-Path $path)) { Fail "$($p.File) is missing from the vendored tree" }
    if (-not (Select-String -Path $path -SimpleMatch -Pattern $p.Marker -Quiet)) {
        Fail "$($p.File) does not contain the patch marker '$($p.Marker)'. $($p.Why). Restore the tree with 'git checkout -- vendor/c-toxcore', or re-apply patches/ if you are moving to a new upstream."
    }
    Note "  $($p.File): patched"
}

$sodium = Have 'libsodium' 'src/libsodium/sodium/core.c'

# --- libsodium, static, x64 -------------------------------------------------
$lib = Join-Path $sodium 'bin\x64\Release\v143\static\libsodium.lib'
if ((Test-Path $lib) -and -not $Force) {
    Note "libsodium.lib already built"
} else {
    Step 'building libsodium (static, x64) with MSBuild'
    # The Build Tools' own MSBuild. Not `Get-Command msbuild`, which finds
    # whatever a stray PATH entry points at.
    $msb = Get-ChildItem `
        'C:\Program Files*\Microsoft Visual Studio\2022\*\MSBuild\Current\Bin\MSBuild.exe' `
        -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName
    if (-not $msb) {
        Fail 'no MSBuild from Visual Studio 2022. Install the C++ Build Tools; the Rust toolchain already needs its linker.'
    }
    $sln = Join-Path $sodium 'builds\msvc\vs2022\libsodium.sln'
    & $msb $sln /p:Configuration=StaticRelease /p:Platform=x64 /v:minimal /m
    if ($LASTEXITCODE -ne 0) { Fail 'libsodium did not build' }
}
if (-not (Test-Path $lib)) { Fail "libsodium built but $lib is not there" }
Note "libsodium: $lib"

Write-Host ""
Write-Host "RESULT  ready. Build with:" -ForegroundColor Green
Write-Host "        cargo build --features tox"
Write-Host ""
Write-Host "        Note that --features tox links GPL-3.0 code and makes the" -ForegroundColor Yellow
Write-Host "        resulting binary GPL-3.0. That is D-019's decision, not this" -ForegroundColor Yellow
Write-Host "        script's, and it is a one-way door." -ForegroundColor Yellow
