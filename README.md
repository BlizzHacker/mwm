<p align="center"><img src="assets/mwm-logo.svg" width="112" alt="MWM logo"></p>
<h1 align="center">MWM - Move Weight Manager</h1>
<p align="center"><b>PC health, repair and recovery - for your PC and your servers.</b><br>
Free and open source. No ads, no "Pro" tier, no telemetry.</p>

<p align="center"><a href="https://manage.moveweight.com/">Website</a> · <a href="https://manage.moveweight.com/app/">Private web manager</a></p>

<p align="center"><img src="docs/screenshots/health.png" width="860" alt="MWM Health Check"></p>

MWM replaces the pile of tools a PC tech carries - **cleaner, uninstaller, dual-pane file manager and the
Geek Squad / Hiren's repair kit** - with one ~4 MB app for Windows, and a single static binary that gives
**Proxmox VE, Unraid and any Linux server** the same power in a browser.

## What it does

| Area | MWM |
|---|---|
| **Health Check** | Junk, startup, outdated apps and disk at a glance; one click cleans only the safe defaults |
| **Custom Clean** | 40+ rules: Windows temp, recycle bin, thumbnails, crash dumps, WER, Update cache, Store apps, NVIDIA/Discord/VS Code/Steam caches, dev caches, every Chromium browser + Firefox (cache, history, cookies). Linux: apt/dnf cache, journals, rotated logs, /tmp, Proxmox task logs |
| **Uninstaller** | Runs the real uninstaller (UAC-aware), then hunts leftover folders, shortcuts and registry keys. Leftovers go to the Recycle Bin; registry keys are backed up as `.reg` first. Store apps, dpkg/flatpak/snap on Linux |
| **Startup Manager** | Startup apps, scheduled tasks, third-party services / systemd units - toggled the Task Manager way, never deleted |
| **Software Updater** | Free, via winget / apt / brew - updates straight from the publisher |
| **Commander** | Dual-pane, keyboard-first file manager (Double Commander / Geek Squad FMOD style): F3 view (text/image/hex), F5 copy, F6 move, F7 mkdir, F8 recycle, search by name/content, compare panes, multi-rename, zip pack/unpack, folder sizes, network drives. Copies run as background jobs with progress, cancel and conflict handling |
| **Keys & Licenses** | Windows key from firmware (OEM) and decoded from the registry, every license's status, Office 2010/2013 keys, **BitLocker recovery keys**, saved Wi-Fi passwords; SSH host keys, Proxmox subscription, Unraid license, WireGuard on Linux. Show / copy / export |
| **Vault** | Open, edit and save KeePassXC-compatible KDBX3/4 databases in the desktop or web UI. Groups, entry history, attachments, key files, TOTP, password generation, search, encrypted KDBX backups, machine-key import, and Google/Brave/Edge CSV import. Master credentials remain in the UI process; agents store encrypted KDBX bytes only. |
| **Repair** | SFC, DISM RestoreHealth + component cleanup, chkdsk, Windows Update reset, restore point, clock resync, icon cache, DNS flush, IP renew, network stack reset, print spooler, Defender update / quick / full scan, battery + energy reports, memory test. Linux: fix broken packages, remove old kernels, Docker cleanup, ZFS scrub, SMART self-tests, journal vacuum. Live output, keeps running across pages |
| **System Report** | Maker/model/serial, BIOS, board, CPU, RAM sticks, GPUs, activation, disk health (temp, wear, hours, errors), battery - exportable HTML report for a customer or a ticket |
| **Security** | Defender, firewall, UAC, BitLocker, threat history |
| **Network** | Router → internet → DNS → HTTPS test with a plain-English verdict and one-click fixes |
| **Crashes & Events** | Blue screens, hard resets, minidumps and a searchable error log |
| **Server / NAS** | Proxmox cluster guests (start / shut down / reboot on any node), storage, ZFS pools + scrub, SMART for every disk, Docker containers, Unraid array, failed services, kernels |
| **Service plugins** | Discover ARR services, qBittorrent, Jellyfin, Plex, RomM, NZBGet, Seerr, Tautulli, Komga and FlareSolverr across connected Proxmox nodes. MWM runs on the PVE nodes and uses `pct` to inspect their LXCs. Optional local MCP-ARR bridge exposes upstream MCP tools in the same desktop/web UI. |
| Also | Duplicate Finder (BLAKE3), Disk Analyzer, Performance (heavy processes), Drivers (inventory - updates only via Windows Update, never third-party mirrors), Shredder + free-space wipe, System Tools launcher |

## Install

### Windows 10 / 11
Windows Store submission is in progress. The earlier preview triggered Microsoft Defender on the publisher's PC. The replacement MSIX and executable passed local and CI Defender scans, and the publisher confirmed the Commander right-click menu works in the updated installed Windows app. Store certification is still pending. Use only a published release; no Windows release is available yet.

### Proxmox VE / Debian / Ubuntu
```sh
curl -fsSL https://github.com/BlizzHacker/mwm/releases/latest/download/install.sh | sh -s -- --web
```
Then open `http://<server>:7777/?token=...` (the installer prints it). `--read-only` gives a look-don't-touch
dashboard. Prefer packages? `apt install ./mwm_x.y.z_amd64.deb` then `systemctl enable --now mwm-web`.

### Unraid
*Plugins → Install Plugin* → `https://github.com/BlizzHacker/mwm/releases/latest/download/mwm.plg`
The web UI starts on port 7777; the plugin prints the sign-in link.

### Command line (every platform)
```
mwm info                 machine, disks, elevation
mwm scan                 measure every cleaning rule (deletes nothing)
mwm clean [--ids a,b] --yes
mwm apps [filter]        installed programs      mwm uninstall <id> --yes
mwm startup [--set ID off]                       mwm updates [--apply all]
mwm keys                 keys & licenses         mwm report    hardware / license report
mwm server               Proxmox / ZFS / SMART / Docker
mwm repair --list        repair tasks            mwm repair sfc
mwm dupes <dir>          mwm analyze <dir>       mwm shred <file> --yes
mwm serve [--bind 0.0.0.0:7777] [--read-only]    browser UI
mwm --json <command>     machine-readable output
```

## Safety model
- Scans never delete. Cleaning only touches caches/temp and skips files in use.
- Uninstall leftovers → Recycle Bin; registry keys exported to `.reg` before removal. Leftover matching refuses
  generic names, anything containing a program root or your profile, and anything another program still uses.
- Startup toggles are Task Manager-compatible and reversible. Repairs use only tools the OS ships.
- `mwm serve` requires a random access token (HttpOnly, SameSite=Strict cookie; JSON-only API, so no CSRF);
  `--read-only` refuses every change. It is plain HTTP - keep it on your LAN or behind a reverse proxy / VPN.
- Local credentials are not sent to the public MWM site. When you connect one MWM agent to another, results you request can traverse that authenticated connection; protect agent tokens and use a trusted network.
- The Vault stores KeePass-compatible KDBX files; its master password and optional key file are never sent to an MWM agent. Vault edits use revision checks to avoid silently overwriting concurrent changes. Windows machine connection tokens use current-user DPAPI protection. Download a KDBX backup before moving or replacing a vault. Browser autofill, passkeys, hardware challenge-response keys and system-wide Auto-Type are not yet implemented in MWM; keep KeePassXC if you rely on them.

## KeePassXC vault migration

Open **Vault** in MWM, choose **Import KeePassXC KDBX**, select the original `.kdbx` file, then enter its master password and optional key file in the app. MWM verifies it can decrypt the database before saving the encrypted KDBX on the selected machine. Your original file stays untouched. **Download KDBX backup** gives you a standard database you can reopen in KeePassXC. The bundled KDBX library is [KdbxWeb](https://github.com/keeweb/kdbxweb), rebuilt with a patched XML parser, and Argon2 uses [hash-wasm](https://github.com/Daninet/hash-wasm). Their licenses are included beside the bundled scripts.

After unlocking the vault, **Import browser CSV** accepts exports from Google Password Manager, Brave, and Microsoft Edge. The dialog includes the browser export steps. Select the CSV in MWM and review the entry count before merging. The CSV is parsed inside the desktop WebView/browser and is never sent to an MWM agent; only the resulting encrypted KDBX is saved. Repeating an import updates matching entries with KDBX history. Delete the plaintext CSV after confirming the imported entries. This is a one-time transfer, not automatic Brave sync or autofill. Windows sign-in passwords and passkeys cannot be exported through this flow; **Import machine keys** handles readable license, BitLocker, and network key records instead.

## MCP-ARR compatibility

MWM has its own Proxmox service discovery and can also connect to [MCP-ARR](https://github.com/aplaceforallmystuff/mcp-arr). On a Proxmox node that owns Sonarr/Radarr/Lidarr/Prowlarr, run `sudo python3 deploy/arr/install.py` from an MWM checkout to install MCP-ARR on `127.0.0.1:3000/mcp`. The installer reads each running service's API key from its LXC, writes a root-only `/etc/mwm/mcp-arr.env`, and starts the upstream container. Docker administrators can inspect container environment and should be trusted with these keys.

The **Plugins** page queries every connected PVE agent from one desktop or manager install and routes container actions to the owning node. qBittorrent's Web UI port is read from its LXC configuration; when the Web UI requires a login, MWM reports that state and opens the service login without extracting or bypassing its password. The **Proxmox Cluster** page lists all LXCs and VMs, including services without a dedicated plugin.

For Arkana running in an LXC, select the owning PVE node, open **Malware Lab**, and choose **Connect Arkana LXC**. Enter the LXC ID. MWM reads that LXC's existing API key through `pct`, connects to its MCP endpoint, and copies analysis samples into its mounted samples folder through `pct push`. The agent stays on the PVE node.

In MWM, choose that node in the machine switcher, open **Plugins**, click **Connect MCP-ARR**, then **Browse tools**. On a Windows PC, the same page can connect to an MCP-ARR instance running locally even without Proxmox. The bridge accepts loopback endpoints only, because MCP-ARR HTTP mode has no documented access control. MWM's native service status view remains usable without MCP-ARR.

## Build (everything cross-compiles from Linux)
```
rustup target add x86_64-pc-windows-msvc x86_64-unknown-linux-musl
cargo install cargo-xwin tauri-cli
cd app/src-tauri && cargo tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc
cargo build --release --target x86_64-unknown-linux-musl -p mwm-cli
packaging/linux/build-packages.sh 4.0.0 dist/            # .deb, Unraid .plg, install.sh, SHA256SUMS
```
Store package (on Windows with the Windows SDK): `packaging/msix/build-msix.ps1`.
Layout: `crates/mwm-core` (engine + one command table `api.rs`), `crates/mwm-cli` (`mwm`, incl. `serve`),
`app/src-tauri` (desktop shell), `app/ui` (plain HTML/JS UI shared by desktop, web and demo).

## Roadmap
1. **Windows** - desktop app, installer, MSIX for the Microsoft Store. *In validation.*
2. **Linux / Proxmox / Unraid** - static CLI + browser UI + server dashboard. *Shipping.* Next: AppImage desktop
   build, TrueNAS SCALE app, multi-server view.
3. **MWM Rescue** - bootable USB (SystemRescue-based) with MWM, offline Windows repair (SFC /offbootdir,
   DISM /image), file recovery and malware scanning for PCs that won't boot.
4. **Android** - storage insight, large/duplicate files, app cache cleanup within Android's rules.
5. **iOS / iPadOS** - photo/file duplicate cleanup and storage insight (Apple allows nothing more).
6. **macOS** - engine rules exist; needs a signed/notarized build.

## License
GPL-3.0-or-later. © 2026 MoveWeight Foundation. See [LICENSE](LICENSE) and [PRIVACY.md](PRIVACY.md).
