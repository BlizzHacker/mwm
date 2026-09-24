# Microsoft Store submission kit - MWM - Move Weight Manager

Draft listing and packaging notes. **Do not upload this build yet:** Microsoft Defender detected the current Windows preview as `HackTool:Win32/Mimikatz.NPTT`. The signature/source cause has not been confirmed. The Windows Defender release gate must pass on the exact package candidate before submission.

Partner Center product: **MWM - Move Weight Manager**, Store ID `9NQNXS66M029`. Verified package identity on 2026-09-24: `MOVEWEIGHT.MWM-MoveWeightManager`, publisher `CN=6375D74B-5E4F-45B4-B246-B29507C1332A`, display name `MOVE WEIGHT`.

## 1. Reserved identity
The name has been reserved as an **MSIX or PWA app**. *Product management → Product identity* shows:

| Field | Value in `packaging/msix/AppxManifest.xml` |
|---|---|
| Package/Identity/Name | `MOVEWEIGHT.MWM-MoveWeightManager` |
| Package/Identity/Publisher | `CN=6375D74B-5E4F-45B4-B246-B29507C1332A` |
| Package/Properties/PublisherDisplayName | `MOVE WEIGHT` |

Build after the Defender gate passes:
`packaging/msix/build-msix.ps1 -Exe <MoveWeightManager.exe> -Version <x.y.z>`.

## 2. Package
Upload **`MWM_<version>_x64.msix`** (unsigned - the Store signs it). Target: Windows 10 1809+ desktop, x64.

## 3. Restricted capabilities - justification (paste into "Submission options → Restricted capabilities")
> **runFullTrust** - MWM is a Win32 system-maintenance utility (cleaner, uninstaller, startup manager, file
> manager, repair toolkit). It must run as a full-trust desktop process to read the installed-programs
> registry, run program uninstallers, and manage startup entries and services.
>
> **unvirtualizedResources** - MWM cleans caches and temporary files and removes uninstall leftovers in the
> user's real profile (%LOCALAPPDATA%, %APPDATA%) and registry (HKCU). With file-system/registry write
> virtualization it would only see a per-package copy and could not clean or repair anything.
>
> **allowElevation** - some maintenance the user explicitly starts (system temp cleanup, System File Checker,
> DISM, chkdsk, service start-mode changes, BitLocker recovery-key display) requires administrator rights; MWM
> asks through the standard UAC prompt only when the user starts such an action.

## 4. Store listing (English)
**Product name:** MWM - Move Weight Manager

**Short description (≤ 100 chars):**
PC health, repair and recovery: clean junk, uninstall cleanly, fix Windows, recover your keys.

**Description:**
MWM - Move Weight Manager is a free, open-source PC doctor. It replaces a whole toolbox - cleaner,
uninstaller, startup manager, file manager and technician repair kit - with one lightweight app. No ads,
no "Pro" upsell, no telemetry.

• Health Check - see junk, startup load, outdated apps and disk space at a glance; clean the safe stuff in one click.
• Custom Clean - Windows temp files, caches, crash dumps, update leftovers, browser caches/history/cookies.
• Uninstaller - runs the program's own uninstaller, then finds leftover folders, shortcuts and registry keys. Leftovers go to the Recycle Bin and registry keys are backed up first.
• Startup Manager - turn off apps, scheduled tasks and services that slow down boot (reversible, Task Manager-compatible).
• Software Updater - free updates for your apps through Windows Package Manager.
• Commander - a fast dual-pane, keyboard-driven file manager with search, compare, multi-rename and zip.
• Keys & Licenses - view, copy and export your Windows product key, license status, BitLocker recovery keys and saved Wi-Fi passwords.
• Repair - System File Checker, DISM, check disk, Windows Update reset, network reset, print spooler fix, restore points, Defender scans.
• System Report - hardware, BIOS, activation, disk and battery health; export to HTML.
• Network doctor - finds whether the problem is your Wi-Fi, router, internet or DNS, in plain English.
• Crashes & Events - blue screens, unexpected shutdowns and errors in one place.
• Also: duplicate finder, disk analyzer, performance view, driver inventory, secure file shredder and free-space wipe.

Everything MWM finds stays on your PC. Source code: github.com/BlizzHacker/mwm

**Features (up to 20, ≤ 200 chars each):** use the bullet list above, one per line.

**Keywords (7):** pc cleaner; uninstaller; repair; file manager; product key; startup manager; system report

**Category:** Utilities & tools (subcategory: none) · **Privacy policy URL:** https://github.com/BlizzHacker/mwm/blob/main/PRIVACY.md
**Website:** https://github.com/BlizzHacker/mwm · **Support contact:** https://github.com/BlizzHacker/mwm/issues
**Copyright:** © 2026 MoveWeight Foundation · **License terms:** GPL-3.0-or-later

**Screenshots:** `store/screenshots/*.png` (1920×1080, demo data - no personal info).
**Store logo:** `store/logo-300.png` (300×300).

## 5. Age rating (IARC questionnaire)
Category **Utility / productivity**. Answer **No** to every content question (violence, sexual content, gambling,
user interaction/chat, location sharing, digital purchases). Expected rating: **3+ / Everyone**.
(Lesson from the Cryptic Realm submission: do not tick "users can interact" - MWM has no online features.)

## 6. Pricing & availability
Free · all markets · Windows 10/11 Desktop.

## 7. Notes for certification (paste into "Notes for certification")
> MWM is an open-source system utility. No account or sign-in. To exercise the main flow: open Health Check
> (scan starts automatically) → Custom Clean → Scan. Actions that change the system always show a confirmation
> first. Administrator-only repairs show a UAC prompt; declining it is handled gracefully.
