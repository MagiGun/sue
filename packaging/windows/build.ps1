# Builds Sue for Windows: a portable zip and an .msi installer, for one
# architecture.
#
#   powershell -File packaging/windows/build.ps1                  (x64)
#   powershell -File packaging/windows/build.ps1 -Arch arm64
#   powershell -File packaging/windows/build.ps1 -Arch x86
#
# Needs Rust with the MSVC build tools (and its target for -Arch). The
# installer needs WiX 5: `dotnet tool install --global wix --version 5.0.2`
# and `wix extension add -g WixToolset.UI.wixext/5.0.2`; without it only the
# zip is made. The version is SUE_VERSION when set (the Release workflow sets
# it from the version typed or the tag pushed), else Cargo.toml's.
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
Remove-Item -Recurse -Force (Split-Path $work) -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $out, $work | Out-Null

# Sue.exe opens the window with no console; sue-cli.exe is for a terminal.
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

# The installer. MSI versions are three numbers: 0.0.1-beta installs as 0.0.1.
$wix = Get-Command wix -ErrorAction SilentlyContinue
if (-not $wix) {
    Write-Warning "WiX is not installed: no .msi this time"
} else {
    $numbers = ($version -split "[^0-9.]")[0]
    # The license page shows the license as RTF.
    $text = (Get-Content LICENSE -Raw) -replace "\\", "\\" -replace "\{", "\{" -replace "\}", "\}"
    $rtf = "{\rtf1\ansi\deff0{\fonttbl{\f0 Segoe UI;}}\fs18 " + ($text -replace "`r?`n", "\par`n") + "}"
    Set-Content -Path (Join-Path $work "LICENSE.rtf") -Value $rtf -Encoding ascii
    $msi = Join-Path $out "Sue-$version-windows-$Arch.msi"
    & $wix.Source build packaging\windows\sue.wxs -arch $Arch -ext WixToolset.UI.wixext `
        -d "Version=$numbers" -d "Source=$work" -o $msi
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Remove-Item (Join-Path $out "Sue-$version-windows-$Arch.wixpdb") -ErrorAction SilentlyContinue
}
Get-ChildItem $out -Filter "Sue-$version-windows-$Arch*"
