<#
.SYNOPSIS
    Puts this machine's --remap-path-prefix flags into the cargo home's
    config.toml, so a release names nothing of the machine it was built on.

.DESCRIPTION
    S1-EN. rustc writes the source path of every panic location and trace point
    into the binary, and a dependency's path is absolute: the cargo home, under
    the user profile, with the user name in it. Cargo's trim-paths is not stable,
    and rustc's --remap-path-prefix has to name the paths it hides, so the flags
    cannot be committed. This writes them where they belong, into this machine's
    cargo configuration, between two marker lines that a later run replaces.
    build.rs refuses a release build without them, and tools\check-build-paths.ps1
    reads a built binary for anything they missed.

    Mapped most general first, because rustc takes the last flag that matches:

        the user profile         ~
        the cargo home           /cargo
        the rustup home          /rustup
        this repository          /p2p-poker
        the target directory     /target     (only when it is outside the repository)

    Run it again after moving the repository. -Remove takes the block out.
    Changing the flags rebuilds every dependency once.
#>

[CmdletBinding()]
param([switch]$Remove)

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE '.rustup' }
$targetDir = if ($env:CARGO_TARGET_DIR) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR) } else { Join-Path $root 'target' }

$maps = New-Object System.Collections.Generic.List[object]
$maps.Add(@($env:USERPROFILE, '~'))
$maps.Add(@($cargoHome, '/cargo'))
$maps.Add(@($rustupHome, '/rustup'))
$maps.Add(@($root, '/p2p-poker'))
if (-not $targetDir.StartsWith($root.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
    $maps.Add(@($targetDir, '/target'))
}

$config = Join-Path $cargoHome 'config.toml'
$begin = '# p2p-poker remap-build-paths: begin (written by tools/remap-build-paths.ps1, S1-EN)'
$end = '# p2p-poker remap-build-paths: end'

$text = if (Test-Path $config) { [IO.File]::ReadAllText($config) } else { '' }
$pattern = '(?s)' + [regex]::Escape($begin) + '.*?' + [regex]::Escape($end)
$rest = [regex]::Replace($text, $pattern, '').Trim()

if (-not $Remove) {
    if ($rest -match "(?m)^\s*\[target\.'cfg\(all\(\)\)'\]") {
        throw "$config already has a [target.'cfg(all())'] table of its own: add the flags to its rustflags by hand"
    }
    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add($begin)
    $lines.Add("[target.'cfg(all())']")
    $lines.Add('rustflags = [')
    foreach ($m in $maps) {
        if ($m[0].Contains("'")) { throw "a path with a quote in it cannot be a TOML literal string: $($m[0])" }
        $lines.Add("    '--remap-path-prefix=$($m[0])=$($m[1])',")
    }
    $lines.Add(']')
    $lines.Add($end)
    $block = $lines -join "`n"
    $rest = if ($rest) { $rest + "`n`n" + $block } else { $block }
}

New-Item -ItemType Directory -Force -Path $cargoHome | Out-Null
[IO.File]::WriteAllText($config, $rest + "`n", (New-Object Text.UTF8Encoding $false))
Write-Host "wrote $config"
Get-Content $config | ForEach-Object { Write-Host "    $_" }
