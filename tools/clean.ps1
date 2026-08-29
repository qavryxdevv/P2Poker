# Sweep the build directory of what cargo does not.
#
# `cargo build` never deletes anything. Every test binary it has ever produced
# stays in `target\release\deps` under its own content hash, so a day's work
# leaves seventy copies of the same test suite — gigabytes of it — and `cargo
# clean` is the only tool cargo offers, which throws away the dependency builds
# too and costs ten minutes to get back.
#
# This throws away only what is genuinely dead:
#
#   * test and benchmark executables in `deps` that are not the newest of their
#     kind, and their .pdb debug files;
#   * the whole `debug` tree, when only release is being built;
#   * `incremental`, which is a cache by definition.
#
# It never touches the dependency `.rlib`s, which is the part that is expensive
# to rebuild and the reason `cargo clean` is the wrong tool.
#
#   tools\clean.ps1              sweep, and say what went
#   tools\clean.ps1 -Deep        also drop dependency builds nothing uses
#   tools\clean.ps1 -WhatIf      say what would go, and touch nothing
#   tools\clean.ps1 -KeepDebug   leave the debug tree alone
#   tools\clean.ps1 -StopOrphans stop anything still running out of this tree
#
# It always reports processes running from the build directory, by **path**
# rather than by name: a hung networking test is called `two_nodes-<hash>.exe`,
# which nobody recognises in a task list, and it holds a UDP port and its own
# file open until somebody notices.
#
# `-Deep` is where the gigabytes are, and it does not guess. Cargo is asked, in
# JSON, which files the current build actually consists of; everything else in
# `deps` is a leftover from a build with different features or a different
# version of something, and there can be three ninety-megabyte copies of one
# dependency in there. The worst case if this is wrong is a rebuild.

param(
    [switch]$WhatIf,
    [switch]$KeepDebug,
    [switch]$Deep,
    [switch]$StopOrphans
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$target = Join-Path $root 'target'

if (-not (Test-Path $target)) { Write-Host "nothing at $target"; exit 0 }

function Size($items) { ($items | Measure-Object -Property Length -Sum).Sum }
function Mb($bytes) { "{0:N0} MB" -f ($bytes / 1MB) }

# Anything at all running out of this tree, not just a process called
# `p2p-poker`.
#
# `cargo test` builds one executable per test target and runs them; a networking
# test that hung would leave one alive, holding a UDP port and its own file open,
# and it would be invisible — the name is `two_nodes-<hash>.exe`, which nobody
# recognises in a task list. Reported by path rather than by name for exactly
# that reason.
$orphans = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
    Where-Object { $_.ExecutablePath -and
                   $_.ExecutablePath.StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase) })

if ($orphans) {
    Write-Host ("{0} process(es) are running out of this tree:" -f $orphans.Count) -ForegroundColor Yellow
    foreach ($o in $orphans) {
        $age = if ($o.CreationDate) { "{0:N0} min" -f ((Get-Date) - $o.CreationDate).TotalMinutes } else { "?" }
        Write-Host ("      {0,6}  {1,-44} {2}" -f $o.ProcessId, $o.Name, $age) -ForegroundColor DarkGray
    }
    if ($StopOrphans) {
        $orphans | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
        Start-Sleep -Milliseconds 500
        Write-Host "stopped them" -ForegroundColor Green
    } else {
        Write-Host "close them, or re-run with -StopOrphans" -ForegroundColor Red
        exit 1
    }
}

$before = Size (Get-ChildItem $target -Recurse -File -ErrorAction SilentlyContinue)
Write-Host ("target is {0}" -f (Mb $before))

$doomed = @()

# --- stale test binaries ---------------------------------------------------
#
# A test executable is `name-<hash>.exe`. Cargo writes a new one whenever the
# crate changes and leaves the old, so all but the newest of each name are dead.
# The newest is kept so that `cargo test` does not have to relink to run.
foreach ($dir in @('release', 'debug')) {
    $deps = Join-Path $target "$dir\deps"
    if (-not (Test-Path $deps)) { continue }

    # `-Include` without `-Recurse` or a trailing wildcard silently matches
    # nothing, which is how the first version of this script reported a clean
    # five-gigabyte directory. Two `-Filter` passes instead: one predicate, no
    # surprises.
    $binaries = @()
    foreach ($ext in @('*.exe', '*.pdb')) {
        $binaries += Get-ChildItem $deps -File -Filter $ext -ErrorAction SilentlyContinue
    }
    $groups = $binaries | Group-Object { $_.BaseName -replace '-[0-9a-f]{8,}$', '' }

    foreach ($g in $groups) {
        $sorted = $g.Group | Sort-Object LastWriteTime -Descending
        # One executable and one .pdb of each name survive.
        $keep = @()
        foreach ($ext in @('.exe', '.pdb')) {
            $newest = $sorted | Where-Object Extension -eq $ext | Select-Object -First 1
            if ($newest) { $keep += $newest.FullName }
        }
        $doomed += $sorted | Where-Object { $keep -notcontains $_.FullName }
    }
}

# --- dependency builds nothing uses any more --------------------------------
#
# Cargo keeps every `.rlib` it has ever produced, keyed by a hash of the
# features and the version it was built with, and never removes the ones no
# current build refers to. `cargo clean` is the only offer, and it throws away
# everything including what *is* used.
#
# So cargo is asked instead. `--message-format=json` names every file the
# current build consists of; anything else in `deps` is dead. This is an
# enumeration and not a heuristic — and the cost of being wrong is a rebuild,
# not a lost source file.
if ($Deep) {
    Write-Host '  asking cargo which files the build actually uses' -ForegroundColor DarkGray
    $cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
    if (-not $cargo) {
        $guess = Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe'
        if (Test-Path $guess) { $cargo = $guess }
    }
    if (-not $cargo) {
        Write-Host 'FAIL  cargo is not on the PATH; -Deep needs it' -ForegroundColor Red
        exit 1
    }

    Push-Location $root
    try {
        $json = & $cargo build --release --all-targets --message-format=json 2>$null
        if ($LASTEXITCODE -ne 0) {
            Write-Host 'FAIL  the build did not succeed; nothing swept' -ForegroundColor Red
            exit 1
        }
    }
    finally { Pop-Location }

    # Every artefact the build named, plus its siblings: cargo reports the
    # .rlib and the .rmeta but not always the .d or the .pdb beside them, and a
    # kept library whose debug file went is a library that cannot be debugged.
    $live = [System.Collections.Generic.HashSet[string]]::new(
        [System.StringComparer]::OrdinalIgnoreCase)
    foreach ($line in $json) {
        if ($line -notmatch '"filenames"') { continue }
        try { $msg = $line | ConvertFrom-Json } catch { continue }
        foreach ($f in @($msg.filenames) + @($msg.executable)) {
            if ($f) { [void]$live.Add([System.IO.Path]::GetFileNameWithoutExtension($f)) }
        }
    }
    Write-Host ("  cargo named {0} artefact(s)" -f $live.Count) -ForegroundColor DarkGray

    if ($live.Count -eq 0) {
        Write-Host 'FAIL  cargo named nothing; refusing to sweep on no evidence' -ForegroundColor Red
        exit 1
    }

    $deps = Join-Path $target 'release\deps'
    foreach ($f in Get-ChildItem $deps -File -ErrorAction SilentlyContinue) {
        $stem = $f.BaseName
        # `.rmeta` and `.d` sit beside their library under the same stem; a
        # `.pdb` may drop the `lib` prefix and use underscores.
        $alt = ($stem -replace '^lib', '') -replace '-', '_'
        if (-not ($live.Contains($stem) -or $live.Contains($alt))) {
            $doomed += $f
        }
    }
}

# --- the debug tree --------------------------------------------------------
#
# Everything here is built with --release; a debug tree is left over from a
# `cargo test` or a `cargo clippy` run without the flag, and it is large.
$debug = Join-Path $target 'debug'
if (-not $KeepDebug -and (Test-Path $debug)) {
    $debugFiles = Get-ChildItem $debug -Recurse -File -ErrorAction SilentlyContinue
    $doomed += $debugFiles
}

# --- incremental caches ----------------------------------------------------
foreach ($dir in @('release\incremental', 'debug\incremental')) {
    $inc = Join-Path $target $dir
    if (Test-Path $inc) {
        $doomed += Get-ChildItem $inc -Recurse -File -ErrorAction SilentlyContinue
    }
}

$doomed = $doomed | Sort-Object FullName -Unique
if (-not $doomed) {
    Write-Host "nothing to sweep" -ForegroundColor Green
    exit 0
}

$freed = Size $doomed
Write-Host ("{0} file(s), {1}" -f $doomed.Count, (Mb $freed)) -ForegroundColor DarkGray

if ($WhatIf) {
    $doomed | Select-Object -First 12 | ForEach-Object {
        Write-Host ("      {0,10:N0}  {1}" -f $_.Length, $_.FullName.Substring($target.Length + 1)) -ForegroundColor DarkGray
    }
    if ($doomed.Count -gt 12) { Write-Host "      ... and $($doomed.Count - 12) more" -ForegroundColor DarkGray }
    Write-Host "nothing was deleted (-WhatIf)" -ForegroundColor Yellow
    exit 0
}

$doomed | Remove-Item -Force -ErrorAction SilentlyContinue
if (-not $KeepDebug -and (Test-Path $debug)) {
    Remove-Item $debug -Recurse -Force -ErrorAction SilentlyContinue
}

$after = Size (Get-ChildItem $target -Recurse -File -ErrorAction SilentlyContinue)
Write-Host ("target is {0}, freed {1}" -f (Mb $after), (Mb ($before - $after))) -ForegroundColor Green
