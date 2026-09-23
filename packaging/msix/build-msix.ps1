# Build the Store MSIX from a release MDW.exe. Everything stays on the output
# drive (default T:\MDW) - nothing is staged on C:.
#   .\build-msix.ps1 -Exe T:\MDW\0.1.0\MDW-portable.exe -Icons T:\MDW\icons -Version 0.1.0
param(
  [Parameter(Mandatory)] [string]$Exe,
  [Parameter(Mandatory)] [string]$Icons,
  [string]$Version = "0.1.0",
  [string]$Out = "T:\MDW\$Version"
)
$ErrorActionPreference = "Stop"
$makeappx = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" | Sort-Object FullName | Select-Object -Last 1
if (-not $makeappx) { throw "Windows SDK makeappx.exe not found" }

$layout = Join-Path $Out "msix-layout"
Remove-Item $layout -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$layout\Assets" | Out-Null
Copy-Item $Exe "$layout\MDW.exe"
foreach ($a in "StoreLogo", "Square44x44Logo", "Square150x150Logo", "Square310x310Logo") {
  Copy-Item (Join-Path $Icons "$a.png") "$layout\Assets\$a.png"
}
(Get-Content (Join-Path $PSScriptRoot "AppxManifest.xml") -Raw).Replace("__VERSION__", $Version) |
  Set-Content "$layout\AppxManifest.xml" -Encoding utf8

$msix = Join-Path $Out "MDW_${Version}_x64.msix"
& $makeappx.FullName pack /d $layout /p $msix /o
if ($LASTEXITCODE -ne 0) { throw "makeappx failed ($LASTEXITCODE)" }
Write-Host "Built $msix (unsigned - Partner Center signs Store uploads)"
