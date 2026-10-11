@echo off
rem MWM Rescue ISO builder - Windows WinPE 11 x64 (manual media layout)
rem ADK 28000 layout: source WIM = en-us\winpe.wim ; media skeleton = Media\ ;
rem boot files (etfsboot.com, efisys_noprompt.bin) in Oscdimg dir. No copype.
rem Produces out\MWM-Rescue-WinPE.iso (BIOS+EFI) serving MWM Rescue Center.
rem Run from an ELEVATED cmd (DISM mount needs Administrator).
setlocal EnableDelayedExpansion
set SCRIPTDIR=%~dp0
set STAGE=%SCRIPTDIR%stage\winpe
set OUT=%SCRIPTDIR%out

set ADK=C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit
set DT=%ADK%\Deployment Tools\amd64
set DISM=%DT%\DISM\dism.exe
set OSCDIMG=%DT%\Oscdimg\oscdimg.exe
set BOOTDIR=%DT%\Oscdimg
set WINPE=%ADK%\Windows Preinstallation Environment\amd64

if not exist "%WINPE%\en-us\winpe.wim" (echo ERROR: WinPE source wim not found & exit /b 1)
if not exist "%SCRIPTDIR%tools\winpe\mwm.exe" (echo ERROR: stage the Windows mwm CLI at tools\winpe\mwm.exe & exit /b 1)
if not exist "%SCRIPTDIR%rescue\startnet.cmd" (echo ERROR: rescue\startnet.cmd missing & exit /b 1)

echo [1/6] Fresh WinPE media stage
if exist "%STAGE%" rmdir /s /q "%STAGE%"
mkdir "%STAGE%\media\sources" "%STAGE%\mount" 2>nul
xcopy /e /i /q "%WINPE%\Media\*" "%STAGE%\media" >nul
copy /y "%WINPE%\en-us\winpe.wim" "%STAGE%\media\sources\boot.wim" >nul

echo [2/6] Mount boot.wim and add optional components
"%DISM%" /Mount-Image /ImageFile:"%STAGE%\media\sources\boot.wim" /Index:1 /MountDir:"%STAGE%\mount" >nul || exit /b 1
for %%P in (
  WinPE-WMI WinPE-NetFx WinPE-Scripting WinPE-PowerShell WinPE-StorageWMI
  WinPE-DismCmdlets WinPE-EnhancedStorage WinPE-FMAPI WinPE-SecureStartup
  WinPE-HTA WinPE-RNDIS
) do (
  "%DISM%" /Image:"%STAGE%\mount" /Add-Package /PackagePath:"%WINPE%\WinPE_OCs\%%P.cab" >nul 2>&1
  "%DISM%" /Image:"%STAGE%\mount" /Add-Package /PackagePath:"%WINPE%\WinPE_OCs\en-us\%%P_en-us.cab" >nul 2>&1
)

echo [3/6] Inject MWM + tools + rescue launcher
mkdir "%STAGE%\mount\MWM" 2>nul
xcopy /e /i /q "%SCRIPTDIR%tools\winpe\*" "%STAGE%\mount\MWM\" >nul
copy /y "%SCRIPTDIR%rescue\startnet.cmd" "%STAGE%\mount\Windows\System32\startnet.cmd" >nul

echo [4/6] Unmount and commit
"%DISM%" /Unmount-Image /MountDir:"%STAGE%\mount" /Commit >nul || exit /b 1

echo [5/6] Make ISO (BIOS + UEFI bootable, no prompt)
if not exist "%OUT%" mkdir "%OUT%"
"%OSCDIMG%" -m -o -u2 -udfver102 -bootdata:2#p0,e,b"%BOOTDIR%\etfsboot.com"#pEF,e,b"%BOOTDIR%\efisys_noprompt.bin" "%STAGE%\media" "%OUT%\MWM-Rescue-WinPE.iso" >nul || (echo oscdimg failed & exit /b 1)

echo [6/6] Done
dir "%OUT%\MWM-Rescue-WinPE.iso"
certutil -hashfile "%OUT%\MWM-Rescue-WinPE.iso" SHA256
endlocal
