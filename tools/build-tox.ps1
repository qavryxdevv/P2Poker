<#
.SYNOPSIS
    Fetches and builds what `--features tox` needs: c-toxcore's source and a
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

function Fetch($name, $url, $tag, $commit) {
    $dir = Join-Path $vendor $name
    if ((Test-Path $dir) -and -not $Force) {
        $have = (& git -C $dir rev-parse HEAD 2>$null)
        if ($have -eq $commit) {
            Note "$name already at $commit"
            return $dir
        }
        Note "$name is at $have, wanted $commit - refetching"
        Remove-Item -Recurse -Force $dir
    } elseif (Test-Path $dir) {
        Remove-Item -Recurse -Force $dir
    }
    Step "fetching $name $tag"
    & git clone --depth 1 --branch $tag --quiet $url $dir
    if ($LASTEXITCODE -ne 0) { Fail "could not clone $name" }
    $have = (& git -C $dir rev-parse HEAD)
    if ($have -ne $commit) {
        Fail "$name $tag is $have, not the pinned $commit. The tag moved, which is exactly what pinning is for - check what changed before editing this script."
    }
    return $dir
}

$tox = Fetch 'c-toxcore' 'https://github.com/TokTok/c-toxcore.git' $TOXCORE_TAG $TOXCORE_COMMIT

# `third_party/cmp` is a submodule and a shallow clone does not bring it. Its
# `cmp.c` is the first entry of `toxcore_SOURCES`, so without this the build
# fails on a missing file rather than on a missing dependency.
if (-not (Test-Path (Join-Path $tox 'third_party/cmp/cmp.c'))) {
    Step 'fetching the cmp submodule'
    & git -C $tox submodule update --init --depth 1 third_party/cmp
    if ($LASTEXITCODE -ne 0) { Fail 'could not fetch third_party/cmp' }
}
$cmpHead = (& git -C (Join-Path $tox 'third_party/cmp') rev-parse HEAD)
if ($cmpHead -ne $CMP_COMMIT) { Fail "cmp is $cmpHead, not the pinned $CMP_COMMIT" }

# --- our patches ------------------------------------------------------------
#
# **The vendored tree is fetched and gitignored, so a change to it has to live
# here or it does not exist.** `patches/` is applied in name order to the clone
# straight after the pin is verified, and the whole build stops if one does not
# apply.
#
# **Failing loudly is the point.** A patch that silently does not apply produces
# a library that looks right, links, runs, and is missing the fix — and the last
# time this project had a defect that only showed under measurement it took a
# day to find. `git apply --check` first, so a bad patch is reported before
# anything has been half-applied.
#
# A patch is expected to stop applying when `$TOXCORE_COMMIT` moves. That is not
# a reason to skip it: re-cut the patch against the new tree, and if the change
# has landed upstream, delete the file.
$patchDir = Join-Path (Split-Path -Parent $PSScriptRoot) 'patches'
if (Test-Path $patchDir) {
    $patches = @(Get-ChildItem -Path $patchDir -Filter '*.patch' | Sort-Object Name)
    if ($patches.Count -gt 0) {
        Step "applying $($patches.Count) patch(es) to c-toxcore"
        foreach ($patch in $patches) {
            # Already applied? A rebuild against an existing clone must not fail
            # for having succeeded last time.
            & git -C $tox apply --reverse --check "$($patch.FullName)" 2>$null
            if ($LASTEXITCODE -eq 0) {
                Note "  $($patch.Name) is already applied"
                continue
            }
            & git -C $tox apply --check "$($patch.FullName)" 2>&1 | Out-Null
            if ($LASTEXITCODE -ne 0) {
                Fail "$($patch.Name) does not apply to c-toxcore $TOXCORE_TAG. Re-cut it against the pinned tree, or delete it if the change has landed upstream. Do NOT build without it: the whole point of the patch is a failure that only shows under measurement."
            }
            & git -C $tox apply "$($patch.FullName)"
            if ($LASTEXITCODE -ne 0) { Fail "$($patch.Name) failed to apply after checking clean, which should be impossible" }
            Note "  $($patch.Name)"
        }
    }
}

$sodium = Fetch 'libsodium' 'https://github.com/jedisct1/libsodium.git' $SODIUM_TAG $SODIUM_COMMIT

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
