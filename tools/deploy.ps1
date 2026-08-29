# Build the client and put it where it is played from.
#
# The client is one portable executable that keeps its profile beside itself, so
# a deployment is a copy — and the profile is exactly what must NOT be copied
# over. This script never touches `profile\`: it replaces the binary and leaves
# the player's keys, settings and the tables they have joined alone.
#
#   tools\deploy.ps1                     build, check, copy to ~\Games\P2Poker
#   tools\deploy.ps1 -To D:\Elsewhere    somewhere else
#   tools\deploy.ps1 -SkipChecks         copy what is already built
#
# It refuses to overwrite a running client rather than failing halfway through
# with a file-in-use error that leaves the folder in neither state.

param(
    [string]$To = "~\Games\P2Poker",
    [switch]$SkipChecks,
    [switch]$SkipClean
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

# Cargo is not always on the PATH of the shell this is launched from — a
# scheduled task, or an editor's terminal, or a session that has never opened a
# developer prompt. Found rather than assumed, and named if it is missing.
$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargo) {
    $guess = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
    if (Test-Path $guess) { $cargo = $guess }
}
if (-not $cargo) {
    Write-Host "FAIL  cargo is not on the PATH and is not at ~\.cargo\bin" -ForegroundColor Red
    exit 1
}
$exe = Join-Path $root 'target\release\p2p-poker.exe'

function Step($text) { Write-Host "  $text" -ForegroundColor DarkGray }
function Ok($text) { Write-Host "OK    $text" -ForegroundColor Green }
function Die($text) { Write-Host "FAIL  $text" -ForegroundColor Red; exit 1 }

# A running client holds its own executable open on Windows. Say so before doing
# any work rather than after.
$running = Get-Process p2p-poker -ErrorAction SilentlyContinue
if ($running) {
    Die "p2p-poker is running (pid $($running.Id -join ', ')). Close it and run this again."
}

Push-Location $root
try {
    if (-not $SkipChecks) {
        Step 'cargo test --release'
        $test = & $cargo test --release -- --test-threads=19 2>&1
        if ($LASTEXITCODE -ne 0) { $test | Select-Object -Last 25; Die 'the tests do not pass' }
        Ok 'tests pass'

        Step 'cargo clippy --all-targets --release'
        $clippy = & $cargo clippy --all-targets --release 2>&1
        if ($LASTEXITCODE -ne 0) { $clippy | Select-Object -Last 25; Die 'clippy is not clean' }
        if ($clippy | Select-String -Pattern '^warning' -Quiet) {
            $clippy | Select-String -Pattern '^warning' | Select-Object -First 5
            Die 'clippy has warnings'
        }
        Ok 'clippy is clean'
    }

    Step 'cargo build --release'
    & $cargo build --release
    if ($LASTEXITCODE -ne 0) { Die 'the build failed' }
    if (-not (Test-Path $exe)) { Die "no binary at $exe" }
    Ok ("built {0:N1} MB" -f ((Get-Item $exe).Length / 1MB))
}
finally { Pop-Location }

# The copy. Everything except the profile, which belongs to whoever plays here.
New-Item -ItemType Directory -Force -Path $To | Out-Null

$staged = @()
Copy-Item $exe (Join-Path $To 'p2p-poker.exe') -Force
$staged += 'p2p-poker.exe'

foreach ($doc in @('README.md', 'NEXT.md')) {
    $from = Join-Path $root $doc
    if (Test-Path $from) { Copy-Item $from $To -Force; $staged += $doc }
}

$profile = Join-Path $To 'profile'
$kept = if (Test-Path $profile) {
    (Get-ChildItem $profile -File -ErrorAction SilentlyContinue).Count
} else { 0 }

Write-Host ''
Ok "deployed to $To"
foreach ($f in $staged) {
    $item = Get-Item (Join-Path $To $f)
    Write-Host ("      {0,-16} {1,8:N0} bytes" -f $item.Name, $item.Length) -ForegroundColor DarkGray
}
if ($kept -gt 0) {
    Write-Host "      profile\        $kept file(s) left untouched" -ForegroundColor DarkGray
} else {
    Write-Host "      profile\        will be created on first run" -ForegroundColor DarkGray
}

# A deployed client that cannot start is a deployment that failed, and the
# cheapest possible proof is to start it.
Step 'starting it once, headless, to prove the copy runs'
$out = & (Join-Path $To 'p2p-poker.exe') --headless --for 3 2>&1
if ($LASTEXITCODE -ne 0) { $out | Select-Object -Last 10; Die 'the deployed binary did not run' }
$peer = ($out | Select-String '^peer id').ToString()
if (-not $peer) { $out | Select-Object -Last 10; Die 'it started but printed no identity' }
Ok $peer.Trim()

# And sweep. `cargo build` never deletes anything, so every deployment otherwise
# leaves another copy of every test binary and another set of dependency builds
# behind — it reached five gigabytes before this was written, of which two and a
# half were dead. Done here because a deployment is exactly the moment the
# previous build stopped mattering.
if (-not $SkipClean) {
    Write-Host ''
    & (Join-Path $PSScriptRoot 'clean.ps1') -Deep
}
