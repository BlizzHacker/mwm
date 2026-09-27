# Enable updates for a portable MWM install in the current Windows profile.
# Store MSIX packages are updated by Microsoft Store and do not use this task.
$ErrorActionPreference = 'Stop'
$installDir = Join-Path $env:LOCALAPPDATA 'MWM'
$exe = Join-Path $installDir 'MoveWeightManager.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw "Portable MWM install missing: $exe" }
$source = Join-Path $PSScriptRoot 'mwm-windows-update.ps1'
$target = Join-Path $installDir 'mwm-windows-update.ps1'
Copy-Item -LiteralPath $source -Destination $target -Force
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument ('-NoProfile -NonInteractive -File "{0}"' -f $target)
$triggers = @(
    (New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME),
    (New-ScheduledTaskTrigger -Daily -At 4:30am)
)
$principal = New-ScheduledTaskPrincipal -UserId ([Security.Principal.WindowsIdentity]::GetCurrent().Name) -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -ExecutionTimeLimit (New-TimeSpan -Minutes 20)
Register-ScheduledTask -TaskName 'MWM Portable Updater' -Action $action -Trigger $triggers -Principal $principal -Settings $settings -Force | Out-Null
Write-Output 'MWM Portable Updater scheduled at logon and daily at 04:30.'
