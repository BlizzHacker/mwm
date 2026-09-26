/* Demo backend: lets the MWM UI run in any browser (moveweight.com/mwm/demo)
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
    A("Audacity 3.7.5", "Audacity Team", "3.7.5", 96 * MB, "2026-08-30"),
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
    A("Notepad++ (64-bit x64)", "Notepad++ Team", "8.8.5", 18 * MB, "2026-09-02"),
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
    system_info: () => ({ os: "Windows 11 Home 25H2 (web demo)", platform: "windows", hostname: "DEMO-PC", elevated: false, version: "0.3.0", memory_total: 16 * GB, memory_used: 11.2 * GB, cpu: "Intel Core i7-1165G7", cores: 8, uptime_secs: 86400 * 3, disks }),
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
    leftovers_remove: async ({ items }) => { await wait(700); return { removed: items.map((i) => i.path), failed: [], backup: "C:\\Users\\you\\AppData\\Local\\MWM\\backups" }; },
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
    relaunch_admin: () => { throw "This is the web demo - install MWM to run as administrator."; },
    shred_paths: async () => ({ files: 0, bytes: 0, failed: [] }),
    wipe_free: async () => "Demo: no disk was touched.",
  };

  // ---------- demo filesystem + jobs (Commander / Toolkit) ----------
  const now = Math.floor(Date.now() / 1000);
  const FS = {
    "C:\\": [["Users"], ["Windows"], ["Program Files"], ["pagefile.sys", 28.6 * GB, true], ["hiberfil.sys", 3.2 * GB, true]],
    "C:\\Users": [["you"], ["Public"]],
    "C:\\Users\\you": [["Desktop"], ["Documents"], ["Downloads"], ["Pictures"], [".gitconfig", 420, true], ["NTUSER.DAT", 3 * MB, true]],
    "C:\\Users\\you\\Downloads": [["setup-7zip-26.03.exe", 1.6 * MB], ["ubuntu-24.04.iso", 5.7 * GB], ["invoice-2026-09.pdf", 210 * 1024], ["notes.txt", 2048], ["photos-backup.zip", 812 * MB], ["old-installers"]],
    "C:\\Users\\you\\Downloads\\old-installers": [["vlc-3.0.20.exe", 42 * MB], ["discord-setup.exe", 98 * MB]],
    "C:\\Users\\you\\Documents": [["Taxes 2025"], ["Resume.docx", 48 * 1024], ["budget.xlsx", 96 * 1024], ["readme.md", 1300]],
    "C:\\Users\\you\\Documents\\Taxes 2025": [["W2.pdf", 180 * 1024], ["1099.pdf", 90 * 1024]],
    "C:\\Users\\you\\Desktop": [["MWM.lnk", 1400], ["todo.txt", 512]],
    "C:\\Users\\you\\Pictures": [["IMG_2041.jpg", 4.2 * MB], ["IMG_2042.jpg", 3.9 * MB], ["screenshot.png", 900 * 1024]],
    "C:\\Windows": [["System32"], ["Temp"], ["explorer.exe", 5 * MB]],
    "C:\\Program Files": [["7-Zip"], ["KeePassXC"], ["VideoLAN"]],
    "T:\\": [["MWM"], ["Movies"], ["Backups"]],
    "T:\\MWM": [["0.1.2"], ["README.txt", 900]],
  };
  const kids = (p) => FS[p] || (FS[p] = []);
  const par = (p) => { const t = p.replace(/\\$/, ""); const i = t.lastIndexOf("\\"); return i < 0 ? null : i === 2 ? t.slice(0, 3) : t.slice(0, i); };
  const jp = (d, n) => (d.endsWith("\\") ? d + n : d + "\\" + n);
  const nm = (p) => p.replace(/\\$/, "").split("\\").pop();
  const find = (p) => { const d = par(p); return d ? kids(d).find((e) => e[0] === nm(p)) : null; };
  const djobs = [];
  let jid = 1;
  const fakeJob = (kind, title, page, ms, finish, lines = []) => {
    const j = { id: jid++, kind, title, page, done_bytes: 0, total_bytes: 100, done_items: 0, total_items: 0, current: "", state: "running", message: "", output: [], started: now, finished: 0 };
    djobs.push(j);
    const t0 = Date.now();
    const iv = setInterval(() => {
      const f = Math.min(1, (Date.now() - t0) / ms);
      j.done_bytes = Math.round(f * 100);
      if (lines.length) { const k = Math.floor(f * lines.length); while (j.output.length < k) j.output.push(lines[j.output.length]); j.current = lines[Math.max(0, k - 1)] || ""; }
      if (j.cancel) { clearInterval(iv); j.state = "cancelled"; j.message = "cancelled"; }
      else if (f >= 1) { clearInterval(iv); try { j.message = finish(j) || "Done"; j.state = "done"; } catch (e) { j.state = "failed"; j.message = String(e); } }
    }, 150);
    return j.id;
  };
  const demoSvg = '<svg xmlns="http://www.w3.org/2000/svg" width="480" height="300"><rect width="480" height="300" fill="#35c9e6"/><circle cx="360" cy="80" r="40" fill="#ffffff55"/><path d="M0 300 160 140 260 240 340 170 480 300z" fill="#5ee08f"/></svg>';
  Object.assign(handlers, {
    files_roots: () => [
      { path: "C:\\", label: "Acer", kind: "fixed", total: 475.4 * GB, free: 3.5 * GB },
      { path: "T:\\", label: "usb4", kind: "network", total: 11 * 1024 * GB, free: 1.7 * 1024 * GB },
      { path: "C:\\Users\\you", label: "Home", kind: "place", total: 0, free: 0 },
      { path: "C:\\Users\\you\\Downloads", label: "Downloads", kind: "place", total: 0, free: 0 },
    ],
    files_list: ({ dir }) => {
      const d = dir.length === 2 ? dir + "\\" : dir;
      if (!FS[d] && !find(d)) throw `cannot open ${dir}`;
      return { path: d, parent: par(d), free: d.startsWith("T") ? 1.7 * 1024 * GB : 3.5 * GB,
        entries: kids(d).map(([name, size, hidden], i) => ({ name, path: jp(d, name), is_dir: size === undefined, size: size || 0, modified: now - i * 86400 * 3 - 3600, hidden: !!hidden, readonly: false, link: false, ext: size === undefined || !name.includes(".") ? "" : name.split(".").pop().toLowerCase() })) };
    },
    files_mkdir: ({ parent, name }) => { kids(parent).push([name]); FS[jp(parent, name)] = []; return jp(parent, name); },
    files_rename: ({ path, name }) => { const e = find(path); if (e) e[0] = name; return name; },
    files_conflicts: ({ sources, dest }) => sources.map(nm).filter((n) => kids(dest).some((e) => e[0] === n)),
    files_transfer: ({ sources, dest, isMove }) => fakeJob(isMove ? "move" : "copy", `${isMove ? "Move" : "Copy"} ${sources.length} item(s) to ${dest}`, "files", 2500, () => {
      for (const s of sources) {
        const e = find(s);
        if (!e) continue;
        if (!kids(dest).some((x) => x[0] === e[0])) kids(dest).push([...e]);
        if (isMove) FS[par(s)] = kids(par(s)).filter((x) => x !== e);
      }
      return `${isMove ? "Moved" : "Copied"} ${sources.length} items`;
    }),
    files_delete: ({ paths }) => fakeJob("delete", `Recycle ${paths.length} item(s)`, "files", 700, () => { for (const p of paths) FS[par(p)] = kids(par(p)).filter((x) => x[0] !== nm(p)); return `Recycled ${paths.length} items`; }),
    files_pack: ({ archive }) => fakeJob("pack", `Pack ${archive}`, "files", 1500, () => { kids(par(archive)).push([nm(archive), 12 * MB]); return `Created ${archive}`; }),
    files_unpack: ({ dest }) => fakeJob("unpack", `Unpack to ${dest}`, "files", 1500, () => { kids(par(dest)).push([nm(dest)]); FS[dest] = [["photo1.jpg", 3 * MB], ["photo2.jpg", 3 * MB]]; return `Extracted to ${dest}`; }),
    files_search: ({ root, pattern }) => {
      const hits = [];
      const q = (pattern || "").replace(/\*/g, "").toLowerCase();
      for (const [d, list] of Object.entries(FS)) if (d.startsWith(root)) for (const [n] of list) if (n.toLowerCase().includes(q)) hits.push(jp(d, n));
      return fakeJob("search", `Search "${pattern}"`, "files", 1200, () => `${hits.length} found`, hits);
    },
    files_preview: ({ path }) => {
      const e = find(path) || [];
      const x = nm(path).split(".").pop();
      if (["jpg", "png"].includes(x)) return { kind: "image", size: e[1] || 0, truncated: false, content: "data:image/svg+xml;base64," + btoa(demoSvg) };
      if (["exe", "iso", "zip", "pdf", "docx", "xlsx", "sys"].includes(x)) return { kind: "binary", size: e[1] || 0, truncated: true, content: "00000000  4d 5a 90 00 03 00 00 00 04 00 00 00 ff ff 00 00  MZ..............\n00000010  b8 00 00 00 00 00 00 00 40 00 00 00 00 00 00 00  ........@......." };
      return { kind: "text", size: e[1] || 0, truncated: false, content: `# ${nm(path)}\n\nThis is the MWM web demo - in the desktop app F3 shows the real file.\nText, images and a hex view for binaries.` };
    },
    files_props: ({ path }) => { const e = find(path) || [nm(path)]; return { path, is_dir: e[1] === undefined, size: e[1] || 64 * MB, files: 12, dirs: 2, created: now - 86400 * 90, modified: now - 86400 * 3, accessed: now - 600, readonly: false, hidden: !!e[2] }; },
    files_dir_sizes: async ({ paths }) => { await wait(600); return paths.map((p, i) => [p, (i + 1) * 137 * MB]); },
    files_multi_rename: ({ plan }) => { plan.forEach(([p, n]) => { const e = find(p); if (e) e[0] = n; }); return plan.length; },
    files_write: () => null,
    open_default: () => null,
    edit_file: () => null,
    terminal: () => { throw "Terminal opens in the desktop app."; },
    jobs_list: () => djobs.map((j) => ({ ...j, output: j.output.slice() })),
    job_cancel: ({ id }) => { const j = djobs.find((x) => x.id === id); if (j) j.cancel = true; },
    jobs_clear: () => { for (let i = djobs.length - 1; i >= 0; i--) if (djobs[i].state !== "running") djobs.splice(i, 1); },
    toolkit_tasks: () => [
      ["sfc", "Windows repair", "System File Checker", "Finds and repairs corrupted Windows system files (sfc /scannow).", true, "10-30"],
      ["dism", "Windows repair", "Repair Windows image", "DISM RestoreHealth - repairs the component store that SFC repairs from.", true, "15-40"],
      ["dismclean", "Windows repair", "Clean up component store", "Removes superseded update files from WinSxS. Often frees several GB.", true, "5-20"],
      ["chkdsk", "Windows repair", "Check disk (online)", "Scans C: for file-system errors without rebooting.", true, "2-15"],
      ["restore", "Windows repair", "Create restore point", "Snapshot of system settings and drivers.", true, "1-3"],
      ["dns", "Network", "Flush DNS cache", "Fixes sites that won't load after DNS changes.", false, "<1"],
      ["netreset", "Network", "Reset network stack", "Resets Winsock and TCP/IP to defaults.", true, "<1"],
      ["spooler", "Printing", "Fix stuck printing", "Clears the print queue and restarts the Print Spooler.", true, "<1"],
      ["defquick", "Security", "Defender quick scan", "Scans the places malware usually hides.", false, "2-10"],
      ["battery", "Hardware", "Battery health report", "Design vs. full-charge capacity and usage history.", false, "<1"],
    ].map(([id, group, name, description, admin, minutes]) => ({ id, group, name, description, command: "", admin, minutes, warning: "" })),
    toolkit_run: ({ id }) => fakeJob("repair", { sfc: "System File Checker", dism: "Repair Windows image", defquick: "Defender quick scan" }[id] || id, "repair", 6000, () => "finished",
      id === "sfc" ? ["Beginning system scan.  This process will take some time.", "Beginning verification phase of system scan.", "Verification 25% complete.", "Verification 61% complete.", "Verification 100% complete.", "Windows Resource Protection did not find any integrity violations."]
        : ["Starting...", "Working...", "Completed successfully."]),
    toolkit_report: async () => {
      await wait(900);
      return { Manufacturer: "Acer", Model: "Nitro V 15", SystemType: "x64-based PC", Serial: "NHQNDAA00DEMO", Board: "Acer Tanzanite_RTH", BiosVendor: "Insyde Corp.", BiosVersion: "V1.12", BiosDate: "2025-06-11", Cpu: "13th Gen Intel(R) Core(TM) i5-13420H", Cores: 8, Threads: 12, MaxMHz: 2100, RamBytes: 16 * GB, Os: "Microsoft Windows 11 Home", OsVersion: "10.0.26200", Build: "26200", InstallDate: "2025-08-02", LastBoot: "2026-09-20 08:14", Activation: "Activated", LicenseName: "Windows(R) Operating System, OEM_DM channel", PartialKey: "3V66T", OemKey: "DEMO1-XXXXX-XXXXX-XXXXX-3V66T",
        Gpus: [{ Name: "NVIDIA GeForce RTX 5050 Laptop GPU", Driver: "32.0.15.8157" }, { Name: "Intel(R) UHD Graphics", Driver: "32.0.101.7082" }],
        Memory: [{ Slot: "DIMM A", Size: 8 * GB, Speed: 5200, Maker: "Micron", Part: "MTC4C10163S1SC48BA1" }, { Slot: "DIMM B", Size: 8 * GB, Speed: 5200, Maker: "Micron", Part: "MTC4C10163S1SC48BA1" }],
        Disks: [{ Name: "WD PC SN5000S 512GB", Media: "SSD", Bus: "NVMe", Size: 512 * 1000 ** 3, Health: "Healthy", Status: "OK", Temp: 41, Wear: 3, PowerOnHours: 2890, ReadErrors: 0, WriteErrors: 0 }],
        Battery: [{ Name: "AP23A8L", Charge: 87, Status: 2 }] };
    },
    toolkit_security: async () => {
      await wait(700);
      return { Available: true, RealTime: true, Antivirus: true, Tamper: true, SigVersion: "1.435.212.0", SigUpdated: "2026-09-23 06:02", QuickScanAge: 0, FullScanAge: 12, UAC: 1,
        Firewall: [{ Name: "Domain", Enabled: true }, { Name: "Private", Enabled: true }, { Name: "Public", Enabled: true }],
        Threats: [{ When: "2026-07-14 21:03", ThreatID: 2147725, Name: "PUA:Win32/Bundlore", Resources: "file:_C:\\Users\\you\\Downloads\\free-converter-setup.exe", Cleaned: true }], BitLocker: [{ Drive: "C:", Protection: 1 }] };
    },
    toolkit_events: async () => {
      await wait(800);
      return { Crashes: [{ Time: "2026-09-13 02:41", Id: 41, Source: "Microsoft-Windows-Kernel-Power", Message: "The system has rebooted without cleanly shutting down first." }], Dumps: [],
        Errors: [["2026-09-23 09:12", 7031, "Service Control Manager", "The EABackgroundService service terminated unexpectedly."], ["2026-09-22 22:40", 10016, "DistributedCOM", "The application-specific permission settings do not grant Local Activation permission."], ["2026-09-22 18:03", 1000, "Application Error", "Faulting application name: Discord.exe"], ["2026-09-21 07:55", 10010, "DistributedCOM", "The server did not register with DCOM within the required timeout."]]
          .map(([Time, Id, Source, Message]) => ({ Time, Id, Source, Message, Level: "Error", Log: "System" })) };
    },
    toolkit_network: async () => {
      await wait(1200);
      return { Adapters: [{ Name: "Wi-Fi", Desc: "MediaTek Wi-Fi 6 MT7920", IPv4: "192.168.0.42", Gateway: "192.168.0.1", DNS: "192.168.0.85, 1.1.1.1", Speed: "573.5 Mbps", Mac: "AC-12-03-DE-00-00" }],
        Tests: [{ Name: "Router / gateway", Target: "192.168.0.1", Ok: true, Detail: "3ms" }, { Name: "Internet (ping)", Target: "1.1.1.1", Ok: true, Detail: "19ms" }, { Name: "DNS lookup", Target: "www.microsoft.com", Ok: true, Detail: "23.45.1.9" }, { Name: "HTTPS port", Target: "1.1.1.1:443", Ok: true, Detail: "open" }] };
    },
    toolkit_wifi: async () => { await wait(500); return [{ Name: "HomeNet-5G", Auth: "WPA3-Personal", Key: "demo-password-123" }, { Name: "CoffeeShop", Auth: "Open", Key: "" }]; },
  });

  Object.assign(handlers, {
    keys_list: async () => { await wait(600); return [
      { kind: "windows", name: "Windows product key (firmware)", value: "DEMO7-XXXXX-XXXXX-XXXXX-3V66T", source: "BIOS / UEFI (OA3)", note: "Windows 11 Home OEM:DM", secret: true },
      { kind: "windows", name: "Installed key (Windows 11 Home)", value: "YTMG3-N6DKC-DKB77-7M9GH-8HVX7", source: "Registry (DigitalProductId)", note: "Generic key - this PC is activated by a digital license tied to its hardware / Microsoft account.", secret: true },
      { kind: "license", name: "Windows(R), Core edition", value: "*****-*****-*****-*****-3V66T", source: "Windows(R) Operating System, OEM_DM channel", note: "Licensed. Only the last 5 characters are stored for this product.", secret: false },
      { kind: "office", name: "Office 16, Office16O365HomePremR_Subscription1 edition", value: "*****-*****-*****-*****-Q8TXY", source: "Office 16, TIMEBASED_SUB channel", note: "Licensed. Only the last 5 characters are stored for this product.", secret: false },
      { kind: "bitlocker", name: "BitLocker recovery key C:", value: "123456-654321-111111-222222-333333-444444-555555-666666", source: "Protector {7A1C...}", note: "Keep this somewhere safe - it unlocks the drive if Windows can't.", secret: true },
      { kind: "wifi", name: "HomeNet-5G", value: "demo-password-123", source: "Wi-Fi (WPA3-Personal)", note: "", secret: true },
      { kind: "wifi", name: "CoffeeShop", value: "", source: "Wi-Fi (Open)", note: "Open network, or run as administrator to read the key.", secret: false },
    ]; },
    server_info: async () => { await wait(900); return {
      platform: "proxmox", kernel: "6.8.12-43-pve", kernels: ["proxmox-kernel-6.8.12-13-pve", "proxmox-kernel-6.8.12-43-pve"], load: "1.19 1.67 1.93", failed: [],
      proxmox: { version: "pve-manager/8.4.19 (running kernel: 6.8.12-43-pve)", subscription: "NotFound",
        guests: [[100, "lxc", "sonarr", "running", 0.01, 412, 6144], [101, "lxc", "jellyfin", "running", 0.12, 814, 32768], [104, "lxc", "romm", "running", 0.03, 2860, 16384], [107, "lxc", "traefik", "running", 0.0, 96, 1024], [126, "lxc", "cleanuparr", "stopped", 0, 0, 1024], [200, "qemu", "windows-11", "running", 0.21, 7900, 8192], [201, "qemu", "home-assistant", "running", 0.02, 1500, 4096]]
          .map(([vmid, type, name, status, cpu, mem, max]) => ({ vmid, type, name, status, cpu, mem: mem * MB, maxmem: max * MB, uptime: status === "running" ? 86400 * 3.9 : 0, node: "pve1" })),
        storage: [{ name: "local", type: "dir", status: "active", total: 110 * GB, used: 78 * GB }, { name: "local-lvm", type: "lvmthin", status: "active", total: 4200 * GB, used: 2000 * GB }, { name: "tank", type: "zfspool", status: "active", total: 21800 * GB, used: 14300 * GB }] },
      unraid: null,
      zfs: [{ name: "tank", size: 21800 * GB, alloc: 14300 * GB, free: 7500 * GB, frag: "7", cap: "65", health: "ONLINE", scan: "scrub repaired 0B in 05:12:44 with 0 errors on Sun Sep 14 05:36:45 2026", errors: "No known data errors" }],
      smart: [{ device: "/dev/nvme0", model: "Samsung SSD 980 PRO 1TB", capacity: 1000 * 1000 ** 3, passed: true, temp: 38, hours: 14210, wear: 4, media_errors: 0 },
        { device: "/dev/sda", model: "WDC WD120EDAZ", capacity: 12 * 1000 ** 4, passed: true, temp: 36, hours: 31022, reallocated: 0, pending: 0, rotation: 5400 },
        { device: "/dev/sdb", model: "ST8000DM004", capacity: 8 * 1000 ** 4, passed: true, temp: 44, hours: 42980, reallocated: 8, pending: 0, rotation: 5425 }],
      docker: { containers: [{ name: "portainer", image: "portainer/portainer-ce", state: "running", status: "Up 3 days" }, { name: "watchtower", image: "containrrr/watchtower", state: "exited", status: "Exited (0) 2 days ago" }], df: [{ type: "Images", size: "4.1GB", reclaimable: "1.2GB (29%)" }, { type: "Build Cache", size: "650MB", reclaimable: "650MB" }] },
    }; },
    server_action: async ({ action, target }) => `${action} ${target}: done (demo)`,
  });

  const demoConns = [{ id: "d1", name: "pve1 (Proxmox)", url: "http://192.168.0.6:7777", has_token: true }, { id: "d2", name: "unraid", url: "http://192.168.0.20:7777", has_token: true }];
  Object.assign(handlers, {
    conn_list: () => demoConns,
    conn_save: ({ name, url }) => { const c = { id: "d" + Date.now(), name, url, has_token: true }; demoConns.push(c); return c; },
    conn_remove: ({ id }) => { const i = demoConns.findIndex((c) => c.id === id); if (i >= 0) demoConns.splice(i, 1); },
    remote_call: async ({ id, cmd, args }) => {
      if (id === "d2") throw "can't reach unraid (http://192.168.0.20:7777): ConnectionFailed";
      const r = await handlers[cmd](args || {});
      return cmd === "system_info" ? { ...r, hostname: "pve1", os: "Linux (Debian GNU/Linux 12)", platform: "linux", cores: 16, memory_total: 251 * GB, memory_used: 38 * GB } : r;
    },
  });
  window.MWM_DEMO = async (cmd, args) => {
    const h = handlers[cmd];
    if (!h) throw `demo: ${cmd} not available`;
    return h(args);
  };
})();
