//! Junk / cache / privacy cleaning (CCleaner "Custom Clean", Revo "Junk Files",
//! "Browsers Cleaner" and "Windows Cleaner").

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::fsutil::{self, Filter, Tally};

#[derive(Debug, Clone)]
pub enum Target {
    /// Everything below each directory matching the pattern.
    Contents(String),
    /// Files matching globs inside each directory matching the pattern.
    Files { dir: String, globs: Vec<&'static str>, recursive: bool },
    /// The OS recycle bin / trash.
    RecycleBin,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub id: String,
    pub group: &'static str,
    pub category: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub targets: Vec<Target>,
    pub min_age_hours: u64,
    pub admin: bool,
    pub default_on: bool,
    pub warning: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanItem {
    pub id: String,
    pub group: String,
    pub category: String,
    pub name: String,
    pub description: String,
    pub bytes: u64,
    pub files: u64,
    pub locations: usize,
    pub admin: bool,
    pub default_on: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanResult {
    pub id: String,
    pub name: String,
    pub bytes_freed: u64,
    pub files_deleted: u64,
    pub skipped: u64,
}

impl Rule {
    fn filter(&self, globs: &[&'static str], recursive: bool) -> Filter {
        Filter {
            min_age: (self.min_age_hours > 0).then(|| Duration::from_secs(self.min_age_hours * 3600)),
            patterns: globs.iter().map(|g| g.to_string()).collect(),
            shallow: !recursive,
        }
    }

    fn resolved(&self) -> Vec<(PathBuf, Filter)> {
        let mut out = Vec::new();
        for t in &self.targets {
            match t {
                Target::Contents(p) => {
                    for d in fsutil::expand(Path::new(p)) {
                        out.push((d, self.filter(&[], true)));
                    }
                }
                Target::Files { dir, globs, recursive } => {
                    for d in fsutil::expand(Path::new(dir)) {
                        out.push((d, self.filter(globs, *recursive)));
                    }
                }
                Target::RecycleBin => {}
            }
        }
        out
    }

    fn has_recycle_bin(&self) -> bool {
        self.targets.iter().any(|t| matches!(t, Target::RecycleBin))
    }

    pub fn scan(&self) -> ScanItem {
        let mut t = Tally::default();
        let places = self.resolved();
        for (dir, f) in &places {
            t.add(fsutil::measure(dir, f));
        }
        if self.has_recycle_bin() {
            t.add(recycle_bin::query());
        }
        ScanItem {
            id: self.id.clone(),
            group: self.group.into(),
            category: self.category.into(),
            name: self.name.into(),
            description: self.description.into(),
            bytes: t.bytes,
            files: t.files,
            locations: places.len() + usize::from(self.has_recycle_bin()),
            admin: self.admin,
            default_on: self.default_on,
            warning: self.warning.map(Into::into),
        }
    }

    pub fn clean(&self) -> CleanResult {
        let mut freed = Tally::default();
        let mut skipped = 0;
        for (dir, f) in self.resolved() {
            let (t, e) = fsutil::delete_contents(&dir, &f);
            freed.add(t);
            skipped += e;
        }
        if self.has_recycle_bin() {
            let before = recycle_bin::query();
            if recycle_bin::empty() {
                freed.add(before);
            } else {
                skipped += 1;
            }
        }
        CleanResult {
            id: self.id.clone(),
            name: self.name.into(),
            bytes_freed: freed.bytes,
            files_deleted: freed.files,
            skipped,
        }
    }
}

/// Scan every rule in parallel (each rule is IO bound on different folders).
pub fn scan_all() -> Vec<ScanItem> {
    let rules = rules();
    let mut items: Vec<ScanItem> = std::thread::scope(|s| {
        let handles: Vec<_> = rules.iter().map(|r| s.spawn(move || r.scan())).collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    items.sort_by_key(|i| rules.iter().position(|r| r.id == i.id));
    items
}

pub fn clean(ids: &[String]) -> Vec<CleanResult> {
    let rules = rules();
    let chosen: Vec<&Rule> = rules.iter().filter(|r| ids.iter().any(|i| *i == r.id)).collect();
    std::thread::scope(|s| {
        let handles: Vec<_> = chosen.iter().map(|r| s.spawn(move || r.clean())).collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    })
}

pub fn default_ids() -> Vec<String> {
    rules().into_iter().filter(|r| r.default_on).map(|r| r.id).collect()
}

// ---------------------------------------------------------------- rules ----

#[allow(clippy::too_many_arguments)]
fn rule(
    id: impl Into<String>,
    group: &'static str,
    category: &'static str,
    name: &'static str,
    description: &'static str,
    targets: Vec<Target>,
    min_age_hours: u64,
    admin: bool,
    default_on: bool,
) -> Rule {
    Rule { id: id.into(), group, category, name, description, targets, min_age_hours, admin, default_on, warning: None }
}

fn c(p: String) -> Target {
    Target::Contents(p)
}

fn files(dir: String, globs: &[&'static str], recursive: bool) -> Target {
    Target::Files { dir, globs: globs.to_vec(), recursive }
}

#[cfg(windows)]
pub fn rules() -> Vec<Rule> {
    use crate::util::env_path;
    let s = |v: &str, fallback: &str| {
        env_path(v).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| fallback.to_string())
    };
    let local = s("LOCALAPPDATA", r"C:\Users\Default\AppData\Local");
    let roaming = s("APPDATA", r"C:\Users\Default\AppData\Roaming");
    let temp = s("TEMP", &format!(r"{local}\Temp"));
    let win = s("WINDIR", r"C:\Windows");
    let pdata = s("PROGRAMDATA", r"C:\ProgramData");
    let profile = s("USERPROFILE", r"C:\Users\Default");

    let mut r = vec![
        // Windows
        rule("win.temp", "junk", "Windows", "Temporary files (user)", "Files apps left in your %TEMP% folder more than a day ago.",
            vec![c(temp.clone())], 24, false, true),
        rule("win.systemp", "junk", "Windows", "Temporary files (system)", "C:\\Windows\\Temp older than a day.",
            vec![c(format!(r"{win}\Temp"))], 24, true, true),
        rule("win.recycle", "junk", "Windows", "Recycle Bin", "Everything currently in the Recycle Bin on every drive.",
            vec![Target::RecycleBin], 0, false, true),
        rule("win.thumbs", "junk", "Windows", "Thumbnail cache", "Explorer's picture previews. Rebuilt automatically.",
            vec![files(format!(r"{local}\Microsoft\Windows\Explorer"), &["thumbcache_*.db", "iconcache_*.db"], false)], 0, false, true),
        rule("win.inet", "junk", "Windows", "Internet cache (WinINet)", "Cache used by Windows components and legacy apps.",
            vec![c(format!(r"{local}\Microsoft\Windows\INetCache"))], 0, false, true),
        rule("win.dx", "junk", "System", "DirectX shader cache", "Compiled GPU shaders. Games rebuild them on next launch.",
            vec![c(format!(r"{local}\D3DSCache"))], 0, false, true),
        rule("win.dumps", "junk", "System", "Crash dumps", "Memory dumps left behind by crashed programs.",
            vec![c(format!(r"{local}\CrashDumps")), c(format!(r"{win}\Minidump")), files(win.clone(), &["MEMORY.DMP"], false)], 0, false, true),
        rule("win.wer", "junk", "System", "Error reports", "Windows Error Reporting archives and queues.",
            vec![c(format!(r"{local}\Microsoft\Windows\WER")), c(format!(r"{pdata}\Microsoft\Windows\WER\ReportArchive")), c(format!(r"{pdata}\Microsoft\Windows\WER\ReportQueue"))], 0, false, true),
        rule("win.logs", "junk", "System", "Windows log files", "Setup, CBS and DISM logs.",
            vec![files(format!(r"{win}\Logs"), &["*.log", "*.cab", "*.etl"], true), files(format!(r"{win}\Panther"), &["*.log"], false)], 72, true, false),
        rule("win.update", "junk", "Advanced", "Windows Update downloads", "Installer files for updates that are already installed.",
            vec![c(format!(r"{win}\SoftwareDistribution\Download"))], 72, true, false),
        rule("win.delivery", "junk", "Advanced", "Delivery Optimization cache", "Update pieces Windows shares with other PCs.",
            vec![c(format!(r"{win}\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache"))], 0, true, false),
        rule("win.prefetch", "junk", "Advanced", "Prefetch data", "Boot/launch hints. Windows rebuilds them; first launches are slower.",
            vec![files(format!(r"{win}\Prefetch"), &["*.pf"], false)], 0, true, false),
        rule("win.recent", "privacy", "Windows", "Recent documents list", "Shortcuts in the Recent Items / Quick Access history.",
            vec![files(format!(r"{roaming}\Microsoft\Windows\Recent"), &["*.lnk"], false)], 0, false, false),
        rule("win.store", "junk", "Windows Store", "Store app caches", "Web caches and temp state of Microsoft Store apps.",
            vec![c(format!(r"{local}\Packages\*\AC\INetCache")), c(format!(r"{local}\Packages\*\AC\Temp")), c(format!(r"{local}\Packages\*\TempState"))], 0, false, true),
        rule("win.downloads", "junk", "Windows Downloads", "Downloads older than 30 days", "Files in your Downloads folder untouched for 30 days. Review before cleaning!",
            vec![c(format!(r"{profile}\Downloads"))], 24 * 30, false, false),
        // Applications
        rule("app.nvidia", "junk", "Applications", "NVIDIA shader cache", "DirectX/OpenGL shader caches kept by the NVIDIA driver.",
            vec![c(format!(r"{local}\NVIDIA\DXCache")), c(format!(r"{local}\NVIDIA\GLCache")), c(format!(r"{pdata}\NVIDIA Corporation\NV_Cache"))], 0, false, true),
        rule("app.discord", "junk", "Applications", "Discord cache", "Images and scripts Discord re-downloads.",
            vec![c(format!(r"{roaming}\discord\Cache")), c(format!(r"{roaming}\discord\Code Cache")), c(format!(r"{roaming}\discord\GPUCache"))], 0, false, true),
        rule("app.vscode", "junk", "Applications", "VS Code cache", "Editor caches, cached extension installers and old logs.",
            vec![c(format!(r"{roaming}\Code\Cache")), c(format!(r"{roaming}\Code\CachedData")), c(format!(r"{roaming}\Code\CachedExtensionVSIXs")), c(format!(r"{roaming}\Code\Code Cache")), c(format!(r"{roaming}\Code\GPUCache")), c(format!(r"{roaming}\Code\logs"))], 0, false, true),
        rule("app.steam", "junk", "Applications", "Steam web cache", "The Steam client's embedded browser cache.",
            vec![c(format!(r"{local}\Steam\htmlcache"))], 0, false, true),
        rule("app.spotify", "junk", "Applications", "Spotify cache", "Offline stream cache (not your downloads).",
            vec![c(format!(r"{local}\Spotify\Data")), c(format!(r"{local}\Spotify\Browser\Cache"))], 0, false, false),
        rule("app.office", "junk", "Applications", "Office file cache", "Microsoft Office document upload cache.",
            vec![c(format!(r"{local}\Microsoft\Office\16.0\OfficeFileCache"))], 0, false, false),
        // Developer caches: big, always safe to re-download, off by default.
        rule("dev.npm", "junk", "Developer", "npm cache", "Downloaded npm tarballs (npm cache).",
            vec![c(format!(r"{local}\npm-cache\_cacache"))], 0, false, false),
        rule("dev.pip", "junk", "Developer", "pip cache", "Downloaded Python wheels.",
            vec![c(format!(r"{local}\pip\cache"))], 0, false, false),
        rule("dev.yarn", "junk", "Developer", "Yarn cache", "Yarn's offline package cache.",
            vec![c(format!(r"{local}\Yarn\Cache"))], 0, false, false),
        rule("dev.nuget", "junk", "Developer", "NuGet HTTP cache", "Cached NuGet feed responses.",
            vec![c(format!(r"{local}\NuGet\v3-cache")), c(format!(r"{local}\NuGet\plugins-cache"))], 0, false, false),
        rule("dev.go", "junk", "Developer", "Go build cache", "Compiled Go packages (go clean -cache).",
            vec![c(format!(r"{local}\go-build"))], 0, false, false),
        rule("dev.crash", "junk", "Developer", "Electron crash reports", "Crashpad dumps from Electron apps.",
            vec![c(format!(r"{roaming}\*\Crashpad\reports")), c(format!(r"{local}\*\Crashpad\reports"))], 0, false, true),
    ];

    // Browsers: every Chromium profile gets the same treatment.
    let chromium: [(&'static str, &'static str, String); 5] = [
        ("chrome", "Google Chrome", format!(r"{local}\Google\Chrome\User Data")),
        ("edge", "Microsoft Edge", format!(r"{local}\Microsoft\Edge\User Data")),
        ("brave", "Brave", format!(r"{local}\BraveSoftware\Brave-Browser\User Data")),
        ("vivaldi", "Vivaldi", format!(r"{local}\Vivaldi\User Data")),
        ("opera", "Opera", format!(r"{roaming}\Opera Software\Opera Stable")),
    ];
    for (key, label, root) in chromium {
        r.extend(chromium_rules(key, label, &root));
    }
    let ff_local = format!(r"{local}\Mozilla\Firefox\Profiles");
    let ff_roam = format!(r"{roaming}\Mozilla\Firefox\Profiles");
    r.extend(firefox_rules(&ff_local, &ff_roam));
    r
}

/// Rules for one Chromium-family browser rooted at its "User Data" dir.
fn chromium_rules(key: &str, label: &'static str, root: &str) -> Vec<Rule> {
    let sep = std::path::MAIN_SEPARATOR;
    let p = |tail: &str| format!("{root}{sep}{}", tail.replace('/', &sep.to_string()));
    let cache = rule(
        format!("br.{key}.cache"), "browser", label, "Internet cache",
        "Cached pages, images and compiled scripts for every profile.",
        vec![
            c(p("*/Cache")), c(p("*/Code Cache")), c(p("*/GPUCache")), c(p("*/DawnCache")), c(p("*/DawnGraphiteCache")),
            c(p("ShaderCache")), c(p("GrShaderCache")), c(p("GraphiteDawnCache")),
            c(p("Cache")), c(p("Code Cache")), c(p("GPUCache")),
        ],
        0, false, true,
    );
    let mut history = rule(
        format!("br.{key}.history"), "browser", label, "History",
        "Browsing and download history. Close the browser first.",
        vec![files(p("*"), &["History", "History-journal", "Visited Links", "Top Sites", "Top Sites-journal"], false)],
        0, false, false,
    );
    history.warning = Some("Close the browser first - open files are skipped.");
    let mut cookies = rule(
        format!("br.{key}.cookies"), "browser", label, "Cookies",
        "Signs you out of every website in this browser.",
        vec![files(p("*"), &["Cookies", "Cookies-journal"], false), files(p("*/Network"), &["Cookies", "Cookies-journal"], false)],
        0, false, false,
    );
    cookies.warning = Some("You will be signed out of websites.");
    vec![cache, history, cookies]
}

fn firefox_rules(local_profiles: &str, roaming_profiles: &str) -> Vec<Rule> {
    let sep = std::path::MAIN_SEPARATOR;
    let mut cookies = rule(
        "br.firefox.cookies", "browser", "Firefox", "Cookies", "Signs you out of every website in Firefox.",
        vec![files(format!("{roaming_profiles}{sep}*"), &["cookies.sqlite", "cookies.sqlite-wal"], false)],
        0, false, false,
    );
    cookies.warning = Some("You will be signed out of websites.");
    vec![
        rule("br.firefox.cache", "browser", "Firefox", "Internet cache", "Firefox disk cache and startup cache.",
            vec![c(format!("{local_profiles}{sep}*{sep}cache2")), c(format!("{local_profiles}{sep}*{sep}startupCache")), c(format!("{local_profiles}{sep}*{sep}jumpListCache"))],
            0, false, true),
        cookies,
    ]
}

#[cfg(target_os = "linux")]
pub fn rules() -> Vec<Rule> {
    let home = crate::util::home().to_string_lossy().to_string();
    let mut r = vec![
        rule("lx.tmp", "junk", "System", "/tmp (older than 10 days)", "Temporary files in /tmp untouched for 10 days (systemd default).",
            vec![c("/tmp".into())], 24 * 10, true, true),
        rule("lx.vartmp", "junk", "System", "/var/tmp (older than a week)", "Persistent temp files untouched for 7 days.",
            vec![c("/var/tmp".into())], 24 * 7, true, true),
        rule("lx.apt", "junk", "Packages", "APT package cache", "Downloaded .deb files (apt-get clean).",
            vec![files("/var/cache/apt/archives".into(), &["*.deb"], true)], 0, true, true),
        rule("lx.dnf", "junk", "Packages", "DNF/YUM package cache", "Downloaded .rpm files.",
            vec![files("/var/cache/dnf".into(), &["*.rpm"], true), files("/var/cache/yum".into(), &["*.rpm"], true)], 0, true, true),
        rule("lx.journal", "junk", "Logs", "Archived systemd journals", "Rotated journal files (the live journal is kept).",
            vec![files("/var/log/journal".into(), &["*@*.journal", "*.journal~"], true)], 0, true, true),
        rule("lx.oldlogs", "junk", "Logs", "Rotated log files", "Compressed and numbered logs in /var/log (*.gz, *.1, *.old).",
            vec![files("/var/log".into(), &["*.gz", "*.1", "*.2", "*.3", "*.4", "*.old", "*.xz"], true)], 0, true, true),
        rule("lx.crash", "junk", "System", "Crash reports", "Reports in /var/crash and core dumps.",
            vec![c("/var/crash".into()), c("/var/lib/systemd/coredump".into())], 0, true, true),
        rule("lx.trash", "junk", "User", "Trash", "Files in your desktop trash.",
            vec![c(format!("{home}/.local/share/Trash/files")), c(format!("{home}/.local/share/Trash/info"))], 0, false, true),
        rule("lx.thumbs", "junk", "User", "Thumbnail cache", "Image previews made by file managers.",
            vec![c(format!("{home}/.cache/thumbnails"))], 0, false, true),
        rule("lx.pve", "junk", "Proxmox", "Proxmox task logs (older than 30 days)", "Old task log files in /var/log/pve/tasks.",
            vec![c("/var/log/pve/tasks".into())], 24 * 30, true, false),
        rule("dev.pip", "junk", "Developer", "pip cache", "Downloaded Python wheels.", vec![c(format!("{home}/.cache/pip"))], 0, false, false),
        rule("dev.npm", "junk", "Developer", "npm cache", "Downloaded npm tarballs.", vec![c(format!("{home}/.npm/_cacache"))], 0, false, false),
        rule("dev.go", "junk", "Developer", "Go build cache", "Compiled Go packages.", vec![c(format!("{home}/.cache/go-build"))], 0, false, false),
    ];
    r.extend(chromium_rules("chrome", "Google Chrome", &format!("{home}/.config/google-chrome")));
    r.extend(chromium_rules("chromium", "Chromium", &format!("{home}/.config/chromium")));
    r.extend(chromium_rules("brave", "Brave", &format!("{home}/.config/BraveSoftware/Brave-Browser")));
    r.extend(firefox_rules(&format!("{home}/.cache/mozilla/firefox"), &format!("{home}/.mozilla/firefox")));
    r
}

#[cfg(target_os = "macos")]
pub fn rules() -> Vec<Rule> {
    let home = crate::util::home().to_string_lossy().to_string();
    let mut r = vec![
        rule("mac.caches", "junk", "System", "User caches", "~/Library/Caches (apps rebuild these).",
            vec![c(format!("{home}/Library/Caches"))], 24, false, true),
        rule("mac.logs", "junk", "System", "User logs", "~/Library/Logs.", vec![c(format!("{home}/Library/Logs"))], 24, false, true),
        rule("mac.trash", "junk", "System", "Trash", "Files in the Trash.", vec![c(format!("{home}/.Trash"))], 0, false, true),
        rule("mac.xcode", "junk", "Developer", "Xcode DerivedData", "Build intermediates.",
            vec![c(format!("{home}/Library/Developer/Xcode/DerivedData"))], 0, false, false),
    ];
    let support = format!("{home}/Library/Application Support");
    r.extend(chromium_rules("chrome", "Google Chrome", &format!("{support}/Google/Chrome")));
    r.extend(chromium_rules("brave", "Brave", &format!("{support}/BraveSoftware/Brave-Browser")));
    r.extend(chromium_rules("edge", "Microsoft Edge", &format!("{support}/Microsoft Edge")));
    r.extend(firefox_rules(&format!("{home}/Library/Caches/Firefox/Profiles"), &format!("{support}/Firefox/Profiles")));
    r
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn rules() -> Vec<Rule> {
    Vec::new()
}

// ---------------------------------------------------------- recycle bin ----

#[cfg(windows)]
mod recycle_bin {
    use crate::fsutil::Tally;
    use windows_sys::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHQueryRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
        SHQUERYRBINFO,
    };

    pub fn query() -> Tally {
        let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, i64Size: 0, i64NumItems: 0 };
        let hr = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
        if hr < 0 {
            return Tally::default();
        }
        Tally { bytes: info.i64Size.max(0) as u64, files: info.i64NumItems.max(0) as u64 }
    }

    pub fn empty() -> bool {
        if query().files == 0 {
            return true;
        }
        let hr = unsafe {
            SHEmptyRecycleBinW(
                std::ptr::null_mut(),
                std::ptr::null(),
                SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
            )
        };
        hr >= 0
    }
}

#[cfg(not(windows))]
mod recycle_bin {
    use crate::fsutil::Tally;
    pub fn query() -> Tally {
        Tally::default()
    }
    pub fn empty() -> bool {
        true
    }
}
