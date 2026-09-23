/* Demo backend: lets the MDW UI run in any browser (moveweight.com/mdw/demo)
   with realistic sample data. Never loaded logic-wise inside the desktop app,
   where window.__TAURI__ exists and app.js calls the real engine. */
(function () {
  if (window.__TAURI__) return;
  const GB = 1073741824, MB = 1048576;
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const R = (id, group, category, name, description, bytes, files, admin, default_on, warning) =>
    ({ id, group, category, name, description, bytes, files, locations: 1, admin, default_on, warning: warning || null });

  let scan = [
    R("win.temp", "junk", "Windows", "Temporary files (user)", "Files apps left in your %TEMP% folder more than a day ago.", 3.4 * GB, 18234, false, true),
    R("win.systemp", "junk", "Windows", "Temporary files (system)", "C:\\Windows\\Temp older than a day.", 612 * MB, 903, true, true),
    R("win.recycle", "junk", "Windows", "Recycle Bin", "Everything currently in the Recycle Bin on every drive.", 1.9 * GB, 311, false, true),
    R("win.thumbs", "junk", "Windows", "Thumbnail cache", "Explorer's picture previews. Rebuilt automatically.", 248 * MB, 14, false, true),
    R("win.inet", "junk", "Windows", "Internet cache (WinINet)", "Cache used by Windows components and legacy apps.", 96 * MB, 2210, false, true),
    R("win.dx", "junk", "System", "DirectX shader cache", "Compiled GPU shaders. Games rebuild them on next launch.", 402 * MB, 77, false, true),
    R("win.dumps", "junk", "System", "Crash dumps", "Memory dumps left behind by crashed programs.", 1.1 * GB, 9, false, true),
    R("win.wer", "junk", "System", "Error reports", "Windows Error Reporting archives and queues.", 188 * MB, 64, false, true),
    R("win.update", "junk", "Advanced", "Windows Update downloads", "Installer files for updates that are already installed.", 2.7 * GB, 412, true, false),
    R("win.store", "junk", "Windows Store", "Store app caches", "Web caches and temp state of Microsoft Store apps.", 331 * MB, 5120, false, true),
    R("win.downloads", "junk", "Windows Downloads", "Downloads older than 30 days", "Files in your Downloads folder untouched for 30 days. Review before cleaning!", 14.2 * GB, 388, false, false),
    R("app.nvidia", "junk", "Applications", "NVIDIA shader cache", "DirectX/OpenGL shader caches kept by the NVIDIA driver.", 734 * MB, 1203, false, true),
    R("app.discord", "junk", "Applications", "Discord cache", "Images and scripts Discord re-downloads.", 512 * MB, 4410, false, true),
    R("app.vscode", "junk", "Applications", "VS Code cache", "Editor caches, cached extension installers and old logs.", 890 * MB, 3302, false, true),
    R("dev.npm", "junk", "Developer", "npm cache", "Downloaded npm tarballs (npm cache).", 4.8 * GB, 88120, false, false),
    R("dev.go", "junk", "Developer", "Go build cache", "Compiled Go packages (go clean -cache).", 3.4 * GB, 30211, false, false),
    R("br.chrome.cache", "browser", "Google Chrome", "Internet cache", "Cached pages, images and compiled scripts for every profile.", 1.3 * GB, 22011, false, true),
    R("br.chrome.history", "browser", "Google Chrome", "History", "Browsing and download history. Close the browser first.", 84 * MB, 5, false, false, "Close the browser first - open files are skipped."),
    R("br.chrome.cookies", "browser", "Google Chrome", "Cookies", "Signs you out of every website in this browser.", 6 * MB, 2, false, false, "You will be signed out of websites."),
    R("br.brave.cache", "browser", "Brave", "Internet cache", "Cached pages, images and compiled scripts for every profile.", 922 * MB, 15003, false, true),
    R("br.edge.cache", "browser", "Microsoft Edge", "Internet cache", "Cached pages, images and compiled scripts for every profile.", 451 * MB, 8870, false, true),
    R("br.firefox.cache", "browser", "Firefox", "Internet cache", "Firefox disk cache and startup cache.", 210 * MB, 3021, false, true),
    R("win.recent", "privacy", "Windows", "Recent documents list", "Shortcuts in the Recent Items / Quick Access history.", 2 * MB, 612, false, false),
  ];

  const A = (name, publisher, version, size, date, source = "win32-machine") =>
    ({ id: `${source}:${name}`, name, publisher, version, size_bytes: size, install_date: date, install_location: "", uninstall_cmd: "x", quiet_uninstall_cmd: "", icon: "", source, reg_key: "", can_uninstall: true });
  let apps = [
    A("7-Zip 25.01 (x64)", "Igor Pavlov", "25.01", 6 * MB, "2025-09-14"),
    A("Brave", "Brave Software Inc", "140.1.82.170", 0, "2026-09-01", "win32-user"),
    A("CCleaner", "Piriform Software", "7.2.1123", 212 * MB, "2026-08-30"),
    A("Discord", "Discord Inc.", "1.0.9205", 88 * MB, "2026-03-11", "win32-user"),
    A("Dropbox", "Dropbox, Inc.", "231.4.5732", 410 * MB, "2026-05-02"),
    A("EA app", "Electronic Arts", "13.512.0", 1.2 * GB, "2025-12-24"),
    A("ExpressVPN", "ExpressVPN", "12.93.0", 160 * MB, "2025-07-19"),
    A("Git", "The Git Development Community", "2.51.0", 342 * MB, "2025-10-08"),
    A("KeePassXC", "KeePassXC Team", "2.7.10", 180 * MB, "2025-06-01"),
    A("Microsoft 365 - en-us", "Microsoft Corporation", "16.0.19127", 3.1 * GB, "2026-09-10"),
    A("Microsoft Visual Studio Code (User)", "Microsoft Corporation", "1.124.2", 420 * MB, "2026-07-21", "win32-user"),
    A("Mozilla Thunderbird (x64 en-US)", "Mozilla", "154.0", 260 * MB, "2026-06-09"),
    A("Python 3.13.0 (64-bit)", "Python Software Foundation", "3.13.150.0", 110 * MB, "2025-02-17"),
    A("Revo Uninstaller 2.6.5", "VS Revo Group, Ltd.", "2.6.5", 38 * MB, "2026-09-02"),
    A("VLC media player", "VideoLAN", "3.0.23", 170 * MB, "2025-04-30"),
    A("YouTubeToMP3", "MediaHuman", "3.9.9.93", 94 * MB, "2024-11-12"),
  ];

  const T = (kind, name, command, enabled, scope = "user", detail = "") =>
    ({ id: `${kind}|${name}`, kind, name, command, location: kind === "task" ? "\\" : "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", enabled, scope, admin: scope === "machine", detail });
  let startup = [
    T("run", "Dropbox", "\"C:\\Program Files (x86)\\Dropbox\\Client\\Dropbox.exe\" /systemstartup", false),
    T("run", "KeePassXC", "\"C:\\Program Files\\KeePassXC\\KeePassXC.exe\"", true, "machine"),
    T("run", "OneDrive", "\"C:\\Users\\you\\AppData\\Local\\Microsoft\\OneDrive\\OneDrive.exe\" /background", true),
    T("run", "RtkAudUService", "C:\\Windows\\System32\\DriverStore\\...\\RtkAudUService64.exe -background", true, "machine"),
    T("run", "SecurityHealth", "C:\\Windows\\system32\\SecurityHealthSystray.exe", true, "machine"),
    T("folder", "YouTubeToMP3", "C:\\Users\\you\\AppData\\Roaming\\...\\Startup\\YouTubeToMP3.lnk", true),
    T("run", "Discord", "\"C:\\Users\\you\\AppData\\Local\\Discord\\Update.exe\" --processStart Discord.exe", true),
    T("task", "BraveSoftwareUpdateTaskMachineCore", "C:\\Program Files (x86)\\BraveSoftware\\Update\\BraveUpdate.exe /c", true, "machine", "Ready"),
    T("task", "MicrosoftEdgeUpdateTaskMachineUA", "C:\\Program Files (x86)\\Microsoft\\EdgeUpdate\\MicrosoftEdgeUpdate.exe /ua", true, "machine", "Ready"),
    T("task", "Dropbox Update", "C:\\Program Files (x86)\\Dropbox\\Update\\DropboxUpdate.exe /c", false, "machine", "Disabled"),
    { ...T("service", "ExpressVPN Service", "C:\\Program Files (x86)\\ExpressVPN\\expressvpnd.exe", true, "machine", "Auto - Running"), location: "ExpressVpnService" },
    { ...T("service", "EA app background service", "C:\\Program Files\\Electronic Arts\\EA Desktop\\EABackgroundService.exe", true, "machine", "Manual - Stopped"), location: "EABackgroundService" },
    { ...T("service", "NVIDIA Display Container LS", "C:\\Windows\\...\\NVDisplay.Container.exe", true, "machine", "Auto - Running"), location: "NVDisplay.ContainerLocalSystem" },
  ];

  const U = (name, id, current, available) => ({ name, id, current, available, source: "winget" });
  const updates = [
    U("7-Zip 25.01 (x64)", "7zip.7zip", "25.01", "26.03"),
    U("Mozilla Thunderbird (x64 en-US)", "Mozilla.Thunderbird", "154.0", "156.0"),
    U("Microsoft Visual Studio Code (User)", "Microsoft.VisualStudioCode", "1.124.2", "1.138.0"),
    U("Windows Subsystem for Linux", "Microsoft.WSL", "2.7.3.0", "2.7.13"),
    U("Microsoft .NET Runtime 9.0", "Microsoft.DotNet.Runtime.9", "9.0.17", "9.0.20"),
    U("Python 3.13 64-bit", "Python.Python.3.13", "3.13.0", "3.13.15"),
    U("VLC media player", "VideoLAN.VLC", "3.0.23", "3.0.24.0"),
  ];

  const disks = [
    { mount: "C:\\", name: "Acer", fs: "NTFS", total: 475.4 * GB, free: 2.6 * GB, removable: false },
    { mount: "T:\\", name: "usb4", fs: "NTFS", total: 5.5 * 1024 * GB, free: 1.9 * 1024 * GB, removable: false },
  ];

  const handlers = {
    system_info: () => ({ os: "Windows 11 Home 25H2 (web demo)", platform: "windows", hostname: "DEMO-PC", elevated: false, version: "0.1.0", memory_total: 16 * GB, memory_used: 11.2 * GB, cpu: "Intel Core i7-1165G7", cores: 8, uptime_secs: 86400 * 3, disks }),
    cleaner_scan: async () => { await wait(900); return scan.map((x) => ({ ...x })); },
    cleaner_clean: async ({ ids }) => {
      await wait(1200);
      const res = scan.filter((s) => ids.includes(s.id)).map((s) => ({ id: s.id, name: s.name, bytes_freed: Math.round(s.bytes * 0.97), files_deleted: Math.round(s.files * 0.97), skipped: Math.round(s.files * 0.03) }));
      const freed = res.reduce((a, r) => a + r.bytes_freed, 0);
      scan = scan.map((s) => (ids.includes(s.id) ? { ...s, bytes: Math.round(s.bytes * 0.03), files: Math.round(s.files * 0.03) } : s));
      disks[0].free += freed;
      return res;
    },
    apps_list: async () => { await wait(500); return apps.slice(); },
    app_uninstall: async ({ app }) => { await wait(1500); apps = apps.filter((a) => a.id !== app.id); return { ok: true, code: 0, message: "Uninstaller finished." }; },
    app_leftovers: async ({ app }) => {
      await wait(900);
      const n = app.name.split(" ")[0];
      return [
        { kind: "dir", path: `C:\\Users\\you\\AppData\\Roaming\\${n}`, bytes: 48 * MB },
        { kind: "dir", path: `C:\\ProgramData\\${n}`, bytes: 3 * MB },
        { kind: "file", path: `C:\\Users\\you\\Desktop\\${n}.lnk`, bytes: 1400 },
        { kind: "regkey", path: `HKCU\\SOFTWARE\\${app.publisher.split(" ")[0]}\\${n}`, bytes: 0 },
      ];
    },
    leftovers_remove: async ({ items }) => { await wait(700); return { removed: items.map((i) => i.path), failed: [], backup: "C:\\Users\\you\\AppData\\Local\\MDW\\backups" }; },
    startup_list: async () => { await wait(600); return startup.map((x) => ({ ...x })); },
    startup_set: async ({ id, enabled }) => { startup = startup.map((s) => (s.id === id ? { ...s, enabled } : s)); return { ok: true, message: "Done" }; },
    service_mode: async () => ({ ok: true, message: "Done" }),
    updates_list: async () => { await wait(1400); return updates.slice(); },
    update_apply: async ({ id }) => { await wait(1100); return { id, ok: true, message: "Successfully installed" }; },
    dupes_find: async () => {
      await wait(1500);
      return [
        { size: 1.4 * GB, hash: "a1", wasted: 1.4 * GB, files: [{ path: "C:\\Users\\you\\Downloads\\ubuntu-24.04.iso", modified: 1714000000 }, { path: "C:\\Users\\you\\Desktop\\old\\ubuntu-24.04.iso", modified: 1719000000 }] },
        { size: 220 * MB, hash: "b2", wasted: 440 * MB, files: [{ path: "C:\\Users\\you\\Videos\\clip.mp4", modified: 1700000000 }, { path: "C:\\Users\\you\\Videos\\clip (1).mp4", modified: 1700100000 }, { path: "C:\\Users\\you\\OneDrive\\clip.mp4", modified: 1700200000 }] },
        { size: 8 * MB, hash: "c3", wasted: 8 * MB, files: [{ path: "C:\\Users\\you\\Pictures\\IMG_2041.jpg", modified: 1690000000 }, { path: "C:\\Users\\you\\Pictures\\Backup\\IMG_2041.jpg", modified: 1690500000 }] },
      ];
    },
    dupes_remove: async ({ paths }) => ({ removed: paths, failed: [] }),
    disk_analyze: async ({ root }) => {
      await wait(1300);
      const kids = [["Users", 301], ["Windows", 38], ["Program Files", 29], ["ProgramData", 21], ["Program Files (x86)", 14], ["pagefile.sys", 28.6, false], ["MoveWeight", 20], ["$Recycle.Bin", 1.9]];
      return {
        root, bytes: 473 * GB, files: 1893112,
        children: kids.map(([n, g, d = true]) => ({ name: n, path: root + n, bytes: g * GB, files: 1000, is_dir: d })),
        largest_files: [["C:\\pagefile.sys", 28.6], ["C:\\Users\\you\\Playnite\\library.db", 6.1], ["C:\\Users\\you\\.ollama\\models\\blobs\\sha256-7c2a", 4.9], ["C:\\hiberfil.sys", 3.2]].map(([p, g]) => ({ path: p, name: p.split("\\").pop(), bytes: g * GB, files: 1, is_dir: false })),
        by_type: [["sys", 31.8 * GB], ["mp4", 22 * GB], ["dll", 18 * GB], ["gguf", 12.8 * GB], ["exe", 9 * GB]],
      };
    },
    drivers_list: async () => { await wait(700); return [
      { device: "Realtek(R) Audio", class: "MEDIA", provider: "Realtek Semiconductor Corp.", version: "6.0.9601.1", date: "2021-03-02", inf: "oem12.inf", age_years: 5.5 },
      { device: "Intel(R) UHD Graphics", class: "DISPLAY", provider: "Intel Corporation", version: "31.0.101.4502", date: "2023-06-20", inf: "oem33.inf", age_years: 3.2 },
      { device: "MediaTek Wi-Fi 6 MT7920 Wireless LAN Card", class: "NET", provider: "MediaTek, Inc.", version: "3.4.0.1170", date: "2024-10-12", inf: "oem41.inf", age_years: 1.9 },
      { device: "NVIDIA GeForce RTX 5050 Laptop GPU", class: "DISPLAY", provider: "NVIDIA", version: "32.0.15.8157", date: "2026-08-14", inf: "oem77.inf", age_years: 0.1 },
    ]; },
    procs_list: async () => { await wait(500); return [
      ["chrome.exe", 2.4 * GB, 6.2, 31], ["MsMpEng.exe", 610 * MB, 3.3, 1], ["Code.exe", 1.3 * GB, 2.1, 12], ["Dropbox.exe", 420 * MB, 0.4, 5], ["Discord.exe", 380 * MB, 0.7, 6], ["explorer.exe", 190 * MB, 0.3, 1],
    ].map(([name, memory, cpu, n]) => ({ name, memory, cpu, pids: Array.from({ length: n }, (_, i) => 1000 + i), exe: `C:\\Program Files\\${name}`, impact: memory > GB || cpu > 15 ? "High" : memory > 300 * MB || cpu > 3 ? "Medium" : "Low" })); },
    procs_kill: async ({ pids }) => pids.length,
    reveal: () => null,
    launch_tool: () => { throw "System tools open in the desktop app."; },
    relaunch_admin: () => { throw "This is the web demo - install MDW to run as administrator."; },
    shred_paths: async () => ({ files: 0, bytes: 0, failed: [] }),
    wipe_free: async () => "Demo: no disk was touched.",
  };
  window.MDW_DEMO = async (cmd, args) => {
    const h = handlers[cmd];
    if (!h) throw `demo: ${cmd} not available`;
    return h(args);
  };
})();
