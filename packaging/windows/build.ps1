# Builds Sue for Windows, for one architecture: a portable zip, an .msi
# installer, and a setup .exe (the .msi inside a program that carries Sue's
# icon, since Windows draws every .msi file with its own icon).
#
#   powershell -File packaging/windows/build.ps1                  (x64)
#   powershell -File packaging/windows/build.ps1 -Arch arm64
#   powershell -File packaging/windows/build.ps1 -Arch x86
#
# Needs Rust with the MSVC build tools (and its target for -Arch). The
# installers need WiX 5: `dotnet tool install --global wix --version 5.0.2`,
# then `wix extension add -g WixToolset.UI.wixext/5.0.2` and
# `wix extension add -g WixToolset.BootstrapperApplications.wixext/5.0.2`;
# without it only the zip is made. The version is SUE_VERSION when set (the
# Release workflow sets it from the version typed or the tag pushed), else
# Cargo.toml's.
param(
    [ValidateSet("x64", "x86", "arm64")]
    [string]$Arch = "x64"
)
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $root
$version = if ($env:SUE_VERSION) { $env:SUE_VERSION.TrimStart("v") } else {
    (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"').Matches[0].Groups[1].Value
}
$env:SUE_VERSION = $version
$target = @{ x64 = "x86_64-pc-windows-msvc"; x86 = "i686-pc-windows-msvc"; arm64 = "aarch64-pc-windows-msvc" }[$Arch]
$out = Join-Path $root "dist"
$work = Join-Path $root "target\windows-package\$Arch\Sue"
$art = Join-Path $root "target\windows-package\$Arch\installer"
Remove-Item -Recurse -Force (Split-Path $work) -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $out, $work, $art | Out-Null

# Sue.exe opens the window with no console; sue-cli.exe is for a terminal.
# Both carry Sue's icon (bins/sue/build.rs).
cargo build --release -p sue --bins --target $target
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Copy-Item "target\$target\release\sue.exe" (Join-Path $work "Sue.exe")
Copy-Item "target\$target\release\sue-cli.exe" (Join-Path $work "sue-cli.exe")
Copy-Item README.md $work
Copy-Item LICENSE (Join-Path $work "LICENSE.txt")
Copy-Item assets\icons\sue_icon.ico $work

$zip = Join-Path $out "Sue-$version-windows-$Arch-portable.zip"
Remove-Item $zip -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $work "*") -DestinationPath $zip

$wix = Get-Command wix -ErrorAction SilentlyContinue
if (-not $wix) {
    Write-Warning "WiX is not installed: no .msi or setup .exe this time"
    Get-ChildItem $out -Filter "Sue-$version-windows-$Arch*"
    exit 0
}

# What the installers show beside the files: the license as RTF, one
# paragraph for each of LICENSE's (its lines joined, so the text wraps to
# the page instead of breaking at every line), and Sue's icon on the
# pages (a banner on top, a panel on the left of the first and last page,
# and the setup program's logo).
$text = (Get-Content LICENSE -Raw) -replace "\\", "\\" -replace "\{", "\{" -replace "\}", "\}"
$paragraphs = @($text.Trim() -split "\r?\n\s*\r?\n" | ForEach-Object { ($_.Trim() -split "\s*\r?\n\s*") -join " " })
$paragraphs[0] = "{\b " + $paragraphs[0] + "}"
$rtf = "{\rtf1\ansi\deff0{\fonttbl{\f0\fswiss Segoe UI;}}\pard\sa180\f0\fs18 " + ($paragraphs -join "\par`r`n") + "\par}"
Set-Content -Path (Join-Path $art "LICENSE.rtf") -Value $rtf -Encoding ascii
Copy-Item assets\icons\sue_icon.ico $art

Add-Type -AssemblyName System.Drawing
$icon = [System.Drawing.Image]::FromFile((Join-Path $root "assets\icons\sue_icon_512.png"))
function New-Picture([int]$Width, [int]$Height, [scriptblock]$Paint, [string]$Path, $Format) {
    $picture = New-Object System.Drawing.Bitmap $Width, $Height, ([System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
    if ($Format -eq [System.Drawing.Imaging.ImageFormat]::Png) {
        $picture.Dispose()
        $picture = New-Object System.Drawing.Bitmap $Width, $Height, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    }
    $graphics = [System.Drawing.Graphics]::FromImage($picture)
    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $graphics.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
    & $Paint $graphics
    $graphics.Dispose()
    $picture.Save($Path, $Format)
    $picture.Dispose()
}
$bmp = [System.Drawing.Imaging.ImageFormat]::Bmp
# The banner over the inner pages: white, the title on its left, the icon
# on its right.
New-Picture 493 58 {
    param($g)
    $g.Clear([System.Drawing.Color]::White)
    $g.DrawImage($icon, 493 - 44 - 10, 7, 44, 44)
} (Join-Path $art "banner.bmp") $bmp
# The first and last page: a dark panel with the icon and the name on the
# left, white where the text goes.
New-Picture 493 312 {
    param($g)
    $g.Clear([System.Drawing.Color]::White)
    $panel = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(11, 11, 11))
    $g.FillRectangle($panel, 0, 0, 164, 312)
    $g.DrawImage($icon, 26, 72, 112, 112)
    $font = New-Object System.Drawing.Font "Segoe UI Semibold", 20, ([System.Drawing.GraphicsUnit]::Pixel)
    $format = New-Object System.Drawing.StringFormat
    $format.Alignment = [System.Drawing.StringAlignment]::Center
    $g.DrawString("Sue", $font, [System.Drawing.Brushes]::White, (New-Object System.Drawing.RectangleF 0, 196, 164, 32), $format)
} (Join-Path $art "dialog.bmp") $bmp
New-Picture 64 64 {
    param($g)
    $g.Clear([System.Drawing.Color]::Transparent)
    $g.DrawImage($icon, 0, 0, 64, 64)
} (Join-Path $art "logo.png") ([System.Drawing.Imaging.ImageFormat]::Png)
$icon.Dispose()

# The .msi. Its versions are three numbers: 0.0.1-beta installs as 0.0.1.
$numbers = ($version -split "[^0-9.]")[0]
$msi = Join-Path $out "Sue-$version-windows-$Arch.msi"
& $wix.Source build packaging\windows\sue.wxs -arch $Arch -ext WixToolset.UI.wixext `
    -d "Version=$numbers" -d "Source=$work" -d "Art=$art" -o $msi
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# The setup program, with the .msi inside.
$setup = Join-Path $out "Sue-$version-windows-$Arch-setup.exe"
& $wix.Source build packaging\windows\setup.wxs -arch $Arch -ext WixToolset.BootstrapperApplications.wixext `
    -d "Version=$numbers" -d "Source=$art" -d "Msi=$msi" -o $setup
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Remove-Item (Join-Path $out "*.wixpdb") -ErrorAction SilentlyContinue
Get-ChildItem $out -Filter "Sue-$version-windows-$Arch*"
