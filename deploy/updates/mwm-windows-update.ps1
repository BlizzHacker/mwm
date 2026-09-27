# Portable MWM updater. Windows Store MSIX installs use Store updates instead.
# Run under the interactive user's account from Task Scheduler.
$ErrorActionPreference = 'Stop'
$installDir = Join-Path $env:LOCALAPPDATA 'MWM'
$exe = Join-Path $installDir 'MoveWeightManager.exe'
$stage = Join-Path $installDir 'MoveWeightManager.exe.new'
$previous = Join-Path $installDir 'MoveWeightManager.exe.previous'
$state = Join-Path $installDir 'updates'
$pending = Join-Path $state 'pending.json'
$repo = 'BlizzHacker/mwm'
if (-not (Test-Path -LiteralPath $exe)) { throw "Portable MWM install missing: $exe" }
New-Item -ItemType Directory -Path $state -Force | Out-Null

function Test-MwmRunning {
    return [bool](Get-Process -Name 'MoveWeightManager' -ErrorAction SilentlyContinue)
}
function Get-InstalledVersion {
    $text = (Get-Item -LiteralPath $exe).VersionInfo.ProductVersion
    if ($text -notmatch '^\d+\.\d+\.\d+$') { throw "Invalid installed MWM version" }
    return [version]$text
}
function Install-Staged {
    if (-not (Test-Path -LiteralPath $pending)) { return }
    $record = Get-Content -LiteralPath $pending -Raw | ConvertFrom-Json
    if ($record.version -notmatch '^\d+\.\d+\.\d+$' -or $record.sha256 -notmatch '^[0-9a-fA-F]{64}$') {
        throw 'Invalid pending update metadata'
    }
    if (-not (Test-Path -LiteralPath $stage)) { Remove-Item -LiteralPath $pending; return }
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $stage).Hash -ne $record.sha256) {
        throw 'Staged MWM checksum mismatch'
    }
    if ((Get-Item -LiteralPath $stage).VersionInfo.ProductVersion -ne $record.version) {
        throw 'Staged MWM version mismatch'
    }
    if (Test-MwmRunning) { Write-Output 'MWM is running; verified update remains staged'; return }
    if ([version]$record.version -le (Get-InstalledVersion)) {
        Remove-Item -LiteralPath $stage,$pending
        return
    }
    Copy-Item -LiteralPath $exe -Destination $previous -Force
    try {
        Move-Item -LiteralPath $stage -Destination $exe -Force
        if ((Get-InstalledVersion).ToString(3) -ne $record.version) { throw 'Installed version mismatch' }
        Remove-Item -LiteralPath $pending
        Write-Output "Updated portable MWM to $($record.version)"
    } catch {
        Copy-Item -LiteralPath $previous -Destination $exe -Force
        throw
    }
}

Install-Staged
$current = Get-InstalledVersion
$drive = (Get-Item -LiteralPath $installDir).PSDrive
if ($drive.Free -lt 512MB) { Write-Output 'Skipped download: system drive has less than 512 MB free'; exit 0 }
$release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -Headers @{ 'User-Agent' = 'MWM-Portable-Updater' } -TimeoutSec 30
$tag = [string]$release.tag_name
if ($tag -notmatch '^v(\d+\.\d+\.\d+)$') { throw 'Unexpected release tag' }
$latest = [version]$Matches[1]
if ($latest -le $current) { Write-Output "MWM $current is current"; exit 0 }
$base = "https://github.com/$repo/releases/download/$tag"
$exeDownload = Join-Path $state 'MWM-portable.exe.download'
$hashDownload = Join-Path $state 'MWM-portable.exe.sha256.download'
try {
    Invoke-WebRequest -Uri "$base/MWM-portable.exe" -OutFile $exeDownload -UseBasicParsing -TimeoutSec 120
    Invoke-WebRequest -Uri "$base/MWM-portable.exe.sha256" -OutFile $hashDownload -UseBasicParsing -TimeoutSec 30
    $line = (Get-Content -LiteralPath $hashDownload -Raw).Trim()
    if ($line -notmatch '^([0-9a-fA-F]{64})  MWM-portable\.exe$') { throw 'Invalid release checksum' }
    $expected = $Matches[1].ToUpperInvariant()
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $exeDownload).Hash -ne $expected) {
        throw 'Downloaded MWM checksum mismatch'
    }
    if ((Get-Item -LiteralPath $exeDownload).VersionInfo.ProductVersion -ne $latest.ToString(3)) {
        throw 'Downloaded MWM version mismatch'
    }
    Move-Item -LiteralPath $exeDownload -Destination $stage -Force
    @{ version = $latest.ToString(3); sha256 = $expected } |
        ConvertTo-Json -Compress | Set-Content -LiteralPath $pending -Encoding Ascii
    Install-Staged
} finally {
    Remove-Item -LiteralPath $exeDownload,$hashDownload -Force -ErrorAction SilentlyContinue
}
