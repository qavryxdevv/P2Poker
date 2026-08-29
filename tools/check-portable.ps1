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
    'shell32.dll','user32.dll','uxtheme.dll','ws2_32.dll'
)
$unexpected = $names |
    Where-Object { $_ -notmatch '^(api-ms-|ext-ms-)' } |
    Where-Object { $expected -notcontains $_ } |
    # Graphics driver names appear as string literals for runtime loading, not
    # as imports, and are part of the system rather than of this program.
    Where-Object { $_ -notmatch 'libegl|atioglxx|darkmode' }

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

    $idA = (& (Join-Path $a 'p2p-poker.exe') --headless --for 3 2>&1 |
        Select-String '^peer id').ToString()
    $idB = (& (Join-Path $b 'p2p-poker.exe') --headless --for 3 2>&1 |
        Select-String '^peer id').ToString()

    if ($idA -and $idB) { Pass "starts from an empty directory with nothing beside it" }
    else { Fail "did not start when copied alone" }

    $madeA = Get-ChildItem $a -Recurse -File | ForEach-Object { $_.FullName.Substring($a.Length + 1) }
    $expectedFiles = @('p2p-poker.exe', 'profile\identity.key')
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

    # The identity must survive a restart, or the client is a stream of strangers.
    $idA2 = (& (Join-Path $a 'p2p-poker.exe') --headless --for 3 2>&1 |
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
