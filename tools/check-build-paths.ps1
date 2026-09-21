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
# D-078: the Linux build's program has no .exe.
if (-not $Exe) { $Exe = Join-Path $root $(if ($IsLinux) { 'target/release/p2p-poker' } else { 'target\release\p2p-poker.exe' }) }
if (-not (Test-Path $Exe)) { Write-Host "no binary at $Exe" -ForegroundColor Red; exit 1 }
$Exe = (Resolve-Path $Exe).Path

# D-078: the same names on Linux, where the profile is HOME, the user USER and the machine its host name.
$userHome = if ($env:USERPROFILE) { $env:USERPROFILE } else { $env:HOME }
$userName = if ($env:USERNAME) { $env:USERNAME } elseif ($env:USER) { $env:USER } else { [Environment]::UserName }
$machine = if ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { [Environment]::MachineName }
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $userHome '.cargo' }
$names = New-Object System.Collections.Generic.List[string]
foreach ($n in @($userHome, $cargoHome, $root, "\Users\$userName\", "/home/$userName/", $machine)) {
    if ($n -and $n.Length -ge 6) { $names.Add($n.Replace('/', '\')) }
}
$profileRoot = if ($userHome) { $userHome.Replace('/', '\').TrimEnd('\') + '\' } else { $null }
$rootSeen = $root.Replace('/', '\')
if ($profileRoot -and $rootSeen.StartsWith($profileRoot, [StringComparison]::OrdinalIgnoreCase)) {
    $names.Add($rootSeen.Substring($profileRoot.Length))
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
