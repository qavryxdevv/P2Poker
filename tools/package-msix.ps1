<#
.SYNOPSIS
    `D-080`: the MSIX package of a built client, for the Microsoft Store.

.DESCRIPTION
    Takes the Windows client that is already built -- target\release\p2p-poker.exe --
    and writes dist\P2Poker-<version>.msix beside the other packages.

        pwsh tools\package-msix.ps1 -IdentityName 12345Publisher.P2Poker `
             -Publisher 'CN=ABCDEF01-2345-6789-ABCD-EF0123456789' -PublisherDisplayName 'P2Poker'

    **The package is not signed here, and that is the point.** The Store signs it
    with Microsoft's own certificate after certification, which is why a player who
    installs it from the Store sees no SmartScreen warning and why this project
    needs no certificate of its own. A package for a local test must be signed to
    install at all: `-SelfSign` makes a throwaway certificate, signs with it, and
    writes the certificate beside the package so it can be trusted by hand on the
    test machine. A self-signed package is never what goes to the Store.

    **The three identity values come from Partner Center** -- the reserved name, the
    publisher string and the display name of the account the app is published under
    -- and are passed in rather than written here, because they belong to the
    owner's account and not to a public repository.

    The client itself asks Windows whether it runs from a package (`install::packaged`)
    and then updates through the Store, offers no installer, and keeps the player's
    profile in the user's local data folder.

    Needs the Windows SDK's makeappx.exe (every GitHub windows runner has it) and
    PowerShell 7.
#>
#Requires -Version 7.0

[CmdletBinding()]
param(
    # Left out, it is read from Cargo.toml. The Store wants four parts and a
    # revision of zero: 0.1.4 becomes 0.1.4.0.
    [string]$Version = '',
    # Partner Center: Product identity -> Package/Identity/Name.
    [string]$IdentityName = 'P2Poker',
    # Partner Center: Package/Identity/Publisher, which looks like CN=<guid>.
    [string]$Publisher = 'CN=P2Poker',
    # Partner Center: Package/Properties/PublisherDisplayName.
    [string]$PublisherDisplayName = 'P2Poker',
    # A throwaway signature, for installing the package on a test machine only.
    [switch]$SelfSign,
    [string]$Exe = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root 'target\release\p2p-poker.exe' }
if (-not (Test-Path $Exe)) { throw "no program at ${Exe}: build it first" }
if (-not $Version) {
    $line = Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version = "(.+)"$' | Select-Object -First 1
    if (-not $line) { throw 'Cargo.toml does not say the version' }
    $Version = $line.Matches[0].Groups[1].Value
}
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "the version $Version is not x.y.z" }
$packageVersion = "$Version.0"

# --- the staging folder -----------------------------------------------------
$work = Join-Path $root 'target\msix'
$dist = Join-Path $root 'dist'
if (Test-Path $work) { Remove-Item -Recurse -Force $work }
New-Item -ItemType Directory -Force -Path $work, (Join-Path $work 'Assets'), $dist | Out-Null
Copy-Item $Exe (Join-Path $work 'p2p-poker.exe')
Copy-Item (Join-Path $root 'LICENSE') (Join-Path $work 'LICENSE.txt')

# --- the logos, from the one icon the client already carries ----------------
Add-Type -AssemblyName System.Drawing
$icon = Join-Path $root 'assets\icon-256.png'
if (-not (Test-Path $icon)) { throw "no icon at $icon" }
$source = [System.Drawing.Image]::FromFile($icon)
try {
    foreach ($logo in @(
            @{ Name = 'Square44x44Logo.png'; W = 44; H = 44 },
            @{ Name = 'Square44x44Logo.targetsize-24_altform-unplated.png'; W = 24; H = 24 },
            @{ Name = 'Square150x150Logo.png'; W = 150; H = 150 },
            @{ Name = 'StoreLogo.png'; W = 50; H = 50 },
            @{ Name = 'Wide310x150Logo.png'; W = 310; H = 150 })) {
        $bmp = New-Object System.Drawing.Bitmap $logo.W, $logo.H
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        try {
            $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $g.Clear([System.Drawing.Color]::Transparent)
            # Square in the middle of a wide tile; the square tiles are filled.
            $side = [Math]::Min($logo.W, $logo.H)
            $x = [int](($logo.W - $side) / 2)
            $y = [int](($logo.H - $side) / 2)
            $g.DrawImage($source, $x, $y, $side, $side)
        }
        finally { $g.Dispose() }
        $bmp.Save((Join-Path $work "Assets\$($logo.Name)"), [System.Drawing.Imaging.ImageFormat]::Png)
        $bmp.Dispose()
    }
}
finally { $source.Dispose() }

# --- the manifest -----------------------------------------------------------
# `runFullTrust` is what a Win32 program in a package needs; the three network
# capabilities are what this client does and nothing more: it speaks to the
# internet, it listens for the peers that dial it back, and it finds the other
# clients on the same network by multicast (mDNS).
$escape = { param($s) [System.Security.SecurityElement]::Escape($s) }
$manifest = @"
<?xml version="1.0" encoding="utf-8"?>
<Package
  xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
  IgnorableNamespaces="uap rescap">

  <Identity Name="$(& $escape $IdentityName)" Publisher="$(& $escape $Publisher)" Version="$packageVersion" ProcessorArchitecture="x64" />

  <Properties>
    <DisplayName>P2Poker</DisplayName>
    <PublisherDisplayName>$(& $escape $PublisherDisplayName)</PublisherDisplayName>
    <Logo>Assets\StoreLogo.png</Logo>
  </Properties>

  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.19041.0" MaxVersionTested="10.0.26100.0" />
  </Dependencies>

  <Resources>
    <Resource Language="en-us" />
  </Resources>

  <Applications>
    <Application Id="P2Poker" Executable="p2p-poker.exe" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements
        DisplayName="P2Poker"
        Description="Decentralised poker: no house, no server, every card proven. Play money only."
        BackgroundColor="transparent"
        Square150x150Logo="Assets\Square150x150Logo.png"
        Square44x44Logo="Assets\Square44x44Logo.png">
        <uap:DefaultTile Wide310x150Logo="Assets\Wide310x150Logo.png" />
      </uap:VisualElements>
    </Application>
  </Applications>

  <Capabilities>
    <rescap:Capability Name="runFullTrust" />
    <Capability Name="internetClient" />
    <Capability Name="internetClientServer" />
    <Capability Name="privateNetworkClientServer" />
  </Capabilities>
</Package>
"@
Set-Content -Path (Join-Path $work 'AppxManifest.xml') -Value $manifest -Encoding UTF8

# --- makeappx ---------------------------------------------------------------
$makeappx = Get-ChildItem -Path 'C:\Program Files (x86)\Windows Kits\10\bin' -Filter 'makeappx.exe' -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName -Descending | Select-Object -First 1
if (-not $makeappx) { throw 'makeappx.exe was not found: the Windows SDK is not installed here' }
$package = Join-Path $dist "P2Poker-$Version-x64.msix"
if (Test-Path $package) { Remove-Item -Force $package }
& $makeappx.FullName pack /d $work /p $package /o | ForEach-Object { if ($_ -match 'error|warning') { Write-Host "    $_" } }
if ($LASTEXITCODE -ne 0) { throw "makeappx exited $LASTEXITCODE" }

if ($SelfSign) {
    # A test machine only: the Store's own signature is what a player gets.
    $cert = New-SelfSignedCertificate -Type Custom -Subject $Publisher -KeyUsage DigitalSignature `
        -FriendlyName 'P2Poker test signing' -CertStoreLocation 'Cert:\CurrentUser\My' `
        -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
    $signtool = Get-ChildItem -Path 'C:\Program Files (x86)\Windows Kits\10\bin' -Filter 'signtool.exe' -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName -Descending | Select-Object -First 1
    if (-not $signtool) { throw 'signtool.exe was not found' }
    & $signtool.FullName sign /fd SHA256 /sha1 $cert.Thumbprint $package | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "signtool exited $LASTEXITCODE" }
    Export-Certificate -Cert $cert -FilePath (Join-Path $dist "P2Poker-$Version-test.cer") | Out-Null
    Write-Host '    signed with a throwaway certificate: for a test machine, never for the Store'
}

"$package  $((Get-Item $package).Length) bytes, version $packageVersion"
