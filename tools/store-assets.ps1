<#
.SYNOPSIS
    `D-080`: the pictures the Microsoft Store listing is made of.

.DESCRIPTION
    Writes `dist\store`: five screenshots of the client and the two logo images the
    Store asks for.

        pwsh tools\store-assets.ps1

    **Every picture is made from the program itself, and none of them needs a game.**
    The screenshots come from `--table-preview` and `--album-preview`, which start no
    node and read no profile, so this can be run while somebody is playing on the same
    machine and nothing of theirs is touched. The two logos are drawn around
    `assets\icon.png`, so the listing and the program's own icon cannot drift apart.

    The table's pictures carry the sample's banner -- *PREVIEW: no hand is in progress,
    these cards are a sample* (spec section 22) -- and that band belongs in the Store as
    much as in the program: the hand in the picture was never played.

    The files themselves are not kept in this repository. They are 7 MB of pictures that
    go stale the moment the table is redrawn, and Partner Center already holds the ones
    that are published; this script is what makes them again, and it is the thing worth
    keeping.

    Needs the built client (`target\release\p2p-poker.exe`), a desktop to open a window
    on, and PowerShell 7.
#>
#Requires -Version 7.0

[CmdletBinding()]
param(
    # Where to write the pictures.
    [string]$Out = '',
    # The client to photograph. Left out, the release build of this tree.
    [string]$Exe = '',
    # The window's size. The Store wants 1366 x 768 or larger; this fits a 1920 x 1080
    # screen with room for the taskbar, which `PrintWindow` needs it to have.
    [int]$Wide = 1600,
    [int]$High = 900
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root 'target\release\p2p-poker.exe' }
if (-not $Out) { $Out = Join-Path $root 'dist\store' }
if (-not (Test-Path $Exe)) { throw "no program at ${Exe}: build it first" }
if (-not (Test-Path $Out)) { New-Item -ItemType Directory -Path $Out -Force | Out-Null }

Add-Type -AssemblyName System.Drawing
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class StoreShot {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hh, bool repaint);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
}
'@
[StoreShot]::SetProcessDPIAware() | Out-Null

# One preview, photographed and closed again. `PrintWindow` with PW_RENDERFULLCONTENT
# reads the window's own composed surface, so nothing that happens to lie over it ends
# up in the picture.
function Save-Preview([string]$name, [string[]]$flags) {
    $client = Start-Process -FilePath $Exe -ArgumentList $flags -PassThru
    try {
        Start-Sleep -Seconds 5
        $client.Refresh()
        $window = $client.MainWindowHandle
        if ($window -eq [IntPtr]::Zero) { throw "the preview $name opened no window" }
        [StoreShot]::MoveWindow($window, 0, 0, $Wide, $High, $true) | Out-Null
        [StoreShot]::SetForegroundWindow($window) | Out-Null
        Start-Sleep -Milliseconds 2000
        $box = New-Object StoreShot+RECT
        [StoreShot]::GetWindowRect($window, [ref]$box) | Out-Null
        $w = $box.R - $box.L
        $h = $box.B - $box.T
        $bmp = New-Object System.Drawing.Bitmap $w, $h
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $hdc = $g.GetHdc()
        [StoreShot]::PrintWindow($window, $hdc, 2) | Out-Null
        $g.ReleaseHdc($hdc)
        $file = Join-Path $Out "$name.png"
        $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
        $g.Dispose()
        $bmp.Dispose()
        "{0}  {1}x{2}" -f $file, $w, $h
    }
    finally {
        Stop-Process -Id $client.Id -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 800
    }
}

$iconFile = Join-Path $root 'assets\icon.png'
$icon = [System.Drawing.Image]::FromFile($iconFile)
$probe = New-Object System.Drawing.Bitmap $icon
# The felt's green, read out of the icon's own table rather than written down here.
$felt = $probe.GetPixel(200, 820)
$probe.Dispose()
$gold = [System.Drawing.Color]::FromArgb(232, 185, 48)

function Get-Shade([System.Drawing.Color]$colour, [double]$part) {
    [System.Drawing.Color]::FromArgb(
        [Math]::Min(255, [int]($colour.R * $part)),
        [Math]::Min(255, [int]($colour.G * $part)),
        [Math]::Min(255, [int]($colour.B * $part)))
}

# The icon on a felt gradient, the name under it. The Store shows this one as the
# product's logo, so it says what the product is in the two lines a tile has room for.
function Save-Logo([int]$w, [int]$h, [string]$name, [double]$iconPart, [double]$iconCentre) {
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.InterpolationMode = 'HighQualityBicubic'
    $g.TextRenderingHint = 'ClearTypeGridFit'

    $whole = New-Object System.Drawing.Rectangle 0, 0, $w, $h
    $felted = New-Object System.Drawing.Drawing2D.LinearGradientBrush $whole, (Get-Shade $felt 1.15), (Get-Shade $felt 0.35), 70.0
    $g.FillRectangle($felted, $whole)

    $side = [int]($w * $iconPart)
    $cx = [int]($w / 2)
    $cy = [int]($h * $iconCentre)
    $ring = [int]($side * 1.55)
    $halo = New-Object System.Drawing.Drawing2D.GraphicsPath
    $halo.AddEllipse(($cx - $ring / 2), ($cy - $ring / 2), $ring, $ring)
    $light = New-Object System.Drawing.Drawing2D.PathGradientBrush $halo
    $light.CenterColor = [System.Drawing.Color]::FromArgb(70, 255, 255, 255)
    $light.SurroundColors = @([System.Drawing.Color]::FromArgb(0, 255, 255, 255))
    $g.FillPath($light, $halo)
    $g.DrawImage($icon, ($cx - $side / 2), ($cy - $side / 2), $side, $side)

    $white = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::White)
    $quiet = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(205, 222, 232, 224))
    $goldBrush = New-Object System.Drawing.SolidBrush $gold
    $middle = New-Object System.Drawing.StringFormat
    $middle.Alignment = 'Center'

    $titleSize = [float]($w * 0.115)
    $title = New-Object System.Drawing.Font 'Segoe UI', $titleSize, ([System.Drawing.FontStyle]::Bold), ([System.Drawing.GraphicsUnit]::Pixel)
    $titleY = $cy + $side / 2 + $h * 0.055
    $g.DrawString('P2Poker', $title, $white, (New-Object System.Drawing.RectangleF 0, $titleY, $w, ($h - $titleY)), $middle)

    $ruleY = $titleY + $titleSize * 1.45
    $pen = New-Object System.Drawing.Pen $gold, ([float]($w * 0.005))
    $g.DrawLine($pen, ($w * 0.36), $ruleY, ($w * 0.64), $ruleY)

    $subSize = [float]($w * 0.042)
    $sub = New-Object System.Drawing.Font 'Segoe UI', $subSize, ([System.Drawing.FontStyle]::Regular), ([System.Drawing.GraphicsUnit]::Pixel)
    $g.DrawString('Peer-to-peer Texas Hold''em', $sub, $quiet, (New-Object System.Drawing.RectangleF 0, ($ruleY + $h * 0.022), $w, ($h * 0.2)), $middle)

    $footSize = [float]($w * 0.032)
    $foot = New-Object System.Drawing.Font 'Segoe UI', $footSize, ([System.Drawing.FontStyle]::Regular), ([System.Drawing.GraphicsUnit]::Pixel)
    $g.DrawString('No server  ·  no account  ·  play money', $foot, $goldBrush, (New-Object System.Drawing.RectangleF 0, ($ruleY + $h * 0.022 + $subSize * 2.0), $w, ($h * 0.2)), $middle)

    $file = Join-Path $Out $name
    $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose()
    $bmp.Dispose()
    "{0}  {1}x{2}" -f $file, $w, $h
}

Save-Preview 'table-hand'   @('--table-preview', '--preview-seats', '6', '--preview-odds')
Save-Preview 'table-winner' @('--table-preview', '--preview-seats', '6', '--preview-winner')
Save-Preview 'table-chat'   @('--table-preview', '--preview-seats', '6', '--preview-panels', '--preview-chat')
Save-Preview 'table-heads'  @('--table-preview', '--preview-seats', '2')
Save-Preview 'album'        @('--album-preview', '--preview-all')
Save-Logo 720 1080 'poster-9x16.png' 0.56 0.38
Save-Logo 1080 1080 'cover-1x1.png' 0.44 0.35
$icon.Dispose()
