# Writes Sue's winget manifests (MagiGun.Sue) for a published release, from
# its .msi files: the version, the store-page text and the installers, one
# per architecture with its link, checksum and product code. The winget
# workflow sends them to microsoft/winget-pkgs, after which
#
#   winget install MagiGun.Sue
#
# installs Sue, without the warning Windows shows for a download from a
# browser.
#
#   powershell -File packaging/winget/manifests.ps1 -Tag v0.0.3 -Folder <the .msi files> -Out <folder>
param(
    [Parameter(Mandatory = $true)][string]$Tag,
    [Parameter(Mandatory = $true)][string]$Folder,
    [Parameter(Mandatory = $true)][string]$Out
)
$ErrorActionPreference = "Stop"
$version = $Tag.TrimStart("v")
$repository = "https://github.com/MagiGun/sue"
$manifestVersion = "1.9.0"
New-Item -ItemType Directory -Force $Out | Out-Null

# The product code Windows lists the installed .msi under.
function Get-ProductCode([string]$Path) {
    $installer = New-Object -ComObject WindowsInstaller.Installer
    $database = $installer.GetType().InvokeMember("OpenDatabase", "InvokeMethod", $null, $installer, @($Path, 0))
    $view = $database.GetType().InvokeMember("OpenView", "InvokeMethod", $null, $database, @("SELECT Value FROM Property WHERE Property = 'ProductCode'"))
    $view.GetType().InvokeMember("Execute", "InvokeMethod", $null, $view, $null) | Out-Null
    $record = $view.GetType().InvokeMember("Fetch", "InvokeMethod", $null, $view, $null)
    $code = $record.GetType().InvokeMember("StringData", "GetProperty", $null, $record, @([int]1))
    $view.GetType().InvokeMember("Close", "InvokeMethod", $null, $view, $null) | Out-Null
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($installer) | Out-Null
    $code
}

$installers = foreach ($arch in "x64", "x86", "arm64") {
    $name = "Sue-$version-windows-$arch.msi"
    $path = Join-Path $Folder $name
    if (-not (Test-Path $path)) { continue }
    $sha = (Get-FileHash -Algorithm SHA256 $path).Hash
    $code = Get-ProductCode (Resolve-Path $path).Path
@"
- Architecture: $arch
  InstallerUrl: $repository/releases/download/$Tag/$name
  InstallerSha256: $sha
  ProductCode: '$code'
"@
}
if (-not $installers) { throw "no Sue-$version-windows-*.msi in $Folder" }

$header = "PackageIdentifier: MagiGun.Sue`nPackageVersion: $version"
$files = @{
    "MagiGun.Sue.yaml" = @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.version.$manifestVersion.schema.json
$header
DefaultLocale: en-US
ManifestType: version
ManifestVersion: $manifestVersion
"@
    "MagiGun.Sue.locale.en-US.yaml" = @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.defaultLocale.$manifestVersion.schema.json
$header
PackageLocale: en-US
Publisher: MagiGun
PublisherUrl: https://github.com/MagiGun
PublisherSupportUrl: $repository/issues
PackageName: Sue
PackageUrl: https://magigun.github.io/sue/
License: Sayn License
LicenseUrl: $repository/blob/main/LICENSE
ShortDescription: One creative environment for painting, comics, animation, writing and sound.
Description: Sue, the Singular Unified Engine, is one creative environment for painting, comics, animation, writing and sound. Every tool works on the same project, so a picture, a page and a sound are the same things wherever you use them. No account and no cloud.
Moniker: sue
Tags:
- animation
- comics
- drawing
- illustration
- painting
- writing
ReleaseNotesUrl: $repository/releases/tag/$Tag
ManifestType: defaultLocale
ManifestVersion: $manifestVersion
"@
    "MagiGun.Sue.installer.yaml" = @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.installer.$manifestVersion.schema.json
$header
InstallerType: wix
Scope: machine
UpgradeBehavior: install
AppsAndFeaturesEntries:
- UpgradeCode: '{C7387274-121F-4150-B23C-49B90A593624}'
Installers:
$($installers -join "`n")
ManifestType: installer
ManifestVersion: $manifestVersion
"@
}
foreach ($file in $files.Keys) {
    # winget wants UTF-8 without a byte order mark and plain line ends.
    $text = ($files[$file] -replace "`r`n", "`n") + "`n"
    [System.IO.File]::WriteAllText((Join-Path $Out $file), $text, (New-Object System.Text.UTF8Encoding $false))
}
Get-ChildItem $Out | Select-Object -ExpandProperty Name
