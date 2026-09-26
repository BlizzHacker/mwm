# Build a Store MSIX from a Defender-cleared release executable.
#   .\build-msix.ps1 -Exe <path-to-MoveWeightManager.exe> -Version 4.0.0
param(
  [Parameter(Mandatory)] [string]$Exe,
  [string]$Icons = "",
  [string]$Version = "4.0.0",
  [string]$Out = ""
)
$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
if (-not $Icons) { $Icons = Join-Path $repo "app\src-tauri\icons" }
if (-not $Out) { $Out = Join-Path $repo "dist\msix\$Version" }
if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) { throw "Executable not found: $Exe" }
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Version must be major.minor.patch" }
$makeappx = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" | Sort-Object FullName | Select-Object -Last 1
if (-not $makeappx) { throw "Windows SDK makeappx.exe not found" }

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$layout = Join-Path $Out ("msix-layout-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force "$layout\Assets" | Out-Null
Copy-Item $Exe "$layout\MoveWeightManager.exe"
foreach ($a in "StoreLogo", "Square44x44Logo", "Square150x150Logo", "Square310x310Logo") {
  Copy-Item (Join-Path $Icons "$a.png") "$layout\Assets\$a.png"
}
(Get-Content (Join-Path $PSScriptRoot "AppxManifest.xml") -Raw).Replace("__VERSION__", $Version) |
  Set-Content "$layout\AppxManifest.xml" -Encoding utf8

$msix = Join-Path $Out "MWM_${Version}_x64.msix"
& $makeappx.FullName pack /d $layout /p $msix /o
if ($LASTEXITCODE -ne 0) { throw "makeappx failed ($LASTEXITCODE)" }
Write-Host "Built $msix (unsigned - Partner Center signs Store uploads)"
