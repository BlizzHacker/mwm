@echo off
rem MWM Rescue - WinPE startnet: network up, MWM server, rescue menu.
title MWM Rescue 2026
wpeinit
wpeutil InitializeNetwork
wpeutil DisableFirewall >nul 2>&1
wpeutil UpdateBootInfo >nul 2>&1

echo.
echo  ============================================================
echo   MWM RESCUE 2026 - Move Weight Manager
echo   The Geek Squad MRI replacement. Everything runs from RAM.
echo  ============================================================
echo.

rem Assign a DHCP address and show it.
for /f "tokens=2 delims=:" %%A in ('ipconfig ^| findstr /c:"IPv4"') do set MWMIP=%%A
set MWMIP=%MWMIP: =%

echo  MWM Rescue web manager:   http://%MWMIP%:7777
echo  Open that address on any other PC on this network.
echo  Token is printed below (or: X:\MWM\mwm.exe serve --show-token)
echo.
start "MWM" /min X:\MWM\mwm.exe serve --bind 0.0.0.0:7777
timeout /t 3 >nul
X:\MWM\mwm.exe serve --show-token

:menu
echo.
echo  [1] MWM Rescue Center (web)   http://%MWMIP%:7777
echo  [2] Command prompt            (full WinPE shell)
echo  [3] Disk tools:  TestDisk / PhotoRec
echo  [4] Memory test: Memtest86+
echo  [5] Notepad++    [6] PuTTY     [7] 7-Zip
echo  [8] Sysinternals folder        [9] Reboot   [0] Power off
echo.
set /p CH=Choice: 
if "%CH%"=="1" (echo Open http://%MWMIP%:7777 on another PC. & goto menu)
if "%CH%"=="2" (cmd /k & goto menu)
if "%CH%"=="3" (start "" X:\MWM\testdisk\testdisk_win.exe & start "" X:\MWM\testdisk\qphotorec_win.exe & goto menu)
if "%CH%"=="4" (echo Reboot and pick Memtest86+ from the boot menu, or run it from the USB multiboot. & goto menu)
if "%CH%"=="5" (start "" X:\MWM\notepad++\notepad++.exe & goto menu)
if "%CH%"=="6" (start "" X:\MWM\putty\putty.exe & goto menu)
if "%CH%"=="7" (start "" X:\MWM\7zip\7zFM.exe & goto menu)
if "%CH%"=="8" (explorer X:\MWM\sysinternals & goto menu)
if "%CH%"=="9" (wpeutil reboot)
if "%CH%"=="0" (wpeutil shutdown)
goto menu
