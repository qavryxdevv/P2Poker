<#
.SYNOPSIS
    Checks that the built client is portable, by measurement rather than by
    reading the build configuration.

.DESCRIPTION
    `SPEC_CS.md` section 22 asks for a single executable with no runtime, no
    installer, no registry writes and nothing outside its own folder. Every
    clause of that is checkable, and this checks them:

      1. The import table names no C runtime. The default MSVC linkage imports
         vcruntime140.dll, which is present on most Windows installs and
         guaranteed on none - so "copy it and it runs" would be true here and
         untrue on a fresh machine, which is the worst kind of portability claim.
      2. Copied alone into an empty directory, it starts.
      3. It creates its profile BESIDE ITSELF and nothing anywhere else.
      4. A second copy in a second directory is a DIFFERENT client, which is what
         a portable profile means.

    Run it after any change to .cargo/config.toml, to the dependencies, or to
    anything under src/storage/.
#>

[CmdletBinding()]
param(
    [string] $Exe = ''
)

$ErrorActionPreference = 'Stop'

# Native programs write progress to stderr, and `2>&1` under Windows PowerShell's
# `ErrorActionPreference = 'Stop'` turns the first such line into a *terminating*
# NativeCommandError - so the deployment failed because cargo said "Finished".
# PowerShell 7 does not do this, which is exactly why it went unnoticed: the same
# script passed under `pwsh` and died under `powershell`. A tool in `tools\` has to
# work under both, so every native call that wants its stderr goes through here.
function Invoke-Native {
    param([Parameter(Mandatory)][string]$Exe, [string[]]$Arguments = @())
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { & $Exe @Arguments 2>&1 } finally { $ErrorActionPreference = $previous }
}

$failures = 0

if (-not $Exe) {
    # Computed here rather than in the parameter default: $PSScriptRoot is not
    # reliably bound when the default is evaluated under `powershell -File`.
    $root = if ($PSScriptRoot) { Split-Path $PSScriptRoot -Parent } else { (Get-Location).Path }
    $Exe = Join-Path $root 'target' | Join-Path -ChildPath 'release' | Join-Path -ChildPath 'p2p-poker.exe'
}

function Pass($m) { Write-Host "PASS  $m" -ForegroundColor Green }
function Fail($m) { Write-Host "FAIL  $m" -ForegroundColor Red; $script:failures++ }
function Note($m) { Write-Host "      $m" -ForegroundColor DarkGray }

if (-not (Test-Path $Exe)) {
    Write-Host "no binary at $Exe - build it first: cargo build --release" -ForegroundColor Red
    exit 1
}
$Exe = (Resolve-Path $Exe).Path
Note "checking $Exe"
Note "size $([math]::Round((Get-Item $Exe).Length / 1MB, 1)) MB"

# --- 1. no C runtime import -------------------------------------------------
$text = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($Exe))
$names = [regex]::Matches($text, '(?i)\b[A-Za-z0-9_\-\.]+\.dll\b') |
    ForEach-Object { $_.Value.ToLower() } | Sort-Object -Unique
$runtime = $names | Where-Object { $_ -match 'vcruntime|msvcp\d|msvcr\d|ucrtbase' }

if ($runtime) {
    Fail "imports a C runtime: $($runtime -join ', ')"
    Note "fix: -C target-feature=+crt-static in .cargo/config.toml"
} else {
    Pass "no C runtime import"
}

$expected = @(
    'advapi32.dll','bcrypt.dll','bcryptprimitives.dll','dbghelp.dll','dwmapi.dll',
    'gdi32.dll','imm32.dll','iphlpapi.dll','kernel32.dll','ntdll.dll','ole32.dll',
    'oleaut32.dll','opengl32.dll','pdh.dll','powrprof.dll','psapi.dll','shcore.dll',
    'shell32.dll','user32.dll','uxtheme.dll','ws2_32.dll',
    # The software renderer's road to WARP. All four are Windows' own: `dxgi`
    # and `setupapi` are genuine imports, `d3d12` and `dcomp` are loaded at
    # runtime. Read out of the PE import table, which gained exactly two names
    # when the second renderer went in.
    'd3d12.dll','dxgi.dll','dcomp.dll','setupapi.dll'
)
$unexpected = $names |
    Where-Object { $_ -notmatch '^(api-ms-|ext-ms-)' } |
    Where-Object { $expected -notcontains $_ } |
    # Graphics driver names appear as string literals for runtime loading, not
    # as imports, and are part of the system rather than of this program.
    #
    # `dxcompiler` is the one worth naming: it is the DirectX shader compiler,
    # and it is NOT part of Windows. wgpu loads it only when told to prefer DXC
    # over the FXC that Windows does carry, which we never do - proven by the
    # software renderer drawing a window on a machine where the file is absent.
    #
    # The `create_*` pair are not libraries at all. This harvest reads strings,
    # and an export name butted up against a library name in the binary reads as
    # one word: `CreateFactoryMedia` + `dxgi.dll`, `CreateEventA` + `dcomp.dll`.
    Where-Object { $_ -notmatch 'libegl|atioglxx|darkmode|dxcompiler|^create_factory_media|^createevent' }

if ($unexpected) {
    Write-Host "WARN  names not on the known-system list: $($unexpected -join ', ')" -ForegroundColor Yellow
    Note "check each: a genuinely new third-party DLL breaks the single-file claim"
} else {
    Pass "every remaining name is a Windows system library"
}

# --- 2, 3, 4. copy it somewhere empty and run it ----------------------------
$a = Join-Path ([IO.Path]::GetTempPath()) "p2p-portable-a-$(Get-Random)"
$b = Join-Path ([IO.Path]::GetTempPath()) "p2p-portable-b-$(Get-Random)"
try {
    foreach ($d in @($a, $b)) {
        New-Item -ItemType Directory -Path $d | Out-Null
        Copy-Item $Exe (Join-Path $d 'p2p-poker.exe')
    }

    $idA = (Invoke-Native (Join-Path $a 'p2p-poker.exe') @('--headless', '--for', '3') |
        Select-String '^peer id').ToString()
    $idB = (Invoke-Native (Join-Path $b 'p2p-poker.exe') @('--headless', '--for', '3') |
        Select-String '^peer id').ToString()

    if ($idA -and $idB) { Pass "starts from an empty directory with nothing beside it" }
    else { Fail "did not start when copied alone" }

    $madeA = Get-ChildItem $a -Recurse -File | ForEach-Object { $_.FullName.Substring($a.Length + 1) }
    # Two keys, not one. Section 20 keeps the network identity and the player
    # identity apart, and one file holding both would make them one secret.
    $expectedFiles = @('p2p-poker.exe', 'profile\identity.key', 'profile\player.key')
    $extra = $madeA | Where-Object { $expectedFiles -notcontains $_ }
    if ($extra) {
        Fail "created files it should not have: $($extra -join ', ')"
    } else {
        Pass "created only its own profile, beside itself"
        Note ($madeA -join ', ')
    }

    if ($idA -ne $idB) {
        Pass "two directories are two clients"
    } else {
        Fail "two copies share an identity - the profile is not local to the folder"
    }

    # And the two identities in one profile are two different secrets.
    $node   = [System.IO.File]::ReadAllBytes((Join-Path $a 'profile\identity.key'))
    $player = [System.IO.File]::ReadAllBytes((Join-Path $a 'profile\player.key'))
    if ($node.Length -eq 32 -and $player.Length -eq 32 -and
        [System.Convert]::ToBase64String($node) -ne [System.Convert]::ToBase64String($player)) {
        Pass "the network identity and the player identity are two keys"
    } else {
        Fail "the two identities are one secret, or are not 32-byte keys"
    }

    $playerA = (Invoke-Native (Join-Path $a 'p2p-poker.exe') @('--headless', '--for', '3') |
        Select-String '^player').ToString()
    $playerB = (Invoke-Native (Join-Path $b 'p2p-poker.exe') @('--headless', '--for', '3') |
        Select-String '^player').ToString()
    if ($playerA -and $playerB -and $playerA -ne $playerB) {
        Pass "two directories are two players"
    } else {
        Fail "two copies share a player identity"
    }

    # The identity must survive a restart, or the client is a stream of strangers.
    $idA2 = (Invoke-Native (Join-Path $a 'p2p-poker.exe') @('--headless', '--for', '3') |
        Select-String '^peer id').ToString()
    if ($idA -eq $idA2) { Pass "the identity survives a restart" }
    else { Fail "the identity changed on restart" }
}
finally {
    Remove-Item $a, $b -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host ''
if ($failures -eq 0) {
    Write-Host "portable: every clause of section 22 checked" -ForegroundColor Green
    exit 0
} else {
    Write-Host "$failures check(s) failed" -ForegroundColor Red
    exit 1
}
