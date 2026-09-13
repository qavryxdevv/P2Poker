<#
.SYNOPSIS
    Checks that a built binary names nothing of the machine it was built on.

.DESCRIPTION
    S1-EN. Reads the binary as single bytes and as UTF-16 at both alignments,
    with every slash read as a backslash, and looks for: the user profile's
    path, the user name as a path component, the computer name, the cargo home,
    the repository's path and its path inside the profile. Any of them fails,
    with how often and the first place it was seen.

    A Rust source path left in the binary means the remap is missing or stale:
    tools\remap-build-paths.ps1, then build again. A C source path means the
    compiler was given an absolute path (build.rs names them relative).
#>

[CmdletBinding()]
param([string]$Exe = '')

$ErrorActionPreference = 'Stop'

$root = if ($PSScriptRoot) { Split-Path $PSScriptRoot -Parent } else { (Get-Location).Path }
if (-not $Exe) { $Exe = Join-Path $root 'target\release\p2p-poker.exe' }
if (-not (Test-Path $Exe)) { Write-Host "no binary at $Exe" -ForegroundColor Red; exit 1 }
$Exe = (Resolve-Path $Exe).Path

$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$names = New-Object System.Collections.Generic.List[string]
foreach ($n in @($env:USERPROFILE, $cargoHome, $root, "\Users\$env:USERNAME\", $env:COMPUTERNAME)) {
    if ($n -and $n.Length -ge 6) { $names.Add($n.Replace('/', '\')) }
}
$profileRoot = if ($env:USERPROFILE) { $env:USERPROFILE.TrimEnd('\') + '\' } else { $null }
if ($profileRoot -and $root.StartsWith($profileRoot, [StringComparison]::OrdinalIgnoreCase)) {
    $names.Add($root.Substring($profileRoot.Length))
}

$bytes = [IO.File]::ReadAllBytes($Exe)
$even = $bytes.Length - ($bytes.Length % 2)
$odd = ($bytes.Length - 1) - (($bytes.Length - 1) % 2)
$views = @(
    [Text.Encoding]::GetEncoding(28591).GetString($bytes).Replace('/', '\'),
    [Text.Encoding]::Unicode.GetString($bytes, 0, $even).Replace('/', '\'),
    [Text.Encoding]::Unicode.GetString($bytes, 1, $odd).Replace('/', '\')
)

$failed = 0
foreach ($name in ($names | Sort-Object -Unique)) {
    $count = 0
    $example = $null
    foreach ($view in $views) {
        $i = $view.IndexOf($name, [StringComparison]::OrdinalIgnoreCase)
        while ($i -ge 0) {
            $count++
            if (-not $example) {
                $from = [Math]::Max(0, $i - 30)
                $example = $view.Substring($from, [Math]::Min(140, $view.Length - $from)) -replace '[^\x20-\x7e]', '.'
            }
            $i = $view.IndexOf($name, $i + $name.Length, [StringComparison]::OrdinalIgnoreCase)
        }
    }
    if ($count -gt 0) {
        $failed++
        Write-Host ("FAIL  {0} occurrence(s) of {1}" -f $count, $name) -ForegroundColor Red
        Write-Host "      first: $example" -ForegroundColor DarkGray
    }
}

if ($failed -eq 0) {
    Write-Host ("PASS  {0} names nothing of the machine it was built on ({1} names looked for)" -f (Split-Path $Exe -Leaf), $names.Count) -ForegroundColor Green
    exit 0
}
exit 1
