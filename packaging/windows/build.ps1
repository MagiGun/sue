# Builds Sue for Windows: the release program and a zip to carry it in.
#
#   powershell -File packaging/windows/build.ps1
#
# Needs Rust with the MSVC build tools. The program runs from anywhere;
# its settings live in %APPDATA%\Sue.
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $root
$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"').Matches[0].Groups[1].Value
$arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "arm64" } else { "x64" }
$out = Join-Path $root "dist"
$work = Join-Path $root "target\windows-package\Sue"
Remove-Item -Recurse -Force (Split-Path $work) -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $out, $work | Out-Null

cargo build --release -p sue
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Copy-Item target\release\sue.exe (Join-Path $work "Sue.exe")
Copy-Item README.md $work
Copy-Item assets\icons\sue_icon.ico $work

$zip = Join-Path $out "Sue-$version-windows-$arch.zip"
Remove-Item $zip -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $work "*") -DestinationPath $zip
Get-ChildItem $out
