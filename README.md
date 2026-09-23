# MDW - Move Digital Weight

Free, open-source replacement for **CCleaner** and **Revo Uninstaller**. No ads,
no "Pro" tier, no telemetry. One ~4 MB app (Rust + Tauri) and a ~1 MB `mdw` CLI
for servers.

| CCleaner / Revo feature | MDW |
|---|---|
| Health Check | Health Check - junk, startup, outdated apps, disk at a glance; one-click "Move the weight" (safe defaults only) |
| Custom Clean (Junk / Browser) | Custom Clean - 40+ rules: Windows temp, recycle bin, thumbnails, crash dumps, WER, Update cache, Store apps, NVIDIA/Discord/VS Code/Steam caches, dev caches (npm, pip, Go, NuGet), Chrome/Edge/Brave/Vivaldi/Opera/Firefox cache, history, cookies |
| Uninstaller + Revo leftovers scan | Uninstaller - runs the real uninstaller (UAC-aware), then hunts leftover folders, shortcuts and registry keys. Leftovers go to the Recycle Bin; registry keys are exported to `.reg` first |
| Revo Windows Apps | Store (MSIX/UWP) apps listed and removable |
| Startup Manager / Autorun Manager | Startup apps, scheduled tasks, third-party services. Toggles use Task Manager's own `StartupApproved` flags - nothing is deleted |
| Software Updater (paid in CCleaner) | Free, via **winget** (Windows), apt (Linux), brew (macOS) |
| Driver Updater | Driver inventory with age flags; updates only through Windows Update / the vendor (no third-party driver mirrors - by design) |
| Performance Optimizer | Heaviest programs, end task |
| Duplicate Finder | BLAKE3 content hashing; removals go to the Recycle Bin |
| Revo Unrecoverable Delete | Shredder (random passes + zero pass + name scrub) |
| Revo Evidence Remover | Free-space wipe (`cipher /w` on Windows) |
| Revo Windows Tools | System Tools launcher (Disk Cleanup, Restore, Services, ...) |
| - | Disk Analyzer (largest folders/files, by type) |
| **Double Commander / FMOD** | **Commander** - dual-pane, keyboard-first file manager: F3 view (text/image/hex), F4 edit, F5 copy, F6 move, F7 mkdir, F8 recycle, Shift+Del delete, F2 rename, Alt+F7 search (name + content), compare panes, multi-rename ([N] [E] [C] [D] tokens, find/replace, case), zip pack/unpack, folder sizes, properties, terminal here, type-ahead, drive bar incl. network drives. Copies/moves are background jobs with progress, cancel and conflict handling (skip / keep both / overwrite) |
| **Geek Squad MRI / Hiren's** | **Tech Toolkit** - System Report (hardware, BIOS, serial, RAM sticks, GPUs, Windows activation, product key + OEM key from BIOS, disk health/temp/wear, battery; export to HTML), Repair (SFC, DISM RestoreHealth + component cleanup, chkdsk, Windows Update reset, restore point, clock resync, icon cache, DNS flush, IP renew, Winsock/TCP reset, print spooler, Defender update/quick/full scan, battery + energy reports, memory test), Security (Defender, firewall, UAC, BitLocker, threat history, saved Wi-Fi keys), Network diagnostics (router → internet → DNS → HTTPS with a plain-English verdict), Crashes & Events (BSODs, hard resets, minidumps, error log) |

## Layout

```
crates/mdw-core   the engine - every feature, per-OS behind cfg()
crates/mdw-cli    `mdw` command line (servers, SSH, scripts)
app/src-tauri     desktop app shell (Tauri 2)
app/ui            UI - plain HTML/CSS/JS, no build step; demo.js = browser demo data
packaging/msix    Microsoft Store manifest + build script
```

## CLI

```
mdw info                      # machine, disks, elevation
mdw scan                      # measure all rules (deletes nothing)
mdw clean                     # dry run of the default rules
mdw clean --yes               # clean them
mdw clean --ids lx.apt,lx.journal --yes
mdw apps [filter]             # installed programs
mdw uninstall <id> --yes      # uninstall + list leftovers
mdw startup [--set ID off]
mdw updates [--apply all]
mdw dupes ~/Downloads
mdw analyze /var
mdw shred secret.pdf --yes
mdw --json <any command>
```

## Build (cross-compiled from Linux - no Windows toolchain needed)

```
rustup target add x86_64-pc-windows-msvc x86_64-unknown-linux-musl
cargo install cargo-xwin tauri-cli
cd app/src-tauri && cargo tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc
cargo build --release --target x86_64-unknown-linux-musl -p mdw-cli     # static, any distro
cargo xwin build --release --target x86_64-pc-windows-msvc -p mdw-cli
```

Store package (Windows, needs the Windows SDK): `packaging/msix/build-msix.ps1`.

## Platform roadmap

1. **Windows** - desktop app, NSIS installer, MSIX for the Store. *Done (v0.1).*
2. **Linux / Proxmox** - `mdw` static CLI: apt/dnf caches, journals, rotated logs, /tmp, Proxmox task logs,
   dpkg/flatpak/snap uninstall + leftovers, systemd services, apt updates. *Done (v0.1).* Desktop build (AppImage/.deb) and a
   Proxmox web panel next.
3. **Android** - Tauri 2 mobile target. Android sandboxing means: app caches via `StorageStatsManager`,
   large/duplicate files via storage access, uninstall via system intents. No root tricks.
4. **iOS / iPadOS** - Apple does not let any app clean other apps. MDW for iOS can do duplicate/large photo
   & file cleanup in the user's own library and storage insight - that is the honest ceiling.
4b. **MDW Rescue (bootable)** - the Hiren's / MRI "boot it when Windows won't" piece: a SystemRescue-based
   ISO/USB with the static `mdw` CLI + Commander web UI, offline file recovery, partition tools, offline
   Windows repair (SFC /offbootdir, DISM /image) and malware scanning. Password *reset* for local accounts only
   with the owner present - no key/password cracking tools.
5. **macOS** - engine rules already exist (`~/Library/Caches`, logs, Trash, Xcode DerivedData, app bundle
   + leftover removal, brew updates); needs a signed/notarized build.

## Safety model

* Scans never delete. Cleaning only touches caches/temp and skips files in use.
* Uninstall leftovers -> Recycle Bin; registry keys -> `.reg` backup in `%LOCALAPPDATA%\MDW\backups` before removal.
* Leftover matching refuses generic names (Microsoft, Windows, Common Files, ...), anything that contains a
  program root or your profile, and anything another installed program still uses.
* Startup toggles are Task Manager-compatible and reversible.

License: GPL-3.0-or-later. (c) 2026 MoveWeight Foundation.
